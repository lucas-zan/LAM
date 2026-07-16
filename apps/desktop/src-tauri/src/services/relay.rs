use super::account::{find_account, CodexAccount};
use super::error::{AppError, Result};
use super::gateway::launch_planner::{
    resolve_profile_route, CodexLaunchPlanner, LaunchEntry, LaunchPlanInput,
};
use super::provider_api_v2::{list_binding_views_service_v2, list_provider_views_service_v2};
use super::provider_capability::EffectiveCapability;
use super::provider_relay_compatibility::{
    analyze_relay_compatibility, classify_codex_relay_history, RelayCompatibilityDisposition,
    RelayCompatibilityReport, RelayTargetCapabilities,
};
use super::session::list_sessions;
use super::types::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
pub struct TerminalTarget {
    pub id: String,
    pub display_name: String,
    pub kind: String,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RelayResumeRequest {
    pub from_profile_id: String,
    pub to_profile_id: String,
    pub session_id: String,
    pub cwd: Option<String>,
    pub diverged_strategy: Option<String>,
    #[serde(default)]
    pub confirm_compatibility_loss: bool,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<RelayCompatibilityReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility_fingerprint: Option<String>,
}

pub fn build_resume_command(home_root: &Path, req: &ResumeCommandRequest) -> Result<ResumeCommand> {
    let account = find_account(home_root, &req.profile_id)?;
    let route_kind = resolve_profile_route(home_root, &req.profile_id)?;
    let command = CodexLaunchPlanner::new("lam".into())
        .plan(LaunchPlanInput {
            profile_id: req.profile_id.clone(),
            codex_home: account.codex_home.clone(),
            route_kind,
            entry: LaunchEntry::Resume {
                session_id: req.session_id.clone(),
            },
            cwd: req.cwd.as_ref().map(PathBuf::from),
        })?
        .shell_command;
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
    let source_bytes = fs::read(&source_path)?;
    let source_sessions_root = source_account.codex_home.join("sessions");
    let rel_path = source_path
        .strip_prefix(&source_sessions_root)
        .map_err(|_| AppError::new("PATH_ERROR", "session path outside source profile"))?;
    let target_path = target_account.codex_home.join("sessions").join(rel_path);
    let mut warnings = Vec::new();
    let (compatibility, compatibility_fingerprint) =
        analyze_provider_relay(home_root, req, &source_bytes)?;
    if let Some(report) = &compatibility {
        match report.disposition {
            RelayCompatibilityDisposition::Blocked => {
                return Err(AppError::new(
                    "RELAY_COMPATIBILITY_BLOCKED",
                    "Session history is not portable to the target Provider; no target files were written",
                ));
            }
            RelayCompatibilityDisposition::CompatibleWithLoss
                if !req.confirm_compatibility_loss =>
            {
                return Err(AppError::new(
                    "RELAY_COMPATIBILITY_CONFIRMATION_REQUIRED",
                    "Relay requires explicit confirmation for representation-only loss",
                ));
            }
            RelayCompatibilityDisposition::CompatibleWithLoss => warnings
                .push("Confirmed representation-only loss while moving session history.".into()),
            RelayCompatibilityDisposition::Compatible => {}
        }
    }
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
            home_root,
            &target_account,
            &req.session_id,
            req.cwd.clone().or(source_session.cwd.clone()),
            handoff,
        )?;
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
        compatibility,
        compatibility_fingerprint,
    })
}

fn analyze_provider_relay(
    home_root: &Path,
    req: &RelayResumeRequest,
    source_bytes: &[u8],
) -> Result<(Option<RelayCompatibilityReport>, Option<String>)> {
    let bindings = list_binding_views_service_v2(home_root)?;
    let source = bindings
        .iter()
        .find(|binding| binding.profile_id == req.from_profile_id);
    let target = bindings
        .iter()
        .find(|binding| binding.profile_id == req.to_profile_id);
    let (Some(source), Some(target)) = (source, target) else {
        return Ok((None, None));
    };
    if source.provider_id == target.provider_id && source.selected_model == target.selected_model {
        return Ok((None, None));
    }
    let target_provider = list_provider_views_service_v2(home_root)?
        .into_iter()
        .find(|provider| provider.id == target.provider_id)
        .ok_or_else(|| AppError::new("PROVIDER_NOT_FOUND", &target.provider_id))?;
    let report = analyze_relay_compatibility(
        &classify_codex_relay_history(source_bytes),
        &RelayTargetCapabilities {
            function_tools: target_provider.capabilities.function_tools.effective
                == EffectiveCapability::Supported,
            representation_metadata: true,
        },
    );
    let fingerprint = hex::encode(Sha256::digest(
        serde_json::to_vec(&(
            hex::encode(Sha256::digest(source_bytes)),
            &source.provider_id,
            &source.selected_model,
            &target.provider_id,
            &target.selected_model,
            target.revision,
            &report,
        ))
        .map_err(|_| {
            AppError::new(
                "RELAY_ANALYSIS_FAILED",
                "analysis could not be fingerprinted",
            )
        })?,
    ));
    Ok((Some(report), Some(fingerprint)))
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
    open_terminal_with_command(home_root, &command.command)
}

pub fn open_terminal_with_command(home_root: &Path, command: &str) -> Result<()> {
    let target_id = selected_terminal_target_id(home_root);
    open_terminal_with_target(&target_id, command)
}

fn open_terminal_with_target(target_id: &str, command: &str) -> Result<()> {
    match target_id {
        "ghostty" => open_ghostty_with_command(command),
        "cmux" => open_cmux_with_command(command),
        "codex_app" => Err(AppError::new(
            "CODEX_APP_HANDOFF_UNSUPPORTED",
            "Codex app is installed, but LAM does not have a stable Codex app handoff API yet. Select a terminal target or copy the command.",
        )),
        _ => open_terminal_app_with_command(command),
    }
}

fn open_terminal_app_with_command(command: &str) -> Result<()> {
    let script = terminal_applescript(command);
    let status = Command::new("/usr/bin/osascript")
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

fn open_ghostty_with_command(command: &str) -> Result<()> {
    let status = Command::new("/usr/bin/open")
        .args(["-a", "Ghostty", "--args", "-e", "/bin/zsh", "-lc", command])
        .status()
        .map_err(|err| AppError::new("TERMINAL_LAUNCH_FAILED", err.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::new(
            "TERMINAL_PERMISSION_DENIED",
            "Ghostty did not accept the resume command",
        ))
    }
}

fn open_cmux_with_command(command: &str) -> Result<()> {
    let cmux_bin =
        bundled_app_bin("cmux", &["cmux", "Cmux"]).unwrap_or_else(|| PathBuf::from("cmux"));
    let output = Command::new(cmux_bin)
        .args(cmux_command_args(command))
        .env("CMUX_QUIET", "1")
        .output()
        .map_err(|err| AppError::new("TERMINAL_LAUNCH_FAILED", err.to_string()))?;
    if output.status.success() {
        Ok(())
    } else {
        let code = output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let cli_error = cmux_launch_error(&stderr, &stdout, code);
        if cmux_should_try_applescript_fallback(&stderr, &stdout) {
            open_cmux_with_applescript_command(command, &cli_error)
        } else {
            Err(cli_error)
        }
    }
}

fn cmux_command_args(command: &str) -> Vec<String> {
    vec![
        "workspace".to_string(),
        "create".to_string(),
        "--name".to_string(),
        "LAM Handoff".to_string(),
        "--command".to_string(),
        command.to_string(),
        "--focus".to_string(),
        "true".to_string(),
    ]
}

fn cmux_launch_error(stderr: &str, stdout: &str, code: i32) -> AppError {
    let detail = [stderr.trim(), stdout.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let message = if detail.is_empty() {
        format!("cmux did not accept the resume command (exit code {code})")
    } else {
        format!("cmux did not accept the resume command (exit code {code}): {detail}")
    };
    AppError::new("TERMINAL_PERMISSION_DENIED", message)
}

fn cmux_should_try_applescript_fallback(stderr: &str, stdout: &str) -> bool {
    let detail = format!("{stderr} {stdout}").to_lowercase();
    detail.contains("access denied")
        || detail.contains("only processes started inside cmux")
        || detail.contains("broken pipe")
        || detail.contains("failed to write to socket")
}

fn open_cmux_with_applescript_command(command: &str, cli_error: &AppError) -> Result<()> {
    let script = cmux_applescript_for_command(command);
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|err| cmux_applescript_fallback_error(cli_error, &err.to_string()))?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = [
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
        Err(cmux_applescript_fallback_error(cli_error, &detail))
    }
}

fn cmux_applescript_for_command(command: &str) -> String {
    let escaped = command
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!(
        "tell application \"cmux\"\nactivate\nset targetTab to new tab\ndelay 0.2\nset targetTerminal to focused terminal of targetTab\ninput text (\"{escaped}\" & return) to targetTerminal\nend tell"
    )
}

fn cmux_applescript_fallback_error(cli_error: &AppError, applescript_detail: &str) -> AppError {
    let detail = applescript_detail.trim();
    let message = if detail.is_empty() {
        format!(
            "{} AppleScript fallback also failed without details.",
            cli_error.message
        )
    } else {
        format!(
            "{} AppleScript fallback also failed: {detail}",
            cli_error.message
        )
    };
    AppError::new("TERMINAL_PERMISSION_DENIED", message)
}

pub fn open_terminal_for_login(home_root: &Path, profile_id: &str) -> Result<()> {
    let command = build_login_command(home_root, profile_id)?;
    open_terminal_with_command(home_root, &command.command)
}

pub fn list_terminal_targets() -> Vec<TerminalTarget> {
    terminal_targets_from_probe_results(
        mac_app_exists_any(&["Ghostty"]),
        command_exists("ghostty"),
        mac_app_exists_any(&["cmux", "Cmux"]),
        command_exists("cmux"),
        mac_app_exists_any(&["Codex"]),
    )
}

fn terminal_targets_from_probe_results(
    ghostty_app: bool,
    ghostty_cmd: bool,
    cmux_app: bool,
    cmux_cmd: bool,
    codex_app: bool,
) -> Vec<TerminalTarget> {
    vec![
        TerminalTarget {
            id: "terminal".to_string(),
            display_name: "Terminal.app".to_string(),
            kind: "terminal".to_string(),
            installed: true,
        },
        TerminalTarget {
            id: "ghostty".to_string(),
            display_name: "Ghostty".to_string(),
            kind: "terminal".to_string(),
            installed: ghostty_app || ghostty_cmd,
        },
        TerminalTarget {
            id: "cmux".to_string(),
            display_name: "cmux".to_string(),
            kind: "terminal".to_string(),
            installed: cmux_app || cmux_cmd,
        },
        TerminalTarget {
            id: "codex_app".to_string(),
            display_name: "Codex app".to_string(),
            kind: "app".to_string(),
            installed: codex_app,
        },
    ]
}

fn mac_app_exists_any(names: &[&str]) -> bool {
    names.iter().any(|name| mac_app_exists(name))
}

fn mac_app_exists(name: &str) -> bool {
    mac_app_path(name).is_some()
}

fn mac_app_path(name: &str) -> Option<PathBuf> {
    let app = format!("{name}.app");
    if let Some(path) = ["/Applications", "/System/Applications"]
        .iter()
        .map(Path::new)
        .map(|root| root.join(&app))
        .find(|path| path.exists())
    {
        return Some(path);
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Applications").join(&app))
        .filter(|path| path.exists())
}

fn bundled_app_bin(command_name: &str, app_names: &[&str]) -> Option<PathBuf> {
    app_names
        .iter()
        .filter_map(|name| mac_app_path(name))
        .map(|app| app.join("Contents/Resources/bin").join(command_name))
        .find(|path| path.exists())
}

fn command_exists(name: &str) -> bool {
    Command::new("/usr/bin/which")
        .arg(name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn build_summarize_handoff_resume_command(
    home_root: &Path,
    target: &CodexAccount,
    session_id: &str,
    cwd: Option<String>,
    handoff_path: &Path,
) -> Result<ResumeCommand> {
    let prompt = format!(
        "A diverged branch handoff was written at {}. Read it, summarize the source branch into this target-account session context, preserve the target branch as the active timeline, then state that the handoff has been incorporated.",
        handoff_path.display()
    );
    let route_kind = resolve_profile_route(home_root, &target.id)?;
    let command = CodexLaunchPlanner::new("lam".into())
        .plan(LaunchPlanInput {
            profile_id: target.id.clone(),
            codex_home: target.codex_home.clone(),
            route_kind,
            entry: LaunchEntry::RelayHandoff {
                session_id: session_id.into(),
                prompt,
            },
            cwd: cwd.map(PathBuf::from),
        })?
        .shell_command;
    Ok(ResumeCommand {
        command,
        side_effects: vec![
            format!("Uses target CODEX_HOME {}", target.codex_home.display()),
            format!(
                "Summarizes handoff with target quota from {}",
                handoff_path.display()
            ),
            "Reopens codex resume after the summary turn completes.".into(),
        ],
    })
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn cmux_app_bundle_marks_target_installed() {
        let targets = terminal_targets_from_probe_results(false, false, true, false, false);
        let cmux = targets.iter().find(|target| target.id == "cmux").unwrap();

        assert!(cmux.installed);
    }

    #[test]
    fn cmux_launcher_uses_workspace_create_command() {
        let args = cmux_command_args("CODEX_HOME=/tmp/.codex codex resume --last --all");

        assert_eq!(
            args,
            vec![
                "workspace",
                "create",
                "--name",
                "LAM Handoff",
                "--command",
                "CODEX_HOME=/tmp/.codex codex resume --last --all",
                "--focus",
                "true"
            ]
        );
    }

    #[test]
    fn cmux_launcher_error_includes_stderr() {
        let err = cmux_launch_error("Broken pipe, errno 32\n", "", 1);

        assert!(err.message.contains("Broken pipe, errno 32"));
    }

    #[test]
    fn cmux_socket_access_denied_uses_applescript_fallback() {
        assert!(cmux_should_try_applescript_fallback(
            "ERROR: Access denied — only processes started inside cmux can connect",
            ""
        ));
        assert!(cmux_should_try_applescript_fallback(
            "Failed to write to socket (Broken pipe, errno 32)",
            ""
        ));
        assert!(!cmux_should_try_applescript_fallback(
            "unsupported argument --bogus",
            ""
        ));
    }

    #[test]
    fn cmux_applescript_pastes_command_into_new_tab() {
        let script = cmux_applescript_for_command("echo \"LAM\" && printf 'a\\\\b'");

        assert!(script.contains("tell application \"cmux\""));
        assert!(script.contains("set targetTab to new tab"));
        assert!(script.contains("set targetTerminal to focused terminal of targetTab"));
        assert!(script.contains(
            "input text (\"echo \\\"LAM\\\" && printf 'a\\\\\\\\b'\" & return) to targetTerminal"
        ));
    }

    #[test]
    fn cmux_combined_fallback_error_includes_both_failures() {
        let cli_error = cmux_launch_error("ERROR: Access denied", "", 1);
        let err =
            cmux_applescript_fallback_error(&cli_error, "Not authorized to send Apple events");

        assert!(err.message.contains("ERROR: Access denied"));
        assert!(err.message.contains("Not authorized to send Apple events"));
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
