use localagentmanager_core::provider_api_v2::*;

#[test]
fn probe_cmd_connection() {
    let home = std::path::Path::new("/Users/zhanhd");
    match get_api_account_connection_service_v2(home, "cmd") {
        Ok(view) => println!(
            "OK: profile={} provider={} protocol={:?} model={}",
            view.profile_id, view.provider_id, view.protocol, view.selected_model
        ),
        Err(e) => println!("ERR: code={} msg={}", e.code, e.message),
    }
}
