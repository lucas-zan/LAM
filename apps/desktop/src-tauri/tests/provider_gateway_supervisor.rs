use localagentmanager_core::gateway::recovery::{GatewayProcessControl, GatewayProcessIdentity};
use localagentmanager_core::gateway::sidecar::{GatewayRuntimeState, GatewayStateRepository};
use localagentmanager_core::gateway::supervisor::{
    clear_stale_gateway_claim, inspect_gateway_claim, reconcile_gateway_claim,
    validate_gateway_start_preflight, GatewayIdentityProbe, GatewayIdentityProbeResult,
    GatewayReconcileContext, GatewayReconcileOutcome, GatewaySupervisorAction,
    GatewaySupervisorMachine, GatewaySupervisorObservation, GatewaySupervisorState,
    ProcessInspector, ReloadableSource, ReloadingCache,
};
use localagentmanager_core::storage::{InstallationLock, StoreOptions, VersionedFileStore};
use localagentmanager_core::Result;
use std::future::Future;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn repository(root: &Path) -> GatewayStateRepository {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    GatewayStateRepository::new(VersionedFileStore::<GatewayRuntimeState>::new(
        root.join("gateway-state.json"),
        InstallationLock::new(root.join("provider-hub.lock"), Duration::from_secs(1)),
        1,
        StoreOptions::default(),
    ))
}

fn runtime_state(pid: Option<u32>) -> GatewayRuntimeState {
    GatewayRuntimeState {
        state_schema: 1,
        install_id: "install-a".into(),
        instance_id: "instance-a".into(),
        stable_port: 54_321,
        process_id: pid,
        component_version: "gateway-0.2.1".into(),
        control_protocol_version: 1,
        binding_schema: 1,
        restart_count: 0,
        last_control_nonce: 0,
        updated_at: "2026-07-16T00:00:00Z".into(),
    }
}

#[test]
fn transient_identity_failures_are_bounded_and_never_authorize_replacement() {
    let mut machine = GatewaySupervisorMachine::new(3).unwrap();

    assert_eq!(
        machine.observe(GatewaySupervisorObservation::IdentityHealthy),
        GatewaySupervisorAction::Hold
    );
    assert_eq!(machine.state(), GatewaySupervisorState::Healthy);
    assert_eq!(machine.consecutive_failures(), 0);
    assert_eq!(machine.last_transition_reason(), "identity_healthy");

    assert_eq!(
        machine.observe(GatewaySupervisorObservation::IdentityUnavailable),
        GatewaySupervisorAction::Hold
    );
    assert_eq!(machine.state(), GatewaySupervisorState::Suspect);
    assert_eq!(machine.consecutive_failures(), 1);
    assert_eq!(machine.last_transition_reason(), "identity_unavailable");

    assert_eq!(
        machine.observe(GatewaySupervisorObservation::IdentityUnavailable),
        GatewaySupervisorAction::Hold
    );
    assert_eq!(machine.state(), GatewaySupervisorState::Degraded);
    assert_eq!(machine.consecutive_failures(), 2);

    assert_eq!(
        machine.observe(GatewaySupervisorObservation::IdentityUnavailable),
        GatewaySupervisorAction::Hold
    );
    assert_eq!(machine.state(), GatewaySupervisorState::Degraded);
    assert_eq!(machine.consecutive_failures(), 3);

    assert_eq!(
        machine.observe(GatewaySupervisorObservation::IdentityHealthy),
        GatewaySupervisorAction::Hold
    );
    assert_eq!(machine.state(), GatewaySupervisorState::Healthy);
    assert_eq!(machine.consecutive_failures(), 0);
}

#[test]
fn invalid_recovery_threshold_is_rejected() {
    assert_eq!(
        GatewaySupervisorMachine::new(1).unwrap_err().code,
        "GATEWAY_SUPERVISOR_POLICY_INVALID"
    );
}

#[test]
fn authenticated_mismatch_requires_threshold_before_verified_recovery() {
    let mut machine = GatewaySupervisorMachine::new(3).unwrap();

    for expected_state in [
        GatewaySupervisorState::Suspect,
        GatewaySupervisorState::Degraded,
    ] {
        assert_eq!(
            machine.observe(GatewaySupervisorObservation::AuthenticatedIdentityMismatch),
            GatewaySupervisorAction::Hold
        );
        assert_eq!(machine.state(), expected_state);
    }

    assert_eq!(
        machine.observe(GatewaySupervisorObservation::AuthenticatedIdentityMismatch),
        GatewaySupervisorAction::RecoverVerifiedProcess
    );
    assert_eq!(machine.state(), GatewaySupervisorState::Recovering);

    assert_eq!(
        machine.observe(GatewaySupervisorObservation::RecoveryCompleted),
        GatewaySupervisorAction::ValidateStart
    );
    assert_eq!(machine.state(), GatewaySupervisorState::Stopped);
    assert_eq!(
        machine.observe(GatewaySupervisorObservation::StartPreflightPassed),
        GatewaySupervisorAction::StartGateway
    );
    assert_eq!(machine.state(), GatewaySupervisorState::Starting);
}

#[test]
fn missing_and_reused_pid_take_safe_stopped_paths() {
    let mut machine = GatewaySupervisorMachine::new(3).unwrap();
    assert_eq!(
        machine.observe(GatewaySupervisorObservation::ProcessMissing),
        GatewaySupervisorAction::ValidateStart
    );
    assert_eq!(machine.state(), GatewaySupervisorState::Stopped);

    let mut machine = GatewaySupervisorMachine::new(3).unwrap();
    assert_eq!(
        machine.observe(GatewaySupervisorObservation::ProcessIdentityMismatch),
        GatewaySupervisorAction::ClearStaleClaim
    );
    assert_eq!(machine.state(), GatewaySupervisorState::Stopped);
}

struct FakeInspector(Result<Option<GatewayProcessIdentity>>);

impl ProcessInspector for FakeInspector {
    fn inspect(&self, _pid: u32) -> Result<Option<GatewayProcessIdentity>> {
        self.0.clone()
    }
}

struct FakeProbe(GatewayIdentityProbeResult);

impl GatewayIdentityProbe for FakeProbe {
    fn probe<'a>(
        &'a self,
        _expected: &'a GatewayRuntimeState,
    ) -> Pin<Box<dyn Future<Output = Result<GatewayIdentityProbeResult>> + Send + 'a>> {
        let result = self.0.clone();
        Box::pin(async move { Ok(result) })
    }
}

struct FakeProcessControl {
    identity: Option<GatewayProcessIdentity>,
    termination_succeeds: bool,
    terminated: Mutex<Vec<u32>>,
}

impl GatewayProcessControl for FakeProcessControl {
    fn inspect(&self, _pid: u32) -> Result<Option<GatewayProcessIdentity>> {
        Ok(self.identity.clone())
    }

    fn terminate(&self, pid: u32, _timeout: Duration) -> Result<bool> {
        self.terminated.lock().unwrap().push(pid);
        Ok(self.termination_succeeds)
    }
}

#[derive(Clone)]
struct FakeReloadSource {
    state: Arc<Mutex<(u64, usize, bool)>>,
}

impl ReloadableSource for FakeReloadSource {
    type Value = String;
    type Version = u64;

    fn version(&self) -> Result<Self::Version> {
        Ok(self.state.lock().unwrap().0)
    }

    fn load(&self) -> Result<Self::Value> {
        let mut state = self.state.lock().unwrap();
        state.1 += 1;
        if state.2 {
            return Err(localagentmanager_core::AppError::new(
                "GATEWAY_MANIFEST_INVALID",
                "invalid",
            ));
        }
        Ok(format!("verified-{}", state.0))
    }
}

#[test]
fn reloading_cache_verifies_once_per_version_and_preserves_last_good_value_on_failure() {
    let state = Arc::new(Mutex::new((1, 0, false)));
    let mut cache = ReloadingCache::new(FakeReloadSource {
        state: state.clone(),
    });

    assert_eq!(cache.get().unwrap(), "verified-1");
    assert_eq!(cache.get().unwrap(), "verified-1");
    assert_eq!(state.lock().unwrap().1, 1);

    state.lock().unwrap().0 = 2;
    assert_eq!(cache.get().unwrap(), "verified-2");
    assert_eq!(state.lock().unwrap().1, 2);

    {
        let mut state = state.lock().unwrap();
        state.0 = 3;
        state.2 = true;
    }
    assert_eq!(cache.get().unwrap_err().code, "GATEWAY_MANIFEST_INVALID");
    assert_eq!(cache.cached().map(String::as_str), Some("verified-2"));
}

#[tokio::test]
async fn claim_inspection_checks_uid_and_executable_before_gateway_probe() {
    let state = runtime_state(Some(42));
    let expected = Path::new("/trusted/lam-provider-gateway");

    let missing = inspect_gateway_claim(
        &FakeInspector(Ok(None)),
        &FakeProbe(GatewayIdentityProbeResult::Healthy),
        &state,
        expected,
        501,
    )
    .await
    .unwrap();
    assert_eq!(missing, GatewaySupervisorObservation::ProcessMissing);

    let wrong_uid = inspect_gateway_claim(
        &FakeInspector(Ok(Some(GatewayProcessIdentity {
            executable: expected.into(),
            uid: 502,
            parent_pid: 1,
        }))),
        &FakeProbe(GatewayIdentityProbeResult::Healthy),
        &state,
        expected,
        501,
    )
    .await
    .unwrap();
    assert_eq!(
        wrong_uid,
        GatewaySupervisorObservation::ProcessIdentityMismatch
    );

    let wrong_path = inspect_gateway_claim(
        &FakeInspector(Ok(Some(GatewayProcessIdentity {
            executable: PathBuf::from("/tmp/unrelated"),
            uid: 501,
            parent_pid: 1,
        }))),
        &FakeProbe(GatewayIdentityProbeResult::Healthy),
        &state,
        expected,
        501,
    )
    .await
    .unwrap();
    assert_eq!(
        wrong_path,
        GatewaySupervisorObservation::ProcessIdentityMismatch
    );

    let unavailable = inspect_gateway_claim(
        &FakeInspector(Ok(Some(GatewayProcessIdentity {
            executable: expected.into(),
            uid: 501,
            parent_pid: 2,
        }))),
        &FakeProbe(GatewayIdentityProbeResult::Unavailable),
        &state,
        expected,
        501,
    )
    .await
    .unwrap();
    assert_eq!(
        unavailable,
        GatewaySupervisorObservation::IdentityUnavailable
    );
}

#[tokio::test]
async fn process_inspection_errors_are_transient_and_never_authorize_start_or_recovery() {
    let state = runtime_state(Some(42));
    let observation = inspect_gateway_claim(
        &FakeInspector(Err(localagentmanager_core::AppError::new(
            "GATEWAY_PROCESS_INSPECTION_FAILED",
            "redacted",
        ))),
        &FakeProbe(GatewayIdentityProbeResult::Healthy),
        &state,
        Path::new("/trusted/lam-provider-gateway"),
        501,
    )
    .await
    .unwrap();

    assert_eq!(
        observation,
        GatewaySupervisorObservation::IdentityUnavailable
    );
    let mut machine = GatewaySupervisorMachine::new(3).unwrap();
    assert_eq!(machine.observe(observation), GatewaySupervisorAction::Hold);
    assert_eq!(machine.state(), GatewaySupervisorState::Suspect);
}

#[test]
fn reconciliation_terminates_only_after_authenticated_mismatch_reaches_threshold() {
    let root = tempfile::tempdir().unwrap();
    let repo = repository(root.path());
    let port = TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let initialized = repo
        .initialize(0, port, "gateway-0.2.1", 1, 1, "2026-07-16T00:00:00Z")
        .unwrap();
    let claimed = repo
        .claim_process(initialized.revision, 42, false, "2026-07-16T00:00:01Z")
        .unwrap();
    let control = FakeProcessControl {
        identity: Some(GatewayProcessIdentity {
            executable: PathBuf::from("/trusted/lam-provider-gateway"),
            uid: 501,
            parent_pid: 2,
        }),
        termination_succeeds: true,
        terminated: Mutex::new(Vec::new()),
    };
    let mut machine = GatewaySupervisorMachine::new(3).unwrap();
    let control_path = root.path().join("gateway.sock");

    for _ in 0..2 {
        let observation = GatewaySupervisorObservation::AuthenticatedIdentityMismatch;
        let action = machine.observe(observation);
        let outcome = reconcile_gateway_claim(
            action,
            observation,
            &claimed,
            GatewayReconcileContext {
                control: &control,
                state: &repo,
                expected_executable: Path::new("/trusted/lam-provider-gateway"),
                expected_uid: 501,
                control_path: &control_path,
                now: "2026-07-16T00:00:02Z",
                termination_timeout: Duration::from_secs(2),
            },
        )
        .unwrap();
        assert!(matches!(outcome, GatewayReconcileOutcome::Hold));
        assert!(control.terminated.lock().unwrap().is_empty());
    }

    let observation = GatewaySupervisorObservation::AuthenticatedIdentityMismatch;
    let action = machine.observe(observation);
    let outcome = reconcile_gateway_claim(
        action,
        observation,
        &claimed,
        GatewayReconcileContext {
            control: &control,
            state: &repo,
            expected_executable: Path::new("/trusted/lam-provider-gateway"),
            expected_uid: 501,
            control_path: &control_path,
            now: "2026-07-16T00:00:03Z",
            termination_timeout: Duration::from_secs(2),
        },
    )
    .unwrap();

    assert!(matches!(outcome, GatewayReconcileOutcome::ReadyToStart(_)));
    assert_eq!(control.terminated.lock().unwrap().as_slice(), &[42]);
    assert_eq!(repo.load().unwrap().value.process_id, None);
}

#[test]
fn reused_pid_claim_is_cleared_without_signaling_the_foreign_process() {
    let root = tempfile::tempdir().unwrap();
    let repo = repository(root.path());
    let port = TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let initialized = repo
        .initialize(0, port, "gateway-0.2.1", 1, 1, "2026-07-16T00:00:00Z")
        .unwrap();
    let claimed = repo
        .claim_process(initialized.revision, 42, false, "2026-07-16T00:00:01Z")
        .unwrap();
    let control = FakeProcessControl {
        identity: Some(GatewayProcessIdentity {
            executable: PathBuf::from("/tmp/unrelated"),
            uid: 502,
            parent_pid: 1,
        }),
        termination_succeeds: true,
        terminated: Mutex::new(Vec::new()),
    };

    let outcome = reconcile_gateway_claim(
        GatewaySupervisorAction::ClearStaleClaim,
        GatewaySupervisorObservation::ProcessIdentityMismatch,
        &claimed,
        GatewayReconcileContext {
            control: &control,
            state: &repo,
            expected_executable: Path::new("/trusted/lam-provider-gateway"),
            expected_uid: 501,
            control_path: &root.path().join("gateway.sock"),
            now: "2026-07-16T00:00:02Z",
            termination_timeout: Duration::from_secs(2),
        },
    )
    .unwrap();

    assert!(matches!(outcome, GatewayReconcileOutcome::ReadyToStart(_)));
    assert!(control.terminated.lock().unwrap().is_empty());
    assert_eq!(repo.load().unwrap().value.process_id, None);
}

#[test]
fn recovery_timeout_keeps_the_claim_and_never_authorizes_start() {
    let root = tempfile::tempdir().unwrap();
    let repo = repository(root.path());
    let initialized = repo
        .initialize(0, 54_321, "gateway-0.2.1", 1, 1, "2026-07-16T00:00:00Z")
        .unwrap();
    let claimed = repo
        .claim_process(initialized.revision, 42, false, "2026-07-16T00:00:01Z")
        .unwrap();
    let control = FakeProcessControl {
        identity: Some(GatewayProcessIdentity {
            executable: PathBuf::from("/trusted/lam-provider-gateway"),
            uid: 501,
            parent_pid: 2,
        }),
        termination_succeeds: false,
        terminated: Mutex::new(Vec::new()),
    };

    let error = reconcile_gateway_claim(
        GatewaySupervisorAction::RecoverVerifiedProcess,
        GatewaySupervisorObservation::AuthenticatedIdentityMismatch,
        &claimed,
        GatewayReconcileContext {
            control: &control,
            state: &repo,
            expected_executable: Path::new("/trusted/lam-provider-gateway"),
            expected_uid: 501,
            control_path: &root.path().join("gateway.sock"),
            now: "2026-07-16T00:00:02Z",
            termination_timeout: Duration::from_secs(2),
        },
    )
    .unwrap_err();

    assert_eq!(error.code, "GATEWAY_PROCESS_TERMINATION_TIMEOUT");
    assert_eq!(repo.load().unwrap().value.process_id, Some(42));
}

#[test]
fn stale_claim_does_not_bypass_an_occupied_stable_port() {
    let root = tempfile::tempdir().unwrap();
    let repo = repository(root.path());
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let initialized = repo
        .initialize(0, port, "gateway-0.2.1", 1, 1, "2026-07-16T00:00:00Z")
        .unwrap();
    let claimed = repo
        .claim_process(initialized.revision, 42, false, "2026-07-16T00:00:01Z")
        .unwrap();
    let control = FakeProcessControl {
        identity: Some(GatewayProcessIdentity {
            executable: PathBuf::from("/tmp/unrelated"),
            uid: 502,
            parent_pid: 1,
        }),
        termination_succeeds: true,
        terminated: Mutex::new(Vec::new()),
    };

    let error = reconcile_gateway_claim(
        GatewaySupervisorAction::ClearStaleClaim,
        GatewaySupervisorObservation::ProcessIdentityMismatch,
        &claimed,
        GatewayReconcileContext {
            control: &control,
            state: &repo,
            expected_executable: Path::new("/trusted/lam-provider-gateway"),
            expected_uid: 501,
            control_path: &root.path().join("gateway.sock"),
            now: "2026-07-16T00:00:02Z",
            termination_timeout: Duration::from_secs(2),
        },
    )
    .unwrap_err();

    assert_eq!(error.code, "GATEWAY_START_PORT_OCCUPIED");
    assert!(control.terminated.lock().unwrap().is_empty());
}

#[test]
fn stale_claim_is_cleared_by_cas_without_process_termination() {
    let root = tempfile::tempdir().unwrap();
    let repo = repository(root.path());
    let initialized = repo
        .initialize(0, 54_321, "gateway-0.2.1", 1, 1, "2026-07-16T00:00:00Z")
        .unwrap();
    let claimed = repo
        .claim_process(initialized.revision, 42, false, "2026-07-16T00:00:01Z")
        .unwrap();

    let cleared =
        clear_stale_gateway_claim(&repo, claimed.revision, 42, "2026-07-16T00:00:02Z").unwrap();

    assert_eq!(cleared.value.process_id, None);
}

#[test]
fn start_preflight_requires_empty_claim_and_available_stable_port() {
    let root = tempfile::tempdir().unwrap();
    let repo = repository(root.path());
    let initialized = repo
        .initialize(0, 54_321, "gateway-0.2.1", 1, 1, "2026-07-16T00:00:00Z")
        .unwrap();
    let control_path = root.path().join("gateway.sock");

    let claimed = repo
        .claim_process(initialized.revision, 42, false, "2026-07-16T00:00:01Z")
        .unwrap();
    assert_eq!(
        validate_gateway_start_preflight(&repo, claimed.revision, &control_path)
            .unwrap_err()
            .code,
        "GATEWAY_START_STATE_CLAIMED"
    );

    let cleared = repo
        .release_process(claimed.revision, 42, "2026-07-16T00:00:02Z")
        .unwrap();
    let listener = TcpListener::bind(("127.0.0.1", cleared.value.stable_port)).unwrap();
    assert_eq!(
        validate_gateway_start_preflight(&repo, cleared.revision, &control_path)
            .unwrap_err()
            .code,
        "GATEWAY_START_PORT_OCCUPIED"
    );
    drop(listener);

    let reservation =
        validate_gateway_start_preflight(&repo, cleared.revision, &control_path).unwrap();
    assert_eq!(reservation.snapshot().revision, cleared.revision);
    assert_eq!(
        reservation.listener().local_addr().unwrap().port(),
        cleared.value.stable_port
    );
    assert!(TcpListener::bind(("127.0.0.1", cleared.value.stable_port)).is_err());
    drop(reservation);
    TcpListener::bind(("127.0.0.1", cleared.value.stable_port)).unwrap();
}

#[cfg(unix)]
#[test]
fn start_preflight_rejects_a_live_control_socket() {
    use std::os::unix::net::UnixListener;

    let root = tempfile::tempdir().unwrap();
    let repo = repository(root.path());
    let initialized = repo
        .initialize(0, 54_321, "gateway-0.2.1", 1, 1, "2026-07-16T00:00:00Z")
        .unwrap();
    let control_path = root.path().join("gateway.sock");
    let _listener = UnixListener::bind(&control_path).unwrap();

    assert_eq!(
        validate_gateway_start_preflight(&repo, initialized.revision, &control_path)
            .unwrap_err()
            .code,
        "GATEWAY_START_CONTROL_OCCUPIED"
    );
}

#[test]
fn production_supervisor_no_longer_uses_pid_existence_as_health() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(root.join("src/services/gateway/supervisor.rs")).unwrap();

    assert!(source.contains("ProcessInspector"));
    assert!(source.contains("GatewayIdentityProbe"));
    assert!(source.contains("GatewaySupervisorState"));
    assert!(source.contains("ReloadingCache"));
    assert!(source.contains("reconcile_gateway_claim"));
    assert!(source.contains("validate_gateway_start_preflight"));
    assert!(source.contains("configure_listener_handoff"));
    assert!(!source.contains("drop(listener)"));
    assert!(!source.contains("is_some_and(process_exists)"));
    assert!(!source.contains("fn process_exists("));
}

#[tokio::test]
async fn orphaned_gateway_with_parent_pid_one_is_treated_as_identity_mismatch() {
    let state = runtime_state(Some(42));
    let expected = PathBuf::from("/trusted/lam-provider-gateway");
    let orphaned = inspect_gateway_claim(
        &FakeInspector(Ok(Some(GatewayProcessIdentity {
            executable: expected.clone(),
            uid: 501,
            parent_pid: 1,
        }))),
        &FakeProbe(GatewayIdentityProbeResult::Healthy),
        &state,
        &expected,
        501,
    )
    .await
    .unwrap();
    assert_eq!(
        orphaned,
        GatewaySupervisorObservation::ProcessIdentityMismatch
    );

    let alive = inspect_gateway_claim(
        &FakeInspector(Ok(Some(GatewayProcessIdentity {
            executable: expected.clone(),
            uid: 501,
            parent_pid: 2,
        }))),
        &FakeProbe(GatewayIdentityProbeResult::Healthy),
        &state,
        &expected,
        501,
    )
    .await
    .unwrap();
    assert_eq!(alive, GatewaySupervisorObservation::IdentityHealthy);
}
