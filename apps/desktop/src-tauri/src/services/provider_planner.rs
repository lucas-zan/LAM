use super::error::{AppError, Result};
use super::provider_binding::ProfileProviderBinding;
use super::provider_binding::RouteKind;
use super::provider_config_editor::ConfigProjectionSpec;
use super::provider_credentials::{direct_codex_auth, DirectCodexAuth};
use super::provider_runtime::{materialize_codex_auth, AuthHelperRuntime};
use super::provider_v2::{
    AdapterConfig, CapabilityDeclaration, ProviderProfileV2, ProviderProtocol,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AdapterDescriptor {
    pub source: ProviderProtocol,
    pub target: ProviderProtocol,
    pub version: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct AdapterCatalog(pub BTreeMap<String, AdapterDescriptor>);

impl AdapterCatalog {
    pub fn empty() -> Self {
        Self::default()
    }
    pub fn standard() -> Self {
        Self(BTreeMap::from([(
            "responses_to_chat_completions".into(),
            AdapterDescriptor {
                source: ProviderProtocol::ChatCompletions,
                target: ProviderProtocol::Responses,
                version: "1".into(),
            },
        )]))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RoutePlanInput {
    pub provider: ProviderProfileV2,
    pub selected_model: String,
    pub adapters: AdapterCatalog,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanBlocker {
    ModelNotFound,
    AdapterRequired,
    AdapterRegistryMismatch,
    UnsupportedCredentialRoute,
    CredentialMissing,
    BindingDrift,
    GatewayUnavailable,
    AuthRuntimeUnavailable,
}

impl PlanBlocker {
    pub fn code(self) -> &'static str {
        match self {
            Self::ModelNotFound => "model_not_found",
            Self::AdapterRequired => "adapter_required",
            Self::AdapterRegistryMismatch => "adapter_registry_mismatch",
            Self::UnsupportedCredentialRoute => "unsupported_credential_route",
            Self::CredentialMissing => "credential_missing",
            Self::BindingDrift => "binding_drift",
            Self::GatewayUnavailable => "gateway_unavailable",
            Self::AuthRuntimeUnavailable => "auth_runtime_unavailable",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProviderRoutePlan {
    pub provider_id: String,
    pub selected_model: String,
    pub route_kind: RouteKind,
    pub upstream_endpoint: String,
    pub upstream_protocol: ProviderProtocol,
    pub adapter_id: Option<String>,
    pub adapter_version: Option<String>,
    pub codex_auth: DirectCodexAuth,
    pub effective_capabilities: CapabilityDeclaration,
    pub blockers: Vec<PlanBlocker>,
    pub warnings: Vec<String>,
    pub provider: ProviderProfileV2,
}

pub fn plan_provider_route(input: RoutePlanInput) -> ProviderRoutePlan {
    let mut blockers = Vec::new();
    if !input
        .provider
        .models
        .iter()
        .any(|model| model.id == input.selected_model)
    {
        blockers.push(PlanBlocker::ModelNotFound);
    }
    let (route_kind, adapter_id, adapter_version) =
        match (&input.provider.protocol, &input.provider.adapter) {
            (ProviderProtocol::Responses, _) if input.provider.codex.route_via_gateway => {
                (RouteKind::Gateway, None, None)
            }
            (ProviderProtocol::Responses, _) => (RouteKind::Direct, None, None),
            (ProviderProtocol::ChatCompletions, AdapterConfig::None) => {
                blockers.push(PlanBlocker::AdapterRequired);
                (RouteKind::Gateway, None, None)
            }
            (ProviderProtocol::ChatCompletions, AdapterConfig::Local { adapter_id, .. }) => {
                let descriptor = input.adapters.0.get(adapter_id);
                if !descriptor.is_some_and(|v| {
                    v.source == ProviderProtocol::ChatCompletions
                        && v.target == ProviderProtocol::Responses
                }) {
                    blockers.push(PlanBlocker::AdapterRegistryMismatch);
                }
                (
                    RouteKind::Gateway,
                    Some(adapter_id.clone()),
                    descriptor.map(|v| v.version.clone()),
                )
            }
        };
    let codex_auth = if route_kind == RouteKind::Direct {
        direct_codex_auth(&input.provider.upstream_auth).unwrap_or_else(|_| {
            blockers.push(PlanBlocker::UnsupportedCredentialRoute);
            DirectCodexAuth::None
        })
    } else {
        DirectCodexAuth::Gateway
    };
    ProviderRoutePlan {
        provider_id: input.provider.id.clone(),
        selected_model: input.selected_model,
        route_kind,
        upstream_endpoint: input.provider.base_url.clone(),
        upstream_protocol: input.provider.protocol,
        adapter_id,
        adapter_version,
        codex_auth,
        effective_capabilities: input.provider.capabilities.clone(),
        blockers,
        warnings: Vec::new(),
        provider: input.provider,
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct GatewayPlanContext {
    pub base_url: String,
    pub available: bool,
    pub endpoint_version: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AttachPlanContext {
    pub profile_id: String,
    pub config_path: String,
    pub provider_store_revision: u64,
    pub expected_binding_revision: Option<u64>,
    pub binding_store_revision: u64,
    pub source_config_hash: String,
    pub binding_drifted: bool,
    pub credential_ready: bool,
    pub gateway: GatewayPlanContext,
    pub planner_options: BTreeMap<String, String>,
    pub auth_helper_path: String,
    pub provider_hub_root: String,
    pub gateway_binding_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProfileAttachPlan {
    pub route: ProviderRoutePlan,
    pub profile_id: String,
    pub config_path: String,
    pub expected_provider_store_revision: u64,
    pub expected_binding_revision: Option<u64>,
    pub expected_binding_store_revision: u64,
    pub source_config_hash: String,
    pub gateway_endpoint_version: Option<u64>,
    pub gateway_binding_id: Option<String>,
    pub config_projection: ConfigProjectionSpec,
    pub journal_operations: Vec<String>,
    pub blockers: Vec<PlanBlocker>,
    pub warnings: Vec<String>,
    pub redacted_preview: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProfileDetachPlan {
    pub profile_id: String,
    pub expected_binding_store_revision: u64,
    pub expected_binding_revision: u64,
    pub source_config_hash: String,
    pub binding: ProfileProviderBinding,
    pub fingerprint: String,
}

pub fn plan_profile_detach(
    binding: &ProfileProviderBinding,
    binding_store_revision: u64,
    source_config_hash: String,
) -> ProfileDetachPlan {
    #[derive(Serialize)]
    struct Fingerprint<'a> {
        contract: &'static str,
        operation: &'static str,
        binding: &'a ProfileProviderBinding,
        binding_store_revision: u64,
        source_config_hash: &'a str,
    }
    let bytes = serde_json::to_vec(&Fingerprint {
        contract: "rpg-107-v1",
        operation: "detach",
        binding,
        binding_store_revision,
        source_config_hash: &source_config_hash,
    })
    .expect("serializable detach plan");
    ProfileDetachPlan {
        profile_id: binding.profile_id.clone(),
        expected_binding_store_revision: binding_store_revision,
        expected_binding_revision: binding.revision,
        source_config_hash,
        binding: binding.clone(),
        fingerprint: hex::encode(Sha256::digest(bytes)),
    }
}

pub fn plan_profile_attach(
    mut route: ProviderRoutePlan,
    context: AttachPlanContext,
) -> ProfileAttachPlan {
    let mut blockers = route.blockers.clone();
    if !context.credential_ready {
        blockers.push(PlanBlocker::CredentialMissing);
    }
    if context.binding_drifted {
        blockers.push(PlanBlocker::BindingDrift);
    }
    if route.route_kind == RouteKind::Gateway && !context.gateway.available {
        blockers.push(PlanBlocker::GatewayUnavailable);
    }
    let base_url = if route.route_kind == RouteKind::Gateway {
        context.gateway.base_url.clone()
    } else {
        route.upstream_endpoint.clone()
    };
    match materialize_codex_auth(
        &route.codex_auth,
        &AuthHelperRuntime {
            executable: context.auth_helper_path.clone().into(),
            state_root: context.provider_hub_root.clone().into(),
            profile_id: context.profile_id.clone(),
            gateway_binding_id: context.gateway_binding_id.clone(),
        },
    ) {
        Ok(auth) => route.codex_auth = auth,
        Err(_) => blockers.push(PlanBlocker::AuthRuntimeUnavailable),
    }
    let display_name = if route.route_kind == RouteKind::Gateway {
        format!("LAM Gateway · {}", route.provider.name)
    } else {
        route.provider.name.clone()
    };
    let config_projection = ConfigProjectionSpec {
        provider_id: route.provider_id.clone(),
        model: route.selected_model.clone(),
        display_name,
        base_url,
        auth: route.codex_auth.clone(),
        codex: route.provider.codex.clone(),
        gateway: route.route_kind == RouteKind::Gateway,
    };
    #[derive(Serialize)]
    struct Fingerprint<'a> {
        contract: &'static str,
        route: &'a ProviderRoutePlan,
        context: &'a AttachPlanContext,
        config: &'a ConfigProjectionSpec,
    }
    let bytes = serde_json::to_vec(&Fingerprint {
        contract: "rpg-106-v1",
        route: &route,
        context: &context,
        config: &config_projection,
    })
    .expect("serializable plan");
    let fingerprint = hex::encode(Sha256::digest(bytes));
    let preview = format!(
        "attach {} to {} via {:?}; auth=[REDACTED]",
        context.profile_id, route.provider_id, route.route_kind
    );
    ProfileAttachPlan {
        route,
        profile_id: context.profile_id,
        config_path: context.config_path,
        expected_provider_store_revision: context.provider_store_revision,
        expected_binding_revision: context.expected_binding_revision,
        expected_binding_store_revision: context.binding_store_revision,
        source_config_hash: context.source_config_hash,
        gateway_endpoint_version: (config_projection.gateway)
            .then_some(context.gateway.endpoint_version),
        gateway_binding_id: context.gateway_binding_id,
        config_projection,
        journal_operations: vec!["prepare_config".into(), "commit_binding".into()],
        blockers,
        warnings: Vec::new(),
        redacted_preview: preview,
        fingerprint,
    }
}

struct IssuedPlan {
    ticket: String,
    fingerprint: String,
    expires_at: u64,
    consumed: bool,
}
pub struct DryRunRegistry {
    ttl_ms: u64,
    capacity: usize,
    entries: VecDeque<IssuedPlan>,
}

impl DryRunRegistry {
    pub fn new(ttl_ms: u64, capacity: usize) -> Self {
        Self {
            ttl_ms,
            capacity: capacity.max(1),
            entries: VecDeque::new(),
        }
    }
    pub fn issue(&mut self, plan: &ProfileAttachPlan, now_ms: u64) -> String {
        self.issue_fingerprint(&plan.fingerprint, now_ms)
    }

    pub fn issue_detach(&mut self, plan: &ProfileDetachPlan, now_ms: u64) -> String {
        self.issue_fingerprint(&plan.fingerprint, now_ms)
    }

    fn issue_fingerprint(&mut self, fingerprint: &str, now_ms: u64) -> String {
        while self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        let ticket = Uuid::new_v4().to_string();
        self.entries.push_back(IssuedPlan {
            ticket: ticket.clone(),
            fingerprint: fingerprint.to_string(),
            expires_at: now_ms.saturating_add(self.ttl_ms),
            consumed: false,
        });
        ticket
    }
    pub fn validate(
        &self,
        ticket: &str,
        presented_fingerprint: &str,
        current_fingerprint: &str,
        now_ms: u64,
    ) -> Result<String> {
        let entry = self
            .entries
            .iter()
            .find(|v| v.ticket == ticket)
            .ok_or_else(|| AppError::new("ATTACH_PLAN_UNKNOWN", "dry-run ticket is unknown"))?;
        validate_entry(entry, presented_fingerprint, current_fingerprint, now_ms)?;
        Ok(entry.fingerprint.clone())
    }
    pub fn consume(
        &mut self,
        ticket: &str,
        presented_fingerprint: &str,
        current_fingerprint: &str,
        now_ms: u64,
    ) -> Result<String> {
        let entry = self
            .entries
            .iter_mut()
            .find(|v| v.ticket == ticket)
            .ok_or_else(|| AppError::new("ATTACH_PLAN_UNKNOWN", "dry-run ticket is unknown"))?;
        validate_entry(entry, presented_fingerprint, current_fingerprint, now_ms)?;
        entry.consumed = true;
        Ok(entry.fingerprint.clone())
    }
}

fn validate_entry(
    entry: &IssuedPlan,
    presented_fingerprint: &str,
    current_fingerprint: &str,
    now_ms: u64,
) -> Result<()> {
    if entry.consumed {
        return Err(AppError::new(
            "ATTACH_PLAN_REPLAYED",
            "dry-run ticket was already consumed",
        ));
    }
    if now_ms > entry.expires_at {
        return Err(AppError::new(
            "ATTACH_PLAN_EXPIRED",
            "dry-run ticket expired",
        ));
    }
    if presented_fingerprint != entry.fingerprint {
        return Err(AppError::new(
            "ATTACH_PLAN_TAMPERED",
            "dry-run fingerprint does not match ticket",
        ));
    }
    if current_fingerprint != entry.fingerprint {
        return Err(AppError::new(
            "ATTACH_PLAN_STALE",
            "execution state changed after dry-run",
        ));
    }
    Ok(())
}
