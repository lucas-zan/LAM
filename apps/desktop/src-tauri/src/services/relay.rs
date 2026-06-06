use super::account::{find_account, CodexAccount};
use super::error::{AppError, Result};
use super::session::list_sessions;
use super::types::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResumeCommandRequest {
    pub profile_id: String,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResumeCommand {
    pub command: String,
    pub side_effects: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RelayResumeRequest {
    pub from_profile_id: String,
    pub to_profile_id: String,
    pub session_id: String,
    pub cwd: Option<String>,
    pub diverged_strategy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RelayResumeResult {
    pub action: String,
    pub from_profile_id: String,
    pub to_profile_id: String,
    pub session_id: String,
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub fork_path: Option<PathBuf>,
    pub handoff_path: Option<PathBuf>,
    pub resume: ResumeCommand,
    pub warnings: Vec<String>,
}

pub fn build_resume_command(home_root: &Path, req: &ResumeCommandRequest) -> Result<ResumeCommand> {
    let account = find_account(home_root, &req.profile_id)?;
    let command = if let Some(session_id) = &req.session_id {
        let cd = req
            .cwd
            .as_ref()
            .map(|cwd| format!("cd {} && ", shell_quote(cwd)))
            .unwrap_or_default();
        format!(
            "{}CODEX_HOME={} codex resume {}",
            cd,
            shell_quote(account.codex_home.to_string_lossy()),
            shell_quote(session_id)
        )
    } else {
        format!(
            "CODEX_HOME={} codex resume --last --all",
            shell_quote(account.codex_home.to_string_lossy())
        )
    };
    Ok(ResumeCommand {
        command,
        side_effects: vec![
            format!("Uses CODEX_HOME {}", account.codex_home.display()),
            "Runs codex resume; no files are copied by this command builder.".into(),
        ],
    })
}

pub fn relay_resume_session(
    home_root: &Path,
    req: &RelayResumeRequest,
) -> Result<RelayResumeResult> {
    if req.from_profile_id == req.to_profile_id {
        return Err(AppError::new(
            "INVALID_RELAY_TARGET",
            "Source and target profiles must be different",
        ));
    }
    let source_account = find_account(home_root, &req.from_profile_id)?;
    let target_account = find_account(home_root, &req.to_profile_id)?;
    let source_session = list_sessions(home_root, &source_account.id)?
        .into_iter()
        .find(|session| session.id == req.session_id)
        .ok_or_else(|| AppError::new("SESSION_NOT_FOUND", "Session not found in source profile"))?;
    let source_path = source_session.path.clone();
    let source_sessions_root = source_account.codex_home.join("sessions");
    let rel_path = source_path
        .strip_prefix(&source_sessions_root)
        .map_err(|_| AppError::new("PATH_ERROR", "session path outside source profile"))?;
    let target_path = target_account.codex_home.join("sessions").join(rel_path);
    let mut warnings = Vec::new();
    if source_account.provider_id != target_account.provider_id
        || source_account.model != target_account.model
    {
        warnings.push(
            "Target provider or model differs from source; runtime behavior may change.".into(),
        );
    }

    let action: String;
    let mut backup_path = None;
    let mut fork_path = None;
    let mut handoff_path = None;
    if !target_path.exists() {
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&source_path, &target_path)?;
        action = "copied".into();
    } else {
        let source_bytes = fs::read(&source_path)?;
        let target_bytes = fs::read(&target_path)?;
        if target_bytes == source_bytes || target_bytes.starts_with(&source_bytes) {
            action = "already_current".into();
        } else {
            let backup = backup_file_path(&target_path);
            fs::copy(&target_path, &backup)?;
            backup_path = Some(backup);
            if source_bytes.starts_with(&target_bytes) {
                fs::copy(&source_path, &target_path)?;
                action = "extended".into();
            } else {
                let strategy = req.diverged_strategy.as_deref().unwrap_or("stop_and_ask");
                match strategy {
                    "prefer_source" => {
                        fs::copy(&source_path, &target_path)?;
                        action = "prefer_source".into();
                    }
                    "prefer_target" => {
                        let fork = fork_session_path(&target_path, &source_account.id)?;
                        fs::copy(&source_path, &fork)?;
                        fork_path = Some(fork);
                        action = "prefer_target".into();
                    }
                    "timeline_merge_to_fork" => {
                        let fork = fork_session_path(&target_path, "timeline")?;
                        write_file_private(
                            &fork,
                            &timeline_merge_jsonl(&source_bytes, &target_bytes),
                        )?;
                        fork_path = Some(fork);
                        action = "timeline_merge_to_fork".into();
                        warnings.push(
                            "Timeline merge was written as a fork and did not overwrite the target session.".into(),
                        );
                    }
                    "summarize_fork_with_target_account" => {
                        let handoff = write_diverged_handoff(
                            &target_account,
                            &source_account,
                            &req.session_id,
                            &source_bytes,
                            &target_bytes,
                        )?;
                        handoff_path = Some(handoff);
                        action = "summarize_fork_with_target_account".into();
                        warnings.push(
                            "Diverged branches were preserved; handoff material was written under the target account for summary with target quota.".into(),
                        );
                    }
                    "stop_and_ask" | "" => {
                        return Err(AppError::new(
                            "SESSION_DIVERGED",
                            "Source and target session histories diverged; target was backed up and left unchanged.",
                        ));
                    }
                    other => {
                        return Err(AppError::new(
                            "DIVERGED_STRATEGY_INVALID",
                            format!("unsupported diverged strategy: {other}"),
                        ));
                    }
                }
            }
        }
    }

    let mut resume = build_resume_command(
        home_root,
        &ResumeCommandRequest {
            profile_id: req.to_profile_id.clone(),
            session_id: Some(req.session_id.clone()),
            cwd: req.cwd.clone().or(source_session.cwd.clone()),
        },
    )?;
    if let Some(handoff) = &handoff_path {
        resume = build_summarize_handoff_resume_command(
            &target_account,
            &req.session_id,
            req.cwd.clone().or(source_session.cwd.clone()),
            handoff,
        );
    }

    Ok(RelayResumeResult {
        action,
        from_profile_id: req.from_profile_id.clone(),
        to_profile_id: req.to_profile_id.clone(),
        session_id: req.session_id.clone(),
        source_path,
        target_path,
        backup_path,
        fork_path,
        handoff_path,
        resume,
        warnings,
    })
}

pub fn build_login_command(home_root: &Path, profile_id: &str) -> Result<ResumeCommand> {
    let account = find_account(home_root, profile_id)?;
    let command = format!(
        "CODEX_HOME={} codex login",
        shell_quote(account.codex_home.to_string_lossy())
    );
    Ok(ResumeCommand {
        command,
        side_effects: vec![
            format!("Uses CODEX_HOME {}", account.codex_home.display()),
            "Runs codex login; no auth.json is copied by Lam.".into(),
        ],
    })
}

pub fn terminal_applescript(command: &str) -> String {
    let escaped = command.replace('\\', "\\\\").replace('"', "\\\"");
    format!("tell application \"Terminal\"\nactivate\ndo script \"{escaped}\"\nend tell")
}

pub fn open_terminal_with_resume(home_root: &Path, req: &ResumeCommandRequest) -> Result<()> {
    let command = build_resume_command(home_root, req)?;
    open_terminal_with_command(&command.command)
}

pub fn open_terminal_with_command(command: &str) -> Result<()> {
    let script = terminal_applescript(command);
    let status = std::process::Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .status()
        .map_err(|err| AppError::new("TERMINAL_LAUNCH_FAILED", err.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::new(
            "TERMINAL_PERMISSION_DENIED",
            "Terminal.app did not accept the resume command",
        ))
    }
}

pub fn open_terminal_for_login(home_root: &Path, profile_id: &str) -> Result<()> {
    let command = build_login_command(home_root, profile_id)?;
    let script = terminal_applescript(&command.command);
    let status = std::process::Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .status()
        .map_err(|err| AppError::new("TERMINAL_LAUNCH_FAILED", err.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::new(
            "TERMINAL_PERMISSION_DENIED",
            "Terminal.app did not accept the login command",
        ))
    }
}

fn build_summarize_handoff_resume_command(
    target: &CodexAccount,
    session_id: &str,
    cwd: Option<String>,
    handoff_path: &Path,
) -> ResumeCommand {
    let cd = cwd
        .as_ref()
        .map(|cwd| format!("cd {} && ", shell_quote(cwd)))
        .unwrap_or_default();
    let codex_home = shell_quote(target.codex_home.to_string_lossy());
    let session = shell_quote(session_id);
    let prompt = shell_quote(format!(
        "A diverged branch handoff was written at {}. Read it, summarize the source branch into this target-account session context, preserve the target branch as the active timeline, then state that the handoff has been incorporated.",
        handoff_path.display()
    ));
    let command = format!(
        "{cd}CODEX_HOME={codex_home} codex exec resume {session} {prompt} && CODEX_HOME={codex_home} codex resume {session}"
    );
    ResumeCommand {
        command,
        side_effects: vec![
            format!("Uses target CODEX_HOME {}", target.codex_home.display()),
            format!(
                "Summarizes handoff with target quota from {}",
                handoff_path.display()
            ),
            "Reopens codex resume after the summary turn completes.".into(),
        ],
    }
}

fn backup_file_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "session".into());
    path.with_file_name(format!("{name}.backup.{}", timestamp_yyyymmdd_hhmmss()))
}

fn fork_session_path(path: &Path, label: &str) -> Result<PathBuf> {
    let safe_label = validate_profile_name(label)?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "session.jsonl".into());
    Ok(path.with_file_name(format!(
        "{name}.fork-{safe_label}.{}",
        timestamp_yyyymmdd_hhmmss()
    )))
}

fn jsonl_lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(|line| line.to_string())
        .collect()
}

fn common_prefix_len(source: &[String], target: &[String]) -> usize {
    source
        .iter()
        .zip(target.iter())
        .take_while(|(left, right)| left == right)
        .count()
}

fn extract_jsonl_timestamp(line: &str) -> Option<String> {
    serde_json::from_str::<Value>(line).ok().and_then(|value| {
        ["timestamp", "created_at", "createdAt"]
            .iter()
            .find_map(|key| {
                value
                    .get(key)
                    .and_then(|item| item.as_str())
                    .map(str::to_string)
            })
    })
}

fn timeline_merge_jsonl(source: &[u8], target: &[u8]) -> String {
    let source_lines = jsonl_lines(source);
    let target_lines = jsonl_lines(target);
    let prefix_len = common_prefix_len(&source_lines, &target_lines);
    let mut merged: Vec<(String, usize, String)> = Vec::new();
    for (idx, line) in source_lines.iter().take(prefix_len).enumerate() {
        merged.push((
            extract_jsonl_timestamp(line).unwrap_or_default(),
            idx,
            line.clone(),
        ));
    }
    for (idx, line) in source_lines.iter().skip(prefix_len).enumerate() {
        merged.push((
            extract_jsonl_timestamp(line).unwrap_or_default(),
            prefix_len + idx,
            line.clone(),
        ));
    }
    let offset = source_lines.len();
    for (idx, line) in target_lines.iter().skip(prefix_len).enumerate() {
        merged.push((
            extract_jsonl_timestamp(line).unwrap_or_default(),
            offset + idx,
            line.clone(),
        ));
    }
    merged.sort_by(|a, b| match (a.0.is_empty(), b.0.is_empty()) {
        (false, false) => a.0.cmp(&b.0).then(a.1.cmp(&b.1)),
        _ => a.1.cmp(&b.1),
    });
    let mut body = merged
        .into_iter()
        .map(|(_, _, line)| line)
        .collect::<Vec<_>>()
        .join("\n");
    if !body.is_empty() {
        body.push('\n');
    }
    body
}

fn branch_excerpt(lines: &[String]) -> String {
    let max_lines = 80;
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
}

fn write_diverged_handoff(
    target: &CodexAccount,
    source: &CodexAccount,
    session_id: &str,
    source_bytes: &[u8],
    target_bytes: &[u8],
) -> Result<PathBuf> {
    let source_lines = jsonl_lines(source_bytes);
    let target_lines = jsonl_lines(target_bytes);
    let prefix_len = common_prefix_len(&source_lines, &target_lines);
    let source_branch = &source_lines[prefix_len..];
    let target_branch = &target_lines[prefix_len..];
    let path = target.codex_home.join(".lam-handoffs").join(format!(
        "session-{session_id}.{}.md",
        timestamp_yyyymmdd_hhmmss()
    ));
    let body = format!(
        "# Diverged Session Handoff\n\nTarget account {target_id} should summarize this handoff using target quota before continuing.\n\nSession: {session_id}\nSource account: {source_id}\nTarget account: {target_id}\nCommon prefix lines: {prefix_len}\nSource branch lines: {source_count}\nTarget branch lines: {target_count}\n\n## Source branch excerpt\n\n```jsonl\n{source_excerpt}\n```\n\n## Target branch excerpt\n\n```jsonl\n{target_excerpt}\n```\n",
        target_id = target.id,
        source_id = source.id,
        source_count = source_branch.len(),
        target_count = target_branch.len(),
        source_excerpt = branch_excerpt(source_branch),
        target_excerpt = branch_excerpt(target_branch),
    );
    write_file_private(&path, &body)?;
    Ok(path)
}
