use crate::services::error::{AppError, Result};
use rand::RngCore;

const INSTALL_IDENTITY_SERVICE: &str = "dev.localagentmanager.desktop.provider-hub";
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25_300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredIdentity {
    Found(Vec<u8>),
    Missing,
}

pub trait InstallIdentityStore {
    fn load(&self, install_id: &str) -> Result<StoredIdentity>;
    fn store(&self, install_id: &str, identity: &[u8]) -> Result<()>;
}

pub fn load_or_create_install_identity<S, F>(
    store: &S,
    install_id: &str,
    generate: F,
) -> Result<[u8; 32]>
where
    S: InstallIdentityStore,
    F: FnOnce() -> [u8; 32],
{
    match store.load(install_id)? {
        StoredIdentity::Found(bytes) => validate_identity(bytes),
        StoredIdentity::Missing => {
            let identity = generate();
            store.store(install_id, &identity)?;
            Ok(identity)
        }
    }
}

pub fn load_or_create_system_install_identity(
    provider_hub_root: &std::path::Path,
    install_id: &str,
) -> Result<[u8; 32]> {
    #[cfg(debug_assertions)]
    if let Some(value) = std::env::var_os("LAM_TEST_INSTALL_IDENTITY_KEY") {
        let bytes = hex::decode(value.to_string_lossy().as_ref()).map_err(|_| {
            AppError::new(
                "GATEWAY_IDENTITY_KEY_INVALID",
                "test install identity key is invalid",
            )
        })?;
        return validate_identity(bytes);
    }
    load_or_create_install_identity(
        &SystemInstallIdentityStore::new(provider_hub_root.join("install-identity.json")),
        install_id,
        || {
            let mut identity = [0_u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut identity);
            identity
        },
    )
}

fn validate_identity(bytes: Vec<u8>) -> Result<[u8; 32]> {
    bytes.try_into().map_err(|_| {
        AppError::new(
            "GATEWAY_IDENTITY_KEY_INVALID",
            "install identity key has an invalid length",
        )
    })
}

/// Plaintext-file install identity store backed by the unified
/// `JsonFileStore`. Values are hex-encoded 32-byte keys keyed by install id.
pub struct SystemInstallIdentityStore {
    store: super::super::credential_store::JsonFileStore,
}

impl SystemInstallIdentityStore {
    pub fn new(path: std::path::PathBuf) -> Self {
        Self {
            store: super::super::credential_store::JsonFileStore::new(path),
        }
    }
}

#[cfg(target_os = "macos")]
impl InstallIdentityStore for SystemInstallIdentityStore {
    fn load(&self, install_id: &str) -> Result<StoredIdentity> {
        match self.store.get(install_id)? {
            Some(value) => {
                let bytes = hex::decode(value).map_err(|_| {
                    AppError::new(
                        "INSTALL_IDENTITY_INVALID",
                        "install identity is not valid hex",
                    )
                })?;
                Ok(StoredIdentity::Found(bytes))
            }
            None => {
                // Fall back to the legacy Keychain entry so existing installs
                // migrate transparently on first load after upgrade. The value
                // is persisted into the plaintext file immediately so every
                // later load avoids the Keychain (and its authorization
                // prompt) entirely.
                match security_framework::passwords::get_generic_password(
                    INSTALL_IDENTITY_SERVICE,
                    install_id,
                ) {
                    Ok(bytes) => {
                        self.store.insert(install_id, &hex::encode(&bytes))?;
                        Ok(StoredIdentity::Found(bytes))
                    }
                    Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => {
                        Ok(StoredIdentity::Missing)
                    }
                    Err(_) => Err(keychain_unavailable()),
                }
            }
        }
    }

    fn store(&self, install_id: &str, identity: &[u8]) -> Result<()> {
        self.store.insert(install_id, &hex::encode(identity))
    }
}

#[cfg(not(target_os = "macos"))]
impl InstallIdentityStore for SystemInstallIdentityStore {
    fn load(&self, _install_id: &str) -> Result<StoredIdentity> {
        Err(AppError::new(
            "UNSUPPORTED_REMOTE_PROVIDER_PLATFORM",
            "Gateway requires exact-tested macOS arm64",
        ))
    }

    fn store(&self, _install_id: &str, _identity: &[u8]) -> Result<()> {
        Err(AppError::new(
            "UNSUPPORTED_REMOTE_PROVIDER_PLATFORM",
            "Gateway requires exact-tested macOS arm64",
        ))
    }
}

fn keychain_unavailable() -> AppError {
    AppError::new(
        "KEYCHAIN_UNAVAILABLE",
        "install identity Keychain operation failed [REDACTED]",
    )
}
