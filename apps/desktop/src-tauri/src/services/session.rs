use super::account::find_account;
use super::error::Result;
use super::types::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSession {
    pub id: String,
    pub account_id: String,
    pub path: PathBuf,
    pub modified_at: u64,
    pub size_bytes: u64,
    pub cwd: Option<String>,
    pub summary: Option<String>,
    pub first_user_message: Option<String>,
    pub model: Option<String>,
    pub original_provider_id: Option<String>,
    pub original_model: Option<String>,
    pub current_provider_id: Option<String>,
    pub current_model: Option<String>,
    pub provider_mismatch: bool,
}

pub fn list_sessions(home_root: &Path, profile_id: &str) -> Result<Vec<CodexSession>> {
    let account = find_account(home_root, profile_id)?;
    let mut sessions = Vec::new();
    for file in session_files(&account.codex_home.join("sessions"))? {
        let metadata = fs::metadata(&file)?;
        let first_line = read_first_line(&file)?;
        let snippet = read_tail(&file, 256 * 1024)?;
        let fallback_id = file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        let original_provider_id = extract_json_string(
            &snippet,
            &[
                "provider_id",
                "providerId",
                "original_provider_id",
                "originalProviderId",
            ],
        )
        .or_else(|| account.provider_id.clone());
        let original_model =
            extract_json_string(&snippet, &["model", "original_model", "originalModel"]);
        let current_provider_id = account.provider_id.clone();
        let current_model = account.model.clone();
        let provider_mismatch = match (&original_provider_id, &current_provider_id) {
            (Some(original), Some(current)) => original != current,
            _ => false,
        };
        sessions.push(CodexSession {
            id: extract_session_meta_payload_string(&first_line, "id")
                .or_else(|| {
                    extract_json_string(
                        &snippet,
                        &[
                            "session_id",
                            "sessionId",
                            "conversation_id",
                            "conversationId",
                            "id",
                        ],
                    )
                })
                .unwrap_or(fallback_id),
            account_id: account.id.clone(),
            path: file,
            modified_at: metadata.modified().ok().map(system_secs).unwrap_or(0),
            size_bytes: metadata.len(),
            cwd: extract_json_string(
                &snippet,
                &[
                    "cwd",
                    "workdir",
                    "working_directory",
                    "workingDirectory",
                    "current_dir",
                ],
            ),
            summary: extract_json_string(
                &snippet,
                &["summary", "title", "text", "content", "message"],
            ),
            first_user_message: extract_json_string(
                &snippet,
                &["first_user_message", "firstUserMessage"],
            ),
            model: original_model.clone().or_else(|| current_model.clone()),
            original_provider_id,
            original_model,
            current_provider_id,
            current_model,
            provider_mismatch,
        });
    }
    sessions.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(sessions)
}

fn extract_session_meta_payload_string(line: &str, key: &str) -> Option<String> {
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }
    value
        .get("payload")
        .and_then(|payload| payload.get(key))
        .and_then(Value::as_str)
        .map(|value| short_text(value, 240))
}

fn extract_json_string(snippet: &str, keys: &[&str]) -> Option<String> {
    for key in keys {
        let needle = format!("\"{key}\"");
        if let Some(pos) = snippet.rfind(&needle) {
            let after = &snippet[pos + needle.len()..];
            let colon = after.find(':')?;
            let value = after[colon + 1..].trim_start();
            if let Some(stripped) = value.strip_prefix('"') {
                return parse_json_string_value(stripped);
            }
        }
    }
    None
}

fn parse_json_string_value(input: &str) -> Option<String> {
    let mut out = String::new();
    let mut escaped = false;
    for c in input.chars() {
        if escaped {
            out.push(match c {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '"' => '"',
                '\\' => '\\',
                '/' => '/',
                other => other,
            });
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            return Some(short_text(&out, 240));
        } else {
            out.push(c);
        }
    }
    None
}
