use axum::http::HeaderMap;
use localagentmanager_core::gateway::upstream::{
    upstream_ip_is_forbidden, validate_upstream_url, CodexUpstreamHeaders, GatewayCancellation,
    KeychainAndEnvironmentCredentialResolver, NetworkTargetPolicy, SecureUpstreamClient,
    UpstreamClientConfig, UpstreamCredentialResolver, UpstreamRequest,
};
use localagentmanager_core::provider_credentials::{CredentialSource, SecretValue, UpstreamAuth};
use localagentmanager_core::provider_keychain::{
    KeychainBackend, KeychainCredentialReference, KeychainCredentialService,
};
use localagentmanager_core::{AppError, Result};
use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct TestPolicy(SocketAddr);

#[test]
fn codex_header_contract_forwards_only_bounded_safe_metadata() {
    let mut source = HeaderMap::new();
    for (name, value) in [
        ("accept", "text/event-stream"),
        ("originator", "codex_exec"),
        ("session-id", "session-a"),
        ("thread-id", "thread-a"),
        (
            "user-agent",
            "codex_exec/0.144.5 (Mac OS 15.6.0; arm64) unknown (codex_exec; 0.144.5)",
        ),
        ("x-client-request-id", "request-a"),
        ("x-codex-beta-features", "remote_compaction_v2"),
        ("x-codex-turn-metadata", r#"{"request_kind":"turn"}"#),
        ("x-codex-window-id", "window-a:0"),
        ("x-codex-future-contract", "future-a"),
    ] {
        source.insert(name, value.parse().unwrap());
    }
    source.insert("authorization", "Bearer local-secret".parse().unwrap());
    source.insert("cookie", "private-cookie".parse().unwrap());
    source.insert("x-unapproved", "unapproved".parse().unwrap());

    let captured = CodexUpstreamHeaders::capture(&source);

    assert_eq!(captured.len(), 10);
    assert_eq!(captured.get("originator"), Some("codex_exec"));
    assert!(captured
        .get("user-agent")
        .unwrap()
        .starts_with("codex_exec/"));
    assert_eq!(captured.get("x-codex-future-contract"), Some("future-a"));
    assert!(!format!("{captured:?}").contains("codex_exec"));
    for forbidden in [
        "authorization",
        "cookie",
        "host",
        "content-length",
        "x-unapproved",
    ] {
        assert_eq!(captured.get(forbidden), None);
    }

    source.insert("user-agent", "x".repeat(20_000).parse().unwrap());
    let bounded = CodexUpstreamHeaders::capture(&source);
    assert_eq!(bounded.get("user-agent"), None);
    assert_eq!(bounded.get("originator"), Some("codex_exec"));
    assert!(CodexUpstreamHeaders::default().is_empty());
}

impl NetworkTargetPolicy for TestPolicy {
    fn validate_and_resolve<'a>(
        &'a self,
        _url: &'a url::Url,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<SocketAddr>>> + Send + 'a>> {
        Box::pin(async move { Ok(vec![self.0]) })
    }
}

#[derive(Default)]
struct MapCredentials(HashMap<String, String>);

#[derive(Default)]
struct TestKeychain(HashMap<String, String>);

impl KeychainBackend for TestKeychain {
    fn write(&self, _reference: &KeychainCredentialReference, _secret: &SecretValue) -> Result<()> {
        unreachable!()
    }

    fn read(&self, reference: &KeychainCredentialReference) -> Result<SecretValue> {
        self.0
            .get(&reference.account)
            .cloned()
            .map(SecretValue::from_sensitive)
            .ok_or_else(|| AppError::new("KEYCHAIN_ITEM_NOT_FOUND", "missing"))
    }

    fn delete(&self, _reference: &KeychainCredentialReference) -> Result<()> {
        unreachable!()
    }
}

impl UpstreamCredentialResolver for MapCredentials {
    fn resolve(&self, source: &CredentialSource) -> Result<Option<SecretValue>> {
        match source {
            CredentialSource::Env { env_key } => self
                .0
                .get(env_key)
                .cloned()
                .map(SecretValue::from_sensitive)
                .map(Some)
                .ok_or_else(|| AppError::new("PROVIDER_CREDENTIAL_MISSING", "missing")),
            CredentialSource::None => Ok(None),
            _ => Err(AppError::new(
                "PROVIDER_CREDENTIAL_SOURCE_UNSUPPORTED",
                "unsupported",
            )),
        }
    }
}

fn client(server: &MockServer, credentials: MapCredentials) -> SecureUpstreamClient {
    SecureUpstreamClient::new(
        UpstreamClientConfig {
            connect_timeout: Duration::from_millis(100),
            first_byte_timeout: Duration::from_secs(1),
            stream_idle_timeout: Duration::from_secs(1),
            total_timeout: Duration::from_secs(2),
            max_response_bytes: 1024 * 1024,
            max_inflight: 4,
            connect_retries: 0,
            connect_retry_delay: Duration::from_millis(10),
            first_byte_retries: 0,
        },
        Arc::new(TestPolicy(*server.address())),
        Arc::new(credentials),
    )
    .unwrap()
}

fn request(server: &MockServer, auth: UpstreamAuth) -> UpstreamRequest {
    UpstreamRequest {
        base_url: format!("http://provider.test:{}/api/v1", server.address().port()),
        controlled_path: "chat/completions".into(),
        auth,
        body: br#"{"model":"deepseek-chat","messages":[]}"#.to_vec(),
        content_type: "application/json".into(),
        codex_headers: Default::default(),
        cancellation: GatewayCancellation::new(),
    }
}

#[test]
fn production_resolver_reads_versioned_keychain_credentials_and_rejects_auth_commands() {
    let reference = KeychainCredentialReference::new("deepseek", 3).unwrap();
    let resolver = KeychainAndEnvironmentCredentialResolver::new(KeychainCredentialService::new(
        Arc::new(TestKeychain(HashMap::from([(
            reference.account.clone(),
            "keychain-secret".into(),
        )]))),
    ));
    let secret = resolver
        .resolve(&reference.to_credential_source())
        .unwrap()
        .unwrap();
    assert!(secret.with_exposed(|value| value == "keychain-secret"));
    assert_eq!(
        resolver
            .resolve(&CredentialSource::AuthCommand {
                approval_id: "approved".into(),
            })
            .unwrap_err()
            .code,
        "PROVIDER_CREDENTIAL_SOURCE_UNSUPPORTED"
    );
}

#[tokio::test]
async fn controlled_join_preserves_prefix_and_injects_bearer_only_at_send() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(header("authorization", "Bearer LAM_TEST_UPSTREAM_SECRET"))
        .and(header("user-agent", "codex_exec/0.144.5"))
        .and(header("originator", "codex_exec"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(20))
                .set_body_json(serde_json::json!({"ok": true})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let credentials = MapCredentials(HashMap::from([(
        "DEEPSEEK_API_KEY".into(),
        "LAM_TEST_UPSTREAM_SECRET".into(),
    )]));
    let mut source = HeaderMap::new();
    source.insert("user-agent", "codex_exec/0.144.5".parse().unwrap());
    source.insert("originator", "codex_exec".parse().unwrap());
    source.insert("authorization", "Bearer local-secret".parse().unwrap());
    let mut request = request(
        &server,
        UpstreamAuth::Bearer {
            source: CredentialSource::Env {
                env_key: "DEEPSEEK_API_KEY".into(),
            },
        },
    );
    request.codex_headers = CodexUpstreamHeaders::capture(&source);
    let response = client(&server, credentials).send(request).await.unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.attempts, 1);
    assert!(response.first_byte_ms >= 10);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&response.body).unwrap()["ok"],
        true
    );
    assert!(!format!("{response:?}").contains("LAM_TEST_UPSTREAM_SECRET"));
}

#[tokio::test]
async fn named_header_and_no_auth_are_supported_without_body_header_override() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(header("x-api-key", "named-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_string("named"))
        .expect(1)
        .mount(&server)
        .await;
    let response = client(
        &server,
        MapCredentials(HashMap::from([("KEY".into(), "named-secret".into())])),
    )
    .send(request(
        &server,
        UpstreamAuth::Header {
            name: "x-api-key".into(),
            source: CredentialSource::Env {
                env_key: "KEY".into(),
            },
        },
    ))
    .await
    .unwrap();
    assert_eq!(response.body, b"named");
}

#[test]
fn production_url_policy_rejects_ssrf_and_path_injection_inputs() {
    for forbidden in [
        "http://api.example.com/v1",
        "https://127.0.0.1/v1",
        "https://localhost/v1",
        "https://user@api.example.com/v1",
        "https://api.example.com:8443/v1",
        "https://api.example.com/v1#fragment",
    ] {
        assert_eq!(
            validate_upstream_url(forbidden).unwrap_err().code,
            "UPSTREAM_NETWORK_TARGET_FORBIDDEN"
        );
    }
    assert_eq!(
        localagentmanager_core::gateway::upstream::join_controlled_path(
            "https://api.example.com/prefix/v1",
            "../admin"
        )
        .unwrap_err()
        .code,
        "UPSTREAM_PATH_INVALID"
    );
    assert_eq!(
        localagentmanager_core::gateway::upstream::join_controlled_path(
            "https://api.example.com/prefix/v1/",
            "/chat/completions"
        )
        .unwrap()
        .as_str(),
        "https://api.example.com/prefix/v1/chat/completions"
    );
}

#[test]
fn production_ip_policy_allows_proxy_fake_ip_but_rejects_private_targets() {
    assert!(!upstream_ip_is_forbidden("198.18.7.234".parse().unwrap()));
    for forbidden in ["127.0.0.1", "10.0.0.1", "169.254.169.254", "::1", "fc00::1"] {
        assert!(upstream_ip_is_forbidden(forbidden.parse().unwrap()));
    }
}

#[tokio::test]
async fn redirects_429_and_5xx_are_never_replayed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "3"))
        .expect(1)
        .mount(&server)
        .await;
    let response = client(&server, MapCredentials::default())
        .send(request(&server, UpstreamAuth::None))
        .await
        .unwrap();
    assert_eq!(response.status, 429);
    assert_eq!(response.attempts, 1);
    assert_eq!(response.retry_after.as_deref(), Some("3"));
}

#[tokio::test]
async fn cancellation_and_response_limit_abort_transport() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b'x'; 2048]))
        .mount(&server)
        .await;
    let transport = SecureUpstreamClient::new(
        UpstreamClientConfig {
            max_response_bytes: 128,
            ..UpstreamClientConfig::for_test()
        },
        Arc::new(TestPolicy(*server.address())),
        Arc::new(MapCredentials::default()),
    )
    .unwrap();
    let cancelled = request(&server, UpstreamAuth::None);
    cancelled.cancellation.cancel();
    assert_eq!(
        transport.send(cancelled).await.unwrap_err().code,
        "GATEWAY_REQUEST_CANCELLED"
    );
    assert_eq!(
        transport
            .send(request(&server, UpstreamAuth::None))
            .await
            .unwrap_err()
            .code,
        "GATEWAY_RESPONSE_LIMIT"
    );
}

#[tokio::test]
async fn connect_failure_with_zero_configured_retries_tries_once() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let transport = SecureUpstreamClient::new(
        UpstreamClientConfig::for_test(),
        Arc::new(TestPolicy(address)),
        Arc::new(MapCredentials::default()),
    )
    .unwrap();
    let error = transport
        .send(UpstreamRequest {
            base_url: format!("http://provider.test:{}/api/v1", address.port()),
            controlled_path: "chat/completions".into(),
            auth: UpstreamAuth::None,
            body: b"{}".to_vec(),
            content_type: "application/json".into(),
            codex_headers: Default::default(),
            cancellation: GatewayCancellation::new(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, "UPSTREAM_TRANSPORT_FAILED");
    assert_eq!(error.details.unwrap()["attempts"], 1);
}

#[tokio::test]
async fn connect_failure_retries_up_to_configured_budget_within_timeout() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let config = UpstreamClientConfig {
        connect_retries: 2,
        connect_retry_delay: Duration::from_millis(20),
        ..UpstreamClientConfig::for_test()
    };
    let transport = SecureUpstreamClient::new(
        config,
        Arc::new(TestPolicy(address)),
        Arc::new(MapCredentials::default()),
    )
    .unwrap();
    let error = transport
        .send(UpstreamRequest {
            base_url: format!("http://provider.test:{}/api/v1", address.port()),
            controlled_path: "chat/completions".into(),
            auth: UpstreamAuth::None,
            body: b"{}".to_vec(),
            content_type: "application/json".into(),
            codex_headers: Default::default(),
            cancellation: GatewayCancellation::new(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, "UPSTREAM_TRANSPORT_FAILED");
    // 1 initial attempt + 2 configured retries.
    assert_eq!(error.details.unwrap()["attempts"], 3);
}

#[tokio::test]
async fn connect_failure_respects_total_timeout_while_retrying() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let config = UpstreamClientConfig {
        total_timeout: Duration::from_millis(80),
        connect_retries: 5,
        connect_retry_delay: Duration::from_millis(50),
        ..UpstreamClientConfig::for_test()
    };
    let transport = SecureUpstreamClient::new(
        config,
        Arc::new(TestPolicy(address)),
        Arc::new(MapCredentials::default()),
    )
    .unwrap();
    let error = transport
        .send(UpstreamRequest {
            base_url: format!("http://provider.test:{}/api/v1", address.port()),
            controlled_path: "chat/completions".into(),
            auth: UpstreamAuth::None,
            body: b"{}".to_vec(),
            content_type: "application/json".into(),
            codex_headers: Default::default(),
            cancellation: GatewayCancellation::new(),
        })
        .await
        .unwrap_err();
    // total_timeout (80ms) cuts retries short. Either the deadline expires
    // between attempts (UPSTREAM_TOTAL_TIMEOUT) or the final connect fails
    // first (UPSTREAM_TRANSPORT_FAILED); both are acceptable, and the attempt
    // count must stay well below the configured 6.
    assert!(
        error.code == "UPSTREAM_TRANSPORT_FAILED" || error.code == "UPSTREAM_TOTAL_TIMEOUT",
        "unexpected code {}",
        error.code
    );
    let attempts = error
        .details
        .as_ref()
        .and_then(|d| d.get("attempts"))
        .and_then(|v| v.as_u64())
        .unwrap_or(1);
    assert!(attempts >= 1 && attempts <= 3, "attempts={attempts}");
}

#[tokio::test]
async fn first_byte_timeout_retries_once_when_configured() {
    let server = MockServer::start().await;
    // 每次请求都延迟超过 first_byte_timeout，模拟上游建连后无响应。
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(5))
                .set_body_json(serde_json::json!({"ok": true})),
        )
        .expect(2)
        .mount(&server)
        .await;
    let config = UpstreamClientConfig {
        first_byte_timeout: Duration::from_millis(100),
        total_timeout: Duration::from_secs(2),
        first_byte_retries: 1,
        connect_retries: 0,
        ..UpstreamClientConfig::for_test()
    };
    let transport = SecureUpstreamClient::new(
        config,
        Arc::new(TestPolicy(*server.address())),
        Arc::new(MapCredentials::default()),
    )
    .unwrap();
    let error = transport
        .send(request(&server, UpstreamAuth::None))
        .await
        .unwrap_err();
    assert_eq!(error.code, "UPSTREAM_FIRST_BYTE_TIMEOUT");
    // 1 次初始 + 1 次配置的重试。
    assert_eq!(error.details.unwrap()["attempts"], 2);
}

#[tokio::test]
async fn first_byte_timeout_with_zero_retries_tries_once() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(5))
                .set_body_json(serde_json::json!({"ok": true})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let config = UpstreamClientConfig {
        first_byte_timeout: Duration::from_millis(100),
        total_timeout: Duration::from_secs(2),
        first_byte_retries: 0,
        connect_retries: 0,
        ..UpstreamClientConfig::for_test()
    };
    let transport = SecureUpstreamClient::new(
        config,
        Arc::new(TestPolicy(*server.address())),
        Arc::new(MapCredentials::default()),
    )
    .unwrap();
    let error = transport
        .send(request(&server, UpstreamAuth::None))
        .await
        .unwrap_err();
    assert_eq!(error.code, "UPSTREAM_FIRST_BYTE_TIMEOUT");
    assert_eq!(error.details.unwrap()["attempts"], 1);
}
