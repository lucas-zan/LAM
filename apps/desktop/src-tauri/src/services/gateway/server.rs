use super::binding::{GatewayBindingService, GatewayBindingSnapshot};
use crate::services::error::{AppError, Result};
use crate::services::provider_keychain::KeychainBackend;
use axum::body::{to_bytes, Body};
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
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Semaphore};
use tokio::task::JoinHandle;
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
}

pub struct GatewayHttpResponse {
    status: StatusCode,
    content_type: String,
    body: Body,
    usage: Option<RequestUsageMetadata>,
    retry_count: u8,
}

impl GatewayHttpResponse {
    pub fn json(status: u16, body: Value) -> Self {
        Self {
            status: StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            content_type: "application/json".into(),
            body: Body::from(serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec())),
            usage: None,
            retry_count: 0,
        }
    }

    pub fn from_body(status: u16, content_type: impl Into<String>, body: Body) -> Self {
        Self {
            status: StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            content_type: content_type.into(),
            body,
            usage: None,
            retry_count: 0,
        }
    }

    pub fn with_metrics(mut self, usage: Option<RequestUsageMetadata>, attempts: u8) -> Self {
        self.usage = usage;
        self.retry_count = attempts.saturating_sub(1);
        self
    }

    pub fn status(&self) -> u16 {
        self.status.as_u16()
    }

    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    pub fn usage(&self) -> Option<&RequestUsageMetadata> {
        self.usage.as_ref()
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

    fn into_response(self) -> Response {
        let mut response = Response::new(self.body);
        *response.status_mut() = self.status;
        if let Ok(value) = HeaderValue::from_str(&self.content_type) {
            response.headers_mut().insert("content-type", value);
        }
        response
            .headers_mut()
            .insert("cache-control", HeaderValue::from_static("no-store"));
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
}

#[derive(Clone)]
pub struct GatewayServerActivity(Arc<GatewayServerActivityInner>);

struct GatewayServerActivityInner {
    epoch: Instant,
    last_activity_ms: AtomicU64,
    inflight: AtomicUsize,
}

impl GatewayServerActivity {
    fn new() -> Self {
        Self(Arc::new(GatewayServerActivityInner {
            epoch: Instant::now(),
            last_activity_ms: AtomicU64::new(0),
            inflight: AtomicUsize::new(0),
        }))
    }

    pub fn inflight_requests(&self) -> usize {
        self.0.inflight.load(Ordering::Acquire)
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
        let listener = TcpListener::bind(config.bind_addr)
            .await
            .map_err(|_| AppError::new("GATEWAY_PORT_IN_USE", "Gateway port is unavailable"))?;
        let local_addr = listener
            .local_addr()
            .map_err(|_| AppError::new("GATEWAY_SERVER_IO", "Gateway socket identity failed"))?;
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
    dispatch(
        state,
        binding,
        request_id.0,
        request.uri().path().into(),
        Vec::new(),
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
    let body = match to_bytes(request.into_body(), state.config.max_body_bytes).await {
        Ok(body) => body.to_vec(),
        Err(_) => return gateway_error(StatusCode::PAYLOAD_TOO_LARGE, "GATEWAY_BODY_LIMIT"),
    };
    dispatch(state, binding, request_id.0, path, body).await
}

async fn dispatch(
    state: ServerState,
    binding: GatewayBindingSnapshot,
    request_id: String,
    path: String,
    body: Vec<u8>,
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
                })
            })
            .clone()
    };
    let immediate = state
        .inflight
        .clone()
        .try_acquire_owned()
        .ok()
        .and_then(|global| {
            binding_limits
                .inflight
                .clone()
                .try_acquire_owned()
                .ok()
                .map(|binding| (global, binding))
        });
    let (permit, binding_permit) = if let Some(permits) = immediate {
        permits
    } else {
        let queue = match QueueGuard::enter(
            state.queued.clone(),
            state.config.max_queue,
            binding_limits.queued.clone(),
            state.config.max_queue_per_binding,
        ) {
            Some(queue) => queue,
            None => {
                return gateway_error(StatusCode::TOO_MANY_REQUESTS, "GATEWAY_CONCURRENCY_LIMIT")
            }
        };
        let permits = tokio::time::timeout(state.config.request_timeout, async {
            let global = state.inflight.clone().acquire_owned().await;
            let binding = binding_limits.inflight.clone().acquire_owned().await;
            (global, binding)
        })
        .await;
        drop(queue);
        match permits {
            Ok((Ok(global), Ok(binding))) => (global, binding),
            _ => return gateway_error(StatusCode::GATEWAY_TIMEOUT, "GATEWAY_REQUEST_TIMEOUT"),
        }
    };
    let binding_hash = short_hash(&binding.binding_id);
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
        }),
    )
    .await;
    drop(permit);
    drop(binding_permit);
    let (response, usage, retry_count) = match result {
        Ok(Ok(response)) => {
            let usage = response.usage.clone();
            let retry_count = response.retry_count;
            (response.into_response(), usage, retry_count)
        }
        Ok(Err(error)) => (gateway_error(StatusCode::BAD_GATEWAY, &error.code), None, 0),
        Err(_) => (
            gateway_error(StatusCode::GATEWAY_TIMEOUT, "GATEWAY_REQUEST_TIMEOUT"),
            None,
            0,
        ),
    };
    state.observer.observe(RequestLogMetadata {
        request_id,
        route: path,
        status: response.status().as_u16(),
        latency_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        binding_hash,
        retry_count,
        usage,
    });
    response
}

struct QueueGuard {
    global: Arc<AtomicUsize>,
    binding: Arc<AtomicUsize>,
}

impl QueueGuard {
    fn enter(
        global: Arc<AtomicUsize>,
        global_limit: usize,
        binding: Arc<AtomicUsize>,
        binding_limit: usize,
    ) -> Option<Self> {
        if !try_increment_below(&global, global_limit) {
            return None;
        }
        if !try_increment_below(&binding, binding_limit) {
            global.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        Some(Self { global, binding })
    }
}

impl Drop for QueueGuard {
    fn drop(&mut self) {
        self.global.fetch_sub(1, Ordering::AcqRel);
        self.binding.fetch_sub(1, Ordering::AcqRel);
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
