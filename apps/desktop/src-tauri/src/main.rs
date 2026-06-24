mod commands;
mod tray;

use tauri::{Manager, WindowEvent};

fn should_hide_instead_of_close(label: &str) -> bool {
    label == "main"
}

fn main() {
    tauri::Builder::default()
        .on_window_event(|window, event| {
            if should_hide_instead_of_close(window.label()) {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
                return;
            }
            if window.label() != tray::POPOVER_LABEL {
                return;
            }
            if let WindowEvent::Focused(false) = event {
                if window.is_visible().unwrap_or(false) {
                    let _ = tray::hide_quota_popover(window.app_handle());
                }
            }
        })
        .setup(|app| {
            tray::setup_tray(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::health_check,
            commands::list_accounts,
            commands::list_cached_accounts,
            commands::list_sessions,
            commands::plan_create_account,
            commands::execute_create_account,
            commands::plan_rename_account,
            commands::execute_rename_account,
            commands::update_account_note,
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
            commands::get_antigravity_quota,
            commands::upload_pat_credentials,
            commands::get_pat_metadata,
            commands::check_profile_token_expiration,
            commands::add_pat_account,
            commands::switch_to_pat_account,
            commands::get_auth_mode,
            commands::set_auth_mode,
        ])
        .run(tauri::generate_context!())
        .expect("error while running LAM");
}

#[cfg(test)]
mod tests {
    use super::should_hide_instead_of_close;

    #[test]
    fn main_window_close_hides_instead_of_destroying_window() {
        assert!(should_hide_instead_of_close("main"));
        assert!(!should_hide_instead_of_close(crate::tray::POPOVER_LABEL));
    }
}
