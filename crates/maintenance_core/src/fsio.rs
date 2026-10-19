//! Writing a file so a reader never sees half of it, and appending to one that several test processes share.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Write `bytes` to a temporary file beside `path`, then rename it over `path`. The folder is made if it is missing.
pub fn write_then_rename(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = sibling(path, &format!("tmp.{}.{}", std::process::id(), unique()));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

/// Add `line` to the end of `path`: under a lock file every writer takes, the file is read, the line added, and
/// the whole written back by [`write_then_rename`]. Parallel tests and parallel test binaries each land their line;
/// a reader sees the file before or after a line, never half of one. A lock older than `STALE` (a writer that died
/// holding it) is taken over.
pub fn append_line(path: &Path, line: &str) -> std::io::Result<()> {
    const STALE: Duration = Duration::from_secs(30);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let lock = sibling(path, "lock");
    let started = SystemTime::now();
    loop {
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&lock) {
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let age = std::fs::metadata(&lock).and_then(|m| m.modified()).ok().and_then(|m| m.elapsed().ok());
                if age.is_some_and(|age| age > STALE) {
                    // the writer that held it is gone; its lock is ours now
                    std::fs::write(&lock, b"")?;
                    break;
                }
                if started.elapsed().unwrap_or_default() > STALE * 2 {
                    return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, format!("{} stayed locked", path.display())));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => return Err(e),
        }
    }
    let result = (|| {
        let mut text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e),
        };
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(line);
        text.push('\n');
        write_then_rename(path, text.as_bytes())
    })();
    // releasing the lock is the lock's own file going away
    let released = std::fs::remove_file(&lock);
    result.and(released)
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(format!(".{suffix}"));
    path.with_file_name(name)
}

fn unique() -> u128 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    nanos ^ u128::from(N.fetch_add(1, Ordering::Relaxed))
}

/// A fresh folder for one unit test, under the system's temporary folder. Never removed by the tests.
#[cfg(test)]
pub(crate) fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("maintenance_core_tests").join(format!("{name}-{}-{}", std::process::id(), unique()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parallel_appends_each_land_once() {
        let file = scratch_dir("append").join("run.jsonl");
        let threads: Vec<_> = (0..16)
            .map(|i| {
                let file = file.clone();
                std::thread::spawn(move || append_line(&file, &format!("line {i}")).unwrap())
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let text = std::fs::read_to_string(&file).unwrap();
        let mut lines: Vec<&str> = text.lines().collect();
        lines.sort_unstable();
        let mut want: Vec<String> = (0..16).map(|i| format!("line {i}")).collect();
        want.sort_unstable();
        assert_eq!(lines, want);
        assert!(!file.with_file_name("run.jsonl.lock").exists(), "the lock is released");
    }

    #[test]
    fn write_then_rename_replaces_whole() {
        let file = scratch_dir("rename").join("deep/er/f.json");
        write_then_rename(&file, b"one").unwrap();
        write_then_rename(&file, b"two").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "two");
        let leftovers: Vec<_> = std::fs::read_dir(file.parent().unwrap()).unwrap().collect();
        assert_eq!(leftovers.len(), 1, "no temporary file is left beside it");
    }
}
