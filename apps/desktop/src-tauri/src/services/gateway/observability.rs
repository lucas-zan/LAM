use crate::services::error::{AppError, Result};
use crate::services::provider_credentials::{
    validate_upstream_auth, CredentialSource, UpstreamAuth,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use url::Url;

const MIB: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GatewayTimeoutStage {
    Queue,
    Handler,
    UpstreamFirstByte,
    UpstreamIdle,
    UpstreamTotal,
    Other,
}

impl GatewayTimeoutStage {
    pub fn from_code(code: &str) -> Self {
        match code {
            "UPSTREAM_FIRST_BYTE_TIMEOUT" => Self::UpstreamFirstByte,
            "UPSTREAM_IDLE_TIMEOUT" => Self::UpstreamIdle,
            "UPSTREAM_TOTAL_TIMEOUT" => Self::UpstreamTotal,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GatewayRejectionReason {
    GlobalQueueFull,
    BindingQueueFull,
    UpstreamConcurrency,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct GatewayActivitySnapshot {
    pub inflight_requests: usize,
    pub running_requests: usize,
    pub queued_requests: usize,
    pub queued_body_bytes: usize,
    pub active_streams: usize,
    pub active_upstream_streams: usize,
    pub client_cancellations: u64,
    pub stream_interruptions: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stage5LoadRecommendations {
    pub global_queued_body_bytes: usize,
    pub binding_queued_body_bytes: usize,
    pub queue_timeout: Duration,
}

impl Default for Stage5LoadRecommendations {
    fn default() -> Self {
        Self {
            global_queued_body_bytes: 32 * MIB,
            binding_queued_body_bytes: 8 * MIB,
            queue_timeout: Duration::from_secs(30),
        }
    }
}

pub fn queued_body_envelope(max_body_bytes: usize, queue_slots: usize) -> Result<usize> {
    max_body_bytes.checked_mul(queue_slots).ok_or_else(|| {
        AppError::new(
            "GATEWAY_LOAD_ENVELOPE_OVERFLOW",
            "Gateway queue load envelope overflowed",
        )
    })
}

pub fn capacity_identity_hash(base_url: &str, auth: &UpstreamAuth) -> Result<String> {
    validate_upstream_auth(auth)?;
    let endpoint = canonical_endpoint(base_url)?;
    let identity = format!("{endpoint}|{}", canonical_auth(auth));
    let digest = Sha256::digest(identity.as_bytes());
    Ok(hex::encode(&digest[..8]))
}

fn canonical_endpoint(value: &str) -> Result<String> {
    let url = Url::parse(value).map_err(|_| capacity_identity_invalid())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(capacity_identity_invalid());
    }
    let host = url.host_str().ok_or_else(capacity_identity_invalid)?;
    let port = url
        .port_or_known_default()
        .ok_or_else(capacity_identity_invalid)?;
    let path = url.path().trim_end_matches('/');
    Ok(format!("https://{host}:{port}{path}"))
}

fn canonical_auth(auth: &UpstreamAuth) -> String {
    match auth {
        UpstreamAuth::Bearer { source } => format!("bearer|{}", canonical_source(source)),
        UpstreamAuth::Header { name, source } => {
            format!(
                "header|{}|{}",
                name.to_ascii_lowercase(),
                canonical_source(source)
            )
        }
        UpstreamAuth::None => "none".into(),
    }
}

fn canonical_source(source: &CredentialSource) -> String {
    match source {
        CredentialSource::Env { env_key } => format!("env|{env_key}"),
        CredentialSource::Keychain {
            service,
            account,
            version,
        } => format!("keychain|{service}|{account}|{version}"),
        CredentialSource::AuthCommand { approval_id } => format!("command|{approval_id}"),
        CredentialSource::CodexProfile { profile_id } => format!("profile|{profile_id}"),
        CredentialSource::None => "none".into(),
    }
}

fn capacity_identity_invalid() -> AppError {
    AppError::new(
        "GATEWAY_CAPACITY_IDENTITY_INVALID",
        "Gateway capacity identity input is invalid",
    )
}
