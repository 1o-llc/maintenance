use maintenance_macros::deprecated_after;

#[deprecated_after("2026-02-30", reason = "use v2")]
pub fn v1() {}

fn main() {}
