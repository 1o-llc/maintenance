use maintenance::timed_test;

#[timed_test(max = "1s")]
async fn slow() {}

fn main() {}
