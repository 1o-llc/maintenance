//! The vault: the workspace's most sensitive code, marked.

use maintenance_macros::{deprecated_after, owner, requires_review, sensitivity, sensitivity_levels};

// the workspace's level word: the shipped five, and one above them
sensitivity_levels! {
    pub VaultLevels {
        public = 0,
        internal = 1,
        confidential = 2,
        secret = 3,
        key_material = 4,
        root_of_trust = 5,
    }
}

/// The key everything else is sealed under.
#[sensitivity(key_material)]
#[owner("platform")]
pub struct MasterKey([u8; 32]);

impl MasterKey {
    #[sensitivity(key_material)]
    pub fn load() -> Self {
        MasterKey([7; 32])
    }

    pub fn fingerprint(&self) -> u8 {
        self.0[0]
    }
}

/// Open the vault for one session.
#[sensitivity(secret)]
#[requires_review(reason = "opens the key store")]
pub fn open_vault() -> Vault {
    Vault { key: MasterKey::load() }
}

/// An open vault.
pub struct Vault {
    key: MasterKey,
}

#[sensitivity(secret)]
impl Vault {
    pub fn new() -> Self {
        open_vault()
    }

    pub fn unseal(&self) -> u8 {
        self.key.fingerprint()
    }
}

/// Check a password against the vault.
#[sensitivity(secret)]
pub fn check_password(password: &str) -> bool {
    password.len() > 8 && open_vault().unseal() == 7
}

/// The vault's own audit trail.
#[sensitivity(internal)]
pub fn audit_log() -> Vec<String> {
    vec!["opened".into()]
}

/// The hardware root the master key is derived from.
#[sensitivity(root_of_trust, levels = crate::VaultLevels)]
pub fn root_seed() -> u64 {
    42
}

/// The old way in, kept for one release.
#[deprecated_after("2999-01-01", reason = "use open_vault")]
#[sensitivity(secret)]
pub fn legacy_open() -> Vault {
    open_vault()
}

#[owner("platform")]
#[sensitivity(confidential)]
pub mod store {
    /// Where sealed records live.
    pub fn records() -> usize {
        crate::audit_log().len()
    }
}
