use super::provider_v2::{CapabilityDeclaration, CapabilitySupport};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityVerificationKey {
    pub provider_id: String,
    pub model_id: String,
    pub endpoint_hash: String,
    pub adapter_id: String,
    pub policy_id: String,
    pub suite_version: String,
    pub fingerprint: String,
}

impl CapabilityVerificationKey {
    pub fn new(
        provider_id: &str,
        model_id: &str,
        endpoint: &str,
        adapter_id: &str,
        policy_id: &str,
        suite_version: &str,
    ) -> Self {
        let endpoint_hash = sha256(endpoint.as_bytes());
        let fingerprint = sha256(
            serde_json::to_vec(&(
                provider_id,
                model_id,
                &endpoint_hash,
                adapter_id,
                policy_id,
                suite_version,
            ))
            .expect("capability verification tuple is serializable")
            .as_slice(),
        );
        Self {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
            endpoint_hash,
            adapter_id: adapter_id.into(),
            policy_id: policy_id.into(),
            suite_version: suite_version.into(),
            fingerprint,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceVerdict {
    Passed,
    Failed,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityEvidence {
    pub key: CapabilityVerificationKey,
    pub verdict: EvidenceVerdict,
    pub verified: CapabilityDeclaration,
    pub fixture_ids: Vec<String>,
    pub observed_at_ms: u64,
    pub expires_at_ms: u64,
}

#[derive(Debug, Clone)]
pub struct CapabilityResolutionInput {
    pub declared: CapabilityDeclaration,
    pub adapter_supported: CapabilityDeclaration,
    pub user_override: Option<CapabilityDeclaration>,
    pub expected_key: CapabilityVerificationKey,
    pub evidence: Option<CapabilityEvidence>,
    pub now_ms: u64,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EffectiveCapability {
    Supported,
    Partial,
    Unsupported,
    Unknown,
}

impl From<CapabilitySupport> for EffectiveCapability {
    fn from(value: CapabilitySupport) -> Self {
        match value {
            CapabilitySupport::Supported => Self::Supported,
            CapabilitySupport::Partial => Self::Partial,
            CapabilitySupport::Unsupported => Self::Unsupported,
            CapabilitySupport::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedCapability {
    pub effective: EffectiveCapability,
    pub provenance: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveCapabilities {
    pub streaming: ResolvedCapability,
    pub function_tools: ResolvedCapability,
    pub structured_outputs: ResolvedCapability,
    pub reasoning: ResolvedCapability,
    pub evidence_current: bool,
    pub evidence_fingerprint: Option<String>,
    pub evidence_fixture_ids: Vec<String>,
}

pub fn resolve_capabilities(input: CapabilityResolutionInput) -> EffectiveCapabilities {
    let current_evidence = input.evidence.as_ref().filter(|evidence| {
        evidence.verdict == EvidenceVerdict::Passed
            && evidence.key == input.expected_key
            && evidence.observed_at_ms <= input.now_ms
            && evidence.expires_at_ms >= input.now_ms
    });
    let user = input.user_override.as_ref();
    let resolve = |declared, adapter, verified, user_override| {
        resolve_one(declared, adapter, verified, user_override)
    };
    EffectiveCapabilities {
        streaming: resolve(
            input.declared.streaming,
            input.adapter_supported.streaming,
            current_evidence.map(|value| value.verified.streaming),
            user.map(|value| value.streaming),
        ),
        function_tools: resolve(
            input.declared.function_tools,
            input.adapter_supported.function_tools,
            current_evidence.map(|value| value.verified.function_tools),
            user.map(|value| value.function_tools),
        ),
        structured_outputs: resolve(
            input.declared.structured_outputs,
            input.adapter_supported.structured_outputs,
            current_evidence.map(|value| value.verified.structured_outputs),
            user.map(|value| value.structured_outputs),
        ),
        reasoning: resolve(
            input.declared.reasoning,
            input.adapter_supported.reasoning,
            current_evidence.map(|value| value.verified.reasoning),
            user.map(|value| value.reasoning),
        ),
        evidence_current: current_evidence.is_some(),
        evidence_fingerprint: current_evidence.map(|value| value.key.fingerprint.clone()),
        evidence_fixture_ids: current_evidence
            .map(|value| value.fixture_ids.clone())
            .unwrap_or_default(),
    }
}

fn resolve_one(
    declared: CapabilitySupport,
    adapter: CapabilitySupport,
    verified: Option<CapabilitySupport>,
    user_override: Option<CapabilitySupport>,
) -> ResolvedCapability {
    if adapter == CapabilitySupport::Unsupported {
        return resolved(CapabilitySupport::Unsupported, "adapter");
    }

    let (mut effective, mut provenance) = if let Some(verified) = verified {
        (verified, "evidence")
    } else {
        (declared, "declared")
    };
    if is_more_restrictive(adapter, effective) {
        effective = adapter;
        provenance = "adapter";
    }
    if let Some(user_override) = user_override {
        if is_more_restrictive(user_override, effective) {
            effective = user_override;
            provenance = "user_override";
        }
    }
    resolved(effective, provenance)
}

fn is_more_restrictive(candidate: CapabilitySupport, current: CapabilitySupport) -> bool {
    support_rank(candidate) < support_rank(current)
}

fn support_rank(value: CapabilitySupport) -> u8 {
    match value {
        CapabilitySupport::Unsupported => 0,
        CapabilitySupport::Unknown => 1,
        CapabilitySupport::Partial => 2,
        CapabilitySupport::Supported => 3,
    }
}

fn resolved(value: CapabilitySupport, provenance: &str) -> ResolvedCapability {
    ResolvedCapability {
        effective: value.into(),
        provenance: provenance.into(),
    }
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
