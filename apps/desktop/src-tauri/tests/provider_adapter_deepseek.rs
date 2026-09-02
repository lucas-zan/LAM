use localagentmanager_core::adapters::deepseek::*;
use localagentmanager_core::adapters::nonstream::*;
use localagentmanager_core::adapters::protocol::*;
use localagentmanager_core::adapters::request::*;
use localagentmanager_core::adapters::sse::*;
use std::fs;
use std::path::PathBuf;

fn fixture(name: &str) -> String {
    fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/provider-adapter")
            .join(name),
    )
    .unwrap()
}

fn tool_followup() -> ResponsesRequest {
    parse_responses_request(
        br#"{
      "model":"deepseek-fixture-model","stream":true,"store":false,
      "reasoning":{"effort":"xhigh"},"tool_choice":"auto",
      "tools":[{"type":"function","name":"lookup","parameters":{"type":"object"}}],
      "input":[
        {"type":"message","role":"user","content":[{"type":"input_text","text":"lookup"}]},
        {"type":"function_call","call_id":"call-1","name":"lookup","arguments":"{}"},
        {"type":"function_call_output","call_id":"call-1","output":"one"},
        {"type":"function_call","call_id":"call-2","name":"lookup","arguments":"{}"},
        {"type":"function_call_output","call_id":"call-2","output":"two"}
      ]
    }"#,
    )
    .unwrap()
}

#[test]
fn typed_preset_maps_thinking_effort_and_safe_tool_choice_without_vendor_branches() {
    let preset = DeepSeekCompatibilityPreset::thinking_enabled();
    assert_eq!(preset.upstream_path, "/chat/completions");
    assert_eq!(preset.policy.id, "deepseek-chat-completions-v1");
    let source = tool_followup();
    let mut history = ReasoningHistory::new(MAX_REASONING_BYTES);
    history
        .record_tool_turn(&["call-1", "call-2"], "complete reasoning")
        .unwrap();
    let mut chat = translate_responses_request_with_history(
        &source,
        "deepseek-fixture-model",
        &preset.policy,
        &history,
    )
    .unwrap();
    preset.apply_request(&source, &mut chat).unwrap();
    assert_eq!(chat.thinking.as_ref().unwrap().kind, "enabled");
    assert_eq!(chat.reasoning_effort.as_deref(), Some("max"));
    assert!(
        chat.tool_choice.is_none(),
        "auto must be omitted because DeepSeek thinking rejects tool_choice"
    );
    assert!(chat
        .messages
        .iter()
        .filter_map(|message| match message {
            ChatMessage::Assistant {
                reasoning_content,
                tool_calls,
                ..
            } if !tool_calls.is_empty() => reasoning_content.as_deref(),
            _ => None,
        })
        .all(|reasoning| reasoning == "complete reasoning"));
}

#[test]
fn missing_or_oversized_tool_reasoning_fails_before_upstream() {
    let preset = DeepSeekCompatibilityPreset::thinking_enabled();
    let source = tool_followup();
    let error = translate_responses_request_with_history(
        &source,
        "deepseek-fixture-model",
        &preset.policy,
        &ReasoningHistory::new(MAX_REASONING_BYTES),
    )
    .unwrap_err();
    assert_eq!(
        error.code,
        AdapterRequestErrorCode::ReasoningHistoryRequired
    );

    let mut history = ReasoningHistory::new(4);
    let error = history.record_tool_turn(&["call-1"], "12345").unwrap_err();
    assert_eq!(error.code, DeepSeekErrorCode::AdapterReasoningLimitExceeded);
    assert_eq!(error.stable_code(), "ADAPTER_REASONING_LIMIT_EXCEEDED");
}

#[test]
fn nonstream_and_stream_reasoning_are_suppressed_but_preserved_for_history() {
    let chat: ChatCompletionResponse =
        serde_json::from_str(&fixture("deepseek-thinking-nonstream.json")).unwrap();
    let captured = capture_nonstream_reasoning(&chat, MAX_REASONING_BYTES)
        .unwrap()
        .unwrap();
    assert_eq!(
        captured.len(),
        "LAM_DEEPSEEK_REASONING_REDACTED_FIXTURE".len()
    );
    assert!(!format!("{captured:?}").contains("LAM_DEEPSEEK_REASONING"));
    let mut captured_history = ReasoningHistory::new(MAX_REASONING_BYTES);
    captured_history
        .record_tool_turn_record(&["call-1", "call-2"], captured)
        .unwrap();
    let replay = translate_responses_request_with_history(
        &tool_followup(),
        "deepseek-fixture-model",
        &DeepSeekCompatibilityPreset::thinking_enabled().policy,
        &captured_history,
    )
    .unwrap();
    assert!(replay.messages.iter().any(|message| matches!(
        message,
        ChatMessage::Assistant { reasoning_content: Some(reasoning), .. }
            if reasoning == "LAM_DEEPSEEK_REASONING_REDACTED_FIXTURE"
    )));
    let response = convert_nonstream_response(
        &chat,
        "deepseek-fixture-model",
        &mut DeterministicContext::new(1, "r"),
    )
    .unwrap();
    let encoded = serde_json::to_string(&response).unwrap();
    assert!(encoded.contains("LAM_DEEPSEEK_FINAL"));
    assert!(matches!(
        &response.output[0],
        localagentmanager_core::adapters::nonstream::ResponsesOutputItem::Reasoning {
            summary, ..
        } if summary[0]["text"] == "LAM_DEEPSEEK_REASONING_REDACTED_FIXTURE"
    ));

    let mut stream = StreamingAdapter::new("r", "deepseek-fixture-model", 1);
    stream.enable_reasoning_capture(MAX_REASONING_BYTES);
    let mut events = stream.start().unwrap();
    let wire = fixture("deepseek-thinking-stream.sse") + "\n";
    events.extend(stream.push_bytes(wire.as_bytes()).unwrap());
    stream.end_of_stream().unwrap();
    assert_eq!(
        stream.reasoning_content(),
        Some("LAM_DEEPSEEK_REASONING_REDACTED_FIXTURE")
    );
    let encoded = serde_json::to_string(&events).unwrap();
    assert!(encoded.contains("LAM_DEEPSEEK_FINAL"));
    assert!(encoded.contains("LAM_DEEPSEEK_REASONING"));
}

#[test]
fn generic_policy_forwards_effort_but_stream_rejects_deepseek_only_fields() {
    let source = tool_followup();
    let translated = translate_responses_request(
        &source,
        "deepseek-fixture-model",
        &CompatibilityPolicy::generic_openai_compatible(),
    )
    .unwrap();
    assert_eq!(translated.reasoning_effort.as_deref(), Some("xhigh"));
    assert!(translated.thinking.is_none());

    let mut generic_stream = StreamingAdapter::new("r", "deepseek-fixture-model", 1);
    generic_stream.start().unwrap();
    let first_frame = fixture("deepseek-thinking-stream.sse")
        .split("\n\n")
        .next()
        .unwrap()
        .to_owned()
        + "\n\n";
    let events = generic_stream.push_bytes(first_frame.as_bytes()).unwrap();
    assert!(!serde_json::to_string(&events)
        .unwrap()
        .contains("reasoning_content"));
    assert_eq!(generic_stream.reasoning_content(), None);
}

#[test]
fn disabled_thinking_is_explicit_and_does_not_fabricate_reasoning_fields() {
    let preset = DeepSeekCompatibilityPreset::thinking_disabled();
    let source =
        parse_responses_request(br#"{"model":"m","input":"hi","stream":true,"store":false}"#)
            .unwrap();
    let mut chat = translate_responses_request(&source, "m", &preset.policy).unwrap();
    preset.apply_request(&source, &mut chat).unwrap();
    assert_eq!(chat.thinking.as_ref().unwrap().kind, "disabled");
    assert_eq!(chat.reasoning_effort, None);
    let encoded = serde_json::to_string(&chat).unwrap();
    assert!(!encoded.contains("reasoning_content"));
    assert!(!encoded.contains("signature"));
    assert!(!encoded.contains("encrypted"));
}
