use super::error::{AppError, Result};
use super::provider_credentials::{
    validate_codex_security_options, validate_upstream_auth, CredentialSource, UpstreamAuth,
};
use super::storage::{StoreSnapshot, VersionedFileStore};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use url::Url;

const RESERVED_PROVIDER_IDS: &[&str] = &["openai", "ollama", "lmstudio"];
const RESPONSES_CHAT_ADAPTER: &str = "responses_to_chat_completions";
const COMPATIBILITY_PROFILES: &[&str] = &["openai_chat_completions", "deepseek_chat_completions"];

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderProtocol {
    Responses,
    ChatCompletions,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySupport {
    Supported,
    Partial,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityDeclaration {
    pub streaming: CapabilitySupport,
    pub function_tools: CapabilitySupport,
    pub structured_outputs: CapabilitySupport,
    pub reasoning: CapabilitySupport,
}

impl Default for CapabilityDeclaration {
    fn default() -> Self {
        Self {
            streaming: CapabilitySupport::Unknown,
            function_tools: CapabilitySupport::Unknown,
            structured_outputs: CapabilitySupport::Unknown,
            reasoning: CapabilitySupport::Unknown,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModel {
    pub id: String,
    pub label: String,
    pub capabilities: Option<CapabilityDeclaration>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AdapterConfig {
    None,
    Local {
        adapter_id: String,
        upstream_path: String,
    },
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexProviderOptions {
    pub display_name: Option<String>,
    pub stream_idle_timeout_ms: Option<u64>,
    pub direct_request_max_retries: Option<u8>,
    pub direct_stream_max_retries: Option<u8>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub route_via_gateway: bool,
    pub query_params: BTreeMap<String, String>,
    pub env_http_headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInput {
    pub id: String,
    pub name: String,
    pub protocol: ProviderProtocol,
    pub base_url: String,
    pub default_model: String,
    pub models: Vec<ProviderModel>,
    pub upstream_auth: UpstreamAuth,
    pub adapter: AdapterConfig,
    pub compatibility_profile: Option<String>,
    pub codex: CodexProviderOptions,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfileV2 {
    pub id: String,
    pub name: String,
    pub protocol: ProviderProtocol,
    pub base_url: String,
    pub default_model: String,
    pub models: Vec<ProviderModel>,
    pub upstream_auth: UpstreamAuth,
    pub capabilities: CapabilityDeclaration,
    pub adapter: AdapterConfig,
    pub compatibility_profile: Option<String>,
    pub codex: CodexProviderOptions,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProviderCollection {
    pub providers: Vec<ProviderProfileV2>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyProviderMigration {
    pub providers: ProviderCollection,
    pub warnings: Vec<String>,
}

#[derive(Clone)]
pub struct ProviderRepository {
    store: VersionedFileStore<ProviderCollection>,
}

impl ProviderRepository {
    pub fn new(store: VersionedFileStore<ProviderCollection>) -> Self {
        Self { store }
    }

    pub fn load(&self) -> Result<StoreSnapshot<ProviderCollection>> {
        self.store.load_or_default()
    }

    pub fn create(
        &self,
        expected_revision: u64,
        input: ProviderInput,
        now: &str,
    ) -> Result<StoreSnapshot<ProviderCollection>> {
        let provider = build_provider(input, now)?;
        let mut snapshot = self.load()?;
        if snapshot
            .value
            .providers
            .iter()
            .any(|item| item.id == provider.id)
        {
            return Err(AppError::new("PROVIDER_ALREADY_EXISTS", provider.id));
        }
        snapshot.value.providers.push(provider);
        sort_providers(&mut snapshot.value.providers);
        self.store
            .compare_and_swap(expected_revision, &snapshot.value)
    }

    pub fn update(
        &self,
        expected_revision: u64,
        input: ProviderInput,
        now: &str,
    ) -> Result<StoreSnapshot<ProviderCollection>> {
        let mut replacement = build_provider(input, now)?;
        let mut snapshot = self.load()?;
        let current = snapshot
            .value
            .providers
            .iter_mut()
            .find(|item| item.id == replacement.id)
            .ok_or_else(|| AppError::new("PROVIDER_NOT_FOUND", &replacement.id))?;
        replacement.created_at = current.created_at.clone();
        *current = replacement;
        sort_providers(&mut snapshot.value.providers);
        self.store
            .compare_and_swap(expected_revision, &snapshot.value)
    }

    pub fn delete(
        &self,
        expected_revision: u64,
        provider_id: &str,
    ) -> Result<StoreSnapshot<ProviderCollection>> {
        let mut snapshot = self.load()?;
        let before = snapshot.value.providers.len();
        snapshot
            .value
            .providers
            .retain(|provider| provider.id != provider_id);
        if snapshot.value.providers.len() == before {
            return Err(AppError::new("PROVIDER_NOT_FOUND", provider_id));
        }
        self.store
            .compare_and_swap(expected_revision, &snapshot.value)
    }

    pub fn replace_credential_source(
        &self,
        expected_store_revision: u64,
        provider_id: &str,
        expected_source: &CredentialSource,
        replacement: CredentialSource,
        now: &str,
    ) -> Result<StoreSnapshot<ProviderCollection>> {
        validate_timestamp(now)?;
        let mut snapshot = self.load()?;
        let provider = snapshot
            .value
            .providers
            .iter_mut()
            .find(|item| item.id == provider_id)
            .ok_or_else(|| AppError::new("PROVIDER_NOT_FOUND", provider_id))?;
        let source = match &mut provider.upstream_auth {
            UpstreamAuth::Bearer { source } | UpstreamAuth::Header { source, .. } => source,
            UpstreamAuth::None => {
                return Err(AppError::new(
                    "PROVIDER_AUTH_ROUTE_UNSUPPORTED",
                    "unauthenticated Provider has no credential reference",
                ))
            }
        };
        if source != expected_source {
            return Err(AppError::new(
                "PROVIDER_CREDENTIAL_CONFLICT",
                "Provider credential reference changed",
            ));
        }
        *source = replacement;
        validate_upstream_auth(&provider.upstream_auth)?;
        provider.updated_at = now.into();
        self.store
            .compare_and_swap(expected_store_revision, &snapshot.value)
    }
}

pub fn build_provider(input: ProviderInput, now: &str) -> Result<ProviderProfileV2> {
    validate_timestamp(now)?;
    let id = validate_provider_id(&input.id)?;
    let name = required_trimmed(&input.name, "name")?;
    let base_url = canonicalize_base_url(&input.base_url)?;
    let (models, default_model) = validate_models(input.models, &input.default_model)?;
    let adapter = validate_adapter(input.protocol, input.adapter)?;
    let compatibility_profile = validate_compatibility(input.compatibility_profile)?;
    validate_upstream_auth(&input.upstream_auth)?;
    validate_codex_options(&input.codex)?;
    Ok(ProviderProfileV2 {
        id,
        name,
        protocol: input.protocol,
        base_url,
        default_model,
        models,
        upstream_auth: input.upstream_auth,
        capabilities: CapabilityDeclaration::default(),
        adapter,
        compatibility_profile,
        codex: input.codex,
        created_at: now.to_string(),
        updated_at: now.to_string(),
    })
}

pub fn migrate_legacy_provider_array(bytes: &[u8], now: &str) -> Result<LegacyProviderMigration> {
    validate_timestamp(now)?;
    let legacy: Vec<LegacyProvider> = serde_json::from_slice(bytes)
        .map_err(|error| AppError::new("PROVIDER_STORE_INVALID", error.to_string()))?;
    let mut providers = Vec::with_capacity(legacy.len());
    let mut warnings = Vec::new();
    for item in legacy {
        if item.wire_api == "openai" {
            warnings.push("LEGACY_WIRE_OPENAI_MAPPED_TO_RESPONSES".into());
        } else if item.wire_api != "responses" {
            return Err(AppError::new("PROVIDER_PROTOCOL_UNKNOWN", item.wire_api));
        }
        let upstream_auth = match (item.secret_storage.as_str(), item.env_key) {
            ("env", Some(env_key)) => UpstreamAuth::Bearer {
                source: CredentialSource::Env { env_key },
            },
            ("none", _) => UpstreamAuth::None,
            (kind, _) => return Err(AppError::new("PROVIDER_CREDENTIAL_UNKNOWN", kind)),
        };
        let model = ProviderModel {
            id: item.default_model.clone(),
            label: item.default_model.clone(),
            capabilities: None,
        };
        providers.push(build_provider(
            ProviderInput {
                id: item.id,
                name: item.name,
                protocol: ProviderProtocol::Responses,
                base_url: item.base_url,
                default_model: item.default_model,
                models: vec![model],
                upstream_auth,
                adapter: AdapterConfig::None,
                compatibility_profile: None,
                codex: CodexProviderOptions::default(),
            },
            now,
        )?);
    }
    sort_providers(&mut providers);
    warnings.sort();
    warnings.dedup();
    Ok(LegacyProviderMigration {
        providers: ProviderCollection { providers },
        warnings,
    })
}

pub fn join_upstream_endpoint(base_url: &str, upstream_path: &str) -> Result<String> {
    let base = canonicalize_base_url(base_url)?;
    let path = validate_upstream_path(upstream_path)?;
    Ok(format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyProvider {
    id: String,
    name: String,
    base_url: String,
    wire_api: String,
    default_model: String,
    env_key: Option<String>,
    secret_storage: String,
    #[allow(dead_code)]
    health: String,
}

fn validate_provider_id(input: &str) -> Result<String> {
    let id = input.trim();
    if id.is_empty() || id.len() > 64 {
        return Err(AppError::new("PROVIDER_INVALID_ID", input));
    }
    let mut chars = id.chars();
    if !chars
        .next()
        .is_some_and(|first| first.is_ascii_alphanumeric())
        || !chars.all(|value| value.is_ascii_alphanumeric() || "_-.".contains(value))
    {
        return Err(AppError::new("PROVIDER_INVALID_ID", input));
    }
    if RESERVED_PROVIDER_IDS.contains(&id) {
        return Err(AppError::new("PROVIDER_RESERVED_ID", id));
    }
    Ok(id.to_string())
}

fn canonicalize_base_url(input: &str) -> Result<String> {
    let mut url = Url::parse(input.trim())
        .map_err(|error| AppError::new("PROVIDER_URL_INVALID", error.to_string()))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AppError::new(
            "PROVIDER_URL_USERINFO",
            "URL userinfo is forbidden",
        ));
    }
    let loopback_http =
        url.scheme() == "http" && matches!(url.host_str(), Some("127.0.0.1" | "::1" | "localhost"));
    if url.scheme() != "https" && !loopback_http {
        return Err(AppError::new(
            "PROVIDER_URL_INSECURE",
            "remote Provider base URL must use HTTPS",
        ));
    }
    if url.host_str().is_none() || url.query().is_some() || url.fragment().is_some() {
        return Err(AppError::new(
            "PROVIDER_URL_INVALID",
            "Provider base URL requires a host and no query or fragment",
        ));
    }
    let normalized_path = if url.path() == "/" {
        String::new()
    } else {
        url.path().trim_end_matches('/').to_string()
    };
    url.set_path(&normalized_path);
    Ok(url.to_string().trim_end_matches('/').to_string())
}

fn validate_models(
    mut models: Vec<ProviderModel>,
    default_model: &str,
) -> Result<(Vec<ProviderModel>, String)> {
    let default_model = required_trimmed(default_model, "default_model")?;
    let mut ids = BTreeSet::new();
    for model in &mut models {
        model.id = required_trimmed(&model.id, "model.id")?;
        model.label = required_trimmed(&model.label, "model.label")?;
        if !ids.insert(model.id.clone()) {
            return Err(AppError::new("PROVIDER_MODEL_DUPLICATE", &model.id));
        }
    }
    if !ids.contains(&default_model) {
        return Err(AppError::new(
            "PROVIDER_DEFAULT_MODEL_MISSING",
            &default_model,
        ));
    }
    models.sort_by(|left, right| left.id.cmp(&right.id));
    Ok((models, default_model))
}

fn validate_adapter(protocol: ProviderProtocol, adapter: AdapterConfig) -> Result<AdapterConfig> {
    match (protocol, adapter) {
        (ProviderProtocol::Responses, AdapterConfig::None)
        | (ProviderProtocol::ChatCompletions, AdapterConfig::None) => Ok(AdapterConfig::None),
        (ProviderProtocol::Responses, AdapterConfig::Local { .. }) => Err(AppError::new(
            "PROVIDER_ADAPTER_INVALID",
            "Responses Provider cannot use a local adapter",
        )),
        (
            ProviderProtocol::ChatCompletions,
            AdapterConfig::Local {
                adapter_id,
                upstream_path,
            },
        ) => {
            if adapter_id != RESPONSES_CHAT_ADAPTER {
                return Err(AppError::new("PROVIDER_ADAPTER_UNKNOWN", adapter_id));
            }
            Ok(AdapterConfig::Local {
                adapter_id,
                upstream_path: validate_upstream_path(&upstream_path)?,
            })
        }
    }
}

fn validate_upstream_path(input: &str) -> Result<String> {
    let path = input.trim();
    let invalid = !path.starts_with('/')
        || path.starts_with("//")
        || path.contains(['?', '#', '\\', '%'])
        || path.chars().any(char::is_control)
        || path.split('/').any(|segment| matches!(segment, "." | ".."));
    if invalid {
        return Err(AppError::new(
            "PROVIDER_ADAPTER_PATH_INVALID",
            "adapter path must be an injection-free absolute path",
        ));
    }
    Ok(path.trim_end_matches('/').to_string())
}

fn validate_compatibility(value: Option<String>) -> Result<Option<String>> {
    let Some(value) = value else { return Ok(None) };
    if COMPATIBILITY_PROFILES.contains(&value.as_str()) {
        Ok(Some(value))
    } else {
        Err(AppError::new("PROVIDER_COMPATIBILITY_UNKNOWN", value))
    }
}

fn validate_codex_options(options: &CodexProviderOptions) -> Result<()> {
    if options
        .direct_request_max_retries
        .is_some_and(|value| value > 3)
        || options
            .direct_stream_max_retries
            .is_some_and(|value| value > 3)
        || options
            .stream_idle_timeout_ms
            .is_some_and(|value| !(1_000..=900_000).contains(&value))
    {
        return Err(AppError::new(
            "PROVIDER_CODEX_OPTIONS_INVALID",
            "Codex Provider option is outside the approved range",
        ));
    }
    validate_codex_security_options(options)
}

fn validate_timestamp(value: &str) -> Result<()> {
    DateTime::parse_from_rfc3339(value)
        .map(|_| ())
        .map_err(|error| AppError::new("PROVIDER_TIMESTAMP_INVALID", error.to_string()))
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn required_trimmed(value: &str, field: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        Err(AppError::new(
            "PROVIDER_INVALID",
            format!("{field} is empty"),
        ))
    } else {
        Ok(value.to_string())
    }
}

fn sort_providers(providers: &mut [ProviderProfileV2]) {
    providers.sort_by(|left, right| left.id.cmp(&right.id));
}
