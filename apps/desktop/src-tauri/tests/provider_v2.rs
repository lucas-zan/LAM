use localagentmanager_core::provider_credentials::{CredentialSource, UpstreamAuth};
use localagentmanager_core::provider_v2::{
    build_provider, join_upstream_endpoint, migrate_legacy_provider_array, AdapterConfig,
    CodexProviderOptions, ProviderCollection, ProviderInput, ProviderModel, ProviderProtocol,
    ProviderRepository,
};
use localagentmanager_core::storage::{InstallationLock, StoreOptions, VersionedFileStore};
use std::fs;
use std::time::Duration;

const NOW: &str = "2026-07-10T09:00:00Z";
const LATER: &str = "2026-07-10T10:00:00Z";

fn responses_input(id: &str) -> ProviderInput {
    ProviderInput {
        id: id.into(),
        name: "Example Responses".into(),
        protocol: ProviderProtocol::Responses,
        base_url: "https://provider.example.test/api/v1/".into(),
        default_model: "model-a".into(),
        models: vec![ProviderModel {
            id: "model-a".into(),
            label: "Model A".into(),
            capabilities: None,
        }],
        upstream_auth: UpstreamAuth::Bearer {
            source: CredentialSource::Env {
                env_key: "EXAMPLE_API_KEY".into(),
            },
        },
        adapter: AdapterConfig::None,
        compatibility_profile: None,
        codex: CodexProviderOptions::default(),
    }
}

fn repository(root: &tempfile::TempDir) -> ProviderRepository {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    }
    ProviderRepository::new(VersionedFileStore::<ProviderCollection>::new(
        root.path().join("providers.json"),
        InstallationLock::new(
            root.path().join("provider-hub.lock"),
            Duration::from_millis(250),
        ),
        2,
        StoreOptions::default(),
    ))
}

#[test]
fn provider_v2_repository_creates_updates_and_uses_revision_cas() {
    let root = tempfile::tempdir().unwrap();
    let repository = repository(&root);
    let created = repository
        .create(0, responses_input("company.proxy"), NOW)
        .unwrap();
    assert_eq!(created.revision, 1);
    let provider = &created.value.providers[0];
    assert_eq!(provider.id, "company.proxy");
    assert_eq!(provider.base_url, "https://provider.example.test/api/v1");
    assert_eq!(provider.created_at, NOW);
    assert_eq!(provider.updated_at, NOW);

    let mut update = responses_input("company.proxy");
    update.name = "Renamed".into();
    let updated = repository.update(1, update, LATER).unwrap();
    assert_eq!(updated.revision, 2);
    assert_eq!(updated.value.providers[0].created_at, NOW);
    assert_eq!(updated.value.providers[0].updated_at, LATER);

    let conflict = repository
        .create(1, responses_input("other"), LATER)
        .unwrap_err();
    assert_eq!(conflict.code, "STORE_REVISION_CONFLICT");
    let body = fs::read_to_string(root.path().join("providers.json")).unwrap();
    assert!(body.contains("\"schemaVersion\": 2"));
    assert!(!body.contains("wireApi"));
    assert!(!body.contains("health"));
}

#[test]
fn responses_provider_canonicalizes_legacy_gateway_flag_to_direct() {
    let mut input = responses_input("legacy-responses-gateway");
    input.codex.route_via_gateway = true;

    let provider = build_provider(input, NOW).unwrap();

    assert!(!provider.codex.route_via_gateway);
}

#[test]
fn provider_v2_accepts_typed_chat_adapter_and_rejects_invalid_states() {
    let mut chat = responses_input("chat-service");
    chat.protocol = ProviderProtocol::ChatCompletions;
    chat.adapter = AdapterConfig::Local {
        adapter_id: "responses_to_chat_completions".into(),
        upstream_path: "/chat/completions".into(),
    };
    chat.compatibility_profile = Some("deepseek_chat_completions".into());
    let built = build_provider(chat, NOW).unwrap();
    assert_eq!(built.protocol, ProviderProtocol::ChatCompletions);

    let mut invalid = Vec::new();
    let mut value = responses_input("");
    invalid.push((value, "PROVIDER_INVALID_ID"));
    value = responses_input("openai");
    invalid.push((value, "PROVIDER_RESERVED_ID"));
    value = responses_input("bad id");
    invalid.push((value, "PROVIDER_INVALID_ID"));
    value = responses_input("bad-url");
    value.base_url = "https://user:pass@example.test/v1".into();
    invalid.push((value, "PROVIDER_URL_USERINFO"));
    value = responses_input("insecure");
    value.base_url = "http://remote.example.test/v1".into();
    invalid.push((value, "PROVIDER_URL_INSECURE"));
    value = responses_input("missing-model");
    value.default_model = "absent".into();
    invalid.push((value, "PROVIDER_DEFAULT_MODEL_MISSING"));
    value = responses_input("duplicate-model");
    value.models.push(value.models[0].clone());
    invalid.push((value, "PROVIDER_MODEL_DUPLICATE"));
    value = responses_input("bad-adapter");
    value.protocol = ProviderProtocol::ChatCompletions;
    value.adapter = AdapterConfig::Local {
        adapter_id: "unknown".into(),
        upstream_path: "/chat/completions".into(),
    };
    invalid.push((value, "PROVIDER_ADAPTER_UNKNOWN"));
    value = responses_input("bad-path");
    value.protocol = ProviderProtocol::ChatCompletions;
    value.adapter = AdapterConfig::Local {
        adapter_id: "responses_to_chat_completions".into(),
        upstream_path: "/../admin?secret=x".into(),
    };
    invalid.push((value, "PROVIDER_ADAPTER_PATH_INVALID"));
    value = responses_input("bad-compat");
    value.compatibility_profile = Some("vendor-branch".into());
    invalid.push((value, "PROVIDER_COMPATIBILITY_UNKNOWN"));

    for (input, code) in invalid {
        assert_eq!(build_provider(input, NOW).unwrap_err().code, code);
    }
}

#[test]
fn legacy_provider_arrays_migrate_purely_and_deterministically() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/legacy-provider-contract/provider-store");
    for (file, id, warning) in [
        (
            "legacy-wire-openai.json",
            "legacy-chat-service",
            Some("LEGACY_WIRE_OPENAI_MAPPED_TO_RESPONSES"),
        ),
        (
            "legacy-wire-responses.json",
            "legacy-responses-service",
            None,
        ),
    ] {
        let path = root.join(file);
        let before = fs::read(&path).unwrap();
        let first = migrate_legacy_provider_array(&before, NOW).unwrap();
        let second = migrate_legacy_provider_array(&before, NOW).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.providers.providers[0].id, id);
        assert_eq!(
            first.providers.providers[0].protocol,
            ProviderProtocol::Responses
        );
        assert_eq!(first.providers.providers[0].models.len(), 1);
        assert!(matches!(
            first.providers.providers[0].upstream_auth,
            UpstreamAuth::Bearer { .. }
        ));
        assert_eq!(
            first.warnings.first().map(String::as_str),
            warning,
            "fixture {file}"
        );
        assert_eq!(fs::read(path).unwrap(), before, "migration must not write");
    }
}

#[test]
fn endpoint_join_preserves_prefix_and_rejects_path_injection() {
    assert_eq!(
        join_upstream_endpoint("https://example.test/api/v1/", "/chat/completions").unwrap(),
        "https://example.test/api/v1/chat/completions"
    );
    assert_eq!(
        join_upstream_endpoint("https://example.test", "/chat/completions").unwrap(),
        "https://example.test/chat/completions"
    );
    for path in [
        "https://evil.test/x",
        "//evil.test/x",
        "/../admin",
        "/x?q=1",
        "/x#f",
    ] {
        assert_eq!(
            join_upstream_endpoint("https://example.test/api", path)
                .unwrap_err()
                .code,
            "PROVIDER_ADAPTER_PATH_INVALID"
        );
    }
}

#[test]
fn provider_v2_debug_and_json_have_no_plaintext_secret_surface() {
    let marker = "LAM_TEST_SECRET_API_KEY_sk-rpg405-7f3a";
    let provider = build_provider(responses_input("safe"), NOW).unwrap();
    let debug = format!("{provider:?}");
    let json = serde_json::to_string(&provider).unwrap();
    assert!(!debug.contains(marker));
    assert!(!json.contains(marker));
    assert!(json.contains("EXAMPLE_API_KEY"));
    assert!(!json.contains("secretValue"));
}
