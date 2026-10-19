//! Golden bytes: what the replicas read, pinned.

use maintenance::golden;

#[golden(writer = vault::encode_record)]
#[test]
fn record_bytes_are_pinned() {
    assert_eq!(vault::encode_record(1, b"ok"), [0, 0, 0, 1, b'o', b'k']);
}
