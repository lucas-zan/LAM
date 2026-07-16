use super::protocol::{ChatCompletionResponse, ChatUsage, MAX_TOOL_ARGUMENT_BYTES};
use serde::{Deserialize, Serialize};

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
        arguments: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponsesOutputContent {
    OutputText {
        text: String,
        annotations: Vec<String>,
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
    if choice.finish_reason.as_deref() != Some("tool_calls") || choice.message.content.is_some() {
        output.push(message_item(
            choice.message.content.as_deref().unwrap_or(""),
            context,
        ));
    }
    for call in choice.message.tool_calls.as_deref().unwrap_or_default() {
        validate_call(&call.id, &call.function.name, &call.function.arguments)?;
        output.push(ResponsesOutputItem::FunctionCall {
            id: context.next_id("fc"),
            status: ResponsesStatus::Completed,
            call_id: call.id.clone(),
            name: call.function.name.clone(),
            arguments: call.function.arguments.clone(),
        });
    }
    if choice.finish_reason.as_deref() == Some("tool_calls") && output.is_empty() {
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

fn message_item(content: &str, context: &mut DeterministicContext) -> ResponsesOutputItem {
    ResponsesOutputItem::Message {
        id: context.next_id("msg"),
        status: ResponsesStatus::Completed,
        role: "assistant".into(),
        content: vec![ResponsesOutputContent::OutputText {
            text: content.into(),
            annotations: Vec::new(),
        }],
    }
}

fn finish_status(
    reason: Option<&str>,
) -> Result<(ResponsesStatus, Option<IncompleteDetails>), NonstreamError> {
    match reason {
        Some("stop" | "tool_calls") => Ok((ResponsesStatus::Completed, None)),
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
        _ => Err(NonstreamError::new(
            NonstreamErrorCode::UnknownFinishReason,
            "unknown or missing upstream finish reason",
        )),
    }
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
