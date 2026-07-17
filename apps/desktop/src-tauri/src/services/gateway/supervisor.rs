use super::binding::{binding_requires_gateway, GatewayBindingCollection, GatewayBindingService};
use super::identity::load_or_create_system_install_identity;
use super::launcher::{
    InstallManifest, InstallManifestVerifier, MacCodeSignIdentityVerifier, VerifiedInstallation,
};
use super::recovery::{
    recover_verified_gateway_process, GatewayProcessControl, GatewayProcessIdentity,
    SystemGatewayProcessControl,
};
use super::server::{HealthDocument, HealthProofKey};
use super::sidecar::{
    GatewayRuntimeState, GatewayStateRepository, RestartDecision, SupervisorPolicy,
};
use crate::services::error::{AppError, Result};
use crate::services::provider_keychain::{KeychainCredentialService, SystemKeychainBackend};
use crate::services::provider_runtime::ProviderHubPaths;
use crate::services::storage::{InstallationLock, StoreOptions, StoreSnapshot, VersionedFileStore};
use std::fs;
use std::future::Future;
use std::io::Write;
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewaySupervisorState {
    Healthy,
    Suspect,
    Degraded,
    Recovering,
    Stopped,
    Starting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewaySupervisorObservation {
    IdentityHealthy,
    IdentityUnavailable,
    AuthenticatedIdentityMismatch,
    ProcessMissing,
    ProcessIdentityMismatch,
    RecoveryCompleted,
    StartPreflightPassed,
    StartPreflightBlocked,
    StartCompleted,
    StartFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewaySupervisorAction {
    Hold,
    ClearStaleClaim,
    RecoverVerifiedProcess,
    ValidateStart,
    StartGateway,
}

#[derive(Debug, Clone)]
pub struct GatewaySupervisorMachine {
    state: GatewaySupervisorState,
    consecutive_failures: u32,
    recovery_threshold: u32,
    last_transition_reason: &'static str,
}

impl GatewaySupervisorMachine {
    pub fn new(recovery_threshold: u32) -> Result<Self> {
        if recovery_threshold < 2 {
            return Err(AppError::new(
                "GATEWAY_SUPERVISOR_POLICY_INVALID",
                "Gateway identity recovery threshold must be at least two",
            ));
        }
        Ok(Self {
            state: GatewaySupervisorState::Stopped,
            consecutive_failures: 0,
            recovery_threshold,
            last_transition_reason: "initialized",
        })
    }

    pub fn state(&self) -> GatewaySupervisorState {
        self.state
    }

    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    pub fn last_transition_reason(&self) -> &'static str {
        self.last_transition_reason
    }

    pub fn observe(
        &mut self,
        observation: GatewaySupervisorObservation,
    ) -> GatewaySupervisorAction {
        self.last_transition_reason = observation.reason();
        match observation {
            GatewaySupervisorObservation::IdentityHealthy => {
                self.state = GatewaySupervisorState::Healthy;
                self.consecutive_failures = 0;
                GatewaySupervisorAction::Hold
            }
            GatewaySupervisorObservation::IdentityUnavailable => {
                self.consecutive_failures = self.consecutive_failures.saturating_add(1);
                self.state = if self.consecutive_failures == 1 {
                    GatewaySupervisorState::Suspect
                } else {
                    GatewaySupervisorState::Degraded
                };
                GatewaySupervisorAction::Hold
            }
            GatewaySupervisorObservation::AuthenticatedIdentityMismatch => {
                self.consecutive_failures = self.consecutive_failures.saturating_add(1);
                if self.consecutive_failures >= self.recovery_threshold {
                    self.state = GatewaySupervisorState::Recovering;
                    GatewaySupervisorAction::RecoverVerifiedProcess
                } else {
                    self.state = if self.consecutive_failures == 1 {
                        GatewaySupervisorState::Suspect
                    } else {
                        GatewaySupervisorState::Degraded
                    };
                    GatewaySupervisorAction::Hold
                }
            }
            GatewaySupervisorObservation::ProcessMissing => {
                self.state = GatewaySupervisorState::Stopped;
                self.consecutive_failures = 0;
                GatewaySupervisorAction::ValidateStart
            }
            GatewaySupervisorObservation::ProcessIdentityMismatch => {
                self.state = GatewaySupervisorState::Stopped;
                self.consecutive_failures = 0;
                GatewaySupervisorAction::ClearStaleClaim
            }
            GatewaySupervisorObservation::RecoveryCompleted => {
                self.state = GatewaySupervisorState::Stopped;
                self.consecutive_failures = 0;
                GatewaySupervisorAction::ValidateStart
            }
            GatewaySupervisorObservation::StartPreflightPassed => {
                self.state = GatewaySupervisorState::Starting;
                GatewaySupervisorAction::StartGateway
            }
            GatewaySupervisorObservation::StartPreflightBlocked
            | GatewaySupervisorObservation::StartFailed => {
                self.state = GatewaySupervisorState::Stopped;
                GatewaySupervisorAction::Hold
            }
            GatewaySupervisorObservation::StartCompleted => {
                self.state = GatewaySupervisorState::Starting;
                GatewaySupervisorAction::Hold
            }
        }
    }
}

impl GatewaySupervisorObservation {
    fn reason(self) -> &'static str {
        match self {
            Self::IdentityHealthy => "identity_healthy",
            Self::IdentityUnavailable => "identity_unavailable",
            Self::AuthenticatedIdentityMismatch => "authenticated_identity_mismatch",
            Self::ProcessMissing => "process_missing",
            Self::ProcessIdentityMismatch => "process_identity_mismatch",
            Self::RecoveryCompleted => "recovery_completed",
            Self::StartPreflightPassed => "start_preflight_passed",
            Self::StartPreflightBlocked => "start_preflight_blocked",
            Self::StartCompleted => "start_completed",
            Self::StartFailed => "start_failed",
        }
    }
}

pub trait ReloadableSource {
    type Value;
    type Version: Clone + Eq;

    fn version(&self) -> Result<Self::Version>;
    fn load(&self) -> Result<Self::Value>;
}

pub struct ReloadingCache<S: ReloadableSource> {
    source: S,
    version: Option<S::Version>,
    value: Option<S::Value>,
}

impl<S: ReloadableSource> ReloadingCache<S> {
    pub fn new(source: S) -> Self {
        Self {
            source,
            version: None,
            value: None,
        }
    }

    pub fn get(&mut self) -> Result<&S::Value> {
        let version = self.source.version()?;
        if self.version.as_ref() != Some(&version) {
            let value = self.source.load()?;
            self.value = Some(value);
            self.version = Some(version);
        }
        self.value.as_ref().ok_or_else(|| {
            AppError::new(
                "GATEWAY_INSTALLATION_CACHE_EMPTY",
                "verified Gateway installation cache is empty",
            )
        })
    }

    pub fn cached(&self) -> Option<&S::Value> {
        self.value.as_ref()
    }
}

pub trait ProcessInspector: Send + Sync {
    fn inspect(&self, pid: u32) -> Result<Option<GatewayProcessIdentity>>;
}

impl ProcessInspector for SystemGatewayProcessControl {
    fn inspect(&self, pid: u32) -> Result<Option<GatewayProcessIdentity>> {
        GatewayProcessControl::inspect(self, pid)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayIdentityProbeResult {
    Healthy,
    AuthenticatedMismatch,
    Unavailable,
}

pub trait GatewayIdentityProbe: Send + Sync {
    fn probe<'a>(
        &'a self,
        expected: &'a GatewayRuntimeState,
    ) -> Pin<Box<dyn Future<Output = Result<GatewayIdentityProbeResult>> + Send + 'a>>;
}

pub struct SystemGatewayIdentityProbe {
    client: reqwest::Client,
    gateway_token: String,
    health_key: HealthProofKey,
    timeout: Duration,
}

impl SystemGatewayIdentityProbe {
    pub fn new(gateway_token: String, identity_key: &[u8], timeout: Duration) -> Result<Self> {
        if timeout.is_zero() {
            return Err(AppError::new(
                "GATEWAY_SUPERVISOR_POLICY_INVALID",
                "Gateway identity probe timeout must be positive",
            ));
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| {
                AppError::new(
                    "GATEWAY_IDENTITY_PROBE_FAILED",
                    "Gateway identity probe client could not be created",
                )
            })?;
        Ok(Self {
            client,
            gateway_token,
            health_key: HealthProofKey::new(identity_key)?,
            timeout,
        })
    }
}

impl GatewayIdentityProbe for SystemGatewayIdentityProbe {
    fn probe<'a>(
        &'a self,
        expected: &'a GatewayRuntimeState,
    ) -> Pin<Box<dyn Future<Output = Result<GatewayIdentityProbeResult>> + Send + 'a>> {
        Box::pin(async move {
            let nonce = format!("{:032x}", rand::random::<u128>());
            let request = self
                .client
                .get(format!("http://127.0.0.1:{}/healthz", expected.stable_port))
                .header("authorization", format!("Bearer {}", self.gateway_token))
                .header("x-lam-health-nonce", &nonce)
                .send();
            let response = match tokio::time::timeout(self.timeout, request).await {
                Ok(Ok(response)) => response,
                _ => return Ok(GatewayIdentityProbeResult::Unavailable),
            };
            if !response.status().is_success() {
                return Ok(GatewayIdentityProbeResult::Unavailable);
            }
            let document = match response.json::<HealthDocument>().await {
                Ok(document) => document,
                Err(_) => return Ok(GatewayIdentityProbeResult::Unavailable),
            };
            if !self.health_key.verify(&nonce, &document) {
                return Ok(GatewayIdentityProbeResult::Unavailable);
            }
            if document.install_id == expected.install_id
                && document.instance_id == expected.instance_id
                && document.protocol_version == expected.control_protocol_version
                && document.state_schema == expected.state_schema
                && document.component_version == expected.component_version
                && document.ready
            {
                Ok(GatewayIdentityProbeResult::Healthy)
            } else {
                Ok(GatewayIdentityProbeResult::AuthenticatedMismatch)
            }
        })
    }
}

pub async fn inspect_gateway_claim<I: ProcessInspector, P: GatewayIdentityProbe>(
    inspector: &I,
    probe: &P,
    state: &GatewayRuntimeState,
    expected_executable: &Path,
    expected_uid: u32,
) -> Result<GatewaySupervisorObservation> {
    let Some(pid) = state.process_id else {
        return Ok(GatewaySupervisorObservation::ProcessMissing);
    };
    let identity = match inspector.inspect(pid) {
        Ok(Some(identity)) => identity,
        Ok(None) => return Ok(GatewaySupervisorObservation::ProcessMissing),
        Err(_) => return Ok(GatewaySupervisorObservation::IdentityUnavailable),
    };
    if identity.uid != expected_uid || identity.executable != expected_executable {
        return Ok(GatewaySupervisorObservation::ProcessIdentityMismatch);
    }
    Ok(
        match probe
            .probe(state)
            .await
            .unwrap_or(GatewayIdentityProbeResult::Unavailable)
        {
            GatewayIdentityProbeResult::Healthy => GatewaySupervisorObservation::IdentityHealthy,
            GatewayIdentityProbeResult::AuthenticatedMismatch => {
                GatewaySupervisorObservation::AuthenticatedIdentityMismatch
            }
            GatewayIdentityProbeResult::Unavailable => {
                GatewaySupervisorObservation::IdentityUnavailable
            }
        },
    )
}

pub fn clear_stale_gateway_claim(
    state: &GatewayStateRepository,
    expected_revision: u64,
    pid: u32,
    now: &str,
) -> Result<StoreSnapshot<GatewayRuntimeState>> {
    state.release_process(expected_revision, pid, now)
}

pub fn validate_gateway_start_preflight(
    state: &GatewayStateRepository,
    expected_revision: u64,
    control_path: &Path,
) -> Result<StoreSnapshot<GatewayRuntimeState>> {
    let snapshot = state.load()?;
    if snapshot.revision != expected_revision {
        return Err(AppError::new(
            "STORE_REVISION_CONFLICT",
            "Gateway runtime state changed before start preflight",
        ));
    }
    if snapshot.value.process_id.is_some() {
        return Err(AppError::new(
            "GATEWAY_START_STATE_CLAIMED",
            "Gateway runtime state is still claimed",
        ));
    }
    let listener =
        TcpListener::bind((Ipv4Addr::LOCALHOST, snapshot.value.stable_port)).map_err(|_| {
            AppError::new(
                "GATEWAY_START_PORT_OCCUPIED",
                "Gateway stable port is already occupied",
            )
        })?;
    drop(listener);
    #[cfg(unix)]
    if control_path.exists() && std::os::unix::net::UnixStream::connect(control_path).is_ok() {
        return Err(AppError::new(
            "GATEWAY_START_CONTROL_OCCUPIED",
            "Gateway control socket still has a live listener",
        ));
    }
    Ok(snapshot)
}

#[derive(Debug)]
pub enum GatewayReconcileOutcome {
    Hold,
    ReadyToStart(StoreSnapshot<GatewayRuntimeState>),
}

pub struct GatewayReconcileContext<'a, C> {
    pub control: &'a C,
    pub state: &'a GatewayStateRepository,
    pub expected_executable: &'a Path,
    pub expected_uid: u32,
    pub control_path: &'a Path,
    pub now: &'a str,
    pub termination_timeout: Duration,
}

pub fn reconcile_gateway_claim<C: GatewayProcessControl>(
    action: GatewaySupervisorAction,
    observation: GatewaySupervisorObservation,
    snapshot: &StoreSnapshot<GatewayRuntimeState>,
    context: GatewayReconcileContext<'_, C>,
) -> Result<GatewayReconcileOutcome> {
    match (action, observation) {
        (GatewaySupervisorAction::Hold, _) => Ok(GatewayReconcileOutcome::Hold),
        (
            GatewaySupervisorAction::ClearStaleClaim,
            GatewaySupervisorObservation::ProcessIdentityMismatch,
        ) => clear_claim_and_validate_start(
            context.state,
            snapshot,
            context.control_path,
            context.now,
        ),
        (GatewaySupervisorAction::ValidateStart, GatewaySupervisorObservation::ProcessMissing) => {
            clear_claim_and_validate_start(
                context.state,
                snapshot,
                context.control_path,
                context.now,
            )
        }
        (
            GatewaySupervisorAction::RecoverVerifiedProcess,
            GatewaySupervisorObservation::AuthenticatedIdentityMismatch,
        ) => {
            let pid = required_claimed_pid(snapshot)?;
            recover_verified_gateway_process(
                context.control,
                pid,
                context.expected_executable,
                context.expected_uid,
                context.termination_timeout,
            )?;
            clear_claim_and_validate_start(
                context.state,
                snapshot,
                context.control_path,
                context.now,
            )
        }
        _ => Err(AppError::new(
            "GATEWAY_SUPERVISOR_TRANSITION_INVALID",
            "Gateway Supervisor action does not match its observation",
        )),
    }
}

fn clear_claim_and_validate_start(
    state: &GatewayStateRepository,
    snapshot: &StoreSnapshot<GatewayRuntimeState>,
    control_path: &Path,
    now: &str,
) -> Result<GatewayReconcileOutcome> {
    let current = match snapshot.value.process_id {
        Some(pid) => clear_stale_gateway_claim(state, snapshot.revision, pid, now)?,
        None => snapshot.clone(),
    };
    let ready = validate_gateway_start_preflight(state, current.revision, control_path)?;
    Ok(GatewayReconcileOutcome::ReadyToStart(ready))
}

fn required_claimed_pid(snapshot: &StoreSnapshot<GatewayRuntimeState>) -> Result<u32> {
    snapshot.value.process_id.ok_or_else(|| {
        AppError::new(
            "GATEWAY_PROCESS_OWNERSHIP_MISMATCH",
            "Gateway recovery requires a claimed process",
        )
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ManifestVersion {
    modified: SystemTime,
    bytes: u64,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

struct VerifiedInstallationSource {
    install_root: PathBuf,
    manifest_path: PathBuf,
}

impl ReloadableSource for VerifiedInstallationSource {
    type Value = Arc<VerifiedInstallation>;
    type Version = ManifestVersion;

    fn version(&self) -> Result<Self::Version> {
        let metadata = fs::symlink_metadata(&self.manifest_path)
            .map_err(|_| manifest_error("GATEWAY_MANIFEST_MISSING"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(manifest_error("GATEWAY_MANIFEST_INVALID"));
        }
        Ok(ManifestVersion {
            modified: metadata
                .modified()
                .map_err(|_| manifest_error("GATEWAY_MANIFEST_INVALID"))?,
            bytes: metadata.len(),
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
        })
    }

    fn load(&self) -> Result<Self::Value> {
        let bytes = fs::read(&self.manifest_path)
            .map_err(|_| manifest_error("GATEWAY_MANIFEST_MISSING"))?;
        let manifest: InstallManifest = serde_json::from_slice(&bytes)
            .map_err(|_| manifest_error("GATEWAY_MANIFEST_INVALID"))?;
        let installation = InstallManifestVerifier::new(
            self.install_root.clone(),
            Arc::new(MacCodeSignIdentityVerifier),
        )
        .verify(manifest)?;
        Ok(Arc::new(installation))
    }
}

fn manifest_error(code: &str) -> AppError {
    AppError::new(
        code,
        "Gateway installation manifest is unavailable or invalid",
    )
}

pub async fn monitor_packaged_gateway(home_root: PathBuf) -> Result<()> {
    PackagedGatewaySupervisor::new(home_root)?.run().await
}

type SystemBindingService = GatewayBindingService<SystemKeychainBackend>;
type InstallationCache = ReloadingCache<VerifiedInstallationSource>;

struct PackagedGatewaySupervisor {
    home_root: PathBuf,
    root: PathBuf,
    state: GatewayStateRepository,
    bindings: SystemBindingService,
    policy: SupervisorPolicy,
    installation: InstallationCache,
    process_control: SystemGatewayProcessControl,
    machine: GatewaySupervisorMachine,
    failures: u32,
    expected_uid: u32,
}

impl PackagedGatewaySupervisor {
    fn new(home_root: PathBuf) -> Result<Self> {
        let root = ProviderHubPaths::for_home(&home_root).ensure_canonical_root()?;
        let lock = InstallationLock::new(root.join("provider-hub.lock"), Duration::from_secs(5));
        let state = gateway_state_repository(&root, lock.clone());
        let bindings = system_binding_service(&root, lock);
        let policy = SupervisorPolicy::new(
            3,
            Duration::from_millis(100),
            Duration::from_secs(2),
            Duration::from_secs(30),
        )?;
        let (install_root, manifest_path) = install_paths()?;
        Ok(Self {
            home_root,
            root,
            state,
            bindings,
            policy,
            installation: ReloadingCache::new(VerifiedInstallationSource {
                install_root,
                manifest_path,
            }),
            process_control: SystemGatewayProcessControl,
            machine: GatewaySupervisorMachine::new(3)?,
            failures: 0,
            expected_uid: unsafe { libc::geteuid() },
        })
    }

    async fn run(mut self) -> Result<()> {
        loop {
            let delay = self.cycle().await?;
            tokio::time::sleep(delay).await;
        }
    }

    async fn cycle(&mut self) -> Result<Duration> {
        let active = self.active_bindings()?;
        if active.is_empty() {
            self.failures = 0;
            return Ok(Duration::from_secs(5));
        }
        let snapshot = self.state.load()?;
        let installation = match self.installation.get() {
            Ok(installation) => Arc::clone(installation),
            Err(error) => return self.failure_delay(error),
        };
        let executable = installation.component("gateway")?.path.clone();
        let control_path = packaged_control_path(&snapshot.value)?;
        let outcome = self
            .reconcile(&active[0], &snapshot, &executable, &control_path)
            .await;
        match outcome {
            Ok(GatewayReconcileOutcome::Hold) => self.hold_delay(),
            Ok(GatewayReconcileOutcome::ReadyToStart(ready)) => {
                self.start(ready, &active, installation, &control_path)
            }
            Err(error) => {
                self.transition(GatewaySupervisorObservation::StartPreflightBlocked);
                self.failure_delay(error)
            }
        }
    }

    fn active_bindings(&self) -> Result<Vec<super::binding::GatewayBinding>> {
        Ok(self
            .bindings
            .load()?
            .value
            .bindings
            .into_iter()
            .filter(|binding| binding_requires_gateway(binding))
            .collect())
    }

    async fn reconcile(
        &mut self,
        binding: &super::binding::GatewayBinding,
        snapshot: &StoreSnapshot<GatewayRuntimeState>,
        executable: &Path,
        control_path: &Path,
    ) -> Result<GatewayReconcileOutcome> {
        let observation = inspect_packaged_gateway(
            &self.bindings,
            binding,
            &self.process_control,
            &snapshot.value,
            executable,
            self.expected_uid,
        )
        .await;
        let action = self.transition(observation);
        let now = chrono::Utc::now().to_rfc3339();
        reconcile_gateway_claim(
            action,
            observation,
            snapshot,
            GatewayReconcileContext {
                control: &self.process_control,
                state: &self.state,
                expected_executable: executable,
                expected_uid: self.expected_uid,
                control_path,
                now: &now,
                termination_timeout: Duration::from_secs(2),
            },
        )
    }

    fn start(
        &mut self,
        ready: StoreSnapshot<GatewayRuntimeState>,
        active: &[super::binding::GatewayBinding],
        installation: Arc<VerifiedInstallation>,
        control_path: &Path,
    ) -> Result<Duration> {
        if self.transition(GatewaySupervisorObservation::StartPreflightPassed)
            != GatewaySupervisorAction::StartGateway
        {
            return Err(invalid_transition());
        }
        let timeout =
            crate::services::types::gateway_first_response_timeout_seconds(&self.home_root);
        match start_packaged_sidecar(
            &self.home_root,
            &self.root,
            &ready.value,
            active,
            timeout,
            &installation,
            control_path,
        ) {
            Ok(()) => {
                self.transition(GatewaySupervisorObservation::StartCompleted);
                self.failures = 0;
                Ok(Duration::from_secs(1))
            }
            Err(error) => {
                self.transition(GatewaySupervisorObservation::StartFailed);
                self.failure_delay(error)
            }
        }
    }

    fn hold_delay(&mut self) -> Result<Duration> {
        if self.machine.state() == GatewaySupervisorState::Healthy {
            self.failures = 0;
        }
        Ok(Duration::from_secs(5))
    }

    fn failure_delay(&mut self, error: AppError) -> Result<Duration> {
        self.failures = self.failures.saturating_add(1);
        match self.policy.restart_decision(self.failures, rand::random()) {
            RestartDecision::RestartAfter(delay) => Ok(delay),
            RestartDecision::Failed => Err(AppError::new("GATEWAY_HEALTH_FAILED", error.code)),
        }
    }

    fn transition(&mut self, observation: GatewaySupervisorObservation) -> GatewaySupervisorAction {
        let action = self.machine.observe(observation);
        log_supervisor_transition(&self.machine);
        action
    }
}

fn gateway_state_repository(root: &Path, lock: InstallationLock) -> GatewayStateRepository {
    GatewayStateRepository::new(VersionedFileStore::<GatewayRuntimeState>::new(
        root.join("gateway-state.json"),
        lock,
        1,
        StoreOptions::default(),
    ))
}

fn system_binding_service(root: &Path, lock: InstallationLock) -> SystemBindingService {
    GatewayBindingService::new(
        VersionedFileStore::<GatewayBindingCollection>::new(
            root.join("gateway-bindings.json"),
            lock,
            1,
            StoreOptions::default(),
        ),
        KeychainCredentialService::new(Arc::new(SystemKeychainBackend)),
    )
}

fn invalid_transition() -> AppError {
    AppError::new(
        "GATEWAY_SUPERVISOR_TRANSITION_INVALID",
        "Gateway Supervisor did not authorize the requested transition",
    )
}

async fn inspect_packaged_gateway<B: crate::services::provider_keychain::KeychainBackend>(
    bindings: &GatewayBindingService<B>,
    binding: &super::binding::GatewayBinding,
    inspector: &SystemGatewayProcessControl,
    state: &GatewayRuntimeState,
    expected_executable: &Path,
    expected_uid: u32,
) -> GatewaySupervisorObservation {
    let identity = load_or_create_system_install_identity(&state.install_id);
    let token = bindings.token_for_helper(&binding.profile_id, &binding.binding_id);
    let probe = match (token, identity) {
        (Ok(token), Ok(identity)) => {
            SystemGatewayIdentityProbe::new(token, &identity, Duration::from_secs(2))
        }
        _ => {
            return inspect_without_gateway_probe(
                inspector,
                state,
                expected_executable,
                expected_uid,
            )
        }
    };
    let Ok(probe) = probe else {
        return GatewaySupervisorObservation::IdentityUnavailable;
    };
    inspect_gateway_claim(inspector, &probe, state, expected_executable, expected_uid)
        .await
        .unwrap_or(GatewaySupervisorObservation::IdentityUnavailable)
}

fn inspect_without_gateway_probe<I: ProcessInspector>(
    inspector: &I,
    state: &GatewayRuntimeState,
    expected_executable: &Path,
    expected_uid: u32,
) -> GatewaySupervisorObservation {
    let Some(pid) = state.process_id else {
        return GatewaySupervisorObservation::ProcessMissing;
    };
    match inspector.inspect(pid) {
        Ok(None) => GatewaySupervisorObservation::ProcessMissing,
        Ok(Some(identity))
            if identity.uid != expected_uid || identity.executable != expected_executable =>
        {
            GatewaySupervisorObservation::ProcessIdentityMismatch
        }
        _ => GatewaySupervisorObservation::IdentityUnavailable,
    }
}

fn log_supervisor_transition(machine: &GatewaySupervisorMachine) {
    eprintln!(
        "gateway_supervisor state={:?} failures={} reason={}",
        machine.state(),
        machine.consecutive_failures(),
        machine.last_transition_reason()
    );
}

fn start_packaged_sidecar(
    home_root: &Path,
    root: &Path,
    state: &GatewayRuntimeState,
    bindings: &[super::binding::GatewayBinding],
    first_response_timeout_seconds: u64,
    installation: &VerifiedInstallation,
    control_path: &Path,
) -> Result<()> {
    let identity = load_or_create_system_install_identity(&state.install_id)?;
    let control_parent = control_path.parent().ok_or_else(|| {
        AppError::new(
            "GATEWAY_CONTROL_PATH_UNSAFE",
            "Gateway control path has no parent",
        )
    })?;
    fs::create_dir_all(&control_parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&control_parent, fs::Permissions::from_mode(0o700))?;
    }
    let mut command = Command::new(&installation.component("gateway")?.path);
    command
        .env_clear()
        .env("LAM_PROVIDER_HUB_ROOT", root)
        .env("LAM_GATEWAY_CONTROL_SOCKET", control_path)
        .env(
            crate::services::provider_runtime::GATEWAY_FIRST_RESPONSE_TIMEOUT_ENV,
            first_response_timeout_seconds.to_string(),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    let model_catalog = home_root.join(".codex").join("models_cache.json");
    if model_catalog.is_file() {
        command.env(
            crate::services::provider_runtime::CODEX_MODEL_CATALOG_ENV,
            model_catalog,
        );
    }
    for binding in bindings {
        let source = match &binding.provider.upstream_auth {
            crate::services::provider_credentials::UpstreamAuth::Bearer { source }
            | crate::services::provider_credentials::UpstreamAuth::Header { source, .. } => {
                Some(source)
            }
            crate::services::provider_credentials::UpstreamAuth::None => None,
        };
        if let Some(crate::services::provider_credentials::CredentialSource::Env { env_key }) =
            source
        {
            let value = std::env::var_os(env_key).ok_or_else(|| {
                AppError::new(
                    "PROVIDER_CREDENTIAL_MISSING",
                    "Provider environment credential is unavailable",
                )
            })?;
            command.env(env_key, value);
        }
    }
    let mut child = command.spawn().map_err(|_| {
        AppError::new(
            "GATEWAY_START_FAILED",
            "Gateway sidecar could not be launched",
        )
    })?;
    child
        .stdin
        .take()
        .ok_or_else(|| AppError::new("GATEWAY_BOOTSTRAP_FAILED", "bootstrap pipe is unavailable"))?
        .write_all(&identity)
        .map_err(|_| AppError::new("GATEWAY_BOOTSTRAP_FAILED", "bootstrap delivery failed"))?;
    Ok(())
}

fn packaged_control_path(state: &GatewayRuntimeState) -> Result<PathBuf> {
    let temp_root = std::env::var_os("DARWIN_USER_TEMP_DIR")
        .or_else(|| std::env::var_os("TMPDIR"))
        .map(PathBuf::from)
        .ok_or_else(|| {
            AppError::new(
                "GATEWAY_CONTROL_PATH_UNSAFE",
                "temporary directory is unavailable",
            )
        })?;
    Ok(super::sidecar::gateway_control_socket_path(
        &temp_root,
        &state.install_id,
    ))
}

fn install_paths() -> Result<(PathBuf, PathBuf)> {
    if let (Some(root), Some(manifest)) = (
        std::env::var_os("LAM_INSTALL_ROOT"),
        std::env::var_os("LAM_INSTALL_MANIFEST"),
    ) {
        return Ok((PathBuf::from(root), PathBuf::from(manifest)));
    }
    let executable = std::env::current_exe().map_err(|_| {
        AppError::new(
            "GATEWAY_INSTALL_ROOT_INVALID",
            "application executable is unavailable",
        )
    })?;
    let contents = executable.parent().and_then(Path::parent).ok_or_else(|| {
        AppError::new(
            "GATEWAY_INSTALL_ROOT_INVALID",
            "app bundle layout is invalid",
        )
    })?;
    Ok((
        contents.into(),
        contents.join("Resources/provider-gateway-install-manifest.json"),
    ))
}
