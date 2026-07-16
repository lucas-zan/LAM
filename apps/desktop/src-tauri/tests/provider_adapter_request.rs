use localagentmanager_core::adapters::protocol::*;
use localagentmanager_core::adapters::request::*;

fn request(value: serde_json::Value) -> ResponsesRequest {
    parse_responses_request(&serde_json::to_vec(&value).unwrap()).unwrap()
}

fn generic() -> CompatibilityPolicy {
    CompatibilityPolicy::generic_openai_compatible()
}

#[test]
fn translates_string_messages_instructions_and_authoritative_model() {
    let source = request(serde_json::json!({
        "model": "bound-model",
        "instructions": "system instruction",
        "input": [
            {"type":"message","role":"developer","content":[{"type":"input_text","text":"dev"}]},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]},
            {"type":"message","role":"assistant","content":[{"type":"output_text","text":"prior"}]},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"again"}]}
        ],
        "stream": true, "store": false, "parallel_tool_calls": false,
        "tool_choice": "auto", "max_output_tokens": 512
    }));
    let translated = translate_responses_request(&source, "bound-model", &generic()).unwrap();
    assert_eq!(translated.model, "bound-model");
    assert!(translated.stream);
    assert_eq!(translated.max_tokens, Some(512));
    assert!(translated.stream_options.unwrap().include_usage);
    assert!(matches!(translated.messages[0], ChatMessage::System { .. }));
    assert!(matches!(
        translated.messages[1],
        ChatMessage::Developer { .. }
    ));
    assert!(matches!(translated.messages[2], ChatMessage::User { .. }));
    assert!(matches!(
        translated.messages[3],
        ChatMessage::Assistant { .. }
    ));
    assert!(matches!(translated.messages[4], ChatMessage::User { .. }));
}

#[test]
fn empty_model_mismatch_stateful_and_invalid_role_order_are_rejected() {
    let empty = request(serde_json::json!({"model":"m","input":[],"stream":true,"store":false}));
    assert_eq!(
        translate_responses_request(&empty, "m", &generic())
            .unwrap_err()
            .code,
        AdapterRequestErrorCode::EmptyInput
    );

    let mismatch =
        request(serde_json::json!({"model":"other","input":"hi","stream":true,"store":false}));
    assert_eq!(
        translate_responses_request(&mismatch, "bound", &generic())
            .unwrap_err()
            .code,
        AdapterRequestErrorCode::ModelMismatch
    );

    let mut stateful =
        request(serde_json::json!({"model":"m","input":"hi","stream":true,"store":false}));
    stateful.previous_response_id = Some("resp-old".into());
    assert_eq!(
        translate_responses_request(&stateful, "m", &generic())
            .unwrap_err()
            .code,
        AdapterRequestErrorCode::ResponseStoreUnsupported
    );

    let invalid_order = request(serde_json::json!({
        "model":"m", "stream":true, "store":false,
        "input":[
          {"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]},
          {"type":"message","role":"system","content":[{"type":"input_text","text":"late"}]}
        ]
    }));
    assert_eq!(
        translate_responses_request(&invalid_order, "m", &generic())
            .unwrap_err()
            .code,
        AdapterRequestErrorCode::InvalidRoleOrder
    );
}

#[test]
fn unsupported_items_tools_and_policy_fields_fail_before_upstream_request() {
    let namespace = request(serde_json::json!({
      "model":"m", "input":"hi", "stream":true, "store":false,
      "tools":[{"type":"namespace","name":"remote","tools":{}}]
    }));
    let error = translate_responses_request(&namespace, "m", &generic()).unwrap_err();
    assert_eq!(error.code, AdapterRequestErrorCode::UnsupportedTool);
    assert_eq!(error.path.as_deref(), Some("$.tools[0].type"));

    let parallel = request(serde_json::json!({
      "model":"m", "input":"hi", "stream":true, "store":false,
      "parallel_tool_calls":true
    }));
    let mut policy = generic();
    policy.supports_parallel_tool_calls = false;
    assert_eq!(
        translate_responses_request(&parallel, "m", &policy)
            .unwrap_err()
            .code,
        AdapterRequestErrorCode::UnsupportedParameter
    );

    let structured = request(serde_json::json!({
      "model":"m", "input":"hi", "stream":true, "store":false,
      "text":{"format":{"type":"json_object"}}
    }));
    let mut policy = generic();
    policy.supports_structured_output = false;
    assert_eq!(
        translate_responses_request(&structured, "m", &policy)
            .unwrap_err()
            .code,
        AdapterRequestErrorCode::UnsupportedParameter
    );
}

#[test]
fn web_search_tool_is_rejected_by_chat_completions_translation() {
    let source = request(serde_json::json!({
      "model":"m", "input":"hi", "stream":true, "store":false,
      "tools":[{"type":"web_search","search_context_size":"medium"}]
    }));

    let error = translate_responses_request(&source, "m", &generic()).unwrap_err();
    assert_eq!(error.code, AdapterRequestErrorCode::UnsupportedTool);
    assert_eq!(error.path.as_deref(), Some("$.tools[0].type"));
}

#[test]
fn reasoning_followup_is_rejected_at_the_exact_chat_translation_path() {
    let source = request(serde_json::json!({
      "model":"m", "stream":true, "store":false,
      "input":[
        {"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]},
        {"type":"reasoning","id":"rs_1","summary":[],"encrypted_content":"cipher"}
      ]
    }));

    let error = translate_responses_request(&source, "m", &generic()).unwrap_err();
    assert_eq!(error.code, AdapterRequestErrorCode::UnsupportedInput);
    assert_eq!(error.path.as_deref(), Some("$.input[1]"));
}

#[test]
fn generic_translation_has_no_provider_identity() {
    let policy = generic();
    assert_eq!(policy.id, "generic-openai-compatible-v1");
    let debug = format!("{policy:?}");
    assert!(!debug.to_ascii_lowercase().contains("deepseek"));
    assert!(!debug.contains("provider_id"));
}

#[test]
fn translates_mixed_user_text_and_images_to_chat_content_parts() {
    let source = request(serde_json::json!({
        "model":"vision-model", "stream":true, "store":false,
        "input":[{"type":"message","role":"user","content":[
            {"type":"input_text","text":"before"},
            {"type":"input_image","image_url":"data:image/png;base64,AA==","detail":"high"},
            {"type":"input_text","text":"after"}
        ]}]
    }));

    let translated = translate_responses_request(&source, "vision-model", &generic()).unwrap();
    let wire = serde_json::to_value(translated).unwrap();
    assert_eq!(
        wire["messages"][0]["content"],
        serde_json::json!([
            {"type":"text","text":"before"},
            {"type":"image_url","image_url":{"url":"data:image/png;base64,AA==","detail":"high"}},
            {"type":"text","text":"after"}
        ])
    );
}

#[test]
fn text_only_user_content_remains_a_string() {
    let source = request(serde_json::json!({
        "model":"m", "stream":true, "store":false,
        "input":[{"type":"message","role":"user","content":[
            {"type":"input_text","text":"hello "},
            {"type":"input_text","text":"world"}
        ]}]
    }));

    let translated = translate_responses_request(&source, "m", &generic()).unwrap();
    let wire = serde_json::to_value(translated).unwrap();
    assert_eq!(wire["messages"][0]["content"], "hello world");
}

#[test]
fn invalid_or_non_user_image_content_is_rejected_at_the_content_path() {
    let blank = request(serde_json::json!({
        "model":"m", "stream":true, "store":false,
        "input":[{"type":"message","role":"user","content":[
            {"type":"input_image","image_url":"   "}
        ]}]
    }));
    let error = translate_responses_request(&blank, "m", &generic()).unwrap_err();
    assert_eq!(error.code, AdapterRequestErrorCode::UnsupportedInput);
    assert_eq!(
        error.path.as_deref(),
        Some("$.input[0].content[0].image_url")
    );

    for role in ["system", "developer", "assistant"] {
        let content_type = if role == "assistant" {
            "output_text"
        } else {
            "input_text"
        };
        let source = request(serde_json::json!({
            "model":"m", "stream":true, "store":false,
            "input":[{"type":"message","role":role,"content":[
                {"type":content_type,"text":"context"},
                {"type":"input_image","image_url":"https://example.com/image.png"}
            ]}]
        }));
        let error = translate_responses_request(&source, "m", &generic()).unwrap_err();
        assert_eq!(
            error.code,
            AdapterRequestErrorCode::UnsupportedInput,
            "role={role}"
        );
        assert_eq!(
            error.path.as_deref(),
            Some("$.input[0].content[1].type"),
            "role={role}"
        );
    }
}
