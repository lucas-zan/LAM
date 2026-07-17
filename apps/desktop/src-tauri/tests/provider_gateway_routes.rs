use localagentmanager_core::gateway::binding::GatewayBindingSnapshot;
use localagentmanager_core::gateway::routes::GatewayRouteComposer;
use localagentmanager_core::gateway::server::{GatewayHttpRequest, GatewayRouteHandler};
use localagentmanager_core::gateway::upstream::{
    NetworkTargetPolicy, SecureUpstreamClient, UpstreamClientConfig, UpstreamCredentialResolver,
};
use localagentmanager_core::provider_credentials::{CredentialSource, SecretValue, UpstreamAuth};
use localagentmanager_core::provider_keychain::KeychainCredentialReference;
use localagentmanager_core::provider_v2::{
    build_provider, AdapterConfig, CodexProviderOptions, ProviderInput, ProviderModel,
    ProviderProtocol,
};
use localagentmanager_core::{AppError, Result};
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct TestPolicy(SocketAddr);
impl NetworkTargetPolicy for TestPolicy {
    fn validate_and_resolve<'a>(
        &'a self,
        _url: &'a url::Url,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<SocketAddr>>> + Send + 'a>> {
        Box::pin(async move { Ok(vec![self.0]) })
    }
}
struct TestCredentials;
impl UpstreamCredentialResolver for TestCredentials {
    fn resolve(&self, source: &CredentialSource) -> Result<Option<SecretValue>> {
        match source {
            CredentialSource::Env { .. } => Ok(Some(SecretValue::from_sensitive(
                "test-upstream-key".into(),
            ))),
            CredentialSource::None => Ok(None),
            _ => Err(AppError::new(
                "PROVIDER_CREDENTIAL_SOURCE_UNSUPPORTED",
                "unsupported",
            )),
        }
    }
}

fn snapshot_at(address: SocketAddr) -> GatewayBindingSnapshot {
    let mut provider = build_provider(
        ProviderInput {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            protocol: ProviderProtocol::ChatCompletions,
            base_url: "https://provider.test/api/v1".into(),
            default_model: "deepseek-chat".into(),
            models: vec![
                ProviderModel {
                    id: "deepseek-chat".into(),
                    label: "Chat".into(),
                    capabilities: None,
                },
                ProviderModel {
                    id: "deepseek-reasoner".into(),
                    label: "Reasoner".into(),
                    capabilities: None,
                },
            ],
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
    provider.base_url = format!("http://provider.test:{}/api/v1", address.port());
    GatewayBindingSnapshot {
        binding_id: "binding-a".into(),
        profile_id: "profile-a".into(),
        provider_id: "deepseek".into(),
        selected_model: "deepseek-chat".into(),
        provider_revision: 1,
        provider,
        credential_reference: KeychainCredentialReference::new("gateway-route", 1).unwrap(),
        generation: 1,
    }
}

fn composer(server: &MockServer) -> GatewayRouteComposer {
    composer_at(*server.address())
}

fn composer_at(address: SocketAddr) -> GatewayRouteComposer {
    let upstream = SecureUpstreamClient::new(
        UpstreamClientConfig::for_test(),
        Arc::new(TestPolicy(address)),
        Arc::new(TestCredentials),
    )
    .unwrap();
    GatewayRouteComposer::new(Arc::new(upstream))
}

fn request(server: &MockServer, body: serde_json::Value) -> GatewayHttpRequest {
    request_at(*server.address(), body)
}

fn request_at(address: SocketAddr, body: serde_json::Value) -> GatewayHttpRequest {
    GatewayHttpRequest {
        path: "/v1/responses".into(),
        body: serde_json::to_vec(&body).unwrap(),
        binding: snapshot_at(address),
        request_id: "request-route-1".into(),
        upstream_headers: Default::default(),
    }
}

fn responses_request_at(address: SocketAddr, body: &[u8]) -> GatewayHttpRequest {
    let mut binding = snapshot_at(address);
    binding.provider.protocol = ProviderProtocol::Responses;
    binding.provider.adapter = AdapterConfig::None;
    binding.provider.compatibility_profile = None;
    binding.provider.codex.route_via_gateway = true;
    GatewayHttpRequest {
        path: "/v1/responses".into(),
        body: body.to_vec(),
        binding,
        request_id: "request-responses-1".into(),
        upstream_headers: Default::default(),
    }
}

#[tokio::test]
async fn nonstream_text_is_translated_through_real_upstream_transport() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/api/v1/chat/completions"))
        .and(body_partial_json(serde_json::json!({"model":"deepseek-chat","stream":false})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id":"chat-1","object":"chat.completion","created":1,"model":"deepseek-chat",
            "choices":[{"index":0,"message":{"role":"assistant","content":"hello"},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":2,"completion_tokens":1,"total_tokens":3}
        }))).expect(1).mount(&server).await;
    let response = composer(&server)
        .handle(request(
            &server,
            serde_json::json!({
                "model":"deepseek-chat","input":"hi","stream":false
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value =
        serde_json::from_slice(&response.collect_bytes().await.unwrap()).unwrap();
    assert_eq!(body["object"], "response");
    assert_eq!(body["output"][0]["content"][0]["text"], "hello");
}

#[tokio::test]
async fn stream_flushes_responses_sse_events_in_contract_order() {
    let server = MockServer::start().await;
    let chunks = [
        serde_json::json!({"id":"c","object":"chat.completion.chunk","created":1,"model":"deepseek-chat","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}),
        serde_json::json!({"id":"c","object":"chat.completion.chunk","created":1,"model":"deepseek-chat","choices":[{"index":0,"delta":{"content":"hello"},"finish_reason":"stop"}]}),
    ];
    let wire = format!(
        "data: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
        chunks[0], chunks[1]
    );
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(wire, "text/event-stream"))
        .expect(1)
        .mount(&server)
        .await;
    let response = composer(&server)
        .handle(request(
            &server,
            serde_json::json!({
                "model":"deepseek-chat","input":"hi","stream":true
            }),
        ))
        .await
        .unwrap();
    if response.content_type() != "text/event-stream" {
        panic!(
            "unexpected route response: {}",
            String::from_utf8(response.collect_bytes().await.unwrap()).unwrap()
        );
    }
    let wire = String::from_utf8(response.collect_bytes().await.unwrap()).unwrap();
    let created = wire.find("event: response.created").unwrap();
    let delta = wire.find("event: response.output_text.delta").unwrap();
    let completed = wire.find("event: response.completed").unwrap();
    assert!(created < delta && delta < completed);
    assert!(wire.contains("\"delta\":\"hello\""));
}

#[tokio::test]
async fn validation_rejects_model_switch_and_response_store_before_upstream() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    for body in [
        serde_json::json!({"model":"other-model","input":"hi","stream":false}),
        serde_json::json!({"model":"deepseek-chat","input":"hi","previous_response_id":"resp-old","stream":false}),
    ] {
        let response = composer(&server)
            .handle(request(&server, body))
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
        let error: serde_json::Value =
            serde_json::from_slice(&response.collect_bytes().await.unwrap()).unwrap();
        assert!(error["error"]["code"]
            .as_str()
            .unwrap()
            .starts_with("ADAPTER_"));
    }
}

#[tokio::test]
async fn models_route_returns_only_authenticated_binding_metadata() {
    let server = MockServer::start().await;
    let mut request = request(&server, serde_json::json!({}));
    request.path = "/v1/models".into();
    request.body.clear();
    let response = composer(&server).handle(request).await.unwrap();
    let body: serde_json::Value =
        serde_json::from_slice(&response.collect_bytes().await.unwrap()).unwrap();
    let models = body["models"].as_array().unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0]["slug"], "deepseek-chat");
    assert_eq!(models[1]["slug"], "deepseek-reasoner");
    for model in models {
        for field in [
            "slug",
            "display_name",
            "supported_reasoning_levels",
            "shell_type",
            "visibility",
            "supported_in_api",
            "priority",
            "base_instructions",
            "supports_reasoning_summaries",
            "support_verbosity",
            "truncation_policy",
            "supports_parallel_tool_calls",
            "experimental_supported_tools",
        ] {
            assert!(model.get(field).is_some(), "missing Codex field {field}");
        }
        assert!(model.get("id").is_none());
        assert!(model.get("owned_by").is_none());
    }
}

#[tokio::test]
async fn second_allowlisted_model_reaches_upstream_and_is_reported_in_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(body_partial_json(
            serde_json::json!({"model":"deepseek-reasoner","stream":false}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id":"chat-2","object":"chat.completion","created":1,"model":"deepseek-reasoner",
            "choices":[{"index":0,"message":{"role":"assistant","content":"reasoned"},"finish_reason":"stop"}]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let response = composer(&server)
        .handle(request(
            &server,
            serde_json::json!({
                "model":"deepseek-reasoner","input":"hi","stream":false
            }),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: serde_json::Value =
        serde_json::from_slice(&response.collect_bytes().await.unwrap()).unwrap();
    assert_eq!(body["model"], "deepseek-reasoner");
}

#[tokio::test]
async fn changing_binding_default_does_not_hide_other_provider_models() {
    let server = MockServer::start().await;
    let mut request = request(&server, serde_json::json!({}));
    request.path = "/v1/models".into();
    request.body.clear();
    request.binding.selected_model = "deepseek-reasoner".into();

    let response = composer(&server).handle(request).await.unwrap();
    let body: serde_json::Value =
        serde_json::from_slice(&response.collect_bytes().await.unwrap()).unwrap();
    let slugs = body["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|model| model["slug"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(slugs, vec!["deepseek-chat", "deepseek-reasoner"]);
}

#[tokio::test]
async fn responses_provider_preserves_nonstream_request_status_type_and_body() {
    let server = MockServer::start().await;
    let request_body = br#"{ "model": "deepseek-reasoner", "input": "hi", "stream": false }"#;
    let response_body = br#"{"id":"resp-upstream","opaque":{"preserved":true}}"#;
    Mock::given(method("POST"))
        .and(path("/api/v1/responses"))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_raw(response_body.as_slice(), "application/json; charset=utf-8"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let response = composer(&server)
        .handle(responses_request_at(*server.address(), request_body))
        .await
        .unwrap();

    assert_eq!(response.status(), 201);
    assert_eq!(response.content_type(), "application/json; charset=utf-8");
    assert_eq!(response.collect_bytes().await.unwrap(), response_body);
    let received = server.received_requests().await.unwrap();
    assert_eq!(received[0].body, request_body);
}

#[tokio::test]
async fn responses_provider_wraps_nonstream_upstream_503_with_clear_source() {
    let server = MockServer::start().await;
    let overload = "system cpu overloaded (current: 97.8%, threshold: 90%)\n\t";
    Mock::given(method("POST"))
        .and(path("/api/v1/responses"))
        .respond_with(
            ResponseTemplate::new(503)
                .insert_header("retry-after", "12")
                .set_body_raw(overload, "text/plain"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let response = composer(&server)
        .handle(responses_request_at(
            *server.address(),
            br#"{"model":"deepseek-chat","input":"hi","stream":false}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), 503);
    assert_eq!(response.content_type(), "application/json");
    assert_eq!(response.retry_after(), Some("12"));
    let body: serde_json::Value =
        serde_json::from_slice(&response.collect_bytes().await.unwrap()).unwrap();
    assert_eq!(body["error"]["type"], "upstream_error");
    assert_eq!(body["error"]["source"], "upstream");
    assert_eq!(body["error"]["code"], "UPSTREAM_SERVICE_UNAVAILABLE");
    assert_eq!(body["error"]["upstreamStatus"], 503);
    assert_eq!(body["error"]["upstreamHost"], "provider.test");
    assert_eq!(body["error"]["providerId"], "deepseek");
    assert_eq!(body["error"]["requestId"], "request-responses-1");
    assert_eq!(body["error"]["retryable"], true);
    let message = body["error"]["message"].as_str().unwrap();
    assert!(message.starts_with(
        "Remote API service provider.test returned 503 Service Unavailable: system cpu overloaded"
    ));
    assert!(!message.contains('\n'));
    assert!(!message.contains("/api/v1"));
}

#[tokio::test]
async fn responses_provider_wraps_initial_stream_503_as_json() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/responses"))
        .respond_with(
            ResponseTemplate::new(503)
                .insert_header("retry-after", "Wed, 21 Oct 2026 07:28:00 GMT")
                .set_body_json(serde_json::json!({"error":{"message":"capacity exhausted"}})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let response = composer(&server)
        .handle(responses_request_at(
            *server.address(),
            br#"{"model":"deepseek-chat","input":"hi","stream":true}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), 503);
    assert_eq!(response.content_type(), "application/json");
    assert_eq!(
        response.retry_after(),
        Some("Wed, 21 Oct 2026 07:28:00 GMT")
    );
    let body: serde_json::Value =
        serde_json::from_slice(&response.collect_bytes().await.unwrap()).unwrap();
    assert_eq!(body["error"]["source"], "upstream");
    assert_eq!(body["error"]["code"], "UPSTREAM_SERVICE_UNAVAILABLE");
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .ends_with("capacity exhausted"));
}

#[tokio::test]
async fn responses_provider_bounds_empty_or_oversized_upstream_error_details() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/responses"))
        .respond_with(ResponseTemplate::new(502).set_body_raw("x".repeat(2_048), "text/plain"))
        .expect(1)
        .mount(&server)
        .await;

    let response = composer(&server)
        .handle(responses_request_at(
            *server.address(),
            br#"{"model":"deepseek-chat","input":"hi","stream":false}"#,
        ))
        .await
        .unwrap();
    let body: serde_json::Value =
        serde_json::from_slice(&response.collect_bytes().await.unwrap()).unwrap();

    assert_eq!(body["error"]["code"], "UPSTREAM_BAD_GATEWAY");
    assert!(body["error"]["message"].as_str().unwrap().len() < 700);

    let empty_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/responses"))
        .respond_with(ResponseTemplate::new(500).set_body_raw(" \n\t", "text/plain"))
        .expect(1)
        .mount(&empty_server)
        .await;
    let empty = composer(&empty_server)
        .handle(responses_request_at(
            *empty_server.address(),
            br#"{"model":"deepseek-chat","input":"hi","stream":false}"#,
        ))
        .await
        .unwrap();
    let empty_body: serde_json::Value =
        serde_json::from_slice(&empty.collect_bytes().await.unwrap()).unwrap();
    assert_eq!(empty_body["error"]["code"], "UPSTREAM_HTTP_ERROR");
    assert_eq!(empty_body["error"]["retryable"], false);
    assert_eq!(
        empty_body["error"]["message"],
        "Remote API service provider.test returned 500 Internal Server Error"
    );
}

#[tokio::test]
async fn responses_provider_passes_web_search_tool_through_unchanged() {
    let server = MockServer::start().await;
    let request_body = br#"{"model":"deepseek-chat","input":"hello","stream":false,"tools":[{"type":"web_search","search_context_size":"medium"}]}"#;
    Mock::given(method("POST"))
        .and(path("/api/v1/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id":"resp-web-search","output":[]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let response = composer(&server)
        .handle(responses_request_at(*server.address(), request_body))
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let received = server.received_requests().await.unwrap();
    assert_eq!(received[0].body, request_body);
}

#[tokio::test]
async fn responses_provider_passes_unmodeled_history_items_through_unchanged() {
    let server = MockServer::start().await;
    let request_body = br#"{"model":"deepseek-chat","stream":false,"input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]},{"type":"web_search_call","id":"ws_1","status":"completed","action":{"type":"search","query":"rust"}}]}"#;
    Mock::given(method("POST"))
        .and(path("/api/v1/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id":"resp-web-search-history","output":[]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let response = composer(&server)
        .handle(responses_request_at(*server.address(), request_body))
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let received = server.received_requests().await.unwrap();
    assert_eq!(received[0].body, request_body);
}

#[tokio::test]
async fn responses_provider_records_usage_from_nonstream_response() {
    let server = MockServer::start().await;
    let request_body = br#"{"model":"deepseek-chat","input":"hi","stream":false}"#;
    Mock::given(method("POST"))
        .and(path("/api/v1/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id":"resp-usage","output":[],
            "usage":{"input_tokens":11,"output_tokens":7,"total_tokens":18}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let response = composer(&server)
        .handle(responses_request_at(*server.address(), request_body))
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let usage = response
        .usage()
        .expect("passthrough usage must be recorded");
    assert_eq!(usage.input_tokens, 11);
    assert_eq!(usage.output_tokens, 7);
    assert_eq!(usage.total_tokens, 18);
}

#[tokio::test]
async fn responses_provider_preserves_sse_bytes_without_adapter_conversion() {
    let server = MockServer::start().await;
    let wire = b"event: response.created\ndata: {\"type\":\"response.created\"}\n\nevent: response.completed\ndata: {\"type\":\"response.completed\"}\n\n";
    Mock::given(method("POST"))
        .and(path("/api/v1/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(wire, "text/event-stream"))
        .expect(1)
        .mount(&server)
        .await;
    let request_body = br#"{"model":"deepseek-chat","input":"hi","stream":true}"#;

    let response = composer(&server)
        .handle(responses_request_at(*server.address(), request_body))
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(response.content_type(), "text/event-stream");
    assert_eq!(response.collect_bytes().await.unwrap(), wire);
}

#[tokio::test]
async fn responses_provider_emits_explicit_error_when_stream_ends_before_terminal_event() {
    let server = MockServer::start().await;
    let wire = b"event: response.created\ndata: {\"type\":\"response.created\"}\n\nevent: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n";
    Mock::given(method("POST"))
        .and(path("/api/v1/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(wire, "text/event-stream"))
        .expect(1)
        .mount(&server)
        .await;

    let response = composer(&server)
        .handle(responses_request_at(
            *server.address(),
            br#"{"model":"deepseek-chat","input":"compact history","stream":true}"#,
        ))
        .await
        .unwrap();
    let body = String::from_utf8(response.collect_bytes().await.unwrap().to_vec()).unwrap();
    assert!(body.starts_with(std::str::from_utf8(wire).unwrap()));
    assert!(body.contains("event: error"));
    assert!(body.contains("GATEWAY_UPSTREAM_STREAM_INCOMPLETE"));
    assert!(!body.contains("event: response.completed"));
}

#[tokio::test]
async fn responses_provider_accepts_failed_terminal_without_fabricating_completion_or_extra_error()
{
    let server = MockServer::start().await;
    let wire = b"event: response.created\ndata: {\"type\":\"response.created\"}\n\nevent: response.failed\ndata: {\"type\":\"response.failed\",\"response\":{\"status\":\"failed\"}}\n\n";
    Mock::given(method("POST"))
        .and(path("/api/v1/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(wire, "text/event-stream"))
        .mount(&server)
        .await;
    let response = composer(&server)
        .handle(responses_request_at(
            *server.address(),
            br#"{"model":"deepseek-chat","input":"hi","stream":true}"#,
        ))
        .await
        .unwrap();
    let body = response.collect_bytes().await.unwrap();
    assert_eq!(body, wire.as_slice());
}

#[tokio::test]
async fn local_compact_request_with_long_history_completes_over_the_normal_responses_route() {
    let server = MockServer::start().await;
    let wire = b"event: response.created\ndata: {\"type\":\"response.created\"}\n\nevent: response.output_text.done\ndata: {\"type\":\"response.output_text.done\",\"text\":\"compact summary\"}\n\nevent: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n";
    Mock::given(method("POST"))
        .and(path("/api/v1/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(wire, "text/event-stream"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/responses/compact"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let history = (0..128)
        .map(|index| {
            serde_json::json!({
                "type": "message",
                "role": if index % 2 == 0 { "user" } else { "assistant" },
                "content": [{"type":"input_text","text":format!("history item {index}")}]
            })
        })
        .collect::<Vec<_>>();
    let body = serde_json::to_vec(&serde_json::json!({
        "model": "deepseek-chat",
        "stream": true,
        "store": false,
        "instructions": "Summarize the conversation for continuation after compaction.",
        "input": history
    }))
    .unwrap();

    let response = composer(&server)
        .handle(responses_request_at(*server.address(), &body))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.collect_bytes().await.unwrap(), wire.as_slice());
    let received = server.received_requests().await.unwrap();
    let upstream: serde_json::Value = serde_json::from_slice(&received[0].body).unwrap();
    assert_eq!(upstream["input"].as_array().unwrap().len(), 128);
    assert!(upstream["instructions"]
        .as_str()
        .unwrap()
        .contains("compaction"));
}

#[tokio::test]
async fn automatic_compact_summary_request_uses_responses_when_compact_endpoint_is_absent() {
    let server = MockServer::start().await;
    let wire = b"event: response.created\ndata: {\"type\":\"response.created\"}\n\nevent: response.output_text.done\ndata: {\"type\":\"response.output_text.done\",\"text\":\"automatic compact summary\"}\n\nevent: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n";
    Mock::given(method("POST"))
        .and(path("/api/v1/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(wire, "text/event-stream"))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/responses/compact"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let history = (0..96)
        .map(|index| {
            serde_json::json!({
                "type": "message",
                "role": "user",
                "content": [{"type":"input_text","text":format!("automatic history {index}")}]
            })
        })
        .collect::<Vec<_>>();
    let body = serde_json::to_vec(&serde_json::json!({
        "model": "deepseek-chat",
        "stream": true,
        "store": false,
        "instructions": "Create a continuation summary because the automatic context threshold was reached.",
        "input": history
    }))
    .unwrap();

    let response = composer(&server)
        .handle(responses_request_at(*server.address(), &body))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.collect_bytes().await.unwrap(), wire.as_slice());
    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].url.path(), "/api/v1/responses");
}

#[tokio::test]
async fn responses_provider_rejects_successful_stream_with_invalid_content_type() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(r#"{"status":"not-a-stream"}"#, "application/json"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let request_body = br#"{"model":"deepseek-chat","input":"hi","stream":true}"#;

    let response = composer(&server)
        .handle(responses_request_at(*server.address(), request_body))
        .await
        .unwrap();

    assert_eq!(response.status(), 502);
    let body: serde_json::Value =
        serde_json::from_slice(&response.collect_bytes().await.unwrap()).unwrap();
    assert_eq!(
        body["error"]["code"],
        "GATEWAY_UPSTREAM_CONTENT_TYPE_INVALID"
    );
}

#[tokio::test]
async fn responses_provider_rejects_unlisted_model_and_server_side_state_before_upstream() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    for body in [
        br#"{"model":"unknown","input":"hi","stream":false}"#.as_slice(),
        br#"{"model":"deepseek-chat","input":"hi","store":true,"stream":false}"#.as_slice(),
        br#"{"model":"deepseek-chat","input":"hi","previous_response_id":"resp-old","stream":false}"#.as_slice(),
    ] {
        let response = composer(&server)
            .handle(responses_request_at(*server.address(), body))
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }
}

#[tokio::test]
async fn function_tool_and_followup_round_trip_preserve_call_identity() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(body_partial_json(serde_json::json!({"tools":[{"type":"function","function":{"name":"lookup"}}]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id":"chat-tool","object":"chat.completion","created":1,"model":"deepseek-chat",
            "choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[
                {"id":"call-1","type":"function","function":{"name":"lookup","arguments":"{\"q\":\"x\"}"}}
            ]},"finish_reason":"tool_calls"}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let first = composer(&server)
        .handle(request(
            &server,
            serde_json::json!({
                "model":"deepseek-chat","input":"find x","stream":false,
                "tools":[{"type":"function","name":"lookup","parameters":{"type":"object"}}]
            }),
        ))
        .await
        .unwrap();
    let first: serde_json::Value =
        serde_json::from_slice(&first.collect_bytes().await.unwrap()).unwrap();
    assert_eq!(first["output"][0]["call_id"], "call-1");

    let server2 = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id":"chat-final","object":"chat.completion","created":1,"model":"deepseek-chat",
            "choices":[{"index":0,"message":{"role":"assistant","content":"done"},"finish_reason":"stop"}]
        })))
        .expect(1)
        .mount(&server2)
        .await;
    let followup = composer(&server2)
        .handle(request(&server2, serde_json::json!({
            "model":"deepseek-chat","stream":false,"input":[
                {"type":"message","role":"user","content":[{"type":"input_text","text":"find x"}]},
                {"type":"function_call","call_id":"call-1","name":"lookup","arguments":"{\"q\":\"x\"}"},
                {"type":"function_call_output","call_id":"call-1","output":"result"}
            ]
        })))
        .await
        .unwrap();
    assert_eq!(followup.status(), 200);
    let received = server2.received_requests().await.unwrap();
    let translated: serde_json::Value = serde_json::from_slice(&received[0].body).unwrap();
    assert_eq!(translated["messages"][1]["tool_calls"][0]["id"], "call-1");
    assert_eq!(translated["messages"][2]["tool_call_id"], "call-1");
    assert_eq!(translated["messages"][2]["content"], "result");
}

#[tokio::test]
async fn malformed_sse_emits_one_sanitized_failed_terminal_event() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw("data: {not-json}\n\n", "text/event-stream"),
        )
        .mount(&server)
        .await;
    let response = composer(&server)
        .handle(request(
            &server,
            serde_json::json!({
                "model":"deepseek-chat","input":"hi","stream":true
            }),
        ))
        .await
        .unwrap();
    let wire = String::from_utf8(response.collect_bytes().await.unwrap()).unwrap();
    assert_eq!(wire.matches("event: response.failed").count(), 1);
    assert!(!wire.contains("not-json"));
}

#[tokio::test]
async fn upstream_transport_disconnect_emits_exactly_one_failed_terminal_event() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let upstream = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0_u8; 16 * 1024];
        let _ = socket.read(&mut request).await.unwrap();
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n10\r\ndata: {\"id\":1}\n\n\r\n",
            )
            .await
            .unwrap();
    });
    let response = composer_at(address)
        .handle(request_at(
            address,
            serde_json::json!({
                "model":"deepseek-chat","input":"hi","stream":true
            }),
        ))
        .await
        .unwrap();
    let wire = String::from_utf8(response.collect_bytes().await.unwrap()).unwrap();
    upstream.await.unwrap();
    assert_eq!(wire.matches("event: response.failed").count(), 1);
    assert_eq!(wire.matches("event: response.completed").count(), 0);
}

#[tokio::test]
async fn deepseek_thinking_with_tools_is_rejected_before_upstream_when_codex_history_cannot_carry_reasoning(
) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let response = composer(&server)
        .handle(request(
            &server,
            serde_json::json!({
                "model":"deepseek-chat",
                "input":"think and call",
                "stream":false,
                "reasoning":{"effort":"high"},
                "tools":[{"type":"function","name":"lookup","parameters":{"type":"object"}}]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    let error: serde_json::Value =
        serde_json::from_slice(&response.collect_bytes().await.unwrap()).unwrap();
    assert_eq!(
        error["error"]["code"],
        "ADAPTER_REASONING_HISTORY_UNREPRESENTABLE"
    );
}

#[test]
fn production_route_resolves_registry_exchange_and_never_selects_policy_from_model_name() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/services/gateway/routes.rs"),
    )
    .unwrap();
    assert!(source.contains("self.adapters.resolve"));
    assert!(source.contains("adapter.begin_exchange"));
    assert!(!source.contains("selected_model.contains"));
}
