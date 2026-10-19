use maintenance_macros::timed_test;

#[timed_test(class = "nightly")]
fn slow() {}

fn main() {}
