#[test]
fn bulk_session_sync_is_not_exposed_by_the_native_application() {
    let main_source = include_str!("../src/main.rs");
    let commands_source = include_str!("../src/commands/mod.rs");
    let services_source = include_str!("../src/services/mod.rs");

    for removed_command in ["commands::build_sync_plan", "commands::execute_sync"] {
        assert!(
            !main_source.contains(removed_command),
            "removed command is still registered: {removed_command}"
        );
    }

    for removed_symbol in [
        "build_sync_plan",
        "execute_sync",
        "SyncRequest",
        "SyncPlan",
        "SyncResult",
    ] {
        assert!(
            !commands_source.contains(removed_symbol),
            "removed sync contract is still exposed: {removed_symbol}"
        );
    }

    assert!(
        !services_source.contains("pub mod sync;"),
        "bulk sync service is still compiled"
    );
}
