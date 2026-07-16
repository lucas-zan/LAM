use super::error::{AppError, Result};
use super::provider_credentials::SecretValue;
use super::provider_v2::join_upstream_endpoint;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fmt;
use std::io::Read;
use std::time::Duration;

pub const MAX_MODELS_RESPONSE_BYTES: usize = 1024 * 1024;
pub const MAX_DISCOVERED_MODELS: usize = 2048;
const MAX_MODEL_ID_BYTES: usize = 256;

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverProviderModelsRequestV2 {
    pub base_url: String,
    pub api_key: String,
}

impl fmt::Debug for DiscoverProviderModelsRequestV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DiscoverProviderModelsRequestV2")
            .field("base_url", &self.base_url)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredProviderModelV2 {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverProviderModelsViewV2 {
    pub models: Vec<DiscoveredProviderModelV2>,
}

pub fn parse_openai_model_list(body: &[u8]) -> Result<Vec<DiscoveredProviderModelV2>> {
    if body.len() > MAX_MODELS_RESPONSE_BYTES {
        return Err(discovery_error(
            "PROVIDER_MODEL_DISCOVERY_RESPONSE_LIMIT",
            "Provider models response exceeds 1 MiB",
        ));
    }
    let document: Value = serde_json::from_slice(body).map_err(|_| {
        discovery_error(
            "PROVIDER_MODEL_DISCOVERY_JSON_INVALID",
            "Provider models response is not valid JSON",
        )
    })?;
    let data = document
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            discovery_error(
                "PROVIDER_MODEL_DISCOVERY_SCHEMA_INVALID",
                "Provider models response must contain a data array",
            )
        })?;
    if data.is_empty() {
        return Err(discovery_error(
            "PROVIDER_MODEL_DISCOVERY_EMPTY",
            "Provider returned no models",
        ));
    }
    if data.len() > MAX_DISCOVERED_MODELS {
        return Err(discovery_error(
            "PROVIDER_MODEL_DISCOVERY_MODEL_LIMIT",
            "Provider returned too many models",
        ));
    }
    normalize_models(data)
}

fn normalize_models(data: &[Value]) -> Result<Vec<DiscoveredProviderModelV2>> {
    let mut ids = BTreeSet::new();
    for item in data {
        let id = item.get("id").and_then(Value::as_str).ok_or_else(|| {
            discovery_error(
                "PROVIDER_MODEL_DISCOVERY_MODEL_INVALID",
                "Provider model has no valid id",
            )
        })?;
        if id.is_empty()
            || id.trim() != id
            || id.len() > MAX_MODEL_ID_BYTES
            || id.chars().any(char::is_control)
        {
            return Err(discovery_error(
                "PROVIDER_MODEL_DISCOVERY_MODEL_INVALID",
                "Provider model id is invalid",
            ));
        }
        if !ids.insert(id.to_string()) {
            return Err(discovery_error(
                "PROVIDER_MODEL_DISCOVERY_MODEL_DUPLICATE",
                "Provider returned a duplicate model id",
            ));
        }
    }
    Ok(ids
        .into_iter()
        .map(|id| DiscoveredProviderModelV2 {
            label: id.clone(),
            id,
        })
        .collect())
}

pub fn discover_provider_models_service_v2(
    request: DiscoverProviderModelsRequestV2,
) -> Result<DiscoverProviderModelsViewV2> {
    if request.api_key.is_empty() || request.api_key.trim() != request.api_key {
        return Err(discovery_error(
            "PROVIDER_MODEL_DISCOVERY_CREDENTIAL_INVALID",
            "API key must be non-empty and contain no surrounding whitespace",
        ));
    }
    let endpoint = join_upstream_endpoint(&request.base_url, "/models")?;
    let token = SecretValue::from_sensitive(request.api_key);
    let body = fetch_models(&endpoint, &token)?;
    Ok(DiscoverProviderModelsViewV2 {
        models: parse_openai_model_list(&body)?,
    })
}

fn fetch_models(endpoint: &str, token: &SecretValue) -> Result<Vec<u8>> {
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| {
            discovery_error(
                "PROVIDER_MODEL_DISCOVERY_CLIENT_FAILED",
                "Provider model discovery client could not be created",
            )
        })?;
    let authorization = token
        .with_exposed(|value| reqwest::header::HeaderValue::from_str(&format!("Bearer {value}")));
    let mut response = client
        .get(endpoint)
        .header(
            reqwest::header::AUTHORIZATION,
            authorization.map_err(|_| {
                discovery_error(
                    "PROVIDER_MODEL_DISCOVERY_CREDENTIAL_INVALID",
                    "API key cannot be represented as an HTTP header",
                )
            })?,
        )
        .send()
        .map_err(|_| {
            discovery_error(
                "PROVIDER_MODEL_DISCOVERY_UNAVAILABLE",
                "Provider models endpoint is unavailable",
            )
        })?;
    if !response.status().is_success() {
        return Err(discovery_error(
            "PROVIDER_MODEL_DISCOVERY_HTTP_ERROR",
            &format!(
                "Provider models endpoint returned HTTP {}",
                response.status().as_u16()
            ),
        ));
    }
    read_bounded_body(&mut response)
}

fn read_bounded_body(response: &mut reqwest::blocking::Response) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MODELS_RESPONSE_BYTES as u64)
    {
        return Err(discovery_error(
            "PROVIDER_MODEL_DISCOVERY_RESPONSE_LIMIT",
            "Provider models response exceeds 1 MiB",
        ));
    }
    let mut body = Vec::new();
    response
        .take((MAX_MODELS_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|_| {
            discovery_error(
                "PROVIDER_MODEL_DISCOVERY_RESPONSE_INVALID",
                "Provider models response could not be read",
            )
        })?;
    if body.len() > MAX_MODELS_RESPONSE_BYTES {
        return Err(discovery_error(
            "PROVIDER_MODEL_DISCOVERY_RESPONSE_LIMIT",
            "Provider models response exceeds 1 MiB",
        ));
    }
    Ok(body)
}

fn discovery_error(code: &str, message: &str) -> AppError {
    AppError::new(code, message)
}
