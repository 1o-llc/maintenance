use maintenance_macros::golden;

pub fn encode() -> Vec<u8> {
    vec![1]
}

// the writer is named as code: one that does not exist is the compiler's error, on the path
#[golden(writer = crate::encode_v2)]
fn bytes() {
    assert_eq!(encode(), [1]);
}

fn main() {
    bytes();
}
