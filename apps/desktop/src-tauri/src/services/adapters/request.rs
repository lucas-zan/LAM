use super::deepseek::ReasoningHistory;
use super::protocol::*;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompatibilityPolicy {
    pub id: String,
    pub supports_developer_role: bool,
    pub supports_parallel_tool_calls: bool,
    pub supports_structured_output: bool,
    pub supports_reasoning: bool,
    pub requires_assistant_content_for_tool_calls: bool,
    pub requires_reasoning_for_tool_calls: bool,
}

impl CompatibilityPolicy {
    pub fn generic_openai_compatible() -> Self {
        Self {
            id: "generic-openai-compatible-v1".into(),
            supports_developer_role: true,
            supports_parallel_tool_calls: true,
            supports_structured_output: true,
            supports_reasoning: false,
            requires_assistant_content_for_tool_calls: false,
            requires_reasoning_for_tool_calls: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdapterRequestErrorCode {
    EmptyInput,
    ModelMismatch,
    ResponseStoreUnsupported,
    InvalidRoleOrder,
    UnsupportedInput,
    UnsupportedTool,
    UnsupportedParameter,
    InvalidToolName,
    DuplicateCallId,
    UnknownCallId,
    ToolResultBeforeCall,
    ToolArgumentsInvalid,
    ToolArgumentsLimitExceeded,
    ReasoningHistoryRequired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterRequestError {
    pub code: AdapterRequestErrorCode,
    pub path: Option<String>,
    pub message: String,
}

impl AdapterRequestError {
    pub fn new(
        code: AdapterRequestErrorCode,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            path: Some(path.into()),
            message: message.into(),
        }
    }
}

pub fn translate_responses_request(
    source: &ResponsesRequest,
    bound_model: &str,
    policy: &CompatibilityPolicy,
) -> Result<ChatCompletionRequest, AdapterRequestError> {
    translate_responses_request_with_history(source, bound_model, policy, &ReasoningHistory::new(0))
}

pub fn translate_responses_request_with_history(
    source: &ResponsesRequest,
    bound_model: &str,
    policy: &CompatibilityPolicy,
    history: &ReasoningHistory,
) -> Result<ChatCompletionRequest, AdapterRequestError> {
    validate_request(source, bound_model, policy)?;
    let mut messages = Vec::new();
    if let Some(instructions) = source.instructions.as_ref() {
        messages.push(ChatMessage::System {
            content: instructions.clone(),
        });
    }
    messages.extend(translate_items(&source.input, policy, history)?);
    Ok(ChatCompletionRequest {
        model: bound_model.to_owned(),
        messages,
        stream: source.stream,
        stream_options: source.stream.then_some(ChatStreamOptions {
            include_usage: true,
        }),
        max_tokens: source.max_output_tokens,
        tools: translate_tools(&source.tools)?,
        tool_choice: translate_tool_choice(source.tool_choice.as_ref()),
        parallel_tool_calls: source.parallel_tool_calls,
        response_format: translate_text_format(source.text.as_ref()),
        thinking: None,
        reasoning_effort: None,
    })
}

fn validate_request(
    source: &ResponsesRequest,
    bound_model: &str,
    policy: &CompatibilityPolicy,
) -> Result<(), AdapterRequestError> {
    if source.input.is_empty() {
        return Err(AdapterRequestError::new(
            AdapterRequestErrorCode::EmptyInput,
            "$.input",
            "input must not be empty",
        ));
    }
    if source.model != bound_model {
        return Err(AdapterRequestError::new(
            AdapterRequestErrorCode::ModelMismatch,
            "$.model",
            "request model does not match the attached binding",
        ));
    }
    if source.previous_response_id.is_some() {
        return Err(AdapterRequestError::new(
            AdapterRequestErrorCode::ResponseStoreUnsupported,
            "$.previous_response_id",
            "adapter uses full history and has no response store",
        ));
    }
    if source.store {
        return Err(AdapterRequestError::new(
            AdapterRequestErrorCode::ResponseStoreUnsupported,
            "$.store",
            "response storage is not supported",
        ));
    }
    if source.parallel_tool_calls == Some(true) && !policy.supports_parallel_tool_calls {
        return Err(AdapterRequestError::new(
            AdapterRequestErrorCode::UnsupportedParameter,
            "$.parallel_tool_calls",
            "parallel tool calls are not supported by this compatibility policy",
        ));
    }
    if source
        .text
        .as_ref()
        .and_then(|text| text.format.as_ref())
        .is_some()
        && !policy.supports_structured_output
    {
        return Err(AdapterRequestError::new(
            AdapterRequestErrorCode::UnsupportedParameter,
            "$.text.format",
            "structured output is not supported by this compatibility policy",
        ));
    }
    if source.reasoning.is_some() && !policy.supports_reasoning {
        return Err(AdapterRequestError::new(
            AdapterRequestErrorCode::UnsupportedParameter,
            "$.reasoning",
            "reasoning controls are not supported by this compatibility policy",
        ));
    }
    validate_role_order(&source.input)
}

fn validate_role_order(items: &[ResponsesInputItem]) -> Result<(), AdapterRequestError> {
    let mut dialogue_started = false;
    for (index, item) in items.iter().enumerate() {
        let ResponsesInputItem::Message { role, .. } = item else {
            dialogue_started = true;
            continue;
        };
        if matches!(role, ResponsesRole::System | ResponsesRole::Developer) {
            if dialogue_started {
                return Err(AdapterRequestError::new(
                    AdapterRequestErrorCode::InvalidRoleOrder,
                    format!("$.input[{index}].role"),
                    "system and developer messages must precede dialogue history",
                ));
            }
        } else {
            dialogue_started = true;
        }
    }
    Ok(())
}

fn translate_items(
    items: &[ResponsesInputItem],
    policy: &CompatibilityPolicy,
    history: &ReasoningHistory,
) -> Result<Vec<ChatMessage>, AdapterRequestError> {
    let mut messages = Vec::with_capacity(items.len());
    let mut calls = BTreeSet::new();
    for (index, item) in items.iter().enumerate() {
        match item {
            ResponsesInputItem::Message { role, content } => {
                messages.push(translate_message(*role, content, index, policy)?)
            }
            ResponsesInputItem::FunctionCall {
                call_id,
                name,
                arguments,
                ..
            } => {
                validate_tool_call(call_id, name, arguments, index, &mut calls)?;
                let reasoning_content = history
                    .get(call_id)
                    .map(|record| record.as_str().to_owned());
                if policy.requires_reasoning_for_tool_calls && reasoning_content.is_none() {
                    return Err(AdapterRequestError::new(
                        AdapterRequestErrorCode::ReasoningHistoryRequired,
                        format!("$.input[{index}].call_id"),
                        "reasoning content for an assistant tool-call turn is required",
                    ));
                }
                messages.push(ChatMessage::Assistant {
                    content: policy
                        .requires_assistant_content_for_tool_calls
                        .then(String::new),
                    reasoning_content,
                    tool_calls: vec![ChatToolCall {
                        id: call_id.clone(),
                        kind: FunctionType::Function,
                        function: ChatFunctionCall {
                            name: name.clone(),
                            arguments: arguments.clone(),
                        },
                    }],
                });
            }
            ResponsesInputItem::FunctionCallOutput { call_id, output } => {
                if !calls.contains(call_id) {
                    return Err(AdapterRequestError::new(
                        AdapterRequestErrorCode::UnknownCallId,
                        format!("$.input[{index}].call_id"),
                        "function output does not match an earlier call",
                    ));
                }
                messages.push(ChatMessage::Tool {
                    content: output.clone(),
                    tool_call_id: call_id.clone(),
                });
            }
            ResponsesInputItem::Reasoning { .. } => {
                return Err(AdapterRequestError::new(
                    AdapterRequestErrorCode::UnsupportedInput,
                    format!("$.input[{index}]"),
                    "reasoning history is not supported by Chat Completions",
                ));
            }
        }
    }
    Ok(messages)
}

fn translate_message(
    role: ResponsesRole,
    parts: &[ResponsesContentPart],
    index: usize,
    policy: &CompatibilityPolicy,
) -> Result<ChatMessage, AdapterRequestError> {
    if role == ResponsesRole::User {
        return Ok(ChatMessage::User {
            content: translate_user_content(parts, index)?,
        });
    }

    if let Some(part_index) = parts
        .iter()
        .position(|part| matches!(part, ResponsesContentPart::InputImage { .. }))
    {
        return Err(AdapterRequestError::new(
            AdapterRequestErrorCode::UnsupportedInput,
            format!("$.input[{index}].content[{part_index}].type"),
            "image content is only supported in user messages",
        ));
    }

    let content = parts
        .iter()
        .map(|part| match part {
            ResponsesContentPart::InputText { text }
            | ResponsesContentPart::OutputText { text } => text.as_str(),
            ResponsesContentPart::InputImage { .. } => unreachable!("validated above"),
        })
        .collect::<String>();
    match role {
        ResponsesRole::System => Ok(ChatMessage::System { content }),
        ResponsesRole::Developer if policy.supports_developer_role => {
            Ok(ChatMessage::Developer { content })
        }
        ResponsesRole::Developer => Ok(ChatMessage::System { content }),
        ResponsesRole::User => unreachable!("handled above"),
        ResponsesRole::Assistant => {
            if parts
                .iter()
                .any(|part| matches!(part, ResponsesContentPart::InputText { .. }))
            {
                return Err(AdapterRequestError::new(
                    AdapterRequestErrorCode::UnsupportedInput,
                    format!("$.input[{index}].content"),
                    "assistant history requires output_text",
                ));
            }
            Ok(ChatMessage::Assistant {
                content: Some(content),
                reasoning_content: None,
                tool_calls: Vec::new(),
            })
        }
    }
}

fn translate_user_content(
    parts: &[ResponsesContentPart],
    input_index: usize,
) -> Result<ChatUserContent, AdapterRequestError> {
    if parts
        .iter()
        .all(|part| !matches!(part, ResponsesContentPart::InputImage { .. }))
    {
        let text = parts
            .iter()
            .map(|part| match part {
                ResponsesContentPart::InputText { text }
                | ResponsesContentPart::OutputText { text } => text.as_str(),
                ResponsesContentPart::InputImage { .. } => unreachable!("checked above"),
            })
            .collect::<String>();
        return Ok(ChatUserContent::Text(text));
    }

    let mut translated = Vec::with_capacity(parts.len());
    for (part_index, part) in parts.iter().enumerate() {
        match part {
            ResponsesContentPart::InputText { text }
            | ResponsesContentPart::OutputText { text } => {
                translated.push(ChatContentPart::Text { text: text.clone() });
            }
            ResponsesContentPart::InputImage { image_url, detail } => {
                if image_url.trim().is_empty() {
                    return Err(AdapterRequestError::new(
                        AdapterRequestErrorCode::UnsupportedInput,
                        format!("$.input[{input_index}].content[{part_index}].image_url"),
                        "image_url must not be empty",
                    ));
                }
                translated.push(ChatContentPart::ImageUrl {
                    image_url: ChatImageUrl {
                        url: image_url.clone(),
                        detail: detail.clone(),
                    },
                });
            }
        }
    }
    Ok(ChatUserContent::Parts(translated))
}

fn translate_tools(tools: &[ResponsesTool]) -> Result<Vec<ChatTool>, AdapterRequestError> {
    tools
        .iter()
        .enumerate()
        .map(|(index, tool)| match tool {
            ResponsesTool::Function {
                name,
                description,
                parameters,
                strict,
            } => {
                validate_tool_name(name, format!("$.tools[{index}].name"))?;
                Ok(ChatTool {
                    kind: FunctionType::Function,
                    function: ChatFunctionDefinition {
                        name: name.clone(),
                        description: description.clone(),
                        parameters: parameters.clone(),
                        strict: *strict,
                    },
                })
            }
            ResponsesTool::Namespace { .. } | ResponsesTool::WebSearch { .. } => {
                Err(AdapterRequestError::new(
                    AdapterRequestErrorCode::UnsupportedTool,
                    format!("$.tools[{index}].type"),
                    "namespace, hosted, MCP, and computer tools are not supported",
                ))
            }
        })
        .collect()
}

fn validate_tool_call(
    call_id: &str,
    name: &str,
    arguments: &str,
    index: usize,
    calls: &mut BTreeSet<String>,
) -> Result<(), AdapterRequestError> {
    validate_tool_name(name, format!("$.input[{index}].name"))?;
    if arguments.len() > MAX_TOOL_ARGUMENT_BYTES {
        return Err(AdapterRequestError::new(
            AdapterRequestErrorCode::ToolArgumentsLimitExceeded,
            format!("$.input[{index}].arguments"),
            "tool arguments exceed the configured limit",
        ));
    }
    if serde_json::from_str::<serde_json::Value>(arguments).is_err() {
        return Err(AdapterRequestError::new(
            AdapterRequestErrorCode::ToolArgumentsInvalid,
            format!("$.input[{index}].arguments"),
            "tool arguments must be valid JSON",
        ));
    }
    if !calls.insert(call_id.to_owned()) {
        return Err(AdapterRequestError::new(
            AdapterRequestErrorCode::DuplicateCallId,
            format!("$.input[{index}].call_id"),
            "duplicate function call ID",
        ));
    }
    Ok(())
}

fn validate_tool_name(name: &str, path: String) -> Result<(), AdapterRequestError> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'));
    if valid {
        Ok(())
    } else {
        Err(AdapterRequestError::new(
            AdapterRequestErrorCode::InvalidToolName,
            path,
            "invalid function name",
        ))
    }
}

fn translate_text_format(text: Option<&ResponsesTextConfig>) -> Option<ChatResponseFormat> {
    match text.and_then(|text| text.format.as_ref()) {
        None | Some(TextFormat::Text) => None,
        Some(TextFormat::JsonObject) => Some(ChatResponseFormat {
            kind: "json_object".into(),
            json_schema: None,
        }),
        Some(TextFormat::JsonSchema {
            name,
            schema,
            strict,
        }) => Some(ChatResponseFormat {
            kind: "json_schema".into(),
            json_schema: Some(
                serde_json::json!({"name": name, "schema": schema, "strict": strict}),
            ),
        }),
    }
}

fn translate_tool_choice(choice: Option<&ToolChoice>) -> Option<ChatToolChoice> {
    match choice {
        None => None,
        Some(ToolChoice::Mode(mode)) => Some(ChatToolChoice::Mode(*mode)),
        Some(ToolChoice::Function(named)) => Some(ChatToolChoice::function(named.name.clone())),
    }
}
