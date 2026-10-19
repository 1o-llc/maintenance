use maintenance_macros::timed_test;

// no max, no class, and no maintenance.toml above this crate naming a default class
#[timed_test]
fn slow() {}

fn main() {}
