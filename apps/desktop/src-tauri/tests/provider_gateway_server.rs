use localagentmanager_core::gateway::binding::GatewayBindingSnapshot;
use localagentmanager_core::gateway::server::{
    FileMetadataObserver, GatewayAuthenticator, GatewayHttpRequest, GatewayHttpResponse,
    GatewayLoopbackServer, GatewayRouteHandler, GatewayServerConfig, HealthDocument,
    HealthProofKey, RequestLogMetadata, RequestObserver, RunningGatewayServer,
};
use localagentmanager_core::provider_credentials::{CredentialSource, UpstreamAuth};
use localagentmanager_core::provider_v2::{
    build_provider, AdapterConfig, CodexProviderOptions, ProviderInput, ProviderModel,
    ProviderProtocol,
};
use localagentmanager_core::{AppError, Result};
use std::fs;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct FakeAuth;

impl GatewayAuthenticator for FakeAuth {
    fn authenticate(&self, authorization: &str) -> Result<GatewayBindingSnapshot> {
        if authorization != "Bearer valid" {
            return Err(AppError::new("GATEWAY_AUTH_INVALID", "invalid"));
        }
        Ok(snapshot())
    }
}

struct FakeHandler;

impl GatewayRouteHandler for FakeHandler {
    fn handle(
        &self,
        request: GatewayHttpRequest,
    ) -> Pin<Box<dyn Future<Output = Result<GatewayHttpResponse>> + Send + '_>> {
        Box::pin(async move {
            if request.body == b"slow" {
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            if request.body == b"timeout" {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Ok(GatewayHttpResponse::json(
                200,
                serde_json::json!({
                    "path": request.path,
                    "profile": request.binding.profile_id,
                    "bodyBytes": request.body.len(),
                    "requestId": request.request_id,
                }),
            ))
        })
    }
}

#[derive(Default)]
struct CaptureObserver(Mutex<Vec<RequestLogMetadata>>);

impl RequestObserver for CaptureObserver {
    fn observe(&self, metadata: RequestLogMetadata) {
        self.0.lock().unwrap().push(metadata);
    }
}

fn metadata(request_id: &str) -> RequestLogMetadata {
    RequestLogMetadata {
        request_id: request_id.into(),
        route: "/v1/responses".into(),
        status: 200,
        latency_ms: 7,
        binding_hash: "binding-hash".into(),
        retry_count: 0,
        usage: None,
    }
}

#[test]
fn file_metadata_observer_appends_private_sanitized_jsonl_and_ignores_io_errors() {
    let root = tempfile::tempdir().unwrap();
    let log_path = root.path().join("gateway-requests.jsonl");
    let observer = FileMetadataObserver::new(log_path.clone());
    observer.observe(metadata("request-1"));
    observer.observe(metadata("request-2"));

    let contents = fs::read_to_string(&log_path).unwrap();
    let records = contents
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["requestId"], "request-1");
    assert_eq!(records[1]["requestId"], "request-2");
    for forbidden in ["authorization", "body", "prompt", "credential"] {
        assert!(!contents.to_ascii_lowercase().contains(forbidden));
    }
    #[cfg(unix)]
    assert_eq!(
        fs::metadata(&log_path).unwrap().permissions().mode() & 0o077,
        0
    );

    let blocker = root.path().join("blocker");
    fs::write(&blocker, b"not a directory").unwrap();
    FileMetadataObserver::new(blocker.join("requests.jsonl")).observe(metadata("ignored"));
}

fn snapshot() -> GatewayBindingSnapshot {
    let provider = build_provider(
        ProviderInput {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            protocol: ProviderProtocol::ChatCompletions,
            base_url: "https://api.deepseek.com/v1".into(),
            default_model: "deepseek-chat".into(),
            models: vec![ProviderModel {
                id: "deepseek-chat".into(),
                label: "Chat".into(),
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
        "2026-07-13T01:00:00Z",
    )
    .unwrap();
    GatewayBindingSnapshot {
        binding_id: "binding-a".into(),
        profile_id: "profile-a".into(),
        provider_id: "deepseek".into(),
        selected_model: "deepseek-chat".into(),
        provider_revision: 7,
        provider,
        credential_reference:
            localagentmanager_core::provider_keychain::KeychainCredentialReference::new(
                "gateway-test",
                1,
            )
            .unwrap(),
        generation: 1,
    }
}

async fn start(observer: Arc<CaptureObserver>) -> RunningGatewayServer {
    GatewayLoopbackServer::start(
        GatewayServerConfig {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            protocol_version: 1,
            component_version: "0.2.1".into(),
            state_schema: 1,
            install_id: "install-a".into(),
            instance_id: "instance-a".into(),
            ready: true,
            max_body_bytes: 64,
            max_inflight: 2,
            max_inflight_per_binding: 2,
            max_queue: 0,
            max_queue_per_binding: 0,
            request_timeout: Duration::from_secs(2),
        },
        Arc::new(FakeAuth),
        Arc::new(FakeHandler),
        Arc::new(HealthProofKey::new(b"0123456789abcdef0123456789abcdef").unwrap()),
        observer,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn listener_is_ipv4_loopback_only_and_uses_real_socket() {
    let observer = Arc::new(CaptureObserver::default());
    let server = start(observer).await;
    assert_eq!(server.local_addr().ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert_ne!(server.local_addr().port(), 0);
    server.shutdown().await.unwrap();

    let error = GatewayLoopbackServer::start(
        GatewayServerConfig {
            bind_addr: "0.0.0.0:0".parse().unwrap(),
            ..GatewayServerConfig::for_test()
        },
        Arc::new(FakeAuth),
        Arc::new(FakeHandler),
        Arc::new(HealthProofKey::new(b"0123456789abcdef0123456789abcdef").unwrap()),
        Arc::new(CaptureObserver::default()),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "GATEWAY_BIND_ADDRESS_FORBIDDEN");
}

#[tokio::test]
async fn health_requires_bearer_and_nonce_proof_is_instance_verifiable() {
    let server = start(Arc::new(CaptureObserver::default())).await;
    let client = reqwest::Client::new();
    let url = format!("http://{}/healthz", server.local_addr());
    assert_eq!(client.get(&url).send().await.unwrap().status(), 401);

    let response = client
        .get(&url)
        .header("authorization", "Bearer valid")
        .header("x-lam-health-nonce", "nonce-0123456789abcdef")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let document: HealthDocument = response.json().await.unwrap();
    let verifier = HealthProofKey::new(b"0123456789abcdef0123456789abcdef").unwrap();
    assert!(verifier.verify("nonce-0123456789abcdef", &document));
    assert_eq!(document.install_id, "install-a");
    assert_eq!(document.instance_id, "instance-a");

    let mut spoof = document;
    spoof.instance_id = "foreign-instance".into();
    assert!(!verifier.verify("nonce-0123456789abcdef", &spoof));
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn auth_runs_before_body_read_and_limits_are_stable() {
    let server = start(Arc::new(CaptureObserver::default())).await;
    let client = reqwest::Client::new();
    let url = format!("http://{}/v1/responses", server.local_addr());
    let oversized = "LAM_TEST_PROMPT_SECRET".repeat(10);
    let unauthenticated = client
        .post(&url)
        .body(oversized.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), 401);
    assert_eq!(
        unauthenticated.json::<serde_json::Value>().await.unwrap()["error"]["code"],
        "GATEWAY_AUTH_INVALID"
    );

    let limited = client
        .post(&url)
        .header("authorization", "Bearer valid")
        .body(oversized)
        .send()
        .await
        .unwrap();
    assert_eq!(limited.status(), 413);
    assert_eq!(
        limited.json::<serde_json::Value>().await.unwrap()["error"]["code"],
        "GATEWAY_BODY_LIMIT"
    );
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn routes_request_ids_cors_and_redacted_observer_are_deterministic() {
    let observer = Arc::new(CaptureObserver::default());
    let server = start(observer.clone()).await;
    let client = reqwest::Client::new();
    let base = format!("http://{}", server.local_addr());
    let response = client
        .post(format!("{base}/v1/responses"))
        .header("authorization", "Bearer valid")
        .header("x-request-id", "bad LAM_TEST_SECRET")
        .body("synthetic prompt and tool args")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(response
        .headers()
        .get("access-control-allow-origin")
        .is_none());
    let request_id = response.headers()["x-request-id"]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(!request_id.contains("bad"));

    let missing = client
        .get(format!("{base}/unknown"))
        .header("authorization", "Bearer valid")
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
    assert_eq!(
        missing.json::<serde_json::Value>().await.unwrap()["error"]["code"],
        "GATEWAY_ROUTE_NOT_FOUND"
    );

    let encoded = {
        let logs = observer.0.lock().unwrap();
        serde_json::to_string(&*logs).unwrap()
    };
    assert!(!encoded.contains("synthetic prompt"));
    assert!(!encoded.contains("LAM_TEST_SECRET"));
    assert!(encoded.contains(&request_id));
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn concurrency_and_total_deadline_fail_fast_with_stable_errors() {
    let observer = Arc::new(CaptureObserver::default());
    let server = GatewayLoopbackServer::start(
        GatewayServerConfig {
            max_inflight: 1,
            max_inflight_per_binding: 1,
            max_queue: 0,
            max_queue_per_binding: 0,
            request_timeout: Duration::from_millis(30),
            ..GatewayServerConfig::for_test()
        },
        Arc::new(FakeAuth),
        Arc::new(FakeHandler),
        Arc::new(HealthProofKey::new(b"0123456789abcdef0123456789abcdef").unwrap()),
        observer,
    )
    .await
    .unwrap();
    let client = reqwest::Client::new();
    let url = format!("http://{}/v1/responses", server.local_addr());
    let first = {
        let client = client.clone();
        let url = url.clone();
        tokio::spawn(async move {
            client
                .post(url)
                .header("authorization", "Bearer valid")
                .body("slow")
                .send()
                .await
                .unwrap()
        })
    };
    tokio::time::sleep(Duration::from_millis(10)).await;
    let limited = client
        .post(&url)
        .header("authorization", "Bearer valid")
        .body("fast")
        .send()
        .await
        .unwrap();
    assert_eq!(limited.status(), 429);
    assert_eq!(
        limited.json::<serde_json::Value>().await.unwrap()["error"]["code"],
        "GATEWAY_CONCURRENCY_LIMIT"
    );
    assert_eq!(first.await.unwrap().status(), 504);

    let timeout = client
        .post(&url)
        .header("authorization", "Bearer valid")
        .body("timeout")
        .send()
        .await
        .unwrap();
    assert_eq!(timeout.status(), 504);
    assert_eq!(
        timeout.json::<serde_json::Value>().await.unwrap()["error"]["code"],
        "GATEWAY_REQUEST_TIMEOUT"
    );
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn bounded_queue_waits_once_and_rejects_overflow_per_binding() {
    let server = GatewayLoopbackServer::start(
        GatewayServerConfig {
            max_inflight: 1,
            max_inflight_per_binding: 1,
            max_queue: 1,
            max_queue_per_binding: 1,
            request_timeout: Duration::from_millis(500),
            ..GatewayServerConfig::for_test()
        },
        Arc::new(FakeAuth),
        Arc::new(FakeHandler),
        Arc::new(HealthProofKey::new(b"0123456789abcdef0123456789abcdef").unwrap()),
        Arc::new(CaptureObserver::default()),
    )
    .await
    .unwrap();
    let client = reqwest::Client::new();
    let url = format!("http://{}/v1/responses", server.local_addr());
    let first = tokio::spawn({
        let client = client.clone();
        let url = url.clone();
        async move {
            client
                .post(url)
                .header("authorization", "Bearer valid")
                .body("slow")
                .send()
                .await
                .unwrap()
        }
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    let queued = tokio::spawn({
        let client = client.clone();
        let url = url.clone();
        async move {
            client
                .post(url)
                .header("authorization", "Bearer valid")
                .body("fast")
                .send()
                .await
                .unwrap()
        }
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    let overflow = client
        .post(&url)
        .header("authorization", "Bearer valid")
        .body("fast")
        .send()
        .await
        .unwrap();
    assert_eq!(overflow.status(), 429);
    assert_eq!(first.await.unwrap().status(), 200);
    assert_eq!(queued.await.unwrap().status(), 200);
    server.shutdown().await.unwrap();
}
