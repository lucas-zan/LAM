use super::nonstream::{map_usage, ResponsesUsage};
use super::protocol::ChatUsage;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::time::Duration;

pub const MAX_RETRY_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    Validation,
    Auth,
    Capability,
    Upstream,
    Timeout,
    Cancelled,
    Internal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StableErrorCode {
    UsageInconsistent,
    UpstreamBadRequest,
    UpstreamAuthFailed,
    UpstreamTimeout,
    UpstreamConflict,
    UpstreamRateLimited,
    UpstreamUnavailable,
    UpstreamUnexpectedStatus,
    TransportConnectFailed,
    TransportTimeout,
    RequestCancelled,
    UpstreamDisconnected,
    UpstreamMalformedResponse,
    AdapterBufferOverflow,
    AdapterCapabilityUnsupported,
    AdapterInternal,
}

impl StableErrorCode {
    pub const ALL: &'static [Self] = &[
        Self::UsageInconsistent,
        Self::UpstreamBadRequest,
        Self::UpstreamAuthFailed,
        Self::UpstreamTimeout,
        Self::UpstreamConflict,
        Self::UpstreamRateLimited,
        Self::UpstreamUnavailable,
        Self::UpstreamUnexpectedStatus,
        Self::TransportConnectFailed,
        Self::TransportTimeout,
        Self::RequestCancelled,
        Self::UpstreamDisconnected,
        Self::UpstreamMalformedResponse,
        Self::AdapterBufferOverflow,
        Self::AdapterCapabilityUnsupported,
        Self::AdapterInternal,
    ];
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NormalizedError {
    pub code: StableErrorCode,
    pub category: ErrorCategory,
    pub message: String,
    pub retryable: bool,
    pub replay_safe: bool,
    pub upstream_status: Option<u16>,
    #[serde(skip)]
    pub retry_after: Option<Duration>,
    pub application_retry_count: u8,
}

pub fn normalize_usage(
    usage: Option<&ChatUsage>,
) -> Result<Option<ResponsesUsage>, NormalizedError> {
    let Some(usage) = usage else {
        return Ok(None);
    };
    if usage.prompt_tokens.saturating_add(usage.completion_tokens) != usage.total_tokens {
        return Err(error(
            StableErrorCode::UsageInconsistent,
            ErrorCategory::Upstream,
            "upstream token totals are inconsistent",
            false,
        ));
    }
    Ok(Some(map_usage(usage)))
}

pub fn normalize_upstream_http(
    status: u16,
    retry_after: Option<&str>,
    _upstream_body: &[u8],
    _after_first_byte: bool,
) -> NormalizedError {
    let (code, category, retryable, message) = match status {
        400 => (
            StableErrorCode::UpstreamBadRequest,
            ErrorCategory::Validation,
            false,
            "upstream rejected the translated request",
        ),
        401 | 403 => (
            StableErrorCode::UpstreamAuthFailed,
            ErrorCategory::Auth,
            false,
            "upstream authentication failed",
        ),
        408 => (
            StableErrorCode::UpstreamTimeout,
            ErrorCategory::Timeout,
            true,
            "upstream request timed out",
        ),
        409 => (
            StableErrorCode::UpstreamConflict,
            ErrorCategory::Upstream,
            true,
            "upstream reported a conflict",
        ),
        429 => (
            StableErrorCode::UpstreamRateLimited,
            ErrorCategory::Upstream,
            true,
            "upstream rate limit reached",
        ),
        500..=599 => (
            StableErrorCode::UpstreamUnavailable,
            ErrorCategory::Upstream,
            true,
            "upstream service is unavailable",
        ),
        _ => (
            StableErrorCode::UpstreamUnexpectedStatus,
            ErrorCategory::Upstream,
            false,
            "upstream returned an unexpected status",
        ),
    };
    let mut normalized = error(code, category, message, retryable);
    normalized.upstream_status = Some(status);
    normalized.retry_after = retry_after.and_then(|value| parse_retry_after(value, Utc::now()));
    normalized
}

pub fn parse_retry_after(value: &str, now: DateTime<Utc>) -> Option<Duration> {
    if let Ok(seconds) = value.trim().parse::<u64>() {
        return Some(Duration::from_secs(seconds).min(MAX_RETRY_AFTER));
    }
    let target = DateTime::parse_from_rfc2822(value.trim())
        .ok()?
        .with_timezone(&Utc);
    let seconds = target.signed_duration_since(now).num_seconds().max(0) as u64;
    Some(Duration::from_secs(seconds).min(MAX_RETRY_AFTER))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportFailure {
    Connect,
    Timeout,
    Cancelled,
    Disconnect,
    MalformedResponse,
    BufferOverflow,
}

pub fn normalize_transport_failure(
    kind: TransportFailure,
    after_first_byte: bool,
) -> NormalizedError {
    let (code, category, message, retryable) = match kind {
        TransportFailure::Connect => (
            StableErrorCode::TransportConnectFailed,
            ErrorCategory::Upstream,
            "could not connect to upstream",
            true,
        ),
        TransportFailure::Timeout => (
            StableErrorCode::TransportTimeout,
            ErrorCategory::Timeout,
            "upstream deadline expired",
            true,
        ),
        TransportFailure::Cancelled => (
            StableErrorCode::RequestCancelled,
            ErrorCategory::Cancelled,
            "request was cancelled",
            false,
        ),
        TransportFailure::Disconnect => (
            StableErrorCode::UpstreamDisconnected,
            ErrorCategory::Upstream,
            "upstream disconnected",
            true,
        ),
        TransportFailure::MalformedResponse => (
            StableErrorCode::UpstreamMalformedResponse,
            ErrorCategory::Upstream,
            "upstream response was malformed",
            false,
        ),
        TransportFailure::BufferOverflow => (
            StableErrorCode::AdapterBufferOverflow,
            ErrorCategory::Internal,
            "adapter buffer limit exceeded",
            false,
        ),
    };
    let mut normalized = error(code, category, message, retryable);
    normalized.replay_safe = kind == TransportFailure::Connect && !after_first_byte;
    normalized
}

fn error(
    code: StableErrorCode,
    category: ErrorCategory,
    message: &str,
    retryable: bool,
) -> NormalizedError {
    NormalizedError {
        code,
        category,
        message: message.into(),
        retryable,
        replay_safe: false,
        upstream_status: None,
        retry_after: None,
        application_retry_count: 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryAction {
    FixConfiguration,
    Reauthenticate,
    RetryLater,
    Cancelled,
    ReduceRequest,
    ReportProblem,
    Unknown,
}

pub fn recovery_action(code: StableErrorCode) -> RecoveryAction {
    match code {
        StableErrorCode::UpstreamBadRequest | StableErrorCode::AdapterCapabilityUnsupported => {
            RecoveryAction::FixConfiguration
        }
        StableErrorCode::UpstreamAuthFailed => RecoveryAction::Reauthenticate,
        StableErrorCode::UpstreamTimeout
        | StableErrorCode::UpstreamConflict
        | StableErrorCode::UpstreamRateLimited
        | StableErrorCode::UpstreamUnavailable
        | StableErrorCode::TransportConnectFailed
        | StableErrorCode::TransportTimeout
        | StableErrorCode::UpstreamDisconnected => RecoveryAction::RetryLater,
        StableErrorCode::RequestCancelled => RecoveryAction::Cancelled,
        StableErrorCode::AdapterBufferOverflow => RecoveryAction::ReduceRequest,
        StableErrorCode::UsageInconsistent
        | StableErrorCode::UpstreamUnexpectedStatus
        | StableErrorCode::UpstreamMalformedResponse
        | StableErrorCode::AdapterInternal => RecoveryAction::ReportProblem,
    }
}
