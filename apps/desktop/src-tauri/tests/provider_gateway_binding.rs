use localagentmanager_core::gateway::binding::{
    binding_requires_gateway, GatewayBindingCollection, GatewayBindingService, GatewayTokenRequest,
};
use localagentmanager_core::provider_credentials::{CredentialSource, SecretValue, UpstreamAuth};
use localagentmanager_core::provider_keychain::{
    KeychainBackend, KeychainCredentialReference, KeychainCredentialService,
};
use localagentmanager_core::provider_v2::{
    build_provider, AdapterConfig, CodexProviderOptions, ProviderInput, ProviderModel,
    ProviderProtocol,
};
use localagentmanager_core::storage::{InstallationLock, StoreOptions, VersionedFileStore};
use localagentmanager_core::{AppError, Result};
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
struct FakeKeychain {
    values: Mutex<HashMap<String, String>>,
    fail_delete: Mutex<bool>,
}

impl FakeKeychain {
    fn key(reference: &KeychainCredentialReference) -> String {
        format!("{}:{}", reference.service, reference.account)
    }

    fn fail_deletes(&self) {
        *self.fail_delete.lock().unwrap() = true;
    }
}

impl KeychainBackend for FakeKeychain {
    fn write(&self, reference: &KeychainCredentialReference, secret: &SecretValue) -> Result<()> {
        let value = secret.with_exposed(str::to_owned);
        self.values
            .lock()
            .unwrap()
            .insert(Self::key(reference), value);
        Ok(())
    }

    fn read(&self, reference: &KeychainCredentialReference) -> Result<SecretValue> {
        self.values
            .lock()
            .unwrap()
            .get(&Self::key(reference))
            .cloned()
            .map(SecretValue::from_sensitive)
            .ok_or_else(|| AppError::new("KEYCHAIN_ITEM_NOT_FOUND", "not found"))
    }

    fn delete(&self, reference: &KeychainCredentialReference) -> Result<()> {
        if *self.fail_delete.lock().unwrap() {
            return Err(AppError::new(
                "KEYCHAIN_UNAVAILABLE",
                "synthetic secret marker",
            ));
        }
        self.values.lock().unwrap().remove(&Self::key(reference));
        Ok(())
    }
}

fn provider(id: &str, model: &str) -> localagentmanager_core::provider_v2::ProviderProfileV2 {
    build_provider(
        ProviderInput {
            id: id.into(),
            name: "DeepSeek test".into(),
            protocol: ProviderProtocol::ChatCompletions,
            base_url: "https://api.deepseek.com/v1".into(),
            default_model: model.into(),
            models: vec![ProviderModel {
                id: model.into(),
                label: "Reasoner".into(),
                capabilities: None,
            }],
            upstream_auth: UpstreamAuth::Bearer {
                source: CredentialSource::Env {
                    env_key: "DEEPSEEK_API_KEY".into(),
                },
            },
            adapter: AdapterConfig::Local {
                adapter_id: "responses_to_chat_completions".into(),
                upstream_path: "/chat/completions".into(),
            },
            compatibility_profile: Some("deepseek_chat_completions".into()),
            codex: CodexProviderOptions {
                direct_request_max_retries: Some(0),
                direct_stream_max_retries: Some(0),
                ..Default::default()
            },
        },
        "2026-07-13T01:00:00Z",
    )
    .unwrap()
}

fn service(
    root: &std::path::Path,
    backend: Arc<FakeKeychain>,
) -> GatewayBindingService<FakeKeychain> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let store = VersionedFileStore::<GatewayBindingCollection>::new(
        root.join("gateway-bindings.json"),
        InstallationLock::new(root.join("provider-hub.lock"), Duration::from_secs(1)),
        1,
        StoreOptions {
            max_bytes: 1024 * 1024,
        },
    );
    GatewayBindingService::new(store, KeychainCredentialService::new(backend))
}

fn provision(
    service: &GatewayBindingService<FakeKeychain>,
    profile: &str,
) -> localagentmanager_core::gateway::binding::GatewayBindingProvision {
    service
        .provision(
            0,
            GatewayTokenRequest {
                profile_id: profile.into(),
                provider: provider("deepseek", "deepseek-reasoner"),
                provider_revision: 7,
                selected_model: "deepseek-reasoner".into(),
                expires_at: None,
                now: "2026-07-13T01:01:00Z".into(),
            },
        )
        .unwrap()
}

#[test]
fn provision_persists_only_hash_and_reference_and_helper_reads_token() {
    let root = tempfile::tempdir().unwrap();
    let backend = Arc::new(FakeKeychain::default());
    let service = service(root.path(), backend);
    let provisioned = provision(&service, "profile-a");

    let token = service
        .token_for_helper("profile-a", &provisioned.binding_id)
        .unwrap();
    assert!(token.starts_with("lam_gw_"));
    assert!(token.len() >= 50);

    let bytes = fs::read(root.path().join("gateway-bindings.json")).unwrap();
    let body = String::from_utf8(bytes).unwrap();
    assert!(!body.contains(&token));
    assert!(body.contains("tokenHash"));
    assert!(body.contains("credentialReference"));
    assert!(!format!("{provisioned:?}").contains(&token));
}

#[test]
fn only_active_chat_completions_bindings_require_gateway() {
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path(), Arc::new(FakeKeychain::default()));
    provision(&service, "profile-a");
    let mut binding = service.load().unwrap().value.bindings.remove(0);

    assert!(binding_requires_gateway(&binding));
    binding.provider.protocol = ProviderProtocol::Responses;
    assert!(!binding_requires_gateway(&binding));
    binding.provider.protocol = ProviderProtocol::ChatCompletions;
    binding.revoked_at = Some("2026-07-17T00:00:00Z".into());
    assert!(!binding_requires_gateway(&binding));
}

#[test]
fn bearer_auth_is_strict_profile_scoped_and_returns_immutable_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path(), Arc::new(FakeKeychain::default()));
    let provisioned = provision(&service, "profile-a");
    let token = service
        .token_for_helper("profile-a", &provisioned.binding_id)
        .unwrap();

    for invalid in [
        "",
        "Bearer",
        "bearer token",
        "Bearer ",
        "Basic abc",
        "Bearer bad",
    ] {
        assert_eq!(
            service
                .authenticate(invalid, "2026-07-13T01:02:00Z")
                .unwrap_err()
                .code,
            "GATEWAY_AUTH_INVALID"
        );
    }
    assert_eq!(
        service
            .authenticate_for_profile(
                "profile-b",
                &format!("Bearer {token}"),
                "2026-07-13T01:02:00Z",
            )
            .unwrap_err()
            .code,
        "GATEWAY_AUTH_PROFILE_MISMATCH"
    );

    let snapshot = service
        .authenticate(&format!("Bearer {token}"), "2026-07-13T01:02:00Z")
        .unwrap();
    assert_eq!(snapshot.profile_id, "profile-a");
    assert_eq!(snapshot.provider.id, "deepseek");
    assert_eq!(snapshot.provider_revision, 7);
    assert_eq!(snapshot.selected_model, "deepseek-reasoner");
}

#[test]
fn rotation_immediately_rejects_old_token_but_keeps_inflight_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path(), Arc::new(FakeKeychain::default()));
    let first = provision(&service, "profile-a");
    let old_token = service
        .token_for_helper("profile-a", &first.binding_id)
        .unwrap();
    let inflight = service
        .authenticate(&format!("Bearer {old_token}"), "2026-07-13T01:02:00Z")
        .unwrap();

    let rotated = service
        .rotate(
            1,
            &first.binding_id,
            provider("deepseek", "deepseek-chat"),
            8,
            "deepseek-chat",
            "2026-07-13T01:03:00Z",
        )
        .unwrap();
    let new_token = service
        .token_for_helper("profile-a", &rotated.binding_id)
        .unwrap();

    assert_eq!(
        service
            .authenticate(&format!("Bearer {old_token}"), "2026-07-13T01:04:00Z")
            .unwrap_err()
            .code,
        "GATEWAY_AUTH_INVALID"
    );
    assert_eq!(
        service
            .authenticate(&format!("Bearer {new_token}"), "2026-07-13T01:04:00Z")
            .unwrap()
            .provider_revision,
        8
    );
    assert_eq!(inflight.provider_revision, 7);
    assert_eq!(inflight.selected_model, "deepseek-reasoner");
}

#[test]
fn revoke_and_expiry_reject_new_requests_and_helper_access() {
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path(), Arc::new(FakeKeychain::default()));
    let expiring = service
        .provision(
            0,
            GatewayTokenRequest {
                profile_id: "profile-expiring".into(),
                provider: provider("deepseek", "deepseek-chat"),
                provider_revision: 1,
                selected_model: "deepseek-chat".into(),
                expires_at: Some("2026-07-13T01:05:00Z".into()),
                now: "2026-07-13T01:00:00Z".into(),
            },
        )
        .unwrap();
    let token = service
        .token_for_helper("profile-expiring", &expiring.binding_id)
        .unwrap();
    assert_eq!(
        service
            .authenticate(&format!("Bearer {token}"), "2026-07-13T01:05:00Z")
            .unwrap_err()
            .code,
        "GATEWAY_AUTH_EXPIRED"
    );

    service
        .revoke(1, &expiring.binding_id, "2026-07-13T01:06:00Z")
        .unwrap();
    assert_eq!(
        service
            .token_for_helper("profile-expiring", &expiring.binding_id)
            .unwrap_err()
            .code,
        "GATEWAY_BINDING_REVOKED"
    );
}

#[test]
fn failed_keychain_delete_marks_revocation_pending_without_reenabling_hash() {
    let root = tempfile::tempdir().unwrap();
    let backend = Arc::new(FakeKeychain::default());
    let service = service(root.path(), backend.clone());
    let provisioned = provision(&service, "profile-a");
    let token = service
        .token_for_helper("profile-a", &provisioned.binding_id)
        .unwrap();
    backend.fail_deletes();

    let outcome = service
        .revoke(1, &provisioned.binding_id, "2026-07-13T01:06:00Z")
        .unwrap();
    assert!(outcome.revocation_pending);
    assert_eq!(
        service
            .authenticate(&format!("Bearer {token}"), "2026-07-13T01:07:00Z")
            .unwrap_err()
            .code,
        "GATEWAY_AUTH_INVALID"
    );
    assert!(service.load().unwrap().value.bindings[0].revocation_pending);
}

#[test]
fn token_requests_reject_duplicate_profile_invalid_model_and_timestamp() {
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path(), Arc::new(FakeKeychain::default()));
    provision(&service, "profile-a");

    let duplicate = service
        .provision(
            1,
            GatewayTokenRequest {
                profile_id: "profile-a".into(),
                provider: provider("other", "m"),
                provider_revision: 1,
                selected_model: "m".into(),
                expires_at: None,
                now: "2026-07-13T01:02:00Z".into(),
            },
        )
        .unwrap_err();
    assert_eq!(duplicate.code, "GATEWAY_BINDING_PROFILE_EXISTS");

    let invalid = service
        .provision(
            1,
            GatewayTokenRequest {
                profile_id: "profile-b".into(),
                provider: provider("other", "m"),
                provider_revision: 1,
                selected_model: "not-approved".into(),
                expires_at: None,
                now: "not-a-time".into(),
            },
        )
        .unwrap_err();
    assert!(matches!(
        invalid.code.as_str(),
        "GATEWAY_BINDING_MODEL_INVALID" | "GATEWAY_BINDING_TIMESTAMP_INVALID"
    ));
}

#[test]
fn transaction_candidate_is_idempotent_and_does_not_revoke_predecessor_early() {
    let root = tempfile::tempdir().unwrap();
    let service = service(root.path(), Arc::new(FakeKeychain::default()));
    let first = provision(&service, "profile-a");
    let old_token = service
        .token_for_helper("profile-a", &first.binding_id)
        .unwrap();
    let request = GatewayTokenRequest {
        profile_id: "profile-a".into(),
        provider: provider("deepseek", "deepseek-chat"),
        provider_revision: 8,
        selected_model: "deepseek-chat".into(),
        expires_at: None,
        now: "2026-07-13T01:03:00Z".into(),
    };

    let candidate = service
        .prepare_candidate(1, "attach-operation-1", request.clone())
        .unwrap();
    let replay = service
        .prepare_candidate(0, "attach-operation-1", request)
        .unwrap();
    assert_eq!(candidate.binding_id, replay.binding_id);
    assert_eq!(candidate.store_revision, replay.store_revision);
    assert!(service
        .authenticate(&format!("Bearer {old_token}"), "2026-07-13T01:04:00Z")
        .is_ok());
    assert!(service
        .token_for_helper("profile-a", &candidate.binding_id)
        .is_ok());
}
