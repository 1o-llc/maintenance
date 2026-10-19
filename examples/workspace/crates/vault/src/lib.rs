//! The vault: the workspace's most sensitive code, marked.

use maintenance::{
    boundary, boundary_here, deprecated_after, mechanism, owner, reads_env, requires_review, sensitivity, sensitivity_levels, supersedes,
    wire_writer,
};

// only the api crate may use the vault
boundary_here!(allowed = ["api"]);

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
#[mechanism("vault opening")]
#[supersedes("crate::legacy_open")]
pub fn open_vault() -> Vault {
    Vault { key: MasterKey::load() }
}

/// An open vault.
pub struct Vault {
    key: MasterKey,
}

#[sensitivity(secret)]
impl Vault {
    pub fn open() -> Self {
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

/// Sealed internals: nothing outside the vault crate uses them.
#[boundary(allowed = [])]
pub mod sealed {
    pub fn inner() -> u8 {
        1
    }
}

/// How long to wait before trying the vault again.
#[mechanism("retry policy")]
pub fn backoff(attempt: u32) -> u64 {
    100 * u64::from(attempt)
}

/// Where the vault lives on disk.
#[reads_env("VAULT_PATH")]
pub fn vault_path() -> String {
    std::env::var("VAULT_PATH").unwrap_or_else(|_| "/var/vault".into())
}

/// Whether to log every unseal: read here, declared nowhere.
#[reads_env("VAULT_DEBUG")]
pub fn debug_unseal() -> bool {
    std::env::var("VAULT_DEBUG").is_ok()
}

/// One record as the bytes the replicas read.
#[wire_writer]
pub fn encode_record(id: u32, body: &[u8]) -> Vec<u8> {
    let mut out = id.to_be_bytes().to_vec();
    out.extend_from_slice(body);
    out
}

/// The file header the replicas read: nothing pins its bytes yet.
#[wire_writer]
pub fn encode_header() -> Vec<u8> {
    b"VLT1".to_vec()
}

/// The first sealing format; nothing uses it any more.
pub fn seal_v1(data: &[u8]) -> Vec<u8> {
    data.to_vec()
}

/// The sealing format that replaced v1.
#[supersedes(crate::seal_v1)]
pub fn seal_v2(data: &[u8]) -> Vec<u8> {
    data.iter().rev().copied().collect()
}
