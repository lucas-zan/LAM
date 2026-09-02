use localagentmanager_core::provider_auth_command::{approve_auth_command, AuthCommandSpec};
use localagentmanager_core::provider_config_editor::{
    apply_projection, config_hash, ConfigProjectionSpec,
};
use localagentmanager_core::provider_credentials::SecretValue;
use localagentmanager_core::provider_credentials::{CredentialSource, UpstreamAuth};
use localagentmanager_core::provider_keychain::*;
use localagentmanager_core::provider_v2::*;
use localagentmanager_core::storage::{InstallationLock, StoreOptions, VersionedFileStore};
use localagentmanager_core::{AppError, Result};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
struct FakeKeychain {
    values: Mutex<BTreeMap<String, String>>,
    fail_write: Mutex<Option<&'static str>>,
    fail_read: Mutex<Option<&'static str>>,
    fail_delete: Mutex<Option<&'static str>>,
}

impl KeychainBackend for FakeKeychain {
    fn write(&self, reference: &KeychainCredentialReference, secret: &SecretValue) -> Result<()> {
        if let Some(code) = *self.fail_write.lock().unwrap() {
            return Err(AppError::new(code, "keychain write failed [REDACTED]"));
        }
        let value = secret.with_exposed(str::to_owned);
        self.values
            .lock()
            .unwrap()
            .insert(reference.account.clone(), value);
        Ok(())
    }
    fn read(&self, reference: &KeychainCredentialReference) -> Result<SecretValue> {
        if let Some(code) = *self.fail_read.lock().unwrap() {
            return Err(AppError::new(code, "keychain read failed [REDACTED]"));
        }
        self.values
            .lock()
            .unwrap()
            .get(&reference.account)
            .cloned()
            .map(SecretValue::from_sensitive)
            .ok_or_else(|| AppError::new("KEYCHAIN_ITEM_NOT_FOUND", "keychain item is missing"))
    }
    fn delete(&self, reference: &KeychainCredentialReference) -> Result<()> {
        if let Some(code) = *self.fail_delete.lock().unwrap() {
            return Err(AppError::new(code, "keychain delete failed [REDACTED]"));
        }
        if self
            .values
            .lock()
            .unwrap()
            .remove(&reference.account)
            .is_some()
        {
            Ok(())
        } else {
            Err(AppError::new(
                "KEYCHAIN_ITEM_NOT_FOUND",
                "keychain item is missing",
            ))
        }
    }
}

fn reference() -> KeychainCredentialReference {
    KeychainCredentialReference::new("credential-test", 1).unwrap()
}

#[test]
fn credential_boundary_writes_reads_and_idempotently_revokes_exact_version() {
    let backend = Arc::new(FakeKeychain::default());
    let service = KeychainCredentialService::new(backend.clone());
    let reference = reference();
    service
        .write_exact(
            &reference,
            SecretValue::from_sensitive("boundary-secret".into()),
        )
        .unwrap();
    assert!(service
        .with_secret(&reference, |value| value == "boundary-secret")
        .unwrap());
    service.revoke(&reference).unwrap();
    service.revoke(&reference).unwrap();
    assert!(backend.values.lock().unwrap().is_empty());
}

#[test]
fn credential_boundary_rejects_empty_invalid_and_missing_without_leaks() {
    let backend = Arc::new(FakeKeychain::default());
    let service = KeychainCredentialService::new(backend);
    assert_eq!(
        service
            .write_exact(&reference(), SecretValue::from_sensitive("  ".into()))
            .unwrap_err()
            .code,
        "PROVIDER_SECRET_EMPTY"
    );
    assert_eq!(
        KeychainCredentialReference::from_parts("other.service", "account", 1)
            .unwrap_err()
            .code,
        "KEYCHAIN_REFERENCE_INVALID"
    );
    assert_eq!(
        service
            .with_secret(&reference(), |_| true)
            .unwrap_err()
            .code,
        "KEYCHAIN_ITEM_NOT_FOUND"
    );
}

#[test]
fn credential_boundary_preserves_unavailable_and_denied_codes_and_redacts_marker() {
    let marker = "LAM_TEST_SECRET_KEYCHAIN_sk-rpg108";
    let backend = Arc::new(FakeKeychain::default());
    let service = KeychainCredentialService::new(backend.clone());
    *backend.fail_write.lock().unwrap() = Some("KEYCHAIN_UNAVAILABLE");
    let error = service
        .write_exact(&reference(), SecretValue::from_sensitive(marker.into()))
        .unwrap_err();
    assert_eq!(error.code, "KEYCHAIN_UNAVAILABLE");
    assert!(!format!("{error:?}").contains(marker));
    *backend.fail_write.lock().unwrap() = None;
    service
        .write_exact(&reference(), SecretValue::from_sensitive(marker.into()))
        .unwrap();
    *backend.fail_read.lock().unwrap() = Some("KEYCHAIN_PERMISSION_DENIED");
    assert_eq!(
        service
            .with_secret(&reference(), |_| true)
            .unwrap_err()
            .code,
        "KEYCHAIN_PERMISSION_DENIED"
    );
    assert!(!serde_json::to_string(&reference())
        .unwrap()
        .contains(marker));
}

fn provider_repo(root: &tempfile::TempDir) -> ProviderRepository {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    ProviderRepository::new(VersionedFileStore::new(
        root.path().join("providers.json"),
        InstallationLock::new(
            root.path().join("provider-hub.lock"),
            Duration::from_secs(1),
        ),
        1,
        StoreOptions::default(),
    ))
}

fn provider_input(source: CredentialSource) -> ProviderInput {
    ProviderInput {
        id: "provider-a".into(),
        name: "Provider A".into(),
        protocol: ProviderProtocol::Responses,
        base_url: "https://api.example.test/v1".into(),
        default_model: "model-a".into(),
        models: vec![ProviderModel {
            id: "model-a".into(),
            label: "A".into(),
            capabilities: None,
            context_window: None,
        }],
        upstream_auth: UpstreamAuth::Bearer { source },
        adapter: AdapterConfig::None,
        compatibility_profile: None,
        codex: CodexProviderOptions::default(),
    }
}

#[test]
fn metadata_lifecycle_creates_and_rotates_monotonic_versions_before_old_cleanup() {
    let root = tempfile::tempdir().unwrap();
    let repo = provider_repo(&root);
    let env = CredentialSource::Env {
        env_key: "PROVIDER_TOKEN".into(),
    };
    repo.create(0, provider_input(env.clone()), "2026-07-13T00:00:00Z")
        .unwrap();
    let backend = Arc::new(FakeKeychain::default());
    let service = KeychainCredentialService::new(backend.clone());
    let first = service
        .rotate_provider(
            &repo,
            1,
            "provider-a",
            &env,
            SecretValue::from_sensitive("first-secret".into()),
            "2026-07-13T00:01:00Z",
        )
        .unwrap();
    assert_eq!(first.reference.version, 1);
    assert!(!first.cleanup_pending);
    let first_source = first.reference.to_credential_source();
    let second = service
        .rotate_provider(
            &repo,
            2,
            "provider-a",
            &first_source,
            SecretValue::from_sensitive("second-secret".into()),
            "2026-07-13T00:02:00Z",
        )
        .unwrap();
    assert_eq!(second.reference.version, 2);
    assert_ne!(second.reference.account, first.reference.account);
    assert!(!backend
        .values
        .lock()
        .unwrap()
        .contains_key(&first.reference.account));
    assert!(backend
        .values
        .lock()
        .unwrap()
        .contains_key(&second.reference.account));
    assert_eq!(
        repo.load().unwrap().value.providers[0].upstream_auth,
        UpstreamAuth::Bearer {
            source: second.reference.to_credential_source()
        }
    );
}

struct ConflictMetadata {
    source: CredentialSource,
}
impl ProviderCredentialMetadata for ConflictMetadata {
    fn credential_snapshot(&self, _provider_id: &str) -> Result<ProviderCredentialSnapshot> {
        Ok(ProviderCredentialSnapshot {
            store_revision: 4,
            source: self.source.clone(),
        })
    }
    fn compare_and_swap_credential(
        &self,
        _expected_revision: u64,
        _provider_id: &str,
        _expected_source: &CredentialSource,
        _replacement: CredentialSource,
        _now: &str,
    ) -> Result<u64> {
        Err(AppError::new(
            "STORE_REVISION_CONFLICT",
            "synthetic conflict",
        ))
    }
}

#[test]
fn metadata_lifecycle_cas_conflict_preserves_old_and_deletes_only_new_version() {
    let old = CredentialSource::Keychain {
        service: REMOTE_PROVIDER_KEYCHAIN_SERVICE.into(),
        account: "credential/stable/v1".into(),
        version: 1,
    };
    let backend = Arc::new(FakeKeychain::default());
    let service = KeychainCredentialService::new(backend.clone());
    let old_ref = KeychainCredentialReference::try_from(&old).unwrap();
    service
        .write_exact(&old_ref, SecretValue::from_sensitive("old-secret".into()))
        .unwrap();
    let metadata = ConflictMetadata {
        source: old.clone(),
    };
    assert_eq!(
        service
            .rotate_provider(
                &metadata,
                4,
                "provider-a",
                &old,
                SecretValue::from_sensitive("new-secret".into()),
                "2026-07-13T00:03:00Z"
            )
            .unwrap_err()
            .code,
        "STORE_REVISION_CONFLICT"
    );
    let values = backend.values.lock().unwrap();
    assert_eq!(values.len(), 1);
    assert!(values.contains_key(&old_ref.account));
}

#[test]
fn metadata_lifecycle_stale_precheck_writes_nothing_and_cleanup_failure_is_pending() {
    let root = tempfile::tempdir().unwrap();
    let repo = provider_repo(&root);
    let env = CredentialSource::Env {
        env_key: "PROVIDER_TOKEN".into(),
    };
    repo.create(0, provider_input(env.clone()), "2026-07-13T00:00:00Z")
        .unwrap();
    let backend = Arc::new(FakeKeychain::default());
    let service = KeychainCredentialService::new(backend.clone());
    assert_eq!(
        service
            .rotate_provider(
                &repo,
                0,
                "provider-a",
                &env,
                SecretValue::from_sensitive("never-written".into()),
                "2026-07-13T00:01:00Z"
            )
            .unwrap_err()
            .code,
        "STORE_REVISION_CONFLICT"
    );
    assert!(backend.values.lock().unwrap().is_empty());
    let first = service
        .rotate_provider(
            &repo,
            1,
            "provider-a",
            &env,
            SecretValue::from_sensitive("first".into()),
            "2026-07-13T00:02:00Z",
        )
        .unwrap();
    *backend.fail_delete.lock().unwrap() = Some("KEYCHAIN_PERMISSION_DENIED");
    let second = service
        .rotate_provider(
            &repo,
            2,
            "provider-a",
            &first.reference.to_credential_source(),
            SecretValue::from_sensitive("second".into()),
            "2026-07-13T00:03:00Z",
        )
        .unwrap();
    assert!(second.cleanup_pending);
    assert_eq!(
        repo.load().unwrap().value.providers[0].upstream_auth,
        UpstreamAuth::Bearer {
            source: second.reference.to_credential_source()
        }
    );
}

#[test]
fn metadata_lifecycle_preserves_named_header_and_write_failure_keeps_metadata() {
    let root = tempfile::tempdir().unwrap();
    let repo = provider_repo(&root);
    let env = CredentialSource::Env {
        env_key: "ANTHROPIC_TOKEN".into(),
    };
    let mut input = provider_input(env.clone());
    input.upstream_auth = UpstreamAuth::Header {
        name: "x-api-key".into(),
        source: env.clone(),
    };
    repo.create(0, input, "2026-07-13T00:00:00Z").unwrap();
    let backend = Arc::new(FakeKeychain::default());
    *backend.fail_write.lock().unwrap() = Some("KEYCHAIN_PERMISSION_DENIED");
    let service = KeychainCredentialService::new(backend.clone());
    assert_eq!(
        service
            .rotate_provider(
                &repo,
                1,
                "provider-a",
                &env,
                SecretValue::from_sensitive("denied".into()),
                "2026-07-13T00:01:00Z"
            )
            .unwrap_err()
            .code,
        "KEYCHAIN_PERMISSION_DENIED"
    );
    assert_eq!(
        repo.load().unwrap().value.providers[0].upstream_auth,
        UpstreamAuth::Header {
            name: "x-api-key".into(),
            source: env.clone()
        }
    );
    *backend.fail_write.lock().unwrap() = None;
    let outcome = service
        .rotate_provider(
            &repo,
            1,
            "provider-a",
            &env,
            SecretValue::from_sensitive("accepted".into()),
            "2026-07-13T00:02:00Z",
        )
        .unwrap();
    assert_eq!(
        repo.load().unwrap().value.providers[0].upstream_auth,
        UpstreamAuth::Header {
            name: "x-api-key".into(),
            source: outcome.reference.to_credential_source()
        }
    );
}

#[test]
fn codex_projection_uses_approved_helper_and_reference_only() {
    let reference = KeychainCredentialReference::new("opaque-reference", 3).unwrap();
    let approved = approve_auth_command(AuthCommandSpec {
        executable: PathBuf::from("/usr/bin/printf"),
        args: Vec::new(),
        cwd: None,
        timeout_ms: 500,
        max_stdout_bytes: 128,
        cache_ttl_ms: 1_000,
    })
    .unwrap();
    let auth = codex_auth_for_keychain(&reference, &approved).unwrap();
    let applied = apply_projection(
        "",
        &config_hash(b""),
        &ConfigProjectionSpec {
            provider_id: "keychain-direct".into(),
            model: "model-a".into(),
            display_name: "Keychain Direct".into(),
            base_url: "https://api.example.test/v1".into(),
            auth,
            codex: CodexProviderOptions::default(),
            gateway: false,
            model_catalog_path: "/tmp/keychain-direct/models.json".into(),
            model_context_window: None,
            model_auto_compact_token_limit: None,
            reasoning_effort: None,
        },
    )
    .unwrap();
    assert!(applied
        .contents
        .contains("[model_providers.keychain-direct.auth]"));
    assert!(applied.contents.contains("/usr/bin/printf"));
    assert!(applied.contents.contains(&reference.account));
    assert!(applied.contents.contains("lam.remote-provider"));
    assert!(!applied.contents.contains("LAM_TEST_SECRET"));
    assert!(!applied.contents.contains("env_key"));
}
