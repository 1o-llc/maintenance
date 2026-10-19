//! The public face: what the outside world calls.

use maintenance_macros::{mechanism, owner, sensitivity};

/// Health check: unmarked, so it counts as `public`.
pub fn health() -> bool {
    vault::MasterKey::load().fingerprint() == 7
}

/// Status page.
#[sensitivity(public)]
#[owner("web")]
pub fn status() -> String {
    let v = vault::open_vault();
    format!("vault {}", v.unseal())
}

/// Log in: reaches the vault's password check, which maintenance.toml accepts.
#[sensitivity(public)]
pub fn login(password: &str) -> bool {
    vault::check_password(password)
}

/// The admin report reads the audit trail: confidential above internal, so no finding.
#[sensitivity(confidential)]
pub fn admin_report() -> usize {
    vault::audit_log().len() + vault::store::records()
}

/// Internal tooling that reaches for the root seed.
#[sensitivity(internal)]
pub fn rotate() -> u64 {
    vault::root_seed()
}

/// The old way in, still called from here.
#[allow(deprecated)]
pub fn old_path() -> u8 {
    vault::legacy_open().unseal()
}

/// The api's own retry wait: a second home for a rule the vault already holds.
#[mechanism("retry policy")]
pub fn retry_delay(attempt: u32) -> u64 {
    100 * u64::from(attempt)
}

/// Reaches into the vault's sealed module.
pub fn peek() -> u8 {
    vault::sealed::inner()
}

#[cfg(test)]
mod tests {
    use maintenance_macros::timed_test;

    #[timed_test]
    fn health_is_quick() {
        assert!(super::health());
    }

    #[timed_test(max = "1s")]
    fn status_reads_the_vault() {
        assert_eq!(super::status(), "vault 7");
    }
}
