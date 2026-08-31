use super::deepseek::ReasoningHistory;
use super::protocol::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompatibilityPolicy {
    pub id: String,
    pub supports_developer_role: bool,
    pub supports_parallel_tool_calls: bool,
    pub supports_structured_output: bool,
    pub supports_namespace_function_tools: bool,
    pub supports_reasoning: bool,
    pub supports_hosted_web_search: bool,
    pub requires_assistant_content_for_tool_calls: bool,
    pub requires_reasoning_for_tool_calls: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolIdentity {
    pub namespace: Option<String>,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    Function,
    Custom,
    ToolSearch,
    LocalShell,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolTranslationContext {
    chat_to_responses: BTreeMap<String, ToolIdentity>,
    responses_to_chat: BTreeMap<(Option<String>, String), String>,
    kind_by_chat_name: BTreeMap<String, ToolKind>,
}

impl ToolTranslationContext {
    pub fn resolve_chat_tool(&self, chat_name: &str) -> Option<&ToolIdentity> {
        self.chat_to_responses.get(chat_name)
    }

    pub fn chat_name_for_response_function(&self, name: &str, namespace: Option<&str>) -> String {
        let namespace = namespace.filter(|value| !value.is_empty());
        self.responses_to_chat
            .get(&(namespace.map(str::to_owned), name.to_owned()))
            .cloned()
            .unwrap_or_else(|| match namespace {
                Some(namespace) => flatten_namespace_tool_name(namespace, name),
                None => name.to_owned(),
            })
    }

    pub fn tool_kind(&self, chat_name: &str) -> Option<ToolKind> {
        self.kind_by_chat_name.get(chat_name).copied()
    }

    fn register(
        &mut self,
        chat_name: String,
        identity: ToolIdentity,
        kind: ToolKind,
        path: impl Into<String>,
    ) -> Result<(), AdapterRequestError> {
        if let Some(existing) = self.chat_to_responses.get(&chat_name) {
            if existing != &identity || self.tool_kind(&chat_name) != Some(kind) {
                return Err(AdapterRequestError::new(
                    AdapterRequestErrorCode::ToolNameCollision,
                    path,
                    "namespace tools map to the same Chat Completions function name",
                ));
            }
            return Ok(());
        }
        self.responses_to_chat.insert(
            (identity.namespace.clone(), identity.name.clone()),
            chat_name.clone(),
        );
        self.chat_to_responses.insert(chat_name.clone(), identity);
        self.kind_by_chat_name.insert(chat_name, kind);
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct TranslatedChatRequest {
    pub request: ChatCompletionRequest,
    pub context: ToolTranslationContext,
}

impl CompatibilityPolicy {
    pub fn generic_openai_compatible() -> Self {
        Self {
            id: "generic-openai-compatible-v1".into(),
            supports_developer_role: true,
            supports_parallel_tool_calls: true,
            supports_structured_output: true,
            supports_namespace_function_tools: true,
            supports_reasoning: true,
            supports_hosted_web_search: true,
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
    ToolNameCollision,
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
    Ok(translate_responses_request_with_context(source, bound_model, policy)?.request)
}

pub fn translate_responses_request_with_context(
    source: &ResponsesRequest,
    bound_model: &str,
    policy: &CompatibilityPolicy,
) -> Result<TranslatedChatRequest, AdapterRequestError> {
    translate_responses_request_with_history_and_context(
        source,
        bound_model,
        policy,
        &ReasoningHistory::new(0),
    )
}

pub fn translate_responses_request_with_history(
    source: &ResponsesRequest,
    bound_model: &str,
    policy: &CompatibilityPolicy,
    history: &ReasoningHistory,
) -> Result<ChatCompletionRequest, AdapterRequestError> {
    Ok(
        translate_responses_request_with_history_and_context(source, bound_model, policy, history)?
            .request,
    )
}

pub fn translate_responses_request_with_history_and_context(
    source: &ResponsesRequest,
    bound_model: &str,
    policy: &CompatibilityPolicy,
    history: &ReasoningHistory,
) -> Result<TranslatedChatRequest, AdapterRequestError> {
    validate_request(source, bound_model, policy)?;
    let declared_tools = collect_tools(source);
    let (tools, context, web_search_options) = translate_tools(&declared_tools, policy)?;
    let mut messages = Vec::new();
    if let Some(instructions) = source.instructions.as_ref() {
        messages.push(ChatMessage::System {
            content: instructions.clone(),
        });
    }
    messages.extend(translate_items(&source.input, policy, history, &context)?);
    Ok(TranslatedChatRequest {
        request: ChatCompletionRequest {
            model: bound_model.to_owned(),
            messages,
            stream: source.stream,
            stream_options: source.stream.then_some(ChatStreamOptions {
                include_usage: true,
            }),
            max_tokens: source.max_output_tokens,
            tools,
            tool_choice: translate_tool_choice(source.tool_choice.as_ref(), &context),
            parallel_tool_calls: source.parallel_tool_calls,
            response_format: translate_text_format(source.text.as_ref()),
            thinking: None,
            reasoning_effort: source
                .reasoning
                .as_ref()
                .and_then(|reasoning| reasoning.effort)
                .and_then(ReasoningEffort::as_wire_value)
                .map(str::to_owned),
            service_tier: source.service_tier.clone(),
            web_search_options,
        },
        context,
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
        if matches!(
            item,
            ResponsesInputItem::AdditionalTools { .. } | ResponsesInputItem::Reasoning { .. }
        ) {
            continue;
        }
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
    tool_context: &ToolTranslationContext,
) -> Result<Vec<ChatMessage>, AdapterRequestError> {
    let mut messages = Vec::with_capacity(items.len());
    let mut calls = BTreeSet::new();
    let mut pending_tool_calls = Vec::new();
    let mut pending_reasoning = String::new();
    for (index, item) in items.iter().enumerate() {
        match item {
            ResponsesInputItem::AdditionalTools { .. } => continue,
            ResponsesInputItem::Message { role, content, .. } => {
                flush_pending_tool_calls(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                    policy,
                )?;
                let mut message = translate_message(*role, content, index, policy)?;
                if *role == ResponsesRole::Assistant && !pending_reasoning.is_empty() {
                    set_reasoning_content(&mut message, std::mem::take(&mut pending_reasoning));
                } else if *role != ResponsesRole::Assistant && !pending_reasoning.is_empty() {
                    messages.push(ChatMessage::Assistant {
                        content: Some(String::new()),
                        reasoning_content: Some(std::mem::take(&mut pending_reasoning)),
                        tool_calls: Vec::new(),
                    });
                }
                messages.push(message);
            }
            ResponsesInputItem::FunctionCall {
                call_id,
                name,
                arguments,
                namespace,
                ..
            } => {
                let chat_name =
                    tool_context.chat_name_for_response_function(name, namespace.as_deref());
                validate_tool_call(call_id, &chat_name, arguments, index, &mut calls)?;
                append_pending_reasoning(
                    &mut pending_reasoning,
                    history.get(call_id).map(|record| record.as_str()),
                );
                pending_tool_calls.push(ChatToolCall {
                    id: call_id.clone(),
                    kind: FunctionType::Function,
                    function: ChatFunctionCall {
                        name: chat_name,
                        arguments: arguments.clone(),
                    },
                });
            }
            ResponsesInputItem::FunctionCallOutput { call_id, output } => {
                flush_pending_tool_calls(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                    policy,
                )?;
                if !calls.contains(call_id) {
                    return Err(AdapterRequestError::new(
                        AdapterRequestErrorCode::UnknownCallId,
                        format!("$.input[{index}].call_id"),
                        "function output does not match an earlier call",
                    ));
                }
                messages.push(ChatMessage::Tool {
                    content: tool_output_text(output),
                    tool_call_id: call_id.clone(),
                });
            }
            ResponsesInputItem::McpToolCallOutput { call_id, output } => {
                flush_pending_tool_calls(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                    policy,
                )?;
                ensure_call_output(&calls, call_id, index)?;
                messages.push(ChatMessage::Tool {
                    content: tool_output_text(output),
                    tool_call_id: call_id.clone(),
                });
            }
            ResponsesInputItem::CustomToolCall {
                call_id,
                name,
                namespace,
                input,
                ..
            } => {
                let chat_name =
                    tool_context.chat_name_for_response_function(name, namespace.as_deref());
                let arguments = serde_json::json!({"input": input}).to_string();
                validate_tool_call(call_id, &chat_name, &arguments, index, &mut calls)?;
                pending_tool_calls.push(ChatToolCall {
                    id: call_id.clone(),
                    kind: FunctionType::Function,
                    function: ChatFunctionCall {
                        name: chat_name,
                        arguments,
                    },
                });
            }
            ResponsesInputItem::CustomToolCallOutput {
                call_id, output, ..
            } => {
                flush_pending_tool_calls(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                    policy,
                )?;
                ensure_call_output(&calls, call_id, index)?;
                messages.push(ChatMessage::Tool {
                    content: tool_output_text(output),
                    tool_call_id: call_id.clone(),
                });
            }
            ResponsesInputItem::ToolSearchCall {
                call_id: Some(call_id),
                arguments,
                ..
            } => {
                let arguments = tool_output_text(arguments);
                validate_tool_call(call_id, "tool_search", &arguments, index, &mut calls)?;
                pending_tool_calls.push(ChatToolCall {
                    id: call_id.clone(),
                    kind: FunctionType::Function,
                    function: ChatFunctionCall {
                        name: "tool_search".into(),
                        arguments,
                    },
                });
            }
            ResponsesInputItem::ToolSearchCall { call_id: None, .. } => {
                return Err(AdapterRequestError::new(
                    AdapterRequestErrorCode::UnsupportedInput,
                    format!("$.input[{index}].call_id"),
                    "tool_search call_id is required for Chat Completions history",
                ));
            }
            ResponsesInputItem::ToolSearchOutput { call_id, tools, .. } => {
                flush_pending_tool_calls(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                    policy,
                )?;
                let Some(call_id) = call_id else {
                    return Err(AdapterRequestError::new(
                        AdapterRequestErrorCode::UnsupportedInput,
                        format!("$.input[{index}].call_id"),
                        "tool_search output call_id is required for Chat Completions history",
                    ));
                };
                ensure_call_output(&calls, call_id, index)?;
                messages.push(ChatMessage::Tool {
                    content: serde_json::to_string(tools).unwrap_or_else(|_| "[]".into()),
                    tool_call_id: call_id.clone(),
                });
            }
            ResponsesInputItem::LocalShellCall {
                call_id, action, ..
            } => {
                let Some(call_id) = call_id.as_deref() else {
                    return Err(AdapterRequestError::new(
                        AdapterRequestErrorCode::UnsupportedInput,
                        format!("$.input[{index}].call_id"),
                        "local shell call_id is required for Chat Completions history",
                    ));
                };
                let arguments = tool_output_text(action);
                validate_tool_call(call_id, "local_shell", &arguments, index, &mut calls)?;
                pending_tool_calls.push(ChatToolCall {
                    id: call_id.into(),
                    kind: FunctionType::Function,
                    function: ChatFunctionCall {
                        name: "local_shell".into(),
                        arguments,
                    },
                });
            }
            ResponsesInputItem::AgentMessage {
                author,
                recipient,
                content,
                ..
            } => {
                flush_pending_tool_calls(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                    policy,
                )?;
                let text = agent_message_text(author, recipient, content);
                if !text.is_empty() {
                    messages.push(ChatMessage::Assistant {
                        content: Some(text),
                        reasoning_content: None,
                        tool_calls: Vec::new(),
                    });
                }
            }
            ResponsesInputItem::WebSearchCall { action, .. } => {
                flush_pending_tool_calls(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                    policy,
                )?;
                let text = action
                    .as_ref()
                    .map(|action| format!("[web_search_call] {}", tool_output_text(action)))
                    .unwrap_or_else(|| "[web_search_call]".into());
                messages.push(ChatMessage::Assistant {
                    content: Some(text),
                    reasoning_content: None,
                    tool_calls: Vec::new(),
                });
            }
            ResponsesInputItem::ImageGenerationCall {
                id,
                status,
                revised_prompt,
                result,
            } => {
                flush_pending_tool_calls(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                    policy,
                )?;
                let mut text = format!("[image_generation_call status={status}");
                if let Some(id) = id.as_deref() {
                    text.push_str(&format!(" id={id}"));
                }
                text.push(']');
                if let Some(prompt) = revised_prompt.as_deref() {
                    text.push_str(&format!(" revised_prompt: {prompt}"));
                }
                if let Some(result) = result.as_deref() {
                    text.push_str(&format!(" result: {result}"));
                }
                push_assistant_context(&mut messages, text);
            }
            ResponsesInputItem::Compaction {
                id,
                encrypted_content,
            } => {
                flush_pending_tool_calls(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                    policy,
                )?;
                push_assistant_context(
                    &mut messages,
                    format!(
                        "[compaction{}] {encrypted_content}",
                        id.as_deref()
                            .map(|id| format!(" id={id}"))
                            .unwrap_or_default()
                    ),
                );
            }
            ResponsesInputItem::ContextCompaction {
                id,
                encrypted_content,
            } => {
                flush_pending_tool_calls(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                    policy,
                )?;
                push_assistant_context(
                    &mut messages,
                    format!(
                        "[context_compaction{}] {}",
                        id.as_deref()
                            .map(|id| format!(" id={id}"))
                            .unwrap_or_default(),
                        encrypted_content.as_deref().unwrap_or_default()
                    ),
                );
            }
            ResponsesInputItem::CompactionTrigger => {
                flush_pending_tool_calls(
                    &mut messages,
                    &mut pending_tool_calls,
                    &mut pending_reasoning,
                    policy,
                )?;
                push_assistant_context(&mut messages, "[compaction_trigger]".into());
            }
            ResponsesInputItem::Reasoning { options } => {
                append_pending_reasoning(
                    &mut pending_reasoning,
                    reasoning_text(options).as_deref(),
                );
            }
        }
    }
    flush_pending_tool_calls(
        &mut messages,
        &mut pending_tool_calls,
        &mut pending_reasoning,
        policy,
    )?;
    if !pending_reasoning.is_empty() {
        messages.push(ChatMessage::Assistant {
            content: Some(String::new()),
            reasoning_content: Some(std::mem::take(&mut pending_reasoning)),
            tool_calls: Vec::new(),
        });
    }
    Ok(messages)
}

fn flush_pending_tool_calls(
    messages: &mut Vec<ChatMessage>,
    pending_tool_calls: &mut Vec<ChatToolCall>,
    pending_reasoning: &mut String,
    policy: &CompatibilityPolicy,
) -> Result<(), AdapterRequestError> {
    if pending_tool_calls.is_empty() {
        return Ok(());
    }
    if policy.requires_reasoning_for_tool_calls && pending_reasoning.is_empty() {
        return Err(AdapterRequestError::new(
            AdapterRequestErrorCode::ReasoningHistoryRequired,
            "$.input",
            "reasoning content for an assistant tool-call turn is required",
        ));
    }
    let tool_calls = std::mem::take(pending_tool_calls);
    let reasoning_content =
        (!pending_reasoning.is_empty()).then(|| std::mem::take(pending_reasoning));
    if let Some(ChatMessage::Assistant {
        content,
        reasoning_content: existing_reasoning,
        tool_calls: existing_calls,
    }) = messages.last_mut()
    {
        if existing_calls.is_empty() {
            if policy.requires_assistant_content_for_tool_calls && content.is_none() {
                *content = Some(String::new());
            }
            existing_calls.extend(tool_calls);
            merge_reasoning(existing_reasoning, reasoning_content);
            return Ok(());
        }
    }
    messages.push(ChatMessage::Assistant {
        content: policy
            .requires_assistant_content_for_tool_calls
            .then(String::new),
        reasoning_content,
        tool_calls,
    });
    Ok(())
}

fn merge_reasoning(target: &mut Option<String>, incoming: Option<String>) {
    let Some(incoming) = incoming.filter(|value| !value.is_empty()) else {
        return;
    };
    match target {
        Some(existing) if existing == &incoming => {}
        Some(existing) => {
            existing.push('\n');
            existing.push_str(&incoming);
        }
        None => *target = Some(incoming),
    }
}

fn append_pending_reasoning(target: &mut String, incoming: Option<&str>) {
    let Some(incoming) = incoming.filter(|value| !value.is_empty()) else {
        return;
    };
    if target.is_empty() {
        target.push_str(incoming);
    } else if target != incoming {
        target.push('\n');
        target.push_str(incoming);
    }
}

fn set_reasoning_content(message: &mut ChatMessage, reasoning: String) {
    if let ChatMessage::Assistant {
        reasoning_content, ..
    } = message
    {
        merge_reasoning(reasoning_content, Some(reasoning));
    }
}

fn ensure_call_output(
    calls: &BTreeSet<String>,
    call_id: &str,
    index: usize,
) -> Result<(), AdapterRequestError> {
    if calls.contains(call_id) {
        Ok(())
    } else {
        Err(AdapterRequestError::new(
            AdapterRequestErrorCode::UnknownCallId,
            format!("$.input[{index}].call_id"),
            "tool output does not match an earlier call",
        ))
    }
}

fn tool_output_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .map(|part| match part {
                serde_json::Value::String(text) => text.clone(),
                serde_json::Value::Object(object) => object
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| serde_json::to_string(part).unwrap_or_default()),
                _ => serde_json::to_string(part).unwrap_or_default(),
            })
            .collect(),
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn reasoning_text(options: &BTreeMap<String, serde_json::Value>) -> Option<String> {
    let mut text = String::new();
    for key in ["summary", "content"] {
        let Some(value) = options.get(key) else {
            continue;
        };
        if let Some(parts) = value.as_array() {
            for part in parts {
                let part_text = part
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .or_else(|| part.as_str());
                if let Some(part_text) = part_text.filter(|value| !value.is_empty()) {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(part_text);
                }
            }
        } else if let Some(value) = value.as_str().filter(|value| !value.is_empty()) {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(value);
        }
    }
    (!text.is_empty()).then_some(text)
}

fn agent_message_text(author: &str, recipient: &str, content: &[serde_json::Value]) -> String {
    let content = content
        .iter()
        .filter_map(|part| {
            part.get("text")
                .and_then(serde_json::Value::as_str)
                .or_else(|| part.as_str())
        })
        .collect::<Vec<_>>()
        .join("\n");
    if content.is_empty() {
        String::new()
    } else {
        format!("[agent {author} -> {recipient}] {content}")
    }
}

fn push_assistant_context(messages: &mut Vec<ChatMessage>, text: String) {
    if !text.is_empty() {
        messages.push(ChatMessage::Assistant {
            content: Some(text),
            reasoning_content: None,
            tool_calls: Vec::new(),
        });
    }
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

fn collect_tools(source: &ResponsesRequest) -> Vec<ResponsesTool> {
    let mut tools = source.tools.clone();
    for item in &source.input {
        if let ResponsesInputItem::AdditionalTools {
            tools: additional, ..
        } = item
        {
            tools.extend(additional.iter().cloned());
        }
    }
    tools
}

fn translate_tools(
    tools: &[ResponsesTool],
    policy: &CompatibilityPolicy,
) -> Result<
    (
        Vec<ChatTool>,
        ToolTranslationContext,
        Option<ChatWebSearchOptions>,
    ),
    AdapterRequestError,
> {
    let mut translated = Vec::new();
    let mut context = ToolTranslationContext::default();
    let mut web_search_options = None;
    for (index, tool) in tools.iter().enumerate() {
        match tool {
            ResponsesTool::Function {
                name,
                description,
                parameters,
                strict,
            } => add_function_tool(
                &mut translated,
                &mut context,
                name,
                description.clone(),
                parameters.clone(),
                *strict,
                None,
                ToolKind::Function,
                format!("$.tools[{index}].name"),
            )?,
            ResponsesTool::Namespace { name, tools, .. } => {
                if !policy.supports_namespace_function_tools {
                    return Err(AdapterRequestError::new(
                        AdapterRequestErrorCode::UnsupportedTool,
                        format!("$.tools[{index}].type"),
                        "namespace function tools are not supported by this compatibility policy",
                    ));
                }
                if name.trim().is_empty() {
                    return Err(AdapterRequestError::new(
                        AdapterRequestErrorCode::InvalidToolName,
                        format!("$.tools[{index}].name"),
                        "namespace name must not be empty",
                    ));
                }
                append_namespace_tools(
                    &mut translated,
                    &mut context,
                    name,
                    tools,
                    format!("$.tools[{index}].tools"),
                )?;
            }
            ResponsesTool::Custom {
                name, description, ..
            } => add_function_tool(
                &mut translated,
                &mut context,
                name,
                description.clone(),
                serde_json::json!({
                    "type": "object",
                    "properties": {"input": {"type": "string"}},
                    "required": ["input"]
                }),
                false,
                None,
                ToolKind::Custom,
                format!("$.tools[{index}].name"),
            )?,
            ResponsesTool::ToolSearch {
                description,
                parameters,
                ..
            } => add_function_tool(
                &mut translated,
                &mut context,
                "tool_search",
                description.clone(),
                parameters.clone(),
                false,
                None,
                ToolKind::ToolSearch,
                format!("$.tools[{index}].name"),
            )?,
            ResponsesTool::WebSearch { options } => {
                if !policy.supports_hosted_web_search {
                    return Err(AdapterRequestError::new(
                        AdapterRequestErrorCode::UnsupportedTool,
                        format!("$.tools[{index}].type"),
                        "the selected Chat Completions compatibility profile does not provide hosted web search",
                    ));
                }
                if web_search_options.is_some() {
                    return Err(AdapterRequestError::new(
                        AdapterRequestErrorCode::UnsupportedTool,
                        format!("$.tools[{index}].type"),
                        "only one hosted web search tool can be mapped to Chat Completions",
                    ));
                }
                web_search_options = Some(translate_web_search_options(
                    options,
                    format!("$.tools[{index}]"),
                )?);
            }
        }
    }
    Ok((translated, context, web_search_options))
}

fn append_namespace_tools(
    translated: &mut Vec<ChatTool>,
    context: &mut ToolTranslationContext,
    namespace: &str,
    value: &serde_json::Value,
    path: String,
) -> Result<(), AdapterRequestError> {
    let Some(children) = value
        .as_array()
        .or_else(|| value.get("tools").and_then(serde_json::Value::as_array))
        .or_else(|| value.get("children").and_then(serde_json::Value::as_array))
    else {
        return Err(AdapterRequestError::new(
            AdapterRequestErrorCode::UnsupportedTool,
            path,
            "namespace tools must be an array of function definitions",
        ));
    };
    for (index, child) in children.iter().enumerate() {
        let child_path = format!("{path}[{index}]");
        match child.get("type").and_then(serde_json::Value::as_str) {
            Some("function") => {
                let function = child.get("function").unwrap_or(child);
                let Some(name) = function.get("name").and_then(serde_json::Value::as_str) else {
                    return Err(AdapterRequestError::new(
                        AdapterRequestErrorCode::InvalidToolName,
                        format!("{child_path}.name"),
                        "function name must not be empty",
                    ));
                };
                add_function_tool(
                    translated,
                    context,
                    name,
                    function
                        .get("description")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                    function.get("parameters").cloned().unwrap_or_default(),
                    function
                        .get("strict")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                    Some(namespace.to_owned()),
                    ToolKind::Function,
                    format!("{child_path}.name"),
                )?;
            }
            Some("custom") => {
                let Some(name) = child.get("name").and_then(serde_json::Value::as_str) else {
                    return Err(AdapterRequestError::new(
                        AdapterRequestErrorCode::InvalidToolName,
                        format!("{child_path}.name"),
                        "custom tool name must not be empty",
                    ));
                };
                add_function_tool(
                    translated,
                    context,
                    name,
                    child
                        .get("description")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                    serde_json::json!({
                        "type": "object",
                        "properties": {"input": {"type": "string"}},
                        "required": ["input"]
                    }),
                    false,
                    Some(namespace.to_owned()),
                    ToolKind::Custom,
                    format!("{child_path}.name"),
                )?;
            }
            Some("tool_search") => add_function_tool(
                translated,
                context,
                "tool_search",
                child
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                child.get("parameters").cloned().unwrap_or_default(),
                false,
                Some(namespace.to_owned()),
                ToolKind::ToolSearch,
                format!("{child_path}.name"),
            )?,
            Some("namespace") => {
                let nested_name = child
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .filter(|name| !name.trim().is_empty())
                    .ok_or_else(|| {
                        AdapterRequestError::new(
                            AdapterRequestErrorCode::InvalidToolName,
                            format!("{child_path}.name"),
                            "nested namespace name must not be empty",
                        )
                    })?;
                let nested_namespace = format!("{namespace}.{nested_name}");
                append_namespace_tools(
                    translated,
                    context,
                    &nested_namespace,
                    child.get("tools").unwrap_or(&serde_json::Value::Null),
                    format!("{child_path}.tools"),
                )?;
            }
            _ => {
                return Err(AdapterRequestError::new(
                    AdapterRequestErrorCode::UnsupportedTool,
                    format!("{child_path}.type"),
                    "only function, custom, and tool_search tools inside a namespace can be bridged to Chat Completions",
                ));
            }
        }
    }
    Ok(())
}

fn add_function_tool(
    translated: &mut Vec<ChatTool>,
    context: &mut ToolTranslationContext,
    name: &str,
    description: Option<String>,
    parameters: serde_json::Value,
    strict: bool,
    namespace: Option<String>,
    kind: ToolKind,
    path: String,
) -> Result<(), AdapterRequestError> {
    validate_tool_name(name, path.clone())?;
    let chat_name = namespace
        .as_deref()
        .map(|namespace| flatten_namespace_tool_name(namespace, name))
        .unwrap_or_else(|| name.to_owned());
    validate_tool_name(&chat_name, path.clone())?;
    let identity = ToolIdentity {
        namespace,
        name: name.to_owned(),
    };
    let already_registered = context.resolve_chat_tool(&chat_name).is_some();
    context.register(chat_name.clone(), identity, kind, path)?;
    if already_registered {
        return Ok(());
    }
    translated.push(ChatTool {
        kind: FunctionType::Function,
        function: ChatFunctionDefinition {
            name: chat_name,
            description,
            parameters,
            strict,
        },
    });
    Ok(())
}

fn translate_web_search_options(
    options: &BTreeMap<String, serde_json::Value>,
    path: String,
) -> Result<ChatWebSearchOptions, AdapterRequestError> {
    for key in ["indexed_web_access", "filters", "search_content_types"] {
        if options.get(key).is_some_and(|value| !value.is_null()) {
            return Err(AdapterRequestError::new(
                AdapterRequestErrorCode::UnsupportedParameter,
                format!("{path}.{key}"),
                "this web search option has no Chat Completions equivalent",
            ));
        }
    }
    Ok(ChatWebSearchOptions {
        search_context_size: options
            .get("search_context_size")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        user_location: options.get("user_location").cloned(),
    })
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

fn translate_tool_choice(
    choice: Option<&ToolChoice>,
    tool_context: &ToolTranslationContext,
) -> Option<ChatToolChoice> {
    match choice {
        None => None,
        Some(ToolChoice::Mode(mode)) => Some(ChatToolChoice::Mode(*mode)),
        Some(ToolChoice::Function(named)) => Some(ChatToolChoice::function(
            tool_context.chat_name_for_response_function(&named.name, named.namespace.as_deref()),
        )),
    }
}

pub fn flatten_namespace_tool_name(namespace: &str, name: &str) -> String {
    let full_name = format!(
        "{}__{}",
        sanitize_tool_name(namespace),
        sanitize_tool_name(name)
    );
    if full_name.len() <= 64 {
        return full_name;
    }
    let digest = Sha256::digest(full_name.as_bytes());
    let suffix = format!("__{}", hex::encode(&digest[..8]));
    let prefix_len = 64 - suffix.len();
    format!("{}{}", &full_name[..prefix_len], suffix)
}

fn sanitize_tool_name(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-') {
                byte as char
            } else {
                '_'
            }
        })
        .collect()
}
