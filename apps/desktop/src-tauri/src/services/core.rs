use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const NEW_MARKER: &str = ".managed-by-agent-workspace.json";
const OLD_MARKER: &str = ".managed-by-codex-session-manager.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
    pub details: Option<Value>,
}

impl AppError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            recoverable: true,
            details: None,
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(value: std::io::Error) -> Self {
        Self::new("IO_ERROR", value.to_string())
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

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
}

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
pub struct SyncRequest {
    pub from_profile_id: String,
    pub to_profile_id: String,
    pub sync_sessions: bool,
    pub backup_target_sessions: bool,
    pub sidecar_backup_history: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncOperation {
    pub kind: String,
    pub from: Option<PathBuf>,
    pub to: Option<PathBuf>,
    pub rel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncPlan {
    pub from_profile_id: String,
    pub to_profile_id: String,
    pub operations: Vec<SyncOperation>,
    pub warnings: Vec<String>,
    pub blocked_files: Vec<String>,
    pub policy_blocked_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub copied: usize,
    pub skipped: usize,
    pub backup_path: Option<PathBuf>,
    pub manifest_path: PathBuf,
    pub warnings: Vec<String>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageQuotaSnapshot {
    pub profile_id: String,
    pub source: String,
    pub fetched_at: u64,
    pub staleness: String,
    pub plan_type: Option<String>,
    pub activity_tokens: Option<u64>,
    pub primary_used_percent: Option<u8>,
    pub secondary_used_percent: Option<u8>,
    pub remaining_percent: Option<u8>,
    pub reset_at: Option<String>,
    pub secondary_reset_at: Option<String>,
    pub alerts: Vec<String>,
    pub suggested_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuotaRefreshResult {
    pub snapshots: Vec<UsageQuotaSnapshot>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfile {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub wire_api: String,
    pub default_model: String,
    pub env_key: Option<String>,
    pub secret_storage: String,
    pub health: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateProviderRequest {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub wire_api: String,
    pub default_model: String,
    pub env_key: Option<String>,
    pub secret: Option<SecretInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProviderRequest {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub wire_api: String,
    pub default_model: String,
    pub env_key: Option<String>,
    pub secret: Option<SecretInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SecretInput {
    Env { env_key: String },
    Keychain { secret: String },
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttachProviderRequest {
    pub profile_id: String,
    pub provider_id: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttachProviderResult {
    pub profile_id: String,
    pub provider_id: String,
    pub config_path: PathBuf,
    pub backup_path: PathBuf,
    pub operations: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn list_accounts(home_root: &Path) -> Result<Vec<CodexAccount>> {
    let mut accounts = Vec::new();
    if !home_root.exists() {
        return Ok(accounts);
    }

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
            auth_mode: config.auth_mode,
        });
    }
    accounts.sort_by(|a, b| a.id.cmp(&b.id));
    write_accounts_cache(home_root, &accounts)?;
    Ok(accounts)
}

pub fn list_cached_accounts(home_root: &Path) -> Result<Vec<CodexAccount>> {
    Ok(read_accounts_cache(home_root)?.unwrap_or_default())
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

pub fn sync_plan(home_root: &Path, req: &SyncRequest) -> Result<SyncPlan> {
    let from = find_account(home_root, &req.from_profile_id)?;
    let to = find_account(home_root, &req.to_profile_id)?;
    if same_path(&from.codex_home, &to.codex_home) {
        return Err(AppError::new(
            "SYNC_SAME_PROFILE",
            "source and target must differ",
        ));
    }
    let mut operations = Vec::new();
    let mut warnings = Vec::new();
    let policy_blocked_files = blocked_files();
    let blocked_files = seen_blocked_files(&from.codex_home)?;
    if !to.is_relay {
        warnings.push("Target is a primary profile; relay profile is recommended.".into());
    }
    match (&from.provider_id, &to.provider_id) {
        (Some(source), Some(target)) if source != target => warnings.push(format!(
            "Provider mismatch: source uses {source}, target uses {target}."
        )),
        (None, _) | (_, None) => warnings.push(
            "Provider mismatch check is incomplete because one side has unknown provider.".into(),
        ),
        _ => {}
    }
    if req.backup_target_sessions && to.codex_home.join("sessions").exists() {
        operations.push(SyncOperation {
            kind: "backup_dir".into(),
            from: Some(to.codex_home.join("sessions")),
            to: Some(to.codex_home.join("sessions.backup.<timestamp>")),
            rel: Some("sessions".into()),
        });
    }
    if req.sync_sessions {
        let from_sessions = from.codex_home.join("sessions");
        let to_sessions = to.codex_home.join("sessions");
        let files = session_files(&from_sessions)?;
        if files.len() > 5_000 {
            warnings.push(format!(
                "Large session directory: {} files will be evaluated.",
                files.len()
            ));
        }
        for file in files {
            let rel_path = file
                .strip_prefix(&from_sessions)
                .map_err(|_| AppError::new("PATH_ERROR", "session path outside source"))?;
            let rel = rel_path.to_string_lossy().to_string();
            let dst = to_sessions.join(rel_path);
            let kind = if should_skip_copy(&file, &dst)? {
                "skip_file"
            } else {
                "copy_file"
            };
            operations.push(SyncOperation {
                kind: kind.into(),
                from: Some(file),
                to: Some(dst),
                rel: Some(rel),
            });
        }
    }
    if req.sidecar_backup_history {
        operations.push(SyncOperation {
            kind: "copy_history_sidecar".into(),
            from: Some(from.codex_home.join("history.jsonl")),
            to: Some(
                to.codex_home
                    .join(format!("history.from-{}.jsonl", from.id)),
            ),
            rel: Some("history.jsonl".into()),
        });
    }
    Ok(SyncPlan {
        from_profile_id: req.from_profile_id.clone(),
        to_profile_id: req.to_profile_id.clone(),
        operations,
        warnings,
        blocked_files,
        policy_blocked_files,
    })
}

pub fn execute_sync(home_root: &Path, req: &SyncRequest) -> Result<SyncResult> {
    let plan = sync_plan(home_root, req)?;
    let from = find_account(home_root, &req.from_profile_id)?;
    let to = find_account(home_root, &req.to_profile_id)?;
    let mut copied = 0;
    let mut skipped = 0;
    let mut backup_path = None;
    if req.backup_target_sessions && to.codex_home.join("sessions").exists() {
        let backup = to
            .codex_home
            .join(format!("sessions.backup.{}", timestamp_yyyymmdd_hhmmss()));
        copy_dir_recursive(&to.codex_home.join("sessions"), &backup)?;
        backup_path = Some(backup);
    }
    for op in plan
        .operations
        .iter()
        .filter(|op| op.kind == "copy_file" || op.kind == "skip_file")
    {
        if op.kind == "skip_file" {
            skipped += 1;
            continue;
        }
        let src = op
            .from
            .as_ref()
            .ok_or_else(|| AppError::new("SYNC_PLAN_INVALID", "copy operation missing source"))?;
        let dst = op
            .to
            .as_ref()
            .ok_or_else(|| AppError::new("SYNC_PLAN_INVALID", "copy operation missing target"))?;
        if should_skip_copy(src, dst)? {
            skipped += 1;
            continue;
        }
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dst)?;
        copied += 1;
    }
    if req.sidecar_backup_history {
        let src = from.codex_home.join("history.jsonl");
        let dst = to
            .codex_home
            .join(format!("history.from-{}.jsonl", from.id));
        if src.exists() {
            fs::copy(src, dst)?;
        }
    }
    let manifest_dir = home_root.join(".config/agent-workspace/sync-manifests");
    fs::create_dir_all(&manifest_dir)?;
    let manifest_path = manifest_dir.join(format!("{}.json", Uuid::new_v4()));
    write_file_private(
        &manifest_path,
        &sync_manifest_json(&plan, copied, skipped, backup_path.as_ref()),
    )?;
    Ok(SyncResult {
        copied,
        skipped,
        backup_path,
        manifest_path,
        warnings: plan.warnings,
    })
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

pub fn get_profile_quota(
    home_root: &Path,
    profile_id: &str,
    force_refresh: bool,
) -> Result<UsageQuotaSnapshot> {
    let account = quota_account(home_root, profile_id)?;
    if force_refresh && app_server_quota_enabled() {
        match try_codex_app_server_quota(&account) {
            Ok(snapshot) => {
                write_quota_cache(home_root, &snapshot)?;
                return Ok(snapshot);
            }
            Err(err) => {
                if let Some(mut cached) = read_quota_cache(home_root, profile_id)? {
                    cached.alerts.push(format!(
                        "Codex app-server quota unavailable: {}",
                        err.message
                    ));
                    return Ok(cached);
                }
                return Ok(unavailable_quota_snapshot(
                    profile_id,
                    Some(format!(
                        "Codex app-server quota unavailable: {}",
                        err.message
                    )),
                ));
            }
        }
    }
    if let Some(cached) = read_quota_cache(home_root, profile_id)? {
        return Ok(cached);
    }
    Ok(unavailable_quota_snapshot(profile_id, None))
}

pub fn list_cached_quotas(
    home_root: &Path,
    profile_ids: Option<Vec<String>>,
) -> Result<Vec<UsageQuotaSnapshot>> {
    let requested = if let Some(profile_ids) = profile_ids {
        profile_ids
    } else {
        list_accounts(home_root)?
            .iter()
            .map(|account| account.id.clone())
            .collect()
    };
    let mut snapshots = Vec::new();
    for profile_id in requested {
        if let Some(snapshot) = read_quota_cache(home_root, &profile_id)? {
            snapshots.push(snapshot);
        }
    }
    snapshots.sort_by(|a, b| a.profile_id.cmp(&b.profile_id));
    Ok(snapshots)
}

pub fn refresh_all_quotas(
    home_root: &Path,
    profile_ids: Option<Vec<String>>,
) -> Result<QuotaRefreshResult> {
    let accounts = list_accounts(home_root)?;
    let requested = profile_ids.unwrap_or_else(|| accounts.iter().map(|a| a.id.clone()).collect());
    let mut snapshots = Vec::new();
    let mut warnings = Vec::new();
    for profile_id in requested {
        match get_profile_quota(home_root, &profile_id, true) {
            Ok(snapshot) => snapshots.push(snapshot),
            Err(err) => warnings.push(format!("{profile_id}: {}", err.message)),
        }
    }
    Ok(QuotaRefreshResult {
        snapshots,
        warnings,
    })
}

pub fn list_providers(home_root: &Path) -> Result<Vec<ProviderProfile>> {
    read_provider_store(home_root)
}

pub fn create_provider(home_root: &Path, req: &CreateProviderRequest) -> Result<ProviderProfile> {
    let id = validate_provider_id(&req.id)?;
    let mut providers = read_provider_store(home_root)?;
    if providers.iter().any(|provider| provider.id == id) {
        return Err(AppError::new("PROVIDER_ALREADY_EXISTS", id));
    }
    let provider = provider_from_create_request(id, req)?;
    persist_secret(home_root, &provider.id, req.secret.as_ref())?;
    providers.push(provider.clone());
    providers.sort_by(|a, b| a.id.cmp(&b.id));
    write_provider_store(home_root, &providers)?;
    Ok(provider)
}

pub fn update_provider(home_root: &Path, req: &UpdateProviderRequest) -> Result<ProviderProfile> {
    let id = validate_provider_id(&req.id)?;
    let mut providers = read_provider_store(home_root)?;
    let pos = providers
        .iter()
        .position(|provider| provider.id == id)
        .ok_or_else(|| AppError::new("PROVIDER_NOT_FOUND", &id))?;
    let create = CreateProviderRequest {
        id: id.clone(),
        name: req.name.clone(),
        base_url: req.base_url.clone(),
        wire_api: req.wire_api.clone(),
        default_model: req.default_model.clone(),
        env_key: req.env_key.clone(),
        secret: req.secret.clone(),
    };
    let provider = provider_from_create_request(id, &create)?;
    persist_secret(home_root, &provider.id, req.secret.as_ref())?;
    providers[pos] = provider.clone();
    providers.sort_by(|a, b| a.id.cmp(&b.id));
    write_provider_store(home_root, &providers)?;
    Ok(provider)
}

pub fn delete_provider(home_root: &Path, provider_id: &str) -> Result<bool> {
    let bound = profiles_using_provider(home_root, provider_id)?;
    if !bound.is_empty() {
        return Err(AppError {
            code: "PROVIDER_IN_USE".into(),
            message: format!("provider {provider_id} is attached to profiles"),
            recoverable: true,
            details: Some(serde_json::json!({ "profiles": bound })),
        });
    }
    let mut providers = read_provider_store(home_root)?;
    let before = providers.len();
    providers.retain(|provider| provider.id != provider_id);
    if providers.len() == before {
        return Ok(false);
    }
    write_provider_store(home_root, &providers)?;
    Ok(true)
}

pub fn test_provider(home_root: &Path, provider_id: &str) -> Result<ProviderProfile> {
    let providers = read_provider_store(home_root)?;
    let mut provider = providers
        .into_iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| AppError::new("PROVIDER_NOT_FOUND", provider_id))?;
    provider.health = if provider.secret_storage == "env"
        && provider
            .env_key
            .as_ref()
            .map(|key| std::env::var(key).is_ok())
            .unwrap_or(false)
    {
        "available".into()
    } else if provider.secret_storage == "keychain" {
        "stored".into()
    } else {
        "metadata_only".into()
    };
    Ok(provider)
}

pub fn plan_attach_provider_to_profile(
    home_root: &Path,
    req: &AttachProviderRequest,
) -> Result<OperationPlan> {
    let account = find_account(home_root, &req.profile_id)?;
    let provider = find_provider(home_root, &req.provider_id)?;
    Ok(OperationPlan {
        operations: vec![
            format!("backup config.toml {}", account.codex_home.display()),
            format!("write provider reference {} -> {}", account.id, provider.id),
        ],
        warnings: vec![
            "Attach writes provider metadata only; secrets remain in env or Keychain.".into(),
        ],
        blocked: vec!["api_key".into(), "auth.json".into()],
    })
}

pub fn attach_provider_to_profile(
    home_root: &Path,
    req: &AttachProviderRequest,
) -> Result<AttachProviderResult> {
    execute_attach_provider_to_profile(home_root, req)
}

pub fn execute_attach_provider_to_profile(
    home_root: &Path,
    req: &AttachProviderRequest,
) -> Result<AttachProviderResult> {
    let plan = plan_attach_provider_to_profile(home_root, req)?;
    let account = find_account(home_root, &req.profile_id)?;
    let provider = find_provider(home_root, &req.provider_id)?;
    let config_path = account.codex_home.join("config.toml");
    let backup_path = account.codex_home.join(format!(
        "config.toml.backup.{}",
        timestamp_yyyymmdd_hhmmss()
    ));
    if config_path.exists() {
        fs::copy(&config_path, &backup_path)?;
    } else {
        write_file_private(&backup_path, "")?;
    }
    let model = req
        .model
        .clone()
        .unwrap_or_else(|| provider.default_model.clone());
    let body = format!(
        "model = \"{}\"\nmodel_provider = \"{}\"\nprovider_base_url = \"{}\"\nprovider_wire_api = \"{}\"\nenv_key = {}\n",
        json_escape(&model),
        json_escape(&provider.id),
        json_escape(&provider.base_url),
        json_escape(&provider.wire_api),
        provider
            .env_key
            .as_ref()
            .map(|key| format!("\"{}\"", json_escape(key)))
            .unwrap_or_else(|| "null".into())
    );
    write_file_private(&config_path, &body)?;
    Ok(AttachProviderResult {
        profile_id: req.profile_id.clone(),
        provider_id: req.provider_id.clone(),
        config_path,
        backup_path,
        operations: plan.operations,
        warnings: plan.warnings,
    })
}

fn is_codex_home_name(name: &str) -> bool {
    name == ".codex" || name.starts_with(".codex-")
}

fn has_codex_signal(home: &Path) -> bool {
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

fn account_id_from_dir_name(name: &str) -> String {
    if name == ".codex" {
        "main".into()
    } else {
        name.trim_start_matches(".codex-").into()
    }
}

fn codex_home_path(home_root: &Path, name: &str) -> PathBuf {
    if name == "main" {
        home_root.join(".codex")
    } else {
        home_root.join(format!(".codex-{name}"))
    }
}

fn wrapper_path(home_root: &Path, name: &str) -> PathBuf {
    home_root.join("bin").join(format!("codex-{name}"))
}

fn find_account(home_root: &Path, profile_id: &str) -> Result<CodexAccount> {
    list_accounts(home_root)?
        .into_iter()
        .find(|a| a.id == profile_id)
        .ok_or_else(|| AppError::new("ACCOUNT_NOT_FOUND", profile_id))
}

fn quota_account(home_root: &Path, profile_id: &str) -> Result<CodexAccount> {
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
    })
}

fn unavailable_quota_snapshot(profile_id: &str, alert: Option<String>) -> UsageQuotaSnapshot {
    UsageQuotaSnapshot {
        profile_id: profile_id.to_string(),
        source: "usage_unavailable".into(),
        fetched_at: system_secs(SystemTime::now()),
        staleness: "unavailable".into(),
        plan_type: None,
        activity_tokens: None,
        primary_used_percent: None,
        secondary_used_percent: None,
        remaining_percent: None,
        reset_at: None,
        secondary_reset_at: None,
        alerts: alert.into_iter().collect(),
        suggested_actions: Vec::new(),
    }
}

fn validate_profile_name(input: &str) -> Result<String> {
    let name = input.trim();
    if name.is_empty() || name == "main" || name == "default" || name.len() > 32 {
        return Err(AppError::new("INVALID_ACCOUNT_NAME", input));
    }
    let mut chars = name.chars();
    let first = chars
        .next()
        .ok_or_else(|| AppError::new("INVALID_ACCOUNT_NAME", input))?;
    if !first.is_ascii_alphanumeric() {
        return Err(AppError::new("INVALID_ACCOUNT_NAME", input));
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(AppError::new("INVALID_ACCOUNT_NAME", input));
    }
    if name.contains("..") || name.contains('/') || name.contains('~') || name.contains(' ') {
        return Err(AppError::new("INVALID_ACCOUNT_NAME", input));
    }
    Ok(name.to_string())
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

fn session_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

fn modified_secs(path: &Path) -> Result<u64> {
    Ok(fs::metadata(path)?
        .modified()
        .ok()
        .map(system_secs)
        .unwrap_or(0))
}

fn system_secs(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn read_tail(path: &Path, max_bytes: usize) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    let start = buf.len().saturating_sub(max_bytes);
    Ok(String::from_utf8_lossy(&buf[start..]).to_string())
}

fn read_first_line(path: &Path) -> Result<String> {
    let file = fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(line)
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

fn short_text(input: &str, max_chars: usize) -> String {
    let compact = input.split_whitespace().collect::<Vec<_>>().join(" ");
    let char_count = compact.chars().count();
    if char_count > max_chars {
        let truncated: String = compact.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{truncated}...")
    } else {
        compact
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
        "{{\n  \"managedBy\": \"LocalAgentManager\",\n  \"accountName\": \"{}\",\n  \"kind\": \"{}\",\n  \"runtimeProfileId\": {},\n  \"sourceProfileId\": {},\n  \"providerPolicy\": {},\n  \"codexHome\": \"{}\",\n  \"wrapperPath\": \"{}\",\n  \"createdAt\": \"{}\"\n}}\n",
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

fn sync_manifest_json(
    plan: &SyncPlan,
    copied: usize,
    skipped: usize,
    backup: Option<&PathBuf>,
) -> String {
    let operations = plan
        .operations
        .iter()
        .map(|op| {
            format!(
                "{{\"kind\":\"{}\",\"from\":{},\"to\":{},\"rel\":{}}}",
                json_escape(&op.kind),
                op.from
                    .as_ref()
                    .map(|p| format!("\"{}\"", json_escape(&p.to_string_lossy())))
                    .unwrap_or_else(|| "null".into()),
                op.to
                    .as_ref()
                    .map(|p| format!("\"{}\"", json_escape(&p.to_string_lossy())))
                    .unwrap_or_else(|| "null".into()),
                op.rel
                    .as_ref()
                    .map(|s| format!("\"{}\"", json_escape(s)))
                    .unwrap_or_else(|| "null".into())
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{{\n  \"fromProfileId\": \"{}\",\n  \"toProfileId\": \"{}\",\n  \"timestamp\": \"{}\",\n  \"copied\": {},\n  \"skipped\": {},\n  \"backupPath\": {},\n  \"operations\": [{}],\n  \"blockedFiles\": [{}],\n  \"policyBlockedFiles\": [{}],\n  \"warnings\": [{}]\n}}\n",
        json_escape(&plan.from_profile_id),
        json_escape(&plan.to_profile_id),
        timestamp_yyyymmdd_hhmmss(),
        copied,
        skipped,
        backup
            .map(|p| format!("\"{}\"", json_escape(&p.to_string_lossy())))
            .unwrap_or_else(|| "null".into()),
        operations,
        plan.blocked_files
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect::<Vec<_>>()
            .join(", "),
        plan.policy_blocked_files
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect::<Vec<_>>()
            .join(", "),
        plan.warnings
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn json_option(value: Option<&str>) -> String {
    value
        .map(|s| format!("\"{}\"", json_escape(s)))
        .unwrap_or_else(|| "null".into())
}

fn json_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn shell_quote<S: AsRef<str>>(value: S) -> String {
    format!("'{}'", value.as_ref().replace('\'', "'\\''"))
}

#[derive(Debug, Default)]
struct CodexConfigBinding {
    provider_id: Option<String>,
    model: Option<String>,
    auth_mode: Option<String>,
}

fn parse_codex_config(path: &Path) -> Result<CodexConfigBinding> {
    if !path.exists() {
        return Ok(CodexConfigBinding::default());
    }
    let body = fs::read_to_string(path)?;
    let provider_id = parse_toml_like_string(&body, "model_provider")
        .or_else(|| parse_toml_like_string(&body, "provider"));
    let model = parse_toml_like_string(&body, "model");
    let auth_mode = if provider_id.is_some() {
        Some("config".into())
    } else {
        None
    };
    Ok(CodexConfigBinding {
        provider_id,
        model,
        auth_mode,
    })
}

fn parse_toml_like_string(body: &str, key: &str) -> Option<String> {
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        let (left, right) = trimmed.split_once('=')?;
        if left.trim() != key {
            continue;
        }
        let value = right.trim().trim_matches('"').trim_matches('\'').trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn config_root(home_root: &Path) -> PathBuf {
    home_root.join(".config/agent-workspace")
}

fn provider_store_path(home_root: &Path) -> PathBuf {
    config_root(home_root).join("providers.json")
}

fn quota_cache_dir(home_root: &Path) -> PathBuf {
    config_root(home_root).join("quota-cache")
}

fn accounts_cache_path(home_root: &Path) -> PathBuf {
    config_root(home_root).join("accounts-cache.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountsCacheFile {
    home_root: String,
    fetched_at: u64,
    accounts: Vec<CodexAccount>,
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

fn read_provider_store(home_root: &Path) -> Result<Vec<ProviderProfile>> {
    let path = provider_store_path(home_root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let body = fs::read_to_string(path)?;
    serde_json::from_str(&body)
        .map_err(|err| AppError::new("PROVIDER_STORE_INVALID", err.to_string()))
}

fn write_provider_store(home_root: &Path, providers: &[ProviderProfile]) -> Result<()> {
    let body = serde_json::to_string_pretty(providers)
        .map_err(|err| AppError::new("PROVIDER_STORE_INVALID", err.to_string()))?;
    write_file_private(&provider_store_path(home_root), &format!("{body}\n"))
}

fn write_quota_cache(home_root: &Path, snapshot: &UsageQuotaSnapshot) -> Result<()> {
    let path = quota_cache_dir(home_root).join(format!("{}.json", snapshot.profile_id));
    let body = serde_json::to_string_pretty(snapshot)
        .map_err(|err| AppError::new("QUOTA_CACHE_INVALID", err.to_string()))?;
    write_file_private(&path, &format!("{body}\n"))
}

fn read_quota_cache(home_root: &Path, profile_id: &str) -> Result<Option<UsageQuotaSnapshot>> {
    let path = quota_cache_dir(home_root).join(format!("{profile_id}.json"));
    if !path.exists() {
        return Ok(None);
    }
    let body = fs::read_to_string(path)?;
    let mut snapshot: UsageQuotaSnapshot = serde_json::from_str(&body)
        .map_err(|err| AppError::new("QUOTA_CACHE_INVALID", err.to_string()))?;
    if snapshot.source != "app_server_rate_limits" {
        return Ok(None);
    }
    snapshot.staleness = "cached".into();
    Ok(Some(snapshot))
}

fn provider_from_create_request(
    id: String,
    req: &CreateProviderRequest,
) -> Result<ProviderProfile> {
    let secret_storage = match &req.secret {
        Some(SecretInput::Env { env_key }) => {
            if req.env_key.as_deref() != Some(env_key.as_str()) {
                return Err(AppError::new(
                    "PROVIDER_ENV_MISMATCH",
                    "secret env key must match provider env_key",
                ));
            }
            "env"
        }
        Some(SecretInput::Keychain { .. }) => "keychain",
        Some(SecretInput::None) | None => "none",
    };
    if req.name.trim().is_empty()
        || req.base_url.trim().is_empty()
        || req.wire_api.trim().is_empty()
        || req.default_model.trim().is_empty()
    {
        return Err(AppError::new(
            "PROVIDER_INVALID",
            "required provider field is empty",
        ));
    }
    Ok(ProviderProfile {
        id,
        name: req.name.trim().to_string(),
        base_url: req.base_url.trim().to_string(),
        wire_api: req.wire_api.trim().to_string(),
        default_model: req.default_model.trim().to_string(),
        env_key: req.env_key.clone(),
        secret_storage: secret_storage.into(),
        health: "untested".into(),
    })
}

fn persist_secret(home_root: &Path, provider_id: &str, secret: Option<&SecretInput>) -> Result<()> {
    match secret {
        Some(SecretInput::Keychain { secret }) => store_keychain_secret(provider_id, secret),
        Some(SecretInput::Env { .. }) | Some(SecretInput::None) | None => {
            fs::create_dir_all(config_root(home_root))?;
            Ok(())
        }
    }
}

fn store_keychain_secret(provider_id: &str, secret: &str) -> Result<()> {
    if secret.trim().is_empty() {
        return Err(AppError::new("PROVIDER_SECRET_EMPTY", "secret is empty"));
    }
    let status = std::process::Command::new("/usr/bin/security")
        .arg("add-generic-password")
        .arg("-U")
        .arg("-s")
        .arg("agent-workspace-manager")
        .arg("-a")
        .arg(format!("provider:{provider_id}"))
        .arg("-w")
        .arg(secret)
        .status()
        .map_err(|err| AppError::new("KEYCHAIN_UNAVAILABLE", err.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::new(
            "KEYCHAIN_WRITE_FAILED",
            "macOS Keychain did not accept the provider secret",
        ))
    }
}

fn validate_provider_id(input: &str) -> Result<String> {
    let id = input.trim();
    if id.is_empty() || id.len() > 64 {
        return Err(AppError::new("PROVIDER_INVALID_ID", input));
    }
    let mut chars = id.chars();
    let first = chars
        .next()
        .ok_or_else(|| AppError::new("PROVIDER_INVALID_ID", input))?;
    if !first.is_ascii_alphanumeric() {
        return Err(AppError::new("PROVIDER_INVALID_ID", input));
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.') {
        return Err(AppError::new("PROVIDER_INVALID_ID", input));
    }
    Ok(id.to_string())
}

fn find_provider(home_root: &Path, provider_id: &str) -> Result<ProviderProfile> {
    read_provider_store(home_root)?
        .into_iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| AppError::new("PROVIDER_NOT_FOUND", provider_id))
}

fn profiles_using_provider(home_root: &Path, provider_id: &str) -> Result<Vec<String>> {
    let mut bound = Vec::new();
    for account in list_accounts(home_root)? {
        if account.provider_id.as_deref() == Some(provider_id) {
            bound.push(account.id);
        }
    }
    bound.sort();
    Ok(bound)
}

fn app_server_quota_enabled() -> bool {
    if let Ok(value) = std::env::var("LAM_ENABLE_CODEX_APP_SERVER_QUOTA") {
        return value == "1" || value.eq_ignore_ascii_case("true");
    }
    if let Ok(value) = std::env::var("LAM_DISABLE_CODEX_APP_SERVER_QUOTA") {
        return !(value == "1" || value.eq_ignore_ascii_case("true"));
    }
    true
}

fn try_codex_app_server_quota(account: &CodexAccount) -> Result<UsageQuotaSnapshot> {
    let codex_bin = std::env::var("LAM_CODEX_BIN").unwrap_or_else(|_| "codex".into());
    let mut child = std::process::Command::new(codex_bin)
        .arg("app-server")
        .arg("--stdio")
        .env("CODEX_HOME", &account.codex_home)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|err| AppError::new("CODEX_APP_SERVER_UNAVAILABLE", err.to_string()))?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(
                b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"clientInfo\":{\"name\":\"lam\",\"version\":\"0.1\"},\"capabilities\":{},\"protocolVersion\":\"2025-03-26\"}}\n",
            )
            .map_err(|err| AppError::new("CODEX_APP_SERVER_WRITE_FAILED", err.to_string()))?;
        stdin
            .write_all(
                b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\",\"params\":{}}\n",
            )
            .map_err(|err| AppError::new("CODEX_APP_SERVER_WRITE_FAILED", err.to_string()))?;
        stdin
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"account/rateLimits/read\",\"params\":{}}\n")
            .map_err(|err| AppError::new("CODEX_APP_SERVER_WRITE_FAILED", err.to_string()))?;
        stdin
            .flush()
            .map_err(|err| AppError::new("CODEX_APP_SERVER_WRITE_FAILED", err.to_string()))?;
    } else {
        return Err(AppError::new(
            "CODEX_APP_SERVER_NO_STDIN",
            "stdin not available",
        ));
    }
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::new("CODEX_APP_SERVER_NO_STDOUT", "stdout not available"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::new("CODEX_APP_SERVER_NO_STDERR", "stderr not available"))?;
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let (tx_err, rx_err) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let _ = tx.send(line.clone());
                }
                Err(_) => break,
            }
        }
    });
    std::thread::spawn(move || {
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(stderr);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let trimmed = line.trim().to_string();
                    if !trimmed.is_empty() {
                        let _ = tx_err.send(trimmed);
                    }
                }
                Err(_) => break,
            }
        }
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(3000);
    let mut parsed: Option<UsageQuotaSnapshot> = None;
    let mut last_stderr: Option<String> = None;
    loop {
        while let Ok(line) = rx_err.try_recv() {
            last_stderr = Some(line);
        }
        if parsed.is_none() {
            while let Ok(line) = rx.try_recv() {
                if let Some(snapshot) = parse_rate_limit_snapshot_line(&line, &account.id) {
                    parsed = Some(snapshot);
                    break;
                }
            }
        }
        if let Some(snapshot) = parsed.clone() {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(snapshot);
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|err| AppError::new("CODEX_APP_SERVER_WAIT_FAILED", err.to_string()))?
        {
            let stderr_hint = last_stderr
                .as_deref()
                .map(|line| format!(" ({line})"))
                .unwrap_or_default();
            return Err(if status.success() {
                AppError::new(
                    "CODEX_APP_SERVER_PROTOCOL_UNRESOLVED",
                    format!(
                        "app-server exited before a rate-limit response was parsed{}",
                        stderr_hint
                    ),
                )
            } else {
                AppError::new(
                    "CODEX_APP_SERVER_FAILED",
                    format!(
                        "app-server exited before quota could be read{}",
                        stderr_hint
                    ),
                )
            });
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            let stderr_hint = last_stderr
                .as_deref()
                .map(|line| format!(" ({line})"))
                .unwrap_or_default();
            return Err(AppError::new(
                "CODEX_APP_SERVER_TIMEOUT",
                format!("app-server quota request timed out{}", stderr_hint),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

fn parse_rate_limit_snapshot_line(line: &str, profile_id: &str) -> Option<UsageQuotaSnapshot> {
    let value: Value = serde_json::from_str(line).ok()?;
    let result = value.get("result").unwrap_or(&value);
    let primary = find_window(result, &["primary", "primary_window"])?;
    let secondary = find_window(result, &["secondary", "secondary_window"]);
    let primary_used = extract_percent(primary)?;
    let secondary_used = secondary.and_then(extract_percent);
    let reset_at = extract_reset(primary);
    let secondary_reset_at = secondary.and_then(extract_reset);
    let plan_type = extract_string(result, &["plan_type", "planType"]).or_else(|| {
        result
            .get("rateLimits")
            .or_else(|| result.get("rate_limits"))
            .and_then(|value| extract_string(value, &["plan_type", "planType"]))
    });

    Some(UsageQuotaSnapshot {
        profile_id: profile_id.to_string(),
        source: "app_server_rate_limits".into(),
        fetched_at: system_secs(SystemTime::now()),
        staleness: "fresh".into(),
        plan_type,
        activity_tokens: None,
        primary_used_percent: Some(primary_used),
        secondary_used_percent: secondary_used,
        remaining_percent: Some(100_u8.saturating_sub(primary_used)),
        reset_at,
        secondary_reset_at,
        alerts: Vec::new(),
        suggested_actions: if primary_used >= 90 {
            vec!["Session quota is high; switch profile or use relay.".into()]
        } else {
            Vec::new()
        },
    })
}

fn find_window<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    for key in keys {
        if let Some(window) = value.get(key) {
            return Some(window);
        }
    }
    if let Some(rate_limit) = value.get("rateLimit").or_else(|| value.get("rate_limit")) {
        for key in keys {
            if let Some(window) = rate_limit.get(key) {
                return Some(window);
            }
        }
    }
    if let Some(rate_limits_obj) = value
        .get("rateLimits")
        .or_else(|| value.get("rate_limits"))
        .and_then(|v| v.as_object())
    {
        for key in keys {
            if let Some(window) = rate_limits_obj.get(*key) {
                return Some(window);
            }
        }
    }
    if let Some(limits) = value
        .get("rateLimits")
        .or_else(|| value.get("rate_limits"))
        .and_then(|v| v.as_array())
    {
        for item in limits {
            for key in keys {
                if let Some(window) = item.get(key) {
                    return Some(window);
                }
            }
        }
    }
    None
}

fn extract_percent(value: &Value) -> Option<u8> {
    let raw = value
        .get("used_percent")
        .or_else(|| value.get("usedPercent"))
        .or_else(|| value.get("used_percentage"))?;
    if let Some(v) = raw.as_u64() {
        return Some((v.min(100)) as u8);
    }
    if let Some(v) = raw.as_f64() {
        return Some(v.round().clamp(0.0, 100.0) as u8);
    }
    None
}

fn extract_reset(value: &Value) -> Option<String> {
    value
        .get("reset_at")
        .or_else(|| value.get("resetAt"))
        .or_else(|| value.get("resetsAt"))
        .and_then(|v| {
            if let Some(s) = v.as_str() {
                return Some(s.to_string());
            }
            if let Some(epoch) = v.as_i64() {
                return Some(epoch.to_string());
            }
            if let Some(epoch) = v.as_u64() {
                return Some(epoch.to_string());
            }
            None
        })
}

fn extract_string(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(s) = value.get(key).and_then(|v| v.as_str()) {
            return Some(s.to_string());
        }
    }
    None
}

fn same_path(a: &Path, b: &Path) -> bool {
    fs::canonicalize(a).ok() == fs::canonicalize(b).ok()
}

fn blocked_files() -> Vec<String> {
    vec![
        "auth.json",
        "config.toml",
        "history.jsonl",
        "*.sqlite*",
        "cache/",
        "tmp/",
        "log/",
        "logs/",
        "installation_id",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn should_skip_copy(src: &Path, dst: &Path) -> Result<bool> {
    if !dst.exists() {
        return Ok(false);
    }
    let src_meta = fs::metadata(src)?;
    let dst_meta = fs::metadata(dst)?;
    let same_size = src_meta.len() == dst_meta.len();
    let dst_newer_or_same = dst_meta.modified().ok() >= src_meta.modified().ok();
    Ok(same_size && dst_newer_or_same)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    if !src.exists() {
        return Ok(());
    }
    for file in session_files(src)? {
        let rel = file
            .strip_prefix(src)
            .map_err(|_| AppError::new("PATH_ERROR", "copy path outside source"))?;
        let target = dst.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(file, target)?;
    }
    Ok(())
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

fn timestamp() -> String {
    system_secs(SystemTime::now()).to_string()
}

fn timestamp_yyyymmdd_hhmmss() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

fn seen_blocked_files(home: &Path) -> Result<Vec<String>> {
    let mut seen = Vec::new();
    for name in [
        "auth.json",
        "config.toml",
        "history.jsonl",
        "installation_id",
    ] {
        if home.join(name).exists() {
            seen.push(name.to_string());
        }
    }
    for dir in ["cache", "tmp", "log", "logs"] {
        if home.join(dir).exists() {
            seen.push(format!("{dir}/"));
        }
    }
    collect_matching_blocked(home, home, &mut seen)?;
    seen.sort();
    seen.dedup();
    Ok(seen)
}

fn collect_matching_blocked(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_matching_blocked(root, &path, out)?;
        } else if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            if name.ends_with(".sqlite")
                || name.ends_with(".sqlite-shm")
                || name.ends_with(".sqlite-wal")
                || (name.starts_with("state_") && name.contains(".sqlite"))
            {
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();
                out.push(rel);
            }
        }
    }
    Ok(())
}

fn write_file_private(path: &Path, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(path)?;
    file.write_all(body.as_bytes())?;
    set_file_private(path)?;
    Ok(())
}

fn write_executable(path: &Path, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, body)?;
    set_file_executable(path)?;
    Ok(())
}

#[cfg(unix)]
fn set_dir_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_dir_private(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_file_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_file_private(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_file_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_file_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[allow(dead_code)]
fn sensitive_rel_set() -> HashSet<&'static str> {
    [
        "auth.json",
        "config.toml",
        "history.jsonl",
        "installation_id",
    ]
    .into_iter()
    .collect()
}
