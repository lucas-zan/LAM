use localagentmanager_core::provider_model_discovery::{
    discover_provider_models_service_v2, parse_openai_model_list, DiscoverProviderModelsRequestV2,
    MAX_DISCOVERED_MODELS, MAX_MODELS_RESPONSE_BYTES,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn server(response: String) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = vec![0_u8; 16 * 1024];
        let read = socket.read(&mut bytes).unwrap();
        sender
            .send(String::from_utf8_lossy(&bytes[..read]).into_owned())
            .unwrap();
        socket.write_all(response.as_bytes()).unwrap();
    });
    (format!("http://{address}/v1"), receiver, handle)
}

fn http_response(status: &str, body: &str, extra_headers: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n{extra_headers}connection: close\r\n\r\n{body}",
        body.len()
    )
}

#[test]
fn parser_accepts_only_standard_data_and_returns_sorted_models() {
    let body = br#"{
        "object":"list",
        "data":[
            {"id":"z-model","object":"model","owned_by":"vendor"},
            {"id":"a-model","created":1}
        ]
    }"#;
    let models = parse_openai_model_list(body).unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "a-model");
    assert_eq!(models[0].label, "a-model");
    assert_eq!(models[1].id, "z-model");
}

#[test]
fn parser_rejects_invalid_schema_ids_duplicates_empty_and_limits() {
    for (body, code) in [
        (
            br#"not-json"#.as_slice(),
            "PROVIDER_MODEL_DISCOVERY_JSON_INVALID",
        ),
        (
            br#"{"models":[{"id":"model-a"}]}"#.as_slice(),
            "PROVIDER_MODEL_DISCOVERY_SCHEMA_INVALID",
        ),
        (
            br#"{"data":[{"id":""}]}"#.as_slice(),
            "PROVIDER_MODEL_DISCOVERY_MODEL_INVALID",
        ),
        (
            br#"{"data":[{"id":" model-a "}]}"#.as_slice(),
            "PROVIDER_MODEL_DISCOVERY_MODEL_INVALID",
        ),
        (
            br#"{"data":[{"id":"same"},{"id":"same"}]}"#.as_slice(),
            "PROVIDER_MODEL_DISCOVERY_MODEL_DUPLICATE",
        ),
        (
            br#"{"data":[]}"#.as_slice(),
            "PROVIDER_MODEL_DISCOVERY_EMPTY",
        ),
    ] {
        assert_eq!(parse_openai_model_list(body).unwrap_err().code, code);
    }

    let oversized = vec![b' '; MAX_MODELS_RESPONSE_BYTES + 1];
    assert_eq!(
        parse_openai_model_list(&oversized).unwrap_err().code,
        "PROVIDER_MODEL_DISCOVERY_RESPONSE_LIMIT"
    );
    let data = (0..=MAX_DISCOVERED_MODELS)
        .map(|index| serde_json::json!({"id": format!("model-{index}")}))
        .collect::<Vec<_>>();
    let body = serde_json::to_vec(&serde_json::json!({"data": data})).unwrap();
    assert_eq!(
        parse_openai_model_list(&body).unwrap_err().code,
        "PROVIDER_MODEL_DISCOVERY_MODEL_LIMIT"
    );
}

#[test]
fn discovery_gets_the_standard_path_with_bearer_and_does_not_leak_debug_secret() {
    let body = r#"{"object":"list","data":[{"id":"model-a"}]}"#;
    let response = http_response("200 OK", body, "");
    let (base_url, requests, handle) = server(response);
    let request = DiscoverProviderModelsRequestV2 {
        base_url,
        api_key: "synthetic-write-only-key".into(),
    };
    assert!(!format!("{request:?}").contains("synthetic-write-only-key"));

    let discovered = discover_provider_models_service_v2(request).unwrap();
    assert_eq!(discovered.models[0].id, "model-a");
    let wire = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(wire.starts_with("GET /v1/models HTTP/1.1"));
    assert!(wire
        .to_ascii_lowercase()
        .contains("authorization: bearer synthetic-write-only-key"));
    handle.join().unwrap();
}

#[test]
fn discovery_rejects_redirect_http_error_and_blank_token() {
    let redirect = http_response(
        "302 Found",
        "",
        "location: http://127.0.0.1:9/redirected\r\n",
    );
    let (base_url, requests, handle) = server(redirect);
    let error = discover_provider_models_service_v2(DiscoverProviderModelsRequestV2 {
        base_url,
        api_key: "synthetic-redirect-key".into(),
    })
    .unwrap_err();
    assert_eq!(error.code, "PROVIDER_MODEL_DISCOVERY_HTTP_ERROR");
    assert!(requests.recv_timeout(Duration::from_secs(2)).is_ok());
    handle.join().unwrap();

    let response = http_response("401 Unauthorized", r#"{"secret":"must-not-leak"}"#, "");
    let (base_url, requests, handle) = server(response);
    let error = discover_provider_models_service_v2(DiscoverProviderModelsRequestV2 {
        base_url,
        api_key: "synthetic-error-key".into(),
    })
    .unwrap_err();
    assert_eq!(error.code, "PROVIDER_MODEL_DISCOVERY_HTTP_ERROR");
    assert!(!error.message.contains("must-not-leak"));
    assert!(requests.recv_timeout(Duration::from_secs(2)).is_ok());
    handle.join().unwrap();

    let error = discover_provider_models_service_v2(DiscoverProviderModelsRequestV2 {
        base_url: "https://api.example.test/v1".into(),
        api_key: "  ".into(),
    })
    .unwrap_err();
    assert_eq!(error.code, "PROVIDER_MODEL_DISCOVERY_CREDENTIAL_INVALID");
}
