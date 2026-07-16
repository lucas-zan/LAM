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

pub struct KeychainCredentialService<B: KeychainBackend> {
    backend: Arc<B>,
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
        }
    }
}

impl<B: KeychainBackend> KeychainCredentialService<B> {
    pub fn new(backend: Arc<B>) -> Self {
        Self { backend }
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
        let secret = self
            .backend
            .read(reference)
            .map_err(sanitize_keychain_error)?;
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
