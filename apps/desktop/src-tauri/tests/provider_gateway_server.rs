use axum::body::Body;
use localagentmanager_core::gateway::binding::GatewayBindingSnapshot;
use localagentmanager_core::gateway::observability::{GatewayRejectionReason, GatewayTimeoutStage};
use localagentmanager_core::gateway::server::{
    FileMetadataObserver, GatewayAuthenticator, GatewayHttpRequest, GatewayHttpResponse,
    GatewayLoopbackServer, GatewayRouteHandler, GatewayServerConfig, HealthDocument,
    HealthProofKey, RequestLogMetadata, RequestObserver, RunningGatewayServer,
};
use localagentmanager_core::gateway::upstream::GatewayCancellation;
use localagentmanager_core::provider_credentials::{CredentialSource, UpstreamAuth};
use localagentmanager_core::provider_v2::{
    build_provider, AdapterConfig, CodexProviderOptions, ProviderInput, ProviderModel,
    ProviderProtocol,
};
use localagentmanager_core::{AppError, Result};
use std::convert::Infallible;
use std::fs;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, Notify, Semaphore};
use tokio_stream::wrappers::ReceiverStream;

struct FakeAuth;

impl GatewayAuthenticator for FakeAuth {
    fn authenticate(&self, authorization: &str) -> Result<GatewayBindingSnapshot> {
        if authorization != "Bearer valid" {
            return Err(AppError::new("GATEWAY_AUTH_INVALID", "invalid"));
        }
        Ok(snapshot())
    }
}

struct MultiBindingAuth;

impl GatewayAuthenticator for MultiBindingAuth {
    fn authenticate(&self, authorization: &str) -> Result<GatewayBindingSnapshot> {
        let suffix = match authorization {
            "Bearer binding-a" => "a",
            "Bearer binding-b" => "b",
            _ => return Err(AppError::new("GATEWAY_AUTH_INVALID", "invalid")),
        };
        let mut binding = snapshot();
        binding.binding_id = format!("binding-{suffix}");
        binding.profile_id = format!("profile-{suffix}");
        Ok(binding)
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
            if request.body == b"handler-error" {
                return Err(AppError::new("UPSTREAM_TEST_FAILURE", "synthetic failure"));
            }
            if request.body == b"upstream-limit" {
                return Ok(GatewayHttpResponse::json(502, serde_json::json!({}))
                    .with_outcome_code("GATEWAY_UPSTREAM_CONCURRENCY_LIMIT"));
            }
            if request.body == b"retry" {
                return Ok(GatewayHttpResponse::json(503, serde_json::json!({}))
                    .with_retry_after(Some("12".into())));
            }
            if request.body == b"invalid-retry" {
                return Ok(GatewayHttpResponse::json(503, serde_json::json!({}))
                    .with_retry_after(Some("invalid\r\nheader".into())));
            }
            Ok(GatewayHttpResponse::json(
                200,
                serde_json::json!({
                    "path": request.path,
                    "profile": request.binding.profile_id,
                    "bodyBytes": request.body.len(),
                    "requestId": request.request_id,
                    "originator": request.upstream_headers.get("originator"),
                    "userAgent": request.upstream_headers.get("user-agent"),
                    "hasAuthorization": request.upstream_headers.get("authorization").is_some(),
                    "hasUnapproved": request.upstream_headers.get("x-unapproved").is_some(),
                }),
            ))
        })
    }
}

#[derive(Default)]
struct BindingIsolationHandler {
    binding_a_started: Notify,
    release_binding_a: Notify,
}

#[derive(Default)]
struct StreamingLifecycleHandler {
    started: Notify,
    sender: Mutex<Option<mpsc::Sender<std::result::Result<Vec<u8>, Infallible>>>>,
}

struct UpstreamLifecycleHandler {
    capacity: Arc<Semaphore>,
    started: Notify,
}

impl UpstreamLifecycleHandler {
    fn new() -> Self {
        Self {
            capacity: Arc::new(Semaphore::new(1)),
            started: Notify::new(),
        }
    }
}

impl GatewayRouteHandler for StreamingLifecycleHandler {
    fn handle(
        &self,
        _request: GatewayHttpRequest,
    ) -> Pin<Box<dyn Future<Output = Result<GatewayHttpResponse>> + Send + '_>> {
        Box::pin(async move {
            let (sender, receiver) = mpsc::channel(1);
            *self.sender.lock().unwrap() = Some(sender);
            self.started.notify_one();
            Ok(GatewayHttpResponse::from_body(
                200,
                "text/event-stream",
                Body::from_stream(ReceiverStream::new(receiver)),
            )
            .streaming())
        })
    }
}

impl GatewayRouteHandler for UpstreamLifecycleHandler {
    fn handle(
        &self,
        request: GatewayHttpRequest,
    ) -> Pin<Box<dyn Future<Output = Result<GatewayHttpResponse>> + Send + '_>> {
        Box::pin(async move {
            let permit = self.capacity.clone().try_acquire_owned().map_err(|_| {
                AppError::new("GATEWAY_UPSTREAM_CONCURRENCY_LIMIT", "capacity retained")
            })?;
            let cancellation = GatewayCancellation::new();
            let worker_cancellation = cancellation.clone();
            let (completion, completed) = oneshot::channel();
            let (sender, receiver) = mpsc::channel::<std::result::Result<Vec<u8>, Infallible>>(1);
            self.started.notify_one();
            tokio::spawn(async move {
                let _completion = completion;
                let _permit = permit;
                if request.body == b"complete" {
                    let _ = sender.send(Ok(b"done".to_vec())).await;
                } else {
                    let _sender = sender;
                    worker_cancellation.cancelled().await;
                }
            });
            Ok(GatewayHttpResponse::from_body(
                200,
                "text/event-stream",
                Body::from_stream(ReceiverStream::new(receiver)),
            )
            .streaming()
            .with_stream_lifecycle(cancellation, completed))
        })
    }
}

impl GatewayRouteHandler for BindingIsolationHandler {
    fn handle(
        &self,
        request: GatewayHttpRequest,
    ) -> Pin<Box<dyn Future<Output = Result<GatewayHttpResponse>> + Send + '_>> {
        Box::pin(async move {
            if request.body == b"hold-binding-a" {
                self.binding_a_started.notify_one();
                self.release_binding_a.notified().await;
            }
            Ok(GatewayHttpResponse::json(200, serde_json::json!({})))
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
        provider_hash: "provider-hash".into(),
        capacity_hash: "capacity-hash".into(),
        queue_wait_ms: 0,
        queued_body_bytes: 0,
        global_running: 0,
        global_queued: 0,
        binding_running: 0,
        binding_queued: 0,
        active_streams: 0,
        ttft_ms: None,
        upstream_status: None,
        outcome_code: None,
        timeout_stage: None,
        rejection_reason: None,
        retry_count: 0,
        usage: None,
    }
}

#[test]
fn production_concurrency_baseline_remains_32_4_64_8_16() {
    let source = include_str!("../src/bin/lam-provider-gateway.rs");
    for expected in [
        "max_inflight: 32",
        "max_inflight_per_binding: 4",
        "max_queue: 64",
        "max_queue_per_binding: 8",
        "max_inflight: 16",
    ] {
        assert!(source.contains(expected), "missing baseline: {expected}");
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
    for forbidden in [
        "\"authorization\"",
        "\"body\"",
        "\"prompt\"",
        "\"credential\"",
    ] {
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
        observer.clone(),
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
async fn prebound_loopback_listener_is_adopted_without_rebinding() {
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let observer = Arc::new(CaptureObserver::default());
    let server = GatewayLoopbackServer::start_with_listener(
        GatewayServerConfig {
            bind_addr: address,
            ..GatewayServerConfig::for_test()
        },
        listener,
        Arc::new(FakeAuth),
        Arc::new(FakeHandler),
        Arc::new(HealthProofKey::new(b"0123456789abcdef0123456789abcdef").unwrap()),
        observer,
    )
    .await
    .unwrap();

    assert_eq!(server.local_addr(), address);
    assert!(TcpListener::bind(address).is_err());
    server.shutdown().await.unwrap();
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
        .header("user-agent", "codex_exec/0.144.5 (Mac OS; arm64)")
        .header("originator", "codex_exec")
        .header("x-codex-beta-features", "remote_compaction_v2")
        .header("x-unapproved", "must-not-forward")
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
    let body = response.json::<serde_json::Value>().await.unwrap();
    assert_eq!(body["originator"], "codex_exec");
    assert!(body["userAgent"]
        .as_str()
        .unwrap()
        .starts_with("codex_exec/"));
    assert_eq!(body["hasAuthorization"], false);
    assert_eq!(body["hasUnapproved"], false);

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
async fn response_emits_valid_retry_after_and_drops_invalid_header_values() {
    let server = start(Arc::new(CaptureObserver::default())).await;
    let client = reqwest::Client::new();
    let url = format!("http://{}/v1/responses", server.local_addr());

    let retry = client
        .post(&url)
        .header("authorization", "Bearer valid")
        .body("retry")
        .send()
        .await
        .unwrap();
    assert_eq!(retry.status(), 503);
    assert_eq!(retry.headers()["retry-after"], "12");

    let invalid = client
        .post(&url)
        .header("authorization", "Bearer valid")
        .body("invalid-retry")
        .send()
        .await
        .unwrap();
    assert_eq!(invalid.status(), 503);
    assert!(invalid.headers().get("retry-after").is_none());
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
        observer.clone(),
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

    let failed = client
        .post(&url)
        .header("authorization", "Bearer valid")
        .body("handler-error")
        .send()
        .await
        .unwrap();
    assert_eq!(failed.status(), 502);
    let after_failure = client
        .post(&url)
        .header("authorization", "Bearer valid")
        .body("fast")
        .send()
        .await
        .unwrap();
    assert_eq!(after_failure.status(), 200);

    let upstream_limited = client
        .post(&url)
        .header("authorization", "Bearer valid")
        .body("upstream-limit")
        .send()
        .await
        .unwrap();
    assert_eq!(upstream_limited.status(), 502);

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
    let records = observer.0.lock().unwrap();
    assert!(records.iter().any(|record| {
        record.status == 429
            && record.outcome_code.as_deref() == Some("GATEWAY_CONCURRENCY_LIMIT")
            && record.rejection_reason == Some(GatewayRejectionReason::GlobalQueueFull)
    }));
    assert!(records.iter().any(|record| {
        record.status == 504 && record.timeout_stage == Some(GatewayTimeoutStage::Handler)
    }));
    assert!(records.iter().any(|record| {
        record.rejection_reason == Some(GatewayRejectionReason::UpstreamConcurrency)
    }));
    drop(records);
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn bounded_queue_waits_once_and_rejects_overflow_per_binding() {
    let observer = Arc::new(CaptureObserver::default());
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
        observer.clone(),
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
    let encoded = serde_json::to_string(&*observer.0.lock().unwrap()).unwrap();
    for forbidden in [
        "binding-a",
        "profile-a",
        "deepseek",
        "api.deepseek.com",
        "Bearer valid",
    ] {
        assert!(!encoded.contains(forbidden), "leaked {forbidden}");
    }
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn busy_binding_waiter_does_not_reserve_capacity_needed_by_another_binding() {
    let handler = Arc::new(BindingIsolationHandler::default());
    let server = GatewayLoopbackServer::start(
        GatewayServerConfig {
            max_inflight: 2,
            max_inflight_per_binding: 1,
            max_queue: 4,
            max_queue_per_binding: 2,
            request_timeout: Duration::from_secs(2),
            ..GatewayServerConfig::for_test()
        },
        Arc::new(MultiBindingAuth),
        handler.clone(),
        Arc::new(HealthProofKey::new(b"0123456789abcdef0123456789abcdef").unwrap()),
        Arc::new(CaptureObserver::default()),
    )
    .await
    .unwrap();
    let client = reqwest::Client::new();
    let url = format!("http://{}/v1/responses", server.local_addr());

    let first_a = tokio::spawn({
        let client = client.clone();
        let url = url.clone();
        async move {
            client
                .post(url)
                .header("authorization", "Bearer binding-a")
                .body("hold-binding-a")
                .send()
                .await
                .unwrap()
        }
    });
    tokio::time::timeout(Duration::from_secs(1), handler.binding_a_started.notified())
        .await
        .unwrap();

    let second_a = tokio::spawn({
        let client = client.clone();
        let url = url.clone();
        async move {
            client
                .post(url)
                .header("authorization", "Bearer binding-a")
                .body("queued-binding-a")
                .send()
                .await
                .unwrap()
        }
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        while server.activity().inflight_requests() < 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    let queued = server.activity().snapshot();
    assert_eq!(queued.running_requests, 1);
    assert_eq!(queued.queued_requests, 1);
    assert_eq!(queued.queued_body_bytes, b"queued-binding-a".len());

    let binding_b = tokio::time::timeout(
        Duration::from_millis(150),
        client
            .post(&url)
            .header("authorization", "Bearer binding-b")
            .body("binding-b")
            .send(),
    )
    .await;
    handler.release_binding_a.notify_one();
    assert_eq!(first_a.await.unwrap().status(), 200);
    assert_eq!(second_a.await.unwrap().status(), 200);
    let binding_b = binding_b.expect("Binding B was blocked by Binding A's waiter");
    assert_eq!(binding_b.unwrap().status(), 200);
    tokio::time::timeout(Duration::from_secs(1), async {
        while server.activity().snapshot().inflight_requests != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(server.activity().snapshot().running_requests, 0);
    assert_eq!(server.activity().snapshot().queued_requests, 0);
    assert_eq!(server.activity().snapshot().queued_body_bytes, 0);
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn streaming_activity_tracks_completion_and_early_client_drop() {
    let handler = Arc::new(StreamingLifecycleHandler::default());
    let server = GatewayLoopbackServer::start(
        GatewayServerConfig::for_test(),
        Arc::new(FakeAuth),
        handler.clone(),
        Arc::new(HealthProofKey::new(b"0123456789abcdef0123456789abcdef").unwrap()),
        Arc::new(CaptureObserver::default()),
    )
    .await
    .unwrap();
    let response = reqwest::Client::new()
        .post(format!("http://{}/v1/responses", server.local_addr()))
        .header("authorization", "Bearer valid")
        .body("stream")
        .send()
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), handler.started.notified())
        .await
        .unwrap();
    assert_eq!(server.activity().snapshot().active_streams, 1);

    drop(response);
    tokio::time::timeout(Duration::from_secs(1), async {
        while server.activity().snapshot().active_streams != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let snapshot = server.activity().snapshot();
    assert_eq!(snapshot.active_streams, 0);
    assert_eq!(snapshot.client_cancellations, 1);
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn streaming_activity_cancels_upstream_task_and_releases_capacity_on_client_drop() {
    let handler = Arc::new(UpstreamLifecycleHandler::new());
    let server = GatewayLoopbackServer::start(
        GatewayServerConfig::for_test(),
        Arc::new(FakeAuth),
        handler.clone(),
        Arc::new(HealthProofKey::new(b"0123456789abcdef0123456789abcdef").unwrap()),
        Arc::new(CaptureObserver::default()),
    )
    .await
    .unwrap();
    let response = reqwest::Client::new()
        .post(format!("http://{}/v1/responses", server.local_addr()))
        .header("authorization", "Bearer valid")
        .body("stream")
        .send()
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), handler.started.notified())
        .await
        .unwrap();
    assert_eq!(server.activity().snapshot().active_upstream_streams, 1);
    assert_eq!(handler.capacity.available_permits(), 0);

    drop(response);
    tokio::time::timeout(Duration::from_secs(1), async {
        while server.activity().snapshot().active_upstream_streams != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(handler.capacity.available_permits(), 1);
    server.shutdown().await.unwrap();
}

#[tokio::test]
async fn streaming_activity_releases_upstream_task_after_normal_completion() {
    let handler = Arc::new(UpstreamLifecycleHandler::new());
    let server = GatewayLoopbackServer::start(
        GatewayServerConfig::for_test(),
        Arc::new(FakeAuth),
        handler.clone(),
        Arc::new(HealthProofKey::new(b"0123456789abcdef0123456789abcdef").unwrap()),
        Arc::new(CaptureObserver::default()),
    )
    .await
    .unwrap();
    let response = reqwest::Client::new()
        .post(format!("http://{}/v1/responses", server.local_addr()))
        .header("authorization", "Bearer valid")
        .body("complete")
        .send()
        .await
        .unwrap();
    assert_eq!(response.bytes().await.unwrap(), b"done".as_slice());
    tokio::time::timeout(Duration::from_secs(1), async {
        while server.activity().snapshot().active_upstream_streams != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let activity = server.activity().snapshot();
    assert_eq!(activity.active_streams, 0);
    assert_eq!(activity.client_cancellations, 0);
    assert_eq!(handler.capacity.available_permits(), 1);
    server.shutdown().await.unwrap();
}
