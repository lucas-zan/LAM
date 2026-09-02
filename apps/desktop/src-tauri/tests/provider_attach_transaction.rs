use localagentmanager_core::provider_attach_transaction::*;
use localagentmanager_core::provider_binding::RouteKind;
use localagentmanager_core::provider_binding::{
    ManagedConfigProjection, ProfileBindingCollection, ProfileProviderBinding, ProjectionOwnership,
};
use localagentmanager_core::provider_config_editor::ConfigManagedProjection;
use localagentmanager_core::provider_credentials::{CredentialSource, UpstreamAuth};
use localagentmanager_core::provider_planner::*;
use localagentmanager_core::provider_v2::*;
use localagentmanager_core::storage::{InstallationLock, StoreOptions, VersionedFileStore};
use std::collections::BTreeMap;
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn journal_repo(root: &std::path::Path) -> AttachJournalRepository {
    #[cfg(unix)]
    {
        fs::set_permissions(root, fs::Permissions::from_mode(0o700)).unwrap();
    }
    AttachJournalRepository::new(VersionedFileStore::new(
        root.join("attach-journal.json"),
        InstallationLock::new(root.join("provider-hub.lock"), Duration::from_secs(1)),
        1,
        StoreOptions {
            max_bytes: 4 * 1024 * 1024,
        },
    ))
}

fn record(id: &str, profile: &str, created_at_ms: u64) -> AttachJournalRecord {
    let config_projection = ConfigManagedProjection {
        before_hash: "before".into(),
        applied_hash: "intended".into(),
        provider_id: "provider-a".into(),
        previous_values: BTreeMap::new(),
        managed_values: BTreeMap::new(),
        provider_table_created_by_lam: false,
    };
    let binding_projection = ManagedConfigProjection {
        config_path: "/tmp/codex/config.toml".into(),
        ownership: ProjectionOwnership::Managed,
        before_hash: "before".into(),
        applied_hash: "intended".into(),
        previous_values: BTreeMap::new(),
        managed_values: BTreeMap::new(),
        provider_table_created_by_lam: false,
    };
    AttachJournalRecord {
        journal_version: 1,
        operation_id: id.into(),
        operation: AttachOperation::Attach,
        state: AttachJournalState::Prepared,
        profile_id: profile.into(),
        provider_id: "provider-a".into(),
        selected_model: "model-a".into(),
        route_kind: RouteKind::Direct,
        expected_provider_store_revision: 2,
        expected_binding_store_revision: 3,
        expected_binding_revision: None,
        config_path: "/tmp/codex/config.toml".into(),
        config_before_hash: "before".into(),
        config_intended_hash: "intended".into(),
        managed_projection: Some(config_projection),
        previous_binding: None,
        prepared_binding: Some(ProfileProviderBinding::new_for_test(
            profile,
            "provider-a",
            "model-a",
            binding_projection,
            2,
            "fingerprint",
        )),
        new_gateway_binding_ref: None,
        superseded_gateway_binding_ref: None,
        created_at_ms,
        updated_at_ms: created_at_ms,
        last_error_code: None,
    }
}

#[test]
fn journal_create_and_closed_transitions_are_versioned_and_deterministic() {
    let root = tempfile::tempdir().unwrap();
    let repo = journal_repo(root.path());
    let created = repo.create(0, record("op-1", "profile-a", 100)).unwrap();
    assert_eq!(created.revision, 1);
    for (revision, state) in [
        (1, AttachJournalState::ConfigCommitted),
        (2, AttachJournalState::BindingCommitted),
        (3, AttachJournalState::Completed),
    ] {
        repo.transition(revision, "op-1", state, revision + 100, None)
            .unwrap();
    }
    let snapshot = repo.load().unwrap();
    assert_eq!(
        snapshot.value.records[0].state,
        AttachJournalState::Completed
    );
    assert_eq!(
        repo.transition(4, "op-1", AttachJournalState::Prepared, 200, None)
            .unwrap_err()
            .code,
        "ATTACH_JOURNAL_TRANSITION_INVALID"
    );
}

#[test]
fn journal_rejects_duplicate_active_illegal_skip_and_secret_or_oversized_content() {
    let root = tempfile::tempdir().unwrap();
    let repo = journal_repo(root.path());
    repo.create(0, record("op-1", "profile-a", 100)).unwrap();
    assert_eq!(
        repo.create(1, record("op-2", "profile-a", 101))
            .unwrap_err()
            .code,
        "ATTACH_JOURNAL_PROFILE_ACTIVE"
    );
    assert_eq!(
        repo.transition(1, "op-1", AttachJournalState::BindingCommitted, 102, None)
            .unwrap_err()
            .code,
        "ATTACH_JOURNAL_TRANSITION_INVALID"
    );
    let mut secret = record("op-secret", "profile-b", 103);
    secret.last_error_code = Some("LAM_TEST_SECRET_sk-never-store".into());
    assert_eq!(
        repo.create(1, secret).unwrap_err().code,
        "ATTACH_JOURNAL_SENSITIVE_DATA"
    );
    let mut huge = record("op-huge", "profile-b", 104);
    huge.provider_id = "x".repeat(AttachJournalRepository::MAX_RECORD_BYTES + 1);
    assert_eq!(
        repo.create(1, huge).unwrap_err().code,
        "ATTACH_JOURNAL_RECORD_TOO_LARGE"
    );
}

#[test]
fn journal_retention_keeps_recent_newest_active_and_manual_records() {
    let root = tempfile::tempdir().unwrap();
    let repo = journal_repo(root.path());
    let now = 4_000_000_000_u64;
    let old = now - AttachJournalRepository::RETENTION_MS - 1_000;
    let mut revision = 0;
    for index in 0..105 {
        let id = format!("terminal-{index:03}");
        repo.create(revision, record(&id, &format!("p-{index}"), old + index))
            .unwrap();
        revision += 1;
        repo.transition(
            revision,
            &id,
            AttachJournalState::RolledBack,
            old + index,
            None,
        )
        .unwrap();
        revision += 1;
    }
    let mut manual = record("manual", "manual-profile", old);
    manual.state = AttachJournalState::ManualIntervention;
    repo.create(revision, manual).unwrap();
    revision += 1;
    let compacted = repo.compact(revision, now).unwrap();
    assert_eq!(
        compacted
            .value
            .records
            .iter()
            .filter(|r| r.state == AttachJournalState::RolledBack)
            .count(),
        100
    );
    assert!(compacted
        .value
        .records
        .iter()
        .any(|r| r.operation_id == "manual"));
    let with_active = repo
        .create(compacted.revision, record("active", "active-profile", old))
        .unwrap();
    let unchanged = repo.compact(with_active.revision, now).unwrap();
    assert_eq!(unchanged.revision, with_active.revision);
    assert!(unchanged
        .value
        .records
        .iter()
        .any(|r| r.operation_id == "active"));
}

#[test]
fn journal_corrupt_or_future_envelope_fails_closed_without_overwrite() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("attach-journal.json");
    #[cfg(unix)]
    {
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    }
    fs::write(&path, b"{broken").unwrap();
    #[cfg(unix)]
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let before = fs::read(&path).unwrap();
    assert_eq!(
        journal_repo(root.path()).load().unwrap_err().code,
        "STORE_INVALID"
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    fs::write(&path, br#"{"schemaVersion":2,"revision":1,"records":[]}"#).unwrap();
    #[cfg(unix)]
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        journal_repo(root.path()).load().unwrap_err().code,
        "STORE_FUTURE_SCHEMA"
    );
}

#[derive(Default)]
struct FakeGateway {
    prepared: Mutex<Vec<String>>,
    revoked: Mutex<Vec<String>>,
}

impl GatewayBindingLifecycle for FakeGateway {
    fn prepare(
        &self,
        _guard: &localagentmanager_core::storage::InstallationLockGuard,
        _operation_id: &str,
        plan: &ProfileAttachPlan,
    ) -> localagentmanager_core::Result<Option<String>> {
        let reference = plan
            .gateway_binding_id
            .clone()
            .expect("planned Gateway binding");
        self.prepared.lock().unwrap().push(reference.clone());
        Ok(Some(reference))
    }
    fn revoke(
        &self,
        _guard: &localagentmanager_core::storage::InstallationLockGuard,
        reference: &str,
    ) -> localagentmanager_core::Result<()> {
        self.revoked.lock().unwrap().push(reference.into());
        Ok(())
    }
}

fn provider(protocol: ProviderProtocol) -> ProviderProfileV2 {
    ProviderProfileV2 {
        id: "provider-a".into(),
        name: "Provider A".into(),
        protocol,
        base_url: "https://api.example.test/v1".into(),
        default_model: "model-a".into(),
        models: vec![
            ProviderModel {
                id: "model-a".into(),
                label: "A".into(),
                capabilities: None,
                context_window: None,
            },
            ProviderModel {
                id: "model-b".into(),
                label: "B".into(),
                capabilities: None,
                context_window: None,
            },
        ],
        upstream_auth: UpstreamAuth::Bearer {
            source: CredentialSource::Env {
                env_key: "PROVIDER_TOKEN".into(),
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
        created_at: "2026-07-13T00:00:00Z".into(),
        updated_at: "2026-07-13T00:00:00Z".into(),
    }
}

struct Fixture {
    _root: tempfile::TempDir,
    config_path: std::path::PathBuf,
    provider_store: VersionedFileStore<ProviderCollection>,
    binding_store: VersionedFileStore<ProfileBindingCollection>,
    journal_store: VersionedFileStore<AttachJournalCollection>,
    lock: InstallationLock,
    gateway: Arc<FakeGateway>,
}

fn fixture(protocol: ProviderProtocol) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let lock = InstallationLock::new(
        root.path().join("provider-hub.lock"),
        Duration::from_secs(1),
    );
    let provider_store = VersionedFileStore::new(
        root.path().join("providers.json"),
        lock.clone(),
        1,
        StoreOptions::default(),
    );
    let binding_store = VersionedFileStore::new(
        root.path().join("bindings.json"),
        lock.clone(),
        1,
        StoreOptions::default(),
    );
    let journal_store = VersionedFileStore::new(
        root.path().join("attach-journal.json"),
        lock.clone(),
        1,
        StoreOptions::default(),
    );
    provider_store
        .compare_and_swap(
            0,
            &ProviderCollection {
                providers: vec![provider(protocol)],
            },
        )
        .unwrap();
    let config_path = root.path().join("config.toml");
    fs::write(
        &config_path,
        "# user config\n[mcp_servers.keep]\ncommand=\"keep\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
    Fixture {
        _root: root,
        config_path,
        provider_store,
        binding_store,
        journal_store,
        lock,
        gateway: Arc::new(FakeGateway::default()),
    }
}

fn attach_plan(
    value: &Fixture,
    model: &str,
    expected_binding_revision: Option<u64>,
) -> ProfileAttachPlan {
    let providers = value.provider_store.load_or_default().unwrap();
    let bindings = value.binding_store.load_or_default().unwrap();
    let provider = providers.value.providers[0].clone();
    let route = plan_provider_route(RoutePlanInput {
        provider,
        selected_model: model.into(),
        adapters: AdapterCatalog::standard(),
    });
    plan_profile_attach(
        route,
        AttachPlanContext {
            profile_id: "profile-a".into(),
            provider_store_revision: providers.revision,
            config_path: value.config_path.to_string_lossy().into_owned(),
            expected_binding_revision,
            binding_store_revision: bindings.revision,
            source_config_hash: localagentmanager_core::provider_config_editor::config_hash(
                &fs::read(&value.config_path).unwrap(),
            ),
            binding_drifted: false,
            credential_ready: true,
            gateway: GatewayPlanContext {
                base_url: "http://127.0.0.1:43123/v1".into(),
                available: true,
                endpoint_version: 1,
            },
            planner_options: BTreeMap::new(),
            auth_helper_path: "/usr/bin/true".into(),
            provider_hub_root: value._root.path().to_string_lossy().into_owned(),
            gateway_binding_id: Some(uuid::Uuid::new_v4().to_string()),
        },
    )
}

fn coordinator(value: &Fixture) -> AttachTransactionCoordinator<FakeGateway> {
    AttachTransactionCoordinator::new(
        value.lock.clone(),
        value.provider_store.clone(),
        value.binding_store.clone(),
        value.journal_store.clone(),
        value.gateway.clone(),
        1,
    )
}

#[test]
fn transaction_attach_commits_config_binding_and_journal_then_rejects_ticket_replay() {
    let value = fixture(ProviderProtocol::Responses);
    let plan = attach_plan(&value, "model-a", None);
    let mut registry = DryRunRegistry::new(300_000, 128);
    let ticket = registry.issue(&plan, 1_000);
    let result = coordinator(&value)
        .execute_attach(&mut registry, &ticket, &plan, 1_001, None)
        .unwrap();
    assert_eq!(result.state, AttachJournalState::Completed);
    assert!(fs::read_to_string(&value.config_path)
        .unwrap()
        .contains("model-a"));
    let binding = value
        .binding_store
        .load_or_default()
        .unwrap()
        .value
        .bindings
        .pop()
        .unwrap();
    assert_eq!(binding.selected_model, "model-a");
    assert_eq!(
        value.journal_store.load_or_default().unwrap().value.records[0].state,
        AttachJournalState::Completed
    );
    assert_eq!(
        coordinator(&value)
            .execute_attach(&mut registry, &ticket, &plan, 1_002, None)
            .unwrap_err()
            .code,
        "ATTACH_PLAN_REPLAYED"
    );
}

#[test]
fn transaction_stale_config_has_zero_side_effect_and_ticket_remains_retryable() {
    let value = fixture(ProviderProtocol::Responses);
    let plan = attach_plan(&value, "model-a", None);
    let mut registry = DryRunRegistry::new(300_000, 128);
    let ticket = registry.issue(&plan, 1_000);
    let original = fs::read_to_string(&value.config_path).unwrap();
    fs::write(&value.config_path, format!("{original}# changed\n")).unwrap();
    assert_eq!(
        coordinator(&value)
            .execute_attach(&mut registry, &ticket, &plan, 1_001, None)
            .unwrap_err()
            .code,
        "ATTACH_PLAN_STALE"
    );
    assert_eq!(value.binding_store.load_or_default().unwrap().revision, 0);
    assert_eq!(value.journal_store.load_or_default().unwrap().revision, 0);
    fs::write(&value.config_path, original).unwrap();
    coordinator(&value)
        .execute_attach(&mut registry, &ticket, &plan, 1_002, None)
        .unwrap();
}

#[test]
fn transaction_binding_failure_rolls_back_managed_config_and_rebind_keeps_old_binding() {
    let value = fixture(ProviderProtocol::Responses);
    let first = attach_plan(&value, "model-a", None);
    let mut registry = DryRunRegistry::new(300_000, 128);
    let ticket = registry.issue(&first, 1_000);
    coordinator(&value)
        .execute_attach(&mut registry, &ticket, &first, 1_001, None)
        .unwrap();
    let before_config = fs::read_to_string(&value.config_path).unwrap();
    let before_binding = value
        .binding_store
        .load_or_default()
        .unwrap()
        .value
        .bindings[0]
        .clone();
    let rebind = attach_plan(&value, "model-b", Some(before_binding.revision));
    let ticket = registry.issue(&rebind, 2_000);
    assert_eq!(
        coordinator(&value)
            .execute_attach(
                &mut registry,
                &ticket,
                &rebind,
                2_001,
                Some(TransactionFault::FailBindingCommit)
            )
            .unwrap_err()
            .code,
        "ATTACH_BINDING_COMMIT_FAILED"
    );
    assert_eq!(
        fs::read_to_string(&value.config_path).unwrap(),
        before_config
    );
    assert_eq!(
        value
            .binding_store
            .load_or_default()
            .unwrap()
            .value
            .bindings[0],
        before_binding
    );
    assert_eq!(
        value
            .journal_store
            .load_or_default()
            .unwrap()
            .value
            .records
            .last()
            .unwrap()
            .state,
        AttachJournalState::RolledBack
    );
}

#[test]
fn transaction_detach_is_idempotent_and_ownership_conflict_makes_no_mutation() {
    let value = fixture(ProviderProtocol::Responses);
    let plan = attach_plan(&value, "model-a", None);
    let mut registry = DryRunRegistry::new(300_000, 128);
    let ticket = registry.issue(&plan, 1_000);
    coordinator(&value)
        .execute_attach(&mut registry, &ticket, &plan, 1_001, None)
        .unwrap();
    let snapshot = value.binding_store.load_or_default().unwrap();
    let binding = snapshot.value.bindings[0].clone();
    let attached_config = fs::read_to_string(&value.config_path).unwrap();
    let detach = plan_profile_detach(
        &binding,
        snapshot.revision,
        localagentmanager_core::provider_config_editor::config_hash(
            &fs::read(&value.config_path).unwrap(),
        ),
    );
    let ticket = registry.issue_detach(&detach, 2_000);
    let drifted = fs::read_to_string(&value.config_path)
        .unwrap()
        .replace("model-a", "user-model");
    fs::write(&value.config_path, &drifted).unwrap();
    assert_eq!(
        coordinator(&value)
            .execute_detach(&mut registry, &ticket, &detach, 2_001, None)
            .unwrap_err()
            .code,
        "ATTACH_PLAN_STALE"
    );
    assert_eq!(
        value
            .binding_store
            .load_or_default()
            .unwrap()
            .value
            .bindings[0],
        binding
    );
    fs::write(&value.config_path, &attached_config).unwrap();
    let fresh = plan_profile_detach(
        &binding,
        snapshot.revision,
        localagentmanager_core::provider_config_editor::config_hash(
            &fs::read(&value.config_path).unwrap(),
        ),
    );
    let ticket = registry.issue_detach(&fresh, 2_100);
    coordinator(&value)
        .execute_detach(&mut registry, &ticket, &fresh, 2_101, None)
        .unwrap();
    assert!(value
        .binding_store
        .load_or_default()
        .unwrap()
        .value
        .bindings
        .is_empty());
    let repeat = plan_profile_detach(
        &binding,
        value.binding_store.load_or_default().unwrap().revision,
        localagentmanager_core::provider_config_editor::config_hash(
            &fs::read(&value.config_path).unwrap(),
        ),
    );
    let repeat_ticket = registry.issue_detach(&repeat, 2_102);
    assert!(
        coordinator(&value)
            .execute_detach(&mut registry, &repeat_ticket, &repeat, 2_103, None)
            .unwrap()
            .idempotent
    );
}

#[test]
fn transaction_crash_faults_leave_each_documented_durable_state() {
    for (fault, state, has_config, has_binding) in [
        (
            TransactionFault::CrashAfterPrepared,
            AttachJournalState::Prepared,
            false,
            false,
        ),
        (
            TransactionFault::CrashAfterConfigCommitted,
            AttachJournalState::ConfigCommitted,
            true,
            false,
        ),
        (
            TransactionFault::CrashAfterBindingCommitted,
            AttachJournalState::BindingCommitted,
            true,
            true,
        ),
    ] {
        let value = fixture(ProviderProtocol::Responses);
        let plan = attach_plan(&value, "model-a", None);
        let mut registry = DryRunRegistry::new(300_000, 128);
        let ticket = registry.issue(&plan, 1_000);
        assert_eq!(
            coordinator(&value)
                .execute_attach(&mut registry, &ticket, &plan, 1_001, Some(fault))
                .unwrap_err()
                .code,
            "ATTACH_TRANSACTION_INTERRUPTED"
        );
        assert_eq!(
            value.journal_store.load_or_default().unwrap().value.records[0].state,
            state
        );
        assert_eq!(
            fs::read_to_string(&value.config_path)
                .unwrap()
                .contains("model-a"),
            has_config
        );
        assert_eq!(
            !value
                .binding_store
                .load_or_default()
                .unwrap()
                .value
                .bindings
                .is_empty(),
            has_binding
        );
    }
}

#[test]
fn transaction_gateway_rebind_revokes_new_on_failure_and_old_only_after_commit() {
    let value = fixture(ProviderProtocol::ChatCompletions);
    let mut registry = DryRunRegistry::new(300_000, 128);
    let first = attach_plan(&value, "model-a", None);
    let ticket = registry.issue(&first, 1_000);
    coordinator(&value)
        .execute_attach(&mut registry, &ticket, &first, 1_001, None)
        .unwrap();
    let old_reference = value
        .binding_store
        .load_or_default()
        .unwrap()
        .value
        .bindings[0]
        .gateway_binding_id
        .clone()
        .unwrap();

    let binding = value
        .binding_store
        .load_or_default()
        .unwrap()
        .value
        .bindings[0]
        .clone();
    let failed = attach_plan(&value, "model-b", Some(binding.revision));
    let ticket = registry.issue(&failed, 2_000);
    coordinator(&value)
        .execute_attach(
            &mut registry,
            &ticket,
            &failed,
            2_001,
            Some(TransactionFault::FailBindingCommit),
        )
        .unwrap_err();
    let revoked = value.gateway.revoked.lock().unwrap().clone();
    assert!(!revoked.contains(&old_reference));
    assert!(revoked.iter().any(|item| item != &old_reference));
    assert_eq!(
        value
            .binding_store
            .load_or_default()
            .unwrap()
            .value
            .bindings[0]
            .gateway_binding_id
            .as_deref(),
        Some(old_reference.as_str())
    );

    let binding = value
        .binding_store
        .load_or_default()
        .unwrap()
        .value
        .bindings[0]
        .clone();
    let success = attach_plan(&value, "model-b", Some(binding.revision));
    let ticket = registry.issue(&success, 3_000);
    coordinator(&value)
        .execute_attach(&mut registry, &ticket, &success, 3_001, None)
        .unwrap();
    assert!(value
        .gateway
        .revoked
        .lock()
        .unwrap()
        .contains(&old_reference));
}

#[test]
fn transaction_revokes_prepared_gateway_if_journal_creation_fails() {
    let value = fixture(ProviderProtocol::ChatCompletions);
    let mut active = record("already-active", "profile-a", 10);
    active.config_path = value.config_path.to_string_lossy().into_owned();
    value
        .journal_store
        .compare_and_swap(
            0,
            &AttachJournalCollection {
                records: vec![active],
            },
        )
        .unwrap();
    let plan = attach_plan(&value, "model-a", None);
    let mut registry = DryRunRegistry::new(300_000, 128);
    let ticket = registry.issue(&plan, 1_000);
    assert_eq!(
        coordinator(&value)
            .execute_attach(&mut registry, &ticket, &plan, 1_001, None)
            .unwrap_err()
            .code,
        "ATTACH_JOURNAL_PROFILE_ACTIVE"
    );
    let prepared = value.gateway.prepared.lock().unwrap().clone();
    let revoked = value.gateway.revoked.lock().unwrap().clone();
    assert_eq!(prepared.len(), 1);
    assert_eq!(revoked, prepared);
}

#[test]
fn transaction_detects_external_config_drift_before_binding_commit() {
    let value = fixture(ProviderProtocol::Responses);
    let plan = attach_plan(&value, "model-a", None);
    let mut registry = DryRunRegistry::new(300_000, 128);
    let ticket = registry.issue(&plan, 1_000);
    assert_eq!(
        coordinator(&value)
            .execute_attach(
                &mut registry,
                &ticket,
                &plan,
                1_001,
                Some(TransactionFault::DriftConfigBeforeBindingCommit)
            )
            .unwrap_err()
            .code,
        "CODEX_CONFIG_OWNERSHIP_CONFLICT"
    );
    assert!(value
        .binding_store
        .load_or_default()
        .unwrap()
        .value
        .bindings
        .is_empty());
    assert_eq!(
        value.journal_store.load_or_default().unwrap().value.records[0].state,
        AttachJournalState::ManualIntervention
    );
}

#[test]
fn recovery_rolls_back_every_precommit_state_and_is_idempotent() {
    for fault in [
        TransactionFault::CrashAfterPrepared,
        TransactionFault::CrashAfterConfigCommitted,
    ] {
        let value = fixture(ProviderProtocol::Responses);
        let original = fs::read_to_string(&value.config_path).unwrap();
        let plan = attach_plan(&value, "model-a", None);
        let mut registry = DryRunRegistry::new(300_000, 128);
        let ticket = registry.issue(&plan, 1_000);
        coordinator(&value)
            .execute_attach(&mut registry, &ticket, &plan, 1_001, Some(fault))
            .unwrap_err();
        let report = coordinator(&value).recover_pending(2_000, 16).unwrap();
        assert_eq!(report.recovered, 1);
        assert_eq!(fs::read_to_string(&value.config_path).unwrap(), original);
        assert!(value
            .binding_store
            .load_or_default()
            .unwrap()
            .value
            .bindings
            .is_empty());
        assert_eq!(
            value.journal_store.load_or_default().unwrap().value.records[0].state,
            AttachJournalState::RolledBack
        );
        assert_eq!(
            coordinator(&value)
                .recover_pending(2_001, 16)
                .unwrap()
                .recovered,
            0
        );
    }
}

#[test]
fn recovery_rolls_forward_after_binding_commit_and_cleans_up_once() {
    let value = fixture(ProviderProtocol::ChatCompletions);
    let plan = attach_plan(&value, "model-a", None);
    let mut registry = DryRunRegistry::new(300_000, 128);
    let ticket = registry.issue(&plan, 1_000);
    coordinator(&value)
        .execute_attach(
            &mut registry,
            &ticket,
            &plan,
            1_001,
            Some(TransactionFault::CrashAfterBindingCommitted),
        )
        .unwrap_err();
    let report = coordinator(&value).recover_pending(2_000, 16).unwrap();
    assert_eq!(report.recovered, 1);
    assert_eq!(
        value.journal_store.load_or_default().unwrap().value.records[0].state,
        AttachJournalState::Completed
    );
    let revoked = value.gateway.revoked.lock().unwrap().len();
    coordinator(&value).recover_pending(2_001, 16).unwrap();
    assert_eq!(value.gateway.revoked.lock().unwrap().len(), revoked);
}

#[test]
fn recovery_unknown_config_becomes_manual_intervention_without_overwrite() {
    let value = fixture(ProviderProtocol::Responses);
    let plan = attach_plan(&value, "model-a", None);
    let mut registry = DryRunRegistry::new(300_000, 128);
    let ticket = registry.issue(&plan, 1_000);
    coordinator(&value)
        .execute_attach(
            &mut registry,
            &ticket,
            &plan,
            1_001,
            Some(TransactionFault::CrashAfterConfigCommitted),
        )
        .unwrap_err();
    fs::write(&value.config_path, "# user changed during crash\n").unwrap();
    let before = fs::read_to_string(&value.config_path).unwrap();
    assert_eq!(
        coordinator(&value)
            .recover_pending(2_000, 16)
            .unwrap_err()
            .code,
        "ATTACH_RECOVERY_OWNERSHIP_CONFLICT"
    );
    assert_eq!(fs::read_to_string(&value.config_path).unwrap(), before);
    assert_eq!(
        value.journal_store.load_or_default().unwrap().value.records[0].state,
        AttachJournalState::ManualIntervention
    );
}

#[test]
fn recovery_bound_leaves_remaining_active_records_for_next_start() {
    let value = fixture(ProviderProtocol::Responses);
    let mut records = vec![
        record("recover-a", "profile-a", 10),
        record("recover-b", "profile-b", 11),
    ];
    for item in &mut records {
        item.config_path = value.config_path.to_string_lossy().into_owned();
        item.config_before_hash = localagentmanager_core::provider_config_editor::config_hash(
            &fs::read(&value.config_path).unwrap(),
        );
        item.config_intended_hash = "unused-intended".into();
    }
    value
        .journal_store
        .compare_and_swap(0, &AttachJournalCollection { records })
        .unwrap();
    let first = coordinator(&value).recover_pending(100, 1).unwrap();
    assert_eq!(first.recovered, 1);
    assert_eq!(first.remaining, 1);
    let second = coordinator(&value).recover_pending(101, 1).unwrap();
    assert_eq!(second.recovered, 1);
    assert_eq!(second.remaining, 0);
}

#[test]
fn recovery_handles_detach_before_and_after_its_binding_commit() {
    for (fault, remains_attached) in [
        (TransactionFault::CrashAfterConfigCommitted, true),
        (TransactionFault::CrashAfterBindingCommitted, false),
    ] {
        let value = fixture(ProviderProtocol::Responses);
        let mut registry = DryRunRegistry::new(300_000, 128);
        let attach = attach_plan(&value, "model-a", None);
        let ticket = registry.issue(&attach, 1_000);
        coordinator(&value)
            .execute_attach(&mut registry, &ticket, &attach, 1_001, None)
            .unwrap();
        let snapshot = value.binding_store.load_or_default().unwrap();
        let binding = snapshot.value.bindings[0].clone();
        let attached_config = fs::read_to_string(&value.config_path).unwrap();
        let detach = plan_profile_detach(
            &binding,
            snapshot.revision,
            localagentmanager_core::provider_config_editor::config_hash(attached_config.as_bytes()),
        );
        let ticket = registry.issue_detach(&detach, 2_000);
        coordinator(&value)
            .execute_detach(&mut registry, &ticket, &detach, 2_001, Some(fault))
            .unwrap_err();
        coordinator(&value).recover_pending(3_000, 16).unwrap();
        assert_eq!(
            !value
                .binding_store
                .load_or_default()
                .unwrap()
                .value
                .bindings
                .is_empty(),
            remains_attached
        );
        assert_eq!(
            fs::read_to_string(&value.config_path)
                .unwrap()
                .contains("model-a"),
            remains_attached
        );
        assert_eq!(
            value
                .journal_store
                .load_or_default()
                .unwrap()
                .value
                .records
                .last()
                .unwrap()
                .state,
            if remains_attached {
                AttachJournalState::RolledBack
            } else {
                AttachJournalState::Completed
            }
        );
    }
}

#[test]
fn recovery_rebind_precommit_restores_old_route_and_revokes_only_new_reference() {
    let value = fixture(ProviderProtocol::ChatCompletions);
    let mut registry = DryRunRegistry::new(300_000, 128);
    let first = attach_plan(&value, "model-a", None);
    let ticket = registry.issue(&first, 1_000);
    coordinator(&value)
        .execute_attach(&mut registry, &ticket, &first, 1_001, None)
        .unwrap();
    let old = value
        .binding_store
        .load_or_default()
        .unwrap()
        .value
        .bindings[0]
        .clone();
    let rebind = attach_plan(&value, "model-b", Some(old.revision));
    let ticket = registry.issue(&rebind, 2_000);
    coordinator(&value)
        .execute_attach(
            &mut registry,
            &ticket,
            &rebind,
            2_001,
            Some(TransactionFault::CrashAfterConfigCommitted),
        )
        .unwrap_err();
    let new_reference = value
        .journal_store
        .load_or_default()
        .unwrap()
        .value
        .records
        .last()
        .unwrap()
        .new_gateway_binding_ref
        .clone()
        .unwrap();
    coordinator(&value).recover_pending(3_000, 16).unwrap();
    assert_eq!(
        value
            .binding_store
            .load_or_default()
            .unwrap()
            .value
            .bindings[0],
        old
    );
    assert!(fs::read_to_string(&value.config_path)
        .unwrap()
        .contains("model-a"));
    let revoked = value.gateway.revoked.lock().unwrap();
    assert!(revoked.contains(&new_reference));
    assert!(!revoked.contains(old.gateway_binding_id.as_ref().unwrap()));
}

#[test]
fn startup_recovery_entry_point_is_bounded_and_surfaces_outcomes() {
    let value = fixture(ProviderProtocol::Responses);
    let plan = attach_plan(&value, "model-a", None);
    let mut registry = DryRunRegistry::new(300_000, 128);
    let ticket = registry.issue(&plan, 1_000);
    coordinator(&value)
        .execute_attach(
            &mut registry,
            &ticket,
            &plan,
            1_001,
            Some(TransactionFault::CrashAfterPrepared),
        )
        .unwrap_err();
    let report = recover_provider_transactions_on_startup(&coordinator(&value), 2_000, 8).unwrap();
    assert_eq!(
        report,
        RecoveryReport {
            recovered: 1,
            remaining: 0
        }
    );
}
