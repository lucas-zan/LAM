mod commands;
mod tray;

use tauri::{Manager, WindowEvent};

fn should_hide_instead_of_close(label: &str) -> bool {
    label == "main"
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
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
            let home = localagentmanager_core::resolve_home_root()?;
            localagentmanager_core::recover_provider_transactions_service_v2(
                &home,
                chrono::Utc::now().timestamp_millis().max(0) as u64,
            )?;
            localagentmanager_core::migrate_native_responses_bindings_service_v2(
                &home,
                chrono::Utc::now().timestamp_millis().max(0) as u64,
            )?;
            localagentmanager_core::repair_managed_wrappers(&home)?;
            let supervisor_home = home.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) =
                    localagentmanager_core::gateway::supervisor::monitor_packaged_gateway(
                        supervisor_home,
                    )
                    .await
                {
                    eprintln!("{}", error.code);
                }
            });
            tray::setup_tray(app.handle())?;

            #[cfg(target_os = "macos")]
            {
                let hide = localagentmanager_core::types::get_hide_dock_icon(&home);
                if hide {
                    app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                }
            }

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
            commands::delete_account,
            commands::update_account_note,
            commands::plan_create_relay,
            commands::execute_create_relay,
            commands::build_resume_command,
            commands::build_login_command,
            commands::relay_resume_session,
            commands::open_terminal_with_resume,
            commands::open_terminal_with_command,
            commands::open_terminal_for_login,
            commands::list_terminal_targets,
            commands::get_selected_terminal_target,
            commands::set_selected_terminal_target,
            commands::get_profile_quota,
            commands::refresh_all_quotas,
            commands::reset_profile_quota,
            commands::list_cached_quotas,
            commands::refresh_usage_index,
            commands::try_refresh_usage_index,
            commands::refresh_account_usage_snapshot,
            commands::get_usage_summary,
            commands::get_usage_dashboard,
            commands::get_usage_dashboard_response,
            commands::get_usage_scopes,
            commands::get_usage_overview,
            commands::get_usage_activity,
            commands::get_usage_insights,
            commands::get_usage_calls,
            commands::get_usage_threads,
            commands::get_usage_diagnostics,
            commands::get_usage_rate_card,
            commands::reset_usage_index,
            commands::compact_usage_db,
            commands::get_call_raw_contents,
            commands::sync_tray_quota,
            commands::show_main_window,
            commands::show_usage_stats,
            commands::take_pending_route,
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
            commands::list_providers_v2,
            commands::discover_provider_models_v2,
            commands::test_provider_upstream_v2,
            commands::create_provider_v2,
            commands::approve_provider_auth_command_v2,
            commands::list_provider_auth_command_approvals_v2,
            commands::create_provider_with_keychain_v2,
            commands::create_provider_legacy_compat_v2,
            commands::update_provider_v2,
            commands::rotate_provider_credential_v2,
            commands::list_profile_provider_bindings_v2,
            commands::plan_attach_provider_v2,
            commands::execute_attach_provider_v2,
            commands::plan_detach_provider_v2,
            commands::execute_detach_provider_v2,
            commands::plan_api_account_v2,
            commands::execute_api_account_v2,
            commands::plan_api_account_model_switch_v2,
            commands::execute_api_account_model_switch_v2,
            commands::get_api_account_connection_v2,
            commands::update_api_account_connection_v2,
            commands::delete_api_account_v2,
            commands::plan_gateway_port_migration_v2,
            commands::execute_gateway_port_migration_v2,
            commands::get_antigravity_quota,
            commands::upload_pat_credentials,
            commands::get_pat_metadata,
            commands::check_profile_token_expiration,
            commands::add_pat_account,
            commands::add_session_profile_account,
            commands::switch_to_pat_account,
            commands::export_cpa_credentials,
            commands::update_pat_session_auth,
            commands::get_auth_mode,
            commands::set_auth_mode,
            commands::get_gateway_first_response_timeout_seconds,
            commands::set_gateway_first_response_timeout_seconds,
            commands::get_antigravity_port,
            commands::set_antigravity_port,
            commands::get_hide_dock_icon,
            commands::set_hide_dock_icon,
            commands::restart_chatgpt,
            commands::quit_app,
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
