use crate::services::error::{AppError, Result};
use crate::services::storage::{StoreSnapshot, VersionedFileStore};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
#[cfg(unix)]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};
#[cfg(unix)]
use tokio::sync::oneshot;
#[cfg(unix)]
use tokio::task::JoinHandle;

pub const CONTROL_PROTOCOL_VERSION: u32 = 1;
pub const MAX_CONTROL_MESSAGE_BYTES: usize = 64 * 1024;

/// Keep the socket basename independent of the installation UUID length. Darwin's
/// AF_UNIX path limit is small enough that a normal per-user temp path plus a UUID
/// can otherwise make an otherwise valid installation impossible to start.
pub fn gateway_control_socket_name(install_id: &str) -> String {
    format!(
        "gateway-{}.sock",
        hex::encode(&Sha256::digest(install_id.as_bytes())[..8])
    )
}

pub fn gateway_control_socket_path(temp_root: &Path, install_id: &str) -> PathBuf {
    temp_root
        .join("lam")
        .join(gateway_control_socket_name(install_id))
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GatewayRuntimeState {
    pub state_schema: u32,
    pub install_id: String,
    pub instance_id: String,
    pub stable_port: u16,
    pub process_id: Option<u32>,
    pub component_version: String,
    pub control_protocol_version: u32,
    pub binding_schema: u32,
    pub restart_count: u32,
    pub last_control_nonce: u64,
    pub updated_at: String,
}

#[derive(Clone)]
pub struct GatewayStateRepository {
    store: VersionedFileStore<GatewayRuntimeState>,
}

impl GatewayStateRepository {
    pub fn new(store: VersionedFileStore<GatewayRuntimeState>) -> Self {
        Self { store }
    }

    pub fn load(&self) -> Result<StoreSnapshot<GatewayRuntimeState>> {
        self.store.load_or_default()
    }

    pub fn initialize(
        &self,
        expected_revision: u64,
        stable_port: u16,
        component_version: &str,
        control_protocol_version: u32,
        binding_schema: u32,
        now: &str,
    ) -> Result<StoreSnapshot<GatewayRuntimeState>> {
        validate_timestamp(now)?;
        validate_port(stable_port)?;
        let current = self.load()?;
        if current.exists {
            ensure_compatible(
                &current.value,
                component_version,
                control_protocol_version,
                binding_schema,
            )?;
            return Ok(current);
        }
        if current.revision != expected_revision {
            return Err(revision_conflict());
        }
        self.store.compare_and_swap(
            expected_revision,
            &GatewayRuntimeState {
                state_schema: 1,
                install_id: Uuid::new_v4().to_string(),
                instance_id: Uuid::new_v4().to_string(),
                stable_port,
                process_id: None,
                component_version: component_version.into(),
                control_protocol_version,
                binding_schema,
                restart_count: 0,
                last_control_nonce: 0,
                updated_at: now.into(),
            },
        )
    }

    pub fn claim_process(
        &self,
        expected_revision: u64,
        process_id: u32,
        stale_confirmed: bool,
        now: &str,
    ) -> Result<StoreSnapshot<GatewayRuntimeState>> {
        validate_timestamp(now)?;
        if process_id == 0 {
            return Err(AppError::new(
                "GATEWAY_PROCESS_ID_INVALID",
                "Gateway process id must be non-zero",
            ));
        }
        let mut current = self.load()?;
        if current.revision != expected_revision {
            return Err(revision_conflict());
        }
        if current.value.process_id.is_some() && !stale_confirmed {
            return Err(AppError::new(
                "GATEWAY_ALREADY_RUNNING",
                "Gateway state is already claimed by a process",
            ));
        }
        if stale_confirmed {
            current.value.instance_id = Uuid::new_v4().to_string();
        }
        current.value.process_id = Some(process_id);
        current.value.updated_at = now.into();
        self.store
            .compare_and_swap(expected_revision, &current.value)
    }

    pub fn release_process(
        &self,
        expected_revision: u64,
        process_id: u32,
        now: &str,
    ) -> Result<StoreSnapshot<GatewayRuntimeState>> {
        validate_timestamp(now)?;
        let mut current = self.load()?;
        if current.revision != expected_revision {
            return Err(revision_conflict());
        }
        if current.value.process_id != Some(process_id) {
            return Err(AppError::new(
                "GATEWAY_PROCESS_OWNERSHIP_MISMATCH",
                "Gateway process does not own runtime state",
            ));
        }
        current.value.process_id = None;
        current.value.updated_at = now.into();
        self.store
            .compare_and_swap(expected_revision, &current.value)
    }

    pub fn plan_port_change(
        &self,
        new_port: u16,
        profile_ids: &[String],
    ) -> Result<GatewayPortChangePlan> {
        validate_port(new_port)?;
        let current = self.load()?;
        if !current.exists {
            return Err(AppError::new(
                "GATEWAY_STATE_NOT_INITIALIZED",
                "Gateway state is not initialized",
            ));
        }
        if new_port == current.value.stable_port {
            return Err(AppError::new(
                "GATEWAY_PORT_UNCHANGED",
                "new Gateway port matches the stable port",
            ));
        }
        let mut profile_ids = profile_ids.to_vec();
        profile_ids.sort();
        profile_ids.dedup();
        let mut plan = GatewayPortChangePlan {
            expected_state_revision: current.revision,
            old_port: current.value.stable_port,
            new_port,
            profile_ids,
            fingerprint: String::new(),
        };
        plan.fingerprint = port_plan_fingerprint(&plan);
        Ok(plan)
    }

    pub fn commit_port_change(
        &self,
        expected_revision: u64,
        plan: &GatewayPortChangePlan,
        fingerprint: &str,
        now: &str,
    ) -> Result<StoreSnapshot<GatewayRuntimeState>> {
        validate_timestamp(now)?;
        if plan.fingerprint != fingerprint || port_plan_fingerprint(plan) != fingerprint {
            return Err(AppError::new(
                "GATEWAY_PORT_PLAN_STALE",
                "Gateway port change plan fingerprint is stale",
            ));
        }
        let mut current = self.load()?;
        if current.revision != expected_revision
            || plan.expected_state_revision != expected_revision
            || current.value.stable_port != plan.old_port
        {
            return Err(AppError::new(
                "GATEWAY_PORT_PLAN_STALE",
                "Gateway port change inputs changed",
            ));
        }
        current.value.stable_port = plan.new_port;
        current.value.updated_at = now.into();
        self.store
            .compare_and_swap(expected_revision, &current.value)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GatewayPortChangePlan {
    pub expected_state_revision: u64,
    pub old_port: u16,
    pub new_port: u16,
    pub profile_ids: Vec<String>,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ControlCommand {
    Status,
    Readiness,
    Shutdown,
    Drain,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ControlEnvelope {
    pub protocol_version: u32,
    pub request_nonce: u64,
    pub command: ControlCommand,
    pub payload: Value,
    pub mac: String,
}

impl ControlEnvelope {
    pub fn encode_frame(&self) -> Result<Vec<u8>> {
        let body = serde_json::to_vec(self).map_err(|_| control_frame_invalid())?;
        if body.len() > MAX_CONTROL_MESSAGE_BYTES {
            return Err(control_message_limit());
        }
        let length = u32::try_from(body.len()).map_err(|_| control_message_limit())?;
        let mut frame = Vec::with_capacity(4 + body.len());
        frame.extend_from_slice(&length.to_be_bytes());
        frame.extend_from_slice(&body);
        Ok(frame)
    }

    pub fn decode_frame(frame: &[u8]) -> Result<Self> {
        if frame.len() < 4 {
            return Err(control_frame_invalid());
        }
        let length = u32::from_be_bytes(frame[..4].try_into().map_err(|_| control_frame_invalid())?)
            as usize;
        if length > MAX_CONTROL_MESSAGE_BYTES || frame.len() != length + 4 {
            return Err(control_frame_invalid());
        }
        serde_json::from_slice(&frame[4..]).map_err(|_| control_frame_invalid())
    }
}

#[derive(Clone)]
pub struct AuthenticatedControl {
    key: [u8; 32],
    expected_uid: u32,
}

impl fmt::Debug for AuthenticatedControl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthenticatedControl")
            .field("key", &"[REDACTED]")
            .field("expected_uid", &self.expected_uid)
            .finish()
    }
}

impl AuthenticatedControl {
    pub fn new(key: &[u8], expected_uid: u32) -> Result<Self> {
        if key.len() != 32 {
            return Err(AppError::new(
                "GATEWAY_IDENTITY_KEY_INVALID",
                "Gateway control key must contain 256 bits",
            ));
        }
        let mut value = [0_u8; 32];
        value.copy_from_slice(key);
        Ok(Self {
            key: value,
            expected_uid,
        })
    }

    pub fn sign(
        &self,
        request_nonce: u64,
        command: ControlCommand,
        payload: Value,
    ) -> Result<ControlEnvelope> {
        let mut envelope = ControlEnvelope {
            protocol_version: CONTROL_PROTOCOL_VERSION,
            request_nonce,
            command,
            payload,
            mac: String::new(),
        };
        let payload = control_payload(&envelope)?;
        if payload.len() > MAX_CONTROL_MESSAGE_BYTES.saturating_sub(128) {
            return Err(control_message_limit());
        }
        envelope.mac = hex::encode(hmac_sha256(&self.key, &payload));
        if serde_json::to_vec(&envelope)
            .map_err(|_| control_frame_invalid())?
            .len()
            > MAX_CONTROL_MESSAGE_BYTES
        {
            return Err(control_message_limit());
        }
        Ok(envelope)
    }

    pub fn verify_and_advance(
        &self,
        peer_uid: u32,
        envelope: &ControlEnvelope,
        last_nonce: u64,
    ) -> Result<u64> {
        if peer_uid != self.expected_uid {
            return Err(AppError::new(
                "GATEWAY_CONTROL_PEER_FORBIDDEN",
                "Gateway control peer uid is not allowed",
            ));
        }
        if envelope.protocol_version != CONTROL_PROTOCOL_VERSION {
            return Err(AppError::new(
                "GATEWAY_CONTROL_VERSION_MISMATCH",
                "Gateway control protocol version is incompatible",
            ));
        }
        if envelope.request_nonce <= last_nonce {
            return Err(AppError::new(
                "GATEWAY_CONTROL_NONCE_REPLAY",
                "Gateway control request nonce was already used",
            ));
        }
        let provided = hex::decode(&envelope.mac).map_err(|_| control_auth_invalid())?;
        let expected = hmac_sha256(&self.key, &control_payload(envelope)?);
        if !constant_time_bytes(&expected, &provided) {
            return Err(control_auth_invalid());
        }
        Ok(envelope.request_nonce)
    }

    fn verify_response(
        &self,
        peer_uid: u32,
        envelope: &ControlEnvelope,
        expected_nonce: u64,
    ) -> Result<()> {
        if peer_uid != self.expected_uid {
            return Err(AppError::new(
                "GATEWAY_CONTROL_PEER_FORBIDDEN",
                "Gateway control peer uid is not allowed",
            ));
        }
        if envelope.protocol_version != CONTROL_PROTOCOL_VERSION
            || envelope.request_nonce != expected_nonce
        {
            return Err(AppError::new(
                "GATEWAY_CONTROL_VERSION_MISMATCH",
                "Gateway control response identity is incompatible",
            ));
        }
        let provided = hex::decode(&envelope.mac).map_err(|_| control_auth_invalid())?;
        let expected = hmac_sha256(&self.key, &control_payload(envelope)?);
        if !constant_time_bytes(&expected, &provided) {
            return Err(control_auth_invalid());
        }
        Ok(())
    }
}

#[cfg(unix)]
pub struct ControlSocketServer;

#[cfg(unix)]
pub struct RunningControlSocket {
    path: PathBuf,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
    shutdown_requested: Arc<ControlShutdownState>,
}

#[cfg(unix)]
#[derive(Default)]
struct ControlShutdownState {
    requested: AtomicBool,
    notify: tokio::sync::Notify,
}

#[cfg(unix)]
impl RunningControlSocket {
    pub async fn wait_for_shutdown_request(&self) {
        if self.shutdown_requested.requested.load(Ordering::SeqCst) {
            return;
        }
        self.shutdown_requested.notify.notified().await;
    }

    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        self.task.await.map_err(|_| {
            AppError::new("GATEWAY_CONTROL_JOIN_FAILED", "control socket task failed")
        })?;
        if self.path.exists() {
            std::fs::remove_file(&self.path).map_err(|_| {
                AppError::new("GATEWAY_CONTROL_IO", "control socket cleanup failed")
            })?;
        }
        Ok(())
    }
}

#[cfg(unix)]
impl ControlSocketServer {
    pub async fn start(
        path: PathBuf,
        control: Arc<AuthenticatedControl>,
    ) -> Result<RunningControlSocket> {
        validate_control_path(&path, control.expected_uid)?;
        if path.exists() {
            let metadata = std::fs::symlink_metadata(&path).map_err(|_| {
                AppError::new("GATEWAY_CONTROL_IO", "control socket metadata failed")
            })?;
            if !metadata.file_type().is_socket() || metadata.uid() != control.expected_uid {
                return Err(AppError::new(
                    "GATEWAY_CONTROL_PATH_UNSAFE",
                    "existing control path is not an owned Unix socket",
                ));
            }
            std::fs::remove_file(&path).map_err(|_| {
                AppError::new("GATEWAY_CONTROL_IO", "stale control socket cleanup failed")
            })?;
        }
        let listener = UnixListener::bind(&path).map_err(|_| {
            AppError::new("GATEWAY_CONTROL_BIND_FAILED", "control socket bind failed")
        })?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|_| AppError::new("GATEWAY_CONTROL_IO", "control socket permission failed"))?;
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let last_nonce = Arc::new(AtomicU64::new(0));
        let shutdown_requested = Arc::new(ControlShutdownState::default());
        let task_shutdown_requested = shutdown_requested.clone();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { continue };
                        let control = control.clone();
                        let nonce = last_nonce.clone();
                        let requested = task_shutdown_requested.clone();
                        tokio::spawn(async move {
                            let _ = handle_control_stream(stream, control, nonce, requested).await;
                        });
                    }
                }
            }
        });
        Ok(RunningControlSocket {
            path,
            shutdown: Some(shutdown_tx),
            task,
            shutdown_requested,
        })
    }
}

#[cfg(unix)]
pub struct ControlSocketClient;

#[cfg(unix)]
impl ControlSocketClient {
    pub async fn send(
        path: &Path,
        control: &AuthenticatedControl,
        envelope: &ControlEnvelope,
    ) -> Result<ControlEnvelope> {
        let mut stream = UnixStream::connect(path).await.map_err(|_| {
            AppError::new(
                "GATEWAY_CONTROL_CONNECT_FAILED",
                "control socket connect failed",
            )
        })?;
        let peer_uid = peer_uid(&stream)?;
        if peer_uid != control.expected_uid {
            return Err(AppError::new(
                "GATEWAY_CONTROL_PEER_FORBIDDEN",
                "control server peer uid is not allowed",
            ));
        }
        stream
            .write_all(&envelope.encode_frame()?)
            .await
            .map_err(|_| AppError::new("GATEWAY_CONTROL_IO", "control request write failed"))?;
        let response = read_control_frame(&mut stream).await?;
        control.verify_response(peer_uid, &response, envelope.request_nonce)?;
        Ok(response)
    }
}

#[cfg(unix)]
async fn handle_control_stream(
    mut stream: UnixStream,
    control: Arc<AuthenticatedControl>,
    last_nonce: Arc<AtomicU64>,
    shutdown_requested: Arc<ControlShutdownState>,
) -> Result<()> {
    let peer_uid = peer_uid(&stream)?;
    let request = read_control_frame(&mut stream).await?;
    let current = last_nonce.load(Ordering::SeqCst);
    let result = control.verify_and_advance(peer_uid, &request, current);
    let command = request.command.clone();
    let payload = match result {
        Ok(nonce) => {
            if last_nonce
                .compare_exchange(current, nonce, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                if command == ControlCommand::Shutdown {
                    shutdown_requested.requested.store(true, Ordering::SeqCst);
                    shutdown_requested.notify.notify_waiters();
                }
                serde_json::json!({ "ok": true, "acceptedNonce": nonce })
            } else {
                serde_json::json!({ "ok": false, "code": "GATEWAY_CONTROL_NONCE_REPLAY" })
            }
        }
        Err(error) => serde_json::json!({ "ok": false, "code": error.code }),
    };
    let response = control.sign(request.request_nonce, ControlCommand::Status, payload)?;
    stream
        .write_all(&response.encode_frame()?)
        .await
        .map_err(|_| AppError::new("GATEWAY_CONTROL_IO", "control response write failed"))
}

#[cfg(unix)]
async fn read_control_frame(stream: &mut UnixStream) -> Result<ControlEnvelope> {
    let mut prefix = [0_u8; 4];
    stream
        .read_exact(&mut prefix)
        .await
        .map_err(|_| control_frame_invalid())?;
    let length = u32::from_be_bytes(prefix) as usize;
    if length > MAX_CONTROL_MESSAGE_BYTES {
        return Err(control_message_limit());
    }
    let mut frame = vec![0_u8; length + 4];
    frame[..4].copy_from_slice(&prefix);
    stream
        .read_exact(&mut frame[4..])
        .await
        .map_err(|_| control_frame_invalid())?;
    ControlEnvelope::decode_frame(&frame)
}

#[cfg(unix)]
fn validate_control_path(path: &Path, expected_uid: u32) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        AppError::new(
            "GATEWAY_CONTROL_PATH_UNSAFE",
            "control socket parent is missing",
        )
    })?;
    let metadata = std::fs::symlink_metadata(parent).map_err(|_| {
        AppError::new(
            "GATEWAY_CONTROL_PATH_UNSAFE",
            "control socket parent is unavailable",
        )
    })?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != expected_uid
        || metadata.mode() & 0o077 != 0
    {
        return Err(AppError::new(
            "GATEWAY_CONTROL_PATH_UNSAFE",
            "control socket parent must be private and owner-controlled",
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn peer_uid(stream: &UnixStream) -> Result<u32> {
    let mut credentials: libc::xucred = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of::<libc::xucred>() as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_LOCAL,
            libc::LOCAL_PEERCRED,
            &mut credentials as *mut _ as *mut libc::c_void,
            &mut length,
        )
    };
    if result != 0 || credentials.cr_version != 0 {
        return Err(AppError::new(
            "GATEWAY_CONTROL_PEER_UNKNOWN",
            "control peer credentials are unavailable",
        ));
    }
    Ok(credentials.cr_uid)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn peer_uid(stream: &UnixStream) -> Result<u32> {
    let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut credentials as *mut _ as *mut libc::c_void,
            &mut length,
        )
    };
    if result != 0 {
        return Err(AppError::new(
            "GATEWAY_CONTROL_PEER_UNKNOWN",
            "control peer credentials are unavailable",
        ));
    }
    Ok(credentials.uid)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartDecision {
    RestartAfter(Duration),
    Failed,
}

#[derive(Debug, Clone)]
pub struct SupervisorPolicy {
    max_restarts: u32,
    initial_backoff: Duration,
    max_backoff: Duration,
    idle_grace: Duration,
}

impl SupervisorPolicy {
    pub fn new(
        max_restarts: u32,
        initial_backoff: Duration,
        max_backoff: Duration,
        idle_grace: Duration,
    ) -> Result<Self> {
        if max_restarts == 0
            || initial_backoff.is_zero()
            || max_backoff < initial_backoff
            || idle_grace.is_zero()
        {
            return Err(AppError::new(
                "GATEWAY_SUPERVISOR_POLICY_INVALID",
                "Gateway supervisor policy is invalid",
            ));
        }
        Ok(Self {
            max_restarts,
            initial_backoff,
            max_backoff,
            idle_grace,
        })
    }

    pub fn restart_decision(&self, failure_count: u32, jitter_seed: u64) -> RestartDecision {
        if failure_count == 0 || failure_count > self.max_restarts {
            return RestartDecision::Failed;
        }
        let exponent = failure_count.saturating_sub(1).min(31);
        let base = self
            .initial_backoff
            .checked_mul(1_u32 << exponent)
            .unwrap_or(self.max_backoff)
            .min(self.max_backoff);
        let jitter_cap = base / 4;
        let jitter = if jitter_cap.is_zero() {
            Duration::ZERO
        } else {
            Duration::from_nanos(jitter_seed % (jitter_cap.as_nanos() as u64 + 1))
        };
        RestartDecision::RestartAfter((base + jitter).min(self.max_backoff))
    }

    pub fn should_idle_shutdown(
        &self,
        attached_bindings: usize,
        inflight_requests: usize,
        idle_for: Duration,
    ) -> bool {
        attached_bindings == 0 && inflight_requests == 0 && idle_for >= self.idle_grace
    }
}

fn ensure_compatible(
    state: &GatewayRuntimeState,
    component_version: &str,
    control_protocol_version: u32,
    binding_schema: u32,
) -> Result<()> {
    if state.component_version != component_version
        || state.control_protocol_version != control_protocol_version
        || state.binding_schema != binding_schema
    {
        return Err(AppError::new(
            "GATEWAY_UPGRADE_REQUIRED",
            "Gateway runtime state is incompatible with installed components",
        ));
    }
    Ok(())
}

fn validate_port(port: u16) -> Result<()> {
    if !(49_152..=65_535).contains(&port) {
        return Err(AppError::new(
            "GATEWAY_PORT_INVALID",
            "Gateway stable port must be in the approved private range",
        ));
    }
    Ok(())
}

fn validate_timestamp(value: &str) -> Result<()> {
    DateTime::parse_from_rfc3339(value)
        .map(|_| ())
        .map_err(|_| {
            AppError::new(
                "GATEWAY_STATE_TIMESTAMP_INVALID",
                "Gateway timestamp must be RFC 3339",
            )
        })
}

fn port_plan_fingerprint(plan: &GatewayPortChangePlan) -> String {
    let bytes = serde_json::to_vec(&(
        "rpg-305-port-plan-v1",
        plan.expected_state_revision,
        plan.old_port,
        plan.new_port,
        &plan.profile_ids,
    ))
    .expect("port plan is serializable");
    hex::encode(Sha256::digest(bytes))
}

fn control_payload(envelope: &ControlEnvelope) -> Result<Vec<u8>> {
    serde_json::to_vec(&(
        envelope.protocol_version,
        envelope.request_nonce,
        &envelope.command,
        &envelope.payload,
    ))
    .map_err(|_| control_frame_invalid())
}

fn hmac_sha256(key: &[u8; 32], message: &[u8]) -> [u8; 32] {
    let mut inner_pad = [0x36_u8; 64];
    let mut outer_pad = [0x5c_u8; 64];
    for (index, byte) in key.iter().enumerate() {
        inner_pad[index] ^= byte;
        outer_pad[index] ^= byte;
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner);
    outer.finalize().into()
}

fn constant_time_bytes(expected: &[u8], provided: &[u8]) -> bool {
    if expected.len() != provided.len() {
        return false;
    }
    expected
        .iter()
        .zip(provided)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn revision_conflict() -> AppError {
    AppError::new("STORE_REVISION_CONFLICT", "Gateway state revision changed")
}

fn control_message_limit() -> AppError {
    AppError::new(
        "GATEWAY_CONTROL_MESSAGE_LIMIT",
        "Gateway control message exceeds 64 KiB",
    )
}

fn control_frame_invalid() -> AppError {
    AppError::new(
        "GATEWAY_CONTROL_FRAME_INVALID",
        "Gateway control frame is invalid",
    )
}

fn control_auth_invalid() -> AppError {
    AppError::new(
        "GATEWAY_CONTROL_AUTH_INVALID",
        "Gateway control authentication failed",
    )
}
