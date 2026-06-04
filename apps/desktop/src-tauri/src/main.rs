mod commands;
mod tray;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            tray::setup_tray(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::health_check,
            commands::list_accounts,
            commands::list_sessions,
            commands::plan_create_account,
            commands::execute_create_account,
            commands::plan_create_relay,
            commands::execute_create_relay,
            commands::build_sync_plan,
            commands::execute_sync,
            commands::build_resume_command,
            commands::build_login_command,
            commands::relay_resume_session,
            commands::open_terminal_with_resume,
            commands::open_terminal_with_command,
            commands::open_terminal_for_login,
            commands::get_profile_quota,
            commands::refresh_all_quotas,
            commands::list_cached_quotas,
            commands::sync_tray_quota,
            commands::show_main_window,
            commands::set_quota_popover_opacity,
            commands::hide_quota_popover,
            commands::list_providers,
            commands::create_provider,
            commands::update_provider,
            commands::delete_provider,
            commands::test_provider,
            commands::plan_attach_provider_to_profile,
            commands::attach_provider_to_profile,
            commands::execute_attach_provider_to_profile,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LocalAgentManager");
}
