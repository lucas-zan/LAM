use localagentmanager_core::account::{list_accounts, repair_managed_wrappers};
use localagentmanager_core::provider_api_v2::{
    create_provider_service_v2, delete_api_account_service_v2,
    delete_api_account_service_v2_with_fault, delete_provider_service_v2,
    execute_api_account_model_switch_service_v2, execute_api_account_service_v2,
    execute_api_account_service_v2_with_fault, get_api_account_connection_service_v2,
    list_binding_views_service_v2, list_provider_hub_view_v2, list_provider_views_service_v2,
    migrate_native_responses_bindings_service_v2,
    migrate_native_responses_bindings_with_keychain_service_v2,
    plan_api_account_model_switch_service_v2, plan_api_account_service_v2,
    recover_api_account_transactions_service_v2, update_api_account_connection_service_v2,
    AdapterDto, ApiAccountFaultPoint, ApiAccountProviderSelectionV2, CodexOptionsDto,
    CreateProviderRequestV2, CredentialReferenceDto, DeleteProviderRequestV2,
    ExecuteApiAccountRequestV2, PlanApiAccountRequestV2, ProviderApiV2State, ProviderDefinitionDto,
    ProviderModelDto, ProviderProtocolDto, UpdateApiAccountConnectionRequestV2, UpstreamAuthDto,
};
use localagentmanager_core::provider_config_editor::config_hash;
use localagentmanager_core::provider_credentials::SecretValue;
use localagentmanager_core::provider_keychain::{KeychainBackend, KeychainCredentialReference};
use localagentmanager_core::{AppError, Result};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::sync::{Arc, Mutex};
use toml_edit::DocumentMut;

#[derive(Default)]
struct MigrationKeychain {
    value: Mutex<Option<String>>,
    reads: Mutex<usize>,
}

impl KeychainBackend for MigrationKeychain {
    fn write(&self, _reference: &KeychainCredentialReference, secret: &SecretValue) -> Result<()> {
        *self.value.lock().unwrap() = Some(secret.with_exposed(str::to_owned));
        Ok(())
    }

    fn read(&self, _reference: &KeychainCredentialReference) -> Result<SecretValue> {
        *self.reads.lock().unwrap() += 1;
        self.value
            .lock()
            .unwrap()
            .clone()
            .map(SecretValue::from_sensitive)
            .ok_or_else(|| AppError::new("KEYCHAIN_ITEM_NOT_FOUND", "missing test key"))
    }

    fn delete(&self, _reference: &KeychainCredentialReference) -> Result<()> {
        self.value.lock().unwrap().take();
        Ok(())
    }
}

fn provider() -> ProviderDefinitionDto {
    ProviderDefinitionDto {
        id: "account-work-api".into(),
        name: "Work API connection".into(),
        protocol: ProviderProtocolDto::Responses,
        base_url: "https://api.example.test/v1".into(),
        default_model: "model-a".into(),
        models: vec![
            ProviderModelDto {
                id: "model-a".into(),
                label: "Model A".into(),
            },
            ProviderModelDto {
                id: "model-b".into(),
                label: "Model B".into(),
            },
        ],
        upstream_auth: UpstreamAuthDto::None,
        adapter: AdapterDto::None,
        compatibility_profile: None,
        codex: CodexOptionsDto {
            display_name: Some("Work API".into()),
            stream_idle_timeout_ms: Some(300_000),
            direct_request_max_retries: Some(1),
            direct_stream_max_retries: Some(1),
            route_via_gateway: false,
            query_params: BTreeMap::new(),
            env_http_headers: BTreeMap::new(),
        },
    }
}

#[test]
fn deleting_a_v2_provider_requires_no_bindings_and_preserves_the_account() {
    let home = tempfile::tempdir().unwrap();
    let created = create_provider_service_v2(
        home.path(),
        CreateProviderRequestV2 {
            expected_revision: 0,
            provider: provider(),
        },
        "2026-08-26T00:00:00Z",
    )
    .unwrap();

    let deleted = delete_provider_service_v2(
        home.path(),
        DeleteProviderRequestV2 {
            expected_revision: created.store_revision,
            provider_id: created.id.clone(),
        },
    )
    .unwrap();

    assert_eq!(deleted.provider_id, "account-work-api");
    assert!(list_provider_views_service_v2(home.path())
        .unwrap()
        .is_empty());
    assert!(list_accounts(home.path()).unwrap().is_empty());
}

#[test]
fn deleting_a_bound_v2_provider_is_rejected_without_detaching_the_account() {
    let home = tempfile::tempdir().unwrap();
    let mut state = ProviderApiV2State::default();
    let plan = plan_api_account_service_v2(home.path(), request(), &mut state, 1_000).unwrap();
    execute_api_account_service_v2(
        home.path(),
        ExecuteApiAccountRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
            api_key: None,
        },
        &mut state,
        1_100,
    )
    .unwrap();
    let snapshot = list_provider_hub_view_v2(home.path()).unwrap();

    let error = delete_provider_service_v2(
        home.path(),
        DeleteProviderRequestV2 {
            expected_revision: snapshot.revision,
            provider_id: "account-work-api".into(),
        },
    )
    .unwrap_err();

    assert_eq!(error.code, "PROVIDER_IN_USE");
    assert_eq!(list_accounts(home.path()).unwrap().len(), 1);
    assert_eq!(
        list_provider_views_service_v2(home.path()).unwrap().len(),
        1
    );
    assert_eq!(list_binding_views_service_v2(home.path()).unwrap().len(), 1);
}

#[test]
fn deleting_a_v2_provider_rejects_a_stale_store_revision() {
    let home = tempfile::tempdir().unwrap();
    create_provider_service_v2(
        home.path(),
        CreateProviderRequestV2 {
            expected_revision: 0,
            provider: provider(),
        },
        "2026-08-26T00:00:00Z",
    )
    .unwrap();

    let error = delete_provider_service_v2(
        home.path(),
        DeleteProviderRequestV2 {
            expected_revision: 0,
            provider_id: "account-work-api".into(),
        },
    )
    .unwrap_err();

    assert_eq!(error.code, "STORE_REVISION_CONFLICT");
    assert_eq!(
        list_provider_views_service_v2(home.path()).unwrap().len(),
        1
    );
}

fn request() -> PlanApiAccountRequestV2 {
    PlanApiAccountRequestV2 {
        account_name: "work-api".into(),
        selected_model: "model-a".into(),
        overwrite_wrapper: false,
        provider: ApiAccountProviderSelectionV2::New {
            provider: Box::new(provider()),
        },
    }
}

fn deepseek_chat_request() -> PlanApiAccountRequestV2 {
    let mut provider = provider();
    provider.id = "account-deepseek-chat".into();
    provider.name = "DeepSeek Chat connection".into();
    provider.protocol = ProviderProtocolDto::ChatCompletions;
    provider.adapter = AdapterDto::Local {
        adapter_id: "responses_to_chat_completions".into(),
        upstream_path: "/chat/completions".into(),
    };
    provider.compatibility_profile = Some("deepseek_chat_completions".into());
    provider.codex.route_via_gateway = true;
    PlanApiAccountRequestV2 {
        account_name: "deepseek-chat".into(),
        selected_model: "model-a".into(),
        overwrite_wrapper: false,
        provider: ApiAccountProviderSelectionV2::New {
            provider: Box::new(provider),
        },
    }
}

#[test]
fn deepseek_chat_api_account_is_planned_through_gateway_and_declares_hosted_search_disabled() {
    let home = tempfile::tempdir().unwrap();
    let mut state = ProviderApiV2State::default();
    let plan = plan_api_account_service_v2(home.path(), deepseek_chat_request(), &mut state, 1_000)
        .unwrap();
    assert_eq!(
        plan.route_kind,
        localagentmanager_core::provider_api_v2::RouteKindDto::Gateway
    );
    assert!(plan.blockers.is_empty());

    let outcome = execute_api_account_service_v2(
        home.path(),
        ExecuteApiAccountRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
            api_key: None,
        },
        &mut state,
        1_100,
    )
    .unwrap();
    let config = fs::read_to_string(outcome.account.home_path.join("config.toml")).unwrap();
    let parsed = config.parse::<toml::Value>().unwrap();
    assert_eq!(parsed["web_search"].as_str(), Some("disabled"));
    assert_eq!(parsed["features"]["multi_agent"].as_bool(), Some(true));
}

fn native_key_request() -> PlanApiAccountRequestV2 {
    let mut request = request();
    let ApiAccountProviderSelectionV2::New { provider } = &mut request.provider else {
        unreachable!();
    };
    provider.upstream_auth = UpstreamAuthDto::Bearer {
        credential: CredentialReferenceDto::None,
    };
    request
}

fn create_native_account(
    home: &std::path::Path,
    state: &mut ProviderApiV2State,
) -> localagentmanager_core::ApiAccountExecutionViewV2 {
    let plan = plan_api_account_service_v2(home, native_key_request(), state, 1_000).unwrap();
    execute_api_account_service_v2(
        home,
        ExecuteApiAccountRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
            api_key: Some("sk-native-not-real".into()),
        },
        state,
        1_100,
    )
    .unwrap()
}

#[test]
fn responses_api_account_uses_native_codex_auth_file_without_keychain_helper() {
    let home = tempfile::tempdir().unwrap();
    let mut state = ProviderApiV2State::default();
    let plan =
        plan_api_account_service_v2(home.path(), native_key_request(), &mut state, 1_000).unwrap();
    let outcome = execute_api_account_service_v2(
        home.path(),
        ExecuteApiAccountRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
            api_key: Some("sk-native-not-real".into()),
        },
        &mut state,
        1_100,
    )
    .unwrap();

    let config = fs::read_to_string(outcome.account.home_path.join("config.toml")).unwrap();
    assert!(config.contains("requires_openai_auth = true"));
    assert!(config.contains("cli_auth_credentials_store = \"file\""));
    assert!(config.contains("model_catalog_json ="));
    assert!(config.contains("models.json"));
    assert!(!config.contains("lam-auth-helper"));
    assert!(!config.contains("keychain-token"));
    let auth: serde_json::Value =
        serde_json::from_slice(&fs::read(outcome.account.home_path.join("auth.json")).unwrap())
            .unwrap();
    assert_eq!(auth["auth_mode"], "apikey");
    assert_eq!(auth["OPENAI_API_KEY"], "sk-native-not-real");
    assert!(matches!(
        list_provider_views_service_v2(home.path()).unwrap()[0].upstream_auth,
        UpstreamAuthDto::Bearer {
            credential: CredentialReferenceDto::CodexProfile { .. }
        }
    ));
    let catalog: serde_json::Value =
        serde_json::from_slice(&fs::read(outcome.account.home_path.join("models.json")).unwrap())
            .unwrap();
    assert_eq!(catalog["models"].as_array().unwrap().len(), 2);
    assert_eq!(catalog["models"][0]["slug"], "model-a");
    assert_eq!(catalog["models"][1]["slug"], "model-b");
}

#[test]
fn legacy_keychain_responses_account_migrates_once_to_native_codex_auth() {
    let home = tempfile::tempdir().unwrap();
    let mut state = ProviderApiV2State::default();
    let plan =
        plan_api_account_service_v2(home.path(), native_key_request(), &mut state, 1_000).unwrap();
    let outcome = execute_api_account_service_v2(
        home.path(),
        ExecuteApiAccountRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
            api_key: Some("sk-native-not-real".into()),
        },
        &mut state,
        1_100,
    )
    .unwrap();
    fs::remove_file(outcome.account.home_path.join("auth.json")).unwrap();

    let hub = home
        .path()
        .join("Library/Application Support/dev.localagentmanager.desktop/provider-hub");
    let providers_path = hub.join("providers.json");
    let bindings_path = hub.join("bindings.json");
    let reference = KeychainCredentialReference::new("legacy-work-api", 1).unwrap();
    let mut providers: serde_json::Value =
        serde_json::from_slice(&fs::read(&providers_path).unwrap()).unwrap();
    providers["providers"][0]["upstreamAuth"]["source"] = serde_json::json!({
        "kind": "keychain",
        "service": reference.service,
        "account": reference.account,
        "version": reference.version
    });
    fs::write(
        &providers_path,
        serde_json::to_vec_pretty(&providers).unwrap(),
    )
    .unwrap();
    let config_path = outcome.account.home_path.join("config.toml");
    let config = fs::read_to_string(&config_path)
        .unwrap()
        .replace("cli_auth_credentials_store = \"file\"\n", "")
        .replace("requires_openai_auth = true", "");
    fs::write(&config_path, &config).unwrap();
    let mut bindings: serde_json::Value =
        serde_json::from_slice(&fs::read(&bindings_path).unwrap()).unwrap();
    bindings["bindings"][0]["configProjection"]["appliedHash"] =
        localagentmanager_core::provider_config_editor::config_hash(config.as_bytes()).into();
    bindings["bindings"][0]["configProjection"]["managedValues"]
        .as_object_mut()
        .unwrap()
        .remove("cli_auth_credentials_store");
    bindings["bindings"][0]["configProjection"]["managedValues"]
        .as_object_mut()
        .unwrap()
        .remove("requires_openai_auth");
    fs::write(
        &bindings_path,
        serde_json::to_vec_pretty(&bindings).unwrap(),
    )
    .unwrap();

    let backend = Arc::new(MigrationKeychain {
        value: Mutex::new(Some("sk-legacy-not-real".into())),
        reads: Mutex::new(0),
    });
    let first = migrate_native_responses_bindings_with_keychain_service_v2(
        home.path(),
        2_000,
        backend.clone(),
    )
    .unwrap();
    assert_eq!(first.migrated_profiles, ["work-api"]);
    assert_eq!(*backend.reads.lock().unwrap(), 1);
    assert!(fs::read_to_string(&config_path)
        .unwrap()
        .contains("requires_openai_auth = true"));
    let auth: serde_json::Value =
        serde_json::from_slice(&fs::read(outcome.account.home_path.join("auth.json")).unwrap())
            .unwrap();
    assert_eq!(auth["OPENAI_API_KEY"], "sk-legacy-not-real");

    let second =
        migrate_native_responses_bindings_with_keychain_service_v2(home.path(), 3_000, backend)
            .unwrap();
    assert!(second.migrated_profiles.is_empty());
}

#[test]
fn api_account_connection_detail_is_redacted_and_url_key_updates_are_projected() {
    let home = tempfile::tempdir().unwrap();
    let mut state = ProviderApiV2State::default();
    let created = create_native_account(home.path(), &mut state);
    let detail = get_api_account_connection_service_v2(home.path(), "work-api").unwrap();
    assert_eq!(detail.profile_id, "work-api");
    assert_eq!(detail.provider_id, "account-work-api");
    assert_eq!(detail.base_url, "https://api.example.test/v1");
    assert!(detail.api_key_configured);
    let serialized = serde_json::to_string(&detail).unwrap();
    assert!(!serialized.contains("sk-native-not-real"));

    let updated = update_api_account_connection_service_v2(
        home.path(),
        UpdateApiAccountConnectionRequestV2 {
            profile_id: "work-api".into(),
            expected_provider_store_revision: detail.provider_store_revision,
            base_url: "https://new.example.test/api/v1/".into(),
            api_key: Some("sk-replaced-not-real".into()),
            models: None,
            selected_model: None,
        },
        &mut state,
        2_000,
    )
    .unwrap();
    assert_eq!(updated.base_url, "https://new.example.test/api/v1");
    assert!(updated.api_key_configured);
    let config = fs::read_to_string(created.account.home_path.join("config.toml")).unwrap();
    assert!(config.contains("base_url = \"https://new.example.test/api/v1\""));
    assert!(config.contains("requires_openai_auth = true"));
    assert!(!config.contains("lam-auth-helper"));
    let auth: serde_json::Value =
        serde_json::from_slice(&fs::read(created.account.home_path.join("auth.json")).unwrap())
            .unwrap();
    assert_eq!(auth["OPENAI_API_KEY"], "sk-replaced-not-real");
}

#[test]
fn api_account_connection_update_preserves_key_and_rejects_stale_or_invalid_changes() {
    let home = tempfile::tempdir().unwrap();
    let mut state = ProviderApiV2State::default();
    let created = create_native_account(home.path(), &mut state);
    let detail = get_api_account_connection_service_v2(home.path(), "work-api").unwrap();

    let updated = update_api_account_connection_service_v2(
        home.path(),
        UpdateApiAccountConnectionRequestV2 {
            profile_id: "work-api".into(),
            expected_provider_store_revision: detail.provider_store_revision,
            base_url: "https://url-only.example.test/v1".into(),
            api_key: None,
            models: None,
            selected_model: None,
        },
        &mut state,
        2_000,
    )
    .unwrap();
    let auth_before_failures = fs::read(created.account.home_path.join("auth.json")).unwrap();
    assert!(String::from_utf8_lossy(&auth_before_failures).contains("sk-native-not-real"));

    for (request, code) in [
        (
            UpdateApiAccountConnectionRequestV2 {
                profile_id: "work-api".into(),
                expected_provider_store_revision: detail.provider_store_revision,
                base_url: "https://stale.example.test/v1".into(),
                api_key: None,
                models: None,
                selected_model: None,
            },
            "STORE_REVISION_CONFLICT",
        ),
        (
            UpdateApiAccountConnectionRequestV2 {
                profile_id: "work-api".into(),
                expected_provider_store_revision: updated.provider_store_revision,
                base_url: "http://insecure.example.test/v1".into(),
                api_key: None,
                models: None,
                selected_model: None,
            },
            "PROVIDER_URL_INSECURE",
        ),
        (
            UpdateApiAccountConnectionRequestV2 {
                profile_id: "work-api".into(),
                expected_provider_store_revision: updated.provider_store_revision,
                base_url: updated.base_url.clone(),
                api_key: Some("   ".into()),
                models: None,
                selected_model: None,
            },
            "CODEX_API_KEY_EMPTY",
        ),
    ] {
        assert_eq!(
            update_api_account_connection_service_v2(home.path(), request, &mut state, 3_000,)
                .unwrap_err()
                .code,
            code
        );
    }
    assert_eq!(
        fs::read(created.account.home_path.join("auth.json")).unwrap(),
        auth_before_failures
    );
    assert_eq!(
        get_api_account_connection_service_v2(home.path(), "work-api")
            .unwrap()
            .base_url,
        "https://url-only.example.test/v1"
    );
}

#[test]
fn api_account_connection_can_replace_saved_models_and_rewrite_catalog() {
    let home = tempfile::tempdir().unwrap();
    let mut state = ProviderApiV2State::default();
    let created = create_native_account(home.path(), &mut state);
    let detail = get_api_account_connection_service_v2(home.path(), "work-api").unwrap();
    assert_eq!(
        detail
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        ["model-a", "model-b"]
    );

    let missing_selected = update_api_account_connection_service_v2(
        home.path(),
        UpdateApiAccountConnectionRequestV2 {
            profile_id: "work-api".into(),
            expected_provider_store_revision: detail.provider_store_revision,
            base_url: detail.base_url.clone(),
            api_key: None,
            models: Some(vec![
                ProviderModelDto {
                    id: "model-c".into(),
                    label: "Model C".into(),
                },
                ProviderModelDto {
                    id: "model-d".into(),
                    label: "Model D".into(),
                },
            ]),
            selected_model: None,
        },
        &mut state,
        2_500,
    )
    .unwrap_err();
    assert_eq!(missing_selected.code, "API_ACCOUNT_SELECTED_MODEL_REQUIRED");

    let updated = update_api_account_connection_service_v2(
        home.path(),
        UpdateApiAccountConnectionRequestV2 {
            profile_id: "work-api".into(),
            expected_provider_store_revision: detail.provider_store_revision,
            base_url: detail.base_url.clone(),
            api_key: None,
            models: Some(vec![
                ProviderModelDto {
                    id: "model-c".into(),
                    label: "Model C".into(),
                },
                ProviderModelDto {
                    id: "model-d".into(),
                    label: "Model D".into(),
                },
            ]),
            selected_model: Some("model-d".into()),
        },
        &mut state,
        2_600,
    )
    .unwrap();
    assert_eq!(
        updated
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        ["model-c", "model-d"]
    );
    assert_eq!(updated.selected_model, "model-d");

    let providers = list_provider_views_service_v2(home.path()).unwrap();
    assert_eq!(providers[0].default_model, "model-d");
    assert_eq!(
        providers[0]
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        ["model-c", "model-d"]
    );
    let bindings = list_binding_views_service_v2(home.path()).unwrap();
    assert_eq!(bindings[0].selected_model, "model-d");

    let catalog: serde_json::Value =
        serde_json::from_slice(&fs::read(created.account.home_path.join("models.json")).unwrap())
            .unwrap();
    let slugs = catalog["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|model| model["slug"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(slugs, ["model-c", "model-d"]);
}

#[test]
fn add_api_account_creates_exactly_one_profile_home_provider_and_binding() {
    let home = tempfile::tempdir().unwrap();
    let mut state = ProviderApiV2State::default();
    let plan = plan_api_account_service_v2(home.path(), request(), &mut state, 1_000).unwrap();
    assert_eq!(plan.account_name, "work-api");
    assert_eq!(plan.provider_id, "account-work-api");
    assert_eq!(plan.selected_model, "model-a");
    assert_eq!(
        plan.route_kind,
        localagentmanager_core::provider_api_v2::RouteKindDto::Direct
    );
    assert!(plan.blockers.is_empty());

    let outcome = execute_api_account_service_v2(
        home.path(),
        ExecuteApiAccountRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
            api_key: None,
        },
        &mut state,
        1_100,
    )
    .unwrap();
    assert_eq!(outcome.account.profile_id, "work-api");
    assert_eq!(outcome.binding.profile_id, "work-api");
    assert_eq!(outcome.binding.provider_id, "account-work-api");

    let accounts = list_accounts(home.path()).unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, "work-api");
    assert_eq!(accounts[0].codex_home, outcome.account.home_path);
    let wrapper = fs::read_to_string(&outcome.account.wrapper_path).unwrap();
    assert!(wrapper.contains("exec \"$CODEX_BIN\" \"$@\""));
    assert!(!wrapper.contains("lam' codex --profile"));
    assert!(repair_managed_wrappers(home.path()).unwrap().is_empty());
    let providers = list_provider_views_service_v2(home.path()).unwrap();
    assert_eq!(providers.len(), 1);
    assert!(!providers[0].codex.route_via_gateway);
    assert_eq!(list_binding_views_service_v2(home.path()).unwrap().len(), 1);
    let config = fs::read_to_string(outcome.account.home_path.join("config.toml")).unwrap();
    assert!(config.contains("model = \"model-a\""));
    assert!(config.contains("model_provider = \"account-work-api\""));
    assert!(config.contains("base_url = \"https://api.example.test/v1\""));
    assert!(!config.contains("127.0.0.1"));
}

#[test]
fn startup_migrates_legacy_responses_gateway_binding_and_wrapper_idempotently() {
    let home = tempfile::tempdir().unwrap();
    let mut state = ProviderApiV2State::default();
    let plan = plan_api_account_service_v2(home.path(), request(), &mut state, 1_000).unwrap();
    let outcome = execute_api_account_service_v2(
        home.path(),
        ExecuteApiAccountRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
            api_key: None,
        },
        &mut state,
        1_100,
    )
    .unwrap();
    let hub = home
        .path()
        .join("Library/Application Support/dev.localagentmanager.desktop/provider-hub");
    let providers_path = hub.join("providers.json");
    let bindings_path = hub.join("bindings.json");
    let mut providers: serde_json::Value =
        serde_json::from_slice(&fs::read(&providers_path).unwrap()).unwrap();
    providers["providers"][0]["codex"]["routeViaGateway"] = true.into();
    fs::write(
        &providers_path,
        serde_json::to_vec_pretty(&providers).unwrap(),
    )
    .unwrap();
    let mut bindings: serde_json::Value =
        serde_json::from_slice(&fs::read(&bindings_path).unwrap()).unwrap();
    bindings["bindings"][0]["routeKind"] = "gateway".into();
    fs::write(
        &bindings_path,
        serde_json::to_vec_pretty(&bindings).unwrap(),
    )
    .unwrap();
    fs::write(
        &outcome.account.wrapper_path,
        "#!/usr/bin/env bash\nexec '/tmp/lam' codex --profile 'work-api' -- \"$@\"\n",
    )
    .unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(outcome.account.home_path.join("config.toml"))
        .unwrap()
        .write_all(b"\n[tui.model_availability_nux]\n\"model-z\" = 1\n")
        .unwrap();

    let migrated = migrate_native_responses_bindings_service_v2(home.path(), 2_000).unwrap();

    assert_eq!(migrated.migrated_profiles, ["work-api"]);
    assert_eq!(
        list_binding_views_service_v2(home.path()).unwrap()[0].route_kind,
        localagentmanager_core::provider_api_v2::RouteKindDto::Direct
    );
    assert!(
        !list_provider_views_service_v2(home.path()).unwrap()[0]
            .codex
            .route_via_gateway
    );
    let wrapper = fs::read_to_string(&outcome.account.wrapper_path).unwrap();
    assert!(wrapper.contains("exec \"$CODEX_BIN\" \"$@\""));
    assert!(!wrapper.contains("lam' codex --profile"));
    assert!(
        fs::read_to_string(outcome.account.home_path.join("config.toml"))
            .unwrap()
            .contains("base_url = \"https://api.example.test/v1\"")
    );
    assert!(
        fs::read_to_string(outcome.account.home_path.join("config.toml"))
            .unwrap()
            .contains("model_catalog_json =")
    );
    assert!(
        migrate_native_responses_bindings_service_v2(home.path(), 3_000)
            .unwrap()
            .migrated_profiles
            .is_empty()
    );
}

#[test]
fn startup_adds_the_official_model_catalog_to_an_existing_direct_account() {
    let home = tempfile::tempdir().unwrap();
    let mut state = ProviderApiV2State::default();
    let created = create_native_account(home.path(), &mut state);
    let config_path = created.account.home_path.join("config.toml");
    let mut config = fs::read_to_string(&config_path)
        .unwrap()
        .parse::<DocumentMut>()
        .unwrap();
    config.as_table_mut().remove("model_catalog_json");
    fs::write(&config_path, config.to_string()).unwrap();
    fs::remove_file(created.account.home_path.join("models.json")).unwrap();

    let bindings_path = home.path().join(
        "Library/Application Support/dev.localagentmanager.desktop/provider-hub/bindings.json",
    );
    let mut bindings: serde_json::Value =
        serde_json::from_slice(&fs::read(&bindings_path).unwrap()).unwrap();
    let projection = &mut bindings["bindings"][0]["configProjection"];
    projection["managedValues"]
        .as_object_mut()
        .unwrap()
        .remove("model_catalog_json");
    projection["previousValues"]
        .as_object_mut()
        .unwrap()
        .remove("model_catalog_json");
    projection["appliedHash"] = config_hash(config.to_string().as_bytes()).into();
    fs::write(
        &bindings_path,
        serde_json::to_vec_pretty(&bindings).unwrap(),
    )
    .unwrap();

    let migrated = migrate_native_responses_bindings_service_v2(home.path(), 2_000).unwrap();
    assert_eq!(migrated.migrated_profiles, ["work-api"]);
    let updated = fs::read_to_string(config_path).unwrap();
    assert!(updated.contains("model_catalog_json ="));
    assert!(created.account.home_path.join("models.json").exists());
    assert!(
        migrate_native_responses_bindings_service_v2(home.path(), 3_000)
            .unwrap()
            .migrated_profiles
            .is_empty()
    );
}

#[test]
fn api_account_inherits_only_safe_preferences_from_normal_codex_config() {
    let home = tempfile::tempdir().unwrap();
    let normal_codex_home = home.path().join(".codex");
    fs::create_dir_all(&normal_codex_home).unwrap();
    fs::write(
        normal_codex_home.join("config.toml"),
        r#"
model = "normal-account-model"
model_provider = "normal-account-provider"
model_reasoning_effort = "high"
personality = "friendly"
service_tier = "default"
approvals_reviewer = "user"
notify = ["sh", "-c", "secret-notification-command"]

[desktop]
enabled-reasoning-efforts = ["low", "high", "max"]
open-link-in-target-preference = "private-machine-state"

[features]
multi_agent = false
js_repl = true
unknown_private_feature = "do-not-copy"

[model_providers.normal-account-provider]
base_url = "https://secret-provider.example/v1"
experimental_bearer_token = "do-not-copy"

[projects."/private/user/project"]
trust_level = "trusted"

[mcp_servers.private]
url = "https://private-mcp.example"

[mcp_servers.private.http_headers]
Authorization = "do-not-copy"
"#,
    )
    .unwrap();

    let mut state = ProviderApiV2State::default();
    let plan = plan_api_account_service_v2(home.path(), request(), &mut state, 1_000).unwrap();
    let outcome = execute_api_account_service_v2(
        home.path(),
        ExecuteApiAccountRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
            api_key: None,
        },
        &mut state,
        1_100,
    )
    .unwrap();

    let config = fs::read_to_string(outcome.account.home_path.join("config.toml")).unwrap();
    let parsed = config.parse::<toml::Value>().unwrap();
    assert_eq!(parsed["model_reasoning_effort"].as_str(), Some("high"));
    assert_eq!(parsed["personality"].as_str(), Some("friendly"));
    assert_eq!(parsed["features"]["multi_agent"].as_bool(), Some(false));
    assert_eq!(parsed["features"]["js_repl"].as_bool(), Some(true));
    assert_eq!(
        parsed["desktop"]["enabled-reasoning-efforts"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(toml::Value::as_str)
            .collect::<Vec<_>>(),
        ["low", "high", "max"]
    );
    assert_eq!(parsed["model"].as_str(), Some("model-a"));
    assert_eq!(parsed["model_provider"].as_str(), Some("account-work-api"));
    assert!(!config.contains("secret-notification-command"));
    assert!(!config.contains("secret-provider.example"));
    assert!(!config.contains("do-not-copy"));
    assert!(!config.contains("/private/user/project"));
    assert!(!config.contains("private-mcp.example"));
    assert!(!config.contains("private-machine-state"));
}

#[test]
fn api_account_uses_builtin_codex_template_when_normal_config_is_missing() {
    let home = tempfile::tempdir().unwrap();
    let mut state = ProviderApiV2State::default();
    let plan = plan_api_account_service_v2(home.path(), request(), &mut state, 1_000).unwrap();
    let outcome = execute_api_account_service_v2(
        home.path(),
        ExecuteApiAccountRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
            api_key: None,
        },
        &mut state,
        1_100,
    )
    .unwrap();

    let config = fs::read_to_string(outcome.account.home_path.join("config.toml")).unwrap();
    let parsed = config.parse::<toml::Value>().unwrap();
    assert_eq!(parsed["model_reasoning_effort"].as_str(), Some("medium"));
    assert_eq!(parsed["personality"].as_str(), Some("pragmatic"));
    assert_eq!(
        parsed["desktop"]["enabled-reasoning-efforts"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(toml::Value::as_str)
            .collect::<Vec<_>>(),
        ["low", "medium", "high", "xhigh", "ultra", "max"]
    );
    assert_eq!(parsed["features"]["multi_agent"].as_bool(), Some(true));
    assert_eq!(parsed["features"]["js_repl"].as_bool(), Some(false));
    assert_eq!(parsed["model"].as_str(), Some("model-a"));
}

#[test]
fn api_account_falls_back_to_builtin_template_for_invalid_normal_config() {
    let home = tempfile::tempdir().unwrap();
    let normal_codex_home = home.path().join(".codex");
    fs::create_dir_all(&normal_codex_home).unwrap();
    fs::write(normal_codex_home.join("config.toml"), "not = [valid").unwrap();

    let mut state = ProviderApiV2State::default();
    let plan = plan_api_account_service_v2(home.path(), request(), &mut state, 1_000).unwrap();
    let outcome = execute_api_account_service_v2(
        home.path(),
        ExecuteApiAccountRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
            api_key: None,
        },
        &mut state,
        1_100,
    )
    .unwrap();

    let config = fs::read_to_string(outcome.account.home_path.join("config.toml")).unwrap();
    let parsed = config.parse::<toml::Value>().unwrap();
    assert_eq!(parsed["model_reasoning_effort"].as_str(), Some("medium"));
    assert_eq!(parsed["personality"].as_str(), Some("pragmatic"));
    assert!(!config.contains("not = [valid"));
}

#[test]
fn embedded_codex_templates_are_versioned_and_contain_no_account_or_machine_state() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources");
    let config = fs::read_to_string(root.join("codex-config-template.toml")).unwrap();
    config.parse::<toml::Value>().unwrap();
    let catalog = fs::read_to_string(root.join("codex-model-catalog.json")).unwrap();
    let catalog_json: serde_json::Value = serde_json::from_str(&catalog).unwrap();
    assert_eq!(catalog_json["schema_version"], 1);
    assert!(catalog_json["models"]
        .as_array()
        .is_some_and(|models| !models.is_empty()));

    let templates = format!("{config}\n{catalog}");
    for forbidden in [
        "auth.json",
        "Authorization",
        "api_key",
        "history.jsonl",
        "session_index",
        "installation_id",
        "/Users/",
        "/home/",
        "mcp_servers",
        "model_providers",
    ] {
        assert!(
            !templates.contains(forbidden),
            "template contains {forbidden}"
        );
    }
}

#[test]
fn switching_model_rebinds_the_same_profile_and_codex_home() {
    let home = tempfile::tempdir().unwrap();
    let mut state = ProviderApiV2State::default();
    let create_plan =
        plan_api_account_service_v2(home.path(), request(), &mut state, 1_000).unwrap();
    let created = execute_api_account_service_v2(
        home.path(),
        ExecuteApiAccountRequestV2 {
            plan_id: create_plan.plan_id,
            fingerprint: create_plan.fingerprint,
            api_key: None,
        },
        &mut state,
        1_100,
    )
    .unwrap();
    let original_home = created.account.home_path.clone();

    let switch = plan_api_account_model_switch_service_v2(
        home.path(),
        "work-api",
        "model-b",
        &mut state,
        2_000,
    )
    .unwrap();
    execute_api_account_model_switch_service_v2(
        home.path(),
        &switch.plan_id,
        &switch.fingerprint,
        &mut state,
        2_100,
    )
    .unwrap();

    let accounts = list_accounts(home.path()).unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].codex_home, original_home);
    let bindings = list_binding_views_service_v2(home.path()).unwrap();
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].selected_model, "model-b");
    let config = fs::read_to_string(original_home.join("config.toml")).unwrap();
    assert!(config.contains("model = \"model-b\""));
}

#[test]
fn create_fault_rolls_back_account_provider_and_secret_free_plan() {
    let home = tempfile::tempdir().unwrap();
    let mut state = ProviderApiV2State::default();
    let plan = plan_api_account_service_v2(home.path(), request(), &mut state, 1_000).unwrap();
    assert!(!serde_json::to_string(&plan).unwrap().contains("secret"));
    let error = execute_api_account_service_v2_with_fault(
        home.path(),
        ExecuteApiAccountRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
            api_key: None,
        },
        &mut state,
        1_100,
        Some(ApiAccountFaultPoint::AfterProviderCreated),
    )
    .unwrap_err();
    assert_eq!(error.code, "API_ACCOUNT_FAULT_INJECTED");
    assert!(list_accounts(home.path()).unwrap().is_empty());
    assert!(list_provider_views_service_v2(home.path())
        .unwrap()
        .is_empty());
    assert!(list_binding_views_service_v2(home.path())
        .unwrap()
        .is_empty());
}

#[test]
fn startup_recovery_compensates_a_crash_after_provider_creation_idempotently() {
    let home = tempfile::tempdir().unwrap();
    let mut state = ProviderApiV2State::default();
    let plan = plan_api_account_service_v2(home.path(), request(), &mut state, 1_000).unwrap();
    let error = execute_api_account_service_v2_with_fault(
        home.path(),
        ExecuteApiAccountRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
            api_key: None,
        },
        &mut state,
        1_100,
        Some(ApiAccountFaultPoint::CrashAfterProviderCreated),
    )
    .unwrap_err();
    assert_eq!(error.code, "API_ACCOUNT_CRASH_SIMULATED");
    assert_eq!(list_accounts(home.path()).unwrap().len(), 1);
    assert_eq!(
        list_provider_views_service_v2(home.path()).unwrap().len(),
        1
    );

    let first = recover_api_account_transactions_service_v2(home.path()).unwrap();
    assert_eq!(first.recovered, 1);
    assert!(list_accounts(home.path()).unwrap().is_empty());
    assert!(list_provider_views_service_v2(home.path())
        .unwrap()
        .is_empty());
    let second = recover_api_account_transactions_service_v2(home.path()).unwrap();
    assert_eq!(second.recovered, 0);
}

#[test]
fn startup_recovery_reattaches_an_account_after_delete_crashes_post_detach() {
    let home = tempfile::tempdir().unwrap();
    let mut state = ProviderApiV2State::default();
    let plan = plan_api_account_service_v2(home.path(), request(), &mut state, 1_000).unwrap();
    execute_api_account_service_v2(
        home.path(),
        ExecuteApiAccountRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
            api_key: None,
        },
        &mut state,
        1_100,
    )
    .unwrap();

    let error = delete_api_account_service_v2_with_fault(
        home.path(),
        "work-api",
        &mut state,
        2_000,
        Some(ApiAccountFaultPoint::CrashAfterDeleteDetach),
    )
    .unwrap_err();
    assert_eq!(error.code, "API_ACCOUNT_DELETE_CRASH_SIMULATED");
    assert_eq!(list_accounts(home.path()).unwrap().len(), 1);
    assert!(list_binding_views_service_v2(home.path())
        .unwrap()
        .is_empty());

    let report = recover_api_account_transactions_service_v2(home.path()).unwrap();
    assert_eq!(report.recovered, 1);
    let bindings = list_binding_views_service_v2(home.path()).unwrap();
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].selected_model, "model-a");
    assert_eq!(
        recover_api_account_transactions_service_v2(home.path())
            .unwrap()
            .recovered,
        0
    );
}

#[test]
fn startup_recovery_preserves_a_runtime_recreated_partially_deleted_account() {
    let home = tempfile::tempdir().unwrap();
    let mut state = ProviderApiV2State::default();
    let plan = plan_api_account_service_v2(home.path(), request(), &mut state, 1_000).unwrap();
    let created = execute_api_account_service_v2(
        home.path(),
        ExecuteApiAccountRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
            api_key: None,
        },
        &mut state,
        1_100,
    )
    .unwrap();

    let error = delete_api_account_service_v2_with_fault(
        home.path(),
        "work-api",
        &mut state,
        2_000,
        Some(ApiAccountFaultPoint::CrashAfterDeleteDetach),
    )
    .unwrap_err();
    assert_eq!(error.code, "API_ACCOUNT_DELETE_CRASH_SIMULATED");

    fs::remove_dir_all(&created.account.home_path).unwrap();
    fs::create_dir_all(&created.account.home_path).unwrap();
    fs::write(
        created.account.home_path.join("history.jsonl"),
        "runtime recreated after delete\n",
    )
    .unwrap();

    let report = recover_api_account_transactions_service_v2(home.path()).unwrap();
    assert_eq!(report.recovered, 1);
    assert!(created.account.home_path.join("history.jsonl").exists());
    assert!(list_binding_views_service_v2(home.path())
        .unwrap()
        .is_empty());
    assert!(list_provider_views_service_v2(home.path())
        .unwrap()
        .is_empty());
    assert_eq!(
        recover_api_account_transactions_service_v2(home.path())
            .unwrap()
            .recovered,
        0
    );
}

#[test]
fn delete_api_account_detaches_and_removes_its_exclusive_provider() {
    let home = tempfile::tempdir().unwrap();
    let mut state = ProviderApiV2State::default();
    let plan = plan_api_account_service_v2(home.path(), request(), &mut state, 1_000).unwrap();
    execute_api_account_service_v2(
        home.path(),
        ExecuteApiAccountRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
            api_key: None,
        },
        &mut state,
        1_100,
    )
    .unwrap();

    let deleted =
        delete_api_account_service_v2(home.path(), "work-api", &mut state, 2_000).unwrap();
    assert_eq!(deleted.profile_id, "work-api");
    assert!(list_accounts(home.path()).unwrap().is_empty());
    assert!(list_binding_views_service_v2(home.path())
        .unwrap()
        .is_empty());
    assert!(list_provider_views_service_v2(home.path())
        .unwrap()
        .is_empty());

    let empty_snapshot = list_provider_hub_view_v2(home.path()).unwrap();
    assert!(empty_snapshot.providers.is_empty());
    assert!(empty_snapshot.revision > 0);

    let create = CreateProviderRequestV2 {
        expected_revision: empty_snapshot.revision,
        provider: provider(),
    };
    let recreated =
        create_provider_service_v2(home.path(), create, "2026-07-16T00:00:00Z").unwrap();
    assert_eq!(recreated.store_revision, empty_snapshot.revision + 1);
}

#[test]
fn existing_provider_is_reused_only_when_explicitly_selected() {
    let selection = ApiAccountProviderSelectionV2::Existing {
        provider_id: "shared-provider".into(),
    };
    let json = serde_json::to_value(selection).unwrap();
    assert_eq!(json["kind"], "existing");
    assert_eq!(json["providerId"], "shared-provider");
    assert!(!json.to_string().contains("credential"));
    let empty = CredentialReferenceDto::None;
    assert_eq!(serde_json::to_value(empty).unwrap()["kind"], "none");
}
