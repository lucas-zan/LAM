use localagentmanager_core::adapters::protocol::*;
use localagentmanager_core::adapters::request::*;
use localagentmanager_core::adapters::sse::*;

fn parsed(value: serde_json::Value) -> ResponsesRequest {
    parse_responses_request(&serde_json::to_vec(&value).unwrap()).unwrap()
}

#[test]
fn function_definition_choice_and_full_history_preserve_call_identity() {
    let request = parsed(serde_json::json!({
      "model":"m","stream":true,"store":false,"parallel_tool_calls":true,
      "tool_choice":{"type":"function","name":"weather"},
      "tools":[{"type":"function","name":"weather","description":"lookup","parameters":{"type":"object"},"strict":true}],
      "input":[
        {"type":"message","role":"user","content":[{"type":"input_text","text":"weather"}]},
        {"type":"function_call","call_id":"call-1","name":"weather","arguments":"{\"city\":\"HZ\"}"},
        {"type":"function_call_output","call_id":"call-1","output":"sunny"},
        {"type":"message","role":"user","content":[{"type":"input_text","text":"and SH?"}]}
      ]
    }));
    let chat = translate_responses_request(
        &request,
        "m",
        &CompatibilityPolicy::generic_openai_compatible(),
    )
    .unwrap();
    assert_eq!(chat.tools[0].function.name, "weather");
    assert!(chat.tools[0].function.strict);
    assert_eq!(
        chat.tool_choice
            .as_ref()
            .and_then(ChatToolChoice::function_name),
        Some("weather")
    );
    assert!(chat.messages.iter().any(|message| matches!(message, ChatMessage::Assistant { tool_calls, .. } if tool_calls[0].id == "call-1")));
    assert!(chat.messages.iter().any(|message| matches!(message, ChatMessage::Tool { tool_call_id, content } if tool_call_id == "call-1" && content == "sunny")));
}

#[test]
fn streamed_interleaved_tool_arguments_produce_stable_function_items() {
    let frames = [
        serde_json::json!({"index":0,"id":"call-a","type":"function","function":{"name":"a","arguments":"{\"x\":"}}),
        serde_json::json!({"index":1,"id":"call-b","type":"function","function":{"name":"b","arguments":"{\"y\":"}}),
        serde_json::json!({"index":0,"function":{"arguments":"1}"}}),
        serde_json::json!({"index":1,"function":{"arguments":"2}"}}),
    ];
    let mut adapter = StreamingAdapter::new("resp-tools", "m", 1);
    let mut events = adapter.start().unwrap();
    for (index, delta) in frames.into_iter().enumerate() {
        let finish = (index == 3).then_some("tool_calls");
        let chunk = serde_json::json!({
          "id":"chat","object":"chat.completion.chunk","created":1,"model":"m",
          "choices":[{"index":0,"delta":{"tool_calls":[delta]},"finish_reason":finish}]
        });
        events.extend(
            adapter
                .push_bytes(format!("data: {chunk}\n\n").as_bytes())
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
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind() == "response.function_call_arguments.delta")
            .count(),
        4
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind() == "response.function_call_arguments.done")
            .count(),
        2
    );
    let completed = events.last().unwrap().completed_response().unwrap();
    assert!(
        matches!(&completed.output[0], localagentmanager_core::adapters::nonstream::ResponsesOutputItem::FunctionCall { call_id, arguments, .. } if call_id == "call-a" && arguments == "{\"x\":1}")
    );
    assert!(
        matches!(&completed.output[1], localagentmanager_core::adapters::nonstream::ResponsesOutputItem::FunctionCall { call_id, arguments, .. } if call_id == "call-b" && arguments == "{\"y\":2}")
    );
}

#[test]
fn consecutive_responses_tool_calls_share_one_chat_assistant_message() {
    let request = parsed(serde_json::json!({
      "model":"m","stream":false,"store":false,
      "input":[
        {"type":"message","role":"user","content":[{"type":"input_text","text":"run both"}]},
        {"type":"function_call","call_id":"call-a","name":"a","arguments":"{}"},
        {"type":"function_call","call_id":"call-b","name":"b","arguments":"{}"},
        {"type":"function_call_output","call_id":"call-a","output":"a"},
        {"type":"function_call_output","call_id":"call-b","output":"b"}
      ]
    }));
    let chat = translate_responses_request(
        &request,
        "m",
        &CompatibilityPolicy::generic_openai_compatible(),
    )
    .unwrap();
    let assistant_messages = chat
        .messages
        .iter()
        .filter_map(|message| match message {
            ChatMessage::Assistant { tool_calls, .. } => Some(tool_calls),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(assistant_messages.len(), 1);
    assert_eq!(assistant_messages[0].len(), 2);
    assert_eq!(assistant_messages[0][1].id, "call-b");
}

#[test]
fn malformed_linkage_arguments_limits_and_unsupported_tools_fail_preflight() {
    let result_first = parsed(serde_json::json!({
      "model":"m","stream":true,"store":false,
      "input":[{"type":"function_call_output","call_id":"missing","output":"x"}]
    }));
    assert_eq!(
        translate_responses_request(
            &result_first,
            "m",
            &CompatibilityPolicy::generic_openai_compatible()
        )
        .unwrap_err()
        .code,
        AdapterRequestErrorCode::UnknownCallId
    );

    let duplicate = parsed(serde_json::json!({
      "model":"m","stream":true,"store":false,
      "input":[
        {"type":"function_call","call_id":"same","name":"f","arguments":"{}"},
        {"type":"function_call","call_id":"same","name":"f","arguments":"{}"}
      ]
    }));
    assert_eq!(
        translate_responses_request(
            &duplicate,
            "m",
            &CompatibilityPolicy::generic_openai_compatible()
        )
        .unwrap_err()
        .code,
        AdapterRequestErrorCode::DuplicateCallId
    );

    let malformed = parsed(serde_json::json!({
      "model":"m","stream":true,"store":false,
      "input":[{"type":"function_call","call_id":"c","name":"f","arguments":"{"}]
    }));
    assert_eq!(
        translate_responses_request(
            &malformed,
            "m",
            &CompatibilityPolicy::generic_openai_compatible()
        )
        .unwrap_err()
        .code,
        AdapterRequestErrorCode::ToolArgumentsInvalid
    );
}
