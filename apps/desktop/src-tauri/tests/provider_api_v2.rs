use localagentmanager_core::provider_api_v2::*;
use localagentmanager_core::provider_credentials::{CredentialSource, SecretValue};
use localagentmanager_core::provider_keychain::{KeychainBackend, KeychainCredentialReference};
use localagentmanager_core::Result;
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{mpsc, mpsc::Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

struct ReadyResolver;

impl ProviderCredentialResolver for ReadyResolver {
    fn resolve(&self, _source: &CredentialSource) -> Result<SecretValue> {
        Ok(SecretValue::from_sensitive("synthetic-ready".into()))
    }
}

fn model_server(body: &str) -> (String, Receiver<String>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = vec![0_u8; 8 * 1024];
        let count = socket.read(&mut request).unwrap();
        sender
            .send(String::from_utf8_lossy(&request[..count]).into_owned())
            .unwrap();
        socket.write_all(response.as_bytes()).unwrap();
    });
    (format!("http://{address}/v1"), receiver, handle)
}

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[test]
fn dto_create_request_and_provider_view_have_stable_camel_case_goldens() {
    let request_json = json!({
        "expectedRevision": 7,
        "provider": {
            "id": "company-proxy", "name": "Company Proxy", "protocol": "responses",
            "baseUrl": "https://proxy.example.test/v1", "defaultModel": "model-a",
            "models": [{"id":"model-a","label":"Model A"}],
            "upstreamAuth": {"kind":"bearer","credential":{"kind":"env","envKey":"COMPANY_TOKEN"}},
            "adapter": {"kind":"none"}, "codex": {"queryParams":{},"envHttpHeaders":{}}
        }
    });
    let request: CreateProviderRequestV2 = serde_json::from_value(request_json.clone()).unwrap();
    assert_eq!(serde_json::to_value(&request).unwrap(), request_json);
    let view = ProviderProfileView::from_domain(
        &request.to_domain("2026-07-13T00:00:00Z").unwrap(),
        8,
        vec!["profile-a".into()],
    );
    let value = serde_json::to_value(&view).unwrap();
    assert_eq!(value["storeRevision"], 8);
    assert_eq!(value["adapter"]["kind"], "none");
    assert_eq!(value["codex"]["queryParams"], json!({}));
    assert_eq!(
        value["upstreamAuth"]["credential"]["envKey"],
        "COMPANY_TOKEN"
    );
    assert_eq!(value["usedBy"], json!(["profile-a"]));
    assert!(value.get("secret").is_none());
    assert!(!value.to_string().contains("env_key"));
}

#[test]
fn dto_provider_list_view_keeps_revision_when_providers_are_empty() {
    let view = ProviderListViewV2 {
        revision: 7,
        providers: Vec::new(),
    };
    assert_eq!(
        serde_json::to_value(view).unwrap(),
        json!({"revision": 7, "providers": []})
    );
}

#[test]
fn dto_legacy_env_key_and_openai_map_to_responses_with_warning() {
    let legacy: LegacyCreateProviderRequest = serde_json::from_value(json!({
        "id":"legacy", "name":"Legacy", "baseUrl":"https://legacy.example.test/v1",
        "wireApi":"openai", "defaultModel":"legacy-model", "envKey":"LEGACY_TOKEN",
        "secret":{"kind":"env","envKey":"LEGACY_TOKEN"}
    }))
    .unwrap();
    let converted = legacy.into_v2(0).unwrap();
    assert_eq!(
        converted.request.provider.protocol,
        ProviderProtocolDto::Responses
    );
    assert_eq!(
        converted.warnings,
        vec!["LEGACY_WIRE_OPENAI_MAPPED_TO_RESPONSES"]
    );
    let serialized = serde_json::to_string(&converted.request).unwrap();
    assert!(serialized.contains("envKey"));
    assert!(!serialized.contains("env_key"));
}

#[test]
fn dto_unknown_protocol_adapter_and_auth_fail_closed() {
    let base = json!({
        "expectedRevision":0,
        "provider":{"id":"p","name":"P","protocol":"responses","baseUrl":"https://p.example/v1",
        "defaultModel":"m","models":[{"id":"m","label":"M"}],
        "upstreamAuth":{"kind":"none"},"adapter":{"kind":"none"},
        "codex":{"queryParams":{},"envHttpHeaders":{}}}
    });
    for (pointer, value) in [
        ("/provider/protocol", json!("messages")),
        ("/provider/adapter/kind", json!("remote")),
        ("/provider/upstreamAuth/kind", json!("oauth")),
    ] {
        let mut changed = base.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        assert!(serde_json::from_value::<CreateProviderRequestV2>(changed).is_err());
    }
}

#[test]
fn dto_plan_and_error_views_are_redacted_and_recovery_aware() {
    let view = ProfileAttachPlanView {
        plan_id: "plan-1".into(),
        fingerprint: "fingerprint".into(),
        expires_at_ms: 123,
        profile_id: "profile-a".into(),
        provider_id: "provider-a".into(),
        selected_model: "model-a".into(),
        route_kind: RouteKindDto::Direct,
        blockers: vec![],
        warnings: vec!["warning".into()],
        operations: vec!["prepare_config".into()],
        redacted_preview: "auth=[REDACTED]".into(),
        expected_provider_store_revision: 1,
        expected_binding_store_revision: 2,
        expected_binding_revision: None,
        source_config_hash: "hash".into(),
    };
    let marker = "LAM_TEST_SECRET_DTO_sk-rpg110";
    let json = serde_json::to_string(&view).unwrap();
    assert!(!json.contains(marker));
    assert!(json.contains("redactedPreview"));
    let error = StructuredErrorView::from_error(localagentmanager_core::AppError::new(
        "ATTACH_PLAN_STALE",
        marker,
    ));
    let error_json = serde_json::to_string(&error).unwrap();
    assert!(!error_json.contains(marker));
    assert_eq!(error.recovery_actions, vec!["refresh", "plan_again"]);
}

fn service_request(expected_revision: u64) -> CreateProviderRequestV2 {
    serde_json::from_value(json!({
        "expectedRevision":expected_revision,
        "provider":{"id":"service-provider","name":"Service Provider","protocol":"responses",
        "baseUrl":"https://service.example.test/v1","defaultModel":"model-a",
        "models":[{"id":"model-a","label":"Model A"},{"id":"model-b","label":"Model B"}],
        "upstreamAuth":{"kind":"none"},"adapter":{"kind":"none"},
        "codex":{"queryParams":{},"envHttpHeaders":{}}}
    }))
    .unwrap()
}

#[test]
fn service_provider_crud_and_views_are_revision_aware() {
    let root = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let mut created =
        create_provider_service_v2(root.path(), service_request(0), "2026-07-13T00:00:00Z")
            .unwrap();
    assert_eq!(created.store_revision, 1);
    assert_eq!(
        list_provider_views_service_v2(root.path()).unwrap(),
        vec![created.clone()]
    );
    let mut update = service_request(1);
    update.provider.name = "Updated Provider".into();
    created = update_provider_service_v2(root.path(), update, "2026-07-13T00:01:00Z").unwrap();
    assert_eq!(created.name, "Updated Provider");
    assert_eq!(created.store_revision, 2);
    assert_eq!(
        update_provider_service_v2(root.path(), service_request(1), "2026-07-13T00:02:00Z")
            .unwrap_err()
            .code,
        "STORE_REVISION_CONFLICT"
    );
}

#[test]
fn service_provider_list_exposes_the_collection_revision() {
    let root = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();

    let empty = list_provider_hub_view_v2(root.path()).unwrap();
    assert_eq!(empty.revision, 0);
    assert!(empty.providers.is_empty());

    create_provider_service_v2(root.path(), service_request(0), "2026-07-16T00:00:00Z").unwrap();
    let populated = list_provider_hub_view_v2(root.path()).unwrap();
    assert_eq!(populated.revision, 1);
    assert_eq!(populated.providers.len(), 1);
    assert_eq!(populated.providers[0].store_revision, 1);

    let hub_root = localagentmanager_core::ProviderHubPaths::for_home(root.path())
        .ensure_canonical_root()
        .unwrap();
    let repository = localagentmanager_core::ProviderRepository::new(
        localagentmanager_core::VersionedFileStore::<
            localagentmanager_core::ProviderCollection,
        >::new(
            hub_root.join("providers.json"),
            localagentmanager_core::InstallationLock::new(
                hub_root.join("provider-hub.lock"),
                Duration::from_secs(2),
            ),
            1,
            localagentmanager_core::StoreOptions::default(),
        ),
    );
    repository.delete(1, "service-provider").unwrap();

    let deleted = list_provider_hub_view_v2(root.path()).unwrap();
    assert_eq!(deleted.revision, 2);
    assert!(deleted.providers.is_empty());

    let recreated =
        create_provider_service_v2(root.path(), service_request(2), "2026-07-16T00:01:00Z")
            .unwrap();
    assert_eq!(recreated.store_revision, 3);
}

#[test]
fn service_plan_execute_rebind_and_detach_share_ticket_and_revision_contracts() {
    let root = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    create_provider_service_v2(root.path(), service_request(0), "2026-07-13T00:00:00Z").unwrap();
    let profile = root.path().join("profile-a");
    fs::create_dir(&profile).unwrap();
    #[cfg(unix)]
    fs::set_permissions(&profile, fs::Permissions::from_mode(0o700)).unwrap();
    let config = profile.join("config.toml");
    fs::write(&config, "# keep\n[mcp_servers.keep]\ncommand=\"keep\"\n").unwrap();
    #[cfg(unix)]
    fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
    let mut state = ProviderApiV2State::default();
    let request = PlanAttachRequestV2 {
        profile_id: "profile-a".into(),
        provider_id: "service-provider".into(),
        selected_model: "model-a".into(),
    };
    let plan = plan_attach_service_v2_with_resolver(
        root.path(),
        &config,
        request,
        &mut state,
        1_000,
        &ReadyResolver,
        true,
    )
    .unwrap();
    assert!(plan.blockers.is_empty());
    assert!(!plan.redacted_preview.contains("secret"));
    let result = execute_attach_service_v2(
        root.path(),
        ExecuteAttachRequestV2 {
            plan_id: plan.plan_id.clone(),
            fingerprint: plan.fingerprint.clone(),
        },
        &mut state,
        1_001,
    )
    .unwrap();
    assert_eq!(result.state, "completed");
    assert!(fs::read_to_string(&config).unwrap().contains("model-a"));
    assert_eq!(
        execute_attach_service_v2(
            root.path(),
            ExecuteAttachRequestV2 {
                plan_id: plan.plan_id,
                fingerprint: plan.fingerprint
            },
            &mut state,
            1_002
        )
        .unwrap_err()
        .code,
        "ATTACH_PLAN_REPLAYED"
    );

    let detach = plan_detach_service_v2(root.path(), "profile-a", &mut state, 2_000).unwrap();
    execute_detach_service_v2(
        root.path(),
        ExecuteDetachRequestV2 {
            plan_id: detach.plan_id,
            fingerprint: detach.fingerprint,
        },
        &mut state,
        2_001,
    )
    .unwrap();
    assert!(fs::read_to_string(&config).unwrap().contains("# keep"));
}

#[test]
fn service_stale_config_execute_returns_stable_conflict_without_binding() {
    let root = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    create_provider_service_v2(root.path(), service_request(0), "2026-07-13T00:00:00Z").unwrap();
    let config = root.path().join("config.toml");
    fs::write(&config, "# original\n").unwrap();
    #[cfg(unix)]
    fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
    let mut state = ProviderApiV2State::default();
    let plan = plan_attach_service_v2_with_resolver(
        root.path(),
        &config,
        PlanAttachRequestV2 {
            profile_id: "profile-a".into(),
            provider_id: "service-provider".into(),
            selected_model: "model-a".into(),
        },
        &mut state,
        1_000,
        &ReadyResolver,
        true,
    )
    .unwrap();
    fs::write(&config, "# externally changed\n").unwrap();
    assert_eq!(
        execute_attach_service_v2(
            root.path(),
            ExecuteAttachRequestV2 {
                plan_id: plan.plan_id,
                fingerprint: plan.fingerprint
            },
            &mut state,
            1_001
        )
        .unwrap_err()
        .code,
        "ATTACH_PLAN_STALE"
    );
    assert!(list_binding_views_service_v2(root.path())
        .unwrap()
        .is_empty());
}

#[test]
fn service_v2_commands_are_registered_and_keep_legacy_commands() {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let main = fs::read_to_string(manifest.join("src/main.rs")).unwrap();
    let commands = fs::read_to_string(manifest.join("src/commands/mod.rs")).unwrap();
    for name in [
        "list_providers_v2",
        "discover_provider_models_v2",
        "test_provider_upstream_v2",
        "create_provider_v2",
        "approve_provider_auth_command_v2",
        "list_provider_auth_command_approvals_v2",
        "create_provider_with_keychain_v2",
        "update_provider_v2",
        "plan_attach_provider_v2",
        "execute_attach_provider_v2",
        "list_profile_provider_bindings_v2",
        "plan_detach_provider_v2",
        "execute_detach_provider_v2",
        "rotate_provider_credential_v2",
        "create_provider_legacy_compat_v2",
        "get_api_account_connection_v2",
        "update_api_account_connection_v2",
    ] {
        assert!(
            main.contains(&format!("commands::{name}")),
            "missing registration {name}"
        );
        assert!(
            commands.contains(&format!("fn {name}")),
            "missing command {name}"
        );
    }
    assert!(main.contains("commands::create_provider,"));
    assert!(main.contains("commands::attach_provider_to_profile,"));
}

#[derive(Default)]
struct ApiFakeKeychain(Mutex<BTreeMap<String, String>>);
impl KeychainBackend for ApiFakeKeychain {
    fn write(&self, reference: &KeychainCredentialReference, secret: &SecretValue) -> Result<()> {
        self.0.lock().unwrap().insert(
            reference.account.clone(),
            secret.with_exposed(str::to_owned),
        );
        Ok(())
    }
    fn read(&self, reference: &KeychainCredentialReference) -> Result<SecretValue> {
        self.0
            .lock()
            .unwrap()
            .get(&reference.account)
            .cloned()
            .map(SecretValue::from_sensitive)
            .ok_or_else(|| {
                localagentmanager_core::AppError::new("KEYCHAIN_ITEM_NOT_FOUND", "missing")
            })
    }
    fn delete(&self, reference: &KeychainCredentialReference) -> Result<()> {
        self.0.lock().unwrap().remove(&reference.account);
        Ok(())
    }
}

#[test]
fn service_keychain_rotation_request_returns_reference_only_view() {
    let root = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let mut request = service_request(0);
    request.provider.upstream_auth = UpstreamAuthDto::Bearer {
        credential: CredentialReferenceDto::Env {
            env_key: "COMPANY_TOKEN".into(),
        },
    };
    create_provider_service_v2(root.path(), request, "2026-07-13T00:00:00Z").unwrap();
    let backend = Arc::new(ApiFakeKeychain::default());
    let marker = "LAM_TEST_SECRET_ROTATE_sk-rpg110";
    let rotation = RotateProviderCredentialRequestV2 {
        expected_revision: 1,
        provider_id: "service-provider".into(),
        expected_credential: CredentialReferenceDto::Env {
            env_key: "COMPANY_TOKEN".into(),
        },
        secret: marker.into(),
    };
    assert!(!format!("{rotation:?}").contains(marker));
    let result = rotate_provider_credential_service_v2(
        root.path(),
        rotation,
        backend,
        "2026-07-13T00:01:00Z",
    )
    .unwrap();
    let json = serde_json::to_string(&result).unwrap();
    assert!(!json.contains(marker));
    assert!(json.contains("lam.remote-provider"));
}

#[test]
fn service_keychain_create_writes_secret_before_metadata_and_compensates_on_conflict() {
    let root = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let backend = Arc::new(ApiFakeKeychain::default());
    let marker = "LAM_TEST_SECRET_CREATE_sk-rpg112";
    let mut create = service_request(0);
    create.provider.upstream_auth = UpstreamAuthDto::Bearer {
        credential: CredentialReferenceDto::None,
    };
    let request = CreateProviderWithKeychainRequestV2 {
        expected_revision: 0,
        provider: create.provider,
        secret: marker.into(),
    };
    assert!(!format!("{request:?}").contains(marker));
    let view = create_provider_with_keychain_service_v2(
        root.path(),
        request,
        backend.clone(),
        "2026-07-13T00:00:00Z",
    )
    .unwrap();
    assert!(matches!(
        view.upstream_auth,
        UpstreamAuthDto::Bearer {
            credential: CredentialReferenceDto::Keychain { version: 1, .. }
        }
    ));
    assert!(!serde_json::to_string(&view).unwrap().contains(marker));
    assert_eq!(backend.0.lock().unwrap().len(), 1);

    let mut conflicting = service_request(0);
    conflicting.provider.id = "conflict".into();
    conflicting.provider.upstream_auth = UpstreamAuthDto::Bearer {
        credential: CredentialReferenceDto::None,
    };
    let error = create_provider_with_keychain_service_v2(
        root.path(),
        CreateProviderWithKeychainRequestV2 {
            expected_revision: 0,
            provider: conflicting.provider,
            secret: "compensate-me".into(),
        },
        backend.clone(),
        "2026-07-13T00:01:00Z",
    )
    .unwrap_err();
    assert_eq!(error.code, "STORE_REVISION_CONFLICT");
    assert_eq!(backend.0.lock().unwrap().len(), 1);
}

#[test]
fn service_upstream_test_validates_only_responses_and_returns_redacted_metadata() {
    let root = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let mut request = service_request(0);
    request.provider.upstream_auth = UpstreamAuthDto::Bearer {
        credential: CredentialReferenceDto::Env {
            env_key: "SUPER_SECRET_ENV_NAME".into(),
        },
    };
    create_provider_service_v2(root.path(), request, "2026-07-13T00:00:00Z").unwrap();

    assert_eq!(
        test_provider_upstream_service_v2(root.path(), "service-provider")
            .unwrap_err()
            .code,
        "PROVIDER_CREDENTIAL_MISSING"
    );

    let mut update = service_request(1);
    update.provider.protocol = ProviderProtocolDto::ChatCompletions;
    update_provider_service_v2(root.path(), update, "2026-07-13T00:01:00Z").unwrap();
    assert_eq!(
        test_provider_upstream_service_v2(root.path(), "service-provider")
            .unwrap_err()
            .code,
        "PROVIDER_DIRECT_ROUTE_REQUIRED"
    );
}

#[test]
fn service_upstream_test_normalizes_legacy_responses_gateway_flag_to_direct() {
    let (base_url, requests, handle) =
        model_server(r#"{"object":"list","data":[{"id":"model-a"}]}"#);
    let root = tempfile::tempdir().unwrap();
    let mut request = service_request(0);
    request.provider.base_url = base_url;
    request.provider.codex.route_via_gateway = true;
    create_provider_service_v2(root.path(), request, "2026-07-14T00:00:00Z").unwrap();

    let view = test_provider_upstream_service_v2(root.path(), "service-provider").unwrap();
    let wire = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    handle.join().unwrap();
    assert_eq!(view.route_kind, RouteKindDto::Direct);
    assert!(view.redacted_summary.contains("Responses Provider"));
    assert!(wire.starts_with("GET /v1/models HTTP/1.1"));
}

#[test]
fn service_upstream_test_rejects_nonstandard_codex_catalog_shape() {
    let (base_url, requests, handle) = model_server(r#"{"models":[{"slug":"model-a"}]}"#);
    let root = tempfile::tempdir().unwrap();
    let mut request = service_request(0);
    request.provider.base_url = base_url;
    create_provider_service_v2(root.path(), request, "2026-07-14T00:00:00Z").unwrap();

    let error = test_provider_upstream_service_v2(root.path(), "service-provider").unwrap_err();
    assert!(requests.recv_timeout(Duration::from_secs(2)).is_ok());
    handle.join().unwrap();
    assert_eq!(error.code, "PROVIDER_HEALTH_RESPONSE_INVALID");
}

#[test]
fn service_creates_hmac_bound_auth_command_approval_reference() {
    let root = tempfile::tempdir().unwrap();
    let executable = root.path().join("approved-helper");
    fs::write(&executable, "#!/bin/sh\nprintf synthetic-token\n").unwrap();
    #[cfg(unix)]
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let result = approve_auth_command_service_v2_with_key(
        root.path(),
        ApproveAuthCommandRequestV2 {
            expected_revision: 0,
            executable: executable.to_string_lossy().into_owned(),
            args: vec![],
            timeout_ms: 1_000,
            max_stdout_bytes: 128,
            refresh_interval_ms: 0,
        },
        &[9_u8; 32],
    )
    .unwrap();
    assert_eq!(result.revision, 1);
    assert_eq!(result.approval_id.len(), 64);
    assert_eq!(
        result.executable,
        fs::canonicalize(&executable).unwrap().to_string_lossy()
    );
    let persisted = fs::read_to_string(
        localagentmanager_core::provider_runtime::ProviderHubPaths::for_home(root.path())
            .canonical_root()
            .join("auth-command-approvals.json"),
    )
    .unwrap();
    assert!(!persisted.contains("synthetic-token"));
    assert!(!persisted.contains(&hex::encode([9_u8; 32])));
}

#[test]
fn provider_list_builder_is_snapshot_typed_and_keeps_readiness_io_outside() {
    let source = include_str!("../src/services/provider_api_v2.rs");
    let builder_start = source.find("fn build_provider_views(").unwrap();
    let builder_tail = &source[builder_start..];
    let builder_end = builder_tail
        .find("\nfn apply_provider_readiness(")
        .unwrap_or(builder_tail.len());
    let builder = &builder_tail[..builder_end];

    assert!(builder.contains("providers: &StoreSnapshot<ProviderCollection>"));
    assert!(builder.contains("readiness: &BTreeMap<String, ProviderViewReadiness>"));
    assert!(!builder.contains("ProductionCredentialResolver"));
    assert!(!builder.contains("fs::read"));
    assert!(!builder.contains("apply_provider_readiness("));
}
