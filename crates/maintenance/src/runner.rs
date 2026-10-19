//! The checks and the timing report as declared lines of the `maintenance` program, so a tool runs them as commands
//! (an xccute line it can preview, approve and run) rather than calling the library.
//!
//! ```text
//! maintenance check <which> --workspace <root> [--registry <file>] [--decl <glob> --key <path>]
//! maintenance timed report <dir>
//! ```
//!
//! `<which>` is one of [`CheckName`]'s words: `sensitivity`, `mechanism`, `boundary`, `supersedes`, `reads_env`,
//! `golden`, `all`. `--registry` writes the registry there as well. `--decl` and `--key` go together and name where
//! `reads_env` finds declared keys; without them it reads maintenance.toml's `[env] declared_in`.
//!
//! Each line names its exit statuses: 0 `clean`, 1 `findings` (a check failed, or a test ran over its threshold),
//! 2 `refused` (the line is not one of these, or the workspace could not be read). On stdout is one JSON document:
//! a [`CheckRun`] for a `check` line, a [`crate::timed::TimingReport`] for `timed report`, a [`Refusal`] for status 2.
//!
//! [`run_argv`] is the one dispatcher: the `maintenance` bin (feature `bin`) is `run_argv` of its arguments, and
//! [`execute`] runs a declared line in this process the same way. The library ([`crate::check`],
//! [`crate::timed::report`]) is the one implementation; this is its facade.

use std::ffi::{OsStr, OsString};
use std::path::Path;

use maintenance_core::codes::{render_ctx, Code, LiaiseCode};
use serde::{Deserialize, Serialize};
use simple_impl_derive::{CompositeShell, SimpleSubCommand};
use xccute_contract::{CommandValidationError, CommandValidationResult, CompositeShellCommand, ValidatedCommand};
use xccute_runtime::{ExitStatusDecision, ExitStatusPolicy, RuntimeExitStatus};

use crate::check::{self, Report};

/// The format [`CheckRun`] and [`Refusal`] are written in.
pub const RUN_FORMAT: u32 = 1;

/// The checks a `check` line names, each by its one word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckName {
    Sensitivity,
    Mechanism,
    Boundary,
    Supersedes,
    ReadsEnv,
    Golden,
    All,
}

impl CheckName {
    /// Every check, in the order `all` runs them.
    pub const ALL: &'static [CheckName] =
        &[CheckName::Sensitivity, CheckName::Mechanism, CheckName::Boundary, CheckName::Supersedes, CheckName::ReadsEnv, CheckName::Golden, CheckName::All];

    /// Its word on the line.
    pub const fn word(self) -> &'static str {
        match self {
            CheckName::Sensitivity => "sensitivity",
            CheckName::Mechanism => "mechanism",
            CheckName::Boundary => "boundary",
            CheckName::Supersedes => "supersedes",
            CheckName::ReadsEnv => "reads_env",
            CheckName::Golden => "golden",
            CheckName::All => "all",
        }
    }

    /// The check a word names.
    pub fn of_word(word: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|c| c.word() == word)
    }
}

/// The `maintenance` program.
#[derive(CompositeShell, Debug, Clone, Copy, Default)]
#[shell(program = "maintenance")]
#[composite(command = "check", ty = Check, init = "check,workspace")]
#[composite(command = "timed", ty = Timed, init = "action,dir")]
pub struct Maintenance;

/// `maintenance check <which> --workspace <root> [--registry <file>] [--decl <glob> --key <path>]`.
// one #[exit_status] per status is the derive's form; clippy reads the repeats as duplicates
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "check")]
#[exit_status(code = 0, word = "clean")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
#[validate(with = "check_valid")]
pub struct Check {
    #[builder(init_required, into)]
    #[arg(positional, each = check_word)]
    pub check: OsString,
    #[builder(init_required, into)]
    #[arg(kv = "--workspace")]
    pub workspace: OsString,
    #[builder(into)]
    #[arg(kv = "--registry")]
    pub registry: Option<OsString>,
    #[builder(into)]
    #[requires("key")]
    #[arg(kv = "--decl")]
    pub decl: Option<OsString>,
    #[builder(into)]
    #[requires("decl")]
    #[arg(kv = "--key")]
    pub key: Option<OsString>,
}

/// `maintenance timed report <dir>`.
#[allow(clippy::duplicated_attributes)]
#[derive(SimpleSubCommand, Debug, Clone, Default)]
#[subcommand(segment = "timed")]
#[exit_status(code = 0, word = "clean")]
#[exit_status(code = 1, word = "findings")]
#[exit_status(code = 2, word = "refused")]
pub struct Timed {
    #[builder(init_required, into)]
    #[arg(positional, each = timed_word)]
    pub action: OsString,
    #[builder(init_required, into)]
    #[arg(positional, each = nonempty)]
    pub dir: OsString,
}

/// The one action a `timed` line takes.
pub const TIMED_REPORT: &str = "report";

fn check_word(field: &str, word: &OsStr) -> CommandValidationResult {
    match word.to_str().and_then(CheckName::of_word) {
        Some(_) => Ok(()),
        None => {
            let words: Vec<&str> = CheckName::ALL.iter().map(|c| c.word()).collect();
            Err(CommandValidationError::structural(format!("{word:?} is not a check: {}", words.join(", "))).with_field(field))
        }
    }
}

fn timed_word(field: &str, word: &OsStr) -> CommandValidationResult {
    if word == TIMED_REPORT {
        Ok(())
    } else {
        Err(CommandValidationError::structural(format!("{word:?} is not a timed action: {TIMED_REPORT}")).with_field(field))
    }
}

fn nonempty(field: &str, word: &OsStr) -> CommandValidationResult {
    if word.is_empty() {
        Err(CommandValidationError::structural(format!("{field} is empty")).with_field(field))
    } else {
        Ok(())
    }
}

fn check_valid(line: &Check) -> CommandValidationResult {
    nonempty("workspace", &line.workspace)
}

impl Maintenance {
    /// `maintenance check <name> --workspace <root>`, by the check's name.
    pub fn check_of(name: CheckName, workspace: impl AsRef<OsStr>) -> CheckRooted<Maintenance> {
        Maintenance::check(name.word(), workspace.as_ref())
    }

    /// `maintenance timed report <dir>`.
    pub fn timed_report(dir: impl AsRef<OsStr>) -> TimedRooted<Maintenance> {
        Maintenance::timed(TIMED_REPORT, dir.as_ref())
    }
}

/// What a `check` line prints: every report it ran, and where it wrote the registry.
///
/// ```json
/// { "format": 1, "workspace": "examples/workspace", "reports": [Report, ...], "registry": null }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckRun {
    pub format: u32,
    pub workspace: String,
    /// [`check::Report`]s: one for a single check, every check for `all` (the scan's own first).
    pub reports: Vec<Report>,
    /// The file `--registry` wrote, when the line named one.
    pub registry: Option<String>,
}

/// What a refused line prints (status 2).
///
/// ```json
/// { "format": 1, "code": "MAINT9001", "message": "[MAINT9001] the line is not a declared maintenance line: ..." }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Refusal {
    pub format: u32,
    pub code: String,
    pub message: String,
}

/// A line, run: its exit status, the word its line names for it, and the JSON document it printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub code: i32,
    pub word: &'static str,
    pub stdout: String,
}

const CLEAN: i32 = 0;
const FINDINGS: i32 = 1;
const REFUSED: i32 = 2;

impl Outcome {
    /// What xccute decides after it, by the statuses the line names (0 goes on, the rest stop).
    pub fn decision<C: CompositeShellCommand + ?Sized>(&self, line: &C) -> ExitStatusDecision {
        let accepted = line.ok_codes().contains(&self.code);
        ExitStatusPolicy::of(line).decide(RuntimeExitStatus::new(Some(self.code), accepted))
    }
}

fn finish<C: CompositeShellCommand + ?Sized>(line: &C, code: i32, stdout: String) -> Outcome {
    let word = line.exit_status_of(Some(code)).map_or("unnamed", |s| s.word);
    Outcome { code, word, stdout }
}

fn refuse(code: Code, detail: impl core::fmt::Display) -> String {
    let refusal = Refusal { format: RUN_FORMAT, code: code.code(), message: render_ctx(code, detail) };
    serde_json::to_string_pretty(&refusal).unwrap_or_default()
}

/// Run a declared line in this process: validated as xccute validates it, then dispatched as the bin would.
pub fn execute<C: CompositeShellCommand + ValidatedCommand>(line: &C) -> Outcome {
    if let Err(e) = line.validate() {
        let refused = Maintenance::check("all", ".");
        return finish(&refused, REFUSED, refuse(Code::LineRefused, e));
    }
    run_argv(&line.argv())
}

/// Run the words after `maintenance`. The words are read back into the declared line and must be what that line
/// writes, word for word, so the bin takes exactly the lines this module declares and nothing else.
pub fn run_argv(args: &[OsString]) -> Outcome {
    let words: Vec<&str> = match args.iter().map(|a| a.to_str()).collect::<Option<Vec<_>>>() {
        Some(words) => words,
        None => return finish(&Maintenance::check("all", "."), REFUSED, refuse(Code::LineRefused, "a word is not UTF-8")),
    };
    match words.first().copied() {
        Some("check") => run_check(args, &words[1..]),
        Some("timed") => run_timed(args, &words[1..]),
        _ => finish(&Maintenance::check("all", "."), REFUSED, refuse(Code::LineRefused, format!("`{}`; the lines are `check <which> --workspace <root> ...` and `timed report <dir>`", words.join(" ")))),
    }
}

/// The line `check ...` reads as, or why not.
fn read_check(words: &[&str]) -> Result<CheckRooted<Maintenance>, String> {
    let (which, rest) = words.split_first().ok_or("check names a check")?;
    let mut workspace = None;
    let mut options: Vec<(&str, &str)> = Vec::new();
    let mut at = rest.iter();
    while let Some(flag) = at.next() {
        let value = at.next().ok_or_else(|| format!("{flag} takes a value"))?;
        match *flag {
            "--workspace" => workspace = Some(*value),
            "--registry" | "--decl" | "--key" => options.push((flag, value)),
            other => return Err(format!("{other} is not an option of check")),
        }
    }
    let mut line = Maintenance::check(*which, workspace.ok_or("check needs --workspace <root>")?);
    for (flag, value) in options {
        line = match flag {
            "--registry" => line.registry(value),
            "--decl" => line.decl(value),
            _ => line.key(value),
        };
    }
    Ok(line)
}

fn run_check(args: &[OsString], words: &[&str]) -> Outcome {
    let line = match read_check(words) {
        Ok(line) => line,
        Err(e) => return finish(&Maintenance::check("all", "."), REFUSED, refuse(Code::LineRefused, e)),
    };
    if line.argv() != args {
        return finish(&line, REFUSED, refuse(Code::LineRefused, "the options are written in the declared order: --workspace, --registry, --decl, --key"));
    }
    if let Err(e) = line.validate() {
        return finish(&line, REFUSED, refuse(Code::LineRefused, e));
    }
    let value = |key: &str| words.iter().position(|w| *w == key).and_then(|i| words.get(i + 1)).copied();
    let workspace = value("--workspace").unwrap_or(".");
    let name = CheckName::of_word(words[0]).unwrap_or(CheckName::All);
    let ws = match check::Workspace::scan(Path::new(workspace)) {
        Ok(ws) => ws,
        Err(e) => return finish(&line, REFUSED, refuse(Code::RunRefused, e)),
    };
    let decl = value("--decl").zip(value("--key")).map(|(glob, path)| vec![maintenance_core::config::DeclaredIn { glob: glob.to_owned(), path: path.to_owned() }]);
    let one = |report: Report| vec![check::with_scan_problems(&ws, report)];
    let reports = match name {
        CheckName::Sensitivity => one(ws.sensitivity()),
        CheckName::Mechanism => one(ws.mechanism()),
        CheckName::Boundary => one(ws.boundary()),
        CheckName::Supersedes => one(ws.supersedes()),
        CheckName::ReadsEnv => one(match &decl {
            Some(decl) => ws.reads_env(decl),
            None => ws.reads_env_configured(),
        }),
        CheckName::Golden => one(ws.golden()),
        CheckName::All => match &decl {
            Some(decl) => ws.all_with(decl),
            None => ws.all(),
        },
    };
    let registry = match value("--registry") {
        Some(file) => match crate::registry::write_to(Path::new(file), &ws.registry()) {
            Ok(()) => Some(file.to_owned()),
            Err(e) => return finish(&line, REFUSED, refuse(Code::RunRefused, e)),
        },
        None => None,
    };
    let code = if reports.iter().all(|r| r.passed) { CLEAN } else { FINDINGS };
    let run = CheckRun { format: RUN_FORMAT, workspace: workspace.to_owned(), reports, registry };
    finish(&line, code, serde_json::to_string_pretty(&run).unwrap_or_default())
}

fn run_timed(args: &[OsString], words: &[&str]) -> Outcome {
    let line = match words {
        [action, dir] => Maintenance::timed(*action, *dir),
        _ => return finish(&Maintenance::timed_report("."), REFUSED, refuse(Code::LineRefused, "timed report <dir>")),
    };
    if line.argv() != args {
        return finish(&line, REFUSED, refuse(Code::LineRefused, "timed report <dir>"));
    }
    if let Err(e) = line.validate() {
        return finish(&line, REFUSED, refuse(Code::LineRefused, e));
    }
    match crate::timed::report(Path::new(words[1])) {
        Ok(report) => {
            let code = if report.over == 0 { CLEAN } else { FINDINGS };
            finish(&line, code, serde_json::to_string_pretty(&report).unwrap_or_default())
        }
        Err(e) => finish(&line, REFUSED, refuse(Code::RunRefused, format!("{}: {e}", words[1]))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lines_are_declared_words() {
        let line = Maintenance::check_of(CheckName::ReadsEnv, "ws").registry("r.json").decl("deploy/*.toml").key("env");
        assert_eq!(line.build_display(), "maintenance check reads_env --workspace ws --registry r.json --decl deploy/*.toml --key env");
        line.validate().unwrap();
        assert_eq!(Maintenance::timed_report("ws").build_display(), "maintenance timed report ws");
        assert_eq!(line.ok_codes(), &[0]);
        assert_eq!(line.exit_status_of(Some(1)).map(|s| s.word), Some("findings"));
        assert_eq!(line.exit_status_of(Some(2)).map(|s| s.word), Some("refused"));
    }

    #[test]
    fn a_line_that_is_not_declared_is_refused() {
        assert!(Maintenance::check("everything", "ws").validate().unwrap_err().to_string().contains("\"everything\" is not a check"));
        assert!(Maintenance::check_of(CheckName::All, "ws").decl("x").validate().is_err(), "--decl without --key");
        assert!(Maintenance::timed("summary", "ws").validate().is_err());
        let out = run_argv(&["check", "all"].map(OsString::from));
        assert_eq!((out.code, out.word), (2, "refused"));
        let refusal: Refusal = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(refusal.code, "MAINT9001");
        assert_eq!(refusal.message, "[MAINT9001] the line is not a declared maintenance line: check needs --workspace <root>");
        let out = run_argv(&["check", "all", "--registry", "r", "--workspace", "ws"].map(OsString::from));
        assert!(out.stdout.contains("the options are written in the declared order"), "{}", out.stdout);
        assert_eq!(run_argv(&["deploy".into()]).code, 2);
    }

    #[test]
    fn a_workspace_that_is_not_there_is_refused_coded() {
        let out = execute(&Maintenance::check_of(CheckName::Golden, "/no/such/workspace"));
        assert_eq!((out.code, out.word), (2, "refused"));
        assert!(out.stdout.contains("MAINT9002"), "{}", out.stdout);
        let decision = out.decision(&Maintenance::check_of(CheckName::Golden, "x"));
        assert_eq!((decision.disposition, decision.reason.as_str()), (xccute_runtime::ExitDisposition::Stop, "refused"));
    }
}
