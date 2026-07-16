use localagentmanager_core::provider_config_editor::*;
use localagentmanager_core::provider_credentials::DirectCodexAuth;
use localagentmanager_core::provider_v2::CodexProviderOptions;
use std::fs;

fn spec() -> ConfigProjectionSpec {
    ConfigProjectionSpec {
        provider_id: "company.proxy".into(),
        model: "model-a".into(),
        display_name: "Company Proxy".into(),
        base_url: "https://proxy.example.test/v1".into(),
        auth: DirectCodexAuth::EnvKey {
            env_key: "COMPANY_API_KEY".into(),
        },
        codex: CodexProviderOptions::default(),
        gateway: false,
    }
}

#[test]
fn direct_projection_preserves_complex_user_toml_and_quotes_dotted_id() {
    let source = fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/legacy-provider-contract/config/official-provider.toml"),
    )
    .unwrap();
    let mut value = spec();
    value
        .codex
        .query_params
        .insert("api-version".into(), "2026-01-01".into());
    value
        .codex
        .env_http_headers
        .insert("X-Feature".into(), "FEATURE_ENV".into());
    value.codex.direct_request_max_retries = Some(2);
    let applied = apply_projection(&source, &config_hash(source.as_bytes()), &value).unwrap();
    assert!(applied
        .contents
        .contains("# preserve this trailing comment"));
    assert!(applied.contents.contains("[mcp_servers.fixture_tool]"));
    assert!(applied
        .contents
        .contains("[model_providers.\"company.proxy\"]"));
    assert!(applied.contents.contains("wire_api = \"responses\""));
    assert!(applied.contents.contains("env_key = \"COMPANY_API_KEY\""));
    assert!(applied.contents.contains("request_max_retries = 2"));
    assert!(applied.contents.contains("api-version"));
    assert!(applied.contents.contains("X-Feature"));
    assert!(!applied.contents.contains("experimental_bearer_token"));
}

#[test]
fn gateway_projection_forces_both_retries_zero_and_auth_is_mutually_exclusive() {
    let mut value = spec();
    value.gateway = true;
    value.auth = DirectCodexAuth::AuthCommand {
        approval_id: "gateway-binding-1".into(),
        command: "lam-auth-helper".into(),
        args: vec![
            "gateway-token".into(),
            "--binding".into(),
            "gateway-binding-1".into(),
        ],
    };
    let applied = apply_projection("", &config_hash(b""), &value).unwrap();
    assert!(applied.contents.contains("request_max_retries = 0"));
    assert!(applied.contents.contains("stream_max_retries = 0"));
    assert!(applied
        .contents
        .contains("[model_providers.\"company.proxy\".auth]"));
    assert!(applied.contents.contains("timeout_ms = 5000"));
    assert!(applied.contents.contains("refresh_interval_ms = 0"));
    assert!(!applied.contents.contains("env_key"));

    let conflict = "[model_providers.\"company.proxy\"]\nenv_key=\"X\"\nexperimental_bearer_token=\"secret\"\n";
    assert_eq!(
        apply_projection(conflict, &config_hash(conflict.as_bytes()), &spec())
            .unwrap_err()
            .code,
        "CODEX_CONFIG_SECRET_CONFLICT"
    );
    let existing_auth = "[model_providers.\"company.proxy\".auth]\ncommand=\"user-helper\"\n";
    assert_eq!(
        apply_projection(
            existing_auth,
            &config_hash(existing_auth.as_bytes()),
            &value
        )
        .unwrap_err()
        .code,
        "CODEX_CONFIG_AUTH_CONFLICT"
    );
    let static_header =
        "[model_providers.\"company.proxy\"]\nhttp_headers={ Authorization=\"secret\" }\n";
    assert_eq!(
        apply_projection(
            static_header,
            &config_hash(static_header.as_bytes()),
            &spec()
        )
        .unwrap_err()
        .code,
        "CODEX_CONFIG_STATIC_HEADERS_UNSUPPORTED"
    );
}

#[test]
fn hash_conflict_and_managed_drift_are_non_destructive_while_detach_preserves_unmanaged_additions()
{
    assert_eq!(
        apply_projection("model=\"old\"\n", "wrong", &spec())
            .unwrap_err()
            .code,
        "CODEX_CONFIG_CONFLICT"
    );
    let applied = apply_projection(
        "model=\"old\"\n# user\n",
        &config_hash(b"model=\"old\"\n# user\n"),
        &spec(),
    )
    .unwrap();
    let with_user = format!("{}\n[user_added]\nvalue = 1\n", applied.contents);
    let detached = detach_projection(&with_user, &applied.projection).unwrap();
    assert_eq!(
        detached.parse::<toml::Value>().unwrap()["model"].as_str(),
        Some("old")
    );
    assert!(detached.contains("[user_added]"));
    let drifted = applied.contents.replace("model-a", "changed");
    assert_eq!(
        detach_projection(&drifted, &applied.projection)
            .unwrap_err()
            .code,
        "CODEX_CONFIG_OWNERSHIP_CONFLICT"
    );
}

#[test]
fn launch_validation_allows_codex_additions_but_rejects_managed_drift() {
    let applied = apply_projection("", &config_hash(b""), &spec()).unwrap();
    let with_codex_state = format!(
        "{}\nmodel_reasoning_effort = \"none\"\n[projects.\"/tmp/repo\"]\ntrust_level = \"trusted\"\n[tui.model_availability_nux]\n\"gpt-5.5\" = 1\n",
        applied.contents
    );

    validate_managed_projection(
        &with_codex_state,
        &applied.projection.provider_id,
        &applied.projection.managed_values,
    )
    .unwrap();

    let drifted = with_codex_state.replace("https://proxy.example.test/v1", "https://evil.test");
    assert_eq!(
        validate_managed_projection(
            &drifted,
            &applied.projection.provider_id,
            &applied.projection.managed_values,
        )
        .unwrap_err()
        .code,
        "CODEX_CONFIG_OWNERSHIP_CONFLICT"
    );
}

#[test]
fn file_apply_creates_private_unique_backups_and_pre_rename_failure_preserves_source() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.toml");
    fs::write(&path, "model=\"old\"\n").unwrap();
    let before = fs::read(&path).unwrap();
    assert_eq!(
        apply_projection_file(
            &path,
            &config_hash(&before),
            &spec(),
            Some(ConfigWriteFault::BeforeRename)
        )
        .unwrap_err()
        .code,
        "CODEX_CONFIG_FAULT_INJECTED"
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    let first = apply_projection_file(&path, &config_hash(&before), &spec(), None).unwrap();
    assert!(first.backup_path.as_ref().unwrap().exists());
    let next = fs::read(&path).unwrap();
    let mut changed = spec();
    changed.model = "model-b".into();
    let second = apply_projection_file(&path, &config_hash(&next), &changed, None).unwrap();
    assert_ne!(first.backup_path, second.backup_path);
}
