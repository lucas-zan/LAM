use super::binding::{GatewayBindingService, GatewayBindingSnapshot};
use super::observability::{
    capacity_identity_hash, GatewayActivitySnapshot, GatewayRejectionReason, GatewayTimeoutStage,
};
use super::upstream::{CodexUpstreamHeaders, GatewayCancellation};
use crate::services::error::{AppError, Result};
use crate::services::provider_keychain::KeychainBackend;
use axum::body::{to_bytes, Body, BodyDataStream, Bytes};
use axum::extract::{Extension, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::future::Future;
use std::io::{self, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::{oneshot, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;
use tokio_stream::Stream;
use uuid::Uuid;

const HEALTH_NONCE_HEADER: &str = "x-lam-health-nonce";
const REQUEST_ID_HEADER: &str = "x-request-id";

pub trait GatewayAuthenticator: Send + Sync {
    fn authenticate(&self, authorization: &str) -> Result<GatewayBindingSnapshot>;
}

impl<B: KeychainBackend> GatewayAuthenticator for GatewayBindingService<B> {
    fn authenticate(&self, authorization: &str) -> Result<GatewayBindingSnapshot> {
        self.authenticate(authorization, &Utc::now().to_rfc3339())
    }
}

#[derive(Debug, Clone)]
pub struct GatewayHttpRequest {
    pub path: String,
    pub body: Vec<u8>,
    pub binding: GatewayBindingSnapshot,
    pub request_id: String,
    pub upstream_headers: CodexUpstreamHeaders,
}

pub struct GatewayHttpResponse {
    status: StatusCode,
    content_type: String,
    body: Body,
    retry_after: Option<String>,
    usage: Option<RequestUsageMetadata>,
    retry_count: u8,
    streaming: bool,
    ttft_ms: Option<u64>,
    upstream_status: Option<u16>,
    outcome_code: Option<String>,
    timeout_stage: Option<GatewayTimeoutStage>,
    stream_cancellation: Option<GatewayCancellation>,
    stream_completion: Option<oneshot::Receiver<()>>,
}

impl GatewayHttpResponse {
    pub fn json(status: u16, body: Value) -> Self {
        Self {
            status: StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            content_type: "application/json".into(),
            body: Body::from(serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec())),
            retry_after: None,
            usage: None,
            retry_count: 0,
            streaming: false,
            ttft_ms: None,
            upstream_status: None,
            outcome_code: None,
            timeout_stage: None,
            stream_cancellation: None,
            stream_completion: None,
        }
    }

    pub fn from_body(status: u16, content_type: impl Into<String>, body: Body) -> Self {
        Self {
            status: StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            content_type: content_type.into(),
            body,
            retry_after: None,
            usage: None,
            retry_count: 0,
            streaming: false,
            ttft_ms: None,
            upstream_status: None,
            outcome_code: None,
            timeout_stage: None,
            stream_cancellation: None,
            stream_completion: None,
        }
    }

    pub fn with_retry_after(mut self, retry_after: Option<String>) -> Self {
        self.retry_after = retry_after;
        self
    }

    pub fn with_metrics(mut self, usage: Option<RequestUsageMetadata>, attempts: u8) -> Self {
        self.usage = usage;
        self.retry_count = attempts.saturating_sub(1);
        self
    }

    pub fn streaming(mut self) -> Self {
        self.streaming = true;
        self
    }

    pub fn with_upstream_metrics(mut self, status: u16, ttft_ms: u64) -> Self {
        self.upstream_status = Some(status);
        self.ttft_ms = Some(ttft_ms);
        self
    }

    pub fn with_outcome_code(mut self, code: impl Into<String>) -> Self {
        let code = code.into();
        self.timeout_stage = Some(GatewayTimeoutStage::from_code(&code))
            .filter(|stage| *stage != GatewayTimeoutStage::Other);
        self.outcome_code = Some(code);
        self
    }

    pub fn with_stream_lifecycle(
        mut self,
        cancellation: GatewayCancellation,
        completion: oneshot::Receiver<()>,
    ) -> Self {
        self.stream_cancellation = Some(cancellation);
        self.stream_completion = Some(completion);
        self
    }

    pub fn status(&self) -> u16 {
        self.status.as_u16()
    }

    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    pub fn retry_after(&self) -> Option<&str> {
        self.retry_after.as_deref()
    }

    pub fn usage(&self) -> Option<&RequestUsageMetadata> {
        self.usage.as_ref()
    }

    pub fn ttft_ms(&self) -> Option<u64> {
        self.ttft_ms
    }

    pub fn upstream_status(&self) -> Option<u16> {
        self.upstream_status
    }

    pub fn outcome_code(&self) -> Option<&str> {
        self.outcome_code.as_deref()
    }

    pub async fn collect_bytes(self) -> Result<Vec<u8>> {
        to_bytes(self.body, 32 * 1024 * 1024)
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|_| {
                AppError::new(
                    "GATEWAY_RESPONSE_LIMIT",
                    "Gateway response body exceeded its limit",
                )
            })
    }

    fn into_response(mut self, activity: &GatewayServerActivity) -> Response {
        if let Some(completion) = self.stream_completion.take() {
            activity.track_upstream_stream(completion);
        }
        let body = if self.streaming {
            Body::from_stream(TrackedBodyStream::new(
                self.body.into_data_stream(),
                activity.begin_stream(self.stream_cancellation.take()),
            ))
        } else {
            self.body
        };
        let mut response = Response::new(body);
        *response.status_mut() = self.status;
        if let Ok(value) = HeaderValue::from_str(&self.content_type) {
            response.headers_mut().insert("content-type", value);
        }
        response
            .headers_mut()
            .insert("cache-control", HeaderValue::from_static("no-store"));
        if let Some(value) = self
            .retry_after
            .as_deref()
            .and_then(|value| HeaderValue::from_str(value).ok())
        {
            response.headers_mut().insert("retry-after", value);
        }
        response
    }
}

pub trait GatewayRouteHandler: Send + Sync {
    fn handle(
        &self,
        request: GatewayHttpRequest,
    ) -> Pin<Box<dyn Future<Output = Result<GatewayHttpResponse>> + Send + '_>>;
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HealthDocument {
    pub protocol_version: u32,
    pub component_version: String,
    pub state_schema: u32,
    pub install_id: String,
    pub instance_id: String,
    pub ready: bool,
    pub proof: String,
}

#[derive(Clone)]
pub struct HealthProofKey([u8; 32]);

impl fmt::Debug for HealthProofKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("HealthProofKey([REDACTED])")
    }
}

impl HealthProofKey {
    pub fn new(key: &[u8]) -> Result<Self> {
        if key.len() != 32 {
            return Err(AppError::new(
                "GATEWAY_IDENTITY_KEY_INVALID",
                "Gateway identity key must contain 256 bits",
            ));
        }
        let mut value = [0_u8; 32];
        value.copy_from_slice(key);
        Ok(Self(value))
    }

    pub fn sign(&self, nonce: &str, document: &HealthDocument) -> String {
        hex::encode(hmac_sha256(&self.0, &health_payload(nonce, document)))
    }

    pub fn verify(&self, nonce: &str, document: &HealthDocument) -> bool {
        let Ok(provided) = hex::decode(&document.proof) else {
            return false;
        };
        let expected = hmac_sha256(&self.0, &health_payload(nonce, document));
        constant_time_bytes(&expected, &provided)
    }
}

#[derive(Debug, Clone)]
pub struct GatewayServerConfig {
    pub bind_addr: SocketAddr,
    pub protocol_version: u32,
    pub component_version: String,
    pub state_schema: u32,
    pub install_id: String,
    pub instance_id: String,
    pub ready: bool,
    pub max_body_bytes: usize,
    pub max_inflight: usize,
    pub max_inflight_per_binding: usize,
    pub max_queue: usize,
    pub max_queue_per_binding: usize,
    pub request_timeout: Duration,
}

impl GatewayServerConfig {
    pub fn for_test() -> Self {
        Self {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            protocol_version: 1,
            component_version: "test".into(),
            state_schema: 1,
            install_id: "test-install".into(),
            instance_id: "test-instance".into(),
            ready: true,
            max_body_bytes: 4 * 1024 * 1024,
            max_inflight: 32,
            max_inflight_per_binding: 4,
            max_queue: 64,
            max_queue_per_binding: 8,
            request_timeout: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RequestLogMetadata {
    pub request_id: String,
    pub route: String,
    pub status: u16,
    pub latency_ms: u64,
    pub binding_hash: String,
    pub provider_hash: String,
    pub capacity_hash: String,
    pub queue_wait_ms: u64,
    pub queued_body_bytes: usize,
    pub global_running: usize,
    pub global_queued: usize,
    pub binding_running: usize,
    pub binding_queued: usize,
    pub active_streams: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttft_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_stage: Option<GatewayTimeoutStage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejection_reason: Option<GatewayRejectionReason>,
    pub retry_count: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<RequestUsageMetadata>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RequestUsageMetadata {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

pub trait RequestObserver: Send + Sync {
    fn observe(&self, metadata: RequestLogMetadata);
}

pub struct FileMetadataObserver {
    path: PathBuf,
    write_lock: Mutex<()>,
}

impl FileMetadataObserver {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            write_lock: Mutex::new(()),
        }
    }

    fn append(&self, metadata: &RequestLogMetadata) -> io::Result<()> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| io::Error::other("metadata log lock poisoned"))?;
        let mut line = serde_json::to_vec(metadata).map_err(io::Error::other)?;
        line.push(b'\n');
        private_append_file(&self.path)?.write_all(&line)
    }
}

impl RequestObserver for FileMetadataObserver {
    fn observe(&self, metadata: RequestLogMetadata) {
        let _ = self.append(&metadata);
    }
}

fn private_append_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    let file = options.open(path)?;
    #[cfg(unix)]
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

#[derive(Clone)]
struct ServerState {
    config: GatewayServerConfig,
    authenticator: Arc<dyn GatewayAuthenticator>,
    handler: Arc<dyn GatewayRouteHandler>,
    health_key: Arc<HealthProofKey>,
    observer: Arc<dyn RequestObserver>,
    inflight: Arc<Semaphore>,
    queued: Arc<AtomicUsize>,
    binding_limits: Arc<Mutex<BTreeMap<String, Arc<BindingLimits>>>>,
    activity: GatewayServerActivity,
}

struct BindingLimits {
    inflight: Arc<Semaphore>,
    queued: Arc<AtomicUsize>,
    running: Arc<AtomicUsize>,
}

struct GatewayAdmissionPermits {
    _binding: OwnedSemaphorePermit,
    _global: OwnedSemaphorePermit,
}

impl GatewayAdmissionPermits {
    fn try_acquire(binding: Arc<Semaphore>, global: Arc<Semaphore>) -> Option<Self> {
        let binding = binding.try_acquire_owned().ok()?;
        let global = global.try_acquire_owned().ok()?;
        Some(Self {
            _binding: binding,
            _global: global,
        })
    }

    async fn acquire(
        binding: Arc<Semaphore>,
        global: Arc<Semaphore>,
    ) -> std::result::Result<Self, tokio::sync::AcquireError> {
        let binding = binding.acquire_owned().await?;
        let global = global.acquire_owned().await?;
        Ok(Self {
            _binding: binding,
            _global: global,
        })
    }
}

#[derive(Clone)]
pub struct GatewayServerActivity(Arc<GatewayServerActivityInner>);

struct GatewayServerActivityInner {
    epoch: Instant,
    last_activity_ms: AtomicU64,
    inflight: AtomicUsize,
    running: AtomicUsize,
    queued: AtomicUsize,
    queued_body_bytes: AtomicUsize,
    active_streams: AtomicUsize,
    active_upstream_streams: AtomicUsize,
    client_cancellations: AtomicU64,
    stream_interruptions: AtomicU64,
}

impl GatewayServerActivity {
    fn new() -> Self {
        Self(Arc::new(GatewayServerActivityInner {
            epoch: Instant::now(),
            last_activity_ms: AtomicU64::new(0),
            inflight: AtomicUsize::new(0),
            running: AtomicUsize::new(0),
            queued: AtomicUsize::new(0),
            queued_body_bytes: AtomicUsize::new(0),
            active_streams: AtomicUsize::new(0),
            active_upstream_streams: AtomicUsize::new(0),
            client_cancellations: AtomicU64::new(0),
            stream_interruptions: AtomicU64::new(0),
        }))
    }

    pub fn inflight_requests(&self) -> usize {
        self.0.inflight.load(Ordering::Acquire)
    }

    pub fn snapshot(&self) -> GatewayActivitySnapshot {
        GatewayActivitySnapshot {
            inflight_requests: self.0.inflight.load(Ordering::Acquire),
            running_requests: self.0.running.load(Ordering::Acquire),
            queued_requests: self.0.queued.load(Ordering::Acquire),
            queued_body_bytes: self.0.queued_body_bytes.load(Ordering::Acquire),
            active_streams: self.0.active_streams.load(Ordering::Acquire),
            active_upstream_streams: self.0.active_upstream_streams.load(Ordering::Acquire),
            client_cancellations: self.0.client_cancellations.load(Ordering::Acquire),
            stream_interruptions: self.0.stream_interruptions.load(Ordering::Acquire),
        }
    }

    pub fn idle_for(&self) -> Duration {
        let now = self.0.epoch.elapsed().as_millis().min(u64::MAX as u128) as u64;
        Duration::from_millis(now.saturating_sub(self.0.last_activity_ms.load(Ordering::Acquire)))
    }

    fn begin(&self) -> GatewayActivityGuard {
        self.touch();
        self.0.inflight.fetch_add(1, Ordering::AcqRel);
        GatewayActivityGuard(self.clone())
    }

    fn begin_running(&self, binding: Arc<AtomicUsize>) -> GatewayRunningGuard {
        self.0.running.fetch_add(1, Ordering::AcqRel);
        binding.fetch_add(1, Ordering::AcqRel);
        GatewayRunningGuard {
            activity: self.clone(),
            binding,
        }
    }

    fn begin_queue(&self, body_bytes: usize) {
        self.0.queued.fetch_add(1, Ordering::AcqRel);
        self.0
            .queued_body_bytes
            .fetch_add(body_bytes, Ordering::AcqRel);
    }

    fn end_queue(&self, body_bytes: usize) {
        self.0.queued.fetch_sub(1, Ordering::AcqRel);
        self.0
            .queued_body_bytes
            .fetch_sub(body_bytes, Ordering::AcqRel);
    }

    fn begin_stream(&self, cancellation: Option<GatewayCancellation>) -> GatewayStreamGuard {
        self.0.active_streams.fetch_add(1, Ordering::AcqRel);
        GatewayStreamGuard {
            activity: self.clone(),
            cancellation,
            finished: false,
        }
    }

    fn track_upstream_stream(&self, completion: oneshot::Receiver<()>) {
        self.0
            .active_upstream_streams
            .fetch_add(1, Ordering::AcqRel);
        let activity = self.clone();
        tokio::spawn(async move {
            let _ = completion.await;
            activity
                .0
                .active_upstream_streams
                .fetch_sub(1, Ordering::AcqRel);
        });
    }

    fn touch(&self) {
        let elapsed = self.0.epoch.elapsed().as_millis().min(u64::MAX as u128) as u64;
        self.0.last_activity_ms.store(elapsed, Ordering::Release);
    }
}

struct GatewayActivityGuard(GatewayServerActivity);

impl Drop for GatewayActivityGuard {
    fn drop(&mut self) {
        (self.0).0.inflight.fetch_sub(1, Ordering::AcqRel);
        self.0.touch();
    }
}

struct GatewayRunningGuard {
    activity: GatewayServerActivity,
    binding: Arc<AtomicUsize>,
}

impl Drop for GatewayRunningGuard {
    fn drop(&mut self) {
        self.activity.0.running.fetch_sub(1, Ordering::AcqRel);
        self.binding.fetch_sub(1, Ordering::AcqRel);
    }
}

struct GatewayStreamGuard {
    activity: GatewayServerActivity,
    cancellation: Option<GatewayCancellation>,
    finished: bool,
}

impl GatewayStreamGuard {
    fn complete(&mut self, interrupted: bool) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.activity
            .0
            .active_streams
            .fetch_sub(1, Ordering::AcqRel);
        if interrupted {
            if let Some(cancellation) = &self.cancellation {
                cancellation.cancel();
            }
            self.activity
                .0
                .stream_interruptions
                .fetch_add(1, Ordering::AcqRel);
        }
    }
}

impl Drop for GatewayStreamGuard {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        if let Some(cancellation) = &self.cancellation {
            cancellation.cancel();
        }
        self.activity
            .0
            .active_streams
            .fetch_sub(1, Ordering::AcqRel);
        self.activity
            .0
            .client_cancellations
            .fetch_add(1, Ordering::AcqRel);
    }
}

struct TrackedBodyStream {
    inner: BodyDataStream,
    guard: GatewayStreamGuard,
}

impl TrackedBodyStream {
    fn new(inner: BodyDataStream, guard: GatewayStreamGuard) -> Self {
        Self { inner, guard }
    }
}

impl Stream for TrackedBodyStream {
    type Item = std::result::Result<Bytes, axum::Error>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let result = Pin::new(&mut self.inner).poll_next(context);
        match &result {
            Poll::Ready(None) => self.guard.complete(false),
            Poll::Ready(Some(Err(_))) => self.guard.complete(true),
            _ => {}
        }
        result
    }
}

pub struct GatewayLoopbackServer;

pub struct RunningGatewayServer {
    local_addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<std::io::Result<()>>,
    activity: GatewayServerActivity,
}

impl fmt::Debug for RunningGatewayServer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RunningGatewayServer")
            .field("local_addr", &self.local_addr)
            .finish_non_exhaustive()
    }
}

impl RunningGatewayServer {
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub fn activity(&self) -> GatewayServerActivity {
        self.activity.clone()
    }

    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        self.task
            .await
            .map_err(|_| AppError::new("GATEWAY_SERVER_JOIN_FAILED", "Gateway server task failed"))?
            .map_err(|_| AppError::new("GATEWAY_SERVER_IO", "Gateway server I/O failed"))
    }
}

impl GatewayLoopbackServer {
    pub async fn start(
        config: GatewayServerConfig,
        authenticator: Arc<dyn GatewayAuthenticator>,
        handler: Arc<dyn GatewayRouteHandler>,
        health_key: Arc<HealthProofKey>,
        observer: Arc<dyn RequestObserver>,
    ) -> Result<RunningGatewayServer> {
        validate_server_config(&config)?;
        let listener = TcpListener::bind(config.bind_addr)
            .await
            .map_err(|_| AppError::new("GATEWAY_PORT_IN_USE", "Gateway port is unavailable"))?;
        Self::start_with_listener(
            config,
            listener,
            authenticator,
            handler,
            health_key,
            observer,
        )
        .await
    }

    pub async fn start_with_listener(
        config: GatewayServerConfig,
        listener: TcpListener,
        authenticator: Arc<dyn GatewayAuthenticator>,
        handler: Arc<dyn GatewayRouteHandler>,
        health_key: Arc<HealthProofKey>,
        observer: Arc<dyn RequestObserver>,
    ) -> Result<RunningGatewayServer> {
        validate_server_config(&config)?;
        let local_addr = listener
            .local_addr()
            .map_err(|_| AppError::new("GATEWAY_SERVER_IO", "Gateway socket identity failed"))?;
        if local_addr.ip() != config.bind_addr.ip()
            || (config.bind_addr.port() != 0 && local_addr.port() != config.bind_addr.port())
        {
            return Err(AppError::new(
                "GATEWAY_LISTENER_IDENTITY_INVALID",
                "Gateway listener does not match configured loopback identity",
            ));
        }
        start_on_listener(
            config,
            listener,
            local_addr,
            authenticator,
            handler,
            health_key,
            observer,
        )
    }
}

fn validate_server_config(config: &GatewayServerConfig) -> Result<()> {
    if config.bind_addr.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST) {
        return Err(AppError::new(
            "GATEWAY_BIND_ADDRESS_FORBIDDEN",
            "Gateway must bind only IPv4 loopback",
        ));
    }
    if config.max_body_bytes == 0
        || config.max_inflight == 0
        || config.max_inflight_per_binding == 0
    {
        return Err(AppError::new(
            "GATEWAY_SERVER_LIMIT_INVALID",
            "Gateway limits must be non-zero",
        ));
    }
    Ok(())
}

fn start_on_listener(
    config: GatewayServerConfig,
    listener: TcpListener,
    local_addr: SocketAddr,
    authenticator: Arc<dyn GatewayAuthenticator>,
    handler: Arc<dyn GatewayRouteHandler>,
    health_key: Arc<HealthProofKey>,
    observer: Arc<dyn RequestObserver>,
) -> Result<RunningGatewayServer> {
    let activity = GatewayServerActivity::new();
    let state = ServerState {
        inflight: Arc::new(Semaphore::new(config.max_inflight)),
        queued: Arc::new(AtomicUsize::new(0)),
        binding_limits: Arc::new(Mutex::new(BTreeMap::new())),
        config,
        authenticator,
        handler,
        health_key,
        observer,
        activity: activity.clone(),
    };
    let router = Router::new()
        .route("/healthz", get(health))
        .route("/v1/models", get(dispatch_empty))
        .route("/v1/responses", post(dispatch_body))
        .fallback(route_not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .with_state(state.clone())
        .layer(middleware::from_fn_with_state(state, authenticate_request));
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
    });
    Ok(RunningGatewayServer {
        local_addr,
        shutdown: Some(shutdown_tx),
        task,
        activity,
    })
}

async fn authenticate_request(
    State(state): State<ServerState>,
    mut request: Request,
    next: Next,
) -> Response {
    let authorization = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let binding = match state.authenticator.authenticate(authorization) {
        Ok(binding) => binding,
        Err(_) => return gateway_error(StatusCode::UNAUTHORIZED, "GATEWAY_AUTH_INVALID"),
    };
    let request_id =
        accepted_request_id(request.headers()).unwrap_or_else(|| Uuid::new_v4().to_string());
    request.extensions_mut().insert(binding);
    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));
    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }
    response
}

async fn health(State(state): State<ServerState>, headers: HeaderMap) -> Response {
    let Some(nonce) = headers
        .get(HEALTH_NONCE_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| valid_nonce(value))
    else {
        return gateway_error(StatusCode::BAD_REQUEST, "GATEWAY_HEALTH_NONCE_INVALID");
    };
    let mut document = HealthDocument {
        protocol_version: state.config.protocol_version,
        component_version: state.config.component_version.clone(),
        state_schema: state.config.state_schema,
        install_id: state.config.install_id.clone(),
        instance_id: state.config.instance_id.clone(),
        ready: state.config.ready,
        proof: String::new(),
    };
    document.proof = state.health_key.sign(nonce, &document);
    (StatusCode::OK, Json(document)).into_response()
}

async fn dispatch_empty(
    State(state): State<ServerState>,
    Extension(binding): Extension<GatewayBindingSnapshot>,
    Extension(request_id): Extension<RequestId>,
    request: Request,
) -> Response {
    let upstream_headers = CodexUpstreamHeaders::capture(request.headers());
    dispatch(
        state,
        binding,
        request_id.0,
        request.uri().path().into(),
        Vec::new(),
        upstream_headers,
    )
    .await
}

async fn dispatch_body(
    State(state): State<ServerState>,
    Extension(binding): Extension<GatewayBindingSnapshot>,
    Extension(request_id): Extension<RequestId>,
    request: Request,
) -> Response {
    let path = request.uri().path().to_owned();
    let upstream_headers = CodexUpstreamHeaders::capture(request.headers());
    let body = match to_bytes(request.into_body(), state.config.max_body_bytes).await {
        Ok(body) => body.to_vec(),
        Err(_) => return gateway_error(StatusCode::PAYLOAD_TOO_LARGE, "GATEWAY_BODY_LIMIT"),
    };
    dispatch(state, binding, request_id.0, path, body, upstream_headers).await
}

async fn dispatch(
    state: ServerState,
    binding: GatewayBindingSnapshot,
    request_id: String,
    path: String,
    body: Vec<u8>,
    upstream_headers: CodexUpstreamHeaders,
) -> Response {
    let _activity = state.activity.begin();
    let started = Instant::now();
    let binding_limits = {
        let mut limits = state.binding_limits.lock().expect("binding limits lock");
        limits
            .entry(binding.binding_id.clone())
            .or_insert_with(|| {
                Arc::new(BindingLimits {
                    inflight: Arc::new(Semaphore::new(state.config.max_inflight_per_binding)),
                    queued: Arc::new(AtomicUsize::new(0)),
                    running: Arc::new(AtomicUsize::new(0)),
                })
            })
            .clone()
    };
    let observation = RequestObservation::new(&request_id, &path, &binding, started);
    let immediate = GatewayAdmissionPermits::try_acquire(
        binding_limits.inflight.clone(),
        state.inflight.clone(),
    );
    let mut queue_wait_ms = 0;
    let mut queued_body_bytes = 0;
    let permits = if let Some(permits) = immediate {
        permits
    } else {
        let queue_started = Instant::now();
        let queue = match QueueGuard::enter(
            state.queued.clone(),
            state.config.max_queue,
            binding_limits.queued.clone(),
            state.config.max_queue_per_binding,
            state.activity.clone(),
            body.len(),
        ) {
            Ok(queue) => queue,
            Err(reason) => {
                observation.record(&state, &binding_limits, RequestOutcome::rejected(reason));
                return gateway_error(StatusCode::TOO_MANY_REQUESTS, "GATEWAY_CONCURRENCY_LIMIT");
            }
        };
        queued_body_bytes = body.len();
        let permits = tokio::time::timeout(
            state.config.request_timeout,
            GatewayAdmissionPermits::acquire(
                binding_limits.inflight.clone(),
                state.inflight.clone(),
            ),
        )
        .await;
        queue_wait_ms = elapsed_ms(queue_started);
        drop(queue);
        match permits {
            Ok(Ok(permits)) => permits,
            _ => {
                observation.record(
                    &state,
                    &binding_limits,
                    RequestOutcome::queue_timeout(queue_wait_ms, queued_body_bytes),
                );
                return gateway_error(StatusCode::GATEWAY_TIMEOUT, "GATEWAY_REQUEST_TIMEOUT");
            }
        }
    };
    let _running = state.activity.begin_running(binding_limits.running.clone());
    let remaining = state
        .config
        .request_timeout
        .saturating_sub(started.elapsed());
    let result = tokio::time::timeout(
        remaining,
        state.handler.handle(GatewayHttpRequest {
            path: path.clone(),
            body,
            binding,
            request_id: request_id.clone(),
            upstream_headers,
        }),
    )
    .await;
    drop(permits);
    let (response, outcome) = match result {
        Ok(Ok(response)) => {
            let outcome =
                RequestOutcome::from_gateway_response(&response, queue_wait_ms, queued_body_bytes);
            (response.into_response(&state.activity), outcome)
        }
        Ok(Err(error)) => {
            let outcome =
                RequestOutcome::handler_error(&error.code, queue_wait_ms, queued_body_bytes);
            (gateway_error(StatusCode::BAD_GATEWAY, &error.code), outcome)
        }
        Err(_) => {
            let outcome = RequestOutcome::handler_timeout(queue_wait_ms, queued_body_bytes);
            (
                gateway_error(StatusCode::GATEWAY_TIMEOUT, "GATEWAY_REQUEST_TIMEOUT"),
                outcome,
            )
        }
    };
    observation.record(&state, &binding_limits, outcome);
    response
}

struct RequestObservation {
    request_id: String,
    route: String,
    binding_hash: String,
    provider_hash: String,
    capacity_hash: String,
    started: Instant,
}

impl RequestObservation {
    fn new(
        request_id: &str,
        route: &str,
        binding: &GatewayBindingSnapshot,
        started: Instant,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            route: route.into(),
            binding_hash: short_hash(&binding.binding_id),
            provider_hash: short_hash(&binding.provider_id),
            capacity_hash: capacity_identity_hash(
                &binding.provider.base_url,
                &binding.provider.upstream_auth,
            )
            .unwrap_or_else(|_| "invalid".into()),
            started,
        }
    }

    fn record(&self, state: &ServerState, binding: &BindingLimits, outcome: RequestOutcome) {
        let activity = state.activity.snapshot();
        state.observer.observe(RequestLogMetadata {
            request_id: self.request_id.clone(),
            route: self.route.clone(),
            status: outcome.status,
            latency_ms: elapsed_ms(self.started),
            binding_hash: self.binding_hash.clone(),
            provider_hash: self.provider_hash.clone(),
            capacity_hash: self.capacity_hash.clone(),
            queue_wait_ms: outcome.queue_wait_ms,
            queued_body_bytes: outcome.queued_body_bytes,
            global_running: activity.running_requests,
            global_queued: activity.queued_requests,
            binding_running: binding.running.load(Ordering::Acquire),
            binding_queued: binding.queued.load(Ordering::Acquire),
            active_streams: activity.active_streams,
            ttft_ms: outcome.ttft_ms,
            upstream_status: outcome.upstream_status,
            outcome_code: outcome.outcome_code,
            timeout_stage: outcome.timeout_stage,
            rejection_reason: outcome.rejection_reason,
            retry_count: outcome.retry_count,
            usage: outcome.usage,
        });
    }
}

struct RequestOutcome {
    status: u16,
    queue_wait_ms: u64,
    queued_body_bytes: usize,
    ttft_ms: Option<u64>,
    upstream_status: Option<u16>,
    outcome_code: Option<String>,
    timeout_stage: Option<GatewayTimeoutStage>,
    rejection_reason: Option<GatewayRejectionReason>,
    retry_count: u8,
    usage: Option<RequestUsageMetadata>,
}

impl RequestOutcome {
    fn from_gateway_response(
        response: &GatewayHttpResponse,
        queue_wait_ms: u64,
        queued_body_bytes: usize,
    ) -> Self {
        let rejection_reason = response
            .outcome_code()
            .filter(|code| *code == "GATEWAY_UPSTREAM_CONCURRENCY_LIMIT")
            .map(|_| GatewayRejectionReason::UpstreamConcurrency);
        Self {
            status: response.status(),
            queue_wait_ms,
            queued_body_bytes,
            ttft_ms: response.ttft_ms,
            upstream_status: response.upstream_status,
            outcome_code: response.outcome_code.clone(),
            timeout_stage: response.timeout_stage,
            rejection_reason,
            retry_count: response.retry_count,
            usage: response.usage.clone(),
        }
    }

    fn rejected(reason: GatewayRejectionReason) -> Self {
        Self::failure(429, "GATEWAY_CONCURRENCY_LIMIT", None, Some(reason), 0, 0)
    }

    fn queue_timeout(queue_wait_ms: u64, queued_body_bytes: usize) -> Self {
        Self::failure(
            504,
            "GATEWAY_REQUEST_TIMEOUT",
            Some(GatewayTimeoutStage::Queue),
            None,
            queue_wait_ms,
            queued_body_bytes,
        )
    }

    fn handler_timeout(queue_wait_ms: u64, queued_body_bytes: usize) -> Self {
        Self::failure(
            504,
            "GATEWAY_REQUEST_TIMEOUT",
            Some(GatewayTimeoutStage::Handler),
            None,
            queue_wait_ms,
            queued_body_bytes,
        )
    }

    fn handler_error(code: &str, queue_wait_ms: u64, queued_body_bytes: usize) -> Self {
        Self::failure(502, code, None, None, queue_wait_ms, queued_body_bytes)
    }

    fn failure(
        status: u16,
        code: &str,
        timeout_stage: Option<GatewayTimeoutStage>,
        rejection_reason: Option<GatewayRejectionReason>,
        queue_wait_ms: u64,
        queued_body_bytes: usize,
    ) -> Self {
        Self {
            status,
            queue_wait_ms,
            queued_body_bytes,
            ttft_ms: None,
            upstream_status: None,
            outcome_code: Some(code.into()),
            timeout_stage,
            rejection_reason,
            retry_count: 0,
            usage: None,
        }
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}

struct QueueGuard {
    global: Arc<AtomicUsize>,
    binding: Arc<AtomicUsize>,
    activity: GatewayServerActivity,
    body_bytes: usize,
}

impl QueueGuard {
    fn enter(
        global: Arc<AtomicUsize>,
        global_limit: usize,
        binding: Arc<AtomicUsize>,
        binding_limit: usize,
        activity: GatewayServerActivity,
        body_bytes: usize,
    ) -> std::result::Result<Self, GatewayRejectionReason> {
        if !try_increment_below(&global, global_limit) {
            return Err(GatewayRejectionReason::GlobalQueueFull);
        }
        if !try_increment_below(&binding, binding_limit) {
            global.fetch_sub(1, Ordering::AcqRel);
            return Err(GatewayRejectionReason::BindingQueueFull);
        }
        activity.begin_queue(body_bytes);
        Ok(Self {
            global,
            binding,
            activity,
            body_bytes,
        })
    }
}

impl Drop for QueueGuard {
    fn drop(&mut self) {
        self.global.fetch_sub(1, Ordering::AcqRel);
        self.binding.fetch_sub(1, Ordering::AcqRel);
        self.activity.end_queue(self.body_bytes);
    }
}

fn try_increment_below(counter: &AtomicUsize, limit: usize) -> bool {
    let mut current = counter.load(Ordering::Acquire);
    loop {
        if current >= limit {
            return false;
        }
        match counter.compare_exchange_weak(
            current,
            current + 1,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return true,
            Err(next) => current = next,
        }
    }
}

#[cfg(test)]
mod gateway_admission_tests {
    use super::*;

    #[test]
    fn gateway_admission_immediate_failure_releases_binding_permit() {
        let binding = Arc::new(Semaphore::new(1));
        let global = Arc::new(Semaphore::new(0));

        assert!(GatewayAdmissionPermits::try_acquire(binding.clone(), global).is_none());
        assert_eq!(binding.available_permits(), 1);
    }

    #[tokio::test]
    async fn gateway_admission_cancellation_releases_binding_permit() {
        let binding = Arc::new(Semaphore::new(1));
        let global = Arc::new(Semaphore::new(0));

        assert!(tokio::time::timeout(
            Duration::from_millis(20),
            GatewayAdmissionPermits::acquire(binding.clone(), global),
        )
        .await
        .is_err());
        assert_eq!(binding.available_permits(), 1);
    }

    #[tokio::test]
    async fn gateway_admission_drop_releases_both_permits() {
        let binding = Arc::new(Semaphore::new(1));
        let global = Arc::new(Semaphore::new(1));

        let permits = GatewayAdmissionPermits::acquire(binding.clone(), global.clone())
            .await
            .unwrap();
        assert_eq!(binding.available_permits(), 0);
        assert_eq!(global.available_permits(), 0);
        drop(permits);
        assert_eq!(binding.available_permits(), 1);
        assert_eq!(global.available_permits(), 1);
    }
}

async fn route_not_found() -> Response {
    gateway_error(StatusCode::NOT_FOUND, "GATEWAY_ROUTE_NOT_FOUND")
}

async fn method_not_allowed() -> Response {
    gateway_error(StatusCode::METHOD_NOT_ALLOWED, "GATEWAY_METHOD_NOT_ALLOWED")
}

fn gateway_error(status: StatusCode, code: &str) -> Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": "Gateway request rejected" } })),
    )
        .into_response()
}

#[derive(Clone)]
struct RequestId(String);

fn accepted_request_id(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(REQUEST_ID_HEADER)?.to_str().ok()?;
    (value.len() >= 8
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')))
    .then(|| value.to_owned())
}

fn valid_nonce(value: &str) -> bool {
    (16..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn health_payload(nonce: &str, document: &HealthDocument) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "nonce": nonce,
        "protocolVersion": document.protocol_version,
        "componentVersion": document.component_version,
        "stateSchema": document.state_schema,
        "installId": document.install_id,
        "instanceId": document.instance_id,
        "ready": document.ready,
    }))
    .expect("health proof payload is serializable")
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

fn short_hash(value: &str) -> String {
    hex::encode(&Sha256::digest(value.as_bytes())[..8])
}
