use localagentmanager_core::{
    add_pat_account as core_add_pat_account,
    attach_provider_to_profile as core_attach_provider_to_profile,
    build_login_command as core_build_login_command,
    build_resume_command as core_build_resume_command, check_token_expiration,
    compact_usage_db as core_compact_usage_db, create_account_plan as core_create_account_plan,
    create_provider as core_create_provider, create_relay_plan as core_create_relay_plan,
    delete_provider as core_delete_provider,
    execute_attach_provider_to_profile as core_execute_attach_provider_to_profile,
    execute_create_account as core_execute_create_account,
    execute_create_relay as core_execute_create_relay,
    execute_rename_account as core_execute_rename_account, execute_sync as core_execute_sync,
    export_cpa_credentials as core_export_cpa_credentials,
    get_profile_quota as core_get_profile_quota, get_usage_dashboard as core_get_usage_dashboard,
    get_usage_summary as core_get_usage_summary, list_accounts as core_list_accounts,
    list_cached_accounts as core_list_cached_accounts,
    list_cached_quotas as core_list_cached_quotas, list_providers as core_list_providers,
    list_sessions as core_list_sessions, open_terminal_for_login as core_open_terminal_for_login,
    open_terminal_with_command as core_open_terminal_with_command,
    open_terminal_with_resume as core_open_terminal_with_resume,
    plan_attach_provider_to_profile as core_plan_attach_provider_to_profile,
    process_uploaded_credentials, read_pat_metadata, refresh_all_quotas as core_refresh_all_quotas,
    refresh_usage_index_with_options as core_refresh_usage_index,
    relay_resume_session as core_relay_resume_session,
    rename_account_plan as core_rename_account_plan,
    reset_profile_quota as core_reset_profile_quota, reset_usage_index as core_reset_usage_index,
    resolve_home_root, switch_to_pat_account as core_switch_to_pat_account,
    sync_plan as core_sync_plan, test_provider as core_test_provider,
    update_pat_session_auth as core_update_pat_session_auth,
    update_provider as core_update_provider, AccountNoteUpdate, AddPatAccountRequest,
    AddPatAccountResult, AppError, AttachProviderRequest, AttachProviderResult, AuthMetadata,
    CodexAccount, CodexSession, CpaExport, CreateAccountRequest, CreateProviderRequest,
    CreateRelayRequest, CreateResult, OperationPlan, ProviderProfile, QuotaRefreshResult,
    RelayResumeRequest, RelayResumeResult, RenameAccountRequest, RenameAccountResult,
    ResetQuotaResult, ResumeCommand, ResumeCommandRequest, SyncPlan, SyncRequest, SyncResult,
    TokenExpirationStatus, UpdateProviderRequest, UploadedCredentials, UsageDashboard,
    UsageDashboardRequest, UsageQuotaSnapshot, UsageRefreshResult, UsageSummary,
    UsageSummaryRequest,
};
use std::sync::Mutex;
use tauri::Emitter;

static PENDING_ROUTE: Mutex<Option<String>> = Mutex::new(None);

async fn run_blocking<T, F>(task: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|err| AppError::new("BACKGROUND_TASK_FAILED", err.to_string()))?
}

fn home_root() -> Result<std::path::PathBuf, AppError> {
    resolve_home_root()
}

#[cfg(target_os = "macos")]
const CODEX_APP_PATH: &str = "/Applications/Codex.app";
#[cfg(all(test, target_os = "macos"))]
const CODEX_BUNDLE_PATH_PREFIX: &str = "/Applications/Codex.app/Contents/";
#[cfg(target_os = "macos")]
const CODEX_BUNDLE_PROCESS_PATTERN: &str = "/Applications/Codex[.]app/Contents/";

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
struct WindowBounds {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

#[cfg(target_os = "macos")]
fn parse_window_bounds(output: &str) -> Option<WindowBounds> {
    let nums: Vec<i32> = output
        .split(|c: char| !(c == '-' || c.is_ascii_digit()))
        .filter(|s| !s.is_empty() && *s != "-")
        .filter_map(|s| s.parse().ok())
        .collect();
    if nums.len() < 4 {
        return None;
    }
    Some(WindowBounds {
        x: nums[0],
        y: nums[1],
        width: nums[2],
        height: nums[3],
    })
}

#[cfg(target_os = "macos")]
fn codex_window_bounds() -> Option<WindowBounds> {
    let output = std::process::Command::new("osascript")
        .args([
            "-e",
            r#"tell application "System Events"
if exists process "Codex" then
  tell process "Codex"
    if exists window 1 then return (position of window 1 as list) & (size of window 1 as list)
  end tell
end if
end tell"#,
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_window_bounds(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(target_os = "macos")]
fn restore_codex_window_bounds(bounds: WindowBounds) {
    let script = format!(
        r#"tell application "System Events"
repeat 20 times
  if exists process "Codex" then
    tell process "Codex"
      if exists window 1 then
        set position of window 1 to {{{}, {}}}
        set size of window 1 to {{{}, {}}}
        return
      end if
    end tell
  end if
  delay 0.1
end repeat
end tell"#,
        bounds.x, bounds.y, bounds.width, bounds.height
    );
    let _ = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output();
}

#[cfg(all(test, target_os = "macos"))]
fn codex_bundle_path_matches(path: &str) -> bool {
    path.starts_with(CODEX_BUNDLE_PATH_PREFIX)
}

#[cfg(target_os = "macos")]
fn codex_bundle_processes_running() -> bool {
    std::process::Command::new("pgrep")
        .args(["-f", CODEX_BUNDLE_PROCESS_PATTERN])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn wait_for_codex_exit(timeout: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if !codex_bundle_processes_running() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[cfg(target_os = "macos")]
fn stop_codex_app_processes() -> Result<(), AppError> {
    let _ = std::process::Command::new("osascript")
        .args([
            "-e",
            r#"tell application "System Events"
if exists process "Codex" then
  tell application "Codex" to quit
end if
end tell"#,
        ])
        .output();

    if wait_for_codex_exit(std::time::Duration::from_secs(2)) {
        return Ok(());
    }

    let _ = std::process::Command::new("pkill")
        .args(["-TERM", "-f", CODEX_BUNDLE_PROCESS_PATTERN])
        .output();

    if wait_for_codex_exit(std::time::Duration::from_secs(2)) {
        return Ok(());
    }

    let _ = std::process::Command::new("pkill")
        .args(["-KILL", "-f", CODEX_BUNDLE_PROCESS_PATTERN])
        .output();

    if wait_for_codex_exit(std::time::Duration::from_millis(500)) {
        return Ok(());
    }

    Err(AppError::new(
        "STOP_CODEX_FAILED",
        "Codex processes did not exit",
    ))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    pub ok: bool,
    pub version: String,
    pub home_root: String,
}

#[tauri::command]
pub fn health_check() -> Result<HealthCheck, AppError> {
    let home = home_root()?;
    Ok(HealthCheck {
        ok: true,
        version: env!("CARGO_PKG_VERSION").to_string(),
        home_root: home.to_string_lossy().to_string(),
    })
}

#[tauri::command]
pub async fn list_accounts() -> Result<Vec<CodexAccount>, AppError> {
    let home = home_root()?;
    run_blocking(move || core_list_accounts(&home)).await
}

#[tauri::command]
pub async fn list_cached_accounts() -> Result<Vec<CodexAccount>, AppError> {
    let home = home_root()?;
    run_blocking(move || core_list_cached_accounts(&home)).await
}

#[tauri::command]
pub async fn list_sessions(account_id: String) -> Result<Vec<CodexSession>, AppError> {
    let home = home_root()?;
    run_blocking(move || core_list_sessions(&home, &account_id)).await
}

#[tauri::command]
pub fn plan_create_account(req: CreateAccountRequest) -> Result<OperationPlan, AppError> {
    core_create_account_plan(&home_root()?, &req)
}

#[tauri::command]
pub fn execute_create_account(req: CreateAccountRequest) -> Result<CreateResult, AppError> {
    core_execute_create_account(&home_root()?, &req)
}

#[tauri::command]
pub fn plan_rename_account(req: RenameAccountRequest) -> Result<OperationPlan, AppError> {
    core_rename_account_plan(&home_root()?, &req)
}

#[tauri::command]
pub fn execute_rename_account(req: RenameAccountRequest) -> Result<RenameAccountResult, AppError> {
    core_execute_rename_account(&home_root()?, &req)
}

#[tauri::command]
pub fn update_account_note(req: AccountNoteUpdate) -> Result<CodexAccount, AppError> {
    localagentmanager_core::update_account_note(&home_root()?, &req)
}

#[tauri::command]
pub fn plan_create_relay(req: CreateRelayRequest) -> Result<OperationPlan, AppError> {
    core_create_relay_plan(&home_root()?, &req)
}

#[tauri::command]
pub fn execute_create_relay(req: CreateRelayRequest) -> Result<CreateResult, AppError> {
    core_execute_create_relay(&home_root()?, &req)
}

#[tauri::command]
pub fn build_sync_plan(req: SyncRequest) -> Result<SyncPlan, AppError> {
    core_sync_plan(&home_root()?, &req)
}

#[tauri::command]
pub fn execute_sync(req: SyncRequest) -> Result<SyncResult, AppError> {
    core_execute_sync(&home_root()?, &req)
}

#[tauri::command]
pub fn build_resume_command(req: ResumeCommandRequest) -> Result<ResumeCommand, AppError> {
    core_build_resume_command(&home_root()?, &req)
}

#[tauri::command]
pub fn open_terminal_with_resume(req: ResumeCommandRequest) -> Result<(), AppError> {
    core_open_terminal_with_resume(&home_root()?, &req)
}

#[tauri::command]
pub fn open_terminal_with_command(command: String) -> Result<(), AppError> {
    core_open_terminal_with_command(&command)
}

#[tauri::command]
pub fn relay_resume_session(req: RelayResumeRequest) -> Result<RelayResumeResult, AppError> {
    core_relay_resume_session(&home_root()?, &req)
}

#[tauri::command]
pub fn open_terminal_for_login(profile_id: String) -> Result<(), AppError> {
    core_open_terminal_for_login(&home_root()?, &profile_id)
}

#[tauri::command]
pub fn build_login_command(profile_id: String) -> Result<ResumeCommand, AppError> {
    core_build_login_command(&home_root()?, &profile_id)
}

#[tauri::command]
pub async fn get_profile_quota(
    profile_id: String,
    force_refresh: Option<bool>,
) -> Result<UsageQuotaSnapshot, AppError> {
    let home = home_root()?;
    run_blocking(move || core_get_profile_quota(&home, &profile_id, force_refresh.unwrap_or(false)))
        .await
}

#[tauri::command]
pub async fn refresh_all_quotas(
    profile_ids: Option<Vec<String>>,
) -> Result<QuotaRefreshResult, AppError> {
    let home = home_root()?;
    run_blocking(move || core_refresh_all_quotas(&home, profile_ids)).await
}

#[tauri::command]
pub async fn reset_profile_quota(profile_id: String) -> Result<ResetQuotaResult, AppError> {
    let home = home_root()?;
    run_blocking(move || core_reset_profile_quota(&home, &profile_id)).await
}

#[tauri::command]
pub async fn list_cached_quotas(
    profile_ids: Option<Vec<String>>,
) -> Result<Vec<UsageQuotaSnapshot>, AppError> {
    let home = home_root()?;
    run_blocking(move || core_list_cached_quotas(&home, profile_ids)).await
}

#[tauri::command]
pub async fn refresh_usage_index(include_archived: bool) -> Result<UsageRefreshResult, AppError> {
    let home = home_root()?;
    run_blocking(move || core_refresh_usage_index(&home, include_archived)).await
}

#[tauri::command]
pub async fn get_usage_summary(req: UsageSummaryRequest) -> Result<UsageSummary, AppError> {
    let home = home_root()?;
    run_blocking(move || core_get_usage_summary(&home, req)).await
}

#[tauri::command]
pub async fn get_usage_dashboard(req: UsageDashboardRequest) -> Result<UsageDashboard, AppError> {
    let home = home_root()?;
    run_blocking(move || core_get_usage_dashboard(&home, req)).await
}

#[tauri::command]
pub async fn reset_usage_index() -> Result<(), AppError> {
    let home = home_root()?;
    run_blocking(move || core_reset_usage_index(&home)).await
}

#[tauri::command]
pub async fn compact_usage_db() -> Result<(), AppError> {
    let home = home_root()?;
    run_blocking(move || core_compact_usage_db(&home)).await
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallRawContents {
    pub request: String,
    pub assistant: String,
    pub tool_output: String,
}

#[tauri::command]
pub async fn get_call_raw_contents(
    source_file: String,
    line_number: i64,
) -> Result<CallRawContents, AppError> {
    run_blocking(move || {
        let file = std::fs::File::open(&source_file)
            .map_err(|err| AppError::new("FILE_OPEN_FAILED", err.to_string()))?;
        call_raw_contents_from_reader(std::io::BufReader::new(file), line_number)
    })
    .await
}

fn call_raw_contents_from_reader<R: std::io::BufRead>(
    reader: R,
    line_number: i64,
) -> Result<CallRawContents, AppError> {
    let mut request = Vec::new();
    let mut assistant = Vec::new();
    let mut tool_output = Vec::new();

    for (index, line_res) in reader.lines().enumerate() {
        let current_line = (index + 1) as i64;
        let line = line_res.map_err(|err| AppError::new("FILE_READ_FAILED", err.to_string()))?;
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let entry_type = value
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let payload = value.get("payload").unwrap_or(&serde_json::Value::Null);
        let payload_type = payload
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();

        if entry_type == "turn_context"
            || (entry_type == "event_msg" && payload_type == "token_count")
        {
            if entry_type == "event_msg"
                && payload_type == "token_count"
                && current_line == line_number
            {
                return Ok(CallRawContents {
                    request: request.join("\n\n"),
                    assistant: assistant.join("\n\n"),
                    tool_output: tool_output.join("\n\n"),
                });
            }
            request.clear();
            assistant.clear();
            tool_output.clear();
            continue;
        }

        if entry_type == "response_item" {
            match payload_type {
                "message" => {
                    let text = content_text(payload);
                    let role = payload
                        .get("role")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    if !text.is_empty() && role == "user" {
                        request.push(text);
                    } else if !text.is_empty() && role == "assistant" {
                        assistant.push(text);
                    }
                }
                "function_call" => {
                    let name = payload
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("tool");
                    let args = payload
                        .get("arguments")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    tool_output.push(format!("$ {name}\n{args}"));
                }
                "function_call_output" => {
                    if let Some(output) = payload.get("output").and_then(serde_json::Value::as_str)
                    {
                        tool_output.push(output.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    Ok(CallRawContents {
        request: String::new(),
        assistant: String::new(),
        tool_output: String::new(),
    })
}

fn content_text(payload: &serde_json::Value) -> String {
    payload
        .get("content")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("text").and_then(serde_json::Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

#[tauri::command]
pub fn sync_tray_quota(app: tauri::AppHandle) -> Result<(), AppError> {
    crate::tray::refresh_tray_menu_background(app, false);
    Ok(())
}

#[tauri::command]
pub fn show_main_window(app: tauri::AppHandle) -> Result<(), AppError> {
    crate::tray::show_main_window(&app);
    Ok(())
}

#[tauri::command]
pub fn show_usage_stats(app: tauri::AppHandle) -> Result<(), AppError> {
    crate::tray::show_main_window(&app);
    let mut route = PENDING_ROUTE
        .lock()
        .map_err(|_| AppError::new("PENDING_ROUTE_LOCK", "pending route lock is poisoned"))?;
    *route = Some("usage".to_string());
    app.emit("lam:navigate", "usage")
        .map_err(|err| AppError::new("NAVIGATE_USAGE_FAILED", err.to_string()))?;
    Ok(())
}

#[tauri::command]
pub fn take_pending_route() -> Result<Option<String>, AppError> {
    let mut route = PENDING_ROUTE
        .lock()
        .map_err(|_| AppError::new("PENDING_ROUTE_LOCK", "pending route lock is poisoned"))?;
    Ok(route.take())
}

#[tauri::command]
pub fn set_quota_popover_opacity(app: tauri::AppHandle, percent: u8) -> Result<(), AppError> {
    crate::tray::set_quota_popover_opacity(&app, percent)
}

#[tauri::command]
pub fn hide_quota_popover(app: tauri::AppHandle) -> Result<(), AppError> {
    crate::tray::hide_quota_popover(&app)
}

#[tauri::command]
pub async fn list_providers() -> Result<Vec<ProviderProfile>, AppError> {
    let home = home_root()?;
    run_blocking(move || core_list_providers(&home)).await
}

#[tauri::command]
pub fn create_provider(req: CreateProviderRequest) -> Result<ProviderProfile, AppError> {
    core_create_provider(&home_root()?, &req)
}

#[tauri::command]
pub fn update_provider(req: UpdateProviderRequest) -> Result<ProviderProfile, AppError> {
    core_update_provider(&home_root()?, &req)
}

#[tauri::command]
pub fn delete_provider(provider_id: String) -> Result<bool, AppError> {
    core_delete_provider(&home_root()?, &provider_id)
}

#[tauri::command]
pub fn test_provider(provider_id: String) -> Result<ProviderProfile, AppError> {
    core_test_provider(&home_root()?, &provider_id)
}

#[tauri::command]
pub fn plan_attach_provider_to_profile(
    req: AttachProviderRequest,
) -> Result<OperationPlan, AppError> {
    core_plan_attach_provider_to_profile(&home_root()?, &req)
}

#[tauri::command]
pub fn attach_provider_to_profile(
    req: AttachProviderRequest,
) -> Result<AttachProviderResult, AppError> {
    core_attach_provider_to_profile(&home_root()?, &req)
}

#[tauri::command]
pub fn execute_attach_provider_to_profile(
    req: AttachProviderRequest,
) -> Result<AttachProviderResult, AppError> {
    core_execute_attach_provider_to_profile(&home_root()?, &req)
}

#[tauri::command]
pub async fn get_antigravity_quota(
) -> Result<localagentmanager_core::AntigravityQuotaResponse, AppError> {
    run_blocking(localagentmanager_core::get_live_antigravity_quota).await
}

#[tauri::command]
pub fn upload_pat_credentials(
    profile_id: String,
    uploaded: UploadedCredentials,
) -> Result<(), AppError> {
    process_uploaded_credentials(&home_root()?, &profile_id, &uploaded)
}

#[tauri::command]
pub fn get_pat_metadata(profile_id: String) -> Result<Option<AuthMetadata>, AppError> {
    read_pat_metadata(&home_root()?, &profile_id)
}

#[tauri::command]
pub fn check_profile_token_expiration(
    profile_id: String,
) -> Result<TokenExpirationStatus, AppError> {
    check_token_expiration(&home_root()?, &profile_id)
}

#[tauri::command]
pub fn add_pat_account(req: AddPatAccountRequest) -> Result<AddPatAccountResult, AppError> {
    core_add_pat_account(&home_root()?, &req)
}

#[tauri::command]
pub fn switch_to_pat_account(account_id: String) -> Result<(), AppError> {
    core_switch_to_pat_account(&home_root()?, &account_id)
}

#[tauri::command]
pub fn export_cpa_credentials(profile_id: String) -> Result<CpaExport, AppError> {
    core_export_cpa_credentials(&home_root()?, &profile_id)
}

#[tauri::command]
pub fn update_pat_session_auth(
    profile_id: String,
    auth_json: serde_json::Map<String, serde_json::Value>,
) -> Result<(), AppError> {
    core_update_pat_session_auth(&home_root()?, &profile_id, auth_json)
}

#[tauri::command]
pub fn get_auth_mode() -> Result<String, AppError> {
    localagentmanager_core::types::get_auth_mode(&home_root()?)
}

#[tauri::command]
pub fn set_auth_mode(mode: String) -> Result<(), AppError> {
    localagentmanager_core::types::set_auth_mode(&home_root()?, &mode)
}

#[tauri::command]
pub fn get_hide_dock_icon() -> Result<bool, AppError> {
    Ok(localagentmanager_core::types::get_hide_dock_icon(
        &home_root()?,
    ))
}

#[tauri::command]
pub fn set_hide_dock_icon(app_handle: tauri::AppHandle, hide: bool) -> Result<(), AppError> {
    localagentmanager_core::types::set_hide_dock_icon(&home_root()?, hide)?;

    #[cfg(target_os = "macos")]
    {
        let policy = if hide {
            tauri::ActivationPolicy::Accessory
        } else {
            tauri::ActivationPolicy::Regular
        };
        let _ = app_handle.set_activation_policy(policy);
    }

    Ok(())
}

#[tauri::command]
pub async fn restart_codex() -> Result<(), AppError> {
    run_blocking(|| {
        #[cfg(target_os = "macos")]
        {
            let bounds = codex_window_bounds();
            stop_codex_app_processes()?;

            std::process::Command::new("open")
                .arg(CODEX_APP_PATH)
                .spawn()
                .map_err(|e| AppError::new("RESTART_CODEX_FAILED", e.to_string()))?;

            if let Some(bounds) = bounds {
                restore_codex_window_bounds(bounds);
            }

            Ok(())
        }

        #[cfg(not(target_os = "macos"))]
        {
            Err(AppError::new(
                "RESTART_CODEX_UNSUPPORTED",
                "Codex restart is only supported on macOS",
            ))
        }
    })
    .await
}

#[tauri::command]
pub async fn quit_app(app_handle: tauri::AppHandle) -> Result<(), AppError> {
    app_handle.exit(0);
    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::{
        call_raw_contents_from_reader, codex_bundle_path_matches, parse_window_bounds,
        CODEX_BUNDLE_PATH_PREFIX, CODEX_BUNDLE_PROCESS_PATTERN,
    };
    use serde_json::json;
    use std::io::Cursor;

    #[test]
    fn parses_osascript_window_bounds() {
        let bounds = parse_window_bounds("100, 80, 1280, 820").unwrap();
        assert_eq!(bounds.x, 100);
        assert_eq!(bounds.y, 80);
        assert_eq!(bounds.width, 1280);
        assert_eq!(bounds.height, 820);
    }

    #[test]
    fn codex_process_matcher_covers_bundle_helpers_only() {
        assert_eq!(
            CODEX_BUNDLE_PROCESS_PATTERN,
            "/Applications/Codex[.]app/Contents/"
        );
        assert!(CODEX_BUNDLE_PROCESS_PATTERN.contains("Codex[.]app"));
        assert!(!CODEX_BUNDLE_PROCESS_PATTERN.contains("Codex.app"));

        assert_eq!(
            CODEX_BUNDLE_PATH_PREFIX,
            "/Applications/Codex.app/Contents/"
        );
        assert!(codex_bundle_path_matches(
            "/Applications/Codex.app/Contents/MacOS/Codex"
        ));
        assert!(codex_bundle_path_matches("/Applications/Codex.app/Contents/Frameworks/Codex Framework.framework/Versions/149.0.7827.197/Helpers/browser_crashpad_handler"));
        assert!(codex_bundle_path_matches(
            "/Applications/Codex.app/Contents/Resources/native/bare-modifier-monitor"
        ));
        assert!(!codex_bundle_path_matches("./Codex Computer Use.app/Contents/SharedSupport/SkyComputerUseClient.app/Contents/MacOS/SkyComputerUseClient"));
        assert!(!codex_bundle_path_matches(
            "/Applications/CodexXapp/Contents/MacOS/Codex"
        ));
    }

    #[test]
    fn call_raw_contents_splits_request_assistant_and_tool_output() {
        let body = [
            json!({"type":"turn_context"}).to_string(),
            json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"user request"}]}}).to_string(),
            json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"assistant reply"}]}}).to_string(),
            json!({"type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"date\"}"}}).to_string(),
            json!({"type":"response_item","payload":{"type":"function_call_output","output":"tool result"}}).to_string(),
            json!({"type":"event_msg","payload":{"type":"token_count"}}).to_string(),
        ]
        .join("\n");

        let raw = call_raw_contents_from_reader(Cursor::new(body), 6).unwrap();
        assert_eq!(raw.request, "user request");
        assert_eq!(raw.assistant, "assistant reply");
        assert!(raw.tool_output.contains("exec_command"));
        assert!(raw.tool_output.contains("tool result"));
    }
}
