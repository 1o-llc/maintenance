//! The workspace checks: what a proc macro cannot see, because it sees one item at a time.
//!
//! [`Workspace::scan`] reads a workspace once: klepto's item graph (every item, every reference, across crates,
//! `src/` and `tests/`), and every mark in the source, read by [`crate::attrs`] (the readers the macros use). Each
//! check is then a method returning a [`Report`]; the free functions ([`sensitivity`], [`mechanism`], [`boundary`],
//! [`supersedes`], [`reads_env`], [`golden`]) scan and run one, and [`all`] scans once and runs them all.
//! [`registry`] / [`write_registry`] make `.maintenance/registry.json` from the same scan.
//!
//! There is no command line here: a tool calls these. Every report is data with one shape ([`Report`]).
//!
//! What klepto cannot resolve is not judged: a method call whose receiver it cannot type is counted in the
//! report (`unresolved_references`), never guessed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use klepto::{EdgeKind, Klepto, NodeKind};
use liaise_proc_macro::SynBuffer;
use serde::{Deserialize, Serialize};
use syn::spanned::Spanned;

use crate::attrs::{self, Marks};
use crate::codes::{render_ctx, Code, LiaiseCode};
use crate::config::{self, DeclaredIn, MaintenanceToml};
use crate::levels::LevelTable;
use crate::registry::{self, Deprecation, Entry, Registry, Review};

/// The format every [`Report`] is written in.
pub const REPORT_FORMAT: u32 = 1;

/// What one check found.
///
/// ```json
/// {
///   "format": 1,
///   "check": "sensitivity",
///   "workspace": "examples/workspace",
///   "passed": false,
///   "findings": [ { "code": "MAINT6001", "message": "[MAINT6001] a call from ...: ...", "item": "api::status",
///                   "target": "vault::open_vault", "file": "crates/api/src/lib.rs", "line": 14 } ],
///   "notes": [ ... ],
///   "counts": { "references_judged": 9, "unresolved_references": 4 }
/// }
/// ```
///
/// `findings` fail the check (`passed` is false when there is one); `notes` are reported and do not (a reach
/// maintenance.toml allows, what still uses a superseded item). `item` is the item the finding is about, `target`
/// the other item when there is one; `file` is relative to the workspace root. `counts` are named numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub format: u32,
    pub check: String,
    pub workspace: String,
    pub passed: bool,
    pub findings: Vec<Finding>,
    pub notes: Vec<Finding>,
    pub counts: BTreeMap<String, u64>,
}

/// One line of a report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    /// `MAINT6001`.
    pub code: String,
    /// `[MAINT6001] message: what, where`.
    pub message: String,
    pub item: Option<String>,
    pub target: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
}

impl Finding {
    fn new(code: Code, ctx: impl core::fmt::Display) -> Self {
        Self { code: code.code(), message: render_ctx(code, ctx), item: None, target: None, file: None, line: None }
    }

    fn about(mut self, item: &str) -> Self {
        self.item = Some(item.to_owned());
        self
    }

    fn target(mut self, target: &str) -> Self {
        self.target = Some(target.to_owned());
        self
    }

    fn at(mut self, place: Option<(String, u32)>) -> Self {
        if let Some((file, line)) = place {
            self.file = Some(file);
            self.line = Some(line);
        }
        self
    }
}

impl Report {
    fn new(check: &str, ws: &Workspace) -> Self {
        Self {
            format: REPORT_FORMAT,
            check: check.to_owned(),
            workspace: ws.shown_root.clone(),
            passed: true,
            findings: Vec::new(),
            notes: Vec::new(),
            counts: BTreeMap::new(),
        }
    }

    fn finish(mut self) -> Self {
        self.passed = self.findings.is_empty();
        self
    }

    /// The report as text: a heading, the findings, the notes, the counts.
    pub fn render(&self) -> String {
        let verdict = if self.passed { "passed" } else { "failed" };
        let mut out = format!("{} check, {}: {verdict}, {} findings, {} notes\n", self.check, self.workspace, self.findings.len(), self.notes.len());
        let place = |f: &Finding| match (&f.file, f.line) {
            (Some(file), Some(line)) => format!(" ({file}:{line})"),
            _ => String::new(),
        };
        for f in &self.findings {
            out.push_str(&format!("  {}{}\n", f.message, place(f)));
        }
        for f in &self.notes {
            out.push_str(&format!("  note {}{}\n", f.message, place(f)));
        }
        if !self.counts.is_empty() {
            let counts: Vec<String> = self.counts.iter().map(|(k, v)| format!("{k} {v}")).collect();
            out.push_str(&format!("  counts: {}\n", counts.join(", ")));
        }
        out
    }
}

/// Why a workspace could not be read at all.
#[derive(Debug)]
pub struct CheckError {
    pub code: Code,
    pub detail: String,
}

impl core::fmt::Display for CheckError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&render_ctx(self.code, &self.detail))
    }
}

impl std::error::Error for CheckError {}

/// One marked item, found in the source.
#[derive(Debug, Clone)]
pub struct Site {
    /// klepto's canonical id: `vault::open_vault`, `vault::Vault::new`, `vault::<impl Vault>`, `vault::store`.
    pub id: String,
    /// The package whose folder holds the file.
    pub package: String,
    pub kind: String,
    /// Relative to the workspace root.
    pub file: String,
    pub line: u32,
    pub marks: Marks,
}

/// A workspace, read once.
pub struct Workspace {
    root: PathBuf,
    shown_root: String,
    config: MaintenanceToml,
    table: LevelTable,
    sites: Vec<Site>,
    problems: Vec<Finding>,
    k: Klepto,
    /// Manifest folders, longest first, and their package.
    packages: Vec<(PathBuf, String)>,
    /// Where the marks themselves are written: a path inside one (`#[supersedes(crate::old)]`) is not a use.
    mark_lines: BTreeSet<(PathBuf, u32)>,
}

/// The name klepto files a workspace's `tests/` files under: their module keys start with it.
const TESTS_CRATE: &str = "workspace_tests";

impl Workspace {
    /// Read the workspace at `root`: klepto's graph of `src/` and `tests/`, every mark, the level table, and
    /// `maintenance.toml` beside the root's `Cargo.toml`.
    pub fn scan(root: &Path) -> Result<Self, CheckError> {
        let shown_root = root.display().to_string();
        let root = root.canonicalize().map_err(|e| CheckError { code: Code::WorkspaceUnread, detail: format!("{shown_root}: {e}") })?;
        let unread = |e: klepto::KleptoError| CheckError { code: Code::WorkspaceUnread, detail: format!("{shown_root}: {e}") };
        let k = Klepto::new(TESTS_CRATE)
            .include_tests(true)
            .scan_workspace_root(&root)
            .map_err(unread)?
            .exclude_generated()
            .map_err(unread)?
            .ignore_parse_errors(true)
            .parse()
            .map_err(unread)?;

        let mut problems = Vec::new();
        let config_file = root.join(config::FILE);
        let config = if config_file.is_file() {
            config::load(&config_file).unwrap_or_else(|e| {
                problems.push(Finding::new(Code::CheckConfig, e));
                MaintenanceToml::default()
            })
        } else {
            MaintenanceToml::default()
        };

        let mut packages: Vec<(PathBuf, String)> = k
            .modules
            .modules
            .values()
            .filter(|m| m.segments.is_empty() && !m.orphan && m.package != TESTS_CRATE)
            .filter_map(|m| {
                let dir = m.file.parent()?;
                let dir = if dir.ends_with("src") { dir.parent()? } else { dir };
                Some((dir.to_owned(), m.package.clone()))
            })
            .collect();
        packages.sort_by_key(|(d, _)| std::cmp::Reverse(d.as_os_str().len()));
        packages.dedup();

        let mut ws = Self { root, shown_root, config, table: LevelTable::shipped(), sites: Vec::new(), problems, k, packages, mark_lines: BTreeSet::new() };
        ws.read_sites();
        Ok(ws)
    }

    /// Every marked item found.
    pub fn sites(&self) -> &[Site] {
        &self.sites
    }

    /// The level table in force: the workspace's one `sensitivity_levels!`, or the shipped levels.
    pub fn levels(&self) -> &LevelTable {
        &self.table
    }

    /// What the scan itself found wrong: a mark that does not read, two level tables, a maintenance.toml that
    /// does not read.
    pub fn problems(&self) -> &[Finding] {
        &self.problems
    }

    fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.root).unwrap_or(path).display().to_string()
    }

    /// The package whose manifest folder holds `path`.
    fn package_of_file(&self, path: &Path) -> Option<&str> {
        self.packages.iter().find(|(dir, _)| path.starts_with(dir)).map(|(_, p)| p.as_str())
    }

    /// The package of a graph node, by its file; a node with no file (an external) has none.
    fn package_of_node(&self, id: &str) -> Option<&str> {
        let node = self.k.graph.node(id)?;
        let loc = node.location.as_ref()?;
        self.package_of_file(&loc.path).or(node.package.as_deref())
    }

    fn place_of_node(&self, id: &str) -> Option<(String, u32)> {
        let loc = self.k.graph.node(id)?.location.as_ref()?;
        Some((self.relative(&loc.path), loc.line.unwrap_or(0)))
    }

    fn read_sites(&mut self) {
        let mut decls: Vec<(LevelTable, String)> = Vec::new();
        let mut sites = Vec::new();
        let mut problems = Vec::new();
        let mut mark_lines = BTreeSet::new();
        for file in &self.k.files {
            let Some(module) = file.module.clone() else { continue };
            let mut walk = Walk {
                ws: self,
                file: &file.path,
                rel: self.relative(&file.path),
                package: self.package_of_file(&file.path).unwrap_or(module.split("::").next().unwrap_or("")).to_owned(),
                sites: &mut sites,
                problems: &mut problems,
                decls: &mut decls,
                mark_lines: &mut mark_lines,
            };
            walk.items(&file.ast.items, &module);
        }
        // one level table per workspace
        let mut tables = decls.iter();
        if let Some((first, place)) = tables.next() {
            self.table = first.clone();
            for (other, other_place) in tables {
                if other != first {
                    problems.push(Finding::new(Code::TwoLevelTables, format!("{} at {place} and {} at {other_place}", first.word, other.word)));
                }
            }
        }
        sites.sort_by(|a: &Site, b: &Site| a.id.cmp(&b.id));
        self.sites = sites;
        self.mark_lines = mark_lines;
        self.problems.extend(problems);
    }

    fn site(&self, id: &str) -> Option<&Site> {
        self.sites.iter().find(|s| s.id == id)
    }

    /// An item's level and where it is written: its own, its impl's, or the nearest enclosing module's.
    fn level_of(&self, id: &str) -> Option<(&str, &str)> {
        self.inherited(id, |m| m.sensitivity.as_ref().map(|s| s.level.as_str()))
    }

    fn owner_of(&self, id: &str) -> Option<(&str, &str)> {
        self.inherited(id, |m| m.owner.as_deref())
    }

    fn inherited<'a>(&'a self, id: &str, pick: impl Fn(&'a Marks) -> Option<&'a str>) -> Option<(&'a str, &'a str)> {
        if let Some(site) = self.site(id) {
            if let Some(v) = pick(&site.marks) {
                return Some((v, site.id.as_str()));
            }
        }
        let decl = self.k.items.iter().find(|i| i.id == id);
        // a method: its impl block's, then its type's (`vault::MasterKey::load` is `vault::MasterKey`'s)
        if let Some(parent) = decl.and_then(|d| d.parent.as_deref()) {
            if let Some(site) = self.site(parent) {
                if let Some(v) = pick(&site.marks) {
                    return Some((v, site.id.as_str()));
                }
            }
        }
        if decl.is_some_and(|d| d.kind == NodeKind::Method) {
            let owner = id.rsplit_once("::").map(|(t, _)| t).unwrap_or("");
            let is_type = self.k.graph.node(owner).is_some_and(|n| matches!(n.kind, NodeKind::Struct | NodeKind::Enum | NodeKind::Union | NodeKind::Trait));
            if let Some(site) = self.site(owner).filter(|_| is_type) {
                if let Some(v) = pick(&site.marks) {
                    return Some((v, site.id.as_str()));
                }
            }
        }
        let module = decl.map(|d| d.module.clone()).unwrap_or_else(|| id.rsplit_once("::").map(|(m, _)| m.to_owned()).unwrap_or_default());
        let mut at = Some(module.as_str());
        while let Some(m) = at {
            if let Some(site) = self.sites.iter().find(|s| s.kind == "mod" && s.id == m) {
                if let Some(v) = pick(&site.marks) {
                    return Some((v, site.id.as_str()));
                }
            }
            at = m.rsplit_once("::").map(|(parent, _)| parent);
        }
        None
    }

    /// Resolve a path a mark wrote (`crate::x`, `self::x`, `super::x`, `vault::x`) from the site that wrote it.
    fn resolve_written(&self, site: &Site, written: &str) -> String {
        let module = match self.k.items.iter().find(|i| i.id == site.id) {
            Some(decl) => decl.module.clone(),
            None => site.id.rsplit_once("::").map(|(m, _)| m.to_owned()).unwrap_or_default(),
        };
        if let Some(rest) = written.strip_prefix("crate::") {
            let krate = module.split("::").next().unwrap_or(&site.package);
            return format!("{krate}::{rest}");
        }
        if let Some(rest) = written.strip_prefix("self::") {
            return format!("{module}::{rest}");
        }
        let mut module = module.as_str();
        let mut rest = written;
        while let Some(r) = rest.strip_prefix("super::") {
            module = module.rsplit_once("::").map(|(p, _)| p).unwrap_or(module);
            rest = r;
        }
        if rest != written {
            return format!("{module}::{rest}");
        }
        written.replace('-', "_")
    }

    /// Every edge of the graph, but those written inside a mark.
    fn edges(&self) -> impl Iterator<Item = &klepto::ItemEdge> {
        self.k.graph.edges.iter().filter(|e| !self.mark_lines.contains(&(e.location.path.clone(), e.location.line.unwrap_or(0))))
    }

    /// Every reference into `id` (or into what it holds), from outside it: `(from, file, line)`.
    fn users_of(&self, id: &str) -> Vec<(String, Option<(String, u32)>)> {
        let inside = |x: &str| x == id || x.starts_with(&format!("{id}::"));
        let mut seen = BTreeSet::new();
        self.edges()
            .filter(|e| matches!(e.kind, EdgeKind::Reference | EdgeKind::Import | EdgeKind::ReExport) && inside(&e.to) && !inside(&e.from))
            .filter(|e| seen.insert(e.from.clone()))
            .map(|e| (e.from.clone(), Some((self.relative(&e.location.path), e.location.line.unwrap_or(0)))))
            .collect()
    }

    // ---- the checks -------------------------------------------------------------------------------------------

    /// Every reference from a function to an item of a higher level than the function's: calls, and the types and
    /// items its body and signature name. An item nothing marks is `[sensitivity] unmarked` (the lowest level when
    /// absent); a reach of more than `max_rise` levels (0 when absent) is a finding unless `[[sensitivity.allow]]`
    /// names it, then a note.
    pub fn sensitivity(&self) -> Report {
        let mut report = Report::new("sensitivity", self);
        let table = &self.table;
        let cfg = &self.config.sensitivity;
        let lowest = table.levels.first().map(|(n, _)| n.clone()).unwrap_or_default();
        let unmarked = cfg.unmarked.clone().unwrap_or(lowest);
        if table.place(&unmarked).is_none() {
            report.findings.push(Finding::new(Code::CheckConfig, format!("[sensitivity] unmarked = \"{unmarked}\"; the levels are {}", table.listed())));
            return report.finish();
        }
        let max_rise = cfg.max_rise.unwrap_or(0) as usize;
        for site in &self.sites {
            if let Some(s) = &site.marks.sensitivity {
                if table.place(&s.level).is_none() {
                    report.findings.push(
                        Finding::new(Code::LevelNotInTable, format!("{} is `{}`; {}'s levels are {}", site.id, s.level, table.word, table.listed()))
                            .about(&site.id)
                            .at(Some((site.file.clone(), site.line))),
                    );
                }
            }
        }
        let callers = |id: &str| self.k.graph.node(id).is_some_and(|n| matches!(n.kind, NodeKind::Fn | NodeKind::Method) && n.package.is_some());
        let mut judged = 0u64;
        let mut seen = BTreeSet::new();
        for edge in self.edges().filter(|e| e.kind == EdgeKind::Reference && callers(&e.from)) {
            if !self.k.graph.node(&edge.to).is_some_and(|n| n.package.is_some()) {
                continue;
            }
            let Some((to_level, to_from)) = self.level_of(&edge.to) else { continue };
            let (from_level, from_note) = match self.level_of(&edge.from) {
                Some((level, _)) => (level.to_owned(), String::new()),
                None => (unmarked.clone(), ", unmarked".to_owned()),
            };
            let (Some(from_place), Some(to_place)) = (table.place(&from_level), table.place(to_level)) else { continue };
            judged += 1;
            if to_place <= from_place + max_rise || !seen.insert((edge.from.clone(), edge.to.clone())) {
                continue;
            }
            let inherited = if to_from == edge.to { String::new() } else { format!(", from {to_from}") };
            let ctx = format!("{} ({from_level}{from_note}) uses {} ({to_level}{inherited})", edge.from, edge.to);
            let allowed = cfg.allow.iter().any(|a| a.from == edge.from && a.to == edge.to);
            let f = Finding::new(Code::CallRises, &ctx)
                .about(&edge.from)
                .target(&edge.to)
                .at(Some((self.relative(&edge.location.path), edge.location.line.unwrap_or(0))));
            if allowed {
                let mut note = f;
                note.message = format!("{} (allowed by maintenance.toml)", note.message);
                report.notes.push(note);
            } else {
                report.findings.push(f);
            }
        }
        report.counts.insert("references_judged".into(), judged);
        report.counts.insert("unresolved_references".into(), self.k.graph.unresolved.iter().filter(|u| callers(&u.from)).count() as u64);
        report.counts.insert("items_with_a_level".into(), self.sites.iter().filter(|s| s.marks.sensitivity.is_some()).count() as u64);
        report.finish()
    }

    /// Every `#[mechanism("name")]` claimed by more than one item.
    pub fn mechanism(&self) -> Report {
        let mut report = Report::new("mechanism", self);
        let mut by_name: BTreeMap<&str, Vec<&Site>> = BTreeMap::new();
        for site in &self.sites {
            if let Some(name) = &site.marks.mechanism {
                by_name.entry(name.as_str()).or_default().push(site);
            }
        }
        for (name, claims) in &by_name {
            let [first, rest @ ..] = claims.as_slice() else { continue };
            for again in rest {
                report.findings.push(
                    Finding::new(Code::MechanismTwice, format!("\"{name}\" is {} ({}:{}) and {}", first.id, first.file, first.line, again.id))
                        .about(&again.id)
                        .target(&first.id)
                        .at(Some((again.file.clone(), again.line))),
                );
            }
        }
        report.counts.insert("mechanisms".into(), by_name.len() as u64);
        report.finish()
    }

    /// Every use of what is inside a `#[boundary]` / `boundary_here!` from a crate it does not allow: a reference or
    /// an import of an item inside it, and, for a crate root, a Cargo dependency on the crate.
    pub fn boundary(&self) -> Report {
        let mut report = Report::new("boundary", self);
        let norm = |s: &str| s.replace('-', "_");
        let mut bounds = 0u64;
        for site in &self.sites {
            let Some(allowed) = &site.marks.boundary else { continue };
            bounds += 1;
            let allowed: BTreeSet<String> = allowed.iter().map(|a| norm(a)).collect();
            let own = norm(&site.package);
            let shown = if allowed.is_empty() { "none".to_owned() } else { allowed.iter().cloned().collect::<Vec<_>>().join(", ") };
            let inside = |x: &str| x == site.id || x.starts_with(&format!("{}::", site.id));
            let mut seen = BTreeSet::new();
            for edge in self.edges().filter(|e| matches!(e.kind, EdgeKind::Reference | EdgeKind::Import) && inside(&e.to)) {
                let Some(from_pkg) = self.package_of_node(&edge.from).map(norm) else { continue };
                if from_pkg == own || allowed.contains(&from_pkg) || !seen.insert((edge.from.clone(), edge.to.clone())) {
                    continue;
                }
                report.findings.push(
                    Finding::new(Code::BoundaryCrossed, format!("{} (crate {from_pkg}) uses {}; {} allows {shown}", edge.from, edge.to, site.id))
                        .about(&edge.from)
                        .target(&edge.to)
                        .at(Some((self.relative(&edge.location.path), edge.location.line.unwrap_or(0)))),
                );
            }
            // a crate root's boundary also covers who may depend on the crate at all
            if site.id == site.package || site.id == own {
                for (pkg, deps) in &self.k.package_deps {
                    let pkg_n = norm(pkg);
                    if pkg_n != own && !allowed.contains(&pkg_n) && deps.iter().any(|d| norm(d) == own) {
                        report.findings.push(
                            Finding::new(Code::BoundaryCrossed, format!("crate {pkg_n} depends on crate {own}; {} allows {shown}", site.id)).about(&pkg_n).target(&own),
                        );
                    }
                }
            }
        }
        report.counts.insert("boundaries".into(), bounds);
        report.finish()
    }

    /// For every `#[supersedes(old)]`: what still uses `old`. Notes, not findings: zero users means it can go.
    pub fn supersedes(&self) -> Report {
        let mut report = Report::new("supersedes", self);
        let mut remaining = 0u64;
        for site in &self.sites {
            let Some(old) = &site.marks.supersedes else { continue };
            let old = self.resolve_written(site, &attrs::path_text(old));
            if self.k.graph.node(&old).is_none() {
                report.notes.push(Finding::new(Code::SupersededGone, format!("{} supersedes {old}", site.id)).about(&site.id).target(&old).at(Some((site.file.clone(), site.line))));
                continue;
            }
            let users = self.users_of(&old);
            remaining += users.len() as u64;
            if users.is_empty() {
                report.notes.push(Finding::new(Code::SupersededFree, format!("{} supersedes {old}", site.id)).about(&site.id).target(&old).at(self.place_of_node(&old)));
            } else {
                let names: Vec<String> = users.iter().map(|(u, place)| match place {
                    Some((file, line)) => format!("{u} ({file}:{line})"),
                    None => u.clone(),
                }).collect();
                report.notes.push(
                    Finding::new(Code::SupersededStillUsed, format!("{} supersedes {old}, still used by {}: {}", site.id, users.len(), names.join(", ")))
                        .about(&site.id)
                        .target(&old)
                        .at(self.place_of_node(&old)),
                );
            }
        }
        report.counts.insert("remaining_users".into(), remaining);
        report.finish()
    }

    /// Every `#[reads_env("KEY")]` whose key no declaration file declares. `declared_in` is where keys are
    /// declared: the files each glob matches (relative to the workspace root), and the dotted path inside each.
    pub fn reads_env(&self, declared_in: &[DeclaredIn]) -> Report {
        let mut report = Report::new("reads_env", self);
        let mut declared: BTreeSet<String> = BTreeSet::new();
        let mut files = 0u64;
        for decl in declared_in {
            match declared_keys(&self.root, decl) {
                Ok((keys, matched)) => {
                    files += matched;
                    declared.extend(keys);
                }
                Err(e) => report.findings.push(Finding::new(Code::CheckConfig, format!("[[env.declared_in]] glob = \"{}\", path = \"{}\": {e}", decl.glob, decl.path))),
            }
        }
        let places: Vec<String> = declared_in.iter().map(|d| format!("{} at {}", d.glob, d.path)).collect();
        let places = if places.is_empty() { "no declaration files are named".to_owned() } else { places.join("; ") };
        let mut read = 0u64;
        for site in &self.sites {
            for key in &site.marks.reads_env {
                read += 1;
                if !declared.contains(key) {
                    report.findings.push(
                        Finding::new(Code::EnvUndeclared, format!("{key} is read by {}; declared in: {places}", site.id)).about(&site.id).at(Some((site.file.clone(), site.line))),
                    );
                }
            }
        }
        report.counts.insert("keys_read".into(), read);
        report.counts.insert("keys_declared".into(), declared.len() as u64);
        report.counts.insert("declaration_files".into(), files);
        report.finish()
    }

    /// Every `#[wire_writer]` with no `#[golden(writer = it)]` test, and every golden naming a writer that is not one.
    pub fn golden(&self) -> Report {
        let mut report = Report::new("golden", self);
        let writers: BTreeSet<&str> = self.sites.iter().filter(|s| s.marks.wire_writer).map(|s| s.id.as_str()).collect();
        let mut pinned: BTreeSet<String> = BTreeSet::new();
        let mut goldens = 0u64;
        for site in &self.sites {
            let Some(writer) = &site.marks.golden else { continue };
            goldens += 1;
            let writer = self.resolve_written(site, &attrs::path_text(writer));
            if writers.contains(writer.as_str()) {
                pinned.insert(writer);
            } else {
                report.findings.push(
                    Finding::new(Code::GoldenWriterUnknown, format!("{} pins {writer}", site.id)).about(&site.id).target(&writer).at(Some((site.file.clone(), site.line))),
                );
            }
        }
        for writer in &writers {
            if !pinned.contains(*writer) {
                let site = self.site(writer);
                report.findings.push(Finding::new(Code::WriterWithoutGolden, *writer).about(writer).at(site.map(|s| (s.file.clone(), s.line))));
            }
        }
        report.counts.insert("wire_writers".into(), writers.len() as u64);
        report.counts.insert("goldens".into(), goldens);
        report.finish()
    }

    /// What the scan itself found wrong, as a report.
    pub fn scan_report(&self) -> Report {
        let mut report = Report::new("scan", self);
        report.findings = self.problems.clone();
        report.counts.insert("files".into(), self.k.files.len() as u64);
        report.counts.insert("items".into(), self.k.items.len() as u64);
        report.counts.insert("marked_items".into(), self.sites.len() as u64);
        report.finish()
    }

    /// The registry this scan makes: every marked item, and every item that takes a level or an owner from its
    /// impl or module.
    pub fn registry(&self) -> Registry {
        let mut reg = Registry::new(self.table.names());
        let mut ids: BTreeSet<String> = self.sites.iter().map(|s| s.id.clone()).collect();
        for decl in &self.k.items {
            if self.level_of(&decl.id).is_some() || self.owner_of(&decl.id).is_some() {
                ids.insert(decl.id.clone());
            }
        }
        for id in ids {
            let site = self.site(&id);
            let decl = self.k.items.iter().find(|i| i.id == id);
            let (package, kind, file, line) = match (site, decl) {
                (Some(s), _) => (s.package.clone(), s.kind.clone(), s.file.clone(), s.line),
                (None, Some(d)) => (
                    self.package_of_file(&d.location.path).unwrap_or(&d.package).to_owned(),
                    node_kind(d.kind).to_owned(),
                    self.relative(&d.location.path),
                    d.location.line.unwrap_or(0),
                ),
                (None, None) => continue,
            };
            let marks = site.map(|s| s.marks.clone()).unwrap_or_default();
            let level = self.level_of(&id);
            let owner = self.owner_of(&id);
            reg.items.push(Entry {
                id: id.clone(),
                package,
                kind,
                file,
                line,
                sensitivity: level.map(|(l, _)| l.to_owned()),
                sensitivity_from: level.map(|(_, f)| f.to_owned()),
                owner: owner.map(|(o, _)| o.to_owned()),
                owner_from: owner.map(|(_, f)| f.to_owned()),
                review: marks.review.map(|reason| Review { reason }),
                deprecated_after: marks.deprecated_after.map(|d| Deprecation { date: d.date, reason: d.reason }),
                mechanism: marks.mechanism,
                boundary: marks.boundary,
                supersedes: marks.supersedes.as_ref().zip(site).map(|(p, s)| self.resolve_written(s, &attrs::path_text(p))),
                reads_env: marks.reads_env,
                golden_writer: marks.golden.as_ref().zip(site).map(|(p, s)| self.resolve_written(s, &attrs::path_text(p))),
                wire_writer: marks.wire_writer,
            });
        }
        reg
    }

    /// Every check, with the scan's own problems first, and `[env] declared_in` from maintenance.toml.
    pub fn all(&self) -> Vec<Report> {
        vec![self.scan_report(), self.sensitivity(), self.mechanism(), self.boundary(), self.supersedes(), self.reads_env(&self.config.env.declared_in), self.golden()]
    }

    /// `<root>/.maintenance/registry.json`, written (write-then-rename).
    pub fn write_registry(&self) -> Result<PathBuf, registry::RegistryError> {
        registry::write(&self.root, &self.registry())
    }
}

fn node_kind(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Fn => "fn",
        NodeKind::Method => "method",
        NodeKind::Struct => "struct",
        NodeKind::Enum => "enum",
        NodeKind::Union => "union",
        NodeKind::Trait => "trait",
        NodeKind::TypeAlias => "type",
        NodeKind::Const => "const",
        NodeKind::Static => "static",
        NodeKind::Macro => "macro",
        NodeKind::Module => "mod",
        NodeKind::Impl => "impl",
        _ => "item",
    }
}

/// The keys one declaration names: every file `glob` matches under `root`, and the dotted `path` inside each,
/// a table (its keys) or a list (its strings, or each table's `name`). How many files matched comes back too.
fn declared_keys(root: &Path, decl: &DeclaredIn) -> Result<(Vec<String>, u64), String> {
    let glob = globset::Glob::new(&decl.glob).map_err(|e| e.to_string())?.compile_matcher();
    let mut keys = Vec::new();
    let mut matched = 0;
    for entry in walkdir::WalkDir::new(root).into_iter().filter_entry(|e| e.file_name() != "target" && e.file_name() != ".git").filter_map(Result::ok) {
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(root) else { continue };
        if !entry.file_type().is_file() || !glob.is_match(rel) {
            continue;
        }
        matched += 1;
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", rel.display()))?;
        let doc: toml::Value = toml::from_str(&text).map_err(|e| format!("{}: {}", rel.display(), e.message()))?;
        let mut at = Some(&doc);
        for part in decl.path.split('.').filter(|p| !p.is_empty()) {
            at = at.and_then(|v| v.get(part));
        }
        match at {
            Some(toml::Value::Table(t)) => keys.extend(t.keys().cloned()),
            Some(toml::Value::Array(items)) => keys.extend(items.iter().filter_map(|v| match v {
                toml::Value::String(s) => Some(s.clone()),
                toml::Value::Table(t) => t.get("name").and_then(|n| n.as_str()).map(str::to_owned),
                _ => None,
            })),
            _ => {}
        }
    }
    Ok((keys, matched))
}

/// Walks one file's items, keeping the module and the impl it is inside.
struct Walk<'a> {
    ws: &'a Workspace,
    file: &'a Path,
    rel: String,
    package: String,
    sites: &'a mut Vec<Site>,
    problems: &'a mut Vec<Finding>,
    decls: &'a mut Vec<(LevelTable, String)>,
    mark_lines: &'a mut BTreeSet<(PathBuf, u32)>,
}

impl Walk<'_> {
    fn line(span: proc_macro2::Span) -> u32 {
        span.start().line as u32
    }

    /// klepto's id for the item that starts at `line` in this file, of one of `kinds`.
    fn id_at(&self, line: u32, kinds: &[NodeKind]) -> Option<String> {
        self.ws.k.items.iter().find(|i| i.location.path == self.file && i.location.line == Some(line) && kinds.contains(&i.kind)).map(|i| i.id.clone())
    }

    fn read(&mut self, attrs: &[syn::Attribute], at: u32) -> Marks {
        for attr in attrs.iter().filter(|a| attrs::MARKS.iter().any(|n| attrs::is_named(a, n))) {
            let span = attr.span();
            for line in span.start().line..=span.end().line {
                self.mark_lines.insert((self.file.to_owned(), line as u32));
            }
        }
        let mut errs = SynBuffer::new();
        let marks = attrs::read_marks(attrs, &mut errs);
        if let Err(e) = errs.finish() {
            for one in e {
                let line = Self::line(one.span());
                self.problems.push(Finding::new(Code::MarkUnread, one.to_string()).at(Some((self.rel.clone(), if line == 0 { at } else { line }))));
            }
        }
        marks
    }

    fn push(&mut self, id: String, kind: &str, line: u32, marks: Marks) {
        if !marks.is_empty() {
            self.sites.push(Site { id, package: self.package.clone(), kind: kind.to_owned(), file: self.rel.clone(), line, marks });
        }
    }

    fn items(&mut self, items: &[syn::Item], module: &str) {
        for item in items {
            let line = Self::line(item.span());
            let named = |ident: &syn::Ident| format!("{module}::{ident}");
            match item {
                syn::Item::Mod(m) => {
                    let id = named(&m.ident);
                    let marks = self.read(&m.attrs, line);
                    self.push(id.clone(), "mod", line, marks);
                    if let Some((_, inner)) = &m.content {
                        self.items(inner, &id);
                    }
                }
                syn::Item::Impl(imp) => {
                    let marks = self.read(&imp.attrs, line);
                    let id = self.id_at(line, &[NodeKind::Impl]).unwrap_or_else(|| format!("{module}::<impl>"));
                    self.push(id, "impl", line, marks);
                    for inner in &imp.items {
                        if let syn::ImplItem::Fn(f) = inner {
                            let fl = Self::line(f.span());
                            let marks = self.read(&f.attrs, fl);
                            let id = self.id_at(fl, &[NodeKind::Method, NodeKind::Fn]).unwrap_or_else(|| named(&f.sig.ident));
                            self.push(id, "method", fl, marks);
                        }
                    }
                }
                syn::Item::Trait(t) => {
                    let marks = self.read(&t.attrs, line);
                    self.push(named(&t.ident), "trait", line, marks);
                    for inner in &t.items {
                        if let syn::TraitItem::Fn(f) = inner {
                            let fl = Self::line(f.span());
                            let marks = self.read(&f.attrs, fl);
                            let id = self.id_at(fl, &[NodeKind::Method, NodeKind::Fn]).unwrap_or_else(|| format!("{module}::{}::{}", t.ident, f.sig.ident));
                            self.push(id, "method", fl, marks);
                        }
                    }
                }
                syn::Item::Macro(mac) => {
                    let Some(name) = mac.mac.path.segments.last().map(|s| s.ident.to_string()) else { continue };
                    if name == "sensitivity_levels" {
                        match crate::levels::parse_declaration(mac.mac.tokens.clone()) {
                            Ok(decl) => self.decls.push((decl.table(), format!("{}:{line}", self.rel))),
                            Err(e) => self.problems.push(Finding::new(Code::MarkUnread, e).at(Some((self.rel.clone(), line)))),
                        }
                    } else if name == "boundary_here" {
                        let mut errs = SynBuffer::new();
                        let allowed = attrs::read_boundary(&attrs::as_attribute(attrs::BOUNDARY, mac.mac.tokens.clone()), &mut errs);
                        if let Err(e) = errs.finish() {
                            self.problems.push(Finding::new(Code::MarkUnread, e).at(Some((self.rel.clone(), line))));
                        }
                        if allowed.is_some() {
                            let marks = Marks { boundary: allowed, ..Marks::default() };
                            self.push(module.to_owned(), "mod", line, marks);
                        }
                    }
                }
                other => {
                    let (attrs, ident, kind, kinds): (&[syn::Attribute], &syn::Ident, &str, &[NodeKind]) = match other {
                        syn::Item::Fn(i) => (&i.attrs, &i.sig.ident, "fn", &[NodeKind::Fn]),
                        syn::Item::Struct(i) => (&i.attrs, &i.ident, "struct", &[NodeKind::Struct]),
                        syn::Item::Enum(i) => (&i.attrs, &i.ident, "enum", &[NodeKind::Enum]),
                        syn::Item::Union(i) => (&i.attrs, &i.ident, "union", &[NodeKind::Union]),
                        syn::Item::Type(i) => (&i.attrs, &i.ident, "type", &[NodeKind::TypeAlias]),
                        syn::Item::Const(i) => (&i.attrs, &i.ident, "const", &[NodeKind::Const]),
                        syn::Item::Static(i) => (&i.attrs, &i.ident, "static", &[NodeKind::Static]),
                        _ => continue,
                    };
                    let marks = self.read(attrs, line);
                    let id = self.id_at(line, kinds).unwrap_or_else(|| named(ident));
                    self.push(id, kind, line, marks);
                }
            }
        }
    }
}

fn with_problems(ws: &Workspace, mut report: Report) -> Report {
    let mut findings = ws.problems.clone();
    findings.append(&mut report.findings);
    report.findings = findings;
    report.finish()
}

/// Scan `root` and run [`Workspace::sensitivity`]; the scan's own problems lead the findings.
pub fn sensitivity(root: &Path) -> Result<Report, CheckError> {
    let ws = Workspace::scan(root)?;
    Ok(with_problems(&ws, ws.sensitivity()))
}

/// Scan `root` and run [`Workspace::mechanism`].
pub fn mechanism(root: &Path) -> Result<Report, CheckError> {
    let ws = Workspace::scan(root)?;
    Ok(with_problems(&ws, ws.mechanism()))
}

/// Scan `root` and run [`Workspace::boundary`].
pub fn boundary(root: &Path) -> Result<Report, CheckError> {
    let ws = Workspace::scan(root)?;
    Ok(with_problems(&ws, ws.boundary()))
}

/// Scan `root` and run [`Workspace::supersedes`].
pub fn supersedes(root: &Path) -> Result<Report, CheckError> {
    let ws = Workspace::scan(root)?;
    Ok(with_problems(&ws, ws.supersedes()))
}

/// Scan `root` and run [`Workspace::reads_env`] against the files `decl_glob` matches (relative to `root`),
/// reading the keys at the dotted `key_path` in each.
pub fn reads_env(root: &Path, decl_glob: &str, key_path: &str) -> Result<Report, CheckError> {
    let ws = Workspace::scan(root)?;
    let decl = DeclaredIn { glob: decl_glob.to_owned(), path: key_path.to_owned() };
    Ok(with_problems(&ws, ws.reads_env(std::slice::from_ref(&decl))))
}

/// Scan `root` and run [`Workspace::golden`].
pub fn golden(root: &Path) -> Result<Report, CheckError> {
    let ws = Workspace::scan(root)?;
    Ok(with_problems(&ws, ws.golden()))
}

/// Scan `root` once and run every check ([`Workspace::all`]).
pub fn all(root: &Path) -> Result<Vec<Report>, CheckError> {
    Ok(Workspace::scan(root)?.all())
}

/// Scan `root` and make its registry.
pub fn registry(root: &Path) -> Result<Registry, CheckError> {
    Ok(Workspace::scan(root)?.registry())
}

/// Scan `root` and write `<root>/.maintenance/registry.json`.
pub fn write_registry(root: &Path) -> Result<PathBuf, CheckError> {
    Workspace::scan(root)?.write_registry().map_err(|e| CheckError { code: e.code, detail: e.detail })
}
