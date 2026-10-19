use maintenance_macros::wire_writer;

#[wire_writer(fast)]
pub fn encode() -> Vec<u8> {
    vec![1]
}

fn main() {}
