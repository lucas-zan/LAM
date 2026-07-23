use localagentmanager_core::gateway::sidecar::{
    gateway_control_socket_name, GatewayRuntimeState, GatewayStateRepository, RestartDecision,
    SupervisorPolicy,
};
use localagentmanager_core::provider_api_v2::{
    execute_gateway_port_migration_service_v2, execute_gateway_port_migration_with_fault_v2,
    plan_gateway_port_migration_service_v2,
};
use localagentmanager_core::provider_binding::{
    ManagedConfigProjection, ProfileBindingCollection, ProfileProviderBinding, ProjectionOwnership,
    RouteKind,
};
use localagentmanager_core::provider_config_editor::config_hash;
use localagentmanager_core::provider_runtime::ProviderHubPaths;
use localagentmanager_core::storage::{InstallationLock, StoreOptions, VersionedFileStore};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[test]
fn production_binaries_wire_bounded_restart_idle_and_dual_stream_budgets() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let launcher = fs::read_to_string(root.join("src/bin/lam.rs")).unwrap();
    let sidecar = fs::read_to_string(root.join("src/bin/lam-provider-gateway.rs")).unwrap();
    let routes = fs::read_to_string(root.join("src/services/gateway/routes.rs")).unwrap();
    let desktop = fs::read_to_string(root.join("src/main.rs")).unwrap();
    let supervisor = fs::read_to_string(root.join("src/services/gateway/supervisor.rs")).unwrap();

    assert!(launcher.contains("restart_decision"));
    assert!(launcher.contains("child.try_wait"));
    assert!(launcher.contains("child.kill"));
    assert!(launcher.contains("inspect_gateway_claim"));
    assert!(launcher.contains("reconcile_gateway_claim"));
    assert!(launcher.contains("owned_child: Mutex<Option<Child>>"));
    assert!(launcher.contains("fn shutdown(&self)"));
    assert!(launcher.contains("self.owned_child.lock"));
    assert!(!launcher.contains(
        "if state.value.process_id.is_some_and(process_exists) {\n            return Err(identity_mismatch());"
    ));
    assert!(sidecar.contains("should_idle_shutdown"));
    assert!(sidecar.contains("activity.inflight_requests"));
    assert!(sidecar.contains("binding_requires_gateway"));
    assert!(routes.contains("MAX_EVENT_CHANNEL_CAPACITY"));
    assert!(routes.contains("MAX_EVENT_CHANNEL_BYTES"));
    assert!(routes.contains("acquire_many_owned"));
    assert!(desktop.contains("monitor_packaged_gateway"));
    assert!(desktop.contains("migrate_native_responses_bindings_service_v2"));
    assert!(launcher.contains("migrate_native_responses_bindings_service_v2"));
    assert!(supervisor.contains("binding_requires_gateway"));
    assert!(supervisor.contains("InstallManifestVerifier"));
    assert!(supervisor.contains("restart_decision"));
    assert!(supervisor.contains("configure_listener_handoff"));
    assert!(launcher.contains("configure_listener_handoff"));
    assert!(sidecar.contains("take_inherited_gateway_listener"));
    assert!(sidecar.contains("start_with_listener"));
    assert!(launcher.contains("GATEWAY_FIRST_RESPONSE_TIMEOUT_ENV"));
    assert!(supervisor.contains("GATEWAY_FIRST_RESPONSE_TIMEOUT_ENV"));
    assert!(sidecar.contains("gateway_first_response_timeout_from_env"));
    assert!(launcher.contains("CODEX_MODEL_CATALOG_ENV"));
    assert!(supervisor.contains("CODEX_MODEL_CATALOG_ENV"));
    assert!(sidecar.contains("CodexModelDefaultsCatalog::from_path"));
    assert!(sidecar.contains("CodexModelDefaultsCatalog::builtin"));
    assert!(sidecar.contains("overlay"));
    assert!(!sidecar.contains("first_byte_timeout: Duration::from_secs(10)"));
}

#[test]
fn production_supervisor_policy_has_a_finite_budget() {
    let policy = SupervisorPolicy::new(
        3,
        Duration::from_millis(100),
        Duration::from_secs(2),
        Duration::from_secs(30),
    )
    .unwrap();
    assert!(matches!(
        policy.restart_decision(1, 0),
        RestartDecision::RestartAfter(_)
    ));
    assert!(matches!(
        policy.restart_decision(3, 0),
        RestartDecision::RestartAfter(_)
    ));
    assert_eq!(policy.restart_decision(4, 0), RestartDecision::Failed);
}

#[test]
fn control_socket_name_is_bounded_and_stable_for_long_temp_roots() {
    let name = gateway_control_socket_name("8f20c875-3b58-4e7e-95ce-7b56ba7a68f7");
    assert_eq!(
        name,
        gateway_control_socket_name("8f20c875-3b58-4e7e-95ce-7b56ba7a68f7")
    );
    assert!(name.len() <= 29);
    assert_ne!(name, gateway_control_socket_name("another-installation"));
}

fn gateway_profile_binding(
    profile_id: &str,
    config_path: &Path,
    port: u16,
) -> ProfileProviderBinding {
    let contents = fs::read(config_path).unwrap();
    let base_url = format!("http://127.0.0.1:{port}/v1");
    let mut binding = ProfileProviderBinding::new_for_test(
        profile_id,
        &format!("provider-{profile_id}"),
        "model-a",
        ManagedConfigProjection {
            config_path: config_path.to_string_lossy().into_owned(),
            ownership: ProjectionOwnership::Managed,
            before_hash: config_hash(&contents),
            applied_hash: config_hash(&contents),
            previous_values: BTreeMap::new(),
            managed_values: BTreeMap::from([
                (
                    "model_provider".into(),
                    format!("\"provider-{profile_id}\""),
                ),
                ("base_url".into(), format!("\"{base_url}\"")),
            ]),
            provider_table_created_by_lam: true,
        },
        1,
        "provider-fingerprint",
    );
    binding.route_kind = RouteKind::Gateway;
    binding.gateway_binding_id = Some(format!("gateway-{profile_id}"));
    binding
}

#[test]
fn stable_port_migration_updates_every_profile_and_rolls_back_partial_failure() {
    let home = tempfile::tempdir().unwrap();
    let hub = ProviderHubPaths::for_home(home.path());
    let root = hub.ensure_canonical_root().unwrap();
    let lock = InstallationLock::new(root.join("provider-hub.lock"), Duration::from_secs(1));
    let state = GatewayStateRepository::new(VersionedFileStore::<GatewayRuntimeState>::new(
        root.join("gateway-state.json"),
        lock.clone(),
        1,
        StoreOptions::default(),
    ));
    state
        .initialize(0, 54_321, "0.2.1", 1, 1, "2026-07-14T01:00:00Z")
        .unwrap();

    let configs = [root.join("profile-a.toml"), root.join("profile-b.toml")];
    for (profile, path) in ["a", "b"].iter().zip(&configs) {
        fs::write(
            path,
            format!(
                "model = \"model-a\"\nmodel_provider = \"provider-{profile}\"\n\n[model_providers.provider-{profile}]\nbase_url = \"http://127.0.0.1:54321/v1\"\n"
            ),
        )
        .unwrap();
    }
    let bindings_store = VersionedFileStore::<ProfileBindingCollection>::new(
        root.join("bindings.json"),
        lock,
        1,
        StoreOptions::default(),
    );
    let original_bindings = ProfileBindingCollection {
        bindings: vec![
            gateway_profile_binding("a", &configs[0], 54_321),
            gateway_profile_binding("b", &configs[1], 54_321),
        ],
    };
    bindings_store
        .compare_and_swap(0, &original_bindings)
        .unwrap();
    let original_configs = configs
        .iter()
        .map(|path| fs::read(path).unwrap())
        .collect::<Vec<_>>();

    let plan = plan_gateway_port_migration_service_v2(home.path(), 54_322).unwrap();
    let error = execute_gateway_port_migration_with_fault_v2(
        home.path(),
        &plan,
        &plan.fingerprint,
        "2026-07-14T01:01:00Z",
        Some(1),
    )
    .unwrap_err();
    assert_eq!(error.code, "GATEWAY_PORT_MIGRATION_FAULT");
    assert_eq!(
        configs
            .iter()
            .map(|path| fs::read(path).unwrap())
            .collect::<Vec<_>>(),
        original_configs
    );
    assert_eq!(
        bindings_store.load_or_default().unwrap().value,
        original_bindings
    );
    assert_eq!(state.load().unwrap().value.stable_port, 54_321);

    let outcome = execute_gateway_port_migration_service_v2(
        home.path(),
        &plan,
        &plan.fingerprint,
        "2026-07-14T01:02:00Z",
    )
    .unwrap();
    assert_eq!(outcome.migrated_profiles, ["a", "b"]);
    assert_eq!(state.load().unwrap().value.stable_port, 54_322);
    for path in configs {
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("http://127.0.0.1:54322/v1"));
        assert!(!contents.contains("http://127.0.0.1:54321/v1"));
    }
    let committed = bindings_store.load_or_default().unwrap().value;
    for binding in committed.bindings {
        assert_eq!(
            binding.config_projection.applied_hash,
            config_hash(&fs::read(binding.config_projection.config_path).unwrap())
        );
    }
}
