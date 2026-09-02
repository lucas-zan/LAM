use localagentmanager_core::provider_api_v2::{
    execute_api_account_service_v2, plan_api_account_service_v2, AdapterDto,
    ApiAccountProviderSelectionV2, CodexOptionsDto, ExecuteApiAccountRequestV2,
    PlanApiAccountRequestV2, ProviderApiV2State, ProviderDefinitionDto, ProviderModelDto,
    ProviderProtocolDto, UpstreamAuthDto,
};
use localagentmanager_core::{relay_resume_session, RelayResumeRequest};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn add_api_account(home: &Path, name: &str, provider_suffix: &str) {
    let mut state = ProviderApiV2State::default();
    let provider_id = format!("account-{name}");
    let plan = plan_api_account_service_v2(
        home,
        PlanApiAccountRequestV2 {
            account_name: name.into(),
            selected_model: format!("model-{provider_suffix}"),
            overwrite_wrapper: false,
            provider: ApiAccountProviderSelectionV2::New {
                provider: Box::new(ProviderDefinitionDto {
                    id: provider_id,
                    name: format!("{name} connection"),
                    protocol: ProviderProtocolDto::Responses,
                    base_url: format!("https://{provider_suffix}.example.test/v1"),
                    default_model: format!("model-{provider_suffix}"),
                    models: vec![ProviderModelDto {
                        id: format!("model-{provider_suffix}"),
                        label: format!("Model {provider_suffix}"),
                        context_window: None,
                    }],
                    upstream_auth: UpstreamAuthDto::None,
                    adapter: AdapterDto::None,
                    compatibility_profile: None,
                    codex: CodexOptionsDto {
                        display_name: None,
                        stream_idle_timeout_ms: None,
                        direct_request_max_retries: Some(1),
                        direct_stream_max_retries: Some(1),
                        route_via_gateway: false,
                        query_params: BTreeMap::new(),
                        env_http_headers: BTreeMap::new(),
                        reasoning_effort: None,
                    },
                }),
            },
        },
        &mut state,
        1_000,
    )
    .unwrap();
    execute_api_account_service_v2(
        home,
        ExecuteApiAccountRequestV2 {
            plan_id: plan.plan_id,
            fingerprint: plan.fingerprint,
            api_key: None,
        },
        &mut state,
        1_100,
    )
    .unwrap();
}

fn write_source_session(home: &Path, body: &str) {
    let path = home.join(".codex-source/sessions/2026/07/14/relay.jsonl");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

fn request() -> RelayResumeRequest {
    RelayResumeRequest {
        from_profile_id: "source".into(),
        to_profile_id: "target".into(),
        session_id: "relay-sid".into(),
        cwd: None,
        diverged_strategy: None,
        confirm_compatibility_loss: false,
    }
}

#[test]
fn provider_mismatch_with_encrypted_reasoning_is_blocked_before_target_write() {
    let home = tempfile::tempdir().unwrap();
    add_api_account(home.path(), "source", "source");
    add_api_account(home.path(), "target", "target");
    write_source_session(
        home.path(),
        concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"relay-sid\",\"cwd\":\"/tmp\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\",\"encrypted_content\":\"opaque\"}}\n"
        ),
    );
    let target = home
        .path()
        .join(".codex-target/sessions/2026/07/14/relay.jsonl");
    let error = relay_resume_session(home.path(), &request()).unwrap_err();
    assert_eq!(error.code, "RELAY_COMPATIBILITY_BLOCKED");
    assert!(!target.exists());
    assert!(!target.parent().unwrap().exists());
}

#[test]
fn provider_mismatch_text_history_copies_and_resumes_through_launcher() {
    let home = tempfile::tempdir().unwrap();
    add_api_account(home.path(), "source", "source");
    add_api_account(home.path(), "target", "target");
    write_source_session(
        home.path(),
        concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"relay-sid\",\"cwd\":\"/tmp\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"hello\"}]}}\n"
        ),
    );
    let result = relay_resume_session(home.path(), &request()).unwrap();
    assert_eq!(result.action, "copied");
    assert_eq!(
        result.compatibility.unwrap().disposition.to_string(),
        "compatible"
    );
    assert!(result.compatibility_fingerprint.is_some());
    assert!(
        result.resume.command.contains("CODEX_HOME="),
        "unexpected direct resume command: {}",
        result.resume.command
    );
    assert!(result.resume.command.contains(" resume relay-sid"));
}
