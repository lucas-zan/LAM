use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelayCompatibilityDisposition {
    Compatible,
    CompatibleWithLoss,
    Blocked,
}

impl std::fmt::Display for RelayCompatibilityDisposition {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Compatible => "compatible",
            Self::CompatibleWithLoss => "compatible_with_loss",
            Self::Blocked => "blocked",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RelayHistoryItemKind {
    Text,
    FunctionCall { call_id: String, verified: bool },
    FunctionResult { call_id: String },
    RepresentationMetadata,
    PreviousResponseState,
    EncryptedReasoning,
    HostedTool,
    McpTool,
    ComputerUse,
    Image,
    Audio,
    File,
    Unknown,
    Corrupt,
}

impl RelayHistoryItemKind {
    fn label(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::FunctionCall { .. } => "function_call",
            Self::FunctionResult { .. } => "function_result",
            Self::RepresentationMetadata => "representation_metadata",
            Self::PreviousResponseState => "previous_response_state",
            Self::EncryptedReasoning => "encrypted_reasoning",
            Self::HostedTool => "hosted_tool",
            Self::McpTool => "mcp_tool",
            Self::ComputerUse => "computer_use",
            Self::Image => "image",
            Self::Audio => "audio",
            Self::File => "file",
            Self::Unknown => "unknown",
            Self::Corrupt => "corrupt",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RelayHistoryItem {
    pub id: String,
    pub kind: RelayHistoryItemKind,
}

impl RelayHistoryItem {
    pub fn new(id: impl Into<String>, kind: RelayHistoryItemKind) -> Self {
        Self {
            id: id.into(),
            kind,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RelayTargetCapabilities {
    pub function_tools: bool,
    pub representation_metadata: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RelayCompatibilityIssue {
    pub item_id: String,
    pub item_type: String,
    pub reason: String,
    pub recovery_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RelayTransformation {
    pub item_id: String,
    pub item_type: String,
    pub action: String,
    pub warning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RelayCompatibilityReport {
    pub disposition: RelayCompatibilityDisposition,
    pub confirmation_required: bool,
    pub issues: Vec<RelayCompatibilityIssue>,
    pub transformations: Vec<RelayTransformation>,
}

/// Pure, credential-free analysis over already-classified session history.
pub fn analyze_relay_compatibility(
    items: &[RelayHistoryItem],
    target: &RelayTargetCapabilities,
) -> RelayCompatibilityReport {
    let mut issues = Vec::new();
    let mut transformations = Vec::new();
    let mut calls: BTreeMap<&str, Vec<&RelayHistoryItem>> = BTreeMap::new();
    let mut results: BTreeMap<&str, Vec<&RelayHistoryItem>> = BTreeMap::new();

    for item in items {
        match &item.kind {
            RelayHistoryItemKind::Text => {}
            RelayHistoryItemKind::FunctionCall { call_id, verified } => {
                calls.entry(call_id).or_default().push(item);
                if !target.function_tools {
                    issues.push(issue(
                        item,
                        "target route has no verified function-tool mapping",
                    ));
                } else if !verified {
                    issues.push(issue(item, "function call mapping is not verified"));
                }
            }
            RelayHistoryItemKind::FunctionResult { call_id } => {
                results.entry(call_id).or_default().push(item);
                if !target.function_tools {
                    issues.push(issue(
                        item,
                        "target route has no verified function-tool mapping",
                    ));
                }
            }
            RelayHistoryItemKind::RepresentationMetadata => {
                if !target.representation_metadata {
                    transformations.push(RelayTransformation {
                        item_id: item.id.clone(),
                        item_type: item.kind.label().into(),
                        action: "drop_non_protocol_metadata".into(),
                        warning: "display-only metadata is unavailable on the target route".into(),
                    });
                }
            }
            _ => issues.push(issue(item, blocked_reason(&item.kind))),
        }
    }

    for (call_id, call_items) in &calls {
        let result_items = results.get(call_id).map(Vec::as_slice).unwrap_or_default();
        if call_items.len() != 1 || result_items.len() != 1 {
            for item in call_items.iter().chain(result_items.iter()) {
                if !issues.iter().any(|issue| issue.item_id == item.id) {
                    issues.push(issue(
                        item,
                        "function tool state is incomplete or ambiguous",
                    ));
                }
            }
        }
    }
    for (call_id, result_items) in &results {
        if !calls.contains_key(call_id) {
            for item in result_items {
                if !issues.iter().any(|issue| issue.item_id == item.id) {
                    issues.push(issue(item, "function result has no matching call"));
                }
            }
        }
    }

    let disposition = if !issues.is_empty() {
        RelayCompatibilityDisposition::Blocked
    } else if !transformations.is_empty() {
        RelayCompatibilityDisposition::CompatibleWithLoss
    } else {
        RelayCompatibilityDisposition::Compatible
    };
    RelayCompatibilityReport {
        confirmation_required: disposition == RelayCompatibilityDisposition::CompatibleWithLoss,
        disposition,
        issues,
        transformations,
    }
}

/// Classifies Codex JSONL without retaining message bodies, tool arguments, or outputs.
pub fn classify_codex_relay_history(bytes: &[u8]) -> Vec<RelayHistoryItem> {
    let Ok(body) = std::str::from_utf8(bytes) else {
        return vec![RelayHistoryItem::new(
            "document",
            RelayHistoryItemKind::Corrupt,
        )];
    };
    body.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .flat_map(|(index, line)| classify_line(index, line))
        .collect()
}

fn classify_line(index: usize, line: &str) -> Vec<RelayHistoryItem> {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return vec![RelayHistoryItem::new(
            format!("line-{}", index + 1),
            RelayHistoryItemKind::Corrupt,
        )];
    };
    let payload = value.get("payload").unwrap_or(&value);
    let id = safe_item_id(payload, index);
    if contains_non_null_key(&value, "previous_response_id") {
        return vec![RelayHistoryItem::new(
            id,
            RelayHistoryItemKind::PreviousResponseState,
        )];
    }
    let envelope_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let item_type = if envelope_type == "response_item" {
        payload
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
    } else {
        envelope_type
    };
    let kind = match item_type {
        "session_meta" | "turn_context" | "token_count" | "task_started" | "task_complete" => {
            RelayHistoryItemKind::RepresentationMetadata
        }
        "event_msg" => match payload.get("type").and_then(Value::as_str) {
            Some("user_message" | "agent_message") => RelayHistoryItemKind::Text,
            _ => RelayHistoryItemKind::RepresentationMetadata,
        },
        "message" | "response" => classify_message(payload),
        "function_call" => RelayHistoryItemKind::FunctionCall {
            call_id: call_id(payload, index),
            verified: true,
        },
        "function_call_output" => RelayHistoryItemKind::FunctionResult {
            call_id: call_id(payload, index),
        },
        "reasoning" => {
            if contains_non_null_key(payload, "encrypted_content")
                || contains_non_null_key(payload, "encryptedContent")
            {
                RelayHistoryItemKind::EncryptedReasoning
            } else {
                RelayHistoryItemKind::RepresentationMetadata
            }
        }
        "web_search_call" | "hosted_tool_call" | "custom_tool_call" | "custom_tool_call_output" => {
            RelayHistoryItemKind::HostedTool
        }
        "mcp_call" | "mcp_tool_call" | "mcp_tool_call_output" => RelayHistoryItemKind::McpTool,
        "computer_call" | "computer_call_output" | "computer_use" => {
            RelayHistoryItemKind::ComputerUse
        }
        "input_image" | "output_image" | "image" => RelayHistoryItemKind::Image,
        "input_audio" | "output_audio" | "audio" => RelayHistoryItemKind::Audio,
        "input_file" | "file" => RelayHistoryItemKind::File,
        "" if value.get("session_id").is_some() || value.get("cwd").is_some() => {
            RelayHistoryItemKind::RepresentationMetadata
        }
        _ => RelayHistoryItemKind::Unknown,
    };
    vec![RelayHistoryItem::new(id, kind)]
}

fn classify_message(payload: &Value) -> RelayHistoryItemKind {
    let content_types = payload
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|part| part.get("type").and_then(Value::as_str));
    for content_type in content_types {
        match content_type {
            "input_image" | "output_image" | "image" => return RelayHistoryItemKind::Image,
            "input_audio" | "output_audio" | "audio" => return RelayHistoryItemKind::Audio,
            "input_file" | "file" => return RelayHistoryItemKind::File,
            _ => {}
        }
    }
    RelayHistoryItemKind::Text
}

fn safe_item_id(value: &Value, index: usize) -> String {
    value
        .get("id")
        .or_else(|| value.get("call_id"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 256)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("line-{}", index + 1))
}

fn call_id(value: &Value, index: usize) -> String {
    value
        .get("call_id")
        .or_else(|| value.get("callId"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 256)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("missing-call-id-{}", index + 1))
}

fn contains_non_null_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(current, nested)| {
            (current == key && !nested.is_null()) || contains_non_null_key(nested, key)
        }),
        Value::Array(values) => values
            .iter()
            .any(|nested| contains_non_null_key(nested, key)),
        _ => false,
    }
}

fn issue(item: &RelayHistoryItem, reason: impl Into<String>) -> RelayCompatibilityIssue {
    RelayCompatibilityIssue {
        item_id: item.id.clone(),
        item_type: item.kind.label().into(),
        reason: reason.into(),
        recovery_action: "resume on the original Provider or start a new target session".into(),
    }
}

fn blocked_reason(kind: &RelayHistoryItemKind) -> &'static str {
    match kind {
        RelayHistoryItemKind::PreviousResponseState => {
            "previous-response state cannot be reconstructed without a response store"
        }
        RelayHistoryItemKind::EncryptedReasoning => {
            "encrypted reasoning is bound to the original runtime"
        }
        RelayHistoryItemKind::HostedTool => "hosted tool state is not portable",
        RelayHistoryItemKind::McpTool => "MCP tool state is not portable",
        RelayHistoryItemKind::ComputerUse => "computer-use state is not portable",
        RelayHistoryItemKind::Image => "image history has no verified target mapping",
        RelayHistoryItemKind::Audio => "audio history has no verified target mapping",
        RelayHistoryItemKind::File => "file history has no verified target mapping",
        RelayHistoryItemKind::Unknown => "unknown session item fails closed",
        RelayHistoryItemKind::Corrupt => "corrupt session item fails closed",
        _ => "session item is unsupported by the target route",
    }
}
