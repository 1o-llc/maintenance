use maintenance::boundary;

#[boundary(allowed = ["api"])]
pub fn open() {}

fn main() {}
