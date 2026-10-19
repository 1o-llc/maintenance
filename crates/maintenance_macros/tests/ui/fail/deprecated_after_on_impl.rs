use maintenance_macros::deprecated_after;

pub struct Vault;

#[deprecated_after("2999-01-01", reason = "use Vault2")]
impl Vault {}

fn main() {}
