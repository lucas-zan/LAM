use crate::services::error::{AppError, Result};
use crate::services::provider_credentials::{
    resolve_credential, CredentialSource, ProcessEnvironment, SecretValue, UpstreamAuth,
};
use crate::services::provider_keychain::{
    KeychainBackend, KeychainCredentialReference, KeychainCredentialService,
};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue};
use serde_json::json;
use std::fmt;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OwnedSemaphorePermit;
use tokio::sync::{Notify, Semaphore};
use tokio::time::Instant;
use url::{Host, Url};

const CODEX_UPSTREAM_HEADER_NAMES: [&str; 6] = [
    "accept",
    "originator",
    "session-id",
    "thread-id",
    "user-agent",
    "x-client-request-id",
];
const MAX_CODEX_UPSTREAM_HEADER_COUNT: usize = 32;
const MAX_CODEX_UPSTREAM_HEADER_VALUE_BYTES: usize = 16 * 1024;
const MAX_CODEX_UPSTREAM_HEADERS_BYTES: usize = 32 * 1024;

#[derive(Clone, Default)]
pub struct CodexUpstreamHeaders(HeaderMap);

impl CodexUpstreamHeaders {
    pub fn capture(source: &HeaderMap) -> Self {
        let mut captured = HeaderMap::new();
        let mut total = 0_usize;
        for (name, value) in source {
            if captured.len() >= MAX_CODEX_UPSTREAM_HEADER_COUNT
                || !codex_header_allowed(name.as_str())
            {
                continue;
            }
            let bytes = name.as_str().len().saturating_add(value.as_bytes().len());
            if value.as_bytes().len() > MAX_CODEX_UPSTREAM_HEADER_VALUE_BYTES
                || total.saturating_add(bytes) > MAX_CODEX_UPSTREAM_HEADERS_BYTES
            {
                continue;
            }
            captured.insert(name.clone(), value.clone());
            total += bytes;
        }
        Self(captured)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).and_then(|value| value.to_str().ok())
    }

    fn iter(&self) -> impl Iterator<Item = (&HeaderName, &HeaderValue)> {
        self.0.iter()
    }
}

fn codex_header_allowed(name: &str) -> bool {
    CODEX_UPSTREAM_HEADER_NAMES.contains(&name) || name.starts_with("x-codex-")
}

impl fmt::Debug for CodexUpstreamHeaders {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexUpstreamHeaders")
            .field("count", &self.len())
            .finish()
    }
}

pub trait NetworkTargetPolicy: Send + Sync {
    fn validate_and_resolve<'a>(
        &'a self,
        url: &'a Url,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<SocketAddr>>> + Send + 'a>>;
}

pub struct ProductionNetworkTargetPolicy;

impl NetworkTargetPolicy for ProductionNetworkTargetPolicy {
    fn validate_and_resolve<'a>(
        &'a self,
        url: &'a Url,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<SocketAddr>>> + Send + 'a>> {
        Box::pin(async move {
            validate_parsed_upstream_url(url)?;
            let host = url.host_str().ok_or_else(target_forbidden)?;
            let port = url.port_or_known_default().ok_or_else(target_forbidden)?;
            let resolved = tokio::net::lookup_host((host, port))
                .await
                .map_err(|_| {
                    AppError::new("UPSTREAM_DNS_FAILED", "upstream DNS resolution failed")
                })?
                .collect::<Vec<_>>();
            if resolved.is_empty()
                || resolved
                    .iter()
                    .any(|address| upstream_ip_is_forbidden(address.ip()))
            {
                return Err(target_forbidden());
            }
            Ok(resolved)
        })
    }
}

pub trait UpstreamCredentialResolver: Send + Sync {
    fn resolve(&self, source: &CredentialSource) -> Result<Option<SecretValue>>;
}

pub struct ProcessCredentialResolver;

impl UpstreamCredentialResolver for ProcessCredentialResolver {
    fn resolve(&self, source: &CredentialSource) -> Result<Option<SecretValue>> {
        match source {
            CredentialSource::None => Ok(None),
            CredentialSource::Env { .. } => {
                resolve_credential(source, &ProcessEnvironment).map(Some)
            }
            _ => Err(AppError::new(
                "PROVIDER_CREDENTIAL_SOURCE_UNSUPPORTED",
                "sidecar credential resolver requires an environment credential",
            )),
        }
    }
}

pub struct KeychainAndEnvironmentCredentialResolver<B: KeychainBackend> {
    keychain: KeychainCredentialService<B>,
}

impl<B: KeychainBackend> KeychainAndEnvironmentCredentialResolver<B> {
    pub fn new(keychain: KeychainCredentialService<B>) -> Self {
        Self { keychain }
    }
}

impl<B: KeychainBackend> UpstreamCredentialResolver
    for KeychainAndEnvironmentCredentialResolver<B>
{
    fn resolve(&self, source: &CredentialSource) -> Result<Option<SecretValue>> {
        match source {
            CredentialSource::None => Ok(None),
            CredentialSource::Env { .. } => {
                resolve_credential(source, &ProcessEnvironment).map(Some)
            }
            CredentialSource::Keychain { .. } => {
                let reference = KeychainCredentialReference::try_from(source)?;
                self.keychain
                    .with_secret(&reference, |value| {
                        SecretValue::from_sensitive(value.to_owned())
                    })
                    .map(Some)
            }
            CredentialSource::AuthCommand { .. } => Err(AppError::new(
                "PROVIDER_CREDENTIAL_SOURCE_UNSUPPORTED",
                "auth-command credentials cannot cross the Gateway process boundary",
            )),
            CredentialSource::CodexProfile { .. } => Err(AppError::new(
                "PROVIDER_CREDENTIAL_SOURCE_UNSUPPORTED",
                "profile-owned Codex credentials cannot cross the Gateway process boundary",
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpstreamClientConfig {
    pub connect_timeout: Duration,
    pub first_byte_timeout: Duration,
    pub stream_idle_timeout: Duration,
    pub total_timeout: Duration,
    pub max_response_bytes: usize,
    pub max_inflight: usize,
    /// Maximum extra connection-establishment attempts after the first try.
    /// Only connection-phase failures (the request body never reached the
    /// upstream) are retried; failures after the request was sent are never
    /// retried automatically.
    pub connect_retries: u8,
    /// Delay between connection retry attempts.
    pub connect_retry_delay: Duration,
    /// Maximum extra attempts when the upstream accepted the connection but
    /// did not produce the first response byte in time. The request may have
    /// reached the upstream, so this is deliberately capped low to limit the
    /// risk of duplicate processing.
    pub first_byte_retries: u8,
}

impl UpstreamClientConfig {
    pub fn for_test() -> Self {
        Self {
            connect_timeout: Duration::from_millis(100),
            first_byte_timeout: Duration::from_secs(1),
            stream_idle_timeout: Duration::from_secs(1),
            total_timeout: Duration::from_secs(2),
            max_response_bytes: 1024 * 1024,
            max_inflight: 4,
            connect_retries: 0,
            connect_retry_delay: Duration::from_millis(50),
            first_byte_retries: 0,
        }
    }
}

#[derive(Clone, Default)]
pub struct GatewayCancellation {
    inner: Arc<CancellationState>,
}

#[derive(Default)]
struct CancellationState {
    cancelled: AtomicBool,
    notify: Notify,
}

impl GatewayCancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        if !self.inner.cancelled.swap(true, Ordering::SeqCst) {
            self.inner.notify.notify_waiters();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.inner.cancelled.load(Ordering::SeqCst)
    }

    pub async fn cancelled(&self) {
        let notified = self.inner.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if !self.is_cancelled() {
            notified.await;
        }
    }
}

pub struct UpstreamRequest {
    pub base_url: String,
    pub controlled_path: String,
    pub auth: UpstreamAuth,
    pub body: Vec<u8>,
    pub content_type: String,
    pub codex_headers: CodexUpstreamHeaders,
    pub cancellation: GatewayCancellation,
}

pub struct UpstreamResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub retry_after: Option<String>,
    pub body: Vec<u8>,
    pub attempts: u8,
    pub first_byte_ms: u64,
}

pub struct UpstreamStreamResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub retry_after: Option<String>,
    pub attempts: u8,
    pub first_byte_ms: u64,
    response: reqwest::Response,
    deadline: Instant,
    idle_timeout: Duration,
    max_response_bytes: usize,
    seen_bytes: usize,
    cancellation: GatewayCancellation,
    _permit: OwnedSemaphorePermit,
}

impl UpstreamStreamResponse {
    pub async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(timeout_error("UPSTREAM_TOTAL_TIMEOUT", self.attempts));
        }
        let idle = self.idle_timeout.min(remaining);
        let chunk = tokio::select! {
            _ = self.cancellation.cancelled() => return Err(cancelled()),
            result = tokio::time::timeout(idle, self.response.chunk()) => result,
        };
        match chunk {
            Ok(Ok(Some(chunk))) => {
                if self.seen_bytes.saturating_add(chunk.len()) > self.max_response_bytes {
                    return Err(AppError::new(
                        "GATEWAY_RESPONSE_LIMIT",
                        "upstream response exceeded its byte limit",
                    ));
                }
                self.seen_bytes += chunk.len();
                Ok(Some(chunk.to_vec()))
            }
            Ok(Ok(None)) => Ok(None),
            Ok(Err(_)) => Err(transport_error(self.attempts)),
            Err(_) => Err(timeout_error("UPSTREAM_IDLE_TIMEOUT", self.attempts)),
        }
    }
}

impl fmt::Debug for UpstreamResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UpstreamResponse")
            .field("status", &self.status)
            .field("content_type", &self.content_type)
            .field("retry_after", &self.retry_after)
            .field("body_bytes", &self.body.len())
            .field("attempts", &self.attempts)
            .field("first_byte_ms", &self.first_byte_ms)
            .finish()
    }
}

pub struct SecureUpstreamClient {
    config: UpstreamClientConfig,
    policy: Arc<dyn NetworkTargetPolicy>,
    credentials: Arc<dyn UpstreamCredentialResolver>,
    inflight: Arc<Semaphore>,
}

impl SecureUpstreamClient {
    pub fn new(
        config: UpstreamClientConfig,
        policy: Arc<dyn NetworkTargetPolicy>,
        credentials: Arc<dyn UpstreamCredentialResolver>,
    ) -> Result<Self> {
        if config.max_response_bytes == 0
            || config.max_inflight == 0
            || config.connect_timeout.is_zero()
            || config.first_byte_timeout.is_zero()
            || config.stream_idle_timeout.is_zero()
            || config.total_timeout.is_zero()
            || config.connect_retry_delay.is_zero()
        {
            return Err(AppError::new(
                "UPSTREAM_CLIENT_LIMIT_INVALID",
                "upstream client limits must be non-zero",
            ));
        }
        Ok(Self {
            inflight: Arc::new(Semaphore::new(config.max_inflight)),
            config,
            policy,
            credentials,
        })
    }

    pub async fn send(&self, request: UpstreamRequest) -> Result<UpstreamResponse> {
        let mut response = self.open_stream(request).await?;
        let mut body = Vec::new();
        while let Some(chunk) = response.next_chunk().await? {
            body.extend_from_slice(&chunk);
        }
        Ok(UpstreamResponse {
            status: response.status,
            content_type: response.content_type,
            retry_after: response.retry_after,
            body,
            attempts: response.attempts,
            first_byte_ms: response.first_byte_ms,
        })
    }

    pub async fn open_stream(&self, request: UpstreamRequest) -> Result<UpstreamStreamResponse> {
        let started = Instant::now();
        if request.cancellation.is_cancelled() {
            return Err(cancelled());
        }
        let permit = self.inflight.clone().try_acquire_owned().map_err(|_| {
            AppError::new(
                "GATEWAY_UPSTREAM_CONCURRENCY_LIMIT",
                "upstream concurrency limit reached",
            )
        })?;
        let url = join_controlled_path(&request.base_url, &request.controlled_path)?;
        let host = url.host_str().ok_or_else(target_forbidden)?.to_owned();
        let auth = self.resolve_auth(&request.auth)?;
        let deadline = Instant::now() + self.config.total_timeout;
        let mut attempts = 0_u8;
        let mut first_byte_attempts = 0_u8;
        let mut client = None;
        let response = loop {
            attempts += 1;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(timeout_error("UPSTREAM_TOTAL_TIMEOUT", attempts));
            }
            // DNS resolution happens before any request is sent, so a transient
            // DNS failure is safe to retry within the connect budget.
            let addresses = tokio::select! {
                _ = request.cancellation.cancelled() => return Err(cancelled()),
                result = self.policy.validate_and_resolve(&url) => result,
            };
            let addresses = match addresses {
                Ok(addresses) if addresses.is_empty() => {
                    let err =
                        AppError::new("UPSTREAM_DNS_FAILED", "upstream DNS returned no addresses");
                    if attempts <= self.config.connect_retries {
                        let delay = self.config.connect_retry_delay.min(remaining);
                        tokio::select! {
                            _ = request.cancellation.cancelled() => return Err(cancelled()),
                            _ = tokio::time::sleep(delay) => {}
                        }
                        continue;
                    }
                    return Err(err);
                }
                Ok(addresses) => addresses,
                Err(error)
                    if error.code == "UPSTREAM_DNS_FAILED"
                        && attempts <= self.config.connect_retries =>
                {
                    let delay = self.config.connect_retry_delay.min(remaining);
                    tokio::select! {
                        _ = request.cancellation.cancelled() => return Err(cancelled()),
                        _ = tokio::time::sleep(delay) => {}
                    }
                    continue;
                }
                Err(error) => return Err(error),
            };
            if client.is_none() {
                client = Some(
                    reqwest::Client::builder()
                        .redirect(reqwest::redirect::Policy::none())
                        .no_proxy()
                        .connect_timeout(self.config.connect_timeout)
                        .pool_max_idle_per_host(4)
                        .resolve_to_addrs(&host, &addresses)
                        .build()
                        .map_err(|_| {
                            AppError::new(
                                "UPSTREAM_CLIENT_BUILD_FAILED",
                                "upstream client initialization failed",
                            )
                        })?,
                );
            }
            let client = client.as_ref().expect("client built above");
            let mut builder = client.post(url.clone()).body(request.body.clone());
            for (name, value) in request.codex_headers.iter() {
                builder = builder.header(name, value);
            }
            builder = builder.header(header::CONTENT_TYPE, request.content_type.clone());
            if let Some((name, value)) = &auth {
                builder = builder.header(name, value);
            }
            let first_byte = self.config.first_byte_timeout.min(remaining);
            let result = tokio::select! {
                _ = request.cancellation.cancelled() => return Err(cancelled()),
                result = tokio::time::timeout(first_byte, builder.send()) => result,
            };
            match result {
                Ok(Ok(response)) => break response,
                Ok(Err(error)) if error.is_connect() && attempts <= self.config.connect_retries => {
                    // Connection-phase failure: the request body never reached the
                    // upstream, so retrying is safe. Back off briefly before the
                    // next attempt while staying inside total_timeout/cancellation.
                    let delay = self.config.connect_retry_delay.min(remaining);
                    tokio::select! {
                        _ = request.cancellation.cancelled() => return Err(cancelled()),
                        _ = tokio::time::sleep(delay) => {}
                    }
                    continue;
                }
                Ok(Err(_)) => return Err(transport_error(attempts)),
                Err(_) => {
                    // The connection was accepted but no first byte arrived.
                    // The request may have reached the upstream, so allow only
                    // a small, separate retry budget.
                    first_byte_attempts += 1;
                    if first_byte_attempts <= self.config.first_byte_retries {
                        let delay = self.config.connect_retry_delay.min(remaining);
                        tokio::select! {
                            _ = request.cancellation.cancelled() => return Err(cancelled()),
                            _ = tokio::time::sleep(delay) => {}
                        }
                        continue;
                    }
                    return Err(timeout_error("UPSTREAM_FIRST_BYTE_TIMEOUT", attempts));
                }
            }
        };
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let retry_after = response
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        Ok(UpstreamStreamResponse {
            status,
            content_type,
            retry_after,
            attempts,
            first_byte_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            response,
            deadline,
            idle_timeout: self.config.stream_idle_timeout,
            max_response_bytes: self.config.max_response_bytes,
            seen_bytes: 0,
            cancellation: request.cancellation,
            _permit: permit,
        })
    }

    fn resolve_auth(&self, auth: &UpstreamAuth) -> Result<Option<(HeaderName, HeaderValue)>> {
        match auth {
            UpstreamAuth::None => Ok(None),
            UpstreamAuth::Bearer { source } => {
                let secret = required_secret(self.credentials.resolve(source)?)?;
                secret.with_exposed(|value| {
                    let value = HeaderValue::from_str(&format!("Bearer {value}"))
                        .map_err(|_| credential_invalid())?;
                    Ok(Some((header::AUTHORIZATION, value)))
                })
            }
            UpstreamAuth::Header { name, source } => {
                let name =
                    HeaderName::from_bytes(name.as_bytes()).map_err(|_| credential_invalid())?;
                let secret = required_secret(self.credentials.resolve(source)?)?;
                secret.with_exposed(|value| {
                    let value = HeaderValue::from_str(value).map_err(|_| credential_invalid())?;
                    Ok(Some((name, value)))
                })
            }
        }
    }
}

pub fn join_controlled_path(base_url: &str, controlled_path: &str) -> Result<Url> {
    let mut base = Url::parse(base_url).map_err(|_| target_forbidden())?;
    if base.cannot_be_a_base() || base.query().is_some() || base.fragment().is_some() {
        return Err(target_forbidden());
    }
    let path = controlled_path.trim_start_matches('/');
    let normalized = path.to_ascii_lowercase();
    if path.is_empty()
        || path.contains(['?', '#', '\\'])
        || path.chars().any(char::is_control)
        || path.split('/').any(|segment| {
            matches!(segment, "" | "." | "..")
                || matches!(
                    segment.to_ascii_lowercase().as_str(),
                    "%2e" | "%2e%2e" | ".%2e" | "%2e."
                )
        })
        || normalized.contains("%2f")
        || normalized.contains("%5c")
    {
        return Err(AppError::new(
            "UPSTREAM_PATH_INVALID",
            "controlled upstream path is invalid",
        ));
    }
    let prefix = base.path().trim_end_matches('/');
    base.set_path(&format!("{prefix}/{path}"));
    Ok(base)
}

pub fn validate_upstream_url(value: &str) -> Result<Url> {
    let url = Url::parse(value).map_err(|_| target_forbidden())?;
    validate_parsed_upstream_url(&url)?;
    Ok(url)
}

fn validate_parsed_upstream_url(url: &Url) -> Result<()> {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.port().is_some_and(|port| port != 443)
        || url.as_str().chars().any(char::is_control)
    {
        return Err(target_forbidden());
    }
    let Host::Domain(host) = url.host().ok_or_else(target_forbidden)? else {
        return Err(target_forbidden());
    };
    let host = host.to_ascii_lowercase();
    if !host.contains('.')
        || host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host == "metadata.google.internal"
    {
        return Err(target_forbidden());
    }
    Ok(())
}

pub fn upstream_ip_is_forbidden(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => forbidden_ipv4(ip),
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.to_ipv4().is_some_and(forbidden_ipv4)
                || documentation_ipv6(ip)
        }
    }
}

fn forbidden_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || octets[0] == 0
        || octets[0] >= 240
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
}

fn documentation_ipv6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    segments[0] == 0x2001 && segments[1] == 0x0db8
}

fn required_secret(value: Option<SecretValue>) -> Result<SecretValue> {
    value.ok_or_else(|| {
        AppError::new(
            "PROVIDER_CREDENTIAL_MISSING",
            "upstream credential is missing",
        )
    })
}

fn target_forbidden() -> AppError {
    AppError::new(
        "UPSTREAM_NETWORK_TARGET_FORBIDDEN",
        "upstream network target is forbidden by policy",
    )
}

fn credential_invalid() -> AppError {
    AppError::new(
        "PROVIDER_CREDENTIAL_INVALID",
        "upstream credential cannot be represented as an HTTP header",
    )
}

fn cancelled() -> AppError {
    AppError::new("GATEWAY_REQUEST_CANCELLED", "Gateway request was cancelled")
}

fn transport_error(attempts: u8) -> AppError {
    AppError {
        code: "UPSTREAM_TRANSPORT_FAILED".into(),
        message: "upstream transport failed".into(),
        recoverable: true,
        details: Some(json!({ "attempts": attempts })),
    }
}

fn timeout_error(code: &str, attempts: u8) -> AppError {
    AppError {
        code: code.into(),
        message: "upstream request timed out".into(),
        recoverable: true,
        details: Some(json!({ "attempts": attempts })),
    }
}
