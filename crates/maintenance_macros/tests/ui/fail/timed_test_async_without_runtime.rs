use maintenance_macros::timed_test;

#[timed_test(max = "1s")]
async fn slow() {}

fn main() {}
