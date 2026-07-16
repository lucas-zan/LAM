use super::error::{AppError, Result};
use super::gateway::binding::{GatewayBindingCollection, GatewayBindingService};
use super::gateway::sidecar::{GatewayPortChangePlan, GatewayRuntimeState, GatewayStateRepository};
use super::provider_attach_transaction::{AttachJournalCollection, AttachTransactionCoordinator};
use super::provider_auth_command::{
    AuthCommandApprovalCollection, AuthCommandApprovalRepository, AuthCommandSpec,
};
use super::provider_binding::{ProfileBindingCollection, ProfileProviderBinding};
use super::provider_capability::{
    resolve_capabilities, CapabilityResolutionInput, CapabilityVerificationKey,
    EffectiveCapabilities,
};
use super::provider_config_editor::{
    config_hash, detach_projection, replace_config_file, ConfigManagedProjection,
};
use super::provider_credentials::{
    resolve_credential, CredentialSource, ProcessEnvironment, SecretValue, UpstreamAuth,
};
use super::provider_keychain::{
    KeychainBackend, KeychainCredentialReference, KeychainCredentialService, SystemKeychainBackend,
};
use super::provider_model_discovery::{parse_openai_model_list, MAX_MODELS_RESPONSE_BYTES};
use super::provider_planner::{
    plan_profile_attach, plan_profile_detach, plan_provider_route, AdapterCatalog,
    AttachPlanContext, DryRunRegistry, GatewayPlanContext, ProfileAttachPlan, ProfileDetachPlan,
    RoutePlanInput,
};
use super::provider_v2::{
    build_provider, join_upstream_endpoint, AdapterConfig, CapabilityDeclaration,
    CodexProviderOptions, ProviderCollection, ProviderInput, ProviderModel, ProviderProfileV2,
    ProviderProtocol, ProviderRepository,
};
use super::storage::{InstallationLock, StoreOptions, VersionedFileStore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::fs;
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use toml_edit::{value, DocumentMut};

pub trait ProviderCredentialResolver {
    fn resolve(&self, source: &CredentialSource) -> Result<SecretValue>;
}

struct ProductionCredentialResolver<'a> {
    home_root: &'a Path,
}

impl ProviderCredentialResolver for ProductionCredentialResolver<'_> {
    fn resolve(&self, source: &CredentialSource) -> Result<SecretValue> {
        match source {
            CredentialSource::Env { .. } => resolve_credential(source, &ProcessEnvironment),
            CredentialSource::Keychain { .. } => {
                let reference = KeychainCredentialReference::try_from(source)?;
                let service = KeychainCredentialService::new(Arc::new(SystemKeychainBackend));
                service.with_secret(&reference, |value| {
                    SecretValue::from_sensitive(value.into())
                })
            }
            CredentialSource::AuthCommand { approval_id } => {
                let stores = provider_hub_stores(self.home_root)?;
                let state = GatewayStateRepository::new(stores.gateway_state.clone()).load()?;
                if !state.exists {
                    return Err(AppError::new(
                        "AUTH_COMMAND_APPROVAL_NOT_FOUND",
                        "auth command install identity is unavailable",
                    ));
                }
                let identity_key =
                    load_or_create_provider_install_identity(&state.value.install_id)?;
                let repository = AuthCommandApprovalRepository::new(VersionedFileStore::<
                    AuthCommandApprovalCollection,
                >::new(
                    stores.root.join("auth-command-approvals.json"),
                    stores.lock,
                    1,
                    StoreOptions {
                        max_bytes: 4 * 1024 * 1024,
                    },
                ));
                let approved = repository.resolve(approval_id, &identity_key)?;
                super::provider_auth_command::run_approved_auth_command(&approved)
            }
            CredentialSource::None => Err(AppError::new(
                "PROVIDER_CREDENTIAL_MISSING",
                "authenticated Provider has no credential",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderProtocolDto {
    Responses,
    ChatCompletions,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteKindDto {
    Direct,
    Gateway,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApproveAuthCommandRequestV2 {
    pub expected_revision: u64,
    pub executable: String,
    pub args: Vec<String>,
    pub timeout_ms: u64,
    pub max_stdout_bytes: usize,
    pub refresh_interval_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthCommandApprovalViewV2 {
    pub revision: u64,
    pub approval_id: String,
    pub executable: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthCommandApprovalListV2 {
    pub revision: u64,
    pub approvals: Vec<AuthCommandApprovalViewV2>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum CredentialReferenceDto {
    Env {
        #[serde(rename = "envKey")]
        env_key: String,
    },
    Keychain {
        service: String,
        account: String,
        version: u64,
    },
    AuthCommand {
        #[serde(rename = "approvalId")]
        approval_id: String,
    },
    None,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpstreamAuthDto {
    Bearer {
        credential: CredentialReferenceDto,
    },
    Header {
        name: String,
        credential: CredentialReferenceDto,
    },
    None,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AdapterDto {
    None,
    Local {
        #[serde(rename = "adapterId")]
        adapter_id: String,
        #[serde(rename = "upstreamPath")]
        upstream_path: String,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelDto {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexOptionsDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_idle_timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_request_max_retries: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_stream_max_retries: Option<u8>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub route_via_gateway: bool,
    #[serde(default)]
    pub query_params: BTreeMap<String, String>,
    #[serde(default)]
    pub env_http_headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDefinitionDto {
    pub id: String,
    pub name: String,
    pub protocol: ProviderProtocolDto,
    pub base_url: String,
    pub default_model: String,
    pub models: Vec<ProviderModelDto>,
    pub upstream_auth: UpstreamAuthDto,
    pub adapter: AdapterDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility_profile: Option<String>,
    #[serde(default)]
    pub codex: CodexOptionsDto,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateProviderRequestV2 {
    pub expected_revision: u64,
    pub provider: ProviderDefinitionDto,
}

pub type UpdateProviderRequestV2 = CreateProviderRequestV2;

impl CreateProviderRequestV2 {
    pub fn to_domain(&self, now: &str) -> Result<ProviderProfileV2> {
        build_provider(self.provider.to_domain()?, now)
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl ProviderDefinitionDto {
    pub fn to_domain(&self) -> Result<ProviderInput> {
        Ok(ProviderInput {
            id: self.id.clone(),
            name: self.name.clone(),
            protocol: match self.protocol {
                ProviderProtocolDto::Responses => ProviderProtocol::Responses,
                ProviderProtocolDto::ChatCompletions => ProviderProtocol::ChatCompletions,
            },
            base_url: self.base_url.clone(),
            default_model: self.default_model.clone(),
            models: self
                .models
                .iter()
                .map(|model| ProviderModel {
                    id: model.id.clone(),
                    label: model.label.clone(),
                    capabilities: None,
                })
                .collect(),
            upstream_auth: self.upstream_auth.to_domain()?,
            adapter: match &self.adapter {
                AdapterDto::None => AdapterConfig::None,
                AdapterDto::Local {
                    adapter_id,
                    upstream_path,
                } => AdapterConfig::Local {
                    adapter_id: adapter_id.clone(),
                    upstream_path: upstream_path.clone(),
                },
            },
            compatibility_profile: self.compatibility_profile.clone(),
            codex: CodexProviderOptions {
                display_name: self.codex.display_name.clone(),
                stream_idle_timeout_ms: self.codex.stream_idle_timeout_ms,
                direct_request_max_retries: self.codex.direct_request_max_retries,
                direct_stream_max_retries: self.codex.direct_stream_max_retries,
                route_via_gateway: self.codex.route_via_gateway,
                query_params: self.codex.query_params.clone(),
                env_http_headers: self.codex.env_http_headers.clone(),
            },
        })
    }
}

impl UpstreamAuthDto {
    fn to_domain(&self) -> Result<UpstreamAuth> {
        Ok(match self {
            Self::Bearer { credential } => UpstreamAuth::Bearer {
                source: credential.to_domain()?,
            },
            Self::Header { name, credential } => UpstreamAuth::Header {
                name: name.clone(),
                source: credential.to_domain()?,
            },
            Self::None => UpstreamAuth::None,
        })
    }
}

impl CredentialReferenceDto {
    fn to_domain(&self) -> Result<CredentialSource> {
        Ok(match self {
            Self::Env { env_key } => CredentialSource::Env {
                env_key: env_key.clone(),
            },
            Self::Keychain {
                service,
                account,
                version,
            } => CredentialSource::Keychain {
                service: service.clone(),
                account: account.clone(),
                version: *version,
            },
            Self::AuthCommand { approval_id } => CredentialSource::AuthCommand {
                approval_id: approval_id.clone(),
            },
            Self::None => CredentialSource::None,
        })
    }

    fn from_domain(value: &CredentialSource) -> Self {
        match value {
            CredentialSource::Env { env_key } => Self::Env {
                env_key: env_key.clone(),
            },
            CredentialSource::Keychain {
                service,
                account,
                version,
            } => Self::Keychain {
                service: service.clone(),
                account: account.clone(),
                version: *version,
            },
            CredentialSource::AuthCommand { approval_id } => Self::AuthCommand {
                approval_id: approval_id.clone(),
            },
            CredentialSource::None => Self::None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfileView {
    pub id: String,
    pub name: String,
    pub protocol: ProviderProtocolDto,
    pub base_url: String,
    pub default_model: String,
    pub models: Vec<ProviderModelDto>,
    pub upstream_auth: UpstreamAuthDto,
    pub adapter: AdapterDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility_profile: Option<String>,
    pub codex: CodexOptionsDto,
    pub store_revision: u64,
    pub used_by: Vec<String>,
    pub readiness_blockers: Vec<String>,
    pub readiness: ProviderReadinessViewV2,
    pub capabilities: EffectiveCapabilities,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_health: Option<ProviderHealthObservationV2>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderReadinessViewV2 {
    pub ready: bool,
    pub blockers: Vec<String>,
    pub binding_count: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHealthObservationV2 {
    pub provider_id: String,
    pub observed_at: String,
    pub ok: bool,
    pub latency_ms: Option<u64>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
struct ProviderHealthCollectionV2 {
    observations: Vec<ProviderHealthObservationV2>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUpstreamTestViewV2 {
    pub provider_id: String,
    pub ok: bool,
    pub route_kind: RouteKindDto,
    pub models_endpoint: String,
    pub redacted_summary: String,
}

impl ProviderProfileView {
    pub fn from_domain(
        provider: &ProviderProfileV2,
        store_revision: u64,
        used_by: Vec<String>,
    ) -> Self {
        let upstream_auth = match &provider.upstream_auth {
            UpstreamAuth::Bearer { source } => UpstreamAuthDto::Bearer {
                credential: CredentialReferenceDto::from_domain(source),
            },
            UpstreamAuth::Header { name, source } => UpstreamAuthDto::Header {
                name: name.clone(),
                credential: CredentialReferenceDto::from_domain(source),
            },
            UpstreamAuth::None => UpstreamAuthDto::None,
        };
        let adapter_supported = adapter_capability_ceiling(provider);
        let adapter_id = match &provider.adapter {
            AdapterConfig::None => "direct",
            AdapterConfig::Local { adapter_id, .. } => adapter_id,
        };
        let policy_id = provider.compatibility_profile.as_deref().unwrap_or("none");
        let capabilities = resolve_capabilities(CapabilityResolutionInput {
            declared: provider.capabilities.clone(),
            adapter_supported,
            user_override: None,
            expected_key: CapabilityVerificationKey::new(
                &provider.id,
                &provider.default_model,
                &provider.base_url,
                adapter_id,
                policy_id,
                "phase4-suite-v1",
            ),
            evidence: None,
            now_ms: 0,
        });
        Self {
            id: provider.id.clone(),
            name: provider.name.clone(),
            protocol: match provider.protocol {
                ProviderProtocol::Responses => ProviderProtocolDto::Responses,
                ProviderProtocol::ChatCompletions => ProviderProtocolDto::ChatCompletions,
            },
            base_url: provider.base_url.clone(),
            default_model: provider.default_model.clone(),
            models: provider
                .models
                .iter()
                .map(|model| ProviderModelDto {
                    id: model.id.clone(),
                    label: model.label.clone(),
                })
                .collect(),
            upstream_auth,
            adapter: match &provider.adapter {
                AdapterConfig::None => AdapterDto::None,
                AdapterConfig::Local {
                    adapter_id,
                    upstream_path,
                } => AdapterDto::Local {
                    adapter_id: adapter_id.clone(),
                    upstream_path: upstream_path.clone(),
                },
            },
            compatibility_profile: provider.compatibility_profile.clone(),
            codex: CodexOptionsDto {
                display_name: provider.codex.display_name.clone(),
                stream_idle_timeout_ms: provider.codex.stream_idle_timeout_ms,
                direct_request_max_retries: provider.codex.direct_request_max_retries,
                direct_stream_max_retries: provider.codex.direct_stream_max_retries,
                route_via_gateway: provider.codex.route_via_gateway,
                query_params: provider.codex.query_params.clone(),
                env_http_headers: provider.codex.env_http_headers.clone(),
            },
            store_revision,
            used_by,
            readiness_blockers: Vec::new(),
            readiness: ProviderReadinessViewV2::default(),
            capabilities,
            last_health: None,
        }
    }
}

fn adapter_capability_ceiling(provider: &ProviderProfileV2) -> CapabilityDeclaration {
    if matches!(provider.adapter, AdapterConfig::None) {
        return CapabilityDeclaration {
            streaming: super::provider_v2::CapabilitySupport::Supported,
            function_tools: super::provider_v2::CapabilitySupport::Supported,
            structured_outputs: super::provider_v2::CapabilitySupport::Supported,
            reasoning: super::provider_v2::CapabilitySupport::Supported,
        };
    }
    CapabilityDeclaration {
        streaming: super::provider_v2::CapabilitySupport::Supported,
        function_tools: super::provider_v2::CapabilitySupport::Supported,
        structured_outputs: super::provider_v2::CapabilitySupport::Unsupported,
        reasoning: if provider.compatibility_profile.as_deref() == Some("deepseek_chat_completions")
        {
            super::provider_v2::CapabilitySupport::Partial
        } else {
            super::provider_v2::CapabilitySupport::Unknown
        },
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LegacyCreateProviderRequest {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub wire_api: String,
    pub default_model: String,
    pub env_key: Option<String>,
    pub secret: Option<LegacySecretInput>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LegacySecretInput {
    Env {
        #[serde(rename = "envKey")]
        env_key: String,
    },
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyConversion {
    pub request: CreateProviderRequestV2,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LegacyCreateProviderResultV2 {
    pub provider: ProviderProfileView,
    pub warnings: Vec<String>,
}

impl LegacyCreateProviderRequest {
    pub fn into_v2(self, expected_revision: u64) -> Result<LegacyConversion> {
        let env_key = match (self.env_key, self.secret) {
            (Some(top), Some(LegacySecretInput::Env { env_key })) if top != env_key => {
                return Err(AppError::new(
                    "PROVIDER_ENV_MISMATCH",
                    "legacy envKey values disagree",
                ))
            }
            (Some(top), _) => Some(top),
            (None, Some(LegacySecretInput::Env { env_key })) => Some(env_key),
            (None, _) => None,
        };
        let (protocol, warnings) = match self.wire_api.as_str() {
            "responses" => (ProviderProtocolDto::Responses, Vec::new()),
            "openai" => (
                ProviderProtocolDto::Responses,
                vec!["LEGACY_WIRE_OPENAI_MAPPED_TO_RESPONSES".into()],
            ),
            _ => {
                return Err(AppError::new(
                    "PROVIDER_PROTOCOL_UNKNOWN",
                    "legacy wireApi is unsupported",
                ))
            }
        };
        let upstream_auth =
            env_key.map_or(UpstreamAuthDto::None, |env_key| UpstreamAuthDto::Bearer {
                credential: CredentialReferenceDto::Env { env_key },
            });
        Ok(LegacyConversion {
            request: CreateProviderRequestV2 {
                expected_revision,
                provider: ProviderDefinitionDto {
                    id: self.id,
                    name: self.name,
                    protocol,
                    base_url: self.base_url,
                    default_model: self.default_model.clone(),
                    models: vec![ProviderModelDto {
                        id: self.default_model.clone(),
                        label: self.default_model,
                    }],
                    upstream_auth,
                    adapter: AdapterDto::None,
                    compatibility_profile: None,
                    codex: CodexOptionsDto::default(),
                },
            },
            warnings,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileAttachPlanView {
    pub plan_id: String,
    pub fingerprint: String,
    pub expires_at_ms: u64,
    pub profile_id: String,
    pub provider_id: String,
    pub selected_model: String,
    pub route_kind: RouteKindDto,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub operations: Vec<String>,
    pub redacted_preview: String,
    pub expected_provider_store_revision: u64,
    pub expected_binding_store_revision: u64,
    pub expected_binding_revision: Option<u64>,
    pub source_config_hash: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanAttachRequestV2 {
    pub profile_id: String,
    pub provider_id: String,
    pub selected_model: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteAttachRequestV2 {
    pub plan_id: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteDetachRequestV2 {
    pub plan_id: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDetachPlanView {
    pub plan_id: String,
    pub fingerprint: String,
    pub expires_at_ms: u64,
    pub profile_id: String,
    pub expected_binding_store_revision: u64,
    pub expected_binding_revision: u64,
    pub source_config_hash: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttachExecutionView {
    pub operation_id: Option<String>,
    pub state: String,
    pub idempotent: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ApiAccountProviderSelectionV2 {
    New {
        provider: Box<ProviderDefinitionDto>,
    },
    Existing {
        provider_id: String,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlanApiAccountRequestV2 {
    pub account_name: String,
    pub selected_model: String,
    pub overwrite_wrapper: bool,
    pub provider: ApiAccountProviderSelectionV2,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteApiAccountRequestV2 {
    pub plan_id: String,
    pub fingerprint: String,
    #[serde(default)]
    pub keychain_secret: Option<String>,
}

impl std::fmt::Debug for ExecuteApiAccountRequestV2 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExecuteApiAccountRequestV2")
            .field("plan_id", &self.plan_id)
            .field("fingerprint", &self.fingerprint)
            .field(
                "keychain_secret",
                &self.keychain_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiAccountPlanViewV2 {
    pub plan_id: String,
    pub fingerprint: String,
    pub expires_at_ms: u64,
    pub account_name: String,
    pub provider_id: String,
    pub selected_model: String,
    pub route_kind: RouteKindDto,
    pub operations: Vec<String>,
    pub warnings: Vec<String>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiAccountExecutionViewV2 {
    pub account: super::account::CreateResult,
    pub provider: ProviderProfileView,
    pub binding: ProfileProviderBindingView,
    pub attach: AttachExecutionView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiAccountFaultPoint {
    AfterAccountCreated,
    AfterProviderCreated,
    CrashAfterProviderCreated,
    CrashAfterDeleteDetach,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
enum ApiAccountJournalKindV2 {
    #[default]
    Create,
    Delete,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct ApiAccountPlanV2 {
    request: PlanApiAccountRequestV2,
    provider_id: String,
    expected_provider_store_revision: u64,
    route_kind: RouteKindDto,
    fingerprint: String,
    expires_at_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ApiAccountJournalStageV2 {
    Planned,
    AccountCreated,
    ProviderCreated,
    Detached,
    AccountDeleted,
    ProviderDeleted,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ApiAccountJournalRecordV2 {
    operation_id: String,
    home_root: String,
    account_name: String,
    provider_id: String,
    exclusive_provider: bool,
    #[serde(default)]
    kind: ApiAccountJournalKindV2,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selected_model: Option<String>,
    stage: ApiAccountJournalStageV2,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
struct ApiAccountJournalCollectionV2 {
    records: Vec<ApiAccountJournalRecordV2>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiAccountRecoveryReportV2 {
    pub recovered: usize,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RotateProviderCredentialRequestV2 {
    pub expected_revision: u64,
    pub provider_id: String,
    pub expected_credential: CredentialReferenceDto,
    pub secret: String,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateProviderWithKeychainRequestV2 {
    pub expected_revision: u64,
    pub provider: ProviderDefinitionDto,
    pub secret: String,
}

impl std::fmt::Debug for CreateProviderWithKeychainRequestV2 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CreateProviderWithKeychainRequestV2")
            .field("expected_revision", &self.expected_revision)
            .field("provider", &self.provider)
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

impl std::fmt::Debug for RotateProviderCredentialRequestV2 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RotateProviderCredentialRequestV2")
            .field("expected_revision", &self.expected_revision)
            .field("provider_id", &self.provider_id)
            .field("expected_credential", &self.expected_credential)
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CredentialRotationViewV2 {
    pub provider: ProviderProfileView,
    pub cleanup_pending: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileProviderBindingView {
    pub profile_id: String,
    pub provider_id: String,
    pub selected_model: String,
    pub route_kind: RouteKindDto,
    pub revision: u64,
    pub provider_revision: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StructuredErrorView {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
    pub recovery_actions: Vec<String>,
}

impl StructuredErrorView {
    pub fn from_error(error: AppError) -> Self {
        let recovery_actions = match error.code.as_str() {
            "ATTACH_PLAN_STALE" | "STORE_REVISION_CONFLICT" | "PROFILE_BINDING_CONFLICT" => {
                vec!["refresh".into(), "plan_again".into()]
            }
            "CODEX_CONFIG_OWNERSHIP_CONFLICT" | "ATTACH_RECOVERY_OWNERSHIP_CONFLICT" => {
                vec!["inspect_config".into(), "resolve_conflict".into()]
            }
            _ => Vec::new(),
        };
        Self {
            code: error.code,
            message: "Operation failed; sensitive diagnostics are redacted".into(),
            recoverable: error.recoverable,
            recovery_actions,
        }
    }
}

pub struct ProviderApiV2State {
    registry: DryRunRegistry,
    attach_plans: BTreeMap<String, ProfileAttachPlan>,
    detach_plans: BTreeMap<String, ProfileDetachPlan>,
    api_account_plans: BTreeMap<String, ApiAccountPlanV2>,
    order: VecDeque<String>,
}

impl Default for ProviderApiV2State {
    fn default() -> Self {
        Self {
            registry: DryRunRegistry::new(300_000, 128),
            attach_plans: BTreeMap::new(),
            detach_plans: BTreeMap::new(),
            api_account_plans: BTreeMap::new(),
            order: VecDeque::new(),
        }
    }
}

impl ProviderApiV2State {
    fn insert_attach(&mut self, id: String, plan: ProfileAttachPlan) {
        self.evict_if_needed();
        self.order.push_back(id.clone());
        self.attach_plans.insert(id, plan);
    }
    fn insert_detach(&mut self, id: String, plan: ProfileDetachPlan) {
        self.evict_if_needed();
        self.order.push_back(id.clone());
        self.detach_plans.insert(id, plan);
    }
    fn insert_api_account(&mut self, id: String, plan: ApiAccountPlanV2) {
        self.evict_if_needed();
        self.order.push_back(id.clone());
        self.api_account_plans.insert(id, plan);
    }
    fn evict_if_needed(&mut self) {
        while self.order.len() >= 128 {
            if let Some(id) = self.order.pop_front() {
                self.attach_plans.remove(&id);
                self.detach_plans.remove(&id);
                self.api_account_plans.remove(&id);
            }
        }
    }
}

struct ProviderHubStores {
    root: PathBuf,
    lock: InstallationLock,
    providers: VersionedFileStore<ProviderCollection>,
    bindings: VersionedFileStore<ProfileBindingCollection>,
    journals: VersionedFileStore<AttachJournalCollection>,
    gateway_bindings: VersionedFileStore<GatewayBindingCollection>,
    gateway_state: VersionedFileStore<GatewayRuntimeState>,
    health: VersionedFileStore<ProviderHealthCollectionV2>,
    api_account_journals: VersionedFileStore<ApiAccountJournalCollectionV2>,
}

fn provider_hub_stores(home_root: &Path) -> Result<ProviderHubStores> {
    let root =
        super::provider_runtime::ProviderHubPaths::for_home(home_root).ensure_canonical_root()?;
    provider_hub_stores_at_root(&root)
}

fn provider_hub_stores_at_root(root: &Path) -> Result<ProviderHubStores> {
    fs::create_dir_all(root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
    }
    let lock = InstallationLock::new(root.join("provider-hub.lock"), Duration::from_secs(2));
    Ok(ProviderHubStores {
        root: root.to_path_buf(),
        providers: VersionedFileStore::new(
            root.join("providers.json"),
            lock.clone(),
            1,
            StoreOptions::default(),
        ),
        bindings: VersionedFileStore::new(
            root.join("bindings.json"),
            lock.clone(),
            1,
            StoreOptions::default(),
        ),
        journals: VersionedFileStore::new(
            root.join("attach-journal.json"),
            lock.clone(),
            1,
            StoreOptions::default(),
        ),
        gateway_bindings: VersionedFileStore::new(
            root.join("gateway-bindings.json"),
            lock.clone(),
            1,
            StoreOptions::default(),
        ),
        gateway_state: VersionedFileStore::new(
            root.join("gateway-state.json"),
            lock.clone(),
            1,
            StoreOptions {
                max_bytes: 1024 * 1024,
            },
        ),
        health: VersionedFileStore::new(
            root.join("provider-health.json"),
            lock.clone(),
            1,
            StoreOptions {
                max_bytes: 1024 * 1024,
            },
        ),
        api_account_journals: VersionedFileStore::new(
            root.join("api-account-journal.json"),
            lock.clone(),
            1,
            StoreOptions {
                max_bytes: 4 * 1024 * 1024,
            },
        ),
        lock,
    })
}

pub fn recover_provider_transactions_at_root_service_v2(
    root: &Path,
    now_ms: u64,
) -> Result<super::provider_attach_transaction::RecoveryReport> {
    let stores = provider_hub_stores_at_root(root)?;
    let gateway = GatewayBindingService::new(
        stores.gateway_bindings,
        KeychainCredentialService::new(Arc::new(SystemKeychainBackend)),
    );
    let coordinator = AttachTransactionCoordinator::new(
        stores.lock,
        stores.providers,
        stores.bindings,
        stores.journals,
        Arc::new(gateway),
        1,
    );
    let report = super::provider_attach_transaction::recover_provider_transactions_on_startup(
        &coordinator,
        now_ms,
        128,
    )?;
    recover_api_account_transactions_at_root_service_v2(root)?;
    Ok(report)
}

pub fn recover_provider_transactions_service_v2(
    home_root: &Path,
    now_ms: u64,
) -> Result<super::provider_attach_transaction::RecoveryReport> {
    let root =
        super::provider_runtime::ProviderHubPaths::for_home(home_root).ensure_canonical_root()?;
    recover_provider_transactions_at_root_service_v2(&root, now_ms)
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GatewayPortMigrationOutcomeV2 {
    pub old_port: u16,
    pub new_port: u16,
    pub migrated_profiles: Vec<String>,
    pub state_revision: u64,
    pub binding_store_revision: u64,
}

struct PreparedPortConfig {
    path: PathBuf,
    before: Vec<u8>,
    before_hash: String,
    after: String,
    after_hash: String,
    profile_id: String,
}

pub fn plan_gateway_port_migration_service_v2(
    home_root: &Path,
    new_port: u16,
) -> Result<GatewayPortChangePlan> {
    let stores = provider_hub_stores(home_root)?;
    let profile_ids = stores
        .bindings
        .load_or_default()?
        .value
        .bindings
        .into_iter()
        .filter(|binding| binding.route_kind == super::provider_binding::RouteKind::Gateway)
        .map(|binding| binding.profile_id)
        .collect::<Vec<_>>();
    GatewayStateRepository::new(stores.gateway_state).plan_port_change(new_port, &profile_ids)
}

pub fn execute_gateway_port_migration_service_v2(
    home_root: &Path,
    plan: &GatewayPortChangePlan,
    fingerprint: &str,
    now: &str,
) -> Result<GatewayPortMigrationOutcomeV2> {
    execute_gateway_port_migration_transaction_v2(home_root, plan, fingerprint, now, None)
}

#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn execute_gateway_port_migration_with_fault_v2(
    home_root: &Path,
    plan: &GatewayPortChangePlan,
    fingerprint: &str,
    now: &str,
    fail_before_config_index: Option<usize>,
) -> Result<GatewayPortMigrationOutcomeV2> {
    execute_gateway_port_migration_transaction_v2(
        home_root,
        plan,
        fingerprint,
        now,
        fail_before_config_index,
    )
}

fn execute_gateway_port_migration_transaction_v2(
    home_root: &Path,
    plan: &GatewayPortChangePlan,
    fingerprint: &str,
    now: &str,
    fail_before_config_index: Option<usize>,
) -> Result<GatewayPortMigrationOutcomeV2> {
    let stores = provider_hub_stores(home_root)?;
    let state_repository = GatewayStateRepository::new(stores.gateway_state.clone());
    let current_plan = plan_gateway_port_migration_service_v2(home_root, plan.new_port)?;
    if &current_plan != plan || plan.fingerprint != fingerprint {
        return Err(AppError::new(
            "GATEWAY_PORT_PLAN_STALE",
            "Gateway port migration plan changed",
        ));
    }
    let binding_snapshot = stores.bindings.load_or_default()?;
    let old_base = format!("http://127.0.0.1:{}/v1", plan.old_port);
    let new_base = format!("http://127.0.0.1:{}/v1", plan.new_port);
    let mut prepared = Vec::new();
    let mut updated_bindings = binding_snapshot.value.clone();
    for binding in updated_bindings
        .bindings
        .iter_mut()
        .filter(|binding| binding.route_kind == super::provider_binding::RouteKind::Gateway)
    {
        let path = PathBuf::from(&binding.config_projection.config_path);
        let before = fs::read(&path).map_err(|_| {
            AppError::new(
                "GATEWAY_PORT_CONFIG_UNAVAILABLE",
                "Gateway profile config is unavailable",
            )
        })?;
        let before_hash = config_hash(&before);
        if before_hash != binding.config_projection.applied_hash {
            return Err(AppError::new(
                "CODEX_CONFIG_DRIFT",
                "Gateway profile config changed since attach",
            ));
        }
        let mut document = String::from_utf8(before.clone())
            .map_err(|_| AppError::new("CODEX_CONFIG_INVALID", "config is not UTF-8"))?
            .parse::<DocumentMut>()
            .map_err(|_| AppError::new("CODEX_CONFIG_INVALID", "config TOML is invalid"))?;
        let provider_id = &binding.config_projection.managed_values;
        if !provider_id
            .get("base_url")
            .is_some_and(|managed| managed.contains(&old_base))
        {
            return Err(AppError::new(
                "GATEWAY_PORT_PROJECTION_MISMATCH",
                "Gateway profile does not reference the planned old port",
            ));
        }
        let table_id = binding
            .config_projection
            .managed_values
            .get("model_provider")
            .map(|value| value.trim_matches('"').to_owned())
            .unwrap_or_else(|| binding.provider_id.clone());
        document["model_providers"][&table_id]["base_url"] = value(&new_base);
        let after = document.to_string();
        after
            .parse::<toml::Value>()
            .map_err(|_| AppError::new("CODEX_CONFIG_INVALID", "migrated config is invalid"))?;
        let after_hash = config_hash(after.as_bytes());
        binding.config_projection.applied_hash = after_hash.clone();
        binding
            .config_projection
            .managed_values
            .insert("base_url".into(), format!("\"{new_base}\""));
        binding.revision += 1;
        binding.updated_at = now.into();
        prepared.push(PreparedPortConfig {
            path,
            before,
            before_hash,
            after,
            after_hash,
            profile_id: binding.profile_id.clone(),
        });
    }
    let mut planned_profiles = prepared
        .iter()
        .map(|item| item.profile_id.clone())
        .collect::<Vec<_>>();
    planned_profiles.sort();
    let mut expected_profiles = plan.profile_ids.clone();
    expected_profiles.sort();
    if planned_profiles != expected_profiles {
        return Err(AppError::new(
            "GATEWAY_PORT_PLAN_STALE",
            "Gateway binding set changed",
        ));
    }

    let mut written = Vec::new();
    for (index, config) in prepared.iter().enumerate() {
        if fail_before_config_index == Some(index) {
            rollback_port_configs(&prepared, &written)?;
            return Err(AppError::new(
                "GATEWAY_PORT_MIGRATION_FAULT",
                "injected config migration failure",
            ));
        }
        if let Err(error) = replace_config_file(&config.path, &config.before_hash, &config.after) {
            rollback_port_configs(&prepared, &written)?;
            return Err(error);
        }
        written.push(index);
    }
    let committed_bindings = match stores
        .bindings
        .compare_and_swap(binding_snapshot.revision, &updated_bindings)
    {
        Ok(snapshot) => snapshot,
        Err(error) => {
            rollback_port_configs(&prepared, &written)?;
            return Err(error);
        }
    };
    let committed_state = match state_repository.commit_port_change(
        plan.expected_state_revision,
        plan,
        fingerprint,
        now,
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            stores
                .bindings
                .compare_and_swap(committed_bindings.revision, &binding_snapshot.value)
                .map_err(|_| {
                    AppError::new(
                        "GATEWAY_PORT_ROLLBACK_CONFLICT",
                        "binding rollback requires manual intervention",
                    )
                })?;
            rollback_port_configs(&prepared, &written)?;
            return Err(error);
        }
    };
    Ok(GatewayPortMigrationOutcomeV2 {
        old_port: plan.old_port,
        new_port: plan.new_port,
        migrated_profiles: plan.profile_ids.clone(),
        state_revision: committed_state.revision,
        binding_store_revision: committed_bindings.revision,
    })
}

fn rollback_port_configs(prepared: &[PreparedPortConfig], written: &[usize]) -> Result<()> {
    for index in written.iter().rev() {
        let config = &prepared[*index];
        replace_config_file(
            &config.path,
            &config.after_hash,
            std::str::from_utf8(&config.before).map_err(|_| {
                AppError::new("GATEWAY_PORT_ROLLBACK_FAILED", "config backup is invalid")
            })?,
        )
        .map_err(|_| {
            AppError::new(
                "GATEWAY_PORT_ROLLBACK_FAILED",
                "config rollback requires manual intervention",
            )
        })?;
    }
    Ok(())
}

pub fn ensure_profile_has_no_provider_binding_service_v2(
    home_root: &Path,
    profile_id: &str,
) -> Result<()> {
    let stores = provider_hub_stores(home_root)?;
    super::provider_binding::ProfileBindingRepository::new(stores.bindings)
        .ensure_profile_can_delete(profile_id)
}

pub fn create_provider_service_v2(
    home_root: &Path,
    request: CreateProviderRequestV2,
    now: &str,
) -> Result<ProviderProfileView> {
    let stores = provider_hub_stores(home_root)?;
    let snapshot = ProviderRepository::new(stores.providers).create(
        request.expected_revision,
        request.provider.to_domain()?,
        now,
    )?;
    let provider = snapshot
        .value
        .providers
        .iter()
        .find(|item| item.id == request.provider.id)
        .expect("created provider");
    let mut view = ProviderProfileView::from_domain(provider, snapshot.revision, Vec::new());
    apply_provider_readiness(
        &mut view,
        provider,
        snapshot.revision,
        &[],
        None,
        &ProductionCredentialResolver { home_root },
        super::provider_runtime::resolve_auth_helper_executable().is_ok(),
    );
    Ok(view)
}

pub fn approve_auth_command_service_v2_with_key(
    home_root: &Path,
    request: ApproveAuthCommandRequestV2,
    identity_key: &[u8; 32],
) -> Result<AuthCommandApprovalViewV2> {
    if request.timeout_ms < 100
        || request.timeout_ms > 5_000
        || request.max_stdout_bytes == 0
        || request.max_stdout_bytes > 16_384
        || request.refresh_interval_ms > 86_400_000
        || request.args.len() > 64
        || request
            .args
            .iter()
            .any(|value| value.len() > 4_096 || value.contains(['\0', '\r', '\n']))
    {
        return Err(AppError::new(
            "AUTH_COMMAND_POLICY_INVALID",
            "auth command policy is outside the approved bounds",
        ));
    }
    let stores = provider_hub_stores(home_root)?;
    let repository = AuthCommandApprovalRepository::new(VersionedFileStore::<
        AuthCommandApprovalCollection,
    >::new(
        stores.root.join("auth-command-approvals.json"),
        stores.lock.clone(),
        1,
        StoreOptions {
            max_bytes: 4 * 1024 * 1024,
        },
    ));
    let spec = AuthCommandSpec {
        executable: PathBuf::from(request.executable),
        args: request.args,
        cwd: None,
        timeout_ms: request.timeout_ms,
        max_stdout_bytes: request.max_stdout_bytes,
        cache_ttl_ms: request.refresh_interval_ms,
    };
    let approval_id =
        super::provider_auth_command::approve_auth_command_with_key(spec.clone(), identity_key)?
            .approval_fingerprint()
            .to_owned();
    let committed = repository.approve(request.expected_revision, spec, identity_key)?;
    let approved = committed
        .value
        .approvals
        .iter()
        .find(|approved| approved.approval_fingerprint() == approval_id)
        .ok_or_else(|| {
            AppError::new("AUTH_COMMAND_APPROVAL_NOT_FOUND", "approval was not stored")
        })?;
    Ok(AuthCommandApprovalViewV2 {
        revision: committed.revision,
        approval_id: approved.approval_fingerprint().into(),
        executable: fs::canonicalize(&approved.spec().executable)?
            .to_string_lossy()
            .into_owned(),
    })
}

pub fn list_auth_command_approvals_service_v2(
    home_root: &Path,
) -> Result<AuthCommandApprovalListV2> {
    let stores = provider_hub_stores(home_root)?;
    let snapshot = AuthCommandApprovalRepository::new(VersionedFileStore::<
        AuthCommandApprovalCollection,
    >::new(
        stores.root.join("auth-command-approvals.json"),
        stores.lock,
        1,
        StoreOptions {
            max_bytes: 4 * 1024 * 1024,
        },
    ))
    .load()?;
    Ok(AuthCommandApprovalListV2 {
        revision: snapshot.revision,
        approvals: snapshot
            .value
            .approvals
            .into_iter()
            .map(|approved| AuthCommandApprovalViewV2 {
                revision: snapshot.revision,
                approval_id: approved.approval_fingerprint().into(),
                executable: approved.spec().executable.to_string_lossy().into_owned(),
            })
            .collect(),
    })
}

pub fn approve_auth_command_system_service_v2(
    home_root: &Path,
    request: ApproveAuthCommandRequestV2,
) -> Result<AuthCommandApprovalViewV2> {
    let stores = provider_hub_stores(home_root)?;
    let state_repository = GatewayStateRepository::new(stores.gateway_state.clone());
    let state = state_repository.load()?;
    let state = if state.exists {
        state
    } else {
        state_repository.initialize(
            state.revision,
            choose_gateway_port()?,
            env!("CARGO_PKG_VERSION"),
            1,
            1,
            &chrono::Utc::now().to_rfc3339(),
        )?
    };
    let identity_key = load_or_create_provider_install_identity(&state.value.install_id)?;
    approve_auth_command_service_v2_with_key(home_root, request, &identity_key)
}

#[cfg(target_os = "macos")]
fn load_or_create_provider_install_identity(install_id: &str) -> Result<[u8; 32]> {
    use rand::RngCore;
    use security_framework::passwords::{get_generic_password, set_generic_password};
    const SERVICE: &str = "dev.localagentmanager.desktop.provider-hub";
    let bytes = match get_generic_password(SERVICE, install_id) {
        Ok(bytes) => bytes,
        Err(_) => {
            let mut bytes = [0_u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut bytes);
            set_generic_password(SERVICE, install_id, &bytes).map_err(|_| {
                AppError::new(
                    "KEYCHAIN_UNAVAILABLE",
                    "install identity Keychain operation failed [REDACTED]",
                )
            })?;
            bytes.to_vec()
        }
    };
    bytes.try_into().map_err(|_| {
        AppError::new(
            "GATEWAY_IDENTITY_KEY_INVALID",
            "install identity key has an invalid length",
        )
    })
}

#[cfg(not(target_os = "macos"))]
fn load_or_create_provider_install_identity(_install_id: &str) -> Result<[u8; 32]> {
    Err(AppError::new(
        "UNSUPPORTED_REMOTE_PROVIDER_PLATFORM",
        "auth command approval requires exact-tested macOS",
    ))
}

pub fn create_legacy_provider_service_v2(
    home_root: &Path,
    request: LegacyCreateProviderRequest,
    expected_revision: u64,
    now: &str,
) -> Result<LegacyCreateProviderResultV2> {
    let converted = request.into_v2(expected_revision)?;
    let provider = create_provider_service_v2(home_root, converted.request, now)?;
    Ok(LegacyCreateProviderResultV2 {
        provider,
        warnings: converted.warnings,
    })
}

pub fn update_provider_service_v2(
    home_root: &Path,
    request: UpdateProviderRequestV2,
    now: &str,
) -> Result<ProviderProfileView> {
    let stores = provider_hub_stores(home_root)?;
    if stores
        .bindings
        .load_or_default()?
        .value
        .bindings
        .iter()
        .any(|binding| binding.provider_id == request.provider.id)
    {
        return Err(AppError::new(
            "PROVIDER_HAS_BINDINGS_REBIND_REQUIRED",
            "attached Provider changes require an explicit rebind plan",
        ));
    }
    let snapshot = ProviderRepository::new(stores.providers).update(
        request.expected_revision,
        request.provider.to_domain()?,
        now,
    )?;
    let provider = snapshot
        .value
        .providers
        .iter()
        .find(|item| item.id == request.provider.id)
        .expect("updated provider");
    let used_by = stores
        .bindings
        .load_or_default()?
        .value
        .bindings
        .iter()
        .filter(|binding| binding.provider_id == provider.id)
        .map(|binding| binding.profile_id.clone())
        .collect();
    let mut view = ProviderProfileView::from_domain(provider, snapshot.revision, used_by);
    apply_provider_readiness(
        &mut view,
        provider,
        snapshot.revision,
        &[],
        None,
        &ProductionCredentialResolver { home_root },
        super::provider_runtime::resolve_auth_helper_executable().is_ok(),
    );
    Ok(view)
}

pub fn list_provider_views_service_v2(home_root: &Path) -> Result<Vec<ProviderProfileView>> {
    let stores = provider_hub_stores(home_root)?;
    let providers = stores.providers.load_or_default()?;
    let bindings = stores.bindings.load_or_default()?;
    let health = stores.health.load_or_default()?;
    Ok(providers
        .value
        .providers
        .iter()
        .map(|provider| {
            let used_by: Vec<String> = bindings
                .value
                .bindings
                .iter()
                .filter(|binding| binding.provider_id == provider.id)
                .map(|binding| binding.profile_id.clone())
                .collect();
            let mut view =
                ProviderProfileView::from_domain(provider, providers.revision, used_by.clone());
            let provider_bindings = bindings
                .value
                .bindings
                .iter()
                .filter(|binding| binding.provider_id == provider.id)
                .collect::<Vec<_>>();
            let last_health = health
                .value
                .observations
                .iter()
                .find(|observation| observation.provider_id == provider.id);
            apply_provider_readiness(
                &mut view,
                provider,
                providers.revision,
                &provider_bindings,
                last_health,
                &ProductionCredentialResolver { home_root },
                super::provider_runtime::resolve_auth_helper_executable().is_ok(),
            );
            view
        })
        .collect())
}

fn apply_provider_readiness(
    view: &mut ProviderProfileView,
    provider: &ProviderProfileV2,
    provider_store_revision: u64,
    bindings: &[&ProfileProviderBinding],
    last_health: Option<&ProviderHealthObservationV2>,
    resolver: &dyn ProviderCredentialResolver,
    gateway_available: bool,
) {
    let mut blockers = provider_readiness_blockers(provider, resolver, gateway_available);
    let provider_bytes = serde_json::to_vec(provider).expect("serializable Provider");
    let provider_fingerprint = hex::encode(Sha256::digest(provider_bytes));
    for binding in bindings {
        if binding.provider_revision != provider_store_revision
            || binding.provider_fingerprint != provider_fingerprint
        {
            blockers.push(format!("PROVIDER_BINDING_STALE:{}", binding.profile_id));
        }
        match fs::read(&binding.config_projection.config_path) {
            Ok(bytes) if config_hash(&bytes) != binding.config_projection.applied_hash => {
                blockers.push(format!("CODEX_CONFIG_DRIFT:{}", binding.profile_id));
            }
            Err(_) => blockers.push(format!("CODEX_CONFIG_UNAVAILABLE:{}", binding.profile_id)),
            _ => {}
        }
    }
    if let Some(observation) = last_health {
        if !observation.ok {
            blockers.push(
                observation
                    .error_code
                    .clone()
                    .unwrap_or_else(|| "PROVIDER_HEALTH_UNAVAILABLE".into()),
            );
        }
        view.last_health = Some(observation.clone());
    }
    blockers.sort();
    blockers.dedup();
    view.readiness_blockers = blockers.clone();
    view.readiness = ProviderReadinessViewV2 {
        ready: blockers.is_empty(),
        blockers,
        binding_count: bindings.len(),
    };
}

fn provider_readiness_blockers(
    provider: &ProviderProfileV2,
    resolver: &dyn ProviderCredentialResolver,
    gateway_available: bool,
) -> Vec<String> {
    let mut blockers = Vec::new();
    match &provider.upstream_auth {
        UpstreamAuth::None => {}
        UpstreamAuth::Bearer { source } | UpstreamAuth::Header { source, .. } => {
            if let Err(error) = resolver.resolve(source) {
                blockers.push(error.code);
            }
        }
    }
    if provider.protocol == ProviderProtocol::ChatCompletions
        && matches!(provider.adapter, AdapterConfig::None)
    {
        blockers.push("PROVIDER_ADAPTER_REQUIRED".into());
    } else if (provider.protocol == ProviderProtocol::ChatCompletions
        || provider.codex.route_via_gateway)
        && !gateway_available
    {
        blockers.push("GATEWAY_UNAVAILABLE".into());
    }
    blockers.sort();
    blockers.dedup();
    blockers
}

pub fn test_provider_upstream_service_v2(
    home_root: &Path,
    provider_id: &str,
) -> Result<ProviderUpstreamTestViewV2> {
    test_provider_upstream_service_v2_with_resolver(
        home_root,
        provider_id,
        &ProductionCredentialResolver { home_root },
    )
}

pub fn test_provider_upstream_service_v2_with_resolver(
    home_root: &Path,
    provider_id: &str,
    resolver: &dyn ProviderCredentialResolver,
) -> Result<ProviderUpstreamTestViewV2> {
    let started = Instant::now();
    let result = probe_provider_upstream_service_v2(home_root, provider_id, resolver);
    let observation = ProviderHealthObservationV2 {
        provider_id: provider_id.into(),
        observed_at: chrono::Utc::now().to_rfc3339(),
        ok: result.is_ok(),
        latency_ms: u64::try_from(started.elapsed().as_millis()).ok(),
        error_code: result.as_ref().err().map(|error| error.code.clone()),
    };
    record_provider_health_observation(home_root, observation)?;
    result
}

fn record_provider_health_observation(
    home_root: &Path,
    observation: ProviderHealthObservationV2,
) -> Result<()> {
    let store = provider_hub_stores(home_root)?.health;
    for _ in 0..3 {
        let mut snapshot = store.load_or_default()?;
        snapshot
            .value
            .observations
            .retain(|item| item.provider_id != observation.provider_id);
        snapshot.value.observations.push(observation.clone());
        snapshot
            .value
            .observations
            .sort_by(|left, right| left.provider_id.cmp(&right.provider_id));
        match store.compare_and_swap(snapshot.revision, &snapshot.value) {
            Ok(_) => return Ok(()),
            Err(error) if error.code == "STORE_REVISION_CONFLICT" => continue,
            Err(error) => return Err(error),
        }
    }
    Err(AppError::new(
        "PROVIDER_HEALTH_WRITE_CONFLICT",
        "Provider health observation changed concurrently",
    ))
}

fn probe_provider_upstream_service_v2(
    home_root: &Path,
    provider_id: &str,
    resolver: &dyn ProviderCredentialResolver,
) -> Result<ProviderUpstreamTestViewV2> {
    let stores = provider_hub_stores(home_root)?;
    let providers = stores.providers.load_or_default()?;
    let provider = providers
        .value
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| AppError::new("PROVIDER_NOT_FOUND", provider_id))?;
    if provider.protocol != ProviderProtocol::Responses
        || !matches!(provider.adapter, AdapterConfig::None)
    {
        return Err(AppError::new(
            "PROVIDER_DIRECT_ROUTE_REQUIRED",
            "upstream validation is available only for direct Responses Providers",
        ));
    }
    super::provider_credentials::validate_upstream_auth(&provider.upstream_auth)?;
    let models_endpoint = join_upstream_endpoint(&provider.base_url, "/models")?;
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| {
            AppError::new(
                "PROVIDER_HEALTH_CLIENT_FAILED",
                "Provider health client could not be created",
            )
        })?;
    let mut request = client.get(&models_endpoint);
    match &provider.upstream_auth {
        UpstreamAuth::Bearer { source } => {
            let secret = resolver.resolve(source)?;
            let header = secret.with_exposed(|token| {
                reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
            });
            request = request.header(
                reqwest::header::AUTHORIZATION,
                header.map_err(|_| {
                    AppError::new(
                        "PROVIDER_CREDENTIAL_HEADER_INVALID",
                        "credential cannot be represented as an HTTP header",
                    )
                })?,
            );
        }
        UpstreamAuth::Header { name, source } => {
            let secret = resolver.resolve(source)?;
            let name = reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                AppError::new(
                    "PROVIDER_AUTH_HEADER_INVALID",
                    "Provider auth header is invalid",
                )
            })?;
            let value = secret.with_exposed(reqwest::header::HeaderValue::from_str);
            request = request.header(
                name,
                value.map_err(|_| {
                    AppError::new(
                        "PROVIDER_CREDENTIAL_HEADER_INVALID",
                        "credential cannot be represented as an HTTP header",
                    )
                })?,
            );
        }
        UpstreamAuth::None => {}
    }
    let started = Instant::now();
    let mut response = request.send().map_err(|_| {
        AppError::new(
            "PROVIDER_HEALTH_UNAVAILABLE",
            "Provider models endpoint is unavailable",
        )
    })?;
    if !response.status().is_success() {
        return Err(AppError::new(
            "PROVIDER_HEALTH_HTTP_ERROR",
            format!(
                "Provider models endpoint returned HTTP {}",
                response.status().as_u16()
            ),
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MODELS_RESPONSE_BYTES as u64)
    {
        return Err(AppError::new(
            "PROVIDER_HEALTH_RESPONSE_LIMIT",
            "Provider models response exceeds 1 MiB",
        ));
    }
    let mut body = Vec::new();
    response
        .by_ref()
        .take((MAX_MODELS_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|_| {
            AppError::new(
                "PROVIDER_HEALTH_RESPONSE_INVALID",
                "Provider models response could not be read",
            )
        })?;
    if body.len() > MAX_MODELS_RESPONSE_BYTES {
        return Err(AppError::new(
            "PROVIDER_HEALTH_RESPONSE_LIMIT",
            "Provider models response exceeds 1 MiB",
        ));
    }
    parse_openai_model_list(&body).map_err(|_| {
        AppError::new(
            "PROVIDER_HEALTH_RESPONSE_INVALID",
            "Provider models response is not a valid OpenAI model list",
        )
    })?;
    let elapsed_ms = started.elapsed().as_millis();
    Ok(ProviderUpstreamTestViewV2 {
        provider_id: provider.id.clone(),
        ok: true,
        route_kind: if provider.codex.route_via_gateway {
            RouteKindDto::Gateway
        } else {
            RouteKindDto::Direct
        },
        models_endpoint,
        redacted_summary: format!(
            "Responses Provider models endpoint healthy ({} ms)",
            elapsed_ms
        ),
    })
}

pub fn list_binding_views_service_v2(home_root: &Path) -> Result<Vec<ProfileProviderBindingView>> {
    let stores = provider_hub_stores(home_root)?;
    Ok(stores
        .bindings
        .load_or_default()?
        .value
        .bindings
        .into_iter()
        .map(binding_view)
        .collect())
}

pub fn rotate_provider_credential_service_v2<B: KeychainBackend>(
    home_root: &Path,
    request: RotateProviderCredentialRequestV2,
    backend: Arc<B>,
    now: &str,
) -> Result<CredentialRotationViewV2> {
    let stores = provider_hub_stores(home_root)?;
    let repository = ProviderRepository::new(stores.providers);
    let expected_source = request.expected_credential.to_domain()?;
    let outcome = KeychainCredentialService::new(backend).rotate_provider(
        &repository,
        request.expected_revision,
        &request.provider_id,
        &expected_source,
        super::provider_credentials::SecretValue::from_sensitive(request.secret),
        now,
    )?;
    let snapshot = repository.load()?;
    let provider = snapshot
        .value
        .providers
        .iter()
        .find(|provider| provider.id == request.provider_id)
        .ok_or_else(|| AppError::new("PROVIDER_NOT_FOUND", request.provider_id))?;
    let used_by = stores
        .bindings
        .load_or_default()?
        .value
        .bindings
        .iter()
        .filter(|binding| binding.provider_id == provider.id)
        .map(|binding| binding.profile_id.clone())
        .collect();
    Ok(CredentialRotationViewV2 {
        provider: ProviderProfileView::from_domain(
            provider,
            outcome.provider_store_revision,
            used_by,
        ),
        cleanup_pending: outcome.cleanup_pending,
    })
}

pub fn create_provider_with_keychain_service_v2<B: KeychainBackend>(
    home_root: &Path,
    mut request: CreateProviderWithKeychainRequestV2,
    backend: Arc<B>,
    now: &str,
) -> Result<ProviderProfileView> {
    let credential = match &mut request.provider.upstream_auth {
        UpstreamAuthDto::Bearer { credential } | UpstreamAuthDto::Header { credential, .. }
            if matches!(credential, CredentialReferenceDto::None) =>
        {
            credential
        }
        _ => {
            return Err(AppError::new(
                "PROVIDER_KEYCHAIN_CREATE_AUTH",
                "new Keychain Provider requires an authenticated route with an empty reference",
            ))
        }
    };
    let reference = KeychainCredentialReference::new(&uuid::Uuid::new_v4().to_string(), 1)?;
    let service = KeychainCredentialService::new(backend);
    service.write_exact(
        &reference,
        super::provider_credentials::SecretValue::from_sensitive(request.secret),
    )?;
    *credential = CredentialReferenceDto::Keychain {
        service: reference.service.clone(),
        account: reference.account.clone(),
        version: reference.version,
    };
    match create_provider_service_v2(
        home_root,
        CreateProviderRequestV2 {
            expected_revision: request.expected_revision,
            provider: request.provider,
        },
        now,
    ) {
        Ok(view) => Ok(view),
        Err(error) => {
            service.revoke(&reference).map_err(|_| {
                AppError::new(
                    "KEYCHAIN_COMPENSATION_FAILED",
                    "Provider creation failed and Keychain cleanup is pending",
                )
            })?;
            Err(error)
        }
    }
}

pub fn create_provider_with_keychain_system_service_v2(
    home_root: &Path,
    request: CreateProviderWithKeychainRequestV2,
    now: &str,
) -> Result<ProviderProfileView> {
    create_provider_with_keychain_service_v2(
        home_root,
        request,
        Arc::new(SystemKeychainBackend),
        now,
    )
}

pub fn plan_api_account_service_v2(
    home_root: &Path,
    mut request: PlanApiAccountRequestV2,
    state: &mut ProviderApiV2State,
    now_ms: u64,
) -> Result<ApiAccountPlanViewV2> {
    normalize_new_api_account_route(&mut request);
    if super::account::list_accounts(home_root)?
        .iter()
        .any(|account| account.id == request.account_name)
    {
        return Err(AppError::new(
            "ACCOUNT_ALREADY_EXISTS",
            "an Account already owns this Profile/CODEX_HOME",
        ));
    }
    let account_plan = super::account::create_account_plan(
        home_root,
        &super::account::CreateAccountRequest {
            name: request.account_name.clone(),
            copy_config_from: None,
            overwrite_wrapper: request.overwrite_wrapper,
        },
    )?;
    if !account_plan.warnings.is_empty() {
        return Err(AppError::new(
            "API_ACCOUNT_TARGET_NOT_EMPTY",
            "API Account requires a new Profile/CODEX_HOME",
        ));
    }

    let stores = provider_hub_stores(home_root)?;
    let providers = stores.providers.load_or_default()?;
    let (provider_id, provider) = match &request.provider {
        ApiAccountProviderSelectionV2::New { provider } => {
            let expected_id = format!("account-{}", request.account_name);
            if provider.id != expected_id {
                return Err(AppError::new(
                    "API_ACCOUNT_PROVIDER_ID_INVALID",
                    "exclusive Provider id must be derived from the Account name",
                ));
            }
            if providers
                .value
                .providers
                .iter()
                .any(|current| current.id == provider.id)
            {
                return Err(AppError::new(
                    "PROVIDER_ALREADY_EXISTS",
                    "exclusive API Account Provider already exists",
                ));
            }
            let planning_definition = provider_definition_for_plan(provider);
            (
                provider.id.clone(),
                planning_definition
                    .to_domain()
                    .and_then(|input| build_provider(input, "1970-01-01T00:00:00Z"))?,
            )
        }
        ApiAccountProviderSelectionV2::Existing { provider_id } => {
            let provider = providers
                .value
                .providers
                .iter()
                .find(|provider| provider.id == *provider_id)
                .cloned()
                .ok_or_else(|| AppError::new("PROVIDER_NOT_FOUND", provider_id))?;
            (provider_id.clone(), provider)
        }
    };
    let route = plan_provider_route(RoutePlanInput {
        provider,
        selected_model: request.selected_model.clone(),
        adapters: AdapterCatalog::standard(),
    });
    let blockers = route
        .blockers
        .iter()
        .map(|blocker| format!("{blocker:?}"))
        .collect::<Vec<_>>();
    let route_kind = match route.route_kind {
        super::provider_binding::RouteKind::Direct => RouteKindDto::Direct,
        super::provider_binding::RouteKind::Gateway => RouteKindDto::Gateway,
    };
    let expires_at_ms = now_ms.saturating_add(300_000);
    let fingerprint_input = serde_json::to_vec(&(
        &request,
        &provider_id,
        providers.revision,
        route_kind,
        expires_at_ms,
    ))
    .map_err(|_| {
        AppError::new(
            "API_ACCOUNT_PLAN_SERIALIZATION",
            "API Account plan could not be fingerprinted",
        )
    })?;
    let fingerprint = hex::encode(Sha256::digest(fingerprint_input));
    let plan_id = uuid::Uuid::new_v4().to_string();
    state.insert_api_account(
        plan_id.clone(),
        ApiAccountPlanV2 {
            request: request.clone(),
            provider_id: provider_id.clone(),
            expected_provider_store_revision: providers.revision,
            route_kind,
            fingerprint: fingerprint.clone(),
            expires_at_ms,
        },
    );
    let mut operations = account_plan.operations;
    operations.push(match request.provider {
        ApiAccountProviderSelectionV2::New { .. } => {
            format!("create exclusive Provider {provider_id}")
        }
        ApiAccountProviderSelectionV2::Existing { .. } => {
            format!("reuse Provider {provider_id}")
        }
    });
    operations.push(format!(
        "attach Provider {provider_id} model {}",
        request.selected_model
    ));
    Ok(ApiAccountPlanViewV2 {
        plan_id,
        fingerprint,
        expires_at_ms,
        account_name: request.account_name,
        provider_id,
        selected_model: request.selected_model,
        route_kind,
        operations,
        warnings: account_plan.warnings,
        blockers,
    })
}

fn normalize_new_api_account_route(request: &mut PlanApiAccountRequestV2) {
    let ApiAccountProviderSelectionV2::New { provider } = &mut request.provider else {
        return;
    };
    provider.codex.route_via_gateway = true;
    provider.codex.direct_request_max_retries = Some(0);
    provider.codex.direct_stream_max_retries = Some(0);
}

pub fn execute_api_account_service_v2(
    home_root: &Path,
    request: ExecuteApiAccountRequestV2,
    state: &mut ProviderApiV2State,
    now_ms: u64,
) -> Result<ApiAccountExecutionViewV2> {
    execute_api_account_service_v2_with_fault(home_root, request, state, now_ms, None)
}

#[doc(hidden)]
pub fn execute_api_account_service_v2_with_fault(
    home_root: &Path,
    request: ExecuteApiAccountRequestV2,
    state: &mut ProviderApiV2State,
    now_ms: u64,
    fault: Option<ApiAccountFaultPoint>,
) -> Result<ApiAccountExecutionViewV2> {
    let plan = state
        .api_account_plans
        .get(&request.plan_id)
        .cloned()
        .ok_or_else(|| AppError::new("API_ACCOUNT_PLAN_UNKNOWN", "API Account plan is unknown"))?;
    if request.fingerprint != plan.fingerprint {
        return Err(AppError::new(
            "API_ACCOUNT_PLAN_TAMPERED",
            "API Account plan fingerprint does not match",
        ));
    }
    if now_ms > plan.expires_at_ms {
        return Err(AppError::new(
            "API_ACCOUNT_PLAN_EXPIRED",
            "API Account plan expired",
        ));
    }
    let stores = provider_hub_stores(home_root)?;
    if stores.providers.load_or_default()?.revision != plan.expected_provider_store_revision {
        return Err(AppError::new(
            "API_ACCOUNT_PLAN_STALE",
            "Provider store changed after planning",
        ));
    }
    if super::account::list_accounts(home_root)?
        .iter()
        .any(|account| account.id == plan.request.account_name)
    {
        return Err(AppError::new(
            "API_ACCOUNT_PLAN_STALE",
            "Account was created after planning",
        ));
    }

    create_api_account_journal(home_root, &request.plan_id, &plan)?;

    let account = match super::account::execute_create_account(
        home_root,
        &super::account::CreateAccountRequest {
            name: plan.request.account_name.clone(),
            copy_config_from: None,
            overwrite_wrapper: plan.request.overwrite_wrapper,
        },
    ) {
        Ok(account) => account,
        Err(error) => {
            clear_api_account_journal(home_root, &request.plan_id)?;
            return Err(error);
        }
    };
    if let Err(error) = ensure_empty_api_account_config(&account.home_path) {
        compensate_api_account_creation(home_root, &request.plan_id, &plan, None)?;
        return Err(error);
    }
    update_api_account_journal(
        home_root,
        &request.plan_id,
        ApiAccountJournalStageV2::AccountCreated,
    )?;
    if fault == Some(ApiAccountFaultPoint::AfterAccountCreated) {
        compensate_api_account_creation(home_root, &request.plan_id, &plan, None)?;
        return Err(api_account_fault());
    }

    let mut created_provider = false;
    let provider_result = (|| -> Result<ProviderProfileView> {
        match &plan.request.provider {
            ApiAccountProviderSelectionV2::New { provider } => {
                created_provider = true;
                if request.keychain_secret.is_some() {
                    create_provider_with_keychain_system_service_v2(
                        home_root,
                        CreateProviderWithKeychainRequestV2 {
                            expected_revision: plan.expected_provider_store_revision,
                            provider: provider.as_ref().clone(),
                            secret: request.keychain_secret.clone().unwrap_or_default(),
                        },
                        &timestamp_from_ms(now_ms),
                    )
                } else {
                    if provider_requires_keychain_secret(provider) {
                        return Err(AppError::new(
                            "API_ACCOUNT_SECRET_REQUIRED",
                            "Keychain-backed API Account requires a write-only secret",
                        ));
                    }
                    create_provider_service_v2(
                        home_root,
                        CreateProviderRequestV2 {
                            expected_revision: plan.expected_provider_store_revision,
                            provider: provider.as_ref().clone(),
                        },
                        &timestamp_from_ms(now_ms),
                    )
                }
            }
            ApiAccountProviderSelectionV2::Existing { provider_id } => {
                if request.keychain_secret.is_some() {
                    return Err(AppError::new(
                        "API_ACCOUNT_SHARED_SECRET_REJECTED",
                        "existing Provider credentials cannot be changed by Account creation",
                    ));
                }
                list_provider_views_service_v2(home_root)?
                    .into_iter()
                    .find(|provider| provider.id == *provider_id)
                    .ok_or_else(|| AppError::new("PROVIDER_NOT_FOUND", provider_id))
            }
        }
    })();
    let provider_view = match provider_result {
        Ok(provider) => provider,
        Err(error) => {
            compensate_api_account_creation(home_root, &request.plan_id, &plan, None)?;
            return Err(error);
        }
    };
    update_api_account_journal(
        home_root,
        &request.plan_id,
        ApiAccountJournalStageV2::ProviderCreated,
    )?;
    if fault == Some(ApiAccountFaultPoint::AfterProviderCreated) {
        compensate_api_account_creation(
            home_root,
            &request.plan_id,
            &plan,
            created_provider.then_some(&provider_view),
        )?;
        return Err(api_account_fault());
    }
    if fault == Some(ApiAccountFaultPoint::CrashAfterProviderCreated) {
        return Err(AppError::new(
            "API_ACCOUNT_CRASH_SIMULATED",
            "simulated process interruption after Provider creation",
        ));
    }

    let attach_plan = match plan_attach_service_v2(
        home_root,
        &account.home_path.join("config.toml"),
        PlanAttachRequestV2 {
            profile_id: plan.request.account_name.clone(),
            provider_id: plan.provider_id.clone(),
            selected_model: plan.request.selected_model.clone(),
        },
        state,
        now_ms,
    ) {
        Ok(value) => value,
        Err(error) => {
            compensate_api_account_creation(
                home_root,
                &request.plan_id,
                &plan,
                created_provider.then_some(&provider_view),
            )?;
            return Err(error);
        }
    };
    if !attach_plan.blockers.is_empty() {
        compensate_api_account_creation(
            home_root,
            &request.plan_id,
            &plan,
            created_provider.then_some(&provider_view),
        )?;
        return Err(AppError::new(
            "API_ACCOUNT_NOT_READY",
            "API Account attach plan has readiness blockers",
        ));
    }
    let attach = match execute_attach_service_v2(
        home_root,
        ExecuteAttachRequestV2 {
            plan_id: attach_plan.plan_id,
            fingerprint: attach_plan.fingerprint,
        },
        state,
        now_ms,
    ) {
        Ok(value) => value,
        Err(error) => {
            compensate_api_account_creation(
                home_root,
                &request.plan_id,
                &plan,
                created_provider.then_some(&provider_view),
            )?;
            return Err(error);
        }
    };
    let binding = list_binding_views_service_v2(home_root)?
        .into_iter()
        .find(|binding| binding.profile_id == plan.request.account_name)
        .ok_or_else(|| AppError::new("PROFILE_BINDING_NOT_FOUND", &plan.request.account_name))?;
    clear_api_account_journal(home_root, &request.plan_id)?;
    state.api_account_plans.remove(&request.plan_id);
    Ok(ApiAccountExecutionViewV2 {
        account,
        provider: provider_view,
        binding,
        attach,
    })
}

pub fn plan_api_account_model_switch_service_v2(
    home_root: &Path,
    profile_id: &str,
    selected_model: &str,
    state: &mut ProviderApiV2State,
    now_ms: u64,
) -> Result<ProfileAttachPlanView> {
    let account = super::account::list_accounts(home_root)?
        .into_iter()
        .find(|account| account.id == profile_id)
        .ok_or_else(|| AppError::new("ACCOUNT_NOT_FOUND", profile_id))?;
    let binding = list_binding_views_service_v2(home_root)?
        .into_iter()
        .find(|binding| binding.profile_id == profile_id)
        .ok_or_else(|| AppError::new("PROFILE_BINDING_NOT_FOUND", profile_id))?;
    plan_attach_service_v2(
        home_root,
        &account.codex_home.join("config.toml"),
        PlanAttachRequestV2 {
            profile_id: profile_id.into(),
            provider_id: binding.provider_id,
            selected_model: selected_model.into(),
        },
        state,
        now_ms,
    )
}

pub fn execute_api_account_model_switch_service_v2(
    home_root: &Path,
    plan_id: &str,
    fingerprint: &str,
    state: &mut ProviderApiV2State,
    now_ms: u64,
) -> Result<AttachExecutionView> {
    execute_attach_service_v2(
        home_root,
        ExecuteAttachRequestV2 {
            plan_id: plan_id.into(),
            fingerprint: fingerprint.into(),
        },
        state,
        now_ms,
    )
}

pub fn delete_api_account_service_v2(
    home_root: &Path,
    profile_id: &str,
    state: &mut ProviderApiV2State,
    now_ms: u64,
) -> Result<super::account::DeleteAccountResult> {
    delete_api_account_service_v2_with_fault(home_root, profile_id, state, now_ms, None)
}

#[doc(hidden)]
pub fn delete_api_account_service_v2_with_fault(
    home_root: &Path,
    profile_id: &str,
    state: &mut ProviderApiV2State,
    now_ms: u64,
    fault: Option<ApiAccountFaultPoint>,
) -> Result<super::account::DeleteAccountResult> {
    let binding = list_binding_views_service_v2(home_root)?
        .into_iter()
        .find(|binding| binding.profile_id == profile_id)
        .ok_or_else(|| AppError::new("PROFILE_BINDING_NOT_FOUND", profile_id))?;
    let operation_id = format!("delete-{profile_id}-{}", uuid::Uuid::new_v4());
    create_api_account_delete_journal(home_root, &operation_id, &binding)?;
    let detach = match plan_detach_service_v2(home_root, profile_id, state, now_ms) {
        Ok(plan) => plan,
        Err(error) => {
            clear_api_account_journal(home_root, &operation_id)?;
            return Err(error);
        }
    };
    if let Err(error) = execute_detach_service_v2(
        home_root,
        ExecuteDetachRequestV2 {
            plan_id: detach.plan_id,
            fingerprint: detach.fingerprint,
        },
        state,
        now_ms,
    ) {
        clear_api_account_journal(home_root, &operation_id)?;
        return Err(error);
    }
    update_api_account_journal(home_root, &operation_id, ApiAccountJournalStageV2::Detached)?;
    if fault == Some(ApiAccountFaultPoint::CrashAfterDeleteDetach) {
        return Err(AppError::new(
            "API_ACCOUNT_DELETE_CRASH_SIMULATED",
            "simulated process interruption after API Account detach",
        ));
    }
    let deleted = match super::account::delete_account(
        home_root,
        &super::account::DeleteAccountRequest {
            profile_id: profile_id.into(),
        },
    ) {
        Ok(deleted) => deleted,
        Err(error) => {
            restore_detached_api_account(home_root, &binding, state, now_ms)?;
            clear_api_account_journal(home_root, &operation_id)?;
            return Err(error);
        }
    };
    update_api_account_journal(
        home_root,
        &operation_id,
        ApiAccountJournalStageV2::AccountDeleted,
    )?;
    if binding.provider_id == format!("account-{profile_id}")
        && !list_binding_views_service_v2(home_root)?
            .iter()
            .any(|current| current.provider_id == binding.provider_id)
    {
        delete_exclusive_provider(home_root, &binding.provider_id)?;
        update_api_account_journal(
            home_root,
            &operation_id,
            ApiAccountJournalStageV2::ProviderDeleted,
        )?;
    }
    clear_api_account_journal(home_root, &operation_id)?;
    Ok(deleted)
}

fn restore_detached_api_account(
    home_root: &Path,
    binding: &ProfileProviderBindingView,
    state: &mut ProviderApiV2State,
    now_ms: u64,
) -> Result<()> {
    let account = super::account::find_account(home_root, &binding.profile_id)?;
    let plan = plan_attach_service_v2(
        home_root,
        &account.codex_home.join("config.toml"),
        PlanAttachRequestV2 {
            profile_id: binding.profile_id.clone(),
            provider_id: binding.provider_id.clone(),
            selected_model: binding.selected_model.clone(),
        },
        state,
        now_ms,
    )?;
    execute_attach_service_v2(
        home_root,
        ExecuteAttachRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
        },
        state,
        now_ms,
    )?;
    Ok(())
}

fn rollback_api_account_creation(
    home_root: &Path,
    plan: &ApiAccountPlanV2,
    created_provider: Option<&ProviderProfileView>,
) -> Result<()> {
    if let Some(provider) = created_provider {
        delete_exclusive_provider(home_root, &provider.id)?;
    }
    super::account::rollback_created_account(home_root, &plan.request.account_name)
}

fn compensate_api_account_creation(
    home_root: &Path,
    operation_id: &str,
    plan: &ApiAccountPlanV2,
    created_provider: Option<&ProviderProfileView>,
) -> Result<()> {
    rollback_api_account_creation(home_root, plan, created_provider)?;
    clear_api_account_journal(home_root, operation_id)
}

pub fn recover_api_account_transactions_service_v2(
    home_root: &Path,
) -> Result<ApiAccountRecoveryReportV2> {
    let root =
        super::provider_runtime::ProviderHubPaths::for_home(home_root).ensure_canonical_root()?;
    recover_api_account_transactions_at_root_service_v2(&root)
}

pub fn recover_api_account_transactions_at_root_service_v2(
    root: &Path,
) -> Result<ApiAccountRecoveryReportV2> {
    let stores = provider_hub_stores_at_root(root)?;
    let snapshot = stores.api_account_journals.load_or_default()?;
    let mut recovered = 0;
    for record in snapshot.value.records.clone() {
        let home_root = PathBuf::from(&record.home_root);
        let binding_committed = stores
            .bindings
            .load_or_default()?
            .value
            .bindings
            .iter()
            .any(|binding| binding.profile_id == record.account_name);
        match record.kind {
            ApiAccountJournalKindV2::Create if !binding_committed => {
                let provider_exists = stores
                    .providers
                    .load_or_default()?
                    .value
                    .providers
                    .iter()
                    .any(|provider| provider.id == record.provider_id);
                if record.exclusive_provider && provider_exists {
                    delete_exclusive_provider(&home_root, &record.provider_id)?;
                }
                if super::account::list_accounts(&home_root)?
                    .iter()
                    .any(|account| account.id == record.account_name)
                {
                    super::account::rollback_created_account(&home_root, &record.account_name)?;
                }
            }
            ApiAccountJournalKindV2::Delete if !binding_committed => {
                let account_exists = super::account::list_accounts(&home_root)?
                    .iter()
                    .any(|account| account.id == record.account_name);
                if account_exists {
                    let mut state = ProviderApiV2State::default();
                    restore_detached_api_account(
                        &home_root,
                        &ProfileProviderBindingView {
                            profile_id: record.account_name.clone(),
                            provider_id: record.provider_id.clone(),
                            selected_model: record.selected_model.clone().ok_or_else(|| {
                                AppError::new(
                                    "API_ACCOUNT_JOURNAL_INVALID",
                                    "delete recovery journal has no selected model",
                                )
                            })?,
                            route_kind: RouteKindDto::Direct,
                            revision: 0,
                            provider_revision: 0,
                        },
                        &mut state,
                        0,
                    )?;
                } else if record.exclusive_provider
                    && stores
                        .providers
                        .load_or_default()?
                        .value
                        .providers
                        .iter()
                        .any(|provider| provider.id == record.provider_id)
                {
                    delete_exclusive_provider(&home_root, &record.provider_id)?;
                }
            }
            _ => {}
        }
        clear_api_account_journal_at_root(root, &record.operation_id)?;
        recovered += 1;
    }
    Ok(ApiAccountRecoveryReportV2 { recovered })
}

fn create_api_account_journal(
    home_root: &Path,
    operation_id: &str,
    plan: &ApiAccountPlanV2,
) -> Result<()> {
    let stores = provider_hub_stores(home_root)?;
    let mut snapshot = stores.api_account_journals.load_or_default()?;
    if snapshot
        .value
        .records
        .iter()
        .any(|record| record.operation_id == operation_id)
    {
        return Err(AppError::new(
            "API_ACCOUNT_JOURNAL_CONFLICT",
            "API Account operation already has a recovery journal",
        ));
    }
    snapshot.value.records.push(ApiAccountJournalRecordV2 {
        operation_id: operation_id.into(),
        home_root: home_root.to_string_lossy().into_owned(),
        account_name: plan.request.account_name.clone(),
        provider_id: plan.provider_id.clone(),
        exclusive_provider: matches!(
            plan.request.provider,
            ApiAccountProviderSelectionV2::New { .. }
        ),
        kind: ApiAccountJournalKindV2::Create,
        selected_model: Some(plan.request.selected_model.clone()),
        stage: ApiAccountJournalStageV2::Planned,
    });
    stores
        .api_account_journals
        .compare_and_swap(snapshot.revision, &snapshot.value)?;
    Ok(())
}

fn create_api_account_delete_journal(
    home_root: &Path,
    operation_id: &str,
    binding: &ProfileProviderBindingView,
) -> Result<()> {
    let stores = provider_hub_stores(home_root)?;
    let mut snapshot = stores.api_account_journals.load_or_default()?;
    snapshot.value.records.push(ApiAccountJournalRecordV2 {
        operation_id: operation_id.into(),
        home_root: home_root.to_string_lossy().into_owned(),
        account_name: binding.profile_id.clone(),
        provider_id: binding.provider_id.clone(),
        exclusive_provider: binding.provider_id == format!("account-{}", binding.profile_id),
        kind: ApiAccountJournalKindV2::Delete,
        selected_model: Some(binding.selected_model.clone()),
        stage: ApiAccountJournalStageV2::Planned,
    });
    stores
        .api_account_journals
        .compare_and_swap(snapshot.revision, &snapshot.value)?;
    Ok(())
}

fn update_api_account_journal(
    home_root: &Path,
    operation_id: &str,
    stage: ApiAccountJournalStageV2,
) -> Result<()> {
    let stores = provider_hub_stores(home_root)?;
    let mut snapshot = stores.api_account_journals.load_or_default()?;
    let record = snapshot
        .value
        .records
        .iter_mut()
        .find(|record| record.operation_id == operation_id)
        .ok_or_else(|| {
            AppError::new(
                "API_ACCOUNT_JOURNAL_MISSING",
                "API Account recovery journal is missing",
            )
        })?;
    record.stage = stage;
    stores
        .api_account_journals
        .compare_and_swap(snapshot.revision, &snapshot.value)?;
    Ok(())
}

fn clear_api_account_journal(home_root: &Path, operation_id: &str) -> Result<()> {
    let root =
        super::provider_runtime::ProviderHubPaths::for_home(home_root).ensure_canonical_root()?;
    clear_api_account_journal_at_root(&root, operation_id)
}

fn clear_api_account_journal_at_root(root: &Path, operation_id: &str) -> Result<()> {
    let stores = provider_hub_stores_at_root(root)?;
    let mut snapshot = stores.api_account_journals.load_or_default()?;
    let before = snapshot.value.records.len();
    snapshot
        .value
        .records
        .retain(|record| record.operation_id != operation_id);
    if snapshot.value.records.len() != before {
        stores
            .api_account_journals
            .compare_and_swap(snapshot.revision, &snapshot.value)?;
    }
    Ok(())
}

fn delete_exclusive_provider(home_root: &Path, provider_id: &str) -> Result<()> {
    let stores = provider_hub_stores(home_root)?;
    let repository = ProviderRepository::new(stores.providers);
    let snapshot = repository.load()?;
    let provider = snapshot
        .value
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .cloned()
        .ok_or_else(|| AppError::new("PROVIDER_NOT_FOUND", provider_id))?;
    repository.delete(snapshot.revision, provider_id)?;
    let source = match provider.upstream_auth {
        UpstreamAuth::Bearer { source } | UpstreamAuth::Header { source, .. } => Some(source),
        UpstreamAuth::None => None,
    };
    if let Some(source) = source {
        if let Ok(reference) = KeychainCredentialReference::try_from(&source) {
            KeychainCredentialService::new(Arc::new(SystemKeychainBackend)).revoke(&reference)?;
        }
    }
    Ok(())
}

fn provider_definition_for_plan(provider: &ProviderDefinitionDto) -> ProviderDefinitionDto {
    let mut provider = provider.clone();
    match &mut provider.upstream_auth {
        UpstreamAuthDto::Bearer { credential } | UpstreamAuthDto::Header { credential, .. }
            if matches!(credential, CredentialReferenceDto::None) =>
        {
            *credential = CredentialReferenceDto::Env {
                env_key: "LAM_API_ACCOUNT_PLAN_ONLY".into(),
            };
        }
        _ => {}
    }
    provider
}

fn provider_requires_keychain_secret(provider: &ProviderDefinitionDto) -> bool {
    matches!(
        &provider.upstream_auth,
        UpstreamAuthDto::Bearer {
            credential: CredentialReferenceDto::None
        } | UpstreamAuthDto::Header {
            credential: CredentialReferenceDto::None,
            ..
        }
    )
}

fn timestamp_from_ms(now_ms: u64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(now_ms.min(i64::MAX as u64) as i64)
        .unwrap_or(chrono::DateTime::<chrono::Utc>::UNIX_EPOCH)
        .to_rfc3339()
}

fn api_account_fault() -> AppError {
    AppError::new(
        "API_ACCOUNT_FAULT_INJECTED",
        "injected API Account transaction failure",
    )
}

fn ensure_empty_api_account_config(codex_home: &Path) -> Result<()> {
    let path = codex_home.join("config.toml");
    if path.exists() {
        return Err(AppError::new(
            "API_ACCOUNT_CONFIG_ALREADY_EXISTS",
            "new API Account config must start empty",
        ));
    }
    fs::write(&path, b"")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn rotate_provider_credential_system_service_v2(
    home_root: &Path,
    request: RotateProviderCredentialRequestV2,
    now: &str,
) -> Result<CredentialRotationViewV2> {
    rotate_provider_credential_service_v2(home_root, request, Arc::new(SystemKeychainBackend), now)
}

pub fn plan_attach_service_v2(
    home_root: &Path,
    config_path: &Path,
    request: PlanAttachRequestV2,
    state: &mut ProviderApiV2State,
    now_ms: u64,
) -> Result<ProfileAttachPlanView> {
    let gateway_available = super::provider_runtime::resolve_auth_helper_executable().is_ok();
    plan_attach_service_v2_with_resolver(
        home_root,
        config_path,
        request,
        state,
        now_ms,
        &ProductionCredentialResolver { home_root },
        gateway_available,
    )
}

pub fn plan_attach_service_v2_with_resolver(
    home_root: &Path,
    config_path: &Path,
    request: PlanAttachRequestV2,
    state: &mut ProviderApiV2State,
    now_ms: u64,
    resolver: &dyn ProviderCredentialResolver,
    gateway_available: bool,
) -> Result<ProfileAttachPlanView> {
    let stores = provider_hub_stores(home_root)?;
    let providers = stores.providers.load_or_default()?;
    let provider = providers
        .value
        .providers
        .iter()
        .find(|provider| provider.id == request.provider_id)
        .cloned()
        .ok_or_else(|| AppError::new("PROVIDER_NOT_FOUND", request.provider_id))?;
    let bindings = stores.bindings.load_or_default()?;
    let existing = bindings
        .value
        .bindings
        .iter()
        .find(|binding| binding.profile_id == request.profile_id);
    let source = match fs::read(config_path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    let route = plan_provider_route(RoutePlanInput {
        provider,
        selected_model: request.selected_model,
        adapters: AdapterCatalog::standard(),
    });
    let gateway_port = if route.route_kind == super::provider_binding::RouteKind::Direct {
        54_600
    } else {
        let gateway_state = GatewayStateRepository::new(stores.gateway_state.clone()).load()?;
        if gateway_state.exists {
            gateway_state.value.stable_port
        } else {
            choose_gateway_port()?
        }
    };
    let binding_drifted = existing.is_some_and(|binding| {
        binding.config_projection.applied_hash
            != super::provider_config_editor::config_hash(&source)
    });
    let gateway_binding_id = (route.route_kind == super::provider_binding::RouteKind::Gateway)
        .then(|| uuid::Uuid::new_v4().to_string());
    let auth_helper_path = if matches!(
        route.codex_auth,
        super::provider_credentials::DirectCodexAuth::Keychain { .. }
            | super::provider_credentials::DirectCodexAuth::ApprovedCommand { .. }
            | super::provider_credentials::DirectCodexAuth::Gateway
    ) {
        super::provider_runtime::resolve_auth_helper_executable()?
            .to_string_lossy()
            .into_owned()
    } else {
        String::new()
    };
    let credential_ready = match &route.provider.upstream_auth {
        UpstreamAuth::None => true,
        UpstreamAuth::Bearer { source } | UpstreamAuth::Header { source, .. } => {
            resolver.resolve(source).is_ok()
        }
    };
    let mut plan = plan_profile_attach(
        route,
        AttachPlanContext {
            profile_id: request.profile_id,
            config_path: config_path.to_string_lossy().into_owned(),
            provider_store_revision: providers.revision,
            expected_binding_revision: existing.map(|binding| binding.revision),
            binding_store_revision: bindings.revision,
            source_config_hash: super::provider_config_editor::config_hash(&source),
            binding_drifted,
            credential_ready,
            gateway: GatewayPlanContext {
                base_url: format!("http://127.0.0.1:{gateway_port}/v1"),
                available: gateway_available,
                endpoint_version: 1,
            },
            planner_options: BTreeMap::new(),
            auth_helper_path,
            provider_hub_root: stores.root.to_string_lossy().into_owned(),
            gateway_binding_id,
        },
    );
    let source_text = String::from_utf8(source).map_err(|_| {
        AppError::new(
            "CODEX_CONFIG_ENCODING",
            "Codex config must be valid UTF-8 for a safe preview",
        )
    })?;
    let preview_source = detach_managed_preview(&source_text, existing)?;
    let preview_hash = config_hash(preview_source.as_bytes());
    plan.redacted_preview = super::provider_config_editor::apply_projection(
        &preview_source,
        &preview_hash,
        &plan.config_projection,
    )?
    .contents;
    let plan_id = state.registry.issue(&plan, now_ms);
    let view = attach_plan_view(&plan_id, &plan, now_ms + 300_000);
    state.insert_attach(plan_id, plan);
    Ok(view)
}

fn detach_managed_preview(
    source: &str,
    existing: Option<&ProfileProviderBinding>,
) -> Result<String> {
    let Some(binding) = existing else {
        return Ok(source.to_string());
    };
    detach_projection(
        source,
        &ConfigManagedProjection {
            before_hash: binding.config_projection.before_hash.clone(),
            applied_hash: binding.config_projection.applied_hash.clone(),
            provider_id: binding.provider_id.clone(),
            previous_values: binding.config_projection.previous_values.clone(),
            managed_values: binding.config_projection.managed_values.clone(),
            provider_table_created_by_lam: binding.config_projection.provider_table_created_by_lam,
        },
    )
}

pub fn execute_attach_service_v2(
    home_root: &Path,
    request: ExecuteAttachRequestV2,
    state: &mut ProviderApiV2State,
    now_ms: u64,
) -> Result<AttachExecutionView> {
    let plan = state
        .attach_plans
        .get(&request.plan_id)
        .cloned()
        .ok_or_else(|| AppError::new("ATTACH_PLAN_UNKNOWN", "attach plan is unknown"))?;
    if request.fingerprint != plan.fingerprint {
        return Err(AppError::new(
            "ATTACH_PLAN_TAMPERED",
            "attach fingerprint does not match plan",
        ));
    }
    let stores = provider_hub_stores(home_root)?;
    ensure_gateway_state_for_plan(&stores.gateway_state, &plan)?;
    let gateway = GatewayBindingService::new(
        stores.gateway_bindings,
        KeychainCredentialService::new(Arc::new(SystemKeychainBackend)),
    );
    let coordinator = AttachTransactionCoordinator::new(
        stores.lock,
        stores.providers,
        stores.bindings,
        stores.journals,
        Arc::new(gateway),
        1,
    );
    let result =
        coordinator.execute_attach(&mut state.registry, &request.plan_id, &plan, now_ms, None)?;
    Ok(AttachExecutionView {
        operation_id: result.operation_id,
        state: journal_state_name(result.state).into(),
        idempotent: result.idempotent,
    })
}

pub fn plan_detach_service_v2(
    home_root: &Path,
    profile_id: &str,
    state: &mut ProviderApiV2State,
    now_ms: u64,
) -> Result<ProfileDetachPlanView> {
    let stores = provider_hub_stores(home_root)?;
    let bindings = stores.bindings.load_or_default()?;
    let binding = bindings
        .value
        .bindings
        .iter()
        .find(|binding| binding.profile_id == profile_id)
        .ok_or_else(|| AppError::new("PROFILE_BINDING_NOT_FOUND", profile_id))?;
    let hash = super::provider_config_editor::config_hash(&fs::read(
        &binding.config_projection.config_path,
    )?);
    let plan = plan_profile_detach(binding, bindings.revision, hash);
    let plan_id = state.registry.issue_detach(&plan, now_ms);
    let view = ProfileDetachPlanView {
        plan_id: plan_id.clone(),
        fingerprint: plan.fingerprint.clone(),
        expires_at_ms: now_ms + 300_000,
        profile_id: plan.profile_id.clone(),
        expected_binding_store_revision: plan.expected_binding_store_revision,
        expected_binding_revision: plan.expected_binding_revision,
        source_config_hash: plan.source_config_hash.clone(),
    };
    state.insert_detach(plan_id, plan);
    Ok(view)
}

pub fn execute_detach_service_v2(
    home_root: &Path,
    request: ExecuteDetachRequestV2,
    state: &mut ProviderApiV2State,
    now_ms: u64,
) -> Result<AttachExecutionView> {
    let plan = state
        .detach_plans
        .get(&request.plan_id)
        .cloned()
        .ok_or_else(|| AppError::new("ATTACH_PLAN_UNKNOWN", "detach plan is unknown"))?;
    if request.fingerprint != plan.fingerprint {
        return Err(AppError::new(
            "ATTACH_PLAN_TAMPERED",
            "detach fingerprint does not match plan",
        ));
    }
    let stores = provider_hub_stores(home_root)?;
    let gateway = GatewayBindingService::new(
        stores.gateway_bindings,
        KeychainCredentialService::new(Arc::new(SystemKeychainBackend)),
    );
    let coordinator = AttachTransactionCoordinator::new(
        stores.lock,
        stores.providers,
        stores.bindings,
        stores.journals,
        Arc::new(gateway),
        1,
    );
    let result =
        coordinator.execute_detach(&mut state.registry, &request.plan_id, &plan, now_ms, None)?;
    Ok(AttachExecutionView {
        operation_id: result.operation_id,
        state: journal_state_name(result.state).into(),
        idempotent: result.idempotent,
    })
}

fn attach_plan_view(
    plan_id: &str,
    plan: &ProfileAttachPlan,
    expires_at_ms: u64,
) -> ProfileAttachPlanView {
    ProfileAttachPlanView {
        plan_id: plan_id.into(),
        fingerprint: plan.fingerprint.clone(),
        expires_at_ms,
        profile_id: plan.profile_id.clone(),
        provider_id: plan.route.provider_id.clone(),
        selected_model: plan.route.selected_model.clone(),
        route_kind: match plan.route.route_kind {
            super::provider_binding::RouteKind::Direct => RouteKindDto::Direct,
            super::provider_binding::RouteKind::Gateway => RouteKindDto::Gateway,
        },
        blockers: plan
            .blockers
            .iter()
            .map(|value| value.code().into())
            .collect(),
        warnings: plan.warnings.clone(),
        operations: plan.journal_operations.clone(),
        redacted_preview: plan.redacted_preview.clone(),
        expected_provider_store_revision: plan.expected_provider_store_revision,
        expected_binding_store_revision: plan.expected_binding_store_revision,
        expected_binding_revision: plan.expected_binding_revision,
        source_config_hash: plan.source_config_hash.clone(),
    }
}

fn binding_view(binding: ProfileProviderBinding) -> ProfileProviderBindingView {
    ProfileProviderBindingView {
        profile_id: binding.profile_id,
        provider_id: binding.provider_id,
        selected_model: binding.selected_model,
        route_kind: match binding.route_kind {
            super::provider_binding::RouteKind::Direct => RouteKindDto::Direct,
            super::provider_binding::RouteKind::Gateway => RouteKindDto::Gateway,
        },
        revision: binding.revision,
        provider_revision: binding.provider_revision,
    }
}

fn journal_state_name(
    state: super::provider_attach_transaction::AttachJournalState,
) -> &'static str {
    use super::provider_attach_transaction::AttachJournalState;
    match state {
        AttachJournalState::Prepared => "prepared",
        AttachJournalState::ConfigCommitted => "config_committed",
        AttachJournalState::BindingCommitted => "binding_committed",
        AttachJournalState::Completed => "completed",
        AttachJournalState::RolledBack => "rolled_back",
        AttachJournalState::ManualIntervention => "manual_intervention",
    }
}

fn choose_gateway_port() -> Result<u16> {
    for port in 54_600..54_700 {
        if TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)).is_ok() {
            return Ok(port);
        }
    }
    Err(AppError::new(
        "GATEWAY_PORT_UNAVAILABLE",
        "no stable Gateway port is available",
    ))
}

fn ensure_gateway_state_for_plan(
    store: &VersionedFileStore<GatewayRuntimeState>,
    plan: &ProfileAttachPlan,
) -> Result<()> {
    if plan.route.route_kind != super::provider_binding::RouteKind::Gateway {
        return Ok(());
    }
    let endpoint = url::Url::parse(&plan.config_projection.base_url).map_err(|_| {
        AppError::new(
            "GATEWAY_ENDPOINT_INVALID",
            "planned Gateway endpoint is invalid",
        )
    })?;
    if endpoint.scheme() != "http" || endpoint.host_str() != Some("127.0.0.1") {
        return Err(AppError::new(
            "GATEWAY_ENDPOINT_INVALID",
            "planned Gateway endpoint is not IPv4 loopback",
        ));
    }
    let port = endpoint.port().ok_or_else(|| {
        AppError::new(
            "GATEWAY_ENDPOINT_INVALID",
            "planned Gateway endpoint has no port",
        )
    })?;
    let repository = GatewayStateRepository::new(store.clone());
    let current = repository.load()?;
    if current.exists {
        if current.value.stable_port != port {
            return Err(AppError::new(
                "GATEWAY_ENDPOINT_CHANGED",
                "Gateway stable port changed after dry-run",
            ));
        }
        return Ok(());
    }
    let probe = TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
        .map_err(|_| AppError::new("GATEWAY_PORT_IN_USE", "planned Gateway port is occupied"))?;
    drop(probe);
    repository.initialize(
        current.revision,
        port,
        env!("CARGO_PKG_VERSION"),
        1,
        1,
        &chrono::Utc::now().to_rfc3339(),
    )?;
    Ok(())
}

#[cfg(test)]
mod strict_codex_readiness_tests {
    use super::*;

    #[test]
    fn responses_gateway_route_requires_gateway_readiness() {
        let provider = build_provider(
            ProviderInput {
                id: "strict-codex-provider".into(),
                name: "Strict Codex Provider".into(),
                protocol: ProviderProtocol::Responses,
                base_url: "https://provider.example.test/v1".into(),
                default_model: "model-a".into(),
                models: vec![ProviderModel {
                    id: "model-a".into(),
                    label: "Model A".into(),
                    capabilities: None,
                }],
                upstream_auth: UpstreamAuth::None,
                adapter: AdapterConfig::None,
                compatibility_profile: None,
                codex: CodexProviderOptions {
                    route_via_gateway: true,
                    ..CodexProviderOptions::default()
                },
            },
            "2026-07-14T00:00:00Z",
        )
        .unwrap();
        let resolver = ProductionCredentialResolver {
            home_root: Path::new("/nonexistent"),
        };

        assert_eq!(
            provider_readiness_blockers(&provider, &resolver, false),
            ["GATEWAY_UNAVAILABLE"]
        );
    }
}
