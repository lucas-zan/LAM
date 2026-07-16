use localagentmanager_core::adapters::nonstream::*;
use localagentmanager_core::adapters::protocol::*;

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
