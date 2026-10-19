use maintenance_macros::timed_test;

#[timed_test(max = "2 hours")]
fn slow() {}

fn main() {}
