//! Every macro, on every kind of item it takes, compiling clean.

use maintenance::{
    boundary, boundary_here, deprecated_after, mechanism, owner, reads_env, requires_review, sensitivity, sensitivity_levels, supersedes, wire_writer,
};

boundary_here!(allowed = ["api"]);

sensitivity_levels! {
    pub TradeLevels { public = 0, internal = 1, trade_secret = 4 }
}

#[sensitivity(secret)]
#[owner("platform")]
#[requires_review(reason = "holds the key")]
pub struct Key([u8; 4]);

#[sensitivity(key_material)]
impl Key {
    #[mechanism("key loading")]
    pub fn load() -> Self {
        Key([1; 4])
    }

    #[sensitivity(secret)]
    pub fn first(&self) -> u8 {
        self.0[0]
    }
}

#[sensitivity(trade_secret, levels = crate::TradeLevels)]
pub fn recipe() -> &'static str {
    "salt"
}

#[sensitivity(internal, levels = "crate::TradeLevels")]
pub enum Mode {
    A,
}

#[deprecated_after("2999-01-01", reason = "use encode")]
pub fn encode_v1() -> Vec<u8> {
    vec![0]
}

#[supersedes(crate::encode_v1)]
#[wire_writer]
pub fn encode() -> Vec<u8> {
    vec![1]
}

#[reads_env("HOME", "PATH")]
pub fn home() -> Option<String> {
    std::env::var("HOME").ok()
}

#[boundary(allowed = [])]
#[owner("platform")]
pub mod sealed {
    pub const DEPTH: u8 = 3;
}

#[sensitivity(confidential)]
pub trait Store {
    fn get(&self) -> u8;
}

#[sensitivity(public)]
pub type Bytes = Vec<u8>;

#[sensitivity(internal)]
pub static LIMIT: u32 = 4;

#[cfg(test)]
mod tests {
    use super::*;
    use maintenance::{golden, timed_test};

    #[timed_test(max = "2s")]
    #[golden(writer = crate::encode)]
    fn encode_is_pinned() {
        assert_eq!(encode(), [1]);
    }
}

fn main() {
    assert_eq!(Key::load().first(), 1);
    assert_eq!(recipe(), "salt");
    assert_eq!(sealed::DEPTH, 3);
    assert!(home().is_some() || home().is_none());
    let _ = (Mode::A, LIMIT, Bytes::new());
    #[allow(deprecated)]
    let _ = encode_v1();
}
