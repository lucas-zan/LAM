use localagentmanager_core::provider_capability::{
    resolve_capabilities, CapabilityEvidence, CapabilityResolutionInput, CapabilityVerificationKey,
    EffectiveCapability, EvidenceVerdict,
};
use localagentmanager_core::provider_v2::{CapabilityDeclaration, CapabilitySupport};

fn declaration(value: CapabilitySupport) -> CapabilityDeclaration {
    CapabilityDeclaration {
        streaming: value,
        function_tools: value,
        structured_outputs: value,
        reasoning: value,
    }
}

fn key(model: &str, endpoint: &str) -> CapabilityVerificationKey {
    CapabilityVerificationKey::new(
        "provider-a",
        model,
        endpoint,
        "responses_to_chat_completions",
        "deepseek_chat_completions",
        "phase4-suite-v1",
    )
}

#[test]
fn adapter_impossibility_wins_and_user_override_cannot_upgrade_it() {
    let expected_key = key("deepseek-chat", "https://api.example.test/v1");
    let resolved = resolve_capabilities(CapabilityResolutionInput {
        declared: declaration(CapabilitySupport::Supported),
        adapter_supported: CapabilityDeclaration {
            streaming: CapabilitySupport::Supported,
            function_tools: CapabilitySupport::Supported,
            structured_outputs: CapabilitySupport::Unsupported,
            reasoning: CapabilitySupport::Partial,
        },
        user_override: Some(declaration(CapabilitySupport::Supported)),
        expected_key: expected_key.clone(),
        evidence: Some(CapabilityEvidence {
            key: expected_key,
            verdict: EvidenceVerdict::Passed,
            verified: declaration(CapabilitySupport::Supported),
            fixture_ids: vec!["synthetic-capability-fixture".into()],
            observed_at_ms: 1_000,
            expires_at_ms: 10_000,
        }),
        now_ms: 2_000,
    });

    assert_eq!(resolved.streaming.effective, EffectiveCapability::Supported);
    assert_eq!(
        resolved.structured_outputs.effective,
        EffectiveCapability::Unsupported
    );
    assert_eq!(resolved.structured_outputs.provenance, "adapter");
    assert!(resolved.evidence_current);
}

#[test]
fn stale_or_mismatched_evidence_is_ignored_and_every_key_component_invalidates() {
    let expected = key("model-a", "https://api.example.test/v1");
    for stale_key in [
        key("model-b", "https://api.example.test/v1"),
        key("model-a", "https://other.example.test/v1"),
        CapabilityVerificationKey::new(
            "provider-b",
            "model-a",
            "https://api.example.test/v1",
            "responses_to_chat_completions",
            "deepseek_chat_completions",
            "phase4-suite-v1",
        ),
        CapabilityVerificationKey::new(
            "provider-a",
            "model-a",
            "https://api.example.test/v1",
            "other-adapter",
            "deepseek_chat_completions",
            "phase4-suite-v1",
        ),
        CapabilityVerificationKey::new(
            "provider-a",
            "model-a",
            "https://api.example.test/v1",
            "responses_to_chat_completions",
            "openai_chat_completions",
            "phase4-suite-v1",
        ),
        CapabilityVerificationKey::new(
            "provider-a",
            "model-a",
            "https://api.example.test/v1",
            "responses_to_chat_completions",
            "deepseek_chat_completions",
            "phase4-suite-v2",
        ),
    ] {
        let resolved = resolve_capabilities(CapabilityResolutionInput {
            declared: declaration(CapabilitySupport::Unknown),
            adapter_supported: declaration(CapabilitySupport::Supported),
            user_override: None,
            expected_key: expected.clone(),
            evidence: Some(CapabilityEvidence {
                key: stale_key,
                verdict: EvidenceVerdict::Passed,
                verified: declaration(CapabilitySupport::Supported),
                fixture_ids: vec!["fixture".into()],
                observed_at_ms: 1_000,
                expires_at_ms: 10_000,
            }),
            now_ms: 2_000,
        });
        assert!(!resolved.evidence_current);
        assert_eq!(resolved.streaming.effective, EffectiveCapability::Unknown);
    }

    let expired = resolve_capabilities(CapabilityResolutionInput {
        declared: declaration(CapabilitySupport::Unknown),
        adapter_supported: declaration(CapabilitySupport::Supported),
        user_override: None,
        expected_key: expected.clone(),
        evidence: Some(CapabilityEvidence {
            key: expected,
            verdict: EvidenceVerdict::Passed,
            verified: declaration(CapabilitySupport::Supported),
            fixture_ids: vec!["fixture".into()],
            observed_at_ms: 1_000,
            expires_at_ms: 1_999,
        }),
        now_ms: 2_000,
    });
    assert!(!expired.evidence_current);
}

#[test]
fn evidence_serialization_contains_only_sanitized_references() {
    let evidence = CapabilityEvidence {
        key: key("model-a", "https://api.example.test/v1"),
        verdict: EvidenceVerdict::Passed,
        verified: declaration(CapabilitySupport::Supported),
        fixture_ids: vec!["fixture-sha256:abcd".into()],
        observed_at_ms: 1_000,
        expires_at_ms: 2_000,
    };
    let json = serde_json::to_string(&evidence).unwrap();
    assert!(json.contains("fixture-sha256:abcd"));
    for forbidden in ["authorization", "api_key", "prompt", "reasoning_content"] {
        assert!(!json.to_ascii_lowercase().contains(forbidden));
    }
}
