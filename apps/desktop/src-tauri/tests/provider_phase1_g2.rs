#![cfg(unix)]

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use localagentmanager_core::provider_api_v2::*;
use localagentmanager_core::provider_auth_command::{approve_auth_command, AuthCommandSpec};
use localagentmanager_core::provider_credentials::{CredentialSource, SecretValue};
use localagentmanager_core::Result;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

type MockRecords = Arc<Mutex<Vec<(String, String, Vec<String>)>>>;

struct ReadyResolver;

impl ProviderCredentialResolver for ReadyResolver {
    fn resolve(&self, _source: &CredentialSource) -> Result<SecretValue> {
        Ok(SecretValue::from_sensitive("synthetic-ready".into()))
    }
}

fn record_headers(records: &MockRecords, method: &str, path: &str, headers: &HeaderMap) {
    records.lock().unwrap().push((
        method.into(),
        path.into(),
        headers.keys().map(|name| name.as_str().into()).collect(),
    ));
}

async fn mock_models(State(records): State<MockRecords>, headers: HeaderMap) -> impl IntoResponse {
    record_headers(&records, "GET", "/v1/models", &headers);
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        r#"{"object":"list","data":[{"id":"model-a","object":"model"}]}"#,
    )
}

async fn mock_responses(
    State(records): State<MockRecords>,
    headers: HeaderMap,
) -> impl IntoResponse {
    record_headers(&records, "POST", "/v1/responses", &headers);
    (
        StatusCode::OK,
        [("content-type", "text/event-stream")],
        "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp-g2\"}}\n\n",
    )
}

fn request(
    expected_revision: u64,
    id: &str,
    credential: CredentialReferenceDto,
) -> CreateProviderRequestV2 {
    CreateProviderRequestV2 {
        expected_revision,
        provider: ProviderDefinitionDto {
            id: id.into(),
            name: format!("{id} Provider"),
            protocol: ProviderProtocolDto::Responses,
            base_url: format!("https://{id}.example.test/v1"),
            default_model: "model-a".into(),
            models: vec![
                ProviderModelDto {
                    id: "model-a".into(),
                    label: "Model A".into(),
                    context_window: None,
                },
                ProviderModelDto {
                    id: "model-b".into(),
                    label: "Model B".into(),
                    context_window: None,
                },
            ],
            upstream_auth: UpstreamAuthDto::Bearer { credential },
            adapter: AdapterDto::None,
            compatibility_profile: None,
            codex: CodexOptionsDto {
                display_name: Some(format!("{id} Provider")),
                stream_idle_timeout_ms: Some(300_000),
                direct_request_max_retries: Some(0),
                direct_stream_max_retries: Some(0),
                route_via_gateway: false,
                query_params: BTreeMap::new(),
                env_http_headers: BTreeMap::new(),
                reasoning_effort: None,
            },
        },
    }
}

fn secure_root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    root
}

#[tokio::test(flavor = "current_thread")]
async fn local_responses_mock_exercises_pinned_routes_without_recording_secret_or_body() {
    let records = MockRecords::default();
    let app = Router::new()
        .route("/v1/models", get(mock_models))
        .route("/v1/responses", post(mock_responses))
        .with_state(records.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();
    let secret = "LAM_G2_HTTP_SECRET_sk-never-record";
    let prompt = "LAM_G2_PRIVATE_PROMPT_never-record";

    let models = client
        .get(format!("http://{address}/v1/models"))
        .bearer_auth(secret)
        .send()
        .await
        .unwrap();
    assert_eq!(models.status(), StatusCode::OK);
    let response = client
        .post(format!("http://{address}/v1/responses"))
        .bearer_auth(secret)
        .json(&serde_json::json!({
            "model": "model-a",
            "input": prompt,
            "stream": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response
        .text()
        .await
        .unwrap()
        .contains("response.completed"));
    server.abort();

    let recorded = format!("{:?}", records.lock().unwrap());
    assert!(recorded.contains("/v1/models"));
    assert!(recorded.contains("/v1/responses"));
    assert!(recorded.contains("authorization"));
    assert!(!recorded.contains(secret));
    assert!(!recorded.contains(prompt));

    let manifest = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/codex-gateway-contract/manifest.json"),
    )
    .unwrap();
    assert!(manifest.contains("/v1/models"));
    assert!(manifest.contains("/v1/responses"));
}

fn config(root: &std::path::Path, profile: &str, contents: &str) -> PathBuf {
    let profile_root = root.join(profile);
    fs::create_dir(&profile_root).unwrap();
    fs::set_permissions(&profile_root, fs::Permissions::from_mode(0o700)).unwrap();
    let path = profile_root.join("config.toml");
    fs::write(&path, contents).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    path
}

#[test]
fn env_responses_provider_runs_create_validate_attach_rebind_stale_and_detach_without_gateway() {
    let root = secure_root();
    create_provider_service_v2(
        root.path(),
        request(
            0,
            "env-provider",
            CredentialReferenceDto::Env {
                env_key: "G2_ENV_TOKEN".into(),
            },
        ),
        "2026-07-13T00:00:00Z",
    )
    .unwrap();
    assert_eq!(
        test_provider_upstream_service_v2(root.path(), "env-provider")
            .unwrap_err()
            .code,
        "PROVIDER_CREDENTIAL_MISSING"
    );

    let config_path = config(
        root.path(),
        "profile-env",
        "# user comment\nmodel = \"user-model\"\n[mcp_servers.keep]\ncommand = \"keep\"\n[features]\nweb_search = true\n",
    );
    let mut state = ProviderApiV2State::default();
    let attach = plan_attach_service_v2_with_resolver(
        root.path(),
        &config_path,
        PlanAttachRequestV2 {
            profile_id: "profile-env".into(),
            provider_id: "env-provider".into(),
            selected_model: "model-a".into(),
        },
        &mut state,
        1_000,
        &ReadyResolver,
        true,
    )
    .unwrap();
    assert_eq!(attach.route_kind, RouteKindDto::Direct);
    assert!(attach.blockers.is_empty());
    assert!(!attach
        .operations
        .iter()
        .any(|item| item.contains("gateway")));
    execute_attach_service_v2(
        root.path(),
        ExecuteAttachRequestV2 {
            plan_id: attach.plan_id,
            fingerprint: attach.fingerprint,
        },
        &mut state,
        1_001,
    )
    .unwrap();
    let attached = fs::read_to_string(&config_path).unwrap();
    assert!(attached.contains("# user comment"));
    assert!(attached.contains("[mcp_servers.keep]"));
    assert!(attached.contains("wire_api = \"responses\""));
    assert!(attached.contains("env_key = \"G2_ENV_TOKEN\""));
    assert!(!attached.contains("gateway"));

    let rebind = plan_attach_service_v2_with_resolver(
        root.path(),
        &config_path,
        PlanAttachRequestV2 {
            profile_id: "profile-env".into(),
            provider_id: "env-provider".into(),
            selected_model: "model-b".into(),
        },
        &mut state,
        2_000,
        &ReadyResolver,
        true,
    )
    .unwrap();
    execute_attach_service_v2(
        root.path(),
        ExecuteAttachRequestV2 {
            plan_id: rebind.plan_id,
            fingerprint: rebind.fingerprint,
        },
        &mut state,
        2_001,
    )
    .unwrap();
    let rebound = fs::read_to_string(&config_path).unwrap();
    assert!(rebound.contains("model = \"model-b\""));

    let stale = plan_attach_service_v2_with_resolver(
        root.path(),
        &config_path,
        PlanAttachRequestV2 {
            profile_id: "profile-env".into(),
            provider_id: "env-provider".into(),
            selected_model: "model-a".into(),
        },
        &mut state,
        3_000,
        &ReadyResolver,
        true,
    )
    .unwrap();
    fs::write(&config_path, format!("{rebound}\n# external edit\n")).unwrap();
    assert_eq!(
        execute_attach_service_v2(
            root.path(),
            ExecuteAttachRequestV2 {
                plan_id: stale.plan_id,
                fingerprint: stale.fingerprint,
            },
            &mut state,
            3_001,
        )
        .unwrap_err()
        .code,
        "ATTACH_PLAN_STALE"
    );
    fs::write(&config_path, &rebound).unwrap();

    let detach = plan_detach_service_v2(root.path(), "profile-env", &mut state, 4_000).unwrap();
    execute_detach_service_v2(
        root.path(),
        ExecuteDetachRequestV2 {
            plan_id: detach.plan_id,
            fingerprint: detach.fingerprint,
        },
        &mut state,
        4_001,
    )
    .unwrap();
    let detached = fs::read_to_string(&config_path).unwrap();
    assert!(detached.contains("# user comment"));
    assert!(detached.contains("[mcp_servers.keep]"));
    assert!(!detached.contains("model_providers.env-provider"));
    assert!(list_binding_views_service_v2(root.path())
        .unwrap()
        .is_empty());
}

#[test]
fn keychain_and_approved_auth_command_are_attachable_direct_routes_without_secret_persistence() {
    let root = secure_root();
    create_provider_service_v2(
        root.path(),
        request(
            0,
            "keychain-provider",
            CredentialReferenceDto::Env {
                env_key: "ROTATE_FROM_ENV".into(),
            },
        ),
        "2026-07-13T00:00:00Z",
    )
    .unwrap();
    let marker = "LAM_G2_KEYCHAIN_SECRET_sk-never-persist";
    rotate_provider_credential_service_v2(
        root.path(),
        RotateProviderCredentialRequestV2 {
            expected_revision: 1,
            provider_id: "keychain-provider".into(),
            expected_credential: CredentialReferenceDto::Env {
                env_key: "ROTATE_FROM_ENV".into(),
            },
            secret: marker.into(),
        },
        "2026-07-13T00:01:00Z",
    )
    .unwrap();
    let keychain_config = config(root.path(), "profile-keychain", "# keychain profile\n");
    let mut state = ProviderApiV2State::default();
    let keychain_plan = plan_attach_service_v2_with_resolver(
        root.path(),
        &keychain_config,
        PlanAttachRequestV2 {
            profile_id: "profile-keychain".into(),
            provider_id: "keychain-provider".into(),
            selected_model: "model-a".into(),
        },
        &mut state,
        1_000,
        &ReadyResolver,
        true,
    )
    .unwrap();
    assert!(keychain_plan.blockers.is_empty());
    assert!(keychain_plan
        .redacted_preview
        .contains("[model_providers.keychain-provider.auth]"));
    assert!(!keychain_plan.redacted_preview.contains(marker));

    let approved = approve_auth_command(AuthCommandSpec {
        executable: PathBuf::from("/usr/bin/printf"),
        args: vec!["synthetic-auth-token".into()],
        cwd: None,
        timeout_ms: 500,
        max_stdout_bytes: 128,
        cache_ttl_ms: 1_000,
    })
    .unwrap();
    create_provider_service_v2(
        root.path(),
        request(
            2,
            "command-provider",
            CredentialReferenceDto::AuthCommand {
                approval_id: approved.approval_fingerprint().into(),
            },
        ),
        "2026-07-13T00:02:00Z",
    )
    .unwrap();
    let command_config = config(root.path(), "profile-command", "# command profile\n");
    let command_plan = plan_attach_service_v2_with_resolver(
        root.path(),
        &command_config,
        PlanAttachRequestV2 {
            profile_id: "profile-command".into(),
            provider_id: "command-provider".into(),
            selected_model: "model-a".into(),
        },
        &mut state,
        2_000,
        &ReadyResolver,
        true,
    )
    .unwrap();
    assert!(command_plan.blockers.is_empty());
    assert!(command_plan
        .redacted_preview
        .contains("[model_providers.command-provider.auth]"));
    assert!(!command_plan
        .redacted_preview
        .contains("synthetic-auth-token"));

    // Secrets must only live in the dedicated 0600 credentials file, never
    // scattered across other LAM state.
    let mut credentials_files = 0;
    for path in walkdir(root.path()) {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let bytes = fs::read(&path).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        if text.contains(marker) {
            assert_eq!(
                file_name,
                "provider-credentials.json",
                "secret leaked into {}",
                path.display()
            );
            credentials_files += 1;
        }
    }
    assert!(
        credentials_files >= 1,
        "provider credential must be persisted in the plaintext store"
    );
}

#[test]
fn legacy_provider_migrates_then_completes_direct_attach_and_detach() {
    let root = secure_root();
    let migrated = create_legacy_provider_service_v2(
        root.path(),
        LegacyCreateProviderRequest {
            id: "legacy-provider".into(),
            name: "Legacy Provider".into(),
            base_url: "https://legacy.example.test/v1".into(),
            wire_api: "openai".into(),
            default_model: "legacy-model".into(),
            env_key: Some("LEGACY_TOKEN".into()),
            secret: None,
        },
        0,
        "2026-07-13T00:00:00Z",
    )
    .unwrap();
    assert_eq!(
        migrated.warnings,
        vec!["LEGACY_WIRE_OPENAI_MAPPED_TO_RESPONSES"]
    );
    let config_path = config(
        root.path(),
        "profile-legacy",
        "# preserve legacy user config\n",
    );
    let mut state = ProviderApiV2State::default();
    let plan = plan_attach_service_v2_with_resolver(
        root.path(),
        &config_path,
        PlanAttachRequestV2 {
            profile_id: "profile-legacy".into(),
            provider_id: "legacy-provider".into(),
            selected_model: "legacy-model".into(),
        },
        &mut state,
        1_000,
        &ReadyResolver,
        true,
    )
    .unwrap();
    assert!(plan.blockers.is_empty());
    execute_attach_service_v2(
        root.path(),
        ExecuteAttachRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
        },
        &mut state,
        1_001,
    )
    .unwrap();
    let detach = plan_detach_service_v2(root.path(), "profile-legacy", &mut state, 2_000).unwrap();
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
    assert!(fs::read_to_string(config_path)
        .unwrap()
        .contains("# preserve legacy user config"));
}

fn walkdir(root: &std::path::Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}

#[test]
fn every_phase1_provider_source_has_a_focused_test_mapping() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for (source, test) in [
        ("provider_v2.rs", "provider_v2.rs"),
        ("provider_credentials.rs", "provider_credentials.rs"),
        ("provider_binding.rs", "provider_binding.rs"),
        ("provider_config_editor.rs", "provider_config_editor.rs"),
        ("provider_planner.rs", "provider_planner.rs"),
        (
            "provider_attach_transaction.rs",
            "provider_attach_transaction.rs",
        ),
        ("provider_keychain.rs", "provider_keychain.rs"),
        ("provider_auth_command.rs", "provider_auth_command.rs"),
        ("provider_api_v2.rs", "provider_api_v2.rs"),
    ] {
        let source_path = manifest.join("src/services").join(source);
        let test_path = manifest.join("tests").join(test);
        assert!(
            source_path.is_file(),
            "missing Phase 1 source {}",
            source_path.display()
        );
        assert!(
            test_path.is_file(),
            "missing focused test {}",
            test_path.display()
        );
        assert!(
            fs::metadata(&test_path).unwrap().len() > 100,
            "focused test is unexpectedly empty: {}",
            test_path.display()
        );
    }
    assert!(manifest.join("tests/provider_versioned_store.rs").is_file());
    assert!(manifest.join("tests/provider_phase1_g2.rs").is_file());
}
