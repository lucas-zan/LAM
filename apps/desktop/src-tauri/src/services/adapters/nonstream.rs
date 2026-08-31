use super::protocol::{ChatCompletionResponse, ChatUsage, MAX_TOOL_ARGUMENT_BYTES};
use super::request::{ToolIdentity, ToolKind, ToolTranslationContext};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponsesStatus {
    Completed,
    Incomplete,
    Failed,
    Cancelled,
    InProgress,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsesResponse {
    pub id: String,
    pub object: String,
    pub created_at: i64,
    pub status: ResponsesStatus,
    pub model: String,
    pub output: Vec<ResponsesOutputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ResponsesUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incomplete_details: Option<IncompleteDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ResponsesErrorBody>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponsesOutputItem {
    Message {
        id: String,
        status: ResponsesStatus,
        role: String,
        content: Vec<ResponsesOutputContent>,
    },
    FunctionCall {
        id: String,
        status: ResponsesStatus,
        call_id: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        namespace: Option<String>,
        arguments: String,
    },
    CustomToolCall {
        id: String,
        status: ResponsesStatus,
        call_id: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        namespace: Option<String>,
        input: String,
    },
    ToolSearchCall {
        id: String,
        status: ResponsesStatus,
        call_id: String,
        execution: String,
        arguments: Value,
    },
    LocalShellCall {
        id: String,
        status: ResponsesStatus,
        call_id: String,
        action: Value,
    },
    WebSearchCall {
        id: String,
        status: ResponsesStatus,
        action: Option<Value>,
    },
    Reasoning {
        id: String,
        status: ResponsesStatus,
        summary: Vec<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        encrypted_content: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponsesOutputContent {
    OutputText {
        text: String,
        annotations: Vec<Value>,
    },
}

impl ResponsesOutputContent {
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::OutputText { text, .. } => Some(text),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsesUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens_details: Option<ResponsesInputTokenDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens_details: Option<ResponsesOutputTokenDetails>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsesInputTokenDetails {
    pub cached_tokens: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsesOutputTokenDetails {
    pub reasoning_tokens: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncompleteDetails {
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponsesErrorBody {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NonstreamErrorCode {
    ChoiceCount,
    ChoiceIndex,
    ModelMismatch,
    UnknownFinishReason,
    InvalidToolCall,
    ToolArgumentsLimitExceeded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NonstreamError {
    pub code: NonstreamErrorCode,
    pub message: String,
}

impl NonstreamError {
    fn new(code: NonstreamErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

pub struct DeterministicContext {
    created_at: i64,
    response_id: String,
    item_index: usize,
}

impl DeterministicContext {
    pub fn new(created_at: i64, response_id: impl Into<String>) -> Self {
        Self {
            created_at,
            response_id: response_id.into(),
            item_index: 0,
        }
    }
    fn next_id(&mut self, prefix: &str) -> String {
        self.item_index += 1;
        format!("{prefix}-{}-{}", self.response_id, self.item_index)
    }
}

pub fn convert_nonstream_response(
    source: &ChatCompletionResponse,
    expected_model: &str,
    context: &mut DeterministicContext,
) -> Result<ResponsesResponse, NonstreamError> {
    convert_nonstream_response_with_tools(
        source,
        expected_model,
        context,
        &ToolTranslationContext::default(),
    )
}

pub fn convert_nonstream_response_with_tools(
    source: &ChatCompletionResponse,
    expected_model: &str,
    context: &mut DeterministicContext,
    tool_context: &ToolTranslationContext,
) -> Result<ResponsesResponse, NonstreamError> {
    if source.model != expected_model {
        return Err(NonstreamError::new(
            NonstreamErrorCode::ModelMismatch,
            "upstream model does not match the binding",
        ));
    }
    if source.choices.len() != 1 {
        return Err(NonstreamError::new(
            NonstreamErrorCode::ChoiceCount,
            "exactly one upstream choice is required",
        ));
    }
    let choice = &source.choices[0];
    if choice.index != 0 {
        return Err(NonstreamError::new(
            NonstreamErrorCode::ChoiceIndex,
            "upstream choice index must be zero",
        ));
    }
    let (status, incomplete_details) = finish_status(choice.finish_reason.as_deref())?;
    let mut output = Vec::new();
    if !is_tool_finish_reason(choice.finish_reason.as_deref())
        || choice.message.content.is_some()
        || !choice.message.annotations.is_empty()
        || choice.message.refusal.is_some()
    {
        output.push(message_item_with_annotations(
            choice
                .message
                .content
                .as_deref()
                .or(choice.message.refusal.as_deref())
                .unwrap_or(""),
            map_annotations(&choice.message.annotations),
            context,
        ));
    }
    let legacy_call =
        choice
            .message
            .function_call
            .as_ref()
            .map(|function| super::protocol::ChatToolCall {
                id: format!("call-{}", context.response_id),
                kind: super::protocol::FunctionType::Function,
                function: function.clone(),
            });
    let mut tool_calls = choice
        .message
        .tool_calls
        .as_deref()
        .unwrap_or_default()
        .iter()
        .collect::<Vec<_>>();
    if tool_calls.is_empty() {
        tool_calls.extend(legacy_call.iter());
    }
    for call in tool_calls {
        validate_call(&call.id, &call.function.name, &call.function.arguments)?;
        let identity = tool_context
            .resolve_chat_tool(&call.function.name)
            .cloned()
            .unwrap_or_else(|| ToolIdentity {
                namespace: None,
                name: call.function.name.clone(),
            });
        match tool_context
            .tool_kind(&call.function.name)
            .or_else(|| (call.function.name == "local_shell").then_some(ToolKind::LocalShell))
            .unwrap_or(ToolKind::Function)
        {
            ToolKind::Custom => output.push(ResponsesOutputItem::CustomToolCall {
                id: context.next_id("ctc"),
                status: ResponsesStatus::Completed,
                call_id: call.id.clone(),
                name: identity.name,
                namespace: identity.namespace,
                input: unwrap_custom_input(&call.function.arguments),
            }),
            ToolKind::ToolSearch => output.push(ResponsesOutputItem::ToolSearchCall {
                id: context.next_id("tsc"),
                status: ResponsesStatus::Completed,
                call_id: call.id.clone(),
                execution: "client".into(),
                arguments: parse_arguments(&call.function.arguments)?,
            }),
            ToolKind::LocalShell => output.push(ResponsesOutputItem::LocalShellCall {
                id: context.next_id("lsh"),
                status: ResponsesStatus::Completed,
                call_id: call.id.clone(),
                action: parse_arguments(&call.function.arguments)?,
            }),
            ToolKind::Function => output.push(ResponsesOutputItem::FunctionCall {
                id: context.next_id("fc"),
                status: ResponsesStatus::Completed,
                call_id: call.id.clone(),
                name: identity.name,
                namespace: identity.namespace,
                arguments: call.function.arguments.clone(),
            }),
        }
    }
    if is_tool_finish_reason(choice.finish_reason.as_deref()) && output.is_empty() {
        return Err(NonstreamError::new(
            NonstreamErrorCode::InvalidToolCall,
            "tool_calls finish reason requires a function call",
        ));
    }
    Ok(ResponsesResponse {
        id: context.response_id.clone(),
        object: "response".into(),
        created_at: context.created_at,
        status,
        model: expected_model.into(),
        output,
        usage: source.usage.as_ref().map(map_usage),
        incomplete_details,
        error: None,
    })
}

fn message_item_with_annotations(
    content: &str,
    annotations: Vec<Value>,
    context: &mut DeterministicContext,
) -> ResponsesOutputItem {
    ResponsesOutputItem::Message {
        id: context.next_id("msg"),
        status: ResponsesStatus::Completed,
        role: "assistant".into(),
        content: vec![ResponsesOutputContent::OutputText {
            text: content.into(),
            annotations,
        }],
    }
}

pub(crate) fn unwrap_custom_input(arguments: &str) -> String {
    serde_json::from_str::<Value>(arguments)
        .ok()
        .and_then(|value| value.get("input").cloned())
        .map(|value| match value {
            Value::String(text) => text,
            other => serde_json::to_string(&other).unwrap_or_default(),
        })
        .unwrap_or_else(|| arguments.to_owned())
}

fn parse_arguments(arguments: &str) -> Result<Value, NonstreamError> {
    serde_json::from_str(arguments).map_err(|_| {
        NonstreamError::new(
            NonstreamErrorCode::InvalidToolCall,
            "upstream tool arguments are not valid JSON",
        )
    })
}

pub(crate) fn map_annotations(annotations: &[Value]) -> Vec<Value> {
    annotations
        .iter()
        .map(|annotation| {
            if annotation.get("type").and_then(Value::as_str) == Some("url_citation") {
                if let Some(citation) = annotation.get("url_citation").and_then(Value::as_object) {
                    let mut mapped = citation.clone();
                    mapped.insert("type".into(), Value::String("url_citation".into()));
                    return Value::Object(mapped);
                }
            }
            annotation.clone()
        })
        .collect()
}

fn finish_status(
    reason: Option<&str>,
) -> Result<(ResponsesStatus, Option<IncompleteDetails>), NonstreamError> {
    match reason {
        Some("stop" | "tool_calls" | "function_call") => Ok((ResponsesStatus::Completed, None)),
        Some("length") => Ok((
            ResponsesStatus::Incomplete,
            Some(IncompleteDetails {
                reason: "max_output_tokens".into(),
            }),
        )),
        Some("content_filter") => Ok((
            ResponsesStatus::Incomplete,
            Some(IncompleteDetails {
                reason: "content_filter".into(),
            }),
        )),
        Some("insufficient_system_resource") => Ok((
            ResponsesStatus::Incomplete,
            Some(IncompleteDetails {
                reason: "insufficient_system_resource".into(),
            }),
        )),
        _ => Err(NonstreamError::new(
            NonstreamErrorCode::UnknownFinishReason,
            "unknown or missing upstream finish reason",
        )),
    }
}

fn is_tool_finish_reason(reason: Option<&str>) -> bool {
    matches!(reason, Some("tool_calls" | "function_call"))
}

fn validate_call(id: &str, name: &str, arguments: &str) -> Result<(), NonstreamError> {
    if id.is_empty()
        || name.is_empty()
        || serde_json::from_str::<serde_json::Value>(arguments).is_err()
    {
        return Err(NonstreamError::new(
            NonstreamErrorCode::InvalidToolCall,
            "invalid upstream function call",
        ));
    }
    if arguments.len() > MAX_TOOL_ARGUMENT_BYTES {
        return Err(NonstreamError::new(
            NonstreamErrorCode::ToolArgumentsLimitExceeded,
            "tool arguments exceed the configured limit",
        ));
    }
    Ok(())
}

pub fn map_usage(usage: &ChatUsage) -> ResponsesUsage {
    ResponsesUsage {
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
        input_tokens_details: usage.prompt_tokens_details.as_ref().map(|details| {
            ResponsesInputTokenDetails {
                cached_tokens: details.cached_tokens,
            }
        }),
        output_tokens_details: usage.completion_tokens_details.as_ref().map(|details| {
            ResponsesOutputTokenDetails {
                reasoning_tokens: details.reasoning_tokens,
            }
        }),
    }
}
