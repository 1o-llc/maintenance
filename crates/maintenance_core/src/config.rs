//! `maintenance.toml`: the thresholds, as data.
//!
//! ```toml
//! [timed_test]
//! default = "unit"          # the class a bare #[timed_test] gets
//! [timed_test.classes]      # added to (or replacing) the shipped classes
//! unit = "500ms"
//! integration = "10s"
//!
//! [sensitivity]
//! unmarked = "public"       # the level of an item nothing marks; the lowest level when absent
//! max_rise = 0              # how many levels above its own an item may reach; 0 when absent
//! [[sensitivity.allow]]     # a reach the workspace has looked at and accepts
//! from = "api::login"
//! to = "vault::check_password"
//!
//! [[env.declared_in]]       # where environment keys are declared, for #[reads_env]
//! glob = "deploy/*.toml"    # files, relative to this file's folder
//! path = "env"              # the dotted path inside each: a table whose keys are the keys, or a list of them
//! ```
//!
//! The file is the nearest `maintenance.toml` at or above the crate's folder ([`find`]); its folder is the root the
//! timing report and the registry are written under.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::duration::parse_ms;

/// The file's name.
pub const FILE: &str = "maintenance.toml";

/// The classes this crate ships: `unit` 500ms, `integration` 10s, `slow` 1m. A file's classes are added to these
/// and replace one of the same name.
pub const SHIPPED_CLASSES: &[(&str, &str)] = &[("unit", "500ms"), ("integration", "10s"), ("slow", "1m")];

/// The whole file.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceToml {
    #[serde(default)]
    pub timed_test: TimedTestToml,
    #[serde(default)]
    pub sensitivity: SensitivityToml,
    #[serde(default)]
    pub env: EnvToml,
}

/// `[env]`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvToml {
    #[serde(default)]
    pub declared_in: Vec<DeclaredIn>,
}

/// One place keys are declared: the files `glob` matches, and the dotted `path` inside them.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclaredIn {
    pub glob: String,
    pub path: String,
}

/// `[timed_test]`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimedTestToml {
    pub default: Option<String>,
    #[serde(default)]
    pub classes: BTreeMap<String, String>,
}

/// `[sensitivity]`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensitivityToml {
    pub unmarked: Option<String>,
    pub max_rise: Option<u32>,
    #[serde(default)]
    pub allow: Vec<Allow>,
}

/// One accepted reach: `from` may use `to` whatever their levels.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Allow {
    pub from: String,
    pub to: String,
}

/// The nearest `maintenance.toml` at or above `start`.
pub fn find(start: &Path) -> Option<PathBuf> {
    start.ancestors().map(|dir| dir.join(FILE)).find(|f| f.is_file())
}

/// Read a file.
pub fn load(path: &Path) -> Result<MaintenanceToml, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Read a file's text.
pub fn parse(text: &str) -> Result<MaintenanceToml, String> {
    toml::from_str(text).map_err(|e| e.message().to_owned())
}

/// Where a test's threshold came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `max = "2s"` on the test.
    Max,
    /// `class = "integration"` on the test.
    Class,
    /// The file's `default` class.
    Default,
}

impl Source {
    pub const fn as_str(self) -> &'static str {
        match self {
            Source::Max => "max",
            Source::Class => "class",
            Source::Default => "default",
        }
    }
}

/// A test's threshold, decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub max_ms: u64,
    pub class: Option<String>,
    pub from: Source,
}

/// Why a threshold could not be decided: which code, and what about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub code: crate::codes::Code,
    pub detail: String,
}

/// The classes in force: the shipped ones, then the file's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thresholds {
    pub default: Option<String>,
    pub classes: BTreeMap<String, u64>,
}

impl Thresholds {
    /// The shipped classes, no default.
    pub fn shipped() -> Self {
        let classes = SHIPPED_CLASSES.iter().map(|(n, t)| ((*n).to_owned(), parse_ms(t).unwrap_or(0))).collect();
        Self { default: None, classes }
    }

    /// The shipped classes with the file's on top. A class whose length does not read is refused.
    pub fn with_file(file: &TimedTestToml) -> Result<Self, Refused> {
        use crate::codes::Code;
        let mut t = Self::shipped();
        for (name, text) in &file.classes {
            let ms = parse_ms(text).map_err(|e| Refused { code: Code::BadDuration, detail: format!("[timed_test.classes] {name}: {e}") })?;
            t.classes.insert(name.clone(), ms);
        }
        if let Some(default) = &file.default {
            if !t.classes.contains_key(default) {
                return Err(Refused { code: Code::UnknownClass, detail: format!("[timed_test] default = \"{default}\"; the classes are {}", t.listed()) });
            }
            t.default = Some(default.clone());
        }
        Ok(t)
    }

    /// `unit 500ms, integration 10s, ...`
    pub fn listed(&self) -> String {
        self.classes.iter().map(|(n, ms)| format!("{n} {}", crate::duration::show_ms(*ms))).collect::<Vec<_>>().join(", ")
    }

    /// A test's threshold from what it wrote: `max`, else `class`, else the default class.
    pub fn resolve(&self, max: Option<&str>, class: Option<&str>) -> Result<Resolved, Refused> {
        use crate::codes::Code;
        if let Some(max) = max {
            let max_ms = parse_ms(max).map_err(|detail| Refused { code: Code::BadDuration, detail })?;
            return Ok(Resolved { max_ms, class: None, from: Source::Max });
        }
        let (name, from) = match (class, &self.default) {
            (Some(class), _) => (class, Source::Class),
            (None, Some(default)) => (default.as_str(), Source::Default),
            (None, None) => return Err(Refused { code: Code::NoThreshold, detail: format!("the classes are {}", self.listed()) }),
        };
        match self.classes.get(name) {
            Some(ms) => Ok(Resolved { max_ms: *ms, class: Some(name.to_owned()), from }),
            None => Err(Refused { code: Code::UnknownClass, detail: format!("`{name}`; the classes are {}", self.listed()) }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_threshold_comes_from_max_then_class_then_the_default() {
        let file = parse("[timed_test]\ndefault = \"quick\"\n[timed_test.classes]\nquick = \"50ms\"\nunit = \"1s\"\n").unwrap();
        let t = Thresholds::with_file(&file.timed_test).unwrap();
        assert_eq!(t.resolve(Some("2s"), None).unwrap(), Resolved { max_ms: 2_000, class: None, from: Source::Max });
        assert_eq!(t.resolve(None, Some("unit")).unwrap().max_ms, 1_000, "the file's unit replaces the shipped one");
        assert_eq!(t.resolve(None, Some("integration")).unwrap().max_ms, 10_000, "shipped classes stay");
        assert_eq!(t.resolve(None, None).unwrap(), Resolved { max_ms: 50, class: Some("quick".into()), from: Source::Default });
    }

    #[test]
    fn what_cannot_be_decided_is_refused_coded() {
        use crate::codes::{Code, LiaiseCode};
        let t = Thresholds::shipped();
        let e = t.resolve(None, None).unwrap_err();
        assert_eq!((e.code, e.detail.as_str()), (Code::NoThreshold, "the classes are integration 10s, slow 1m, unit 500ms"));
        assert_eq!(t.resolve(None, Some("nightly")).unwrap_err().code, Code::UnknownClass);
        assert_eq!(t.resolve(Some("fast"), None).unwrap_err().code.code(), "MAINT2002");
        let file = parse("[timed_test]\ndefault = \"nope\"\n").unwrap();
        assert_eq!(Thresholds::with_file(&file.timed_test).unwrap_err().code, Code::UnknownClass);
        let file = parse("[timed_test.classes]\nunit = \"soon\"\n").unwrap();
        assert_eq!(Thresholds::with_file(&file.timed_test).unwrap_err().code, Code::BadDuration);
        assert!(parse("[timed_test]\ndefualt = \"unit\"\n").unwrap_err().contains("unknown field `defualt`"));
    }

    #[test]
    fn the_sensitivity_table_reads() {
        let file = parse("[sensitivity]\nunmarked = \"internal\"\nmax_rise = 1\n[[sensitivity.allow]]\nfrom = \"a::f\"\nto = \"b::g\"\n").unwrap();
        assert_eq!(file.sensitivity.unmarked.as_deref(), Some("internal"));
        assert_eq!(file.sensitivity.max_rise, Some(1));
        assert_eq!(file.sensitivity.allow, [Allow { from: "a::f".into(), to: "b::g".into() }]);
        let file = parse("[[env.declared_in]]\nglob = \"deploy/*.toml\"\npath = \"service.env\"\n").unwrap();
        assert_eq!(file.env.declared_in, [DeclaredIn { glob: "deploy/*.toml".into(), path: "service.env".into() }]);
    }
}
