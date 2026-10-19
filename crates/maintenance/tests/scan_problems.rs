//! What a scan reports about the marks themselves: written into a workspace made for the test.

use std::path::{Path, PathBuf};

use maintenance::check::Workspace;

/// A one-crate workspace under the system's temporary folder (never removed by the test).
fn workspace(name: &str, lib: &str, toml: &str) -> PathBuf {
    let root = std::env::temp_dir().join("maintenance_scan_tests").join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n").unwrap();
    std::fs::write(root.join("src/lib.rs"), lib).unwrap();
    std::fs::write(root.join("maintenance.toml"), toml).unwrap();
    root
}

fn codes(found: &[maintenance::check::Finding]) -> Vec<&str> {
    found.iter().map(|f| f.code.as_str()).collect()
}

#[test]
fn a_mark_that_does_not_read_two_level_tables_and_a_bad_file_are_reported() {
    let root = workspace(
        "problems",
        r#"
maintenance::sensitivity_levels! { pub A { low = 0, high = 1 } }
maintenance::sensitivity_levels! { pub B { low = 0, top = 1 } }

#[owner]
pub fn nobody() {}

#[sensitivity(cosmic)]
pub fn far() {}

#[supersedes(crate::already_gone)]
pub fn replacement() {}
"#,
        "[sensitivity]\nunmarkd = \"low\"\n",
    );
    let ws = Workspace::scan(&root).unwrap();
    assert_eq!(codes(ws.problems()), ["MAINT6004", "MAINT6006", "MAINT6003"]);
    assert!(ws.problems()[1].message.starts_with("[MAINT6006] a mark in the source does not read: [MAINT4001] "), "{}", ws.problems()[1].message);
    assert_eq!(ws.problems()[1].line, Some(5));
    assert_eq!(ws.problems()[2].message, "[MAINT6003] the workspace declares two different level tables; it has one: A at src/lib.rs:2 and B at src/lib.rs:3");
    assert_eq!(codes(&ws.sensitivity().findings), ["MAINT6002"], "cosmic is not a level of A");
    let sup = ws.supersedes();
    assert_eq!(codes(&sup.notes), ["MAINT6032"]);
    assert!(sup.notes[0].message.ends_with("probe::replacement supersedes probe::already_gone"));
    let free = maintenance::check::sensitivity(&root).unwrap();
    assert_eq!(codes(&free.findings), ["MAINT6003", "MAINT6004", "MAINT6006", "MAINT6002"], "the free function's report holds the scan's problems too, by place");
}

#[test]
fn an_unmarked_level_outside_the_table_is_refused_and_a_missing_workspace_is_an_error() {
    let root = workspace("unmarked", "pub fn f() {}\n", "[sensitivity]\nunmarked = \"nobody\"\n");
    let report = Workspace::scan(&root).unwrap().sensitivity();
    assert_eq!(codes(&report.findings), ["MAINT6004"]);
    assert!(report.findings[0].message.ends_with("[sensitivity] unmarked = \"nobody\"; the levels are public, internal, confidential, secret, key_material"));
    let err = Workspace::scan(Path::new("/no/such/workspace")).err().unwrap();
    assert!(err.to_string().starts_with("[MAINT6005] the workspace could not be read: /no/such/workspace"), "{err}");
}

#[test]
fn a_golden_naming_a_writer_that_is_not_one_is_refused() {
    let root = workspace(
        "golden",
        r#"
pub fn plain() -> Vec<u8> { vec![1] }

#[wire_writer]
pub fn bytes() -> Vec<u8> { vec![2] }

#[golden(writer = crate::plain)]
pub fn pins_plain() {}

#[golden(writer = "probe::bytes")]
pub fn pins_bytes() {}
"#,
        "",
    );
    let report = Workspace::scan(&root).unwrap().golden();
    assert_eq!(codes(&report.findings), ["MAINT6051"]);
    assert_eq!(report.findings[0].message, "[MAINT6051] a golden test names a writer that is not a #[wire_writer]: probe::pins_plain pins probe::plain");
    assert_eq!((report.counts["wire_writers"], report.counts["goldens"]), (1, 2));
}
