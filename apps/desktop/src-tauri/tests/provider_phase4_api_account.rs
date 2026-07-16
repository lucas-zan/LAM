use localagentmanager_core::account::list_accounts;
use localagentmanager_core::provider_api_v2::{
    delete_api_account_service_v2, delete_api_account_service_v2_with_fault,
    execute_api_account_model_switch_service_v2, execute_api_account_service_v2,
    execute_api_account_service_v2_with_fault, list_binding_views_service_v2,
    list_provider_views_service_v2, plan_api_account_model_switch_service_v2,
    plan_api_account_service_v2, recover_api_account_transactions_service_v2, AdapterDto,
    ApiAccountFaultPoint, ApiAccountProviderSelectionV2, CodexOptionsDto, CredentialReferenceDto,
    ExecuteApiAccountRequestV2, PlanApiAccountRequestV2, ProviderApiV2State, ProviderDefinitionDto,
    ProviderModelDto, ProviderProtocolDto, UpstreamAuthDto,
};
use std::collections::BTreeMap;
use std::fs;

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
        localagentmanager_core::provider_api_v2::RouteKindDto::Gateway
    );
    assert!(plan.blockers.is_empty());

    let outcome = execute_api_account_service_v2(
        home.path(),
        ExecuteApiAccountRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
            keychain_secret: None,
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
    let providers = list_provider_views_service_v2(home.path()).unwrap();
    assert_eq!(providers.len(), 1);
    assert!(providers[0].codex.route_via_gateway);
    assert_eq!(list_binding_views_service_v2(home.path()).unwrap().len(), 1);
    let config = fs::read_to_string(outcome.account.home_path.join("config.toml")).unwrap();
    assert!(config.contains("model = \"model-a\""));
    assert!(config.contains("model_provider = \"account-work-api\""));
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
            keychain_secret: None,
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
            keychain_secret: None,
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
            keychain_secret: None,
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
            keychain_secret: None,
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
fn delete_api_account_detaches_and_removes_its_exclusive_provider() {
    let home = tempfile::tempdir().unwrap();
    let mut state = ProviderApiV2State::default();
    let plan = plan_api_account_service_v2(home.path(), request(), &mut state, 1_000).unwrap();
    execute_api_account_service_v2(
        home.path(),
        ExecuteApiAccountRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
            keychain_secret: None,
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
