use localagentmanager_core::gateway::catalog::{
    build_codex_model_catalog, build_codex_model_catalog_with_defaults, CodexModelDefaultsCatalog,
};
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

#[test]
fn catalog_uses_matching_native_codex_context_and_leaves_auto_compact_unset() {
    let defaults = CodexModelDefaultsCatalog::from_json(
        r#"{
          "models": [{
            "slug": "gpt-5.6-sol",
            "context_window": 272000,
            "max_context_window": 272000,
            "auto_compact_token_limit": null,
            "effective_context_window_percent": 95
          }]
        }"#,
    )
    .unwrap();
    let catalog =
        build_codex_model_catalog_with_defaults(&[model("gpt-5.6-sol", "GPT 5.6 Sol")], &defaults)
            .unwrap();
    let value = serde_json::to_value(catalog).unwrap();
    let info = value["models"][0].as_object().unwrap();
    assert_eq!(info["context_window"], 272000);
    assert_eq!(info["max_context_window"], 272000);
    assert_eq!(info["effective_context_window_percent"], 95);
    assert!(!info.contains_key("auto_compact_token_limit"));
}

#[test]
fn native_catalog_omits_unknown_or_invalid_context_metadata() {
    let defaults = CodexModelDefaultsCatalog::from_json(
        r#"{
          "models": [
            {"slug":"zero","context_window":0,"max_context_window":-1},
            {"slug":"valid","context_window":128000,"max_context_window":128000}
          ]
        }"#,
    )
    .unwrap();
    let catalog = build_codex_model_catalog_with_defaults(
        &[model("unknown", "Unknown"), model("zero", "Zero")],
        &defaults,
    )
    .unwrap();
    let value = serde_json::to_value(catalog).unwrap();
    for info in value["models"].as_array().unwrap() {
        let info = info.as_object().unwrap();
        assert!(!info.contains_key("context_window"));
        assert!(!info.contains_key("max_context_window"));
        assert!(!info.contains_key("auto_compact_token_limit"));
    }
    assert!(CodexModelDefaultsCatalog::from_json("not-json").is_err());
}

#[test]
fn bundled_catalog_provides_context_without_a_normal_codex_cache() {
    let defaults = CodexModelDefaultsCatalog::builtin().unwrap();
    let catalog =
        build_codex_model_catalog_with_defaults(&[model("gpt-5.6-sol", "GPT 5.6 Sol")], &defaults)
            .unwrap();
    let value = serde_json::to_value(catalog).unwrap();
    assert_eq!(value["models"][0]["context_window"], 272000);
    assert_eq!(value["models"][0]["max_context_window"], 272000);
    assert_eq!(value["models"][0]["effective_context_window_percent"], 95);
    assert!(value["models"][0]
        .as_object()
        .is_some_and(|model| !model.contains_key("auto_compact_token_limit")));
}

#[test]
fn local_catalog_overlays_matching_bundled_models_and_keeps_other_fallbacks() {
    let defaults = CodexModelDefaultsCatalog::builtin()
        .unwrap()
        .overlay_json(
            r#"{"models":[{"slug":"gpt-5.6-sol","context_window":196000,"max_context_window":196000,"effective_context_window_percent":90}]}"#,
        )
        .unwrap();
    let catalog = build_codex_model_catalog_with_defaults(
        &[model("gpt-5.6-sol", "Sol"), model("gpt-5.4", "GPT 5.4")],
        &defaults,
    )
    .unwrap();
    let value = serde_json::to_value(catalog).unwrap();
    assert_eq!(value["models"][1]["context_window"], 196000);
    assert_eq!(value["models"][0]["context_window"], 272000);

    let fallback = CodexModelDefaultsCatalog::builtin().unwrap();
    assert!(fallback.overlay_json("not-json").is_err());
}
