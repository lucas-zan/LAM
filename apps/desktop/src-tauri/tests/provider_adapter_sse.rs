use localagentmanager_core::adapters::protocol::{parse_responses_request, ResponsesRequest};
use localagentmanager_core::adapters::protocol::{MAX_EVENT_CHANNEL_CAPACITY, MAX_SSE_FRAME_BYTES};
use localagentmanager_core::adapters::request::{
    translate_responses_request_with_context, CompatibilityPolicy,
};
use localagentmanager_core::adapters::sse::*;

fn chunk(
    delta: serde_json::Value,
    finish: Option<&str>,
    usage: Option<serde_json::Value>,
) -> String {
    serde_json::json!({
      "id":"chat-stream","object":"chat.completion.chunk","created":100,"model":"bound-model",
      "choices":[{"index":0,"delta":delta,"finish_reason":finish}], "usage":usage
    })
    .to_string()
}

fn text_stream_bytes() -> Vec<u8> {
    let frames = [
        chunk(serde_json::json!({"role":"assistant"}), None, None),
        chunk(serde_json::json!({"content":"你"}), None, None),
        chunk(serde_json::json!({"content":"好"}), Some("stop"), None),
        serde_json::json!({
          "id":"chat-stream","object":"chat.completion.chunk","created":100,"model":"bound-model",
          "choices":[],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}
        })
        .to_string(),
    ];
    let mut wire = frames
        .iter()
        .map(|frame| format!("data: {frame}\n\n"))
        .collect::<String>();
    wire.push_str("data: [DONE]\n\n");
    wire.into_bytes()
}

fn run_partition(partitions: &[usize]) -> Vec<ResponsesStreamEvent> {
    let wire = text_stream_bytes();
    let mut adapter = StreamingAdapter::new("resp-stream", "bound-model", 1_700_000_000);
    let mut events = adapter.start().unwrap();
    let mut cursor = 0;
    for size in partitions {
        if cursor == wire.len() {
            break;
        }
        let end = (cursor + size).min(wire.len());
        events.extend(adapter.push_bytes(&wire[cursor..end]).unwrap());
        cursor = end;
    }
    if cursor < wire.len() {
        events.extend(adapter.push_bytes(&wire[cursor..]).unwrap());
    }
    adapter.end_of_stream().unwrap();
    events
}

#[test]
fn text_stream_emits_verified_responses_event_order_and_stable_ids() {
    let events = run_partition(&[usize::MAX]);
    assert_eq!(
        events
            .iter()
            .map(ResponsesStreamEvent::kind)
            .collect::<Vec<_>>(),
        [
            "response.created",
            "response.output_item.added",
            "response.content_part.added",
            "response.output_text.delta",
            "response.output_text.delta",
            "response.output_text.done",
            "response.content_part.done",
            "response.output_item.done",
            "response.completed"
        ]
    );
    assert!(events
        .iter()
        .all(|event| event.response_id() == "resp-stream"));
    assert_eq!(
        events
            .iter()
            .filter_map(ResponsesStreamEvent::text_delta)
            .collect::<String>(),
        "你好"
    );
    let completed = events.last().unwrap().completed_response().unwrap();
    assert_eq!(completed.usage.as_ref().unwrap().total_tokens, 5);
}

#[test]
fn arbitrary_byte_and_utf8_partitions_have_identical_semantics() {
    let whole = serde_json::to_value(run_partition(&[usize::MAX])).unwrap();
    for sizes in [vec![1], vec![2, 3, 5, 7, 11], vec![17, 1, 4, 2, 9]] {
        let partitioned = serde_json::to_value(run_partition(&sizes)).unwrap();
        assert_eq!(partitioned, whole);
    }
}

#[test]
fn malformed_oversized_drop_missing_done_and_duplicate_terminal_are_bounded() {
    let mut malformed = StreamingAdapter::new("r", "bound-model", 1);
    malformed.start().unwrap();
    let error = malformed.push_bytes(b"bogus: value\n\n").unwrap_err();
    assert_eq!(error.code, StreamErrorCode::MalformedSse);
    assert_eq!(malformed.state(), StreamState::Failed);
    assert_eq!(malformed.failure_events().len(), 1);

    let mut oversized = StreamingAdapter::new("r", "bound-model", 1);
    oversized.start().unwrap();
    let error = oversized
        .push_bytes(&vec![b'x'; MAX_SSE_FRAME_BYTES + 1])
        .unwrap_err();
    assert_eq!(error.code, StreamErrorCode::FrameLimitExceeded);

    let mut dropped = StreamingAdapter::new("r", "bound-model", 1);
    dropped.start().unwrap();
    dropped
        .push_bytes(
            format!(
                "data: {}\n\n",
                chunk(serde_json::json!({"content":"partial"}), None, None)
            )
            .as_bytes(),
        )
        .unwrap();
    assert_eq!(
        dropped.end_of_stream().unwrap_err().code,
        StreamErrorCode::MissingDone
    );
    assert_eq!(dropped.failure_events().len(), 1);

    let mut duplicate = StreamingAdapter::new("r", "bound-model", 1);
    duplicate.start().unwrap();
    duplicate
        .push_bytes(
            format!(
                "data: {}\n\n",
                chunk(serde_json::json!({}), Some("stop"), None)
            )
            .as_bytes(),
        )
        .unwrap();
    duplicate.push_bytes(b"data: [DONE]\n\n").unwrap();
    assert_eq!(
        duplicate.push_bytes(b"data: [DONE]\n\n").unwrap_err().code,
        StreamErrorCode::AlreadyTerminal
    );
}

#[test]
fn incomplete_finish_reasons_usage_consistency_and_response_limit_are_enforced() {
    for (reason, expected) in [
        ("length", "max_output_tokens"),
        ("content_filter", "content_filter"),
    ] {
        let mut adapter = StreamingAdapter::new("r", "bound-model", 1);
        adapter.start().unwrap();
        let mut events = adapter
            .push_bytes(
                format!(
                    "data: {}\n\ndata: [DONE]\n\n",
                    chunk(serde_json::json!({"content":"partial"}), Some(reason), None)
                )
                .as_bytes(),
            )
            .unwrap();
        let terminal = events.pop().unwrap();
        assert_eq!(terminal.kind(), "response.incomplete");
        assert_eq!(
            terminal
                .completed_response()
                .unwrap()
                .incomplete_details
                .as_ref()
                .unwrap()
                .reason,
            expected
        );
    }

    let mut limited = StreamingAdapter::new("r", "bound-model", 1);
    limited.set_response_limit(4);
    limited.start().unwrap();
    let error = limited
        .push_bytes(
            format!(
                "data: {}\n\n",
                chunk(serde_json::json!({"content":"12345"}), None, None)
            )
            .as_bytes(),
        )
        .unwrap_err();
    assert_eq!(error.code, StreamErrorCode::ResponseLimitExceeded);

    let mut usage = StreamingAdapter::new("r", "bound-model", 1);
    usage.start().unwrap();
    let wire = serde_json::json!({
      "id":"chat","object":"chat.completion.chunk","created":1,"model":"bound-model","choices":[],
      "usage":{"prompt_tokens":2,"completion_tokens":2,"total_tokens":99}
    });
    assert_eq!(
        usage
            .push_bytes(format!("data: {wire}\n\n").as_bytes())
            .unwrap_err()
            .code,
        StreamErrorCode::ProtocolMismatch
    );
}

#[test]
fn cancellation_and_backpressure_have_single_terminal_outcomes() {
    for cancel_after_data in [false, true] {
        let mut adapter = StreamingAdapter::new("r", "bound-model", 1);
        adapter.start().unwrap();
        if cancel_after_data {
            adapter
                .push_bytes(
                    format!(
                        "data: {}\n\n",
                        chunk(serde_json::json!({"content":"x"}), None, None)
                    )
                    .as_bytes(),
                )
                .unwrap();
        }
        let terminal = adapter.cancel().unwrap();
        assert_eq!(terminal.kind(), "response.cancelled");
        assert_eq!(
            adapter.cancel().unwrap_err().code,
            StreamErrorCode::AlreadyTerminal
        );
    }

    let mut queue = BoundedEventQueue::new(MAX_EVENT_CHANNEL_CAPACITY, 1024 * 1024);
    for _ in 0..MAX_EVENT_CHANNEL_CAPACITY {
        queue
            .push(ResponsesStreamEvent::test_delta("r", "x"))
            .unwrap();
    }
    assert_eq!(
        queue
            .push(ResponsesStreamEvent::test_delta("r", "x"))
            .unwrap_err()
            .code,
        StreamErrorCode::BackpressureOverflow
    );

    let mut byte_limited = BoundedEventQueue::new(MAX_EVENT_CHANNEL_CAPACITY, 1);
    assert_eq!(
        byte_limited
            .push(ResponsesStreamEvent::test_delta("r", "x"))
            .unwrap_err()
            .code,
        StreamErrorCode::BackpressureOverflow
    );
}

#[test]
fn namespace_tool_stream_restores_original_name_and_namespace() {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "model":"m",
        "stream":true,
        "store":false,
        "tools":[{"type":"namespace","name":"remote","tools":[
            {"type":"function","name":"lookup","parameters":{"type":"object"}}
        ]}],
        "input":"lookup"
    }))
    .unwrap();
    let request: ResponsesRequest = parse_responses_request(&bytes).unwrap();
    let translated = translate_responses_request_with_context(
        &request,
        "m",
        &CompatibilityPolicy::generic_openai_compatible(),
    )
    .unwrap();
    let mut adapter =
        StreamingAdapter::new_with_tool_context("resp-namespace", "m", 1, translated.context);
    adapter.start().unwrap();
    let chunk = serde_json::json!({
      "id":"chat","object":"chat.completion.chunk","created":1,"model":"m",
      "choices":[{"index":0,"delta":{"tool_calls":[
        {"index":0,"id":"call-namespace","type":"function","function":{"name":"remote__lookup","arguments":"{}"}}
      ]},"finish_reason":"tool_calls"}]
    });
    let mut events = adapter
        .push_bytes(format!("data: {chunk}\n\ndata: [DONE]\n\n").as_bytes())
        .unwrap();
    adapter.end_of_stream().unwrap();
    let terminal = events.pop().unwrap();
    let completed = terminal.completed_response().unwrap();
    assert!(matches!(
        &completed.output[0],
        localagentmanager_core::adapters::nonstream::ResponsesOutputItem::FunctionCall {
            name, namespace, ..
        } if name == "lookup" && namespace.as_deref() == Some("remote")
    ));
}

#[test]
fn streamed_tool_identity_may_arrive_after_arguments_without_data_loss() {
    let mut adapter = StreamingAdapter::new("resp-late-tool", "m", 1);
    adapter.start().unwrap();
    let frames = [
        serde_json::json!({
          "id":"chat","object":"chat.completion.chunk","created":1,"model":"m",
          "choices":[{"index":0,"delta":{"tool_calls":[
            {"index":0,"function":{"arguments":"{\"x\":"}}
          ]}}]
        }),
        serde_json::json!({
          "id":"chat","object":"chat.completion.chunk","created":1,"model":"m",
          "choices":[{"index":0,"delta":{"tool_calls":[
            {"index":0,"id":"call-late","type":"function","function":{"name":"lookup","arguments":"1}"}}
          ]},"finish_reason":"tool_calls"}]
        }),
    ];
    let mut events = Vec::new();
    for frame in frames {
        events.extend(
            adapter
                .push_bytes(format!("data: {frame}\n\n").as_bytes())
                .unwrap(),
        );
    }
    events.extend(adapter.push_bytes(b"data: [DONE]\n\n").unwrap());
    adapter.end_of_stream().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind() == "response.output_item.added")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter_map(|event| match event {
                ResponsesStreamEvent::FunctionCallArgumentsDelta { delta, .. } =>
                    Some(delta.as_str()),
                _ => None,
            })
            .collect::<String>(),
        "{\"x\":1}"
    );
    let terminal = events.pop().unwrap();
    let completed = terminal.completed_response().unwrap();
    assert!(matches!(
        &completed.output[0],
        localagentmanager_core::adapters::nonstream::ResponsesOutputItem::FunctionCall {
            call_id, name, arguments, ..
        } if call_id == "call-late" && name == "lookup" && arguments == "{\"x\":1}"
    ));
}

#[test]
fn streamed_custom_tool_call_uses_codex_custom_input_events() {
    let request: ResponsesRequest = parse_responses_request(
        &serde_json::to_vec(&serde_json::json!({
            "model":"m","stream":true,"store":false,
            "tools":[{"type":"custom","name":"apply_patch","description":"patch","format":{"type":"text"}}],
            "input":"edit"
        })).unwrap(),
    ).unwrap();
    let translated = translate_responses_request_with_context(
        &request,
        "m",
        &CompatibilityPolicy::generic_openai_compatible(),
    )
    .unwrap();
    let mut adapter =
        StreamingAdapter::new_with_tool_context("resp-custom", "m", 1, translated.context);
    adapter.start().unwrap();
    let frames = [
        serde_json::json!({
          "id":"chat","object":"chat.completion.chunk","created":1,"model":"m",
          "choices":[{"index":0,"delta":{"tool_calls":[
            {"index":0,"id":"custom-1","type":"function","function":{"name":"apply_patch","arguments":"{\"input\":\""}}
          ]}}]
        }),
        serde_json::json!({
          "id":"chat","object":"chat.completion.chunk","created":1,"model":"m",
          "choices":[{"index":0,"delta":{"tool_calls":[
            {"index":0,"function":{"arguments":"*** End Patch\"}"}}
          ]},"finish_reason":"tool_calls"}]
        }),
    ];
    let mut events = Vec::new();
    for frame in frames {
        events.extend(
            adapter
                .push_bytes(format!("data: {frame}\n\n").as_bytes())
                .unwrap(),
        );
    }
    events.extend(adapter.push_bytes(b"data: [DONE]\n\n").unwrap());
    adapter.end_of_stream().unwrap();
    assert!(events
        .iter()
        .any(|event| event.kind() == "response.custom_tool_call_input.delta"));
    let terminal = events.last().unwrap().completed_response().unwrap();
    assert!(matches!(
        &terminal.output[0],
        localagentmanager_core::adapters::nonstream::ResponsesOutputItem::CustomToolCall { input, .. }
            if input == "*** End Patch"
    ));
}

#[test]
fn generic_stream_tolerates_reasoning_extensions_and_legacy_function_finish() {
    let mut adapter = StreamingAdapter::new("resp-legacy", "bound-model", 1);
    adapter.start().unwrap();
    let frame = chunk(
        serde_json::json!({
            "reasoning_content":"provider-private",
            "function_call":{"name":"local_shell","arguments":"{\"command\":[\"pwd\"]}"}
        }),
        Some("function_call"),
        None,
    );
    let mut events = adapter
        .push_bytes(format!("data: {frame}\n\ndata: [DONE]\n\n").as_bytes())
        .unwrap();
    adapter.end_of_stream().unwrap();
    let terminal = events.pop().unwrap();
    let response = terminal.completed_response().unwrap();
    assert!(matches!(
        &response.output[0],
        localagentmanager_core::adapters::nonstream::ResponsesOutputItem::LocalShellCall { .. }
    ));
}
