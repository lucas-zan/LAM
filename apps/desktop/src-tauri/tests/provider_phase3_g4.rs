use localagentmanager_core::gateway::binding::{GatewayBindingCollection, GatewayBindingService};
use localagentmanager_core::gateway::routes::GatewayRouteComposer;
use localagentmanager_core::gateway::server::{
    GatewayLoopbackServer, GatewayServerConfig, HealthProofKey, RequestLogMetadata, RequestObserver,
};
use localagentmanager_core::gateway::upstream::{
    NetworkTargetPolicy, SecureUpstreamClient, UpstreamClientConfig, UpstreamCredentialResolver,
};
use localagentmanager_core::provider_attach_transaction::{
    AttachJournalCollection, AttachTransactionCoordinator,
};
use localagentmanager_core::provider_binding::{ProfileBindingCollection, RouteKind};
use localagentmanager_core::provider_credentials::{CredentialSource, SecretValue, UpstreamAuth};
use localagentmanager_core::provider_keychain::{
    KeychainBackend, KeychainCredentialReference, KeychainCredentialService,
};
use localagentmanager_core::provider_planner::{
    plan_profile_attach, plan_profile_detach, plan_provider_route, AdapterCatalog,
    AttachPlanContext, DryRunRegistry, GatewayPlanContext, RoutePlanInput,
};
use localagentmanager_core::provider_v2::{
    build_provider, AdapterConfig, CodexProviderOptions, ProviderCollection, ProviderInput,
    ProviderModel, ProviderProtocol,
};
use localagentmanager_core::storage::{InstallationLock, StoreOptions, VersionedFileStore};
use localagentmanager_core::{AppError, Result};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::future::Future;
use std::net::SocketAddr;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[test]
fn g4_manifest_declares_every_required_runtime_scenario() {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/provider-gateway-g4/manifest.json")).unwrap();
    let scenarios = manifest["scenarios"].as_array().unwrap();
    for required in [
        "attach_runtime_detach",
        "ui_close_sidecar_survival",
        "restart_recovery",
        "rebind_rotate_drift",
        "foreign_port_migration_rollback",
        "security_retry_fault_matrix",
    ] {
        assert!(scenarios.iter().any(|value| value == required));
    }
}

#[derive(Default)]
struct MemoryKeychain(Mutex<HashMap<String, String>>);

impl KeychainBackend for MemoryKeychain {
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
            .ok_or_else(|| AppError::new("KEYCHAIN_ITEM_NOT_FOUND", "missing"))
    }

    fn delete(&self, reference: &KeychainCredentialReference) -> Result<()> {
        self.0.lock().unwrap().remove(&reference.account);
        Ok(())
    }
}

struct LoopbackPolicy(SocketAddr);

impl NetworkTargetPolicy for LoopbackPolicy {
    fn validate_and_resolve<'a>(
        &'a self,
        _url: &'a url::Url,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<SocketAddr>>> + Send + 'a>> {
        Box::pin(async move { Ok(vec![self.0]) })
    }
}

struct SyntheticCredential;

impl UpstreamCredentialResolver for SyntheticCredential {
    fn resolve(&self, source: &CredentialSource) -> Result<Option<SecretValue>> {
        match source {
            CredentialSource::Env { env_key } if env_key == "DEEPSEEK_API_KEY" => Ok(Some(
                SecretValue::from_sensitive("G4_UPSTREAM_SECRET_NEVER_PERSIST".into()),
            )),
            CredentialSource::None => Ok(None),
            _ => Err(AppError::new(
                "PROVIDER_CREDENTIAL_SOURCE_UNSUPPORTED",
                "unexpected synthetic credential source",
            )),
        }
    }
}

#[derive(Default)]
struct CapturedMetadata(Mutex<Vec<RequestLogMetadata>>);

impl RequestObserver for CapturedMetadata {
    fn observe(&self, metadata: RequestLogMetadata) {
        self.0.lock().unwrap().push(metadata);
    }
}

#[tokio::test]
async fn deepseek_attach_real_gateway_codex_requests_and_detach() {
    let root = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let lock = InstallationLock::new(
        root.path().join("provider-hub.lock"),
        Duration::from_secs(2),
    );
    let provider_store = VersionedFileStore::<ProviderCollection>::new(
        root.path().join("providers.json"),
        lock.clone(),
        1,
        StoreOptions::default(),
    );
    let binding_store = VersionedFileStore::<ProfileBindingCollection>::new(
        root.path().join("bindings.json"),
        lock.clone(),
        1,
        StoreOptions::default(),
    );
    let journal_store = VersionedFileStore::<AttachJournalCollection>::new(
        root.path().join("attach-journal.json"),
        lock.clone(),
        1,
        StoreOptions::default(),
    );
    let gateway_store = VersionedFileStore::<GatewayBindingCollection>::new(
        root.path().join("gateway-bindings.json"),
        lock.clone(),
        1,
        StoreOptions::default(),
    );
    let keychain = Arc::new(MemoryKeychain::default());
    let gateway = Arc::new(GatewayBindingService::new(
        gateway_store,
        KeychainCredentialService::new(keychain.clone()),
    ));
    let upstream_server = MockServer::start().await;
    let mut provider = build_provider(
        ProviderInput {
            id: "deepseek-g4".into(),
            name: "Synthetic DeepSeek G4".into(),
            protocol: ProviderProtocol::ChatCompletions,
            base_url: "https://api.deepseek.com/v1".into(),
            default_model: "deepseek-chat".into(),
            models: vec![ProviderModel {
                id: "deepseek-chat".into(),
                label: "DeepSeek Chat".into(),
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
            codex: CodexProviderOptions::default(),
        },
        "2026-07-13T02:00:00Z",
    )
    .unwrap();
    provider.base_url = format!(
        "http://provider.test:{}/v1",
        upstream_server.address().port()
    );
    provider_store
        .compare_and_swap(
            0,
            &ProviderCollection {
                providers: vec![provider.clone()],
            },
        )
        .unwrap();
    let config_path = root.path().join("config.toml");
    fs::write(&config_path, "# existing user config\n").unwrap();
    #[cfg(unix)]
    fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
    let auth_helper = root.path().join("lam-auth-helper");
    fs::write(&auth_helper, "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    fs::set_permissions(&auth_helper, fs::Permissions::from_mode(0o700)).unwrap();

    let route = plan_provider_route(RoutePlanInput {
        provider,
        selected_model: "deepseek-chat".into(),
        adapters: AdapterCatalog::standard(),
    });
    assert_eq!(route.route_kind, RouteKind::Gateway);
    let plan = plan_profile_attach(
        route,
        AttachPlanContext {
            profile_id: "profile-g4".into(),
            provider_store_revision: 1,
            config_path: config_path.to_string_lossy().into_owned(),
            expected_binding_revision: None,
            binding_store_revision: 0,
            source_config_hash: localagentmanager_core::provider_config_editor::config_hash(
                &fs::read(&config_path).unwrap(),
            ),
            binding_drifted: false,
            credential_ready: true,
            gateway: GatewayPlanContext {
                base_url: "http://127.0.0.1:43123/v1".into(),
                available: true,
                endpoint_version: 1,
            },
            planner_options: BTreeMap::new(),
            auth_helper_path: auth_helper.to_string_lossy().into_owned(),
            provider_hub_root: root.path().to_string_lossy().into_owned(),
            gateway_binding_id: Some("00000000-0000-4000-8000-000000000004".into()),
        },
    );
    let coordinator = AttachTransactionCoordinator::new(
        lock,
        provider_store,
        binding_store.clone(),
        journal_store.clone(),
        gateway.clone(),
        1,
    );
    let mut registry = DryRunRegistry::new(300_000, 32);
    let ticket = registry.issue(&plan, 1_000);
    coordinator
        .execute_attach(&mut registry, &ticket, &plan, 1_001, None)
        .unwrap();
    let binding_snapshot = binding_store.load_or_default().unwrap();
    let binding = binding_snapshot.value.bindings[0].clone();
    let gateway_binding_id = binding.gateway_binding_id.clone().unwrap();
    let bearer = gateway
        .token_for_helper("profile-g4", &gateway_binding_id)
        .unwrap();
    let config = fs::read_to_string(&config_path).unwrap();
    assert!(config.contains("http://127.0.0.1:43123/v1"));
    assert!(config.contains("wire_api = \"responses\""));
    assert!(config.contains("request_max_retries = 0"));
    assert!(config.contains("stream_max_retries = 0"));
    assert!(config.contains(&gateway_binding_id));
    assert!(config.contains(
        fs::canonicalize(&auth_helper)
            .unwrap()
            .to_string_lossy()
            .as_ref()
    ));
    assert!(!config.contains("gateway-binding\""));
    assert!(!config.contains(&bearer));

    let upstream = SecureUpstreamClient::new(
        UpstreamClientConfig::for_test(),
        Arc::new(LoopbackPolicy(*upstream_server.address())),
        Arc::new(SyntheticCredential),
    )
    .unwrap();
    let observer = Arc::new(CapturedMetadata::default());
    let server = GatewayLoopbackServer::start(
        GatewayServerConfig::for_test(),
        gateway.clone(),
        Arc::new(GatewayRouteComposer::new(Arc::new(upstream))),
        Arc::new(HealthProofKey::new(&[7_u8; 32]).unwrap()),
        observer.clone(),
    )
    .await
    .unwrap();
    let gateway_url = format!("http://{}/v1", server.local_addr());
    let client = reqwest::Client::new();
    let models = client
        .get(format!("{gateway_url}/models"))
        .bearer_auth(&bearer)
        .send()
        .await
        .unwrap();
    assert_eq!(models.status(), 200);
    let catalog = models.json::<serde_json::Value>().await.unwrap();
    let model = &catalog["models"][0];
    assert_eq!(model["slug"], "deepseek-chat");
    assert!(model.get("id").is_none());
    assert_eq!(model["supported_reasoning_levels"], serde_json::json!([]));
    assert_eq!(model["supports_parallel_tool_calls"], false);

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header(
            "authorization",
            "Bearer G4_UPSTREAM_SECRET_NEVER_PERSIST",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id":"g4-text","object":"chat.completion","created":1,"model":"deepseek-chat",
            "choices":[{"index":0,"message":{"role":"assistant","content":"g4-ok"},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}
        })))
        .expect(1)
        .mount(&upstream_server)
        .await;
    let text = client
        .post(format!("{gateway_url}/responses"))
        .bearer_auth(&bearer)
        .json(&serde_json::json!({"model":"deepseek-chat","input":"hello","stream":false}))
        .send()
        .await
        .unwrap();
    assert_eq!(text.status(), 200);
    assert_eq!(
        text.json::<serde_json::Value>().await.unwrap()["output"][0]["content"][0]["text"],
        "g4-ok"
    );

    upstream_server.reset().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            concat!(
                "data: {\"id\":\"g4-stream\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"deepseek-chat\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"stream-ok\"},\"finish_reason\":\"stop\"}]}\n\n",
                "data: [DONE]\n\n"
            ),
            "text/event-stream",
        ))
        .expect(1)
        .mount(&upstream_server)
        .await;
    let stream = client
        .post(format!("{gateway_url}/responses"))
        .bearer_auth(&bearer)
        .json(&serde_json::json!({"model":"deepseek-chat","input":"hello","stream":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(stream.status(), 200);
    let stream = stream.text().await.unwrap();
    assert!(stream.contains("response.output_text.delta"));
    assert!(stream.contains("stream-ok"));

    upstream_server.reset().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id":"g4-tool","object":"chat.completion","created":1,"model":"deepseek-chat",
            "choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[
                {"id":"call-g4","type":"function","function":{"name":"lookup","arguments":"{\"q\":\"x\"}"}}
            ]},"finish_reason":"tool_calls"}]
        })))
        .expect(1)
        .mount(&upstream_server)
        .await;
    let tool = client
        .post(format!("{gateway_url}/responses"))
        .bearer_auth(&bearer)
        .json(&serde_json::json!({
            "model":"deepseek-chat","input":"lookup x","stream":false,
            "tools":[{"type":"function","name":"lookup","parameters":{"type":"object"}}]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(tool.status(), 200);
    assert_eq!(
        tool.json::<serde_json::Value>().await.unwrap()["output"][0]["call_id"],
        "call-g4"
    );

    let after_ui_close = client
        .get(format!("{gateway_url}/models"))
        .bearer_auth(&bearer)
        .send()
        .await
        .unwrap();
    assert_eq!(after_ui_close.status(), 200);

    let detach = plan_profile_detach(
        &binding,
        binding_snapshot.revision,
        localagentmanager_core::provider_config_editor::config_hash(
            &fs::read(&config_path).unwrap(),
        ),
    );
    let detach_ticket = registry.issue_detach(&detach, 2_000);
    coordinator
        .execute_detach(&mut registry, &detach_ticket, &detach, 2_001, None)
        .unwrap();
    let rejected = client
        .get(format!("{gateway_url}/models"))
        .bearer_auth(&bearer)
        .send()
        .await
        .unwrap();
    assert_eq!(rejected.status(), 401);
    assert_eq!(
        gateway
            .token_for_helper("profile-g4", &gateway_binding_id)
            .unwrap_err()
            .code,
        "GATEWAY_BINDING_REVOKED"
    );
    server.shutdown().await.unwrap();

    let captured = observer.0.lock().unwrap();
    let text_metric = captured
        .iter()
        .find(|metric| metric.route == "/v1/responses" && metric.usage.is_some())
        .unwrap();
    assert_eq!(text_metric.retry_count, 0);
    assert_eq!(text_metric.usage.as_ref().unwrap().total_tokens, 5);
    let metadata = serde_json::to_string(&*captured).unwrap();
    assert!(!metadata.contains("G4_UPSTREAM_SECRET_NEVER_PERSIST"));
    assert!(!metadata.contains(&bearer));
    assert!(!read_tree(root.path()).contains("G4_UPSTREAM_SECRET_NEVER_PERSIST"));
    assert!(!read_tree(root.path()).contains(&bearer));
    assert!(journal_store
        .load_or_default()
        .unwrap()
        .value
        .records
        .iter()
        .all(|record| record.last_error_code.as_deref().is_none()));
    assert!(keychain.0.lock().unwrap().is_empty());
}

fn read_tree(root: &Path) -> String {
    let mut output = String::new();
    for entry in fs::read_dir(root).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            output.push_str(&read_tree(&path));
        } else if let Ok(value) = fs::read_to_string(path) {
            output.push_str(&value);
        }
    }
    output
}
