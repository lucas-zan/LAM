use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const MAX_REQUEST_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_SSE_FRAME_BYTES: usize = 256 * 1024;
pub const MAX_TOOL_ARGUMENT_BYTES: usize = 1024 * 1024;
pub const MAX_TOOLS: usize = 128;
pub const MAX_EVENT_CHANNEL_CAPACITY: usize = 64;
pub const MAX_EVENT_CHANNEL_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolErrorCode {
    InvalidJson,
    InvalidType,
    MissingField,
    UnknownField,
    UnsupportedInput,
    UnsupportedTool,
    InvalidIdentifier,
    LimitExceeded,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolError {
    pub code: ProtocolErrorCode,
    pub path: Option<String>,
    pub message: String,
}

impl ProtocolError {
    pub fn new(
        code: ProtocolErrorCode,
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

impl fmt::Debug for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProtocolError")
            .field("code", &self.code)
            .field("path", &self.path)
            .field("message", &self.message)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponsesRequest {
    pub model: String,
    #[serde(deserialize_with = "deserialize_responses_input")]
    pub input: Vec<ResponsesInputItem>,
    #[serde(default)]
    pub instructions: Option<String>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub store: bool,
    #[serde(default)]
    pub previous_response_id: Option<String>,
    #[serde(default)]
    pub parallel_tool_calls: Option<bool>,
    #[serde(default)]
    pub tool_choice: Option<ToolChoice>,
    #[serde(default)]
    pub tools: Vec<ResponsesTool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub prompt_cache_key: Option<String>,
    #[serde(default)]
    pub reasoning: Option<ReasoningConfig>,
    #[serde(default)]
    pub max_output_tokens: Option<u32>,
    #[serde(default)]
    pub text: Option<ResponsesTextConfig>,
    #[serde(default)]
    pub client_metadata: BTreeMap<String, String>,
}

impl fmt::Debug for ResponsesRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResponsesRequest")
            .field("model", &self.model)
            .field("input", &"<redacted>")
            .field("input_count", &self.input.len())
            .field(
                "instructions",
                &self.instructions.as_ref().map(|_| "<redacted>"),
            )
            .field("stream", &self.stream)
            .field("store", &self.store)
            .field("previous_response_id", &self.previous_response_id)
            .field("tools", &"<redacted>")
            .field("tool_count", &self.tools.len())
            .finish()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    Mode(ToolChoiceMode),
    Function(NamedToolChoice),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoiceMode {
    None,
    Auto,
    Required,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedToolChoice {
    #[serde(rename = "type")]
    pub kind: FunctionType,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FunctionType {
    Function,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReasoningConfig {
    #[serde(default)]
    pub effort: Option<ReasoningEffort>,
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
    Xhigh,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponsesTextConfig {
    #[serde(default)]
    pub verbosity: Option<String>,
    #[serde(default)]
    pub format: Option<TextFormat>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextFormat {
    Text,
    JsonObject,
    JsonSchema {
        name: String,
        schema: Value,
        #[serde(default)]
        strict: bool,
    },
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponsesInputItem {
    Message {
        role: ResponsesRole,
        content: Vec<ResponsesContentPart>,
    },
    FunctionCall {
        call_id: String,
        name: String,
        arguments: String,
        #[serde(default)]
        id: Option<String>,
    },
    FunctionCallOutput {
        call_id: String,
        output: String,
    },
    Reasoning {
        #[serde(flatten)]
        options: BTreeMap<String, Value>,
    },
}

impl fmt::Debug for ResponsesInputItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message { role, content } => f
                .debug_struct("Message")
                .field("role", role)
                .field("content", &"<redacted>")
                .field("content_count", &content.len())
                .finish(),
            Self::FunctionCall {
                call_id, name, id, ..
            } => f
                .debug_struct("FunctionCall")
                .field("call_id", call_id)
                .field("name", name)
                .field("arguments", &"<redacted>")
                .field("id", id)
                .finish(),
            Self::FunctionCallOutput { call_id, .. } => f
                .debug_struct("FunctionCallOutput")
                .field("call_id", call_id)
                .field("output", &"<redacted>")
                .finish(),
            Self::Reasoning { options } => f
                .debug_struct("Reasoning")
                .field("options", &"<redacted>")
                .field("option_count", &options.len())
                .finish(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResponsesRole {
    System,
    Developer,
    User,
    Assistant,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponsesContentPart {
    InputText {
        text: String,
    },
    OutputText {
        text: String,
    },
    InputImage {
        image_url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponsesTool {
    Function {
        name: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        parameters: Value,
        #[serde(default)]
        strict: bool,
    },
    Namespace {
        name: String,
        #[serde(default)]
        description: Option<String>,
        tools: Value,
    },
    WebSearch {
        #[serde(flatten)]
        options: BTreeMap<String, Value>,
    },
}

fn deserialize_responses_input<'de, D>(deserializer: D) -> Result<Vec<ResponsesInputItem>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum WireInput {
        Text(String),
        Items(Vec<ResponsesInputItem>),
    }
    match WireInput::deserialize(deserializer)? {
        WireInput::Text(text) => Ok(vec![ResponsesInputItem::Message {
            role: ResponsesRole::User,
            content: vec![ResponsesContentPart::InputText { text }],
        }]),
        WireInput::Items(items) => Ok(items),
    }
}

/// Minimal routing metadata extracted from a Responses request without
/// validating the shape of `input` items.
///
/// The passthrough route forwards the original request bytes verbatim to a
/// Responses-native upstream, so it must not reject requests merely because
/// they contain input/output item types that this crate does not model
/// (e.g. `web_search_call`, `computer_call`, and future OpenAI additions).
/// Only the fields needed for routing and binding policy are read here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponsesPassthroughMetadata {
    pub model: String,
    pub stream: bool,
    pub store: bool,
    pub previous_response_id: Option<String>,
    pub input_empty: bool,
}

pub fn parse_responses_passthrough(
    bytes: &[u8],
) -> Result<ResponsesPassthroughMetadata, ProtocolError> {
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(ProtocolError::new(
            ProtocolErrorCode::LimitExceeded,
            "$",
            "request body exceeds the configured limit",
        ));
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|error| {
        ProtocolError::new(ProtocolErrorCode::InvalidJson, "$", error.to_string())
    })?;
    let object = value.as_object().ok_or_else(|| {
        ProtocolError::new(
            ProtocolErrorCode::InvalidType,
            "$",
            "request must be an object",
        )
    })?;
    let model = object
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ProtocolError::new(
                ProtocolErrorCode::MissingField,
                "$.model",
                "request model is required",
            )
        })?
        .to_string();
    let stream = object
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let store = object
        .get("store")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let previous_response_id = object
        .get("previous_response_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let input_empty = match object.get("input") {
        None | Some(Value::Null) => true,
        Some(Value::String(text)) => text.is_empty(),
        Some(Value::Array(items)) => items.is_empty(),
        Some(_) => false,
    };
    Ok(ResponsesPassthroughMetadata {
        model,
        stream,
        store,
        previous_response_id,
        input_empty,
    })
}

/// Token usage extracted from a Responses API response body.
///
/// OpenAI's Responses API reports usage as `input_tokens`/`output_tokens`/
/// `total_tokens`. This reads those fields leniently for metrics only; the
/// original response bytes are still forwarded unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResponsesUsageTokens {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

pub fn extract_responses_usage(value: &Value) -> Option<ResponsesUsageTokens> {
    let usage = value.get("usage")?.as_object()?;
    let field = |key: &str| usage.get(key).and_then(Value::as_u64);
    let input_tokens = field("input_tokens").unwrap_or(0);
    let output_tokens = field("output_tokens").unwrap_or(0);
    let total_tokens = field("total_tokens").unwrap_or(input_tokens + output_tokens);
    if input_tokens == 0 && output_tokens == 0 && total_tokens == 0 {
        return None;
    }
    Some(ResponsesUsageTokens {
        input_tokens,
        output_tokens,
        total_tokens,
    })
}

pub fn parse_responses_request(bytes: &[u8]) -> Result<ResponsesRequest, ProtocolError> {
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(ProtocolError::new(
            ProtocolErrorCode::LimitExceeded,
            "$",
            "request body exceeds the configured limit",
        ));
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|error| {
        ProtocolError::new(ProtocolErrorCode::InvalidJson, "$", error.to_string())
    })?;
    validate_request_shape(&value)?;
    serde_json::from_value(value)
        .map_err(|error| ProtocolError::new(ProtocolErrorCode::InvalidType, "$", error.to_string()))
}

fn validate_request_shape(value: &Value) -> Result<(), ProtocolError> {
    let object = value.as_object().ok_or_else(|| {
        ProtocolError::new(
            ProtocolErrorCode::InvalidType,
            "$",
            "request must be an object",
        )
    })?;
    let known = BTreeSet::from([
        "model",
        "input",
        "instructions",
        "stream",
        "store",
        "previous_response_id",
        "parallel_tool_calls",
        "tool_choice",
        "tools",
        "include",
        "prompt_cache_key",
        "reasoning",
        "max_output_tokens",
        "text",
        "client_metadata",
    ]);
    if let Some(field) = object.keys().find(|field| !known.contains(field.as_str())) {
        return Err(ProtocolError::new(
            ProtocolErrorCode::UnknownField,
            format!("$.{field}"),
            "unknown request field",
        ));
    }
    validate_input_parts(object.get("input"))?;
    validate_tool_limits(object.get("tools"))
}

fn validate_input_parts(input: Option<&Value>) -> Result<(), ProtocolError> {
    let Some(items) = input.and_then(Value::as_array) else {
        return Ok(());
    };
    for (item_index, item) in items.iter().enumerate() {
        let Some(parts) = item.get("content").and_then(Value::as_array) else {
            continue;
        };
        for (part_index, part) in parts.iter().enumerate() {
            let kind = part.get("type").and_then(Value::as_str).unwrap_or("");
            if !matches!(kind, "input_text" | "output_text" | "input_image") {
                return Err(ProtocolError::new(
                    ProtocolErrorCode::UnsupportedInput,
                    format!("$.input[{item_index}].content[{part_index}].type"),
                    format!("unsupported content type: {kind}"),
                ));
            }
        }
    }
    Ok(())
}

fn validate_tool_limits(tools: Option<&Value>) -> Result<(), ProtocolError> {
    let Some(tools) = tools.and_then(Value::as_array) else {
        return Ok(());
    };
    if tools.len() > MAX_TOOLS {
        return Err(ProtocolError::new(
            ProtocolErrorCode::LimitExceeded,
            "$.tools",
            "tool count exceeds the configured limit",
        ));
    }
    for (index, tool) in tools.iter().enumerate() {
        let bytes = serde_json::to_vec(tool).unwrap_or_default().len();
        if bytes > MAX_TOOL_ARGUMENT_BYTES {
            return Err(ProtocolError::new(
                ProtocolErrorCode::LimitExceeded,
                format!("$.tools[{index}]"),
                "tool definition exceeds the configured limit",
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub stream_options: Option<ChatStreamOptions>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub tools: Vec<ChatTool>,
    #[serde(default)]
    pub tool_choice: Option<ChatToolChoice>,
    #[serde(default)]
    pub parallel_tool_calls: Option<bool>,
    #[serde(default)]
    pub response_format: Option<ChatResponseFormat>,
    #[serde(default)]
    pub thinking: Option<ThinkingConfig>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatStreamOptions {
    #[serde(default)]
    pub include_usage: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatToolChoice {
    Mode(ToolChoiceMode),
    Function {
        #[serde(rename = "type")]
        kind: FunctionType,
        function: ChatNamedFunction,
    },
}

impl ChatToolChoice {
    pub fn function(name: String) -> Self {
        Self::Function {
            kind: FunctionType::Function,
            function: ChatNamedFunction { name },
        }
    }
    pub fn function_name(&self) -> Option<&str> {
        match self {
            Self::Function { function, .. } => Some(&function.name),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatNamedFunction {
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum ChatMessage {
    System {
        content: String,
    },
    Developer {
        content: String,
    },
    User {
        content: ChatUserContent,
    },
    Assistant {
        content: Option<String>,
        #[serde(default)]
        reasoning_content: Option<String>,
        #[serde(default)]
        tool_calls: Vec<ChatToolCall>,
    },
    Tool {
        content: String,
        tool_call_id: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatUserContent {
    Text(String),
    Parts(Vec<ChatContentPart>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatContentPart {
    Text { text: String },
    ImageUrl { image_url: ChatImageUrl },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatImageUrl {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatTool {
    #[serde(rename = "type")]
    pub kind: FunctionType,
    pub function: ChatFunctionDefinition,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatFunctionDefinition {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parameters: Value,
    #[serde(default)]
    pub strict: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatResponseFormat {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub json_schema: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThinkingConfig {
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatChoice>,
    #[serde(default)]
    pub usage: Option<ChatUsage>,
    #[serde(default)]
    pub system_fingerprint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatChoice {
    pub index: u32,
    pub message: ChatAssistantMessage,
    pub finish_reason: Option<String>,
    #[serde(default)]
    pub logprobs: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatAssistantMessage {
    pub role: String,
    pub content: Option<String>,
    #[serde(default)]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ChatToolCall>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: FunctionType,
    pub function: ChatFunctionCall,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    #[serde(default)]
    pub prompt_tokens_details: Option<PromptTokenDetails>,
    #[serde(default)]
    pub completion_tokens_details: Option<CompletionTokenDetails>,
    #[serde(default)]
    pub prompt_cache_hit_tokens: Option<u64>,
    #[serde(default)]
    pub prompt_cache_miss_tokens: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PromptTokenDetails {
    #[serde(default)]
    pub cached_tokens: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletionTokenDetails {
    #[serde(default)]
    pub reasoning_tokens: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatChunk {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatChunkChoice>,
    #[serde(default)]
    pub usage: Option<ChatUsage>,
    #[serde(default)]
    pub system_fingerprint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatChunkChoice {
    pub index: u32,
    pub delta: ChatDelta,
    #[serde(default)]
    pub finish_reason: Option<String>,
    #[serde(default)]
    pub logprobs: Option<Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatDelta {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub reasoning_content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ChatToolCallDelta>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatToolCallDelta {
    pub index: u32,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "type")]
    pub kind: Option<FunctionType>,
    #[serde(default)]
    pub function: Option<ChatFunctionCallDelta>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatFunctionCallDelta {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatErrorEnvelope {
    pub error: ChatErrorBody,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatErrorBody {
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub param: Option<String>,
}
