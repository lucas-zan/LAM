use super::error::{AppError, Result};
use super::provider_auth_command::ApprovedAuthCommand;
use super::provider_credentials::SecretValue;
use super::provider_credentials::{CredentialSource, DirectCodexAuth, UpstreamAuth};
use super::provider_v2::ProviderRepository;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

pub const REMOTE_PROVIDER_KEYCHAIN_SERVICE: &str = "lam.remote-provider";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KeychainCredentialReference {
    pub service: String,
    pub account: String,
    pub version: u64,
}

impl KeychainCredentialReference {
    pub fn new(credential_id: &str, version: u64) -> Result<Self> {
        let valid_id = !credential_id.is_empty()
            && credential_id.len() <= 128
            && credential_id
                .chars()
                .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_'));
        if !valid_id || version == 0 {
            return Err(invalid_reference());
        }
        Self::from_parts(
            REMOTE_PROVIDER_KEYCHAIN_SERVICE,
            &format!("credential/{credential_id}/v{version}"),
            version,
        )
    }

    pub fn from_parts(service: &str, account: &str, version: u64) -> Result<Self> {
        let expected_suffix = format!("/v{version}");
        if service != REMOTE_PROVIDER_KEYCHAIN_SERVICE
            || version == 0
            || account.len() > 256
            || !account.starts_with("credential/")
            || !account.ends_with(&expected_suffix)
            || account.chars().any(char::is_control)
        {
            return Err(invalid_reference());
        }
        Ok(Self {
            service: service.into(),
            account: account.into(),
            version,
        })
    }

    pub fn to_credential_source(&self) -> CredentialSource {
        CredentialSource::Keychain {
            service: self.service.clone(),
            account: self.account.clone(),
            version: self.version,
        }
    }
}

impl TryFrom<&CredentialSource> for KeychainCredentialReference {
    type Error = AppError;

    fn try_from(value: &CredentialSource) -> Result<Self> {
        match value {
            CredentialSource::Keychain {
                service,
                account,
                version,
            } => Self::from_parts(service, account, *version),
            _ => Err(invalid_reference()),
        }
    }
}

pub trait KeychainBackend: Send + Sync {
    fn write(&self, reference: &KeychainCredentialReference, secret: &SecretValue) -> Result<()>;
    fn read(&self, reference: &KeychainCredentialReference) -> Result<SecretValue>;
    fn delete(&self, reference: &KeychainCredentialReference) -> Result<()>;
}

pub struct SystemKeychainBackend;

#[cfg(target_os = "macos")]
impl KeychainBackend for SystemKeychainBackend {
    fn write(&self, reference: &KeychainCredentialReference, secret: &SecretValue) -> Result<()> {
        use security_framework::passwords::set_generic_password;
        secret
            .with_exposed(|value| {
                set_generic_password(&reference.service, &reference.account, value.as_bytes())
            })
            .map_err(map_security_framework_error)
    }

    fn read(&self, reference: &KeychainCredentialReference) -> Result<SecretValue> {
        use security_framework::passwords::get_generic_password;
        let bytes = get_generic_password(&reference.service, &reference.account)
            .map_err(map_security_framework_error)?;
        let value = String::from_utf8(bytes).map_err(|_| {
            AppError::new(
                "KEYCHAIN_VALUE_ENCODING",
                "Keychain credential is not valid UTF-8",
            )
        })?;
        Ok(SecretValue::from_sensitive(value))
    }

    fn delete(&self, reference: &KeychainCredentialReference) -> Result<()> {
        use security_framework::passwords::delete_generic_password;
        delete_generic_password(&reference.service, &reference.account)
            .map_err(map_security_framework_error)
    }
}

#[cfg(target_os = "macos")]
fn map_security_framework_error(error: security_framework::base::Error) -> AppError {
    let code = match error.code() {
        -25300 => "KEYCHAIN_ITEM_NOT_FOUND",
        -25293 => "KEYCHAIN_PERMISSION_DENIED",
        _ => "KEYCHAIN_UNAVAILABLE",
    };
    AppError::new(code, "macOS Keychain operation failed [REDACTED]")
}

#[cfg(not(target_os = "macos"))]
impl KeychainBackend for SystemKeychainBackend {
    fn write(&self, _reference: &KeychainCredentialReference, _secret: &SecretValue) -> Result<()> {
        Err(unsupported_platform())
    }
    fn read(&self, _reference: &KeychainCredentialReference) -> Result<SecretValue> {
        Err(unsupported_platform())
    }
    fn delete(&self, _reference: &KeychainCredentialReference) -> Result<()> {
        Err(unsupported_platform())
    }
}

#[cfg(not(target_os = "macos"))]
fn unsupported_platform() -> AppError {
    AppError::new(
        "UNSUPPORTED_REMOTE_PROVIDER_PLATFORM",
        "versioned Keychain credentials are supported only on the exact-tested macOS platform",
    )
}

pub fn codex_auth_for_keychain(
    reference: &KeychainCredentialReference,
    helper: &ApprovedAuthCommand,
) -> Result<DirectCodexAuth> {
    KeychainCredentialReference::from_parts(
        &reference.service,
        &reference.account,
        reference.version,
    )?;
    let mut args = helper.spec().args.clone();
    args.extend([
        "keychain-token".into(),
        "--service".into(),
        reference.service.clone(),
        "--account".into(),
        reference.account.clone(),
        "--version".into(),
        reference.version.to_string(),
    ]);
    Ok(DirectCodexAuth::AuthCommand {
        approval_id: helper.approval_fingerprint().into(),
        command: helper.spec().executable.to_string_lossy().into_owned(),
        args,
    })
}

/// Extracts the `credential/<id>` portion of a keychain account
/// (`credential/<uuid>/v<version>`) for use as the plaintext file key.
pub fn plaintext_credential_key(account: &str) -> Option<String> {
    let trimmed = account.strip_prefix("credential/")?;
    let id = trimmed.split('/').next()?;
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

pub struct KeychainCredentialService<B: KeychainBackend> {
    backend: Arc<B>,
    /// Optional plaintext credential file. When set, provider credentials
    /// (`lam.remote-provider`) are read/written through this file instead of
    /// the macOS Keychain, avoiding repeated authorization prompts under
    /// ad-hoc signing. Non-provider keychain entries (install identity, etc.)
    /// always use the Keychain backend.
    plaintext_path: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCredentialSnapshot {
    pub store_revision: u64,
    pub source: CredentialSource,
}

pub trait ProviderCredentialMetadata {
    fn credential_snapshot(&self, provider_id: &str) -> Result<ProviderCredentialSnapshot>;
    fn compare_and_swap_credential(
        &self,
        expected_revision: u64,
        provider_id: &str,
        expected_source: &CredentialSource,
        replacement: CredentialSource,
        now: &str,
    ) -> Result<u64>;
}

impl ProviderCredentialMetadata for ProviderRepository {
    fn credential_snapshot(&self, provider_id: &str) -> Result<ProviderCredentialSnapshot> {
        let snapshot = self.load()?;
        let provider = snapshot
            .value
            .providers
            .iter()
            .find(|item| item.id == provider_id)
            .ok_or_else(|| AppError::new("PROVIDER_NOT_FOUND", provider_id))?;
        let source = match &provider.upstream_auth {
            UpstreamAuth::Bearer { source } | UpstreamAuth::Header { source, .. } => source.clone(),
            UpstreamAuth::None => {
                return Err(AppError::new(
                    "PROVIDER_AUTH_ROUTE_UNSUPPORTED",
                    "unauthenticated Provider has no credential reference",
                ))
            }
        };
        Ok(ProviderCredentialSnapshot {
            store_revision: snapshot.revision,
            source,
        })
    }

    fn compare_and_swap_credential(
        &self,
        expected_revision: u64,
        provider_id: &str,
        expected_source: &CredentialSource,
        replacement: CredentialSource,
        now: &str,
    ) -> Result<u64> {
        Ok(self
            .replace_credential_source(
                expected_revision,
                provider_id,
                expected_source,
                replacement,
                now,
            )?
            .revision)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialRotationOutcome {
    pub reference: KeychainCredentialReference,
    pub provider_store_revision: u64,
    pub cleanup_pending: bool,
}

impl<B: KeychainBackend> Clone for KeychainCredentialService<B> {
    fn clone(&self) -> Self {
        Self {
            backend: self.backend.clone(),
            plaintext_path: self.plaintext_path.clone(),
        }
    }
}

impl<B: KeychainBackend> KeychainCredentialService<B> {
    pub fn new(backend: Arc<B>) -> Self {
        Self {
            backend,
            plaintext_path: None,
        }
    }

    /// Creates a service that routes `lam.remote-provider` credentials through
    /// a plaintext file at `plaintext_path` (with Keychain fallback on first
    /// read so existing secrets migrate automatically).
    pub fn new_with_plaintext(backend: Arc<B>, plaintext_path: std::path::PathBuf) -> Self {
        Self {
            backend,
            plaintext_path: Some(plaintext_path),
        }
    }

    fn plaintext_store(&self) -> Option<super::credential_store::JsonFileStore> {
        self.plaintext_path
            .as_ref()
            .map(|path| super::credential_store::JsonFileStore::new(path.clone()))
    }

    fn uses_plaintext(&self, reference: &KeychainCredentialReference) -> bool {
        self.plaintext_path.is_some() && reference.service == REMOTE_PROVIDER_KEYCHAIN_SERVICE
    }

    pub fn write_exact(
        &self,
        reference: &KeychainCredentialReference,
        secret: SecretValue,
    ) -> Result<()> {
        KeychainCredentialReference::from_parts(
            &reference.service,
            &reference.account,
            reference.version,
        )?;
        if secret.with_exposed(|value| value.trim().is_empty()) {
            return Err(AppError::new("PROVIDER_SECRET_EMPTY", "secret is empty"));
        }
        if self.uses_plaintext(reference) {
            let store = self.plaintext_store().expect("plaintext store exists");
            let key = plaintext_credential_key(&reference.account).ok_or_else(invalid_reference)?;
            let value = secret.with_exposed(str::to_owned);
            return store.insert(&key, &value);
        }
        self.backend
            .write(reference, &secret)
            .map_err(sanitize_keychain_error)
    }

    pub fn with_secret<R>(
        &self,
        reference: &KeychainCredentialReference,
        operation: impl FnOnce(&str) -> R,
    ) -> Result<R> {
        KeychainCredentialReference::from_parts(
            &reference.service,
            &reference.account,
            reference.version,
        )?;
        let secret = if self.uses_plaintext(reference) {
            let store = self.plaintext_store().expect("plaintext store exists");
            let key = plaintext_credential_key(&reference.account).ok_or_else(invalid_reference)?;
            match store.get(&key)? {
                Some(value) => SecretValue::from_sensitive(value),
                None => {
                    // First read after migration: fall back to Keychain and
                    // persist into the plaintext file so subsequent reads
                    // never touch the Keychain again.
                    let secret = self
                        .backend
                        .read(reference)
                        .map_err(sanitize_keychain_error)?;
                    let value = secret.with_exposed(str::to_owned);
                    store.insert(&key, &value)?;
                    secret
                }
            }
        } else {
            self.backend
                .read(reference)
                .map_err(sanitize_keychain_error)?
        };
        if secret.with_exposed(|value| value.trim().is_empty()) {
            return Err(AppError::new(
                "PROVIDER_SECRET_EMPTY",
                "Keychain item is empty",
            ));
        }
        Ok(secret.with_exposed(operation))
    }

    pub fn revoke(&self, reference: &KeychainCredentialReference) -> Result<()> {
        KeychainCredentialReference::from_parts(
            &reference.service,
            &reference.account,
            reference.version,
        )?;
        if self.uses_plaintext(reference) {
            let store = self.plaintext_store().expect("plaintext store exists");
            let key = plaintext_credential_key(&reference.account).ok_or_else(invalid_reference)?;
            return store.remove(&key);
        }
        match self.backend.delete(reference) {
            Ok(()) => Ok(()),
            Err(error) if error.code == "KEYCHAIN_ITEM_NOT_FOUND" => Ok(()),
            Err(error) => Err(sanitize_keychain_error(error)),
        }
    }

    pub fn rotate_provider<M: ProviderCredentialMetadata>(
        &self,
        metadata: &M,
        expected_store_revision: u64,
        provider_id: &str,
        expected_source: &CredentialSource,
        secret: SecretValue,
        now: &str,
    ) -> Result<CredentialRotationOutcome> {
        if secret.with_exposed(|value| value.trim().is_empty()) {
            return Err(AppError::new("PROVIDER_SECRET_EMPTY", "secret is empty"));
        }
        let snapshot = metadata.credential_snapshot(provider_id)?;
        if snapshot.store_revision != expected_store_revision {
            return Err(AppError::new(
                "STORE_REVISION_CONFLICT",
                "Provider store revision changed",
            ));
        }
        if &snapshot.source != expected_source {
            return Err(AppError::new(
                "PROVIDER_CREDENTIAL_CONFLICT",
                "Provider credential reference changed",
            ));
        }
        let old_reference = KeychainCredentialReference::try_from(expected_source).ok();
        let version = match &old_reference {
            Some(reference) => reference.version.checked_add(1).ok_or_else(|| {
                AppError::new("KEYCHAIN_VERSION_OVERFLOW", "credential version overflow")
            })?,
            None => 1,
        };
        let credential_id = Uuid::new_v4().to_string();
        let new_reference = KeychainCredentialReference::new(&credential_id, version)?;
        self.write_exact(&new_reference, secret)?;
        let provider_store_revision = match metadata.compare_and_swap_credential(
            expected_store_revision,
            provider_id,
            expected_source,
            new_reference.to_credential_source(),
            now,
        ) {
            Ok(revision) => revision,
            Err(error) => {
                if self.revoke(&new_reference).is_err() {
                    return Err(AppError::new(
                        "KEYCHAIN_COMPENSATION_FAILED",
                        "new credential metadata failed and cleanup is pending",
                    ));
                }
                return Err(error);
            }
        };
        let cleanup_pending = old_reference
            .as_ref()
            .is_some_and(|reference| self.revoke(reference).is_err());
        Ok(CredentialRotationOutcome {
            reference: new_reference,
            provider_store_revision,
            cleanup_pending,
        })
    }
}

impl KeychainCredentialService<SystemKeychainBackend> {
    /// Creates a service backed by the system Keychain with provider
    /// credentials routed through the canonical plaintext file under
    /// `provider_hub_root`. This is the single construction entry point for
    /// provider credential storage; callers must not assemble the plaintext
    /// path themselves.
    pub fn system_with_plaintext(provider_hub_root: &std::path::Path) -> Self {
        Self::new_with_plaintext(
            Arc::new(SystemKeychainBackend),
            provider_hub_root.join("provider-credentials.json"),
        )
    }
}

fn invalid_reference() -> AppError {
    AppError::new(
        "KEYCHAIN_REFERENCE_INVALID",
        "Keychain credential reference is invalid",
    )
}

fn sanitize_keychain_error(error: AppError) -> AppError {
    AppError {
        code: error.code,
        message: "Keychain operation failed [REDACTED]".into(),
        recoverable: error.recoverable,
        details: None,
    }
}
