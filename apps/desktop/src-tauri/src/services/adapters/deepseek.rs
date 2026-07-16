use super::protocol::{
    ChatCompletionRequest, ChatCompletionResponse, ChatToolChoice, ReasoningEffort,
    ResponsesRequest, ThinkingConfig, ToolChoiceMode,
};
use super::request::CompatibilityPolicy;
use std::collections::BTreeMap;
use std::fmt;

pub const MAX_REASONING_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeepSeekErrorCode {
    AdapterReasoningLimitExceeded,
    UnsupportedToolChoiceInThinking,
    ReasoningUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeepSeekError {
    pub code: DeepSeekErrorCode,
    pub message: String,
}

impl DeepSeekError {
    fn new(code: DeepSeekErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub fn stable_code(&self) -> &'static str {
        match self.code {
            DeepSeekErrorCode::AdapterReasoningLimitExceeded => "ADAPTER_REASONING_LIMIT_EXCEEDED",
            DeepSeekErrorCode::UnsupportedToolChoiceInThinking => "ADAPTER_UNSUPPORTED_TOOL_CHOICE",
            DeepSeekErrorCode::ReasoningUnavailable => "ADAPTER_REASONING_UNAVAILABLE",
        }
    }
}

#[derive(Clone)]
pub struct ReasoningRecord {
    content: String,
}

impl ReasoningRecord {
    pub fn len(&self) -> usize {
        self.content.len()
    }
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }
    pub(crate) fn as_str(&self) -> &str {
        &self.content
    }
}

impl fmt::Debug for ReasoningRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReasoningRecord")
            .field("content", &"<redacted>")
            .field("bytes", &self.content.len())
            .finish()
    }
}

#[derive(Clone, Default)]
pub struct ReasoningHistory {
    max_bytes: usize,
    by_call_id: BTreeMap<String, ReasoningRecord>,
}

impl ReasoningHistory {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            by_call_id: BTreeMap::new(),
        }
    }
    pub fn record_tool_turn(
        &mut self,
        call_ids: &[&str],
        reasoning: &str,
    ) -> Result<(), DeepSeekError> {
        self.record_tool_turn_record(
            call_ids,
            ReasoningRecord {
                content: reasoning.into(),
            },
        )
    }
    pub fn record_tool_turn_record(
        &mut self,
        call_ids: &[&str],
        record: ReasoningRecord,
    ) -> Result<(), DeepSeekError> {
        if record.len() > self.max_bytes {
            return Err(DeepSeekError::new(
                DeepSeekErrorCode::AdapterReasoningLimitExceeded,
                "reasoning content exceeds the configured limit",
            ));
        }
        if call_ids.is_empty() || record.is_empty() {
            return Err(DeepSeekError::new(
                DeepSeekErrorCode::ReasoningUnavailable,
                "tool turn reasoning and call IDs are required",
            ));
        }
        for call_id in call_ids {
            self.by_call_id.insert((*call_id).into(), record.clone());
        }
        Ok(())
    }
    pub(crate) fn get(&self, call_id: &str) -> Option<&ReasoningRecord> {
        self.by_call_id.get(call_id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ThinkingMode {
    Enabled,
    Disabled,
}

#[derive(Clone, Debug)]
pub struct DeepSeekCompatibilityPreset {
    pub upstream_path: &'static str,
    pub policy: CompatibilityPolicy,
    mode: ThinkingMode,
}

impl DeepSeekCompatibilityPreset {
    pub fn thinking_enabled() -> Self {
        Self::new(ThinkingMode::Enabled)
    }
    pub fn thinking_disabled() -> Self {
        Self::new(ThinkingMode::Disabled)
    }
    fn new(mode: ThinkingMode) -> Self {
        Self {
            upstream_path: "/chat/completions",
            policy: CompatibilityPolicy {
                id: "deepseek-chat-completions-v1".into(),
                supports_developer_role: false,
                supports_parallel_tool_calls: true,
                supports_structured_output: true,
                supports_reasoning: true,
                requires_assistant_content_for_tool_calls: true,
                requires_reasoning_for_tool_calls: mode == ThinkingMode::Enabled,
            },
            mode,
        }
    }

    pub fn apply_request(
        &self,
        source: &ResponsesRequest,
        target: &mut ChatCompletionRequest,
    ) -> Result<(), DeepSeekError> {
        target.thinking = Some(ThinkingConfig {
            kind: match self.mode {
                ThinkingMode::Enabled => "enabled",
                ThinkingMode::Disabled => "disabled",
            }
            .into(),
        });
        if self.mode == ThinkingMode::Disabled {
            target.reasoning_effort = None;
            return Ok(());
        }
        target.reasoning_effort = Some(
            map_effort(
                source
                    .reasoning
                    .as_ref()
                    .and_then(|reasoning| reasoning.effort),
            )
            .into(),
        );
        match target.tool_choice.as_ref() {
            Some(ChatToolChoice::Mode(ToolChoiceMode::Auto)) => target.tool_choice = None,
            Some(ChatToolChoice::Mode(ToolChoiceMode::None)) | None => {}
            Some(_) => {
                return Err(DeepSeekError::new(
                    DeepSeekErrorCode::UnsupportedToolChoiceInThinking,
                    "explicit tool choice is unsupported in DeepSeek thinking mode",
                ))
            }
        }
        Ok(())
    }
}

fn map_effort(effort: Option<ReasoningEffort>) -> &'static str {
    match effort {
        Some(ReasoningEffort::Xhigh) => "max",
        Some(ReasoningEffort::Low | ReasoningEffort::Medium | ReasoningEffort::High) | None => {
            "high"
        }
    }
}

pub fn capture_nonstream_reasoning(
    response: &ChatCompletionResponse,
    max_bytes: usize,
) -> Result<Option<ReasoningRecord>, DeepSeekError> {
    let reasoning = response
        .choices
        .first()
        .and_then(|choice| choice.message.reasoning_content.as_ref());
    let Some(reasoning) = reasoning else {
        return Ok(None);
    };
    if reasoning.len() > max_bytes {
        return Err(DeepSeekError::new(
            DeepSeekErrorCode::AdapterReasoningLimitExceeded,
            "reasoning content exceeds the configured limit",
        ));
    }
    Ok(Some(ReasoningRecord {
        content: reasoning.clone(),
    }))
}
