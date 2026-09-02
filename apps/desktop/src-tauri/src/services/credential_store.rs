//! Unified private JSON file store for LAM secrets.
//!
//! Both provider credentials (`provider-credentials.json`) and the gateway
//! install identity (`install-identity.json`) are small maps of key -> value
//! stored as 0600-permission JSON files under the provider-hub directory.
//! This single implementation replaces two near-identical copies so every
//! secret path shares one atomic-write, permission-tightening code path.

use super::error::{AppError, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A private JSON map file (key -> string value) with 0600 permissions and
/// atomic writes (temp file + rename).
pub struct JsonFileStore {
    path: PathBuf,
}

impl JsonFileStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_all(&self) -> Result<BTreeMap<String, String>> {
        if !self.path.exists() {
            return Ok(BTreeMap::new());
        }
        let body = std::fs::read_to_string(&self.path).map_err(|error| {
            AppError::new(
                "SECRET_STORE_READ_FAILED",
                format!("secret store could not be read: {error}"),
            )
        })?;
        serde_json::from_str::<BTreeMap<String, String>>(&body).map_err(|error| {
            AppError::new(
                "SECRET_STORE_INVALID",
                format!("secret store is not valid JSON: {error}"),
            )
        })
    }

    pub fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.load_all()?.get(key).cloned())
    }

    pub fn insert(&self, key: &str, value: &str) -> Result<()> {
        let mut entries = self.load_all()?;
        entries.insert(key.to_string(), value.to_string());
        self.persist(&entries)
    }

    pub fn remove(&self, key: &str) -> Result<()> {
        let mut entries = self.load_all()?;
        entries.remove(key);
        self.persist(&entries)
    }

    fn persist(&self, entries: &BTreeMap<String, String>) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                AppError::new(
                    "SECRET_STORE_WRITE_FAILED",
                    format!("secret store directory could not be created: {error}"),
                )
            })?;
        }
        let body = serde_json::to_string_pretty(entries).map_err(|error| {
            AppError::new(
                "SECRET_STORE_INVALID",
                format!("secret store could not be serialized: {error}"),
            )
        })?;
        // Atomic write: temp file + rename, then tighten permissions to 0600.
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, format!("{body}\n")).map_err(|error| {
            AppError::new(
                "SECRET_STORE_WRITE_FAILED",
                format!("secret store could not be written: {error}"),
            )
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&tmp, &self.path).map_err(|error| {
            AppError::new(
                "SECRET_STORE_WRITE_FAILED",
                format!("secret store could not be committed: {error}"),
            )
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

/// Resolves the provider-hub root and exposes the canonical secret stores.
///
/// Every component (LAM GUI, gateway sidecar, auth helper, CLI) resolves the
/// same paths from the provider-hub root so a secret written by one is read
/// by all.
pub struct CredentialPaths {
    root: PathBuf,
}

impl CredentialPaths {
    pub fn for_home(home: &Path) -> Result<Self> {
        let root =
            super::provider_runtime::ProviderHubPaths::for_home(home).ensure_canonical_root()?;
        Ok(Self { root })
    }

    pub fn at_root(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Provider credentials file (`provider-credentials.json`).
    pub fn provider_credentials_store(&self) -> JsonFileStore {
        JsonFileStore::new(self.root.join("provider-credentials.json"))
    }

    /// Gateway install identity file (`install-identity.json`).
    pub fn install_identity_store(&self) -> JsonFileStore {
        JsonFileStore::new(self.root.join("install-identity.json"))
    }
}
