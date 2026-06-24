use super::error::{AppError, Result};
use super::types::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
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
    pub credentials: UploadedCredentials,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddPatAccountResult {
    pub account_id: String,
    pub email: String,
    pub expired: String,
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
    pub expired: String,  // ISO 8601 format
    #[serde(default)]
    pub headers: Option<serde_json::Map<String, serde_json::Value>>,
    pub id_token: Option<String>,
    pub last_refresh: String,
    pub refresh_token: Option<String>,
    #[serde(rename = "type")]
    pub credential_type: String,
    pub websockets: bool,
}

/// Lam-tracked PAT metadata (stored in ~/.config/agent-workspace/auth-metadata/)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthMetadata {
    pub profile_id: String,
    pub auth_type: String,  // "personal_token" | "oauth" | "api_key"
    pub token_expiration: Option<String>,  // ISO 8601
    pub last_checked: String,  // ISO 8601
}

/// Token expiration status for UI display
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TokenExpirationStatus {
    pub profile_id: String,
    pub is_expired: bool,
    pub days_until_expiration: Option<i64>,
    pub expiration_date: Option<String>,
    pub warning_level: String,  // "ok" | "warning" | "critical" | "expired"
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
            renewal_date: note.and_then(|metadata| metadata.renewal_date.clone()),
            note: note.and_then(|metadata| metadata.note.clone()),
        });
    }
    
    // Scan PAT accounts
    let pat_dir = pat_accounts_dir(home_root);
    if pat_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&pat_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("auth-") && name.ends_with(".json") {
                        // Extract account_id from "auth-{id}.json"
                        let account_id = name
                            .strip_prefix("auth-")
                            .and_then(|s| s.strip_suffix(".json"))
                            .unwrap_or("");
                        
                        if account_id.is_empty() {
                            continue;
                        }
                        
                        // Read metadata
                        let metadata_path = pat_metadata_path(home_root, account_id);
                        let metadata: Option<serde_json::Value> = std::fs::read_to_string(&metadata_path)
                            .ok()
                            .and_then(|s| serde_json::from_str(&s).ok());
                        
                        let email = metadata.as_ref()
                            .and_then(|m| m.get("email"))
                            .and_then(|e| e.as_str())
                            .unwrap_or("");
                        
                        let expired = metadata.as_ref()
                            .and_then(|m| m.get("expired"))
                            .and_then(|e| e.as_str());
                        
                        accounts.push(CodexAccount {
                            id: account_id.to_string(),
                            display_name: format!("PAT: {} ({})", account_id, email),
                            codex_home: home_root.join(".codex"), // Shared
                            wrapper_path: None,
                            has_auth: true,
                            has_config: true, // Shares config.toml
                            has_history: false,
                            session_count: 0, // TODO: count shared sessions?
                            latest_session_modified_at: None,
                            managed: true,
                            is_relay: false,
                            relay_source: None,
                            relay_identity: None,
                            provider_id: Some("anthropic".to_string()), // Assume Codex
                            model: None,
                            auth_mode: Some("personal_token".to_string()),
                            renewal_date: expired.map(|s| s.to_string()),
                            note: None,
                        });
                    }
                }
            }
        }
    }
    
    accounts.sort_by(|a, b| a.id.cmp(&b.id));
    write_accounts_cache(home_root, &accounts)?;
    Ok(accounts)
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
    let plan = create_account_plan(home_root, req)?;
    let name = validate_profile_name(&req.name)?;
    let home = codex_home_path(home_root, &name);
    let wrapper = wrapper_path(home_root, &name);
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
    write_executable(&wrapper, &wrapper_script(&name))?;
    Ok(CreateResult {
        profile_id: name,
        home_path: home,
        wrapper_path: wrapper,
        operations: plan.operations,
        warnings: plan.warnings,
    })
}

pub fn rename_account_plan(home_root: &Path, req: &RenameAccountRequest) -> Result<OperationPlan> {
    let from = find_account(home_root, &req.from_profile_id)?;
    if from.id == "main" {
        return Err(AppError::new(
            "MAIN_ACCOUNT_RENAME_BLOCKED",
            "The main ~/.codex profile cannot be renamed",
        ));
    }
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
    write_executable(&target_wrapper, &wrapper_script(&to_name))?;
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
    write_executable(&wrapper, &wrapper_script(&name))?;
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

fn wrapper_script(name: &str) -> String {
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
    if accounts.is_empty() {
        return Ok(());
    }
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
    use crate::services::types::{auth_metadata_dir, auth_metadata_path};
    
    let metadata = AuthMetadata {
        profile_id: profile_id.to_string(),
        auth_type: "personal_token".to_string(),
        token_expiration: expiration,
        last_checked: chrono::Utc::now().to_rfc3339(),
    };

    let dir = auth_metadata_dir(home_root);
    std::fs::create_dir_all(&dir).map_err(|e| {
        AppError::new("CREATE_DIR_FAILED", format!("Failed to create auth-metadata dir: {}", e))
    })?;

    let path = auth_metadata_path(home_root, profile_id);
    let content = serde_json::to_string_pretty(&metadata).map_err(|e| {
        AppError::new("SERIALIZE_FAILED", format!("Serialize failed: {}", e))
    })?;

    std::fs::write(&path, content).map_err(|e| {
        AppError::new("WRITE_METADATA_FAILED", format!("Write failed: {}", e))
    })?;

    Ok(())
}

/// Reads PAT metadata for a profile
pub fn read_pat_metadata(home_root: &Path, profile_id: &str) -> Result<Option<AuthMetadata>> {
    use crate::services::types::auth_metadata_path;
    
    let path = auth_metadata_path(home_root, profile_id);
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path).map_err(|e| {
        AppError::new("READ_METADATA_FAILED", format!("Failed to read: {}", e))
    })?;

    let metadata: AuthMetadata = serde_json::from_str(&content).map_err(|e| {
        AppError::new("INVALID_METADATA", format!("Invalid metadata: {}", e))
    })?;

    Ok(Some(metadata))
}

/// Transforms uploaded credentials and records metadata
pub fn process_uploaded_credentials(
    home_root: &Path,
    profile_id: &str,
    creds: &UploadedCredentials,
) -> Result<()> {
    if creds.access_token.is_empty() {
        return Err(AppError::new(
            "INVALID_CREDENTIALS",
            "access_token is required",
        ));
    }

    // Validate expiration date format
    if chrono::DateTime::parse_from_rfc3339(&creds.expired).is_err() {
        return Err(AppError::new(
            "INVALID_EXPIRATION",
            "expired field must be valid ISO 8601 date",
        ));
    }

    // Record metadata in Lam's config
    record_pat_metadata(home_root, profile_id, Some(creds.expired.clone()))?;

    Ok(())
}

/// Checks token expiration from metadata
pub fn check_token_expiration(
    home_root: &Path,
    profile_id: &str,
) -> Result<TokenExpirationStatus> {
    let metadata = read_pat_metadata(home_root, profile_id)?;

    let (expiration_date, is_expired, days_until, warning_level) = match metadata {
        Some(meta) if meta.token_expiration.is_some() => {
            let exp_str = meta.token_expiration.unwrap();
            let expiry = chrono::DateTime::parse_from_rfc3339(&exp_str).map_err(|e| {
                AppError::new("INVALID_EXPIRATION_FORMAT", e.to_string())
            })?;

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

/// Adds a new PAT account by storing credentials in Lam storage
pub fn add_pat_account(
    home_root: &Path,
    req: &AddPatAccountRequest,
) -> Result<AddPatAccountResult> {
    let account_id = &req.credentials.account_id;
    
    // 1. Validate account_id not empty
    if account_id.trim().is_empty() {
        return Err(AppError::new("INVALID_ACCOUNT_ID", "account_id cannot be empty"));
    }
    
    // 2. Check if account already exists (both OAuth and PAT)
    if codex_home_path(home_root, account_id).exists() {
        return Err(AppError::new("ACCOUNT_EXISTS", 
            format!("OAuth account '{}' already exists", account_id)));
    }
    
    let auth_path = pat_auth_path(home_root, account_id);
    if auth_path.exists() {
        return Err(AppError::new("ACCOUNT_EXISTS", 
            format!("PAT account '{}' already exists", account_id)));
    }
    
    // 3. Extract token from headers.authorization (optional)
    let token = extract_bearer_token(&req.credentials)?;
    
    // 4. Generate and save auth-{id}.json
    let auth_json = generate_pat_auth_json(token.as_deref());
    let dir = pat_accounts_dir(home_root);
    std::fs::create_dir_all(&dir).map_err(|e| {
        AppError::new("CREATE_DIR_FAILED", format!("Failed to create pat-accounts dir: {}", e))
    })?;
    write_file_private(&auth_path, &auth_json)?;
    
    // 5. Save metadata
    let metadata = serde_json::json!({
        "accountId": account_id,
        "email": req.credentials.email,
        "expired": req.credentials.expired,
        "type": req.credentials.credential_type,
        "addedAt": chrono::Utc::now().to_rfc3339(),
    });
    let metadata_path = pat_metadata_path(home_root, account_id);
    let metadata_str = serde_json::to_string_pretty(&metadata).map_err(|e| {
        AppError::new("SERIALIZE_FAILED", format!("Metadata serialize failed: {}", e))
    })?;
    write_file_private(&metadata_path, &metadata_str)?;
    
    Ok(AddPatAccountResult {
        account_id: account_id.clone(),
        email: req.credentials.email.clone(),
        expired: req.credentials.expired.clone(),
    })
}

fn extract_bearer_token(creds: &UploadedCredentials) -> Result<Option<String>> {
    let headers = match creds.headers.as_ref() {
        Some(h) => h,
        None => return Ok(None), // No headers = no token (optional)
    };
    
    let auth_value = match headers.get("authorization").and_then(|v| v.as_str()) {
        Some(v) => v,
        None => return Ok(None), // No authorization header = no token (optional)
    };
    
    if let Some(token) = auth_value.strip_prefix("Bearer ") {
        Ok(Some(token.to_string()))
    } else {
        Err(AppError::new("INVALID_AUTH_FORMAT", "Authorization must be 'Bearer <token>'"))
    }
}

fn generate_pat_auth_json(token: Option<&str>) -> String {
    match token {
        Some(t) => format!(r#"{{
  "OPENAI_API_KEY": null,
  "personal_access_token": "{}"
}}"#, json_escape(t)),
        None => r#"{
  "OPENAI_API_KEY": null
}"#.to_string(),
    }
}

/// Switches to a PAT account by copying its auth file to ~/.codex/auth.json
pub fn switch_to_pat_account(
    home_root: &Path,
    account_id: &str,
) -> Result<()> {
    // 1. Verify PAT account exists
    let source_auth = pat_auth_path(home_root, account_id);
    if !source_auth.exists() {
        return Err(AppError::new("ACCOUNT_NOT_FOUND", 
            format!("PAT account '{}' not found", account_id)));
    }
    
    // 2. Copy to ~/.codex/auth.json
    let codex_home = home_root.join(".codex");
    std::fs::create_dir_all(&codex_home).map_err(|e| {
        AppError::new("CREATE_DIR_FAILED", format!("Failed to create .codex dir: {}", e))
    })?;
    
    let target_auth = codex_home.join("auth.json");
    std::fs::copy(&source_auth, &target_auth).map_err(|e| {
        AppError::new("COPY_FAILED", format!("Failed to copy auth file: {}", e))
    })?;
    
    // Set private permissions
    set_file_private(&target_auth)?;
    
    Ok(())
}

/// Detects auth mode by checking both Lam metadata and Codex auth.json
fn detect_auth_mode(
    home_root: &Path,
    profile_id: &str,
    codex_home: &Path,
    config: &CodexConfigBinding,
) -> Option<String> {
    // Priority 1: Check if Lam has recorded PAT metadata
    if let Ok(Some(metadata)) = read_pat_metadata(home_root, profile_id) {
        if metadata.auth_type == "personal_token" {
            return Some("personal_token".to_string());
        }
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

        record_pat_metadata(home_root, "test-profile", Some("2030-12-31T10:00:00+08:00".to_string())).unwrap();

        let metadata = read_pat_metadata(home_root, "test-profile").unwrap().unwrap();
        assert_eq!(metadata.profile_id, "test-profile");
        assert_eq!(metadata.auth_type, "personal_token");
        assert_eq!(metadata.token_expiration, Some("2030-12-31T10:00:00+08:00".to_string()));
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
        };

        process_uploaded_credentials(temp.path(), "test", &creds).unwrap();

        let metadata = read_pat_metadata(temp.path(), "test").unwrap().unwrap();
        assert_eq!(metadata.auth_type, "personal_token");
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
        };
        assert!(process_uploaded_credentials(temp.path(), "test", &creds).is_err());
    }
    #[test]
    fn test_expiration_not_expired() {
        let temp = TempDir::new().unwrap();

        record_pat_metadata(temp.path(), "test", Some("2030-12-31T10:00:00+08:00".to_string())).unwrap();

        let status = check_token_expiration(temp.path(), "test").unwrap();
        assert!(!status.is_expired);
        assert_eq!(status.warning_level, "ok");
        assert!(status.days_until_expiration.unwrap() > 0);
    }

    #[test]
    fn test_expiration_expired() {
        let temp = TempDir::new().unwrap();

        record_pat_metadata(temp.path(), "test", Some("2020-01-01T10:00:00+08:00".to_string())).unwrap();

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
        record_pat_metadata(home_root, "a", Some("2030-12-31T10:00:00+08:00".to_string())).unwrap();

        let config = CodexConfigBinding {
            provider_id: Some("test".to_string()),
            model: None,
            auth_mode: Some("config".to_string()),
        };

        let detected = detect_auth_mode(home_root, "a", &codex_home, &config);
        assert_eq!(detected, Some("personal_token".to_string()));
    }
}
