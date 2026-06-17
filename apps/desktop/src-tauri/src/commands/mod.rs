use localagentmanager_core::{
    attach_provider_to_profile as core_attach_provider_to_profile,
    build_login_command as core_build_login_command,
    build_resume_command as core_build_resume_command,
    create_account_plan as core_create_account_plan, create_provider as core_create_provider,
    create_relay_plan as core_create_relay_plan, delete_provider as core_delete_provider,
    execute_attach_provider_to_profile as core_execute_attach_provider_to_profile,
    execute_create_account as core_execute_create_account,
    execute_create_relay as core_execute_create_relay,
    execute_rename_account as core_execute_rename_account, execute_sync as core_execute_sync,
    get_profile_quota as core_get_profile_quota, list_accounts as core_list_accounts,
    list_cached_accounts as core_list_cached_accounts,
    list_cached_quotas as core_list_cached_quotas, list_providers as core_list_providers,
    list_sessions as core_list_sessions, open_terminal_for_login as core_open_terminal_for_login,
    open_terminal_with_command as core_open_terminal_with_command,
    open_terminal_with_resume as core_open_terminal_with_resume,
    plan_attach_provider_to_profile as core_plan_attach_provider_to_profile,
    refresh_all_quotas as core_refresh_all_quotas,
    relay_resume_session as core_relay_resume_session,
    rename_account_plan as core_rename_account_plan, resolve_home_root,
    sync_plan as core_sync_plan, test_provider as core_test_provider,
    update_provider as core_update_provider, AppError, AttachProviderRequest, AttachProviderResult,
    CodexAccount, CodexSession, CreateAccountRequest, CreateProviderRequest, CreateRelayRequest,
    CreateResult, OperationPlan, ProviderProfile, QuotaRefreshResult, RelayResumeRequest,
    RelayResumeResult, RenameAccountRequest, RenameAccountResult, ResumeCommand,
    ResumeCommandRequest, SyncPlan, SyncRequest, SyncResult, UpdateProviderRequest,
    UsageQuotaSnapshot,
};

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
pub async fn list_cached_quotas(
    profile_ids: Option<Vec<String>>,
) -> Result<Vec<UsageQuotaSnapshot>, AppError> {
    let home = home_root()?;
    run_blocking(move || core_list_cached_quotas(&home, profile_ids)).await
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
