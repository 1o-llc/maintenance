use maintenance::timed_test;

#[timed_test(maks = "2s")]
fn slow() {}

fn main() {}
