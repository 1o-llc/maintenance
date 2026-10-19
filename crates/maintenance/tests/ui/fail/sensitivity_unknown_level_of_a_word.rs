use maintenance::{sensitivity, sensitivity_levels};

sensitivity_levels! {
    pub TradeLevels { public = 0, trade_secret = 1 }
}

#[sensitivity(crown_jewel, levels = crate::TradeLevels)]
pub fn open() {}

fn main() {}
