use localagentmanager_core::provider_binding::RouteKind;
use localagentmanager_core::provider_credentials::{
    CredentialSource, DirectCodexAuth, UpstreamAuth,
};
use localagentmanager_core::provider_planner::*;
use localagentmanager_core::provider_v2::*;
use std::collections::BTreeMap;

fn provider(protocol: ProviderProtocol) -> ProviderProfileV2 {
    ProviderProfileV2 {
        id: "remote-provider".into(),
        name: "Remote Provider".into(),
        protocol,
        base_url: "https://api.example.test/v1".into(),
        default_model: "model-a".into(),
        models: vec![ProviderModel {
            id: "model-a".into(),
            label: "A".into(),
            capabilities: None,
        }],
        upstream_auth: UpstreamAuth::Bearer {
            source: CredentialSource::Env {
                env_key: "REMOTE_TOKEN".into(),
            },
        },
        capabilities: CapabilityDeclaration::default(),
        adapter: if protocol == ProviderProtocol::ChatCompletions {
            AdapterConfig::Local {
                adapter_id: "responses_to_chat_completions".into(),
                upstream_path: "chat/completions".into(),
            }
        } else {
            AdapterConfig::None
        },
        compatibility_profile: None,
        codex: CodexProviderOptions::default(),
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    }
}

fn context() -> AttachPlanContext {
    AttachPlanContext {
        profile_id: "profile-a".into(),
        config_path: "/profiles/profile-a/config.toml".into(),
        provider_store_revision: 7,
        expected_binding_revision: Some(3),
        binding_store_revision: 5,
        source_config_hash: "config-hash".into(),
        binding_drifted: false,
        credential_ready: true,
        gateway: GatewayPlanContext {
            base_url: "http://127.0.0.1:43123/v1".into(),
            available: true,
            endpoint_version: 9,
        },
        planner_options: BTreeMap::from([("contract".into(), "v1".into())]),
        auth_helper_path: "/usr/bin/true".into(),
        provider_hub_root: "/private/tmp".into(),
        gateway_binding_id: Some("00000000-0000-4000-8000-000000000001".into()),
    }
}

#[test]
fn routes_responses_direct_and_chat_through_registered_gateway() {
    let adapters = AdapterCatalog::standard();
    let direct = plan_provider_route(RoutePlanInput {
        provider: provider(ProviderProtocol::Responses),
        selected_model: "model-a".into(),
        adapters: adapters.clone(),
    });
    assert_eq!(direct.route_kind, RouteKind::Direct);
    assert!(direct.blockers.is_empty());
    assert_eq!(
        direct.codex_auth,
        DirectCodexAuth::EnvKey {
            env_key: "REMOTE_TOKEN".into()
        }
    );

    let gateway = plan_provider_route(RoutePlanInput {
        provider: provider(ProviderProtocol::ChatCompletions),
        selected_model: "model-a".into(),
        adapters,
    });
    assert_eq!(gateway.route_kind, RouteKind::Gateway);
    assert_eq!(
        gateway.adapter_id.as_deref(),
        Some("responses_to_chat_completions")
    );
    assert!(gateway.blockers.is_empty());
}

#[test]
fn explicit_responses_gateway_route_uses_gateway_auth_without_an_adapter() {
    let mut responses = provider(ProviderProtocol::Responses);
    responses.codex.route_via_gateway = true;

    let route = plan_provider_route(RoutePlanInput {
        provider: responses,
        selected_model: "model-a".into(),
        adapters: AdapterCatalog::standard(),
    });

    assert_eq!(route.route_kind, RouteKind::Gateway);
    assert_eq!(route.codex_auth, DirectCodexAuth::Gateway);
    assert_eq!(route.upstream_protocol, ProviderProtocol::Responses);
    assert_eq!(route.adapter_id, None);
    assert_eq!(route.adapter_version, None);
    assert!(route.blockers.is_empty());
}

#[test]
fn gateway_projection_uses_a_custom_provider_identity_for_local_compact() {
    let mut gateway_provider = provider(ProviderProtocol::Responses);
    gateway_provider.name = "OpenAI".into();
    gateway_provider.codex.route_via_gateway = true;
    let gateway_route = plan_provider_route(RoutePlanInput {
        provider: gateway_provider,
        selected_model: "model-a".into(),
        adapters: AdapterCatalog::standard(),
    });
    let gateway_plan = plan_profile_attach(gateway_route, context());
    assert_eq!(
        gateway_plan.config_projection.display_name,
        "LAM Gateway · OpenAI"
    );
    assert_ne!(gateway_plan.config_projection.display_name, "OpenAI");
    assert_ne!(gateway_plan.config_projection.display_name, "Azure");
    assert!(gateway_plan
        .config_projection
        .base_url
        .starts_with("http://127.0.0.1:"));

    let mut direct_provider = provider(ProviderProtocol::Responses);
    direct_provider.name = "Company API".into();
    let direct_route = plan_provider_route(RoutePlanInput {
        provider: direct_provider,
        selected_model: "model-a".into(),
        adapters: AdapterCatalog::standard(),
    });
    let direct_plan = plan_profile_attach(direct_route, context());
    assert_eq!(direct_plan.config_projection.display_name, "Company API");
}

#[test]
fn missing_adapter_invalid_model_and_explicit_runtime_readiness_are_blockers() {
    let mut chat = provider(ProviderProtocol::ChatCompletions);
    chat.adapter = AdapterConfig::None;
    let missing = plan_provider_route(RoutePlanInput {
        provider: chat,
        selected_model: "model-a".into(),
        adapters: AdapterCatalog::standard(),
    });
    assert!(missing.blockers.contains(&PlanBlocker::AdapterRequired));
    let invalid = plan_provider_route(RoutePlanInput {
        provider: provider(ProviderProtocol::Responses),
        selected_model: "unknown".into(),
        adapters: AdapterCatalog::standard(),
    });
    assert!(invalid.blockers.contains(&PlanBlocker::ModelNotFound));

    let route = plan_provider_route(RoutePlanInput {
        provider: provider(ProviderProtocol::ChatCompletions),
        selected_model: "model-a".into(),
        adapters: AdapterCatalog::empty(),
    });
    assert!(route
        .blockers
        .contains(&PlanBlocker::AdapterRegistryMismatch));
    let mut ctx = context();
    ctx.credential_ready = false;
    ctx.gateway.available = false;
    ctx.binding_drifted = true;
    let attach = plan_profile_attach(route, ctx);
    assert!(attach.blockers.contains(&PlanBlocker::CredentialMissing));
    assert!(attach.blockers.contains(&PlanBlocker::GatewayUnavailable));
    assert!(attach.blockers.contains(&PlanBlocker::BindingDrift));
}

#[test]
fn fingerprint_covers_all_state_and_preview_is_redacted() {
    let route = plan_provider_route(RoutePlanInput {
        provider: provider(ProviderProtocol::Responses),
        selected_model: "model-a".into(),
        adapters: AdapterCatalog::standard(),
    });
    let base = plan_profile_attach(route.clone(), context());
    let mut variants = Vec::new();
    let mut c = context();
    c.provider_store_revision += 1;
    variants.push(plan_profile_attach(route.clone(), c));
    let mut c = context();
    c.binding_store_revision += 1;
    variants.push(plan_profile_attach(route.clone(), c));
    let mut c = context();
    c.expected_binding_revision = Some(4);
    variants.push(plan_profile_attach(route.clone(), c));
    let mut c = context();
    c.source_config_hash.push('x');
    variants.push(plan_profile_attach(route.clone(), c));
    let mut c = context();
    c.gateway.endpoint_version += 1;
    variants.push(plan_profile_attach(route.clone(), c));
    let mut c = context();
    c.planner_options.insert("new".into(), "value".into());
    variants.push(plan_profile_attach(route.clone(), c));
    let mut changed = route;
    changed.selected_model = "model-b".into();
    variants.push(plan_profile_attach(changed, context()));
    assert!(variants.iter().all(|p| p.fingerprint != base.fingerprint));
    let json = serde_json::to_string(&base).unwrap();
    assert!(!json.contains("synthetic-secret-marker"));
    assert!(!base.redacted_preview.contains("REMOTE_TOKEN="));
}

#[test]
fn issued_plan_expires_and_rejects_replay_tamper_and_stale_state() {
    let route = plan_provider_route(RoutePlanInput {
        provider: provider(ProviderProtocol::Responses),
        selected_model: "model-a".into(),
        adapters: AdapterCatalog::standard(),
    });
    let plan = plan_profile_attach(route, context());
    let mut registry = DryRunRegistry::new(100, 4);
    let ticket = registry.issue(&plan, 1_000);
    assert_eq!(
        registry
            .consume(&ticket, &plan.fingerprint, &plan.fingerprint, 1_050)
            .unwrap(),
        plan.fingerprint
    );
    assert_eq!(
        registry
            .consume(&ticket, &plan.fingerprint, &plan.fingerprint, 1_060)
            .unwrap_err()
            .code,
        "ATTACH_PLAN_REPLAYED"
    );
    let ticket = registry.issue(&plan, 2_000);
    assert_eq!(
        registry
            .consume(&ticket, "tampered", &plan.fingerprint, 2_010)
            .unwrap_err()
            .code,
        "ATTACH_PLAN_TAMPERED"
    );
    let ticket = registry.issue(&plan, 3_000);
    assert_eq!(
        registry
            .consume(&ticket, &plan.fingerprint, "different-current-state", 3_010)
            .unwrap_err()
            .code,
        "ATTACH_PLAN_STALE"
    );
    let ticket = registry.issue(&plan, 4_000);
    assert_eq!(
        registry
            .consume(&ticket, &plan.fingerprint, &plan.fingerprint, 4_101)
            .unwrap_err()
            .code,
        "ATTACH_PLAN_EXPIRED"
    );
}
