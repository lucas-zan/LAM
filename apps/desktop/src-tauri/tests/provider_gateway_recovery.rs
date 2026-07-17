use localagentmanager_core::gateway::recovery::{
    recover_verified_gateway_process, GatewayProcessControl, GatewayProcessIdentity,
    RecoveryOutcome, SystemGatewayProcessControl,
};
use localagentmanager_core::{AppError, Result};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

#[cfg(target_os = "macos")]
use std::process::Command;

#[cfg(target_os = "macos")]
use std::thread;

struct FakeProcessControl {
    identity: Result<Option<GatewayProcessIdentity>>,
    termination: Result<bool>,
    terminated: Mutex<Vec<u32>>,
}

impl FakeProcessControl {
    fn running(path: &str, uid: u32) -> Self {
        Self {
            identity: Ok(Some(GatewayProcessIdentity {
                executable: PathBuf::from(path),
                uid,
            })),
            termination: Ok(true),
            terminated: Mutex::new(Vec::new()),
        }
    }
}

impl GatewayProcessControl for FakeProcessControl {
    fn inspect(&self, _pid: u32) -> Result<Option<GatewayProcessIdentity>> {
        self.identity.clone()
    }

    fn terminate(&self, pid: u32, _timeout: Duration) -> Result<bool> {
        self.terminated.lock().unwrap().push(pid);
        self.termination.clone()
    }
}

#[test]
fn verified_gateway_process_is_terminated_for_bounded_recovery() {
    let control = FakeProcessControl::running("/trusted/lam-provider-gateway", 501);

    let outcome = recover_verified_gateway_process(
        &control,
        42,
        Path::new("/trusted/lam-provider-gateway"),
        501,
        Duration::from_secs(2),
    )
    .unwrap();

    assert_eq!(outcome, RecoveryOutcome::Terminated);
    assert_eq!(control.terminated.lock().unwrap().as_slice(), &[42]);
}

#[test]
fn verified_gateway_missing_process_needs_no_signal() {
    let control = FakeProcessControl {
        identity: Ok(None),
        termination: Ok(true),
        terminated: Mutex::new(Vec::new()),
    };

    let outcome = recover_verified_gateway_process(
        &control,
        42,
        Path::new("/trusted/lam-provider-gateway"),
        501,
        Duration::from_secs(2),
    )
    .unwrap();

    assert_eq!(outcome, RecoveryOutcome::NotRunning);
    assert!(control.terminated.lock().unwrap().is_empty());
}

#[cfg(target_os = "macos")]
#[test]
fn exited_unreaped_process_is_not_reported_as_running() {
    let mut child = Command::new("/usr/bin/true").spawn().unwrap();
    thread::sleep(Duration::from_millis(100));

    let inspected = GatewayProcessControl::inspect(&SystemGatewayProcessControl, child.id());
    child.wait().unwrap();

    assert_eq!(inspected.unwrap(), None);
}

#[test]
fn verified_gateway_rejects_wrong_uid_without_signaling() {
    let control = FakeProcessControl::running("/trusted/lam-provider-gateway", 502);

    let error = recover_verified_gateway_process(
        &control,
        42,
        Path::new("/trusted/lam-provider-gateway"),
        501,
        Duration::from_secs(2),
    )
    .unwrap_err();

    assert_eq!(error.code, "GATEWAY_PROCESS_OWNER_MISMATCH");
    assert!(control.terminated.lock().unwrap().is_empty());
}

#[test]
fn verified_gateway_rejects_wrong_executable_without_signaling() {
    let control = FakeProcessControl::running("/tmp/unrelated", 501);

    let error = recover_verified_gateway_process(
        &control,
        42,
        Path::new("/trusted/lam-provider-gateway"),
        501,
        Duration::from_secs(2),
    )
    .unwrap_err();

    assert_eq!(error.code, "GATEWAY_PROCESS_EXECUTABLE_MISMATCH");
    assert!(control.terminated.lock().unwrap().is_empty());
}

#[test]
fn verified_gateway_propagates_inspection_and_termination_failures() {
    let inspection = FakeProcessControl {
        identity: Err(AppError::new(
            "GATEWAY_PROCESS_INSPECTION_FAILED",
            "inspect",
        )),
        termination: Ok(true),
        terminated: Mutex::new(Vec::new()),
    };
    let error = recover_verified_gateway_process(
        &inspection,
        42,
        Path::new("/trusted/lam-provider-gateway"),
        501,
        Duration::from_secs(2),
    )
    .unwrap_err();
    assert_eq!(error.code, "GATEWAY_PROCESS_INSPECTION_FAILED");

    let mut timeout = FakeProcessControl::running("/trusted/lam-provider-gateway", 501);
    timeout.termination = Ok(false);
    let error = recover_verified_gateway_process(
        &timeout,
        42,
        Path::new("/trusted/lam-provider-gateway"),
        501,
        Duration::from_secs(2),
    )
    .unwrap_err();
    assert_eq!(error.code, "GATEWAY_PROCESS_TERMINATION_TIMEOUT");
}
