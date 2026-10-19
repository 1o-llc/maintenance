//! The registry: every item the attributes mark, in one file.
//!
//! The file is `<workspace>/.maintenance/registry.json` ([`path`]), written whole by `sensitivity registry` and
//! `sensitivity check` (write-then-rename, so a reader never sees half of one) and read by [`read`]. It does not
//! depend on a build: it is made from the source, so an item is in it whether or not anything links it.
//!
//! ```json
//! {
//!   "format": 1,
//!   "levels": ["public", "internal", "confidential", "secret", "key_material"],
//!   "items": [
//!     {
//!       "id": "vault::open_vault",
//!       "package": "vault",
//!       "kind": "fn",
//!       "file": "crates/vault/src/lib.rs",
//!       "line": 12,
//!       "sensitivity": "secret",
//!       "sensitivity_from": "vault::open_vault",
//!       "owner": "platform",
//!       "owner_from": "vault",
//!       "review": { "reason": "touches the key store" },
//!       "deprecated_after": { "date": "2026-12-01", "reason": "use open_vault_v2" }
//!     }
//!   ]
//! }
//! ```
//!
//! `levels` is the workspace's level table, lowest first: a level's place in it is its rank, the bit it has in the
//! level word. `id` is the item's canonical path (klepto's spelling: `package::module::Type::method`), `file` is
//! relative to the workspace root, `line` is the line of the item's name. `sensitivity` and `owner` are the item's
//! own or the nearest enclosing `mod` / `impl`'s, and `*_from` names the item they are written on; `review` and
//! `deprecated_after` are the item's own. A field with nothing to say is `null`. Items are sorted by `id`.
//!
//! Inside a running binary the same marks are in [`linked`]: what each attribute submitted, for a test that wants
//! to ask its own crate.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::codes::{render_ctx, Code};

/// The folder beside the workspace's `Cargo.toml` that this set writes into. Named once.
pub const DIR: &str = ".maintenance";
/// The registry's file name inside [`DIR`].
pub const FILE: &str = "registry.json";
/// The format this crate writes and reads.
pub const FORMAT: u32 = 1;

/// `<workspace_root>/.maintenance/registry.json`.
pub fn path(workspace_root: &Path) -> PathBuf {
    workspace_root.join(DIR).join(FILE)
}

/// The whole file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registry {
    pub format: u32,
    /// The level table, lowest first; a level's index is its rank.
    pub levels: Vec<String>,
    /// Sorted by `id`.
    pub items: Vec<Entry>,
}

/// One marked item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub id: String,
    pub package: String,
    /// `fn`, `method`, `struct`, `enum`, `union`, `trait`, `type`, `const`, `static`, `mod`, `impl`.
    pub kind: String,
    pub file: String,
    pub line: u32,
    pub sensitivity: Option<String>,
    pub sensitivity_from: Option<String>,
    pub owner: Option<String>,
    pub owner_from: Option<String>,
    pub review: Option<Review>,
    pub deprecated_after: Option<Deprecation>,
}

/// `#[requires_review(reason = "...")]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub reason: String,
}

/// `#[deprecated_after("2026-12-01", reason = "...")]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deprecation {
    pub date: String,
    pub reason: String,
}

impl Registry {
    /// An empty registry over `levels`.
    pub fn new(levels: Vec<String>) -> Self {
        Self { format: FORMAT, levels, items: Vec::new() }
    }

    /// A level's rank: its place in the table, lowest first.
    pub fn rank(&self, level: &str) -> Option<usize> {
        self.levels.iter().position(|l| l == level)
    }

    /// The entry for `id`.
    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.items.binary_search_by(|e| e.id.as_str().cmp(id)).ok().map(|i| &self.items[i])
    }

    /// Every item whose level ranks at `level` or above. Empty for a level the table does not have.
    pub fn at_or_above<'a>(&'a self, level: &str) -> impl Iterator<Item = &'a Entry> + 'a {
        let floor = self.rank(level);
        self.items.iter().filter(move |e| match (floor, e.sensitivity.as_deref().and_then(|l| self.rank(l))) {
            (Some(floor), Some(rank)) => rank >= floor,
            _ => false,
        })
    }
}

/// Why the registry could not be read or written; prints as a coded line.
#[derive(Debug)]
pub struct RegistryError {
    pub code: Code,
    pub detail: String,
}

impl core::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&render_ctx(self.code, &self.detail))
    }
}

impl std::error::Error for RegistryError {}

/// Read `<workspace_root>/.maintenance/registry.json`.
pub fn read(workspace_root: &Path) -> Result<Registry, RegistryError> {
    let file = path(workspace_root);
    let text = std::fs::read_to_string(&file)
        .map_err(|e| RegistryError { code: Code::RegistryUnreadable, detail: format!("{}: {e}", file.display()) })?;
    from_json(&text).map_err(|detail| RegistryError { code: Code::RegistryFormat, detail: format!("{}: {detail}", file.display()) })
}

/// Read a registry from its text.
pub fn from_json(text: &str) -> Result<Registry, String> {
    let registry: Registry = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if registry.format != FORMAT {
        return Err(format!("format {} (this reader reads {FORMAT})", registry.format));
    }
    Ok(registry)
}

/// Write the registry whole: to a temporary file beside it, then renamed over it. Items are sorted first.
pub fn write(workspace_root: &Path, registry: &Registry) -> Result<PathBuf, RegistryError> {
    let mut sorted = registry.clone();
    sorted.items.sort_by(|a, b| a.id.cmp(&b.id));
    let text = serde_json::to_string_pretty(&sorted).map_err(|e| RegistryError { code: Code::RegistryUnwritten, detail: e.to_string() })?;
    let file = path(workspace_root);
    crate::fsio::write_then_rename(&file, format!("{text}\n").as_bytes())
        .map_err(|e| RegistryError { code: Code::RegistryUnwritten, detail: format!("{}: {e}", file.display()) })?;
    Ok(file)
}

/// What one attribute submitted inside a running binary: the item as the compiler saw it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    /// `module_path!()` where the item is.
    pub module: &'static str,
    /// The item's name; `impl` for an impl block (`self_ty` says of what).
    pub name: &'static str,
    pub kind: &'static str,
    pub file: &'static str,
    pub line: u32,
    pub what: Marked,
}

/// Which attribute, and what it said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marked {
    Sensitivity { level: &'static str, rank: u32 },
    Owner { owner: &'static str },
    Review { reason: &'static str },
    DeprecatedAfter { date: &'static str, reason: &'static str },
}

inventory::collect!(Mark);

/// Every mark linked into this binary, in no fixed order.
pub fn linked() -> impl Iterator<Item = &'static Mark> {
    inventory::iter::<Mark>.into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, level: Option<&str>) -> Entry {
        Entry {
            id: id.into(),
            package: "p".into(),
            kind: "fn".into(),
            file: "src/lib.rs".into(),
            line: 1,
            sensitivity: level.map(Into::into),
            sensitivity_from: level.map(|_| id.into()),
            owner: None,
            owner_from: None,
            review: None,
            deprecated_after: None,
        }
    }

    #[test]
    fn the_file_round_trips_and_answers_by_rank() {
        let dir = crate::fsio::scratch_dir("registry_round_trip");
        let mut reg = Registry::new(["public", "internal", "secret"].map(String::from).to_vec());
        reg.items = vec![entry("p::z", Some("secret")), entry("p::a", Some("public")), entry("p::m", None)];
        let written = write(&dir, &reg).unwrap();
        assert_eq!(written, dir.join(".maintenance/registry.json"));
        let back = read(&dir).unwrap();
        assert_eq!(back.items.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["p::a", "p::m", "p::z"], "sorted by id");
        assert_eq!(back.get("p::z").unwrap().sensitivity.as_deref(), Some("secret"));
        assert_eq!(back.at_or_above("internal").map(|e| e.id.as_str()).collect::<Vec<_>>(), ["p::z"]);
        assert_eq!(back.at_or_above("public").count(), 2);
        assert_eq!(back.at_or_above("nope").count(), 0);
    }

    #[test]
    fn another_format_or_a_missing_file_is_a_coded_refusal() {
        let dir = crate::fsio::scratch_dir("registry_refusals");
        let err = read(&dir).unwrap_err();
        assert!(err.to_string().starts_with("[MAINT7001] the registry file could not be read: "), "{err}");
        let err = from_json(r#"{"format":2,"levels":[],"items":[]}"#).unwrap_err();
        assert_eq!(err, "format 2 (this reader reads 1)");
        assert!(from_json(r#"{"format":1,"levels":[],"items":[],"extra":1}"#).is_err(), "unknown fields are refused");
    }
}
