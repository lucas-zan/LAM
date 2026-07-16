use localagentmanager_core::gateway::catalog::build_codex_model_catalog;
use localagentmanager_core::provider_v2::ProviderModel;

fn model(id: &str, label: &str) -> ProviderModel {
    ProviderModel {
        id: id.into(),
        label: label.into(),
        capabilities: None,
    }
}

#[test]
fn catalog_uses_the_exact_tested_nonempty_codex_item_contract() {
    let catalog =
        build_codex_model_catalog(&[model("z-model", "Z Model"), model("a-model", "A Model")])
            .unwrap();
    let value = serde_json::to_value(catalog).unwrap();
    let models = value["models"].as_array().unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0]["slug"], "a-model");
    assert_eq!(models[1]["slug"], "z-model");

    let first = models[0].as_object().unwrap();
    for required in [
        "slug",
        "display_name",
        "supported_reasoning_levels",
        "shell_type",
        "visibility",
        "supported_in_api",
        "priority",
        "base_instructions",
        "supports_reasoning_summaries",
        "support_verbosity",
        "truncation_policy",
        "supports_parallel_tool_calls",
        "experimental_supported_tools",
    ] {
        assert!(first.contains_key(required), "missing {required}");
    }
    assert!(!first.contains_key("id"));
    assert!(!first.contains_key("owned_by"));
    assert_eq!(first["supported_reasoning_levels"], serde_json::json!([]));
    assert_eq!(first["supports_reasoning_summaries"], false);
    assert_eq!(first["support_verbosity"], false);
    assert_eq!(first["supports_parallel_tool_calls"], false);
    assert_eq!(first["experimental_supported_tools"], serde_json::json!([]));
    assert!(first["base_instructions"]
        .as_str()
        .is_some_and(|value| !value.trim().is_empty()));
}

#[test]
fn catalog_rejects_blank_and_duplicate_model_slugs() {
    let blank = build_codex_model_catalog(&[model(" ", "Blank")]).unwrap_err();
    assert_eq!(blank.code, "CODEX_MODEL_SLUG_INVALID");

    let duplicate =
        build_codex_model_catalog(&[model("same", "One"), model("same", "Two")]).unwrap_err();
    assert_eq!(duplicate.code, "CODEX_MODEL_SLUG_DUPLICATE");
}
