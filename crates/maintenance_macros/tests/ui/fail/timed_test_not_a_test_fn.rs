use maintenance_macros::timed_test;

#[timed_test(max = "1s")]
fn takes_an_argument(n: u32) {
    let _ = n;
}

fn main() {}
