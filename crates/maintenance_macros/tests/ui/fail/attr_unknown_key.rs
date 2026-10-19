use maintenance_macros::timed_test;

#[timed_test(maks = "2s")]
fn slow() {}

fn main() {}
