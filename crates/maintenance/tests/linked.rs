//! What the attributes submit into the binary they are linked into: `registry::linked()`.

use maintenance::registry::{linked, Marked};
use maintenance::{mechanism, owner, reads_env, requires_review, sensitivity, sensitivity_levels, timed_test, wire_writer};

sensitivity_levels! {
    pub TradeLevels { public = 0, internal = 1, trade_secret = 7 }
}

#[sensitivity(secret)]
#[owner("platform")]
pub struct Key;

#[sensitivity(key_material)]
impl Key {
    #[requires_review(reason = "opens it")]
    pub fn open(&self) -> u8 {
        1
    }
}

#[sensitivity(trade_secret, levels = crate::TradeLevels)]
#[mechanism("recipe")]
pub fn recipe() -> u8 {
    2
}

#[reads_env("HOME")]
#[wire_writer]
pub fn bytes() -> Vec<u8> {
    vec![3]
}

fn marks_of(name: &str) -> Vec<Marked> {
    let mut found: Vec<Marked> = linked().filter(|m| m.name == name && m.kind != "impl").map(|m| m.what).collect();
    found.sort_by_key(|m| format!("{m:?}"));
    found
}

#[test]
fn every_mark_is_in_the_binary_with_its_place() {
    // a fn's marks are submitted from its body, so they are linked once it is
    assert_eq!((Key.open(), recipe(), bytes()), (1, 2, vec![3]));
    assert_eq!(marks_of("Key"), [Marked::Owner { owner: "platform" }, Marked::Sensitivity { level: "secret", rank: 3 }]);
    assert_eq!(marks_of("open"), [Marked::Review { reason: "opens it" }]);
    assert_eq!(marks_of("recipe"), [Marked::Mechanism { name: "recipe" }, Marked::Sensitivity { level: "trade_secret", rank: 7 }]);
    assert_eq!(marks_of("bytes"), [Marked::ReadsEnv { keys: &["HOME"] }, Marked::WireWriter]);
    let key = linked().find(|m| m.name == "Key" && matches!(m.what, Marked::Owner { .. })).unwrap();
    assert_eq!((key.module, key.kind, key.file, key.line), ("linked", "struct", "crates/maintenance/tests/linked.rs", 12));
    let imp = linked().find(|m| m.kind == "impl").unwrap();
    assert_eq!(imp.name, "Key");
}

#[timed_test(max = "5s")]
fn a_timed_test_is_a_test() {
    assert_eq!(recipe(), 2);
}
