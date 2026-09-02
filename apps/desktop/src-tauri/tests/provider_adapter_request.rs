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
    let translated = translate_responses_request(&invalid_order, "m", &generic()).unwrap();
    // Mid-dialogue system is no longer rejected; it is hoisted before the
    // dialogue and the user message order is preserved.
    assert!(matches!(translated.messages[0], ChatMessage::System { .. }));
    assert!(matches!(translated.messages[1], ChatMessage::User { .. }));
}

#[test]
fn unsupported_items_tools_and_policy_fields_fail_before_upstream_request() {
    let namespace = request(serde_json::json!({
      "model":"m", "stream":true, "store":false,
      "tool_choice":{"type":"function","namespace":"remote","name":"exec"},
      "tools":[{"type":"namespace","name":"remote","tools":[
        {"type":"function","name":"exec","description":"run a command","parameters":{"type":"object"}}
      ]}],
      "input":[
        {"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]},
        {"type":"function_call","call_id":"call-1","namespace":"remote","name":"exec","arguments":"{}"}
      ]
    }));
    let translated = translate_responses_request_with_context(&namespace, "m", &generic()).unwrap();
    assert_eq!(translated.request.tools[0].function.name, "remote__exec");
    assert_eq!(
        translated
            .request
            .tool_choice
            .as_ref()
            .and_then(ChatToolChoice::function_name),
        Some("remote__exec")
    );
    assert_eq!(
        translated
            .context
            .resolve_chat_tool("remote__exec")
            .unwrap()
            .namespace
            .as_deref(),
        Some("remote")
    );
    assert!(translated.request.messages.iter().any(|message| matches!(
        message,
        ChatMessage::Assistant { tool_calls, .. }
            if tool_calls[0].function.name == "remote__exec"
    )));

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
fn openai_chat_translation_maps_hosted_search_to_chat_search_options() {
    let source = request(serde_json::json!({
      "model":"m", "input":"hi", "stream":true, "store":false,
      "tools":[{"type":"web_search","external_web_access":false,"search_context_size":"medium","user_location":{"type":"approximate","country":"CN"}}]
    }));

    let translated = translate_responses_request_with_context(&source, "m", &generic()).unwrap();
    assert!(translated.request.tools.is_empty());
    assert_eq!(
        translated
            .request
            .web_search_options
            .as_ref()
            .unwrap()
            .search_context_size
            .as_deref(),
        Some("medium")
    );
    assert_eq!(
        translated
            .request
            .web_search_options
            .as_ref()
            .unwrap()
            .user_location
            .as_ref()
            .unwrap()["country"],
        serde_json::json!("CN")
    );
}

#[test]
fn openai_chat_translation_maps_default_cached_search_without_rejecting_the_turn() {
    let source = request(serde_json::json!({
      "model":"m", "input":"hi", "stream":true, "store":false,
      "tools":[{"type":"web_search","external_web_access":false}]
    }));

    let translated = translate_responses_request_with_context(&source, "m", &generic())
        .expect("default Codex cached search must not block a Chat turn");
    assert_eq!(
        translated.request.web_search_options,
        Some(ChatWebSearchOptions {
            search_context_size: None,
            user_location: None,
        })
    );
}

#[test]
fn additional_tools_and_codex_client_tools_are_bridged_without_loss() {
    let source = request(serde_json::json!({
      "model":"m", "stream":false, "store":false,
      "input":[
        {"type":"additional_tools","role":"developer","tools":[
          {"type":"custom","name":"apply_patch","description":"patch","format":{"type":"text"}},
          {"type":"tool_search","execution":"client","description":"discover","parameters":{"type":"object","properties":{"query":{"type":"string"}}}}
        ]},
        {"type":"message","role":"user","content":[{"type":"input_text","text":"edit"}]},
        {"type":"custom_tool_call","call_id":"custom-1","name":"apply_patch","input":"*** Begin Patch"},
        {"type":"custom_tool_call_output","call_id":"custom-1","output":"ok"},
        {"type":"tool_search_call","call_id":"search-1","execution":"client","arguments":{"query":"calendar"}},
        {"type":"tool_search_output","call_id":"search-1","status":"completed","execution":"client","tools":[]}
      ]
    }));

    let translated = translate_responses_request_with_context(&source, "m", &generic()).unwrap();
    assert_eq!(translated.request.tools.len(), 2);
    assert_eq!(translated.request.tools[0].function.name, "apply_patch");
    assert_eq!(
        translated.request.tools[0].function.parameters,
        serde_json::json!({
            "type":"object",
            "properties":{"input":{"type":"string"}},
            "required":["input"]
        })
    );
    assert_eq!(translated.request.tools[1].function.name, "tool_search");
    assert_eq!(
        translated.context.tool_kind("apply_patch"),
        Some(ToolKind::Custom)
    );
    assert_eq!(
        translated.context.tool_kind("tool_search"),
        Some(ToolKind::ToolSearch)
    );
    assert!(translated.request.messages.iter().any(|message| matches!(
        message,
        ChatMessage::Tool { tool_call_id, content }
            if tool_call_id == "custom-1" && content == "ok"
    )));
    assert!(translated.request.messages.iter().any(|message| matches!(
        message,
        ChatMessage::Tool { tool_call_id, content }
            if tool_call_id == "search-1" && content == "[]"
    )));
}

#[test]
fn malformed_or_colliding_namespace_tools_fail_explicitly() {
    let malformed = request(serde_json::json!({
      "model":"m", "input":"hi", "stream":true, "store":false,
      "tools":[{"type":"namespace","name":"remote","tools":{}}]
    }));
    let error = translate_responses_request(&malformed, "m", &generic()).unwrap_err();
    assert_eq!(error.code, AdapterRequestErrorCode::UnsupportedTool);
    assert_eq!(error.path.as_deref(), Some("$.tools[0].tools"));

    let colliding = request(serde_json::json!({
      "model":"m", "input":"hi", "stream":true, "store":false,
      "tools":[
        {"type":"namespace","name":"remote.one","tools":[{"type":"function","name":"exec"}]},
        {"type":"namespace","name":"remote_one","tools":[{"type":"function","name":"exec"}]}
      ]
    }));
    let error = translate_responses_request(&colliding, "m", &generic()).unwrap_err();
    assert_eq!(error.code, AdapterRequestErrorCode::ToolNameCollision);

    let mut unsupported_policy = generic();
    unsupported_policy.supports_namespace_function_tools = false;
    let error = translate_responses_request(&malformed, "m", &unsupported_policy).unwrap_err();
    assert_eq!(error.code, AdapterRequestErrorCode::UnsupportedTool);
    assert_eq!(error.path.as_deref(), Some("$.tools[0].type"));
}

#[test]
fn readable_reasoning_history_is_replayed_as_chat_assistant_context() {
    let source = request(serde_json::json!({
      "model":"m", "stream":true, "store":false,
      "input":[
        {"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]},
        {"type":"reasoning","id":"rs_1","summary":[{"type":"summary_text","text":"plan"}],"encrypted_content":"cipher"}
      ]
    }));

    let translated = translate_responses_request(&source, "m", &generic()).unwrap();
    assert!(translated.messages.iter().any(|message| matches!(
        message,
        ChatMessage::Assistant { reasoning_content: Some(content), .. }
            if content == "plan"
    )));
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
fn generic_translation_forwards_active_reasoning_and_omits_none() {
    for effort in ["low", "medium", "high", "xhigh", "max", "ultra"] {
        let source = request(serde_json::json!({
            "model":"m", "input":"hi", "stream":true, "store":false,
            "reasoning":{"effort":effort}
        }));
        let translated = translate_responses_request(&source, "m", &generic()).unwrap();
        assert_eq!(translated.reasoning_effort.as_deref(), Some(effort));
    }

    let none = request(serde_json::json!({
        "model":"m", "input":"hi", "stream":true, "store":false,
        "reasoning":{"effort":"none"}
    }));
    let translated = translate_responses_request(&none, "m", &generic()).unwrap();
    assert_eq!(translated.reasoning_effort, None);
}

#[test]
fn translated_chat_request_omits_unset_optional_wire_fields() {
    let source = request(serde_json::json!({
        "model":"m", "input":"hi", "stream":true, "store":false
    }));
    let translated = translate_responses_request_with_context(&source, "m", &generic()).unwrap();
    let wire = serde_json::to_value(translated.request).unwrap();

    for field in [
        "max_tokens",
        "tool_choice",
        "parallel_tool_calls",
        "response_format",
        "thinking",
        "reasoning_effort",
        "service_tier",
        "web_search_options",
    ] {
        assert!(
            wire.get(field).is_none(),
            "unset field must be omitted: {field}"
        );
    }
    assert_eq!(wire["stream_options"]["include_usage"], true);
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

#[test]
fn mid_dialogue_developer_in_multi_turn_history_is_translated_and_hoisted() {
    let source = request(serde_json::json!({
        "model":"m", "stream":true, "store":false,
        "instructions":"system instruction",
        "input":[
            {"type":"message","role":"developer","content":[{"type":"input_text","text":"first dev"}]},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"turn 1"}]},
            {"type":"message","role":"assistant","content":[{"type":"output_text","text":"reply 1"}]},
            {"type":"message","role":"developer","content":[{"type":"input_text","text":"second dev"}]},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"turn 2"}]},
            {"type":"message","role":"assistant","content":[{"type":"output_text","text":"reply 2"}]},
            {"type":"message","role":"developer","content":[{"type":"input_text","text":"third dev"}]},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"turn 3"}]}
        ]
    }));

    let translated = translate_responses_request(&source, "m", &generic()).unwrap();
    // instructions -> System first, then all developer messages in original
    // relative order, then the untouched dialogue.
    assert!(matches!(
        &translated.messages[0],
        ChatMessage::System { content } if content == "system instruction"
    ));
    let developer_contents: Vec<&str> = translated
        .messages
        .iter()
        .filter_map(|message| match message {
            ChatMessage::Developer { content } => Some(content.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        developer_contents,
        vec!["first dev", "second dev", "third dev"]
    );

    let dialogue: Vec<&str> = translated
        .messages
        .iter()
        .filter_map(|message| match message {
            ChatMessage::User { content } => Some(match content {
                ChatUserContent::Text(text) => text.as_str(),
                ChatUserContent::Parts(_) => "<parts>",
            }),
            _ => None,
        })
        .collect();
    assert_eq!(dialogue, vec!["turn 1", "turn 2", "turn 3"]);
}

#[test]
fn repeated_mid_dialogue_developer_is_deduplicated_when_hoisted() {
    let source = request(serde_json::json!({
        "model":"m", "stream":true, "store":false,
        "input":[
            {"type":"message","role":"user","content":[{"type":"input_text","text":"hi"}]},
            {"type":"message","role":"developer","content":[{"type":"input_text","text":"context"}]},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"again"}]},
            {"type":"message","role":"developer","content":[{"type":"input_text","text":"context"}]}
        ]
    }));

    let translated = translate_responses_request(&source, "m", &generic()).unwrap();
    let developer_count = translated
        .messages
        .iter()
        .filter(|message| matches!(message, ChatMessage::Developer { .. }))
        .count();
    assert_eq!(developer_count, 1);
    assert!(
        matches!(&translated.messages[0], ChatMessage::Developer { content } if content == "context")
    );
}
