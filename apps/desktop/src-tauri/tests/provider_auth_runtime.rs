use localagentmanager_core::provider_auth_command::{
    AuthCommandApprovalCollection, AuthCommandApprovalRepository, AuthCommandSpec,
};
use localagentmanager_core::provider_credentials::DirectCodexAuth;
use localagentmanager_core::provider_runtime::{
    materialize_codex_auth, AuthHelperRuntime, ProviderHubPaths,
};
use localagentmanager_core::storage::{InstallationLock, StoreOptions, VersionedFileStore};
use std::fs;
use std::process::Command;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn executable(path: &std::path::Path) {
    fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn canonical_provider_hub_root_migrates_legacy_once_without_split_brain() {
    let home = tempfile::tempdir().unwrap();
    let paths = ProviderHubPaths::for_home(home.path());
    fs::create_dir_all(paths.legacy_root()).unwrap();
    fs::write(paths.legacy_root().join("providers.json"), b"legacy").unwrap();

    let selected = paths.ensure_canonical_root().unwrap();
    assert_eq!(selected, fs::canonicalize(paths.canonical_root()).unwrap());
    assert_eq!(
        fs::read(selected.join("providers.json")).unwrap(),
        b"legacy"
    );
    assert!(!paths.legacy_root().exists());
    assert_eq!(paths.ensure_canonical_root().unwrap(), selected);

    fs::create_dir_all(paths.legacy_root()).unwrap();
    fs::write(paths.legacy_root().join("bindings.json"), b"conflict").unwrap();
    assert_eq!(
        paths.ensure_canonical_root().unwrap_err().code,
        "PROVIDER_HUB_ROOT_CONFLICT"
    );
}

#[test]
fn auth_intents_materialize_to_absolute_executable_and_complete_arguments() {
    let root = tempfile::tempdir().unwrap();
    let helper = root.path().join("bin with space").join("lam-auth-helper");
    fs::create_dir_all(helper.parent().unwrap()).unwrap();
    executable(&helper);
    let state_root = root.path().join("state with space");
    fs::create_dir_all(&state_root).unwrap();
    let canonical_helper = fs::canonicalize(&helper).unwrap();
    let canonical_state_root = fs::canonicalize(&state_root).unwrap();
    let runtime = AuthHelperRuntime {
        executable: helper.clone(),
        state_root: state_root.clone(),
        profile_id: "profile-a".into(),
        gateway_binding_id: Some("binding-a".into()),
    };

    let gateway = materialize_codex_auth(&DirectCodexAuth::Gateway, &runtime).unwrap();
    assert_eq!(
        gateway,
        DirectCodexAuth::AuthCommand {
            approval_id: "gateway-binding:binding-a".into(),
            command: canonical_helper.to_string_lossy().into_owned(),
            args: vec![
                "gateway-token".into(),
                "--state-root".into(),
                canonical_state_root.to_string_lossy().into_owned(),
                "--profile".into(),
                "profile-a".into(),
                "--binding".into(),
                "binding-a".into(),
            ],
        }
    );

    let keychain = materialize_codex_auth(
        &DirectCodexAuth::Keychain {
            service: "lam.remote-provider".into(),
            account: "credential/id/v1".into(),
            version: 1,
        },
        &runtime,
    )
    .unwrap();
    assert!(matches!(
        keychain,
        DirectCodexAuth::AuthCommand { ref command, ref args, .. }
            if command == canonical_helper.to_string_lossy().as_ref()
                && args == &["keychain-token", "--service", "lam.remote-provider", "--account", "credential/id/v1", "--version", "1"]
    ));

    let approved = materialize_codex_auth(
        &DirectCodexAuth::ApprovedCommand {
            approval_id: "approval-a".into(),
        },
        &runtime,
    )
    .unwrap();
    assert!(matches!(
        approved,
        DirectCodexAuth::AuthCommand { ref command, ref args, .. }
            if command == canonical_helper.to_string_lossy().as_ref()
                && args == &["approved-command-token", "--state-root", canonical_state_root.to_string_lossy().as_ref(), "--approval-id", "approval-a"]
    ));
}

#[test]
fn unresolved_or_unsafe_auth_runtime_fails_closed() {
    let root = tempfile::tempdir().unwrap();
    let relative = AuthHelperRuntime {
        executable: "lam-auth-helper".into(),
        state_root: root.path().into(),
        profile_id: "profile-a".into(),
        gateway_binding_id: Some("binding-a".into()),
    };
    assert_eq!(
        materialize_codex_auth(&DirectCodexAuth::Gateway, &relative)
            .unwrap_err()
            .code,
        "AUTH_HELPER_EXECUTABLE_INVALID"
    );

    let helper = root.path().join("helper");
    executable(&helper);
    let missing_binding = AuthHelperRuntime {
        executable: helper,
        state_root: root.path().into(),
        profile_id: "profile-a".into(),
        gateway_binding_id: None,
    };
    assert_eq!(
        materialize_codex_auth(&DirectCodexAuth::Gateway, &missing_binding)
            .unwrap_err()
            .code,
        "GATEWAY_BINDING_NOT_FOUND"
    );
}

#[test]
fn approved_command_helper_executes_persisted_approval_and_prints_only_token() {
    let root = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let repository = AuthCommandApprovalRepository::new(VersionedFileStore::<
        AuthCommandApprovalCollection,
    >::new(
        root.path().join("auth-command-approvals.json"),
        InstallationLock::new(
            root.path().join("provider-hub.lock"),
            Duration::from_secs(1),
        ),
        1,
        StoreOptions::default(),
    ));
    let stored = repository
        .approve(
            0,
            AuthCommandSpec {
                executable: "/usr/bin/printf".into(),
                args: vec!["synthetic-token".into()],
                cwd: None,
                timeout_ms: 1_000,
                max_stdout_bytes: 128,
                cache_ttl_ms: 0,
            },
            &[7_u8; 32],
        )
        .unwrap();
    let approval_id = stored.value.approvals[0].approval_fingerprint();
    assert_eq!(
        repository
            .resolve(approval_id, &[8_u8; 32])
            .unwrap_err()
            .code,
        "AUTH_COMMAND_APPROVAL_TAMPERED"
    );
    let output = Command::new(env!("CARGO_BIN_EXE_lam-auth-helper"))
        .args([
            "approved-command-token",
            "--state-root",
            root.path().to_string_lossy().as_ref(),
            "--approval-id",
            approval_id,
        ])
        .env("LAM_TEST_INSTALL_IDENTITY_KEY", hex::encode([7_u8; 32]))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"synthetic-token\n");
    assert!(output.stderr.is_empty());
}

#[test]
#[ignore = "manual: repairs developer machine bindings after ~/.lam migration"]
fn repair_developer_stale_codex_auth_projections() {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .expect("HOME");
    let repaired =
        localagentmanager_core::repair_stale_codex_auth_projections_service_v2(&home).expect("repair");
    eprintln!("repaired profiles: {repaired:?}");
}
