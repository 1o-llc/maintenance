use maintenance_macros::boundary;

#[boundary(allowed = ["api"])]
pub fn open() {}

fn main() {}
