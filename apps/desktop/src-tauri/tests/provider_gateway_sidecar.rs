use localagentmanager_core::gateway::sidecar::{
    gateway_control_socket_path, AuthenticatedControl, ControlCommand, ControlEnvelope,
    ControlSocketClient, ControlSocketServer, GatewayStateRepository, RestartDecision,
    SupervisorPolicy,
};
use localagentmanager_core::storage::{InstallationLock, StoreOptions, VersionedFileStore};
use std::fs;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn repository(root: &std::path::Path) -> GatewayStateRepository {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700)).unwrap();
    }
    GatewayStateRepository::new(VersionedFileStore::new(
        root.join("gateway-state.json"),
        InstallationLock::new(root.join("provider-hub.lock"), Duration::from_secs(1)),
        1,
        StoreOptions {
            max_bytes: 1024 * 1024,
        },
    ))
}

#[test]
fn gateway_control_socket_fits_the_darwin_unix_path_limit() {
    let temp_root = std::path::Path::new("/var/folders/tx/px7wz0m15psgc4l9s099cysc0000gp/T");
    let path = gateway_control_socket_path(temp_root, "6d4cd8aa-1799-4b47-b682-f2130c5ecf95");

    assert!(path.as_os_str().len() < 104, "{}", path.display());
    assert_eq!(path.parent().unwrap(), temp_root.join("lam"));
}

#[test]
fn first_initialization_persists_install_identity_and_stable_port() {
    let root = tempfile::tempdir().unwrap();
    let repo = repository(root.path());
    let first = repo
        .initialize(0, 54_321, "gateway-0.2.1", 1, 1, "2026-07-13T01:00:00Z")
        .unwrap();
    let replay = repo
        .initialize(
            first.revision,
            60_000,
            "gateway-0.2.1",
            1,
            1,
            "2026-07-13T01:01:00Z",
        )
        .unwrap();
    assert_eq!(first.value.install_id, replay.value.install_id);
    assert_eq!(first.value.instance_id, replay.value.instance_id);
    assert_eq!(replay.value.stable_port, 54_321);
    assert_eq!(replay.revision, first.revision);
    let body = fs::read_to_string(root.path().join("gateway-state.json")).unwrap();
    assert!(!body.contains("0123456789abcdef0123456789abcdef"));
}

#[test]
fn process_claim_is_single_instance_and_stale_pid_recovery_is_explicit() {
    let root = tempfile::tempdir().unwrap();
    let repo = Arc::new(repository(root.path()));
    let initialized = repo
        .initialize(0, 54_321, "gateway-0.2.1", 1, 1, "2026-07-13T01:00:00Z")
        .unwrap();
    let left = repo.clone();
    let right = repo.clone();
    let a = thread::spawn(move || {
        left.claim_process(initialized.revision, 1001, false, "2026-07-13T01:01:00Z")
    });
    let b = thread::spawn(move || {
        right.claim_process(initialized.revision, 1002, false, "2026-07-13T01:01:00Z")
    });
    let outcomes = [a.join().unwrap(), b.join().unwrap()];
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    assert!(outcomes
        .iter()
        .filter_map(|result| result.as_ref().err())
        .any(|error| error.code == "GATEWAY_ALREADY_RUNNING"
            || error.code == "STORE_REVISION_CONFLICT"));

    let current = repo.load().unwrap();
    assert_eq!(
        repo.claim_process(current.revision, 2001, false, "2026-07-13T01:02:00Z")
            .unwrap_err()
            .code,
        "GATEWAY_ALREADY_RUNNING"
    );
    let recovered = repo
        .claim_process(current.revision, 2001, true, "2026-07-13T01:02:00Z")
        .unwrap();
    assert_eq!(recovered.value.process_id, Some(2001));
}

#[test]
fn authenticated_control_rejects_foreign_peer_tamper_replay_and_oversize() {
    let control = AuthenticatedControl::new(b"0123456789abcdef0123456789abcdef", 501).unwrap();
    let envelope = control
        .sign(1, ControlCommand::Status, serde_json::json!({}))
        .unwrap();
    assert_eq!(control.verify_and_advance(501, &envelope, 0).unwrap(), 1);
    assert_eq!(
        control
            .verify_and_advance(502, &envelope, 0)
            .unwrap_err()
            .code,
        "GATEWAY_CONTROL_PEER_FORBIDDEN"
    );
    assert_eq!(
        control
            .verify_and_advance(501, &envelope, 1)
            .unwrap_err()
            .code,
        "GATEWAY_CONTROL_NONCE_REPLAY"
    );
    let mut tampered = envelope.clone();
    tampered.command = ControlCommand::Shutdown;
    assert_eq!(
        control
            .verify_and_advance(501, &tampered, 0)
            .unwrap_err()
            .code,
        "GATEWAY_CONTROL_AUTH_INVALID"
    );

    let oversized = control
        .sign(
            2,
            ControlCommand::Status,
            serde_json::json!({"padding":"x".repeat(70 * 1024)}),
        )
        .unwrap_err();
    assert_eq!(oversized.code, "GATEWAY_CONTROL_MESSAGE_LIMIT");
    assert!(!format!("{control:?}").contains("0123456789abcdef"));
}

#[test]
fn control_codec_is_length_prefixed_canonical_and_rejects_trailing_or_future_version() {
    let control = AuthenticatedControl::new(b"0123456789abcdef0123456789abcdef", 501).unwrap();
    let envelope = control
        .sign(
            9,
            ControlCommand::Readiness,
            serde_json::json!({"nonce":"abc"}),
        )
        .unwrap();
    let encoded = envelope.encode_frame().unwrap();
    assert_eq!(
        u32::from_be_bytes(encoded[..4].try_into().unwrap()) as usize,
        encoded.len() - 4
    );
    assert_eq!(ControlEnvelope::decode_frame(&encoded).unwrap(), envelope);
    let mut trailing = encoded.clone();
    trailing.push(0);
    assert_eq!(
        ControlEnvelope::decode_frame(&trailing).unwrap_err().code,
        "GATEWAY_CONTROL_FRAME_INVALID"
    );
    let mut future = envelope;
    future.protocol_version = 99;
    assert_eq!(
        control
            .verify_and_advance(501, &future, 0)
            .unwrap_err()
            .code,
        "GATEWAY_CONTROL_VERSION_MISMATCH"
    );
}

#[test]
fn supervisor_restart_and_idle_shutdown_are_bounded_and_binding_aware() {
    let policy = SupervisorPolicy::new(
        3,
        Duration::from_millis(100),
        Duration::from_secs(2),
        Duration::from_secs(30),
    )
    .unwrap();
    assert!(
        matches!(policy.restart_decision(1, 7), RestartDecision::RestartAfter(delay) if delay >= Duration::from_millis(100) && delay <= Duration::from_millis(125))
    );
    assert!(
        matches!(policy.restart_decision(3, 7), RestartDecision::RestartAfter(delay) if delay <= Duration::from_millis(500))
    );
    assert_eq!(policy.restart_decision(4, 7), RestartDecision::Failed);
    assert!(!policy.should_idle_shutdown(1, 0, Duration::from_secs(60)));
    assert!(!policy.should_idle_shutdown(0, 1, Duration::from_secs(60)));
    assert!(!policy.should_idle_shutdown(0, 0, Duration::from_secs(29)));
    assert!(policy.should_idle_shutdown(0, 0, Duration::from_secs(30)));
}

#[test]
fn port_change_requires_matching_dry_run_and_never_mutates_silently() {
    let root = tempfile::tempdir().unwrap();
    let repo = repository(root.path());
    let initialized = repo
        .initialize(0, 54_321, "gateway-0.2.1", 1, 1, "2026-07-13T01:00:00Z")
        .unwrap();
    let plan = repo
        .plan_port_change(54_322, &["profile-b".into(), "profile-a".into()])
        .unwrap();
    assert_eq!(plan.profile_ids, ["profile-a", "profile-b"]);
    assert_eq!(
        repo.commit_port_change(
            initialized.revision,
            &plan,
            "wrong-fingerprint",
            "2026-07-13T01:01:00Z"
        )
        .unwrap_err()
        .code,
        "GATEWAY_PORT_PLAN_STALE"
    );
    assert_eq!(repo.load().unwrap().value.stable_port, 54_321);
    let changed = repo
        .commit_port_change(
            initialized.revision,
            &plan,
            &plan.fingerprint,
            "2026-07-13T01:01:00Z",
        )
        .unwrap();
    assert_eq!(changed.value.stable_port, 54_322);
}

#[cfg(unix)]
#[tokio::test]
async fn private_control_socket_checks_peer_uid_mode_hmac_and_nonce() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let socket = root.path().join("gateway-control.sock");
    let uid = unsafe { libc::geteuid() };
    let control =
        Arc::new(AuthenticatedControl::new(b"0123456789abcdef0123456789abcdef", uid).unwrap());
    let server = ControlSocketServer::start(socket.clone(), control.clone())
        .await
        .unwrap();
    assert_eq!(
        fs::metadata(&socket).unwrap().permissions().mode() & 0o777,
        0o600
    );

    let request = control
        .sign(
            1,
            ControlCommand::Readiness,
            serde_json::json!({"challenge":"nonce-a"}),
        )
        .unwrap();
    let response = ControlSocketClient::send(&socket, &control, &request)
        .await
        .unwrap();
    assert_eq!(response.payload["ok"], true);
    assert_eq!(response.payload["acceptedNonce"], 1);

    let replay = ControlSocketClient::send(&socket, &control, &request)
        .await
        .unwrap();
    assert_eq!(replay.payload["ok"], false);
    assert_eq!(replay.payload["code"], "GATEWAY_CONTROL_NONCE_REPLAY");
    server.shutdown().await.unwrap();
    assert!(!socket.exists());
}
