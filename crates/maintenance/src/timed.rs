//! What a `#[timed_test]` runs: a clock around the test, a line in the run's report, and a coded failure when the
//! test took longer than its threshold.
//!
//! The report is `<root>/.maintenance/timings/<run>.jsonl`: one JSON line per test run, appended under a lock and
//! written back whole (write-then-rename), so parallel tests and parallel test binaries each land their line. `root`
//! is the folder of the `maintenance.toml` the test's threshold came from (the crate's folder when there is none);
//! `MAINTENANCE_TIMINGS` names another folder, `MAINTENANCE_RUN` another run (`latest` when unset).
//!
//! The clock measures wall time from the test's first line to its last. A test is failed after it returns; one
//! that never returns is not stopped.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use maintenance_core::codes::{render_ctx, Code};
use maintenance_core::duration::show_ms;

/// The report folder's name inside [`crate::registry::DIR`].
pub const TIMINGS: &str = "timings";
/// The run a report goes to when `MAINTENANCE_RUN` is unset.
pub const DEFAULT_RUN: &str = "latest";

/// What the macro knows about one test, decided when it compiled.
#[derive(Debug)]
pub struct TestMeta {
    pub module: &'static str,
    pub name: &'static str,
    pub file: &'static str,
    pub line: u32,
    pub max_ms: u64,
    pub class: Option<&'static str>,
    /// `max`, `class` or `default`.
    pub threshold_from: &'static str,
    /// The folder the report goes under.
    pub root: &'static str,
}

/// How a test ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Returned within its threshold.
    Passed,
    /// Returned, past its threshold: failed by the clock.
    Over,
    /// Panicked (its own assertion, or a `#[should_panic]` test doing what it should).
    Panicked,
}

/// One line of the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    /// `module::name`.
    pub test: String,
    pub file: String,
    pub line: u32,
    pub ms: u64,
    pub max_ms: u64,
    pub class: Option<String>,
    pub threshold_from: String,
    pub outcome: Outcome,
    /// When it ended, milliseconds since the Unix epoch.
    pub at_unix_ms: u64,
}

/// The report file this test's line goes to.
pub fn report_file(root: &Path) -> PathBuf {
    let dir = std::env::var_os("MAINTENANCE_TIMINGS")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(crate::registry::DIR).join(TIMINGS));
    let run = std::env::var("MAINTENANCE_RUN").ok().filter(|r| !r.is_empty()).unwrap_or_else(|| DEFAULT_RUN.to_owned());
    dir.join(format!("{run}.jsonl"))
}

/// The clock a test runs under. [`Clock::finish`] when the test returns; dropped without it (the test panicked),
/// the line still goes in the report.
#[must_use = "a clock measures until finish() or until it drops"]
pub struct Clock {
    meta: &'static TestMeta,
    started: Instant,
    finished: bool,
}

/// Start the clock for one test.
pub fn start(meta: &'static TestMeta) -> Clock {
    Clock { meta, started: Instant::now(), finished: false }
}

impl Clock {
    /// The test returned: record it, and fail it with `[MAINT2050]` when it ran past its threshold.
    pub fn finish(mut self) {
        self.finished = true;
        let ms = self.elapsed_ms();
        let over = ms > self.meta.max_ms;
        record(self.meta, ms, if over { Outcome::Over } else { Outcome::Passed });
        if over {
            panic!("{}", over_message(self.meta, ms));
        }
    }

    fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

impl Drop for Clock {
    fn drop(&mut self) {
        if !self.finished {
            record(self.meta, self.elapsed_ms(), Outcome::Panicked);
        }
    }
}

/// `[MAINT2050] the test ran past its threshold: m::t took 2s310ms; its threshold is 2s (max)`.
pub fn over_message(meta: &TestMeta, ms: u64) -> String {
    let from = match meta.class {
        Some(class) => format!("class {class}, {}", meta.threshold_from),
        None => meta.threshold_from.to_owned(),
    };
    render_ctx(Code::OverThreshold, format!("{}::{} took {}; its threshold is {} ({from})", meta.module, meta.name, show_ms(ms), show_ms(meta.max_ms)))
}

fn record(meta: &TestMeta, ms: u64, outcome: Outcome) {
    let line = Record {
        test: format!("{}::{}", meta.module, meta.name),
        file: meta.file.to_owned(),
        line: meta.line,
        ms,
        max_ms: meta.max_ms,
        class: meta.class.map(str::to_owned),
        threshold_from: meta.threshold_from.to_owned(),
        outcome,
        at_unix_ms: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
    };
    let file = report_file(Path::new(meta.root));
    let written = serde_json::to_string(&line).map_err(std::io::Error::other).and_then(|text| crate::fsio::append_line(&file, &text));
    if let Err(e) = written {
        // the test's own result stands; the report says it is missing a line
        eprintln!("{}", render_ctx(Code::ReportUnwritten, format!("{}: {e}", file.display())));
    }
}

/// Every record in `path`: a `.jsonl` file, a folder of them, or a workspace root holding
/// `.maintenance/timings`. Lines that do not read are counted, not hidden.
pub fn read_records(path: &Path) -> std::io::Result<(Vec<Record>, usize)> {
    let nested = path.join(crate::registry::DIR).join(TIMINGS);
    let files: Vec<PathBuf> = if path.is_file() {
        vec![path.to_owned()]
    } else {
        let dir = if nested.is_dir() { nested } else { path.to_owned() };
        let mut files: Vec<PathBuf> =
            std::fs::read_dir(&dir)?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "jsonl")).collect();
        files.sort();
        files
    };
    let (mut records, mut unreadable) = (Vec::new(), 0);
    for file in files {
        for line in std::fs::read_to_string(&file)?.lines().filter(|l| !l.trim().is_empty()) {
            match serde_json::from_str::<Record>(line) {
                Ok(r) => records.push(r),
                Err(_) => unreadable += 1,
            }
        }
    }
    Ok((records, unreadable))
}

/// The latest record of each test, slowest first.
pub fn slowest(records: &[Record]) -> Vec<&Record> {
    let mut latest: std::collections::BTreeMap<&str, &Record> = std::collections::BTreeMap::new();
    for r in records {
        let keep = latest.get(r.test.as_str()).is_none_or(|seen| r.at_unix_ms >= seen.at_unix_ms);
        if keep {
            latest.insert(&r.test, r);
        }
    }
    let mut out: Vec<&Record> = latest.into_values().collect();
    out.sort_by(|a, b| b.ms.cmp(&a.ms).then_with(|| a.test.cmp(&b.test)));
    out
}

/// The format [`TimingReport`] is written in.
pub const REPORT_FORMAT: u32 = 1;

/// What [`report`] hands back: the latest run of every test, slowest first.
///
/// ```json
/// { "format": 1, "source": ".../.maintenance/timings", "tests": [Record, ...], "over": 1, "unreadable": 0 }
/// ```
///
/// `tests` are [`Record`]s (one JSON line of the run file each), `over` is how many of them ran past their threshold
/// (`MAINT2050`), `unreadable` how many lines did not read and are not counted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimingReport {
    pub format: u32,
    pub source: String,
    pub tests: Vec<Record>,
    pub over: u64,
    pub unreadable: u64,
}

/// The timing report under `path`: a `.jsonl` run file, a folder of them, or a workspace root holding
/// `.maintenance/timings`.
pub fn report(path: &Path) -> std::io::Result<TimingReport> {
    let (records, unreadable) = read_records(path)?;
    Ok(TimingReport::of(path, &records, unreadable))
}

impl TimingReport {
    /// The report of `records` read from `source`.
    pub fn of(source: &Path, records: &[Record], unreadable: usize) -> Self {
        let tests: Vec<Record> = slowest(records).into_iter().cloned().collect();
        let over = tests.iter().filter(|r| r.outcome == Outcome::Over).count() as u64;
        Self { format: REPORT_FORMAT, source: source.display().to_string(), tests, over, unreadable: unreadable as u64 }
    }

    /// The report as text: the slowest tests, at most `top`.
    pub fn render(&self, top: usize) -> String {
        let mut out = format!("slowest tests, the latest run of each: {} shown of {}\n", self.tests.len().min(top), self.tests.len());
        out.push_str(&format!("{:>10}  {:>16}  {:>6}  {:<8}  {}\n", "took", "threshold", "of it", "outcome", "test"));
        for r in self.tests.iter().take(top) {
            let share = r.ms.saturating_mul(100).checked_div(r.max_ms).unwrap_or(0);
            let outcome = match r.outcome {
                Outcome::Passed => "passed",
                Outcome::Over => "OVER",
                Outcome::Panicked => "panicked",
            };
            let threshold = match &r.class {
                Some(class) => format!("{} {class}", show_ms(r.max_ms)),
                None => show_ms(r.max_ms),
            };
            out.push_str(&format!("{:>10}  {:>16}  {:>5}%  {:<8}  {}  ({}:{})\n", show_ms(r.ms), threshold, share, outcome, r.test, r.file, r.line));
        }
        if self.over > 0 {
            out.push_str(&format!("{} over threshold [{}]\n", self.over, Code::OverThreshold.code()));
        }
        if self.unreadable > 0 {
            out.push_str(&format!("{} lines did not read and are not counted\n", self.unreadable));
        }
        out
    }
}

use maintenance_core::codes::LiaiseCode as _;

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(root: &'static str, max_ms: u64) -> &'static TestMeta {
        Box::leak(Box::new(TestMeta {
            module: "m",
            name: "t",
            file: "src/lib.rs",
            line: 3,
            max_ms,
            class: Some("unit"),
            threshold_from: "default",
            root,
        }))
    }

    #[test]
    fn a_test_over_its_threshold_fails_coded_and_every_run_is_reported() {
        let root: &'static str = Box::leak(crate::fsio::scratch_dir("timing").display().to_string().into_boxed_str());
        let quick = meta(root, 10_000);
        start(quick).finish();
        let slow = meta(root, 0);
        let clock = start(slow);
        std::thread::sleep(std::time::Duration::from_millis(3));
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| clock.finish())).unwrap_err();
        let message = failed.downcast_ref::<String>().unwrap();
        assert!(message.starts_with("[MAINT2050] the test ran past its threshold: m::t took "), "{message}");
        assert!(message.ends_with("; its threshold is 0ms (class unit, default)"), "{message}");
        let _ = std::panic::catch_unwind(|| {
            let _clock = start(quick);
            panic!("its own assertion");
        });
        let (records, unreadable) = read_records(Path::new(root)).unwrap();
        assert_eq!(unreadable, 0);
        assert_eq!(records.iter().map(|r| r.outcome).collect::<Vec<_>>(), [Outcome::Passed, Outcome::Over, Outcome::Panicked]);
        assert!(report_file(Path::new(root)).ends_with(".maintenance/timings/latest.jsonl"));
    }

    #[test]
    fn the_report_lists_the_latest_run_of_each_slowest_first() {
        let r = |test: &str, ms, at, outcome| Record {
            test: test.into(),
            file: "f.rs".into(),
            line: 1,
            ms,
            max_ms: 1_000,
            class: None,
            threshold_from: "max".into(),
            outcome,
            at_unix_ms: at,
        };
        let records = [r("a", 900, 1, Outcome::Passed), r("a", 100, 2, Outcome::Passed), r("b", 1_500, 1, Outcome::Over), r("c", 300, 1, Outcome::Passed)];
        let order: Vec<(&str, u64)> = slowest(&records).iter().map(|r| (r.test.as_str(), r.ms)).collect();
        assert_eq!(order, [("b", 1_500), ("c", 300), ("a", 100)]);
        let report = TimingReport::of(Path::new("x"), &records, 1);
        assert_eq!((report.over, report.unreadable, report.tests.len()), (1, 1, 3));
        let back: TimingReport = serde_json::from_str(&serde_json::to_string(&report).unwrap()).unwrap();
        assert_eq!(back, report, "the report is data with a fixed shape");
        let text = report.render(2);
        assert!(text.starts_with("slowest tests, the latest run of each: 2 shown of 3\n"), "{text}");
        assert!(text.contains("1s500ms                1s    150%  OVER      b  (f.rs:1)"), "{text}");
        assert!(!text.contains("  a  "), "{text}");
        assert!(text.contains("1 over threshold [MAINT2050]\n1 lines did not read and are not counted\n"), "{text}");
    }
}
