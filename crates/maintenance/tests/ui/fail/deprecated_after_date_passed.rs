use maintenance::deprecated_after;

#[deprecated_after("2020-01-01", reason = "use v2")]
pub fn v1() {}

fn main() {}
