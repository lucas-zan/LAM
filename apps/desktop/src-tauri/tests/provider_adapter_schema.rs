use localagentmanager_core::adapters::protocol::{
    extract_responses_usage, parse_responses_passthrough, parse_responses_request, ChatChunk,
    ChatCompletionResponse, ProtocolErrorCode, ResponsesInputItem, ResponsesTool,
    MAX_REQUEST_BYTES,
};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn fixture(relative: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/codex-gateway-contract/normal")
        .join(relative);
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn captured_codex_requests_parse_into_the_controlled_schema() {
    for name in [
        "text-stream-request.json",
        "function-tool-initial-request.json",
        "function-tool-followup-request.json",
        "resume-initial-request.json",
        "resume-request.json",
    ] {
        let envelope = fixture(name);
        let body = serde_json::to_vec(&envelope["body"]).unwrap();
        let request = parse_responses_request(&body).unwrap_or_else(|error| {
            panic!("{name} must parse: {error:?}");
        });
        assert_eq!(request.model, "fixture-model");
        assert!(request.stream);
        assert!(!request.store);
        assert_eq!(request.previous_response_id, None);
        assert!(!request.input.is_empty());
    }

    let followup = parse_responses_request(
        &serde_json::to_vec(&fixture("function-tool-followup-request.json")["body"]).unwrap(),
    )
    .unwrap();
    assert!(followup
        .input
        .iter()
        .any(|item| matches!(item, ResponsesInputItem::FunctionCall { .. })));
    assert!(followup
        .input
        .iter()
        .any(|item| matches!(item, ResponsesInputItem::FunctionCallOutput { .. })));
}

#[test]
fn protocol_types_round_trip_without_losing_null_or_empty_semantics() {
    let response: ChatCompletionResponse = serde_json::from_value(serde_json::json!({
        "id": "chat-1",
        "object": "chat.completion",
        "created": 1,
        "model": "fixture-model",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": null, "tool_calls": []},
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 2, "completion_tokens": 0, "total_tokens": 2}
    }))
    .unwrap();
    assert_eq!(response.choices[0].message.content, None);
    assert_eq!(
        response.choices[0]
            .message
            .tool_calls
            .as_ref()
            .unwrap()
            .len(),
        0
    );

    let chunk: ChatChunk = serde_json::from_value(serde_json::json!({
        "id": "chat-1", "object": "chat.completion.chunk", "created": 1,
        "model": "fixture-model", "choices": [],
        "usage": {"prompt_tokens": 2, "completion_tokens": 1, "total_tokens": 3}
    }))
    .unwrap();
    assert!(chunk.choices.is_empty());
    assert_eq!(chunk.usage.unwrap().total_tokens, 3);
}

#[test]
fn web_search_tool_parses_and_preserves_hosted_options() {
    let request = parse_responses_request(
        br#"{"model":"m","input":"hello","stream":true,"store":false,"tools":[{"type":"web_search","search_context_size":"medium","user_location":{"type":"approximate","country":"CN"}}]}"#,
    )
    .unwrap();

    let ResponsesTool::WebSearch { options } = &request.tools[0] else {
        panic!("expected web_search tool");
    };
    assert_eq!(options["search_context_size"], "medium");
    assert_eq!(options["user_location"]["country"], "CN");
    let encoded = serde_json::to_value(&request.tools[0]).unwrap();
    assert_eq!(encoded["type"], "web_search");
    assert_eq!(encoded["user_location"]["country"], "CN");
}

#[test]
fn reasoning_followup_parses_round_trips_and_redacts_debug_output() {
    let body = serde_json::json!({
        "model": "m", "stream": true, "store": false,
        "input": [
            {"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]},
            {"type":"reasoning","id":"rs_1","summary":[],"encrypted_content":"cipher-secret",
             "internal_chat_message_metadata_passthrough":{"turn_id":"turn-1"}},
            {"type":"function_call","call_id":"call_1","name":"shell","arguments":"{}"},
            {"type":"function_call_output","call_id":"call_1","output":"ok"}
        ]
    });

    let parsed = parse_responses_request(&serde_json::to_vec(&body).unwrap()).unwrap();
    let reasoning = &parsed.input[1];
    let ResponsesInputItem::Reasoning { options } = reasoning else {
        panic!("expected reasoning history");
    };
    assert_eq!(options["encrypted_content"], "cipher-secret");
    assert_eq!(
        options["internal_chat_message_metadata_passthrough"]["turn_id"],
        "turn-1"
    );
    assert_eq!(serde_json::to_value(reasoning).unwrap(), body["input"][1]);
    assert!(!format!("{reasoning:?}").contains("cipher-secret"));
}

#[test]
fn invalid_unknown_and_oversized_requests_report_stable_field_paths() {
    let unknown = br#"{"model":"m","input":"hi","stream":true,"store":false,"mystery":1}"#;
    let error = parse_responses_request(unknown).unwrap_err();
    assert_eq!(error.code, ProtocolErrorCode::UnknownField);
    assert_eq!(error.path.as_deref(), Some("$.mystery"));

    let wrong = br#"{"model":"m","input":[{"type":"message","role":"user","content":[{"type":"input_audio","audio_url":"x"}]}],"stream":true,"store":false}"#;
    let error = parse_responses_request(wrong).unwrap_err();
    assert_eq!(error.code, ProtocolErrorCode::UnsupportedInput);
    assert_eq!(error.path.as_deref(), Some("$.input[0].content[0].type"));

    let oversized = vec![b' '; MAX_REQUEST_BYTES + 1];
    let error = parse_responses_request(&oversized).unwrap_err();
    assert_eq!(error.code, ProtocolErrorCode::LimitExceeded);
    assert_eq!(error.path.as_deref(), Some("$"));
}

#[test]
fn input_image_content_parses_and_round_trips() {
    let body = serde_json::json!({
        "model": "vision-model",
        "input": [{
            "type": "message",
            "role": "user",
            "content": [
                {"type":"input_text","text":"describe"},
                {"type":"input_image","image_url":"data:image/png;base64,AA==","detail":"high"}
            ]
        }],
        "stream": true,
        "store": false
    });

    let parsed = parse_responses_request(&serde_json::to_vec(&body).unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&parsed.input[0]).unwrap(),
        body["input"][0]
    );
}

#[test]
fn passthrough_metadata_accepts_unmodeled_history_items_without_schema_validation() {
    // A `web_search_call` history item is rejected by the strict schema but must
    // pass the lenient passthrough parser used by the Responses-native route.
    let body = br#"{"model":"m","stream":true,"input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]},{"type":"web_search_call","id":"ws_1","status":"completed","action":{"type":"search","query":"rust"}}]}"#;
    assert!(parse_responses_request(body).is_err());

    let metadata = parse_responses_passthrough(body).unwrap();
    assert_eq!(metadata.model, "m");
    assert!(metadata.stream);
    assert!(!metadata.store);
    assert_eq!(metadata.previous_response_id, None);
    assert!(!metadata.input_empty);
}

#[test]
fn passthrough_metadata_reads_routing_fields_and_flags_empty_input() {
    let stored = parse_responses_passthrough(
        br#"{"model":"m","store":true,"previous_response_id":"resp_1","input":[]}"#,
    )
    .unwrap();
    assert!(stored.store);
    assert_eq!(stored.previous_response_id.as_deref(), Some("resp_1"));
    assert!(!stored.stream);
    assert!(stored.input_empty);

    let text = parse_responses_passthrough(br#"{"model":"m","input":"hi"}"#).unwrap();
    assert!(!text.input_empty);

    let missing = parse_responses_passthrough(br#"{"input":"hi"}"#).unwrap_err();
    assert_eq!(missing.code, ProtocolErrorCode::MissingField);
    assert_eq!(missing.path.as_deref(), Some("$.model"));

    let oversized = vec![b' '; MAX_REQUEST_BYTES + 1];
    let error = parse_responses_passthrough(&oversized).unwrap_err();
    assert_eq!(error.code, ProtocolErrorCode::LimitExceeded);
}

#[test]
fn responses_usage_extraction_reads_token_counts_and_defaults_total() {
    let usage = extract_responses_usage(&serde_json::json!({
        "id": "resp-1",
        "usage": {"input_tokens": 11, "output_tokens": 7, "total_tokens": 18}
    }))
    .unwrap();
    assert_eq!(usage.input_tokens, 11);
    assert_eq!(usage.output_tokens, 7);
    assert_eq!(usage.total_tokens, 18);

    let derived = extract_responses_usage(&serde_json::json!({
        "usage": {"input_tokens": 4, "output_tokens": 6}
    }))
    .unwrap();
    assert_eq!(derived.total_tokens, 10);

    assert!(extract_responses_usage(&serde_json::json!({"id": "resp-2"})).is_none());
    assert!(extract_responses_usage(&serde_json::json!({
        "usage": {"input_tokens": 0, "output_tokens": 0, "total_tokens": 0}
    }))
    .is_none());
}

#[test]
fn protocol_debug_redacts_message_and_schema_contents() {
    let request = parse_responses_request(
        br#"{"model":"m","input":"LAM_PRIVATE_PROMPT","stream":true,"store":false,"tools":[{"type":"function","name":"f","description":"secret description","parameters":{"type":"object"}}]}"#,
    )
    .unwrap();
    let debug = format!("{request:?}");
    assert!(!debug.contains("LAM_PRIVATE_PROMPT"));
    assert!(!debug.contains("secret description"));
    assert!(debug.contains("<redacted>"));
}
