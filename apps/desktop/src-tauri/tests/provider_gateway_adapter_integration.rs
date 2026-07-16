use localagentmanager_core::adapters::registry::{
    AdapterRegistry, AdapterRequirement, AdapterVersion, ResponsesToChatCompletionsAdapter,
    WireProtocol,
};
use std::fs;
use std::path::Path;
use std::sync::Arc;

#[test]
fn production_adapter_registry_resolves_each_controlled_compatibility_profile() {
    let mut registry = AdapterRegistry::new();
    for policy in [
        "generic-openai-compatible-v1",
        "deepseek-chat-completions-v1",
    ] {
        registry
            .register(Arc::new(ResponsesToChatCompletionsAdapter::new(policy)))
            .unwrap();
        let adapter = registry
            .resolve(&AdapterRequirement {
                id: "responses_to_chat_completions".into(),
                minimum_version: AdapterVersion::new(1, 0, 0),
                source: WireProtocol::Responses,
                target: WireProtocol::ChatCompletions,
                compatibility_policy: policy.into(),
            })
            .unwrap();
        let exchange = adapter.begin_exchange().unwrap();
        assert!(exchange.response_id().starts_with("exchange-"));
    }
}

#[test]
fn pinned_codex_followup_proves_reasoning_metadata_is_not_representable_without_state() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join(
            "tests/fixtures/codex-gateway-contract/normal/function-tool-followup-request.json",
        ))
        .unwrap(),
    )
    .unwrap();
    let input = fixture["body"]["input"].as_array().unwrap();
    assert!(input.iter().any(|item| item["type"] == "function_call"));
    assert!(input.iter().all(|item| item["type"] != "reasoning"));
    assert_eq!(fixture["body"]["reasoning"], serde_json::Value::Null);

    let route = fs::read_to_string(root.join("src/services/gateway/routes.rs")).unwrap();
    assert!(route.contains("ADAPTER_REASONING_HISTORY_UNREPRESENTABLE"));
    assert!(!route.contains("selected_model.contains"));
}
