use localagentmanager_core::provider_api_v2::*;
use localagentmanager_core::provider_binding::{
    ExistingConfigBinding, ProfileBindingCollection, ProfileBindingRepository,
};
use localagentmanager_core::provider_credentials::{CredentialSource, SecretValue};
use localagentmanager_core::provider_v2::ProviderCollection;
use localagentmanager_core::storage::{InstallationLock, StoreOptions, VersionedFileStore};
use localagentmanager_core::{
    delete_account, execute_create_account, rename_account_plan, AppError, CreateAccountRequest,
    DeleteAccountRequest, RenameAccountRequest, Result,
};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

struct FixedResolver(Option<&'static str>);

impl ProviderCredentialResolver for FixedResolver {
    fn resolve(&self, _source: &CredentialSource) -> Result<SecretValue> {
        self.0
            .map(|value| SecretValue::from_sensitive(value.into()))
            .ok_or_else(|| AppError::new("PROVIDER_CREDENTIAL_MISSING", "missing"))
    }
}

fn request(expected_revision: u64) -> CreateProviderRequestV2 {
    serde_json::from_value(serde_json::json!({
        "expectedRevision": expected_revision,
        "provider": {
            "id": "production-provider",
            "name": "Production Provider",
            "protocol": "responses",
            "baseUrl": "https://api.example.test/v1",
            "defaultModel": "model-a",
            "models": [{"id":"model-a","label":"Model A"}],
            "upstreamAuth": {
                "kind":"bearer",
                "credential":{"kind":"env","envKey":"PRODUCTION_TOKEN"}
            },
            "adapter":{"kind":"none"},
            "codex":{"queryParams":{},"envHttpHeaders":{}}
        }
    }))
    .unwrap()
}

#[test]
fn upstream_test_performs_bounded_authenticated_http_and_missing_credential_blocks_attach() {
    let home = tempfile::tempdir().unwrap();
    create_provider_service_v2(home.path(), request(0), "2026-07-14T00:00:00Z").unwrap();
    let initial_view = list_provider_views_service_v2(home.path())
        .unwrap()
        .remove(0);
    assert!(!initial_view.readiness.ready);
    assert_eq!(initial_view.readiness.binding_count, 0);
    assert!(initial_view
        .readiness
        .blockers
        .iter()
        .any(|value| value == "PROVIDER_CREDENTIAL_MISSING"));
    let provider_root =
        localagentmanager_core::provider_runtime::ProviderHubPaths::for_home(home.path())
            .ensure_canonical_root()
            .unwrap();
    let store = VersionedFileStore::<ProviderCollection>::new(
        provider_root.join("providers.json"),
        InstallationLock::new(
            provider_root.join("provider-hub.lock"),
            Duration::from_secs(1),
        ),
        1,
        StoreOptions::default(),
    );

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let captured = Arc::new(Mutex::new(String::new()));
    let thread_capture = captured.clone();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = [0_u8; 4096];
        let count = stream.read(&mut bytes).unwrap();
        *thread_capture.lock().unwrap() = String::from_utf8_lossy(&bytes[..count]).into_owned();
        let body = r#"{"object":"list","data":[{"id":"model-a","object":"model"}]}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });

    let mut snapshot = store.load_or_default().unwrap();
    snapshot.value.providers[0].base_url = format!("http://{address}/v1");
    store
        .compare_and_swap(snapshot.revision, &snapshot.value)
        .unwrap();

    let result = test_provider_upstream_service_v2_with_resolver(
        home.path(),
        "production-provider",
        &FixedResolver(Some("synthetic-bearer")),
    )
    .unwrap();
    assert!(result.ok);
    let observed = list_provider_views_service_v2(home.path())
        .unwrap()
        .remove(0);
    assert!(observed.last_health.as_ref().is_some_and(|item| item.ok));
    server.join().unwrap();
    let request = captured.lock().unwrap().to_ascii_lowercase();
    assert!(request.starts_with("get /v1/models "));
    assert!(request.contains("authorization: bearer synthetic-bearer"));

    let config_root = home.path().join("profile");
    fs::create_dir(&config_root).unwrap();
    let config = config_root.join("config.toml");
    fs::write(&config, "").unwrap();
    let mut state = ProviderApiV2State::default();
    let plan = plan_attach_service_v2_with_resolver(
        home.path(),
        &config,
        PlanAttachRequestV2 {
            profile_id: "profile-a".into(),
            provider_id: "production-provider".into(),
            selected_model: "model-a".into(),
        },
        &mut state,
        1_000,
        &FixedResolver(None),
        true,
    )
    .unwrap();
    assert!(plan
        .blockers
        .iter()
        .any(|value| value == "credential_missing"));
}

#[test]
fn app_and_launcher_bootstrap_use_the_production_recovery_service_idempotently() {
    let home = tempfile::tempdir().unwrap();
    assert_eq!(
        recover_provider_transactions_service_v2(home.path(), 1_000)
            .unwrap()
            .recovered,
        0
    );
    assert_eq!(
        recover_provider_transactions_service_v2(home.path(), 2_000)
            .unwrap()
            .recovered,
        0
    );

    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let app = fs::read_to_string(manifest_dir.join("src/main.rs")).unwrap();
    let launcher = fs::read_to_string(manifest_dir.join("src/bin/lam.rs")).unwrap();
    assert!(app.contains("recover_provider_transactions_service_v2"));
    assert!(launcher.contains("recover_provider_transactions_at_root_service_v2"));
}

#[test]
fn attached_profile_and_provider_mutations_fail_before_any_filesystem_write() {
    let home = tempfile::tempdir().unwrap();
    execute_create_account(
        home.path(),
        &CreateAccountRequest {
            name: "bound-profile".into(),
            copy_config_from: None,
            overwrite_wrapper: false,
        },
    )
    .unwrap();
    create_provider_service_v2(home.path(), request(0), "2026-07-14T00:00:00Z").unwrap();
    let provider_root =
        localagentmanager_core::provider_runtime::ProviderHubPaths::for_home(home.path())
            .ensure_canonical_root()
            .unwrap();
    let binding_repository =
        ProfileBindingRepository::new(VersionedFileStore::<ProfileBindingCollection>::new(
            provider_root.join("bindings.json"),
            InstallationLock::new(
                provider_root.join("provider-hub.lock"),
                Duration::from_secs(1),
            ),
            1,
            StoreOptions::default(),
        ));
    let config = home.path().join(".codex-bound-profile/config.toml");
    fs::write(&config, "model = \"model-a\"\n").unwrap();
    let config_before = fs::read(&config).unwrap();
    binding_repository
        .adopt(
            0,
            "bound-profile",
            ExistingConfigBinding {
                provider_id: "production-provider".into(),
                selected_model: "model-a".into(),
                config_path: config.to_string_lossy().into_owned(),
                config_hash: localagentmanager_core::provider_config_editor::config_hash(
                    &config_before,
                ),
                auth_supported: true,
                manageable: true,
                ambiguous: false,
            },
            1,
            "fingerprint",
            "2026-07-14T00:00:00Z",
        )
        .unwrap();

    let rename_error = rename_account_plan(
        home.path(),
        &RenameAccountRequest {
            from_profile_id: "bound-profile".into(),
            to_name: "renamed-profile".into(),
            overwrite_wrapper: false,
        },
    )
    .unwrap_err();
    assert_eq!(rename_error.code, "PROFILE_HAS_PROVIDER_BINDING");
    assert!(home.path().join(".codex-bound-profile").exists());
    assert!(!home.path().join(".codex-renamed-profile").exists());

    let delete_error = delete_account(
        home.path(),
        &DeleteAccountRequest {
            profile_id: "bound-profile".into(),
        },
    )
    .unwrap_err();
    assert_eq!(delete_error.code, "PROFILE_HAS_PROVIDER_BINDING");
    assert!(home.path().join(".codex-bound-profile").exists());
    assert_eq!(fs::read(&config).unwrap(), config_before);

    let update: UpdateProviderRequestV2 = serde_json::from_value(serde_json::json!({
        "expectedRevision": 1,
        "provider": {
            "id": "production-provider",
            "name": "Changed",
            "protocol": "responses",
            "baseUrl": "https://api.example.test/v1",
            "defaultModel": "model-a",
            "models": [{"id":"model-a","label":"Model A"}],
            "upstreamAuth": {"kind":"none"},
            "adapter":{"kind":"none"},
            "codex":{"queryParams":{},"envHttpHeaders":{}}
        }
    }))
    .unwrap();
    assert_eq!(
        update_provider_service_v2(home.path(), update, "2026-07-14T01:00:00Z")
            .unwrap_err()
            .code,
        "PROVIDER_HAS_BINDINGS_REBIND_REQUIRED"
    );
    assert_eq!(
        list_provider_views_service_v2(home.path()).unwrap()[0].name,
        "Production Provider"
    );
}
