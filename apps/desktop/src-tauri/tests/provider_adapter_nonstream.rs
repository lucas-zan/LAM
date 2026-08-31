use localagentmanager_core::adapters::nonstream::*;
use localagentmanager_core::adapters::protocol::*;
use localagentmanager_core::adapters::request::*;

fn context() -> DeterministicContext {
    DeterministicContext::new(1_700_000_000, "resp-fixed")
}

fn chat(content: Option<&str>, finish_reason: &str) -> ChatCompletionResponse {
    serde_json::from_value(serde_json::json!({
        "id":"chat-1", "object":"chat.completion", "created": 100,
        "model":"bound-model",
        "choices":[{"index":0,"message":{"role":"assistant","content":content},"finish_reason":finish_reason}],
        "usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}
    })).unwrap()
}

#[test]
fn text_response_is_a_complete_typed_responses_object() {
    let result =
        convert_nonstream_response(&chat(Some("hello"), "stop"), "bound-model", &mut context())
            .unwrap();
    assert_eq!(result.id, "resp-fixed");
    assert_eq!(result.object, "response");
    assert_eq!(result.created_at, 1_700_000_000);
    assert_eq!(result.status, ResponsesStatus::Completed);
    assert_eq!(result.output.len(), 1);
    match &result.output[0] {
        ResponsesOutputItem::Message { id, content, .. } => {
            assert_eq!(id, "msg-resp-fixed-1");
            assert_eq!(content[0].text(), Some("hello"));
        }
        item => panic!("unexpected output: {item:?}"),
    }
    let usage = result.usage.unwrap();
    assert_eq!(
        (usage.input_tokens, usage.output_tokens, usage.total_tokens),
        (3, 2, 5)
    );
}

#[test]
fn empty_null_length_and_content_filter_have_explicit_outcomes() {
    for content in [Some(""), None] {
        let response =
            convert_nonstream_response(&chat(content, "stop"), "bound-model", &mut context())
                .unwrap();
        assert_eq!(response.status, ResponsesStatus::Completed);
        assert_eq!(response.output.len(), 1);
    }

    let length = convert_nonstream_response(
        &chat(Some("partial"), "length"),
        "bound-model",
        &mut context(),
    )
    .unwrap();
    assert_eq!(length.status, ResponsesStatus::Incomplete);
    assert_eq!(
        length.incomplete_details.unwrap().reason,
        "max_output_tokens"
    );

    let filtered =
        convert_nonstream_response(&chat(None, "content_filter"), "bound-model", &mut context())
            .unwrap();
    assert_eq!(filtered.status, ResponsesStatus::Incomplete);
    assert_eq!(
        filtered.incomplete_details.unwrap().reason,
        "content_filter"
    );
}

#[test]
fn choice_model_and_finish_reason_validation_is_deterministic() {
    let mut zero = chat(Some("x"), "stop");
    zero.choices.clear();
    assert_eq!(
        convert_nonstream_response(&zero, "bound-model", &mut context())
            .unwrap_err()
            .code,
        NonstreamErrorCode::ChoiceCount
    );

    let mut multiple = chat(Some("x"), "stop");
    multiple.choices.push(multiple.choices[0].clone());
    assert_eq!(
        convert_nonstream_response(&multiple, "bound-model", &mut context())
            .unwrap_err()
            .code,
        NonstreamErrorCode::ChoiceCount
    );

    let mismatch = chat(Some("x"), "stop");
    assert_eq!(
        convert_nonstream_response(&mismatch, "another-model", &mut context())
            .unwrap_err()
            .code,
        NonstreamErrorCode::ModelMismatch
    );

    let unknown = chat(Some("x"), "provider_magic");
    assert_eq!(
        convert_nonstream_response(&unknown, "bound-model", &mut context())
            .unwrap_err()
            .code,
        NonstreamErrorCode::UnknownFinishReason
    );

    let first = serde_json::to_value(
        convert_nonstream_response(&chat(Some("x"), "stop"), "bound-model", &mut context())
            .unwrap(),
    )
    .unwrap();
    let second = serde_json::to_value(
        convert_nonstream_response(&chat(Some("x"), "stop"), "bound-model", &mut context())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(first, second);
}

#[test]
fn tool_only_response_produces_function_call_items() {
    let response: ChatCompletionResponse = serde_json::from_value(serde_json::json!({
      "id":"chat-tool","object":"chat.completion","created":100,"model":"bound-model",
      "choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[
        {"id":"call-1","type":"function","function":{"name":"lookup","arguments":"{\"q\":\"x\"}"}}
      ]},"finish_reason":"tool_calls"}]
    }))
    .unwrap();
    let result = convert_nonstream_response(&response, "bound-model", &mut context()).unwrap();
    assert_eq!(result.status, ResponsesStatus::Completed);
    assert!(
        matches!(&result.output[0], ResponsesOutputItem::FunctionCall { call_id, .. } if call_id == "call-1")
    );
}

#[test]
fn namespace_tool_response_restores_original_name_and_namespace() {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "model":"bound-model",
        "stream":false,
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
        "bound-model",
        &CompatibilityPolicy::generic_openai_compatible(),
    )
    .unwrap();
    let response: ChatCompletionResponse = serde_json::from_value(serde_json::json!({
      "id":"chat-tool","object":"chat.completion","created":100,"model":"bound-model",
      "choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[
        {"id":"call-namespace","type":"function","function":{"name":"remote__lookup","arguments":"{}"}}
      ]},"finish_reason":"tool_calls"}]
    }))
    .unwrap();

    let result = convert_nonstream_response_with_tools(
        &response,
        "bound-model",
        &mut context(),
        &translated.context,
    )
    .unwrap();
    assert!(matches!(
        &result.output[0],
        ResponsesOutputItem::FunctionCall { call_id, name, namespace, .. }
            if call_id == "call-namespace"
                && name == "lookup"
                && namespace.as_deref() == Some("remote")
    ));
}

#[test]
fn custom_tool_search_and_annotations_round_trip_from_chat_response() {
    let request = parsed(serde_json::json!({
        "model":"bound-model",
        "stream":false,
        "store":false,
        "tools":[
            {"type":"custom","name":"apply_patch","description":"patch","format":{"type":"text"}},
            {"type":"tool_search","execution":"client","description":"discover","parameters":{"type":"object"}}
        ],
        "input":"edit"
    }));
    let translated = translate_responses_request_with_context(
        &request,
        "bound-model",
        &CompatibilityPolicy::generic_openai_compatible(),
    )
    .unwrap();
    let response: ChatCompletionResponse = serde_json::from_value(serde_json::json!({
      "id":"chat-tools","object":"chat.completion","created":100,"model":"bound-model",
      "choices":[{"index":0,"message":{"role":"assistant","content":"done","annotations":[
        {"type":"url_citation","url_citation":{"url":"https://example.com","title":"Example","start_index":0,"end_index":4}}
      ],"tool_calls":[
        {"id":"custom-1","type":"function","function":{"name":"apply_patch","arguments":"{\"input\":\"*** Begin Patch\"}"}},
        {"id":"search-1","type":"function","function":{"name":"tool_search","arguments":"{\"query\":\"calendar\"}"}}
      ]},"finish_reason":"tool_calls"}]
    })).unwrap();
    let result = convert_nonstream_response_with_tools(
        &response,
        "bound-model",
        &mut context(),
        &translated.context,
    )
    .unwrap();
    assert!(matches!(
        &result.output[0],
        ResponsesOutputItem::Message { content, .. }
            if content[0].text() == Some("done")
    ));
    assert!(matches!(
        &result.output[1],
        ResponsesOutputItem::CustomToolCall { call_id, input, .. }
            if call_id == "custom-1" && input == "*** Begin Patch"
    ));
    assert!(matches!(
        &result.output[2],
        ResponsesOutputItem::ToolSearchCall { call_id, arguments, .. }
            if call_id == "search-1" && arguments["query"] == "calendar"
    ));
    let wire = serde_json::to_value(&result).unwrap();
    assert_eq!(
        wire["output"][0]["content"][0]["annotations"][0]["url"],
        "https://example.com"
    );
}

#[test]
fn legacy_function_call_is_emitted_once_and_local_shell_is_restored() {
    let response: ChatCompletionResponse = serde_json::from_value(serde_json::json!({
        "id":"chat-legacy","object":"chat.completion","created":100,"model":"bound-model",
        "choices":[{"index":0,"message":{"role":"assistant","function_call":{
            "name":"local_shell","arguments":"{\"command\":[\"pwd\"]}"
        }},"finish_reason":"function_call"}]
    }))
    .unwrap();
    let result = convert_nonstream_response(&response, "bound-model", &mut context()).unwrap();
    assert_eq!(
        result
            .output
            .iter()
            .filter(|item| matches!(item, ResponsesOutputItem::LocalShellCall { .. }))
            .count(),
        1
    );
    assert!(matches!(
        &result.output[0],
        ResponsesOutputItem::LocalShellCall { action, .. } if action["command"][0] == "pwd"
    ));
}

#[test]
fn provider_insufficient_resources_becomes_an_explicit_incomplete_response() {
    let response = convert_nonstream_response(
        &chat(Some("partial"), "insufficient_system_resource"),
        "bound-model",
        &mut context(),
    )
    .unwrap();
    assert_eq!(response.status, ResponsesStatus::Incomplete);
    assert_eq!(
        response.incomplete_details.unwrap().reason,
        "insufficient_system_resource"
    );
}

fn parsed(value: serde_json::Value) -> ResponsesRequest {
    parse_responses_request(&serde_json::to_vec(&value).unwrap()).unwrap()
}
