use super::error::{AppError, Result};
use super::types::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexAccount {
    pub id: String,
    pub display_name: String,
    pub codex_home: PathBuf,
    pub wrapper_path: Option<PathBuf>,
    pub has_auth: bool,
    pub has_config: bool,
    pub has_history: bool,
    pub session_count: usize,
    pub latest_session_modified_at: Option<u64>,
    pub managed: bool,
    pub is_relay: bool,
    pub relay_source: Option<String>,
    pub relay_identity: Option<String>,
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub auth_mode: Option<String>,
    #[serde(default)]
    pub is_active_auth: bool,
    #[serde(default)]
    pub has_personal_access_token: bool,
    pub renewal_date: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateAccountRequest {
    pub name: String,
    pub copy_config_from: Option<String>,
    pub overwrite_wrapper: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateRelayRequest {
    pub runtime_profile_id: String,
    pub source_profile_id: String,
    pub name: Option<String>,
    pub provider_policy: String,
    pub overwrite_wrapper: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddPatAccountRequest {
    pub account_id: String,
    pub auth_json: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub personal_access_token: Option<String>,
    #[serde(default)]
    pub token_expiration: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddSessionProfileAccountRequest {
    pub account_id: String,
    pub session_json: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub overwrite_wrapper: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddPatAccountResult {
    pub account_id: String,
    pub email: String,
    pub expired: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CpaExport {
    pub file_name: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationPlan {
    pub operations: Vec<String>,
    pub warnings: Vec<String>,
    pub blocked: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateResult {
    pub profile_id: String,
    pub home_path: PathBuf,
    pub wrapper_path: PathBuf,
    pub operations: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RenameAccountRequest {
    pub from_profile_id: String,
    pub to_name: String,
    pub overwrite_wrapper: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RenameAccountResult {
    pub profile_id: String,
    pub previous_profile_id: String,
    pub home_path: PathBuf,
    pub previous_home_path: PathBuf,
    pub wrapper_path: PathBuf,
    pub previous_wrapper_path: PathBuf,
    pub operations: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAccountRequest {
    pub profile_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAccountResult {
    pub profile_id: String,
    pub removed_home_path: PathBuf,
    pub removed_wrapper_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccountNoteUpdate {
    pub profile_id: String,
    pub renewal_date: Option<String>,
    pub note: Option<String>,
}

/// User-uploaded credentials from external account management
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UploadedCredentials {
    pub access_token: String,
    pub account_id: String,
    pub disabled: bool,
    pub email: String,
    pub expired: String, // ISO 8601 format
    #[serde(default)]
    pub headers: Option<serde_json::Map<String, serde_json::Value>>,
    pub id_token: Option<String>,
    pub last_refresh: String,
    pub refresh_token: Option<String>,
    #[serde(rename = "type")]
    pub credential_type: String,
    pub websockets: bool,
    #[serde(default)]
    pub raw_auth_json: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Lam-tracked PAT metadata (stored in ~/.config/agent-workspace/auth-metadata/)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthMetadata {
    pub profile_id: String,
    pub auth_type: String, // "personal_token" | "oauth" | "api_key" | "uploaded"
    pub token_expiration: Option<String>, // ISO 8601
    pub last_checked: String, // ISO 8601
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PatUsageTimeline {
    events: Vec<PatUsageTimelineEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatUsageTimelineEvent {
    account_id: String,
    account_label: String,
    workspace_id: String,
    started_at: String,
    ended_at: Option<String>,
}

/// Token expiration status for UI display
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TokenExpirationStatus {
    pub profile_id: String,
    pub is_expired: bool,
    pub days_until_expiration: Option<i64>,
    pub expiration_date: Option<String>,
    pub warning_level: String, // "ok" | "warning" | "critical" | "expired"
}

pub fn list_accounts(home_root: &Path) -> Result<Vec<CodexAccount>> {
    let mut accounts = Vec::new();
    if !home_root.exists() {
        return Ok(accounts);
    }
    let notes = read_account_notes(home_root)?;

    for entry in fs::read_dir(home_root)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !is_codex_home_name(&name) {
            continue;
        }
        let home = entry.path();
        if !has_codex_signal(&home) {
            continue;
        }
        let id = account_id_from_dir_name(&name);
        let sessions = session_files(&home.join("sessions"))?;
        let latest = sessions.iter().filter_map(|p| modified_secs(p).ok()).max();
        let managed = home.join(NEW_MARKER).exists() || home.join(OLD_MARKER).exists();
        let (is_relay, relay_identity, relay_source) = relay_parts(&id);
        let config = parse_codex_config(&home.join("config.toml"))?;
        let note = notes.accounts.get(&id);
        let auth_mode = detect_auth_mode(home_root, &id, &home, &config);
        accounts.push(CodexAccount {
            id: id.clone(),
            display_name: if id == "main" {
                "main".into()
            } else {
                format!("codex-{id}")
            },
            codex_home: home,
            wrapper_path: if id == "main" {
                None
            } else {
                Some(wrapper_path(home_root, &id))
            },
            has_auth: entry.path().join("auth.json").exists(),
            has_config: entry.path().join("config.toml").exists(),
            has_history: entry.path().join("history.jsonl").exists(),
            session_count: sessions.len(),
            latest_session_modified_at: latest,
            managed,
            is_relay,
            relay_source,
            relay_identity,
            provider_id: config.provider_id,
            model: config.model,
            auth_mode,
            is_active_auth: false,
            has_personal_access_token: auth_has_personal_access_token(
                &entry.path().join("auth.json"),
            ),
            renewal_date: note.and_then(|metadata| metadata.renewal_date.clone()),
            note: note.and_then(|metadata| metadata.note.clone()),
        });
    }

    mark_active_auth_account(home_root, &mut accounts);
    accounts.sort_by(|a, b| a.id.cmp(&b.id));
    write_accounts_cache(home_root, &accounts)?;
    Ok(accounts)
}

fn mark_active_auth_account(home_root: &Path, accounts: &mut [CodexAccount]) {
    let active_auth_path = home_root.join(".codex/auth.json");
    let matches = if let Some(active_suffix) = auth_account_id_suffix(&active_auth_path) {
        accounts
            .iter()
            .enumerate()
            .filter(|(_, account)| account.id != "main")
            .filter(|(_, account)| {
                auth_account_id_suffix(&account.codex_home.join("auth.json")).as_deref()
                    == Some(active_suffix.as_str())
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>()
    } else {
        let Some(active_auth) = fs::read(&active_auth_path).ok() else {
            return;
        };
        accounts
            .iter()
            .enumerate()
            .filter(|(_, account)| account.id != "main")
            .filter(|(_, account)| {
                fs::read(account.codex_home.join("auth.json")).is_ok_and(|auth| auth == active_auth)
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>()
    };

    if let [index] = matches.as_slice() {
        accounts[*index].is_active_auth = true;
    }
}

fn auth_account_id_suffix(auth_path: &Path) -> Option<String> {
    let auth: serde_json::Value = serde_json::from_slice(&fs::read(auth_path).ok()?).ok()?;
    let account_id = auth.get("tokens")?.get("account_id")?.as_str()?;
    let suffix = account_id.chars().rev().take(4).collect::<Vec<_>>();
    if suffix.len() != 4 {
        return None;
    }
    Some(suffix.into_iter().rev().collect())
}

pub fn list_cached_accounts(home_root: &Path) -> Result<Vec<CodexAccount>> {
    Ok(read_accounts_cache(home_root)?.unwrap_or_default())
}

pub fn update_account_note(home_root: &Path, req: &AccountNoteUpdate) -> Result<CodexAccount> {
    let profile_id = validate_existing_profile_id(home_root, &req.profile_id)?;
    let renewal_date = normalize_renewal_date(req.renewal_date.as_deref())?;
    let note = normalize_note(req.note.as_deref())?;
    let mut notes = read_account_notes(home_root)?;

    if renewal_date.is_none() && note.is_none() {
        notes.accounts.remove(&profile_id);
    } else {
        notes
            .accounts
            .insert(profile_id.clone(), AccountNote { renewal_date, note });
    }
    write_account_notes(home_root, &notes)?;

    list_accounts(home_root)?
        .into_iter()
        .find(|account| account.id == profile_id)
        .ok_or_else(|| AppError::new("ACCOUNT_NOT_FOUND", profile_id))
}

pub fn create_account_plan(home_root: &Path, req: &CreateAccountRequest) -> Result<OperationPlan> {
    let name = validate_profile_name(&req.name)?;
    let home = codex_home_path(home_root, &name);
    let wrapper = wrapper_path(home_root, &name);
    let mut warnings = Vec::new();
    if home.exists() {
        warnings.push(format!(
            "Target CODEX_HOME already exists: {}",
            home.display()
        ));
    }
    if wrapper.exists() && !req.overwrite_wrapper {
        return Err(AppError::new(
            "WRAPPER_ALREADY_EXISTS",
            wrapper.display().to_string(),
        ));
    }
    Ok(OperationPlan {
        operations: vec![
            format!("create_dir {}", home.display()),
            format!("write_file {}", home.join(NEW_MARKER).display()),
            format!("write_file {}", wrapper.display()),
        ],
        warnings,
        blocked: vec!["auth.json".into()],
    })
}

pub fn execute_create_account(
    home_root: &Path,
    req: &CreateAccountRequest,
) -> Result<CreateResult> {
    execute_create_account_with_route(home_root, req, super::provider_binding::RouteKind::Direct)
}

pub(crate) fn execute_create_account_with_route(
    home_root: &Path,
    req: &CreateAccountRequest,
    route_kind: super::provider_binding::RouteKind,
) -> Result<CreateResult> {
    let plan = create_account_plan(home_root, req)?;
    let name = validate_profile_name(&req.name)?;
    let home = codex_home_path(home_root, &name);
    let wrapper = wrapper_path(home_root, &name);
    let wrapper_contents = wrapper_script_for_route(&name, route_kind)?;
    fs::create_dir_all(&home)?;
    set_dir_private(&home)?;
    for sub in [
        "sessions", "cache", "log", "tmp", "rules", "skills", "memories",
    ] {
        fs::create_dir_all(home.join(sub))?;
    }
    if let Some(from) = &req.copy_config_from {
        let src_account = find_account(home_root, from)?;
        let src = src_account.codex_home.join("config.toml");
        let dst = home.join("config.toml");
        if src.exists() && !dst.exists() {
            fs::copy(src, dst)?;
        }
    }
    write_file_private(
        &home.join(NEW_MARKER),
        &managed_account_json(&name, None, None, None, &home, &wrapper),
    )?;
    fs::create_dir_all(
        wrapper
            .parent()
            .ok_or_else(|| AppError::new("WRAPPER_PATH_INVALID", "missing wrapper parent"))?,
    )?;
    write_executable(&wrapper, &wrapper_contents)?;
    Ok(CreateResult {
        profile_id: name,
        home_path: home,
        wrapper_path: wrapper,
        operations: plan.operations,
        warnings: plan.warnings,
    })
}

pub fn repair_managed_wrappers(home_root: &Path) -> Result<Vec<PathBuf>> {
    let accounts = list_accounts(home_root)?;
    let managed = accounts
        .into_iter()
        .filter(|account| account.managed && account.id != "main")
        .collect::<Vec<_>>();
    if managed.is_empty() {
        return Ok(Vec::new());
    }
    let mut repaired = Vec::new();
    for account in managed {
        let path = account
            .wrapper_path
            .unwrap_or_else(|| wrapper_path(home_root, &account.id));
        let route_kind =
            super::gateway::launch_planner::resolve_profile_route(home_root, &account.id)?;
        let expected = wrapper_script_for_route(&account.id, route_kind)?;
        if fs::read_to_string(&path).ok().as_deref() == Some(&expected) {
            continue;
        }
        replace_wrapper(&path, &expected)?;
        repaired.push(path);
    }
    Ok(repaired)
}

fn replace_wrapper(path: &Path, contents: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::new("WRAPPER_PATH_INVALID", "missing wrapper parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".lam-wrapper-{}.tmp", uuid::Uuid::new_v4()));
    write_executable(&temporary, contents)?;
    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error.into())
        }
    }
}

/// Compensate a just-created account before it has ever been handed to Codex.
/// The marker and empty session tree are mandatory so this cannot become a
/// general-purpose destructive profile deletion path.
pub fn rollback_created_account(home_root: &Path, profile_id: &str) -> Result<()> {
    let profile_id = validate_profile_id(profile_id)?;
    let home = codex_home_path(home_root, &profile_id);
    let marker = home.join(NEW_MARKER);
    if !marker.is_file() {
        return Err(AppError::new(
            "ACCOUNT_ROLLBACK_OWNERSHIP_CONFLICT",
            "new-account ownership marker is unavailable",
        ));
    }
    let sessions = home.join("sessions");
    if sessions.exists() && fs::read_dir(&sessions)?.next().transpose()?.is_some() {
        return Err(AppError::new(
            "ACCOUNT_ROLLBACK_HAS_SESSIONS",
            "account has session data and cannot be compensated",
        ));
    }
    super::provider_api_v2::ensure_profile_has_no_provider_binding_service_v2(
        home_root,
        &profile_id,
    )?;
    remove_profile_home(&home)?;
    let wrapper = wrapper_path(home_root, &profile_id);
    if wrapper.exists() {
        fs::remove_file(wrapper)?;
    }
    Ok(())
}

pub fn rename_account_plan(home_root: &Path, req: &RenameAccountRequest) -> Result<OperationPlan> {
    let from = find_account(home_root, &req.from_profile_id)?;
    if from.id == "main" {
        return Err(AppError::new(
            "MAIN_ACCOUNT_RENAME_BLOCKED",
            "The main ~/.codex profile cannot be renamed",
        ));
    }
    super::provider_api_v2::ensure_profile_has_no_provider_binding_service_v2(home_root, &from.id)?;
    let to_name = validate_profile_name(&req.to_name)?;
    if to_name == from.id {
        return Err(AppError::new(
            "ACCOUNT_RENAME_NOOP",
            "Target account name is the same as the current name",
        ));
    }

    let target_home = codex_home_path(home_root, &to_name);
    let target_wrapper = wrapper_path(home_root, &to_name);
    let source_wrapper = from
        .wrapper_path
        .clone()
        .unwrap_or_else(|| wrapper_path(home_root, &from.id));

    if target_home.exists() {
        return Err(AppError::new(
            "TARGET_ACCOUNT_ALREADY_EXISTS",
            target_home.display().to_string(),
        ));
    }
    if target_wrapper.exists() && !req.overwrite_wrapper {
        return Err(AppError::new(
            "WRAPPER_ALREADY_EXISTS",
            target_wrapper.display().to_string(),
        ));
    }

    let mut warnings = Vec::new();
    if target_wrapper.exists() && req.overwrite_wrapper {
        warnings.push(format!(
            "Target wrapper exists and will be overwritten: {}",
            target_wrapper.display()
        ));
    }
    if !from.managed {
        warnings.push(
            "Source profile is not managed by Lam; only directory and wrapper are renamed.".into(),
        );
    }

    Ok(OperationPlan {
        operations: vec![
            format!(
                "rename_dir {} -> {}",
                from.codex_home.display(),
                target_home.display()
            ),
            format!("write_file {}", target_home.join(NEW_MARKER).display()),
            format!("write_file {}", target_wrapper.display()),
            format!("remove_file_if_exists {}", source_wrapper.display()),
        ],
        warnings,
        blocked: vec!["auth.json".into()],
    })
}

pub fn execute_rename_account(
    home_root: &Path,
    req: &RenameAccountRequest,
) -> Result<RenameAccountResult> {
    let plan = rename_account_plan(home_root, req)?;
    let from = find_account(home_root, &req.from_profile_id)?;
    let to_name = validate_profile_name(&req.to_name)?;
    let target_home = codex_home_path(home_root, &to_name);
    let target_wrapper = wrapper_path(home_root, &to_name);
    let wrapper_contents = direct_wrapper_script(&to_name);
    let source_wrapper = from
        .wrapper_path
        .clone()
        .unwrap_or_else(|| wrapper_path(home_root, &from.id));

    fs::rename(&from.codex_home, &target_home).map_err(|err| {
        AppError::new(
            "ACCOUNT_RENAME_FAILED",
            format!(
                "Failed to rename {} to {}: {err}",
                from.codex_home.display(),
                target_home.display()
            ),
        )
    })?;
    set_dir_private(&target_home)?;
    fs::create_dir_all(
        target_wrapper
            .parent()
            .ok_or_else(|| AppError::new("WRAPPER_PATH_INVALID", "missing wrapper parent"))?,
    )?;
    write_executable(&target_wrapper, &wrapper_contents)?;
    if source_wrapper.exists() && source_wrapper != target_wrapper {
        fs::remove_file(&source_wrapper)?;
    }
    write_file_private(
        &target_home.join(NEW_MARKER),
        &managed_account_json(&to_name, None, None, None, &target_home, &target_wrapper),
    )?;
    let old_marker = target_home.join(OLD_MARKER);
    if old_marker.exists() {
        fs::remove_file(old_marker)?;
    }
    let accounts = list_accounts(home_root)?;
    write_accounts_cache(home_root, &accounts)?;

    Ok(RenameAccountResult {
        profile_id: to_name,
        previous_profile_id: from.id,
        home_path: target_home,
        previous_home_path: from.codex_home,
        wrapper_path: target_wrapper,
        previous_wrapper_path: source_wrapper,
        operations: plan.operations,
        warnings: plan.warnings,
    })
}

pub fn delete_account(home_root: &Path, req: &DeleteAccountRequest) -> Result<DeleteAccountResult> {
    let profile_id = validate_profile_id(&req.profile_id)?;
    if profile_id == "main" {
        return Err(AppError::new(
            "MAIN_ACCOUNT_DELETE_BLOCKED",
            "The main ~/.codex profile cannot be deleted",
        ));
    }
    super::provider_api_v2::ensure_profile_has_no_provider_binding_service_v2(
        home_root,
        &profile_id,
    )?;

    let account = find_account(home_root, &profile_id)?;
    let removed_home_path = account.codex_home.clone();
    let removed_wrapper_path = account
        .wrapper_path
        .clone()
        .or_else(|| Some(wrapper_path(home_root, &profile_id)));

    terminate_profile_processes(&removed_home_path, removed_wrapper_path.as_deref())?;
    remove_profile_home(&removed_home_path)?;

    if let Some(wrapper) = &removed_wrapper_path {
        if wrapper.exists() {
            fs::remove_file(wrapper).map_err(|err| {
                AppError::new(
                    "WRAPPER_DELETE_FAILED",
                    format!("Failed to delete {}: {err}", wrapper.display()),
                )
            })?;
        }
    }
    verify_deleted(&removed_home_path, "ACCOUNT_DELETE_INCOMPLETE")?;
    if let Some(wrapper) = &removed_wrapper_path {
        verify_deleted(wrapper, "WRAPPER_DELETE_INCOMPLETE")?;
    }

    remove_account_note(home_root, &profile_id)?;
    remove_auth_metadata(home_root, &profile_id)?;
    let accounts = list_accounts(home_root)?;
    write_accounts_cache(home_root, &accounts)?;

    Ok(DeleteAccountResult {
        profile_id,
        removed_home_path,
        removed_wrapper_path,
    })
}

fn remove_profile_home(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    fs::remove_dir_all(path).map_err(|err| {
        AppError::new(
            "ACCOUNT_DELETE_FAILED",
            format!("Failed to delete {}: {err}", path.display()),
        )
    })
}

fn verify_deleted(path: &Path, code: &str) -> Result<()> {
    if path.exists() {
        return Err(AppError::new(
            code,
            format!("{} still exists after delete", path.display()),
        ));
    }
    Ok(())
}

fn terminate_profile_processes(codex_home: &Path, wrapper: Option<&Path>) -> Result<()> {
    #[cfg(unix)]
    {
        let process_ids = profile_process_ids(codex_home, wrapper)?;
        if process_ids.is_empty() {
            return Ok(());
        }
        signal_processes(&process_ids, "TERM");
        wait_for_process_exit(&process_ids, std::time::Duration::from_millis(1500));
        let remaining = process_ids
            .into_iter()
            .filter(|pid| process_is_alive(*pid))
            .collect::<Vec<_>>();
        if !remaining.is_empty() {
            signal_processes(&remaining, "KILL");
            wait_for_process_exit(&remaining, std::time::Duration::from_millis(1500));
        }
    }
    #[cfg(not(unix))]
    {
        let _ = codex_home;
        let _ = wrapper;
    }
    Ok(())
}

#[cfg(unix)]
fn profile_process_ids(codex_home: &Path, wrapper: Option<&Path>) -> Result<Vec<u32>> {
    let output = Command::new("ps")
        .args(["-ww", "-eo", "pid,args"])
        .output()
        .map_err(|err| AppError::new("PROCESS_SCAN_FAILED", err.to_string()))?;
    if !output.status.success() {
        return Err(AppError::new("PROCESS_SCAN_FAILED", "ps command failed"));
    }
    let body = String::from_utf8_lossy(&output.stdout);
    let current_pid = std::process::id();
    Ok(profile_process_ids_from_ps(
        &body,
        current_pid,
        codex_home,
        wrapper,
    ))
}

#[cfg(unix)]
fn profile_process_ids_from_ps(
    ps_output: &str,
    current_pid: u32,
    codex_home: &Path,
    wrapper: Option<&Path>,
) -> Vec<u32> {
    ps_output
        .lines()
        .filter_map(|line| profile_process_id_from_ps_line(line, current_pid, codex_home, wrapper))
        .collect()
}

#[cfg(unix)]
fn profile_process_id_from_ps_line(
    line: &str,
    current_pid: u32,
    codex_home: &Path,
    wrapper: Option<&Path>,
) -> Option<u32> {
    let trimmed = line.trim_start();
    let (pid, command) = trimmed.split_once(char::is_whitespace)?;
    let pid = pid.parse::<u32>().ok()?;
    if pid == current_pid {
        return None;
    }
    if process_command_matches_path(command, codex_home)
        || wrapper.is_some_and(|path| process_command_matches_path(command, path))
    {
        Some(pid)
    } else {
        None
    }
}

#[cfg(unix)]
fn process_command_matches_path(command: &str, path: &Path) -> bool {
    let needle = path.to_string_lossy();
    command.contains(needle.as_ref())
        || command
            .to_ascii_lowercase()
            .contains(&needle.to_ascii_lowercase())
}

#[cfg(unix)]
fn signal_processes(process_ids: &[u32], signal: &str) {
    for pid in process_ids {
        let _ = Command::new("kill")
            .args([format!("-{signal}"), pid.to_string()])
            .status();
    }
}

#[cfg(unix)]
fn wait_for_process_exit(process_ids: &[u32], timeout: std::time::Duration) {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if process_ids.iter().all(|pid| !process_is_alive(*pid)) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .is_ok_and(|status| status.success())
}

pub fn create_relay_plan(home_root: &Path, req: &CreateRelayRequest) -> Result<OperationPlan> {
    find_account(home_root, &req.runtime_profile_id)?;
    find_account(home_root, &req.source_profile_id)?;
    let name = relay_name(req)?;
    create_account_plan(
        home_root,
        &CreateAccountRequest {
            name,
            copy_config_from: None,
            overwrite_wrapper: req.overwrite_wrapper,
        },
    )
}

pub fn execute_create_relay(home_root: &Path, req: &CreateRelayRequest) -> Result<CreateResult> {
    let plan = create_relay_plan(home_root, req)?;
    let name = relay_name(req)?;
    let home = codex_home_path(home_root, &name);
    let wrapper = wrapper_path(home_root, &name);
    let wrapper_contents = direct_wrapper_script(&name);
    fs::create_dir_all(&home)?;
    set_dir_private(&home)?;
    fs::create_dir_all(home.join("sessions"))?;
    write_file_private(
        &home.join(NEW_MARKER),
        &managed_account_json(
            &name,
            Some(&req.runtime_profile_id),
            Some(&req.source_profile_id),
            Some(&req.provider_policy),
            &home,
            &wrapper,
        ),
    )?;
    fs::create_dir_all(
        wrapper
            .parent()
            .ok_or_else(|| AppError::new("WRAPPER_PATH_INVALID", "missing wrapper parent"))?,
    )?;
    write_executable(&wrapper, &wrapper_contents)?;
    Ok(CreateResult {
        profile_id: name,
        home_path: home,
        wrapper_path: wrapper,
        operations: plan.operations,
        warnings: plan.warnings,
    })
}

pub(crate) fn find_account(home_root: &Path, profile_id: &str) -> Result<CodexAccount> {
    list_accounts(home_root)?
        .into_iter()
        .find(|a| a.id == profile_id)
        .ok_or_else(|| AppError::new("ACCOUNT_NOT_FOUND", profile_id))
}

pub(crate) fn codex_home_path(home_root: &Path, name: &str) -> PathBuf {
    if name == "main" {
        home_root.join(".codex")
    } else {
        home_root.join(format!(".codex-{name}"))
    }
}

pub(crate) fn has_codex_signal(home: &Path) -> bool {
    [
        "auth.json",
        "config.toml",
        "history.jsonl",
        "sessions",
        "logs_2.sqlite",
        NEW_MARKER,
        OLD_MARKER,
    ]
    .iter()
    .any(|name| home.join(name).exists())
}

pub(crate) fn quota_account(home_root: &Path, profile_id: &str) -> Result<CodexAccount> {
    let codex_home = codex_home_path(home_root, profile_id);
    if !codex_home.exists() || !has_codex_signal(&codex_home) {
        return Err(AppError::new("ACCOUNT_NOT_FOUND", profile_id));
    }
    let has_personal_access_token = auth_has_personal_access_token(&codex_home.join("auth.json"));
    Ok(CodexAccount {
        id: profile_id.to_string(),
        display_name: if profile_id == "main" {
            "main".into()
        } else {
            format!("codex-{profile_id}")
        },
        codex_home,
        wrapper_path: None,
        has_auth: false,
        has_config: false,
        has_history: false,
        session_count: 0,
        latest_session_modified_at: None,
        managed: false,
        is_relay: false,
        relay_source: None,
        relay_identity: None,
        provider_id: None,
        model: None,
        auth_mode: None,
        is_active_auth: false,
        has_personal_access_token,
        renewal_date: None,
        note: None,
    })
}

fn is_codex_home_name(name: &str) -> bool {
    name == ".codex" || name.starts_with(".codex-")
}

fn account_id_from_dir_name(name: &str) -> String {
    if name == ".codex" {
        "main".into()
    } else {
        name.trim_start_matches(".codex-").into()
    }
}

fn wrapper_path(home_root: &Path, name: &str) -> PathBuf {
    home_root.join("bin").join(format!("codex-{name}"))
}

fn relay_name(req: &CreateRelayRequest) -> Result<String> {
    if let Some(name) = &req.name {
        validate_profile_name(name)
    } else {
        validate_profile_name(&format!(
            "{}-relay-{}",
            req.runtime_profile_id, req.source_profile_id
        ))
    }
}

fn relay_parts(id: &str) -> (bool, Option<String>, Option<String>) {
    if let Some((runtime, source)) = id.split_once("-relay-") {
        (true, Some(runtime.to_string()), Some(source.to_string()))
    } else {
        (false, None, None)
    }
}

fn direct_wrapper_script(name: &str) -> String {
    format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
export CODEX_HOME="$HOME/.codex-{name}"
CODEX_BIN="${{CODEX_BIN:-}}"
if [ -z "$CODEX_BIN" ]; then
  if command -v codex >/dev/null 2>&1; then
    CODEX_BIN="$(command -v codex)"
  else
    echo "codex command not found. Add codex to PATH or set CODEX_BIN=/path/to/codex." >&2
    exit 127
  fi
fi
exec "$CODEX_BIN" "$@"
"#
    )
}

fn wrapper_script_for_route(
    name: &str,
    route_kind: super::provider_binding::RouteKind,
) -> Result<String> {
    match route_kind {
        super::provider_binding::RouteKind::Direct => Ok(direct_wrapper_script(name)),
        super::provider_binding::RouteKind::Gateway => {
            let launcher = super::provider_runtime::resolve_launcher_executable()?;
            super::gateway::launch_planner::CodexLaunchPlanner::new(
                launcher.to_string_lossy().into_owned(),
            )
            .gateway_wrapper_script(name)
        }
    }
}

fn managed_account_json(
    name: &str,
    runtime: Option<&str>,
    source: Option<&str>,
    provider_policy: Option<&str>,
    home: &Path,
    wrapper: &Path,
) -> String {
    let kind = if runtime.is_some() {
        "relay"
    } else {
        "primary"
    };
    format!(
        "{{\n  \"managedBy\": \"LAM\",\n  \"accountName\": \"{}\",\n  \"kind\": \"{}\",\n  \"runtimeProfileId\": {},\n  \"sourceProfileId\": {},\n  \"providerPolicy\": {},\n  \"codexHome\": \"{}\",\n  \"wrapperPath\": \"{}\",\n  \"createdAt\": \"{}\"\n}}\n",
        json_escape(name),
        kind,
        json_option(runtime),
        json_option(source),
        json_option(provider_policy),
        json_escape(&home.to_string_lossy()),
        json_escape(&wrapper.to_string_lossy()),
        timestamp()
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountsCacheFile {
    home_root: String,
    fetched_at: u64,
    accounts: Vec<CodexAccount>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct AccountNotesFile {
    accounts: BTreeMap<String, AccountNote>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct AccountNote {
    renewal_date: Option<String>,
    note: Option<String>,
}

fn accounts_cache_path(home_root: &Path) -> PathBuf {
    config_root(home_root).join("accounts-cache.json")
}

fn account_notes_path(home_root: &Path) -> PathBuf {
    config_root(home_root).join("account-notes.json")
}

fn write_accounts_cache(home_root: &Path, accounts: &[CodexAccount]) -> Result<()> {
    let payload = AccountsCacheFile {
        home_root: home_root.to_string_lossy().to_string(),
        fetched_at: system_secs(SystemTime::now()),
        accounts: accounts.to_vec(),
    };
    let body = serde_json::to_string_pretty(&payload)
        .map_err(|err| AppError::new("ACCOUNTS_CACHE_INVALID", err.to_string()))?;
    write_file_private(&accounts_cache_path(home_root), &format!("{body}\n"))
}

fn read_accounts_cache(home_root: &Path) -> Result<Option<Vec<CodexAccount>>> {
    let path = accounts_cache_path(home_root);
    if !path.exists() {
        return Ok(None);
    }
    let body = fs::read_to_string(path)?;
    let payload: AccountsCacheFile = serde_json::from_str(&body)
        .map_err(|err| AppError::new("ACCOUNTS_CACHE_INVALID", err.to_string()))?;
    if payload.home_root != home_root.to_string_lossy() {
        return Ok(None);
    }
    Ok(Some(payload.accounts))
}

fn read_account_notes(home_root: &Path) -> Result<AccountNotesFile> {
    let path = account_notes_path(home_root);
    if !path.exists() {
        return Ok(AccountNotesFile::default());
    }
    let body = fs::read_to_string(path)?;
    serde_json::from_str(&body)
        .map_err(|err| AppError::new("ACCOUNT_NOTES_INVALID", err.to_string()))
}

fn write_account_notes(home_root: &Path, notes: &AccountNotesFile) -> Result<()> {
    let body = serde_json::to_string_pretty(notes)
        .map_err(|err| AppError::new("ACCOUNT_NOTES_INVALID", err.to_string()))?;
    write_file_private(&account_notes_path(home_root), &format!("{body}\n"))
}

fn remove_account_note(home_root: &Path, profile_id: &str) -> Result<()> {
    let mut notes = read_account_notes(home_root)?;
    if notes.accounts.remove(profile_id).is_some() {
        write_account_notes(home_root, &notes)?;
    }
    Ok(())
}

fn validate_existing_profile_id(home_root: &Path, profile_id: &str) -> Result<String> {
    let trimmed = profile_id.trim();
    if trimmed.is_empty() {
        return Err(AppError::new("ACCOUNT_NOT_FOUND", profile_id));
    }
    let home = codex_home_path(home_root, trimmed);
    if !home.exists() || !has_codex_signal(&home) {
        return Err(AppError::new("ACCOUNT_NOT_FOUND", trimmed));
    }
    Ok(trimmed.to_string())
}

fn normalize_renewal_date(value: Option<&str>) -> Result<Option<String>> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
        .map_err(|_| AppError::new("ACCOUNT_RENEWAL_DATE_INVALID", trimmed))?;
    Ok(Some(trimmed.to_string()))
}

fn normalize_note(value: Option<&str>) -> Result<Option<String>> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > 500 {
        return Err(AppError::new("ACCOUNT_NOTE_TOO_LONG", "max 500 characters"));
    }
    Ok(Some(trimmed.to_string()))
}

/// Records PAT metadata for a profile (Lam-only, doesn't touch Codex files)
pub fn record_pat_metadata(
    home_root: &Path,
    profile_id: &str,
    expiration: Option<String>,
) -> Result<()> {
    record_auth_metadata(home_root, profile_id, "personal_token", expiration)
}

fn record_auth_metadata(
    home_root: &Path,
    profile_id: &str,
    auth_type: &str,
    expiration: Option<String>,
) -> Result<()> {
    use crate::services::types::{auth_metadata_dir, auth_metadata_path};
    let profile_id = validate_profile_id(profile_id)?;

    let metadata = AuthMetadata {
        profile_id: profile_id.clone(),
        auth_type: auth_type.to_string(),
        token_expiration: expiration,
        last_checked: chrono::Utc::now().to_rfc3339(),
    };

    let dir = auth_metadata_dir(home_root);
    std::fs::create_dir_all(&dir).map_err(|e| {
        AppError::new(
            "CREATE_DIR_FAILED",
            format!("Failed to create auth-metadata dir: {}", e),
        )
    })?;

    let path = auth_metadata_path(home_root, &profile_id);
    let content = serde_json::to_string_pretty(&metadata)
        .map_err(|e| AppError::new("SERIALIZE_FAILED", format!("Serialize failed: {}", e)))?;

    std::fs::write(&path, content)
        .map_err(|e| AppError::new("WRITE_METADATA_FAILED", format!("Write failed: {}", e)))?;

    Ok(())
}

fn remove_auth_metadata(home_root: &Path, profile_id: &str) -> Result<()> {
    use crate::services::types::auth_metadata_path;
    let path = auth_metadata_path(home_root, profile_id);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| {
            AppError::new(
                "DELETE_METADATA_FAILED",
                format!("Failed to delete {}: {e}", path.display()),
            )
        })?;
    }
    Ok(())
}

/// Reads PAT metadata for a profile
pub fn read_pat_metadata(home_root: &Path, profile_id: &str) -> Result<Option<AuthMetadata>> {
    use crate::services::types::auth_metadata_path;
    let profile_id = validate_profile_id(profile_id)?;

    let path = auth_metadata_path(home_root, &profile_id);
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| AppError::new("READ_METADATA_FAILED", format!("Failed to read: {}", e)))?;

    let metadata: AuthMetadata = serde_json::from_str(&content)
        .map_err(|e| AppError::new("INVALID_METADATA", format!("Invalid metadata: {}", e)))?;

    Ok(Some(metadata))
}

/// Stores uploaded auth.json in the target profile and records metadata
pub fn process_uploaded_credentials(
    home_root: &Path,
    profile_id: &str,
    creds: &UploadedCredentials,
) -> Result<()> {
    let profile_id = validate_existing_profile_id(home_root, profile_id)?;
    let auth_content = build_pat_auth_json(creds, None)?;
    let auth_path = codex_home_path(home_root, &profile_id).join("auth.json");
    write_file_private(&auth_path, &auth_content)?;

    let expiration = parse_auth_expiration(creds)?;
    let auth_type = infer_uploaded_auth_type(creds, None);
    record_auth_metadata(home_root, &profile_id, &auth_type, expiration)?;

    Ok(())
}

/// Checks token expiration from metadata
pub fn check_token_expiration(home_root: &Path, profile_id: &str) -> Result<TokenExpirationStatus> {
    let metadata = read_pat_metadata(home_root, profile_id)?;

    let (expiration_date, is_expired, days_until, warning_level) = match metadata {
        Some(meta) if meta.token_expiration.is_some() => {
            let exp_str = meta.token_expiration.unwrap();
            let expiry = chrono::DateTime::parse_from_rfc3339(&exp_str)
                .map_err(|e| AppError::new("INVALID_EXPIRATION_FORMAT", e.to_string()))?;

            let now = chrono::Utc::now();
            let days = (expiry.timestamp() - now.timestamp()) / 86400;

            let level = if days < 0 {
                "expired"
            } else if days <= 7 {
                "critical"
            } else if days <= 30 {
                "warning"
            } else {
                "ok"
            };

            (Some(exp_str), days < 0, Some(days), level.to_string())
        }
        _ => (None, false, None, "ok".to_string()),
    };

    Ok(TokenExpirationStatus {
        profile_id: profile_id.to_string(),
        is_expired,
        days_until_expiration: days_until,
        expiration_date,
        warning_level,
    })
}

/// Adds a new PAT account by creating a full .codex-{id} directory
pub fn add_pat_account(
    home_root: &Path,
    req: &AddPatAccountRequest,
) -> Result<AddPatAccountResult> {
    let account_id = &req.account_id;

    // 1. Validate account_id
    if account_id.trim().is_empty() {
        return Err(AppError::new(
            "INVALID_ACCOUNT_ID",
            "account_id cannot be empty",
        ));
    }

    let validated_id = validate_profile_name(account_id)?;

    // 2. Check if account already exists
    let codex_dir = codex_home_path(home_root, &validated_id);
    if codex_dir.exists() {
        return Err(AppError::new(
            "ACCOUNT_EXISTS",
            format!("Account '{}' already exists", validated_id),
        ));
    }

    if req.auth_json.is_empty() {
        return Err(AppError::new(
            "INVALID_AUTH_JSON",
            "auth.json cannot be empty",
        ));
    }

    // 3. Create directory structure
    std::fs::create_dir_all(&codex_dir).map_err(|e| {
        AppError::new(
            "CREATE_DIR_FAILED",
            format!("Failed to create codex directory: {}", e),
        )
    })?;

    let sessions_dir = codex_dir.join("sessions");
    std::fs::create_dir_all(&sessions_dir).map_err(|e| {
        AppError::new(
            "CREATE_DIR_FAILED",
            format!("Failed to create sessions directory: {}", e),
        )
    })?;

    let personal_access_token = req
        .personal_access_token
        .as_deref()
        .map(str::trim)
        .filter(|token| !token.is_empty());
    let expiration = parse_optional_expiration(req.token_expiration.as_deref())?;

    // 4. Save the uploaded auth.json without rebuilding its schema
    let auth_path = codex_dir.join("auth.json");
    let uploaded_auth_content = serde_json::to_string_pretty(&req.auth_json).map_err(|e| {
        AppError::new(
            "SERIALIZE_AUTH_FAILED",
            format!("Failed to serialize auth.json: {e}"),
        )
    })?;
    if let Some(token) = personal_access_token {
        write_file_private(&codex_dir.join("auth-f.json"), &uploaded_auth_content)?;
        let auth_content = serde_json::to_string_pretty(&serde_json::json!({
            "OPENAI_API_KEY": null,
            "personal_access_token": token,
        }))
        .map_err(|e| {
            AppError::new(
                "SERIALIZE_AUTH_FAILED",
                format!("Failed to serialize auth.json: {e}"),
            )
        })?;
        write_file_private(&auth_path, &auth_content)?;
    } else {
        write_file_private(&auth_path, &uploaded_auth_content)?;
    }

    // 5. Create minimal config.toml
    let config_path = codex_dir.join("config.toml");
    let config_content = r#"# PAT account configuration
# This file is managed by LAM
"#;
    write_file_private(&config_path, config_content)?;

    // 6. Mark directory as managed
    let marker_path = codex_dir.join(NEW_MARKER);
    write_file_private(&marker_path, "{}")?;

    record_auth_metadata(home_root, &validated_id, "uploaded", expiration.clone())?;

    Ok(AddPatAccountResult {
        account_id: validated_id,
        email: req
            .auth_json
            .get("email")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
        expired: expiration.unwrap_or_default(),
    })
}

/// Adds a normal isolated Codex profile from pasted ChatGPT session JSON.
pub fn add_session_profile_account(
    home_root: &Path,
    req: &AddSessionProfileAccountRequest,
) -> Result<CreateResult> {
    let account_id = validate_profile_name(&req.account_id)?;
    let codex_dir = codex_home_path(home_root, &account_id);
    if codex_dir.exists() {
        return Err(AppError::new(
            "ACCOUNT_EXISTS",
            format!("Account '{}' already exists", account_id),
        ));
    }

    let wrapper = wrapper_path(home_root, &account_id);
    let wrapper_contents = direct_wrapper_script(&account_id);
    if wrapper.exists() && !req.overwrite_wrapper {
        return Err(AppError::new(
            "WRAPPER_ALREADY_EXISTS",
            wrapper.display().to_string(),
        ));
    }

    let auth_content = build_session_profile_auth_json(&req.session_json)?;
    fs::create_dir_all(&codex_dir)?;
    set_dir_private(&codex_dir)?;
    for sub in [
        "sessions", "cache", "log", "tmp", "rules", "skills", "memories",
    ] {
        fs::create_dir_all(codex_dir.join(sub))?;
    }
    write_file_private(&codex_dir.join("auth.json"), &auth_content)?;
    write_file_private(
        &codex_dir.join("config.toml"),
        "# Session-imported Codex profile\n# This file is managed by LAM\n",
    )?;
    write_file_private(
        &codex_dir.join(NEW_MARKER),
        &managed_account_json(&account_id, None, None, None, &codex_dir, &wrapper),
    )?;
    fs::create_dir_all(
        wrapper
            .parent()
            .ok_or_else(|| AppError::new("WRAPPER_PATH_INVALID", "missing wrapper parent"))?,
    )?;
    write_executable(&wrapper, &wrapper_contents)?;

    Ok(CreateResult {
        profile_id: account_id,
        home_path: codex_dir,
        wrapper_path: wrapper,
        operations: vec![
            "create profile directory".to_string(),
            "write converted auth.json".to_string(),
            "write wrapper".to_string(),
        ],
        warnings: Vec::new(),
    })
}

fn build_session_profile_auth_json(
    session_json: &serde_json::Map<String, serde_json::Value>,
) -> Result<String> {
    if session_json.is_empty() {
        return Err(AppError::new(
            "INVALID_SESSION_JSON",
            "session JSON cannot be empty",
        ));
    }

    let session_value = serde_json::Value::Object(session_json.clone());
    let access_token = auth_string(session_json, "access_token");
    let access_payload = access_token
        .as_deref()
        .and_then(decode_jwt_payload_value)
        .unwrap_or(serde_json::Value::Null);
    let id_payload = auth_string(session_json, "id_token")
        .as_deref()
        .and_then(decode_jwt_payload_value)
        .unwrap_or(serde_json::Value::Null);
    let profile = json_object_at(&access_payload, &["https://api.openai.com/profile"]);
    let access_auth = json_object_at(&access_payload, &["https://api.openai.com/auth"]);
    let id_auth = json_object_at(&id_payload, &["https://api.openai.com/auth"]);

    let account_id = first_json_string(
        &session_value,
        &[
            &["account_id"],
            &["accountId"],
            &["chatgpt_account_id"],
            &["account", "id"],
        ],
    )
    .or_else(|| first_json_string_from_map(id_auth, &["chatgpt_account_id", "account_id"]))
    .or_else(|| first_json_string_from_map(access_auth, &["chatgpt_account_id", "account_id"]));
    let user_id = first_json_string(
        &session_value,
        &[&["chatgpt_user_id"], &["chatgptUserId"], &["user", "id"]],
    )
    .or_else(|| first_json_string_from_map(id_auth, &["chatgpt_user_id", "user_id"]))
    .or_else(|| first_json_string_from_map(access_auth, &["chatgpt_user_id", "user_id"]));
    let organization_id =
        first_json_string(&session_value, &[&["organization_id"], &["organizationId"]])
            .or_else(|| first_json_string_from_map(id_auth, &["organization_id"]))
            .or_else(|| first_json_string_from_map(access_auth, &["organization_id"]));
    let project_id = first_json_string(
        &session_value,
        &[
            &["project_id"],
            &["projectId"],
            &["workspace_id"],
            &["workspaceId"],
        ],
    )
    .or_else(|| first_json_string_from_map(id_auth, &["project_id"]))
    .or_else(|| first_json_string_from_map(access_auth, &["project_id"]));
    let email = first_json_string(
        &session_value,
        &[&["email"], &["account_claims_email"], &["user", "email"]],
    )
    .or_else(|| first_json_string_from_map(profile, &["email"]))
    .or_else(|| json_string_at(&id_payload, &["email"]));
    let plan_type = auth_string(session_json, "plan_type")
        .or_else(|| auth_string(session_json, "chatgpt_plan_type"))
        .or_else(|| {
            first_json_string(
                &session_value,
                &[&["account", "planType"], &["account", "plan_type"]],
            )
        })
        .or_else(|| first_json_string_from_map(access_auth, &["chatgpt_plan_type"]))
        .unwrap_or_else(|| "free".to_string());

    let mut tokens = serde_json::Map::new();
    if let Some(value) = access_token.clone() {
        tokens.insert("access_token".to_string(), serde_json::Value::String(value));
    }
    let id_token = auth_string(session_json, "id_token").or_else(|| {
        access_token.as_ref().map(|_| {
            build_compat_id_token(CompatIdTokenInput {
                account_id: account_id.as_deref(),
                user_id: user_id.as_deref(),
                organization_id: organization_id.as_deref(),
                project_id: project_id.as_deref(),
                email: email.as_deref(),
                plan_type: Some(plan_type.as_str()),
            })
        })
    });
    if let Some(value) = id_token {
        tokens.insert("id_token".to_string(), serde_json::Value::String(value));
    }
    if let Some(value) = auth_string(session_json, "refresh_token")
        .or_else(|| first_json_string(&session_value, &[&["session_token"], &["sessionToken"]]))
    {
        tokens.insert(
            "refresh_token".to_string(),
            serde_json::Value::String(value),
        );
    }
    if let Some(value) = account_id.clone() {
        tokens.insert("account_id".to_string(), serde_json::Value::String(value));
    }
    if !tokens
        .keys()
        .any(|key| matches!(key.as_str(), "access_token" | "id_token" | "refresh_token"))
    {
        return Err(AppError::new(
            "INVALID_SESSION_JSON",
            "session JSON must include accessToken, idToken, or refreshToken",
        ));
    }

    let mut auth = serde_json::Map::new();
    auth.insert(
        "auth_mode".to_string(),
        serde_json::Value::String("chatgpt".to_string()),
    );
    auth.insert("OPENAI_API_KEY".to_string(), serde_json::Value::Null);
    auth.insert("tokens".to_string(), serde_json::Value::Object(tokens));
    auth.insert(
        "last_refresh".to_string(),
        serde_json::Value::String(
            auth_string(session_json, "last_refresh")
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        ),
    );
    auth.insert(
        "type".to_string(),
        serde_json::Value::String(
            auth_string(session_json, "type").unwrap_or_else(|| "codex".to_string()),
        ),
    );
    auth.insert("websockets".to_string(), serde_json::Value::Bool(true));

    if let Some(value) = email {
        auth.insert("email".to_string(), serde_json::Value::String(value));
    }
    if let Some(value) = auth_string(session_json, "expired") {
        auth.insert("expired".to_string(), serde_json::Value::String(value));
    }
    auth.insert(
        "plan_type".to_string(),
        serde_json::Value::String(plan_type.clone()),
    );
    auth.insert(
        "chatgpt_plan_type".to_string(),
        serde_json::Value::String(plan_type),
    );
    if let Some(value) = first_json_string(&session_value, &[&["session_token"], &["sessionToken"]])
    {
        auth.insert(
            "session_token".to_string(),
            serde_json::Value::String(value),
        );
    }

    serde_json::to_string_pretty(&serde_json::Value::Object(auth)).map_err(|e| {
        AppError::new(
            "SERIALIZE_AUTH_FAILED",
            format!("Failed to serialize auth.json: {e}"),
        )
    })
}

struct CompatIdTokenInput<'a> {
    account_id: Option<&'a str>,
    user_id: Option<&'a str>,
    organization_id: Option<&'a str>,
    project_id: Option<&'a str>,
    email: Option<&'a str>,
    plan_type: Option<&'a str>,
}

fn build_compat_id_token(input: CompatIdTokenInput<'_>) -> String {
    let now = chrono::Utc::now().timestamp();
    let account_id = input.account_id.unwrap_or_default();
    let user_id = input.user_id.unwrap_or(account_id);
    let payload = serde_json::json!({
        "aud": ["app_EMoamEEZ73f0CkXaXp7hrann"],
        "email": input.email.unwrap_or_default(),
        "exp": now + 3600,
        "iat": now,
        "iss": "https://auth.openai.com",
        "https://api.openai.com/auth": {
            "account_id": account_id,
            "chatgpt_account_id": account_id,
            "chatgpt_user_id": user_id,
            "user_id": user_id,
            "organization_id": input.organization_id.unwrap_or_default(),
            "project_id": input.project_id.unwrap_or_default(),
            "chatgpt_plan_type": input.plan_type.unwrap_or("free"),
        },
        "sub": if user_id.is_empty() { "local-compat" } else { user_id },
    });
    format!(
        "{}.{}.{}",
        base64url_encode(br#"{"alg":"RS256","typ":"JWT","kid":"compat"}"#),
        base64url_encode(
            serde_json::to_string(&payload)
                .unwrap_or_default()
                .as_bytes()
        ),
        base64url_encode(b"local_compat_signature")
    )
}

fn json_string_at(value: &serde_json::Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn json_object_at<'a>(
    value: &'a serde_json::Value,
    path: &[&str],
) -> Option<&'a serde_json::Map<String, serde_json::Value>> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_object()
}

fn first_json_string(value: &serde_json::Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| json_string_at(value, path))
}

fn first_json_string_from_map(
    map: Option<&serde_json::Map<String, serde_json::Value>>,
    keys: &[&str],
) -> Option<String> {
    let map = map?;
    keys.iter().find_map(|key| {
        map.get(*key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn decode_jwt_payload_value(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64url_decode(payload)?;
    serde_json::from_slice(&bytes).ok()
}

fn base64url_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    let mut index = 0;
    while index < bytes.len() {
        let b0 = bytes[index];
        let b1 = bytes.get(index + 1).copied();
        let b2 = bytes.get(index + 2).copied();
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1.unwrap_or(0) >> 4)) as usize] as char);
        if let Some(b1) = b1 {
            out.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2.unwrap_or(0) >> 6)) as usize] as char);
        }
        if let Some(b2) = b2 {
            out.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        }
        index += 3;
    }
    out
}

fn base64url_decode(value: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buffer = 0_u32;
    let mut bits = 0_u8;
    for byte in value.bytes() {
        let chunk = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            b'=' => continue,
            _ => return None,
        } as u32;
        buffer = (buffer << 6) | chunk;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Some(bytes)
}

/// Builds auth.json content from uploaded credentials and optional PAT
fn build_pat_auth_json(creds: &UploadedCredentials, token: Option<&str>) -> Result<String> {
    let mut auth_json = match &creds.raw_auth_json {
        Some(raw) => serde_json::Value::Object(raw.clone()),
        None => serde_json::json!({
            "access_token": creds.access_token,
            "account_id": creds.account_id,
            "email": creds.email,
            "expired": creds.expired,
            "id_token": creds.id_token,
            "last_refresh": creds.last_refresh,
            "refresh_token": creds.refresh_token,
            "type": creds.credential_type,
            "websockets": creds.websockets,
            "disabled": creds.disabled,
        }),
    };

    if let Some(headers) = &creds.headers {
        auth_json["headers"] = serde_json::to_value(headers).unwrap();
    }

    if let Some(pat) = token {
        auth_json["personal_access_token"] = serde_json::Value::String(pat.to_string());
    }

    serde_json::to_string_pretty(&auth_json).map_err(|e| {
        AppError::new(
            "SERIALIZE_FAILED",
            format!("Failed to serialize auth.json: {}", e),
        )
    })
}

fn parse_auth_expiration(creds: &UploadedCredentials) -> Result<Option<String>> {
    if creds.expired.trim().is_empty() {
        return Ok(None);
    }
    chrono::DateTime::parse_from_rfc3339(&creds.expired).map_err(|_| {
        AppError::new(
            "INVALID_EXPIRATION",
            "expired field must be valid ISO 8601 date",
        )
    })?;
    Ok(Some(creds.expired.clone()))
}

fn parse_optional_expiration(expiration: Option<&str>) -> Result<Option<String>> {
    let Some(expiration) = expiration.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    chrono::DateTime::parse_from_rfc3339(expiration).map_err(|_| {
        AppError::new(
            "INVALID_EXPIRATION",
            "token expiration must be valid ISO 8601 date",
        )
    })?;
    Ok(Some(expiration.to_string()))
}

fn infer_uploaded_auth_type(creds: &UploadedCredentials, token: Option<&str>) -> String {
    if token.is_some() {
        return "personal_token".to_string();
    }
    if let Some(raw) = &creds.raw_auth_json {
        return infer_auth_type(raw);
    }
    if !creds.access_token.is_empty() || creds.id_token.is_some() || creds.refresh_token.is_some() {
        return "oauth".to_string();
    }
    "personal_token".to_string()
}

fn infer_auth_type(auth_json: &serde_json::Map<String, serde_json::Value>) -> String {
    if auth_json.contains_key("personal_access_token") {
        return "personal_token".to_string();
    }
    if auth_json.contains_key("tokens")
        || auth_json.contains_key("access_token")
        || auth_json.contains_key("id_token")
        || auth_json.contains_key("refresh_token")
    {
        return "oauth".to_string();
    }
    if auth_json
        .get("OPENAI_API_KEY")
        .is_some_and(|value| !value.is_null())
    {
        return "api_key".to_string();
    }
    "personal_token".to_string()
}

/// Switches to an account based on the configured auth mode
/// - OAuth mode: switches the entire directory (symlink or copy)
/// - PAT mode: copies only auth.json
pub fn switch_to_pat_account(home_root: &Path, account_id: &str) -> Result<()> {
    let account_id = validate_profile_id(account_id)?;
    if account_id == "main" {
        return Err(AppError::new(
            "MAIN_AUTH_SLOT",
            "The main profile is the active auth slot and cannot be switched",
        ));
    }

    // 1. Verify account exists
    let codex_dir = codex_home_path(home_root, &account_id);
    if !codex_dir.exists() {
        return Err(AppError::new(
            "ACCOUNT_NOT_FOUND",
            format!("Account '{}' not found", account_id),
        ));
    }

    let source_auth = codex_dir.join("auth.json");
    if !source_auth.exists() {
        return Err(AppError::new(
            "AUTH_NOT_FOUND",
            format!("auth.json not found for account '{}'", account_id),
        ));
    }

    let target_codex = home_root.join(".codex");

    std::fs::create_dir_all(&target_codex).map_err(|e| {
        AppError::new(
            "CREATE_DIR_FAILED",
            format!("Failed to create .codex dir: {}", e),
        )
    })?;

    let source_content = fs::read(&source_auth)
        .map_err(|e| AppError::new("READ_FAILED", format!("Failed to read auth.json: {e}")))?;
    let parsed: serde_json::Value = serde_json::from_slice(&source_content)
        .map_err(|e| AppError::new("INVALID_AUTH_JSON", format!("Invalid auth.json: {e}")))?;
    if !parsed.is_object() {
        return Err(AppError::new(
            "INVALID_AUTH_JSON",
            "auth.json must contain a JSON object",
        ));
    }

    let target_auth = target_codex.join("auth.json");
    let temp_auth = target_codex.join(format!(".auth.json.lam-{}.tmp", std::process::id()));
    let write_result = (|| -> Result<()> {
        let mut temp_file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_auth)?;
        temp_file.write_all(&source_content)?;
        temp_file.sync_all()?;
        set_file_private(&temp_auth)?;
        fs::rename(&temp_auth, &target_auth)?;
        set_file_private(&target_auth)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_auth);
    }
    write_result?;

    let active_content = fs::read(&target_auth)
        .map_err(|e| AppError::new("VERIFY_FAILED", format!("Failed to verify auth.json: {e}")))?;
    if active_content != source_content {
        return Err(AppError::new(
            "VERIFY_FAILED",
            "Active auth.json does not match the selected profile",
        ));
    }

    record_pat_usage_switch(home_root, &account_id)?;

    let source_auth_f = codex_dir.join("auth-f.json");
    let target_auth_f = target_codex.join("auth-f.json");
    if source_auth_f.exists() {
        let source_content_f = fs::read(&source_auth_f).map_err(|e| {
            AppError::new("READ_FAILED", format!("Failed to read auth-f.json: {e}"))
        })?;
        let parsed_f: serde_json::Value = serde_json::from_slice(&source_content_f)
            .map_err(|e| AppError::new("INVALID_AUTH_JSON", format!("Invalid auth-f.json: {e}")))?;
        if !parsed_f.is_object() {
            return Err(AppError::new(
                "INVALID_AUTH_JSON",
                "auth-f.json must contain a JSON object",
            ));
        }

        let temp_auth_f = target_codex.join(format!(".auth-f.json.lam-{}.tmp", std::process::id()));
        let write_result_f = (|| -> Result<()> {
            let mut temp_file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_auth_f)?;
            temp_file.write_all(&source_content_f)?;
            temp_file.sync_all()?;
            set_file_private(&temp_auth_f)?;
            fs::rename(&temp_auth_f, &target_auth_f)?;
            set_file_private(&target_auth_f)?;
            Ok(())
        })();
        if write_result_f.is_err() {
            let _ = fs::remove_file(&temp_auth_f);
        }
        write_result_f?;
    } else {
        if target_auth_f.exists() {
            fs::remove_file(&target_auth_f).map_err(|e| {
                AppError::new(
                    "REMOVE_FAILED",
                    format!("Failed to remove stale auth-f.json: {e}"),
                )
            })?;
        }
    }

    Ok(())
}

fn record_pat_usage_switch(home_root: &Path, account_id: &str) -> Result<()> {
    let path = config_root(home_root).join("pat-usage-timeline.json");
    let mut timeline = fs::read_to_string(&path)
        .ok()
        .and_then(|body| serde_json::from_str::<PatUsageTimeline>(&body).ok())
        .unwrap_or_default();
    let now = chrono::Utc::now().to_rfc3339();
    if let Some(open_event) = timeline
        .events
        .iter_mut()
        .rev()
        .find(|event| event.ended_at.is_none())
    {
        open_event.ended_at = Some(now.clone());
    }
    timeline.events.push(PatUsageTimelineEvent {
        account_id: account_id.to_string(),
        account_label: account_label_from_id(account_id),
        workspace_id: "workspace:main".to_string(),
        started_at: now,
        ended_at: None,
    });
    let body = serde_json::to_string_pretty(&timeline).map_err(|e| {
        AppError::new(
            "SERIALIZE_TIMELINE_FAILED",
            format!("Failed to serialize PAT usage timeline: {e}"),
        )
    })?;
    write_file_private(&path, &body)
}

fn account_label_from_id(id: &str) -> String {
    if id == "main" {
        "main".to_string()
    } else {
        format!("codex-{id}")
    }
}

pub fn update_pat_session_auth(
    home_root: &Path,
    profile_id: &str,
    auth_json: serde_json::Map<String, serde_json::Value>,
) -> Result<()> {
    if auth_json.is_empty() {
        return Err(AppError::new(
            "INVALID_AUTH_JSON",
            "session JSON cannot be empty",
        ));
    }
    let account = find_account(home_root, profile_id)?;
    let auth = read_auth_object(&account.codex_home.join("auth.json"), "auth.json")?;
    if auth_string(&auth, "personal_access_token").is_none() {
        return Err(AppError::new(
            "PAT_REQUIRED",
            "This account does not use personal_access_token",
        ));
    }
    let content = serde_json::to_string_pretty(&auth_json).map_err(|err| {
        AppError::new(
            "SERIALIZE_AUTH_FAILED",
            format!("Failed to serialize session JSON: {err}"),
        )
    })?;
    write_file_private(&account.codex_home.join("auth-f.json"), &content)
}

pub fn export_cpa_credentials(home_root: &Path, profile_id: &str) -> Result<CpaExport> {
    let account = find_account(home_root, profile_id)?;
    let auth_json = read_auth_object(&account.codex_home.join("auth.json"), "auth.json")?;
    let auth_f_path = account.codex_home.join("auth-f.json");
    let auth_f_json = if auth_f_path.exists() {
        Some(read_auth_object(&auth_f_path, "auth-f.json")?)
    } else {
        None
    };

    let token_source = auth_f_json.as_ref().unwrap_or(&auth_json);
    let mut out = serde_json::Map::new();
    for key in [
        "id_token",
        "access_token",
        "refresh_token",
        "account_id",
        "last_refresh",
    ] {
        out.insert(
            key.to_string(),
            serde_json::Value::String(auth_string(token_source, key).unwrap_or_default()),
        );
    }

    for (key, value) in [
        ("email", auth_string(token_source, "email")),
        ("expired", auth_string(token_source, "expired")),
        ("plan_type", auth_string(token_source, "plan_type")),
        (
            "chatgpt_plan_type",
            auth_string(token_source, "chatgpt_plan_type"),
        ),
    ] {
        if let Some(value) = value {
            out.insert(key.to_string(), serde_json::Value::String(value));
        }
    }
    out.entry("type".to_string())
        .or_insert_with(|| serde_json::Value::String("codex".to_string()));
    out.entry("websockets".to_string())
        .or_insert(serde_json::Value::Bool(true));

    if let Some(auth_f_json) = &auth_f_json {
        for key in ["email", "expired", "headers", "type", "websockets"] {
            if let Some(value) = auth_f_json.get(key) {
                out.insert(key.to_string(), value.clone());
            }
        }
    }

    if let Some(token) = auth_string(&auth_json, "personal_access_token") {
        let headers = out
            .entry("headers".to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if let Some(headers) = headers.as_object_mut() {
            headers.insert(
                "authorization".to_string(),
                serde_json::Value::String(format!("Bearer {token}")),
            );
        }
    }

    Ok(CpaExport {
        file_name: format!("{}-cpa.json", account.id),
        content: serde_json::Value::Object(out),
    })
}

fn read_auth_object(
    path: &Path,
    label: &str,
) -> Result<serde_json::Map<String, serde_json::Value>> {
    let content = fs::read_to_string(path).map_err(|err| {
        AppError::new("AUTH_READ_FAILED", format!("Failed to read {label}: {err}"))
    })?;
    match serde_json::from_str::<serde_json::Value>(&content)
        .map_err(|err| AppError::new("INVALID_AUTH_JSON", format!("Invalid {label}: {err}")))?
    {
        serde_json::Value::Object(obj) => Ok(obj),
        _ => Err(AppError::new(
            "INVALID_AUTH_JSON",
            format!("{label} must contain a JSON object"),
        )),
    }
}

fn auth_string(auth: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    let aliases = match key {
        "id_token" => &["id_token", "idToken"][..],
        "access_token" => &["access_token", "accessToken"][..],
        "refresh_token" => &["refresh_token", "refreshToken"][..],
        "account_id" => &["account_id", "accountId", "chatgpt_account_id"][..],
        "last_refresh" => &["last_refresh", "lastRefresh"][..],
        "expired" => &["expired", "expires"][..],
        "plan_type" => &["plan_type", "planType"][..],
        "chatgpt_plan_type" => &["chatgpt_plan_type", "chatgptPlanType"][..],
        _ => std::slice::from_ref(&key),
    };
    aliases
        .iter()
        .find_map(|alias| {
            auth.get(*alias)
                .or_else(|| auth.get("tokens").and_then(|tokens| tokens.get(*alias)))
                .or_else(|| auth.get("user").and_then(|user| user.get(*alias)))
        })
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn auth_has_personal_access_token(path: &Path) -> bool {
    read_auth_object(path, "auth.json")
        .ok()
        .and_then(|auth| auth_string(&auth, "personal_access_token"))
        .is_some()
}

/// Detects auth mode by checking both Lam metadata and Codex auth.json
fn detect_auth_mode(
    home_root: &Path,
    profile_id: &str,
    codex_home: &Path,
    config: &CodexConfigBinding,
) -> Option<String> {
    // Priority 1: Use Lam metadata for accounts managed by this app
    if let Ok(Some(metadata)) = read_pat_metadata(home_root, profile_id) {
        return Some(metadata.auth_type);
    }

    // Priority 2: Check Codex auth.json structure (read-only inspection)
    let auth_path = codex_home.join("auth.json");
    if auth_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&auth_path) {
            // Simple heuristic: check for personal_access_token field
            if content.contains("\"personal_access_token\"") {
                return Some("personal_token".to_string());
            }
            if content.contains("\"token\"") {
                return Some("oauth".to_string());
            }
            if content.contains("\"access_token\"")
                || content.contains("\"id_token\"")
                || content.contains("\"refresh_token\"")
            {
                return Some("oauth".to_string());
            }
            if content.contains("\"OPENAI_API_KEY\"") {
                return Some("api_key".to_string());
            }
        }
    }

    // Priority 3: Fall back to config.toml detection
    config.auth_mode.clone()
}

#[cfg(test)]
mod pat_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_record_and_read_metadata() {
        let temp = TempDir::new().unwrap();
        let home_root = temp.path();

        record_pat_metadata(
            home_root,
            "test-profile",
            Some("2030-12-31T10:00:00+08:00".to_string()),
        )
        .unwrap();

        let metadata = read_pat_metadata(home_root, "test-profile")
            .unwrap()
            .unwrap();
        assert_eq!(metadata.profile_id, "test-profile");
        assert_eq!(metadata.auth_type, "personal_token");
        assert_eq!(
            metadata.token_expiration,
            Some("2030-12-31T10:00:00+08:00".to_string())
        );
    }
    #[test]
    fn test_process_valid_credentials() {
        let temp = TempDir::new().unwrap();
        let creds = UploadedCredentials {
            access_token: "at-test".to_string(),
            account_id: "id".to_string(),
            disabled: false,
            email: "test@example.com".to_string(),
            expired: "2030-12-31T10:00:00+08:00".to_string(),
            headers: None,
            id_token: None,
            last_refresh: "2026-06-23T22:19:32+08:00".to_string(),
            refresh_token: None,
            credential_type: "codex".to_string(),
            websockets: true,
            raw_auth_json: None,
        };

        std::fs::create_dir_all(temp.path().join(".codex-test/sessions")).unwrap();
        process_uploaded_credentials(temp.path(), "test", &creds).unwrap();

        let metadata = read_pat_metadata(temp.path(), "test").unwrap().unwrap();
        assert_eq!(metadata.auth_type, "oauth");
        assert!(temp.path().join(".codex-test/auth.json").exists());
    }

    #[test]
    fn test_process_invalid_expiration() {
        let temp = TempDir::new().unwrap();
        let creds = UploadedCredentials {
            access_token: "at-test".to_string(),
            account_id: "id".to_string(),
            disabled: false,
            email: "test@example.com".to_string(),
            expired: "not-a-date".to_string(),
            headers: None,
            id_token: None,
            last_refresh: "2026-06-23T22:19:32+08:00".to_string(),
            refresh_token: None,
            credential_type: "codex".to_string(),
            websockets: true,
            raw_auth_json: None,
        };
        assert!(process_uploaded_credentials(temp.path(), "test", &creds).is_err());
    }
    #[test]
    fn test_expiration_not_expired() {
        let temp = TempDir::new().unwrap();

        record_pat_metadata(
            temp.path(),
            "test",
            Some("2030-12-31T10:00:00+08:00".to_string()),
        )
        .unwrap();

        let status = check_token_expiration(temp.path(), "test").unwrap();
        assert!(!status.is_expired);
        assert_eq!(status.warning_level, "ok");
        assert!(status.days_until_expiration.unwrap() > 0);
    }

    #[test]
    fn test_expiration_expired() {
        let temp = TempDir::new().unwrap();

        record_pat_metadata(
            temp.path(),
            "test",
            Some("2020-01-01T10:00:00+08:00".to_string()),
        )
        .unwrap();

        let status = check_token_expiration(temp.path(), "test").unwrap();
        assert!(status.is_expired);
        assert_eq!(status.warning_level, "expired");
        assert!(status.days_until_expiration.unwrap() < 0);
    }

    #[test]
    fn test_detect_auth_mode_priority() {
        let temp = TempDir::new().unwrap();
        let home_root = temp.path();
        let codex_home = temp.path().join("codex-a");
        std::fs::create_dir_all(&codex_home).unwrap();

        // Create PAT metadata (priority 1 - should override config)
        record_pat_metadata(
            home_root,
            "a",
            Some("2030-12-31T10:00:00+08:00".to_string()),
        )
        .unwrap();

        let config = CodexConfigBinding {
            provider_id: Some("test".to_string()),
            model: None,
            auth_mode: Some("config".to_string()),
        };

        let detected = detect_auth_mode(home_root, "a", &codex_home, &config);
        assert_eq!(detected, Some("personal_token".to_string()));
    }

    #[test]
    fn test_oauth_token_evidence_wins_over_openai_api_key_marker() {
        let mut auth = serde_json::Map::new();
        auth.insert(
            "OPENAI_API_KEY".to_string(),
            serde_json::Value::String("redacted".to_string()),
        );
        auth.insert(
            "tokens".to_string(),
            serde_json::json!({ "access_token": "redacted" }),
        );

        assert_eq!(infer_auth_type(&auth), "oauth");
    }

    #[test]
    fn pat_usage_timeline_records_successful_switch() {
        let temp = TempDir::new().unwrap();
        let home_root = temp.path();
        let account_home = home_root.join(".codex-c");
        std::fs::create_dir_all(&account_home).unwrap();
        std::fs::write(account_home.join("auth.json"), r#"{"source":"runtime"}"#).unwrap();

        switch_to_pat_account(home_root, "c").unwrap();

        let timeline_path = config_root(home_root).join("pat-usage-timeline.json");
        let timeline: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(timeline_path).unwrap()).unwrap();
        let event = &timeline["events"][0];
        assert_eq!(event["accountId"], "c");
        assert_eq!(event["accountLabel"], "codex-c");
        assert_eq!(event["workspaceId"], "workspace:main");
        assert!(event["endedAt"].is_null());
    }

    #[test]
    fn pat_switch_copies_and_removes_auth_f_json() {
        let temp = TempDir::new().unwrap();
        let home_root = temp.path();
        let account_home = home_root.join(".codex-c");
        std::fs::create_dir_all(&account_home).unwrap();
        std::fs::write(account_home.join("auth.json"), r#"{"source":"runtime"}"#).unwrap();
        std::fs::write(
            account_home.join("auth-f.json"),
            r#"{"tokens":{"access_token":"token-c"}}"#,
        )
        .unwrap();

        let target_codex = home_root.join(".codex");
        let target_auth_f = target_codex.join("auth-f.json");

        // 1. Switch to account "c" (should copy auth-f.json)
        switch_to_pat_account(home_root, "c").unwrap();
        assert!(target_auth_f.exists());
        let content: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&target_auth_f).unwrap()).unwrap();
        assert_eq!(content["tokens"]["access_token"], "token-c");

        // 2. Switch to account "d" which does NOT have auth-f.json (should remove stale target auth-f.json)
        let account_home_d = home_root.join(".codex-d");
        std::fs::create_dir_all(&account_home_d).unwrap();
        std::fs::write(
            account_home_d.join("auth.json"),
            r#"{"source":"runtime-d"}"#,
        )
        .unwrap();

        switch_to_pat_account(home_root, "d").unwrap();
        assert!(!target_auth_f.exists());
    }

    #[cfg(unix)]
    #[test]
    fn delete_profile_process_matcher_targets_only_selected_profile() {
        let temp = TempDir::new().unwrap();
        let home = temp.path();
        let selected_home = home.join(".codex-Jone1");
        let selected_wrapper = home.join("bin/codex-Jone1");
        let other_home = home.join(".codex-c");
        let ps_output = format!(
            " 100 /bin/zsh {}\n 101 git -C {}/.tmp/plugins-clone fetch\n 102 {}\n 103 codex --home {}\n",
            selected_wrapper.display(),
            selected_home.display(),
            std::env::current_exe().unwrap().display(),
            other_home.display(),
        );

        let process_ids =
            profile_process_ids_from_ps(&ps_output, 102, &selected_home, Some(&selected_wrapper));

        assert_eq!(process_ids, vec![100, 101]);
    }
}
