//! Every check against examples/workspace, as the README shows it.

use std::path::Path;

use maintenance::check::{Report, Workspace};

fn workspace() -> Workspace {
    Workspace::scan(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/workspace")).unwrap()
}

fn codes(report: &Report) -> Vec<(&str, &str, &str)> {
    report
        .findings
        .iter()
        .map(|f| (f.code.as_str(), f.item.as_deref().unwrap_or(""), f.target.as_deref().unwrap_or("")))
        .collect()
}

#[test]
fn the_example_workspace_fails_where_it_was_written_to() {
    let ws = workspace();
    assert!(ws.problems().is_empty(), "{:?}", ws.problems());
    assert_eq!(ws.levels().word, "VaultLevels");

    let s = ws.sensitivity();
    assert_eq!(
        codes(&s),
        [
            ("MAINT6001", "api::health", "vault::MasterKey::fingerprint"),
            ("MAINT6001", "api::health", "vault::MasterKey::load"),
            ("MAINT6001", "api::status", "vault::open_vault"),
            ("MAINT6001", "api::rotate", "vault::root_seed"),
            ("MAINT6001", "api::old_path", "vault::Vault::unseal"),
            ("MAINT6001", "api::old_path", "vault::legacy_open"),
            ("MAINT6001", "cli::main", "vault::audit_log"),
            ("MAINT6001", "vault::open_vault", "vault::MasterKey::load"),
            ("MAINT6001", "vault::Vault::unseal", "vault::MasterKey::fingerprint"),
        ]
    );
    assert_eq!(s.notes.len(), 1, "api::login reaching check_password is allowed by maintenance.toml");
    assert!(s.notes[0].message.ends_with("(allowed by maintenance.toml)"));

    assert_eq!(codes(&ws.mechanism()), [("MAINT6010", "vault::backoff", "api::retry_delay")]);
    assert_eq!(
        codes(&ws.boundary()),
        [("MAINT6020", "cli", "vault"), ("MAINT6020", "api::peek", "vault::sealed::inner"), ("MAINT6020", "cli::main", "vault::audit_log")]
    );
    let sup = ws.supersedes();
    assert!(sup.passed);
    let notes: Vec<&str> = sup.notes.iter().map(|n| n.code.as_str()).collect();
    assert_eq!(notes, ["MAINT6030", "MAINT6031"]);
    assert!(sup.notes[0].message.contains("still used by 1: api::old_path"));
    assert_eq!(codes(&ws.reads_env_configured()), [("MAINT6040", "vault::debug_unseal", "")]);
    assert_eq!(codes(&ws.golden()), [("MAINT6050", "vault::encode_header", "")]);
}

#[test]
fn the_registry_holds_every_marked_and_inheriting_item() {
    let reg = workspace().registry();
    assert_eq!(reg.levels, ["public", "internal", "confidential", "secret", "key_material", "root_of_trust"]);
    let level = |id: &str| reg.get(id).and_then(|e| e.sensitivity.clone());
    assert_eq!(level("vault::Vault::unseal").as_deref(), Some("secret"), "from its impl");
    assert_eq!(reg.get("vault::Vault::unseal").unwrap().sensitivity_from.as_deref(), Some("vault::<impl Vault>"));
    assert_eq!(level("vault::MasterKey::fingerprint").as_deref(), Some("key_material"), "from its type");
    assert_eq!(level("vault::store::records").as_deref(), Some("confidential"), "from its module");
    assert_eq!(reg.get("vault::store::records").unwrap().owner.as_deref(), Some("platform"));
    assert_eq!(reg.get("vault").unwrap().boundary.as_deref(), Some(&["api".to_owned()][..]));
    assert_eq!(reg.get("vault::open_vault").unwrap().supersedes.as_deref(), Some("vault::legacy_open"));
    assert_eq!(reg.at_or_above("key_material").count(), 4);
    let golden = reg.items.iter().find(|e| e.golden_writer.is_some()).unwrap();
    assert_eq!((golden.package.as_str(), golden.golden_writer.as_deref()), ("api", Some("vault::encode_record")));
}

#[test]
fn the_free_functions_scan_and_run_one_check() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/workspace");
    let report = maintenance::check::reads_env(&root, "deploy/*.toml", "env").unwrap();
    assert_eq!(report.counts["keys_declared"], 1);
    assert_eq!(report.findings.len(), 1);
    let json = serde_json::to_string(&report).unwrap();
    assert_eq!(serde_json::from_str::<Report>(&json).unwrap(), report, "a report is data with one shape");
}
