use super::error::{AppError, Result};
use super::provider_binding::{
    ManagedConfigProjection, ProfileBindingCollection, ProfileProviderBinding, ProjectionOwnership,
    RouteKind,
};
use super::provider_config_editor::{
    apply_projection, apply_projection_file, config_hash, detach_projection,
    reapply_managed_projection, replace_config_file, replace_config_file_with_backup,
    ConfigManagedProjection,
};
use super::provider_planner::{DryRunRegistry, ProfileAttachPlan, ProfileDetachPlan};
use super::provider_v2::{ProviderCollection, ProviderProfileV2};

#[path = "provider_attach_update.rs"]
mod model_update;
use super::storage::{InstallationLock, InstallationLockGuard, StoreSnapshot, VersionedFileStore};
pub use model_update::AttachedProviderUpdate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttachOperation {
    Attach,
    Rebind,
    Detach,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttachJournalState {
    Prepared,
    ConfigCommitted,
    BindingCommitted,
    Completed,
    RolledBack,
    ManualIntervention,
}

impl AttachJournalState {
    fn is_active(self) -> bool {
        matches!(
            self,
            Self::Prepared | Self::ConfigCommitted | Self::BindingCommitted
        )
    }

    fn is_retained_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::RolledBack)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttachJournalRecord {
    pub journal_version: u32,
    pub operation_id: String,
    pub operation: AttachOperation,
    pub state: AttachJournalState,
    pub profile_id: String,
    pub provider_id: String,
    pub selected_model: String,
    pub route_kind: RouteKind,
    pub expected_provider_store_revision: u64,
    pub expected_binding_store_revision: u64,
    pub expected_binding_revision: Option<u64>,
    pub config_path: String,
    pub config_before_hash: String,
    pub config_intended_hash: String,
    pub managed_projection: Option<ConfigManagedProjection>,
    pub previous_binding: Option<ProfileProviderBinding>,
    pub prepared_binding: Option<ProfileProviderBinding>,
    pub new_gateway_binding_ref: Option<String>,
    pub superseded_gateway_binding_ref: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub last_error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_update: Option<AttachedProviderUpdate>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct AttachJournalCollection {
    pub records: Vec<AttachJournalRecord>,
}

#[derive(Clone)]
pub struct AttachJournalRepository {
    store: VersionedFileStore<AttachJournalCollection>,
}

impl AttachJournalRepository {
    pub const MAX_RECORD_BYTES: usize = 256 * 1024;
    pub const MAX_ACTIVE_RECORDS: usize = 128;
    pub const RETENTION_MS: u64 = 30 * 24 * 60 * 60 * 1_000;
    pub const MIN_TERMINAL_RECORDS: usize = 100;

    pub fn new(store: VersionedFileStore<AttachJournalCollection>) -> Self {
        Self { store }
    }

    pub fn load(&self) -> Result<StoreSnapshot<AttachJournalCollection>> {
        let snapshot = self.store.load_or_default()?;
        validate_collection(&snapshot.value)?;
        Ok(snapshot)
    }

    pub fn create(
        &self,
        expected_revision: u64,
        record: AttachJournalRecord,
    ) -> Result<StoreSnapshot<AttachJournalCollection>> {
        validate_record(&record)?;
        let mut snapshot = self.load()?;
        if snapshot
            .value
            .records
            .iter()
            .any(|item| item.operation_id == record.operation_id)
        {
            return Err(AppError::new(
                "ATTACH_JOURNAL_OPERATION_EXISTS",
                "journal operation id already exists",
            ));
        }
        if record.state.is_active()
            && snapshot
                .value
                .records
                .iter()
                .any(|item| item.profile_id == record.profile_id && item.state.is_active())
        {
            return Err(AppError::new(
                "ATTACH_JOURNAL_PROFILE_ACTIVE",
                "profile already has an active attach operation",
            ));
        }
        if record.state.is_active()
            && snapshot
                .value
                .records
                .iter()
                .filter(|item| item.state.is_active())
                .count()
                >= Self::MAX_ACTIVE_RECORDS
        {
            return Err(AppError::new(
                "ATTACH_JOURNAL_ACTIVE_LIMIT",
                "active attach journal limit reached",
            ));
        }
        snapshot.value.records.push(record);
        sort_records(&mut snapshot.value.records);
        self.store
            .compare_and_swap(expected_revision, &snapshot.value)
    }

    pub fn transition(
        &self,
        expected_revision: u64,
        operation_id: &str,
        state: AttachJournalState,
        updated_at_ms: u64,
        last_error_code: Option<String>,
    ) -> Result<StoreSnapshot<AttachJournalCollection>> {
        let mut snapshot = self.load()?;
        let record = snapshot
            .value
            .records
            .iter_mut()
            .find(|item| item.operation_id == operation_id)
            .ok_or_else(|| {
                AppError::new(
                    "ATTACH_JOURNAL_NOT_FOUND",
                    "attach journal operation was not found",
                )
            })?;
        if !valid_transition(record.state, state) {
            return Err(AppError::new(
                "ATTACH_JOURNAL_TRANSITION_INVALID",
                "attach journal state transition is invalid",
            ));
        }
        record.state = state;
        record.updated_at_ms = updated_at_ms;
        record.last_error_code = last_error_code;
        validate_record(record)?;
        self.store
            .compare_and_swap(expected_revision, &snapshot.value)
    }

    pub fn compact(
        &self,
        expected_revision: u64,
        now_ms: u64,
    ) -> Result<StoreSnapshot<AttachJournalCollection>> {
        let mut snapshot = self.load()?;
        if snapshot
            .value
            .records
            .iter()
            .any(|item| item.state.is_active())
        {
            return Ok(snapshot);
        }
        let mut terminals = snapshot
            .value
            .records
            .iter()
            .filter(|item| item.state.is_retained_terminal())
            .collect::<Vec<_>>();
        terminals.sort_by(|a, b| {
            b.updated_at_ms
                .cmp(&a.updated_at_ms)
                .then_with(|| b.operation_id.cmp(&a.operation_id))
        });
        let newest = terminals
            .into_iter()
            .take(Self::MIN_TERMINAL_RECORDS)
            .map(|item| item.operation_id.clone())
            .collect::<BTreeSet<_>>();
        snapshot.value.records.retain(|item| {
            item.state == AttachJournalState::ManualIntervention
                || !item.state.is_retained_terminal()
                || newest.contains(&item.operation_id)
                || now_ms.saturating_sub(item.updated_at_ms) <= Self::RETENTION_MS
        });
        sort_records(&mut snapshot.value.records);
        self.store
            .compare_and_swap(expected_revision, &snapshot.value)
    }
}

fn valid_transition(from: AttachJournalState, to: AttachJournalState) -> bool {
    matches!(
        (from, to),
        (
            AttachJournalState::Prepared,
            AttachJournalState::ConfigCommitted
                | AttachJournalState::RolledBack
                | AttachJournalState::ManualIntervention
        ) | (
            AttachJournalState::ConfigCommitted,
            AttachJournalState::BindingCommitted
                | AttachJournalState::RolledBack
                | AttachJournalState::ManualIntervention
        ) | (
            AttachJournalState::BindingCommitted,
            AttachJournalState::Completed | AttachJournalState::ManualIntervention
        )
    )
}

fn validate_collection(value: &AttachJournalCollection) -> Result<()> {
    let mut operation_ids = BTreeSet::new();
    let mut active_profiles = BTreeSet::new();
    let mut active_count = 0;
    for record in &value.records {
        validate_record(record)?;
        if !operation_ids.insert(&record.operation_id) {
            return Err(AppError::new(
                "ATTACH_JOURNAL_INVALID",
                "duplicate operation id in journal",
            ));
        }
        if record.state.is_active() {
            active_count += 1;
            if !active_profiles.insert(&record.profile_id) {
                return Err(AppError::new(
                    "ATTACH_JOURNAL_INVALID",
                    "multiple active operations for one profile",
                ));
            }
        }
    }
    if active_count > AttachJournalRepository::MAX_ACTIVE_RECORDS {
        return Err(AppError::new(
            "ATTACH_JOURNAL_ACTIVE_LIMIT",
            "active attach journal limit exceeded",
        ));
    }
    Ok(())
}

fn validate_record(record: &AttachJournalRecord) -> Result<()> {
    let expected_version = if record.provider_update.is_some() {
        2
    } else {
        1
    };
    if record.journal_version != expected_version
        || record.operation_id.trim().is_empty()
        || record.profile_id.trim().is_empty()
        || record.provider_id.trim().is_empty()
        || record.config_path.trim().is_empty()
        || record.updated_at_ms < record.created_at_ms
    {
        return Err(AppError::new(
            "ATTACH_JOURNAL_INVALID",
            "attach journal record is incomplete",
        ));
    }
    let shape_valid = match record.operation {
        AttachOperation::Attach => {
            record.previous_binding.is_none() && record.prepared_binding.is_some()
        }
        AttachOperation::Rebind => {
            record.previous_binding.is_some() && record.prepared_binding.is_some()
        }
        AttachOperation::Detach => {
            record.previous_binding.is_some() && record.prepared_binding.is_none()
        }
    } && record.managed_projection.is_some();
    if !shape_valid {
        return Err(AppError::new(
            "ATTACH_JOURNAL_INVALID",
            "attach journal operation shape is invalid",
        ));
    }
    if let Some(update) = &record.provider_update {
        update.validate(record)?;
    }
    let bytes = serde_json::to_vec(record)
        .map_err(|error| AppError::new("ATTACH_JOURNAL_INVALID", error.to_string()))?;
    if bytes.len() > AttachJournalRepository::MAX_RECORD_BYTES {
        return Err(AppError::new(
            "ATTACH_JOURNAL_RECORD_TOO_LARGE",
            "attach journal record exceeds 256 KiB",
        ));
    }
    let text = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
    if text.contains("lam_test_secret")
        || text.contains("bearer eyj")
        || text.contains("authorization: bearer")
        || text.contains("sk-never-store")
    {
        return Err(AppError::new(
            "ATTACH_JOURNAL_SENSITIVE_DATA",
            "attach journal contains forbidden sensitive data",
        ));
    }
    Ok(())
}

fn sort_records(records: &mut [AttachJournalRecord]) {
    records.sort_by(|a, b| {
        a.created_at_ms
            .cmp(&b.created_at_ms)
            .then_with(|| a.operation_id.cmp(&b.operation_id))
    });
}

pub trait GatewayBindingLifecycle: Send + Sync {
    fn prepare(
        &self,
        guard: &InstallationLockGuard,
        operation_id: &str,
        plan: &ProfileAttachPlan,
    ) -> Result<Option<String>>;
    fn revoke(&self, guard: &InstallationLockGuard, reference: &str) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionFault {
    CrashAfterPrepared,
    CrashAfterConfigCommitted,
    FailBindingCommit,
    DriftConfigBeforeBindingCommit,
    CrashAfterBindingCommitted,
    CrashBeforeCleanup,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachTransactionOutcome {
    pub operation_id: Option<String>,
    pub state: AttachJournalState,
    pub idempotent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryReport {
    pub recovered: usize,
    pub remaining: usize,
}

#[derive(Default)]
struct AttachCommitOptions {
    fault: Option<TransactionFault>,
    provider_update: Option<AttachedProviderUpdate>,
}

pub struct AttachTransactionCoordinator<G: GatewayBindingLifecycle> {
    lock: InstallationLock,
    provider_store: VersionedFileStore<ProviderCollection>,
    binding_store: VersionedFileStore<ProfileBindingCollection>,
    journal_store: VersionedFileStore<AttachJournalCollection>,
    gateway: Arc<G>,
    gateway_endpoint_version: u64,
}

pub fn recover_provider_transactions_on_startup<G: GatewayBindingLifecycle>(
    coordinator: &AttachTransactionCoordinator<G>,
    now_ms: u64,
    max_records: usize,
) -> Result<RecoveryReport> {
    coordinator.recover_pending(now_ms, max_records)
}

impl<G: GatewayBindingLifecycle> AttachTransactionCoordinator<G> {
    pub fn new(
        lock: InstallationLock,
        provider_store: VersionedFileStore<ProviderCollection>,
        binding_store: VersionedFileStore<ProfileBindingCollection>,
        journal_store: VersionedFileStore<AttachJournalCollection>,
        gateway: Arc<G>,
        gateway_endpoint_version: u64,
    ) -> Self {
        Self {
            lock,
            provider_store,
            binding_store,
            journal_store,
            gateway,
            gateway_endpoint_version,
        }
    }

    pub fn execute_attach(
        &self,
        registry: &mut DryRunRegistry,
        ticket: &str,
        plan: &ProfileAttachPlan,
        now_ms: u64,
        fault: Option<TransactionFault>,
    ) -> Result<AttachTransactionOutcome> {
        let guard = self.lock.acquire_exclusive()?;
        self.execute_attach_locked(
            &guard,
            registry,
            ticket,
            plan,
            now_ms,
            AttachCommitOptions {
                fault,
                provider_update: None,
            },
        )
    }

    fn execute_attach_locked(
        &self,
        guard: &InstallationLockGuard,
        registry: &mut DryRunRegistry,
        ticket: &str,
        plan: &ProfileAttachPlan,
        now_ms: u64,
        options: AttachCommitOptions,
    ) -> Result<AttachTransactionOutcome> {
        let AttachCommitOptions {
            fault,
            provider_update,
        } = options;
        registry.validate(ticket, &plan.fingerprint, &plan.fingerprint, now_ms)?;
        self.validate_attach_state(guard, plan, provider_update.as_ref())?;

        let bindings = self.binding_store.load_locked(guard)?;
        let previous = bindings
            .value
            .bindings
            .iter()
            .find(|item| item.profile_id == plan.profile_id)
            .cloned();
        let config_path = PathBuf::from(&plan.config_path);
        let source = fs::read_to_string(&config_path)?;
        let applied = if let Some(previous) = &previous {
            let old_projection =
                binding_to_config_projection(&previous.config_projection, &previous.provider_id);
            let unmanaged = detach_projection(&source, &old_projection)?;
            apply_projection(
                &unmanaged,
                &config_hash(unmanaged.as_bytes()),
                &plan.config_projection,
            )?
        } else {
            apply_projection(&source, &plan.source_config_hash, &plan.config_projection)?
        };
        let operation_id = Uuid::new_v4().to_string();
        let new_gateway_reference = if plan.route.route_kind == RouteKind::Gateway {
            self.gateway.prepare(guard, &operation_id, plan)?
        } else {
            None
        };
        if plan.route.route_kind == RouteKind::Gateway
            && new_gateway_reference.as_deref() != plan.gateway_binding_id.as_deref()
        {
            if let Some(reference) = new_gateway_reference.as_deref() {
                let _ = self.gateway.revoke(guard, reference);
            }
            return Err(AppError::new(
                "GATEWAY_BINDING_PLAN_MISMATCH",
                "prepared Gateway binding does not match the approved attach plan",
            ));
        }
        let prepared_binding = build_prepared_binding(
            plan,
            previous.as_ref(),
            &applied.projection,
            new_gateway_reference.clone(),
            now_ms,
        );
        let record = AttachJournalRecord {
            journal_version: if provider_update.is_some() { 2 } else { 1 },
            operation_id: operation_id.clone(),
            operation: if previous.is_some() {
                AttachOperation::Rebind
            } else {
                AttachOperation::Attach
            },
            state: AttachJournalState::Prepared,
            profile_id: plan.profile_id.clone(),
            provider_id: plan.route.provider_id.clone(),
            selected_model: plan.route.selected_model.clone(),
            route_kind: plan.route.route_kind,
            expected_provider_store_revision: plan.expected_provider_store_revision,
            expected_binding_store_revision: plan.expected_binding_store_revision,
            expected_binding_revision: plan.expected_binding_revision,
            config_path: plan.config_path.clone(),
            config_before_hash: plan.source_config_hash.clone(),
            config_intended_hash: applied.projection.applied_hash.clone(),
            managed_projection: Some(applied.projection.clone()),
            previous_binding: previous.clone(),
            prepared_binding: Some(prepared_binding.clone()),
            new_gateway_binding_ref: new_gateway_reference.clone(),
            superseded_gateway_binding_ref: previous
                .as_ref()
                .and_then(|item| item.gateway_binding_id.clone()),
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            last_error_code: None,
            provider_update: provider_update.clone(),
        };
        let journal_revision = match self.create_journal_locked(guard, record) {
            Ok(revision) => revision,
            Err(error) => {
                if let Some(reference) = &new_gateway_reference {
                    let _ = self.gateway.revoke(guard, reference);
                }
                return Err(error);
            }
        };
        registry.consume(ticket, &plan.fingerprint, &plan.fingerprint, now_ms)?;
        if fault == Some(TransactionFault::CrashAfterPrepared) {
            return Err(interrupted("after prepared journal"));
        }

        if let Some(update) = &provider_update {
            update.backup_config(&source)?;
            update.apply(
                &self.provider_store,
                guard,
                plan.expected_provider_store_revision - 1,
            )?;
        }
        let config_result = if previous.is_some() {
            replace_config_file_with_backup(
                &config_path,
                &plan.source_config_hash,
                &applied.contents,
            )
            .map(|_| ())
        } else {
            apply_projection_file(
                &config_path,
                &plan.source_config_hash,
                &plan.config_projection,
                None,
            )
            .map(|_| ())
        };
        if let Err(error) = config_result {
            if let Some(update) = &provider_update {
                update.reconcile(&self.provider_store, guard, false)?;
            }
            if let Some(reference) = &new_gateway_reference {
                let _ = self.gateway.revoke(guard, reference);
            }
            let _ = self.transition_journal_locked(
                guard,
                journal_revision,
                &operation_id,
                AttachJournalState::RolledBack,
                now_ms,
                Some(error.code.clone()),
            );
            return Err(error);
        }
        let journal_revision = self.transition_journal_locked(
            guard,
            journal_revision,
            &operation_id,
            AttachJournalState::ConfigCommitted,
            now_ms,
            None,
        )?;
        if fault == Some(TransactionFault::CrashAfterConfigCommitted) {
            return Err(interrupted("after config commit"));
        }
        if fault == Some(TransactionFault::FailBindingCommit) {
            replace_config_file(&config_path, &applied.projection.applied_hash, &source)?;
            if let Some(reference) = &new_gateway_reference {
                self.gateway.revoke(guard, reference)?;
            }
            if let Some(update) = &provider_update {
                update.reconcile(&self.provider_store, guard, false)?;
            }
            self.transition_journal_locked(
                guard,
                journal_revision,
                &operation_id,
                AttachJournalState::RolledBack,
                now_ms,
                Some("ATTACH_BINDING_COMMIT_FAILED".into()),
            )?;
            return Err(AppError::new(
                "ATTACH_BINDING_COMMIT_FAILED",
                "binding commit failed",
            ));
        }
        if fault == Some(TransactionFault::DriftConfigBeforeBindingCommit) {
            fs::write(&config_path, "# simulated external config edit\n")?;
        }
        if config_hash(&fs::read(&config_path)?) != applied.projection.applied_hash {
            if let Some(reference) = &new_gateway_reference {
                let _ = self.gateway.revoke(guard, reference);
            }
            self.transition_journal_locked(
                guard,
                journal_revision,
                &operation_id,
                AttachJournalState::ManualIntervention,
                now_ms,
                Some("CODEX_CONFIG_OWNERSHIP_CONFLICT".into()),
            )?;
            return Err(AppError::new(
                "CODEX_CONFIG_OWNERSHIP_CONFLICT",
                "config changed before binding commit",
            ));
        }

        let mut next_bindings = bindings.value;
        if let Some(current) = next_bindings
            .bindings
            .iter_mut()
            .find(|item| item.profile_id == plan.profile_id)
        {
            *current = prepared_binding;
        } else {
            next_bindings.bindings.push(prepared_binding);
        }
        next_bindings
            .bindings
            .sort_by(|a, b| a.profile_id.cmp(&b.profile_id));
        self.binding_store.compare_and_swap_locked(
            guard,
            plan.expected_binding_store_revision,
            &next_bindings,
            None,
        )?;
        let journal_revision = self.transition_journal_locked(
            guard,
            journal_revision,
            &operation_id,
            AttachJournalState::BindingCommitted,
            now_ms,
            None,
        )?;
        if fault == Some(TransactionFault::CrashAfterBindingCommitted) {
            return Err(interrupted("after binding commit"));
        }
        if fault == Some(TransactionFault::CrashBeforeCleanup) {
            return Err(interrupted("before cleanup"));
        }
        if let Some(reference) = previous
            .as_ref()
            .and_then(|item| item.gateway_binding_id.as_deref())
        {
            self.gateway.revoke(guard, reference)?;
        }
        self.transition_journal_locked(
            guard,
            journal_revision,
            &operation_id,
            AttachJournalState::Completed,
            now_ms,
            None,
        )?;
        Ok(AttachTransactionOutcome {
            operation_id: Some(operation_id),
            state: AttachJournalState::Completed,
            idempotent: false,
        })
    }

    pub fn execute_detach(
        &self,
        registry: &mut DryRunRegistry,
        ticket: &str,
        plan: &ProfileDetachPlan,
        now_ms: u64,
        fault: Option<TransactionFault>,
    ) -> Result<AttachTransactionOutcome> {
        let guard = self.lock.acquire_exclusive()?;
        registry.validate(ticket, &plan.fingerprint, &plan.fingerprint, now_ms)?;
        let bindings = self.binding_store.load_locked(&guard)?;
        if bindings.revision != plan.expected_binding_store_revision {
            return Err(stale_plan());
        }
        let Some(current) = bindings
            .value
            .bindings
            .iter()
            .find(|item| item.profile_id == plan.profile_id)
            .cloned()
        else {
            registry.consume(ticket, &plan.fingerprint, &plan.fingerprint, now_ms)?;
            return Ok(AttachTransactionOutcome {
                operation_id: None,
                state: AttachJournalState::Completed,
                idempotent: true,
            });
        };
        if current.revision != plan.expected_binding_revision || current != plan.binding {
            return Err(stale_plan());
        }
        let config_path = PathBuf::from(&current.config_projection.config_path);
        let source = fs::read_to_string(&config_path)?;
        if config_hash(source.as_bytes()) != plan.source_config_hash {
            return Err(stale_plan());
        }
        let projection =
            binding_to_config_projection(&current.config_projection, &current.provider_id);
        let detached = detach_projection(&source, &projection)?;
        let operation_id = Uuid::new_v4().to_string();
        let record = AttachJournalRecord {
            journal_version: 1,
            operation_id: operation_id.clone(),
            operation: AttachOperation::Detach,
            state: AttachJournalState::Prepared,
            profile_id: current.profile_id.clone(),
            provider_id: current.provider_id.clone(),
            selected_model: current.selected_model.clone(),
            route_kind: current.route_kind,
            expected_provider_store_revision: current.provider_revision,
            expected_binding_store_revision: bindings.revision,
            expected_binding_revision: Some(current.revision),
            config_path: current.config_projection.config_path.clone(),
            config_before_hash: plan.source_config_hash.clone(),
            config_intended_hash: config_hash(detached.as_bytes()),
            managed_projection: Some(projection),
            previous_binding: Some(current.clone()),
            prepared_binding: None,
            new_gateway_binding_ref: None,
            superseded_gateway_binding_ref: current.gateway_binding_id.clone(),
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            last_error_code: None,
            provider_update: None,
        };
        let journal_revision = self.create_journal_locked(&guard, record)?;
        registry.consume(ticket, &plan.fingerprint, &plan.fingerprint, now_ms)?;
        if fault == Some(TransactionFault::CrashAfterPrepared) {
            return Err(interrupted("after detach prepared journal"));
        }
        replace_config_file(&config_path, &plan.source_config_hash, &detached)?;
        let journal_revision = self.transition_journal_locked(
            &guard,
            journal_revision,
            &operation_id,
            AttachJournalState::ConfigCommitted,
            now_ms,
            None,
        )?;
        if fault == Some(TransactionFault::CrashAfterConfigCommitted) {
            return Err(interrupted("after detach config commit"));
        }
        if fault == Some(TransactionFault::FailBindingCommit) {
            let restored = reapply_managed_projection(
                &detached,
                &binding_to_config_projection(&current.config_projection, &current.provider_id),
            )?;
            replace_config_file(&config_path, &config_hash(detached.as_bytes()), &restored)?;
            self.transition_journal_locked(
                &guard,
                journal_revision,
                &operation_id,
                AttachJournalState::RolledBack,
                now_ms,
                Some("ATTACH_BINDING_COMMIT_FAILED".into()),
            )?;
            return Err(AppError::new(
                "ATTACH_BINDING_COMMIT_FAILED",
                "binding commit failed",
            ));
        }
        let mut next = bindings.value;
        next.bindings
            .retain(|item| item.profile_id != plan.profile_id);
        self.binding_store.compare_and_swap_locked(
            &guard,
            plan.expected_binding_store_revision,
            &next,
            None,
        )?;
        let journal_revision = self.transition_journal_locked(
            &guard,
            journal_revision,
            &operation_id,
            AttachJournalState::BindingCommitted,
            now_ms,
            None,
        )?;
        if fault == Some(TransactionFault::CrashAfterBindingCommitted)
            || fault == Some(TransactionFault::CrashBeforeCleanup)
        {
            return Err(interrupted("after detach binding commit"));
        }
        if let Some(reference) = current.gateway_binding_id.as_deref() {
            self.gateway.revoke(&guard, reference)?;
        }
        self.transition_journal_locked(
            &guard,
            journal_revision,
            &operation_id,
            AttachJournalState::Completed,
            now_ms,
            None,
        )?;
        Ok(AttachTransactionOutcome {
            operation_id: Some(operation_id),
            state: AttachJournalState::Completed,
            idempotent: false,
        })
    }

    fn validate_attach_state(
        &self,
        guard: &InstallationLockGuard,
        plan: &ProfileAttachPlan,
        update: Option<&AttachedProviderUpdate>,
    ) -> Result<()> {
        if !plan.blockers.is_empty() {
            return Err(AppError::new(
                "ATTACH_PLAN_BLOCKED",
                "attach plan contains readiness blockers",
            ));
        }
        let providers = self.provider_store.load_locked(guard)?;
        let expected_revision = plan
            .expected_provider_store_revision
            .checked_sub(u64::from(update.is_some()))
            .ok_or_else(stale_plan)?;
        let expected_provider = update
            .map(|update| &update.previous_provider)
            .unwrap_or(&plan.route.provider);
        if providers.revision != expected_revision
            || !providers
                .value
                .providers
                .iter()
                .any(|provider| provider == expected_provider)
        {
            return Err(stale_plan());
        }
        let bindings = self.binding_store.load_locked(guard)?;
        let revision = bindings
            .value
            .bindings
            .iter()
            .find(|item| item.profile_id == plan.profile_id)
            .map(|item| item.revision);
        if bindings.revision != plan.expected_binding_store_revision
            || revision != plan.expected_binding_revision
        {
            return Err(stale_plan());
        }
        if plan.gateway_endpoint_version.is_some()
            && plan.gateway_endpoint_version != Some(self.gateway_endpoint_version)
        {
            return Err(stale_plan());
        }
        let bytes = fs::read(&plan.config_path)?;
        if config_hash(&bytes) != plan.source_config_hash {
            return Err(stale_plan());
        }
        Ok(())
    }

    fn create_journal_locked(
        &self,
        guard: &InstallationLockGuard,
        record: AttachJournalRecord,
    ) -> Result<u64> {
        validate_record(&record)?;
        let mut snapshot = self.journal_store.load_locked(guard)?;
        validate_collection(&snapshot.value)?;
        if snapshot
            .value
            .records
            .iter()
            .any(|item| item.profile_id == record.profile_id && item.state.is_active())
        {
            return Err(AppError::new(
                "ATTACH_JOURNAL_PROFILE_ACTIVE",
                "profile already has an active attach operation",
            ));
        }
        snapshot.value.records.push(record);
        sort_records(&mut snapshot.value.records);
        Ok(self
            .journal_store
            .compare_and_swap_locked(guard, snapshot.revision, &snapshot.value, None)?
            .revision)
    }

    fn transition_journal_locked(
        &self,
        guard: &InstallationLockGuard,
        expected_revision: u64,
        operation_id: &str,
        state: AttachJournalState,
        now_ms: u64,
        error: Option<String>,
    ) -> Result<u64> {
        let mut snapshot = self.journal_store.load_locked(guard)?;
        if snapshot.revision != expected_revision {
            return Err(AppError::new(
                "ATTACH_JOURNAL_CONFLICT",
                "attach journal revision changed",
            ));
        }
        let record = snapshot
            .value
            .records
            .iter_mut()
            .find(|item| item.operation_id == operation_id)
            .ok_or_else(|| AppError::new("ATTACH_JOURNAL_NOT_FOUND", operation_id))?;
        if !valid_transition(record.state, state) {
            return Err(AppError::new(
                "ATTACH_JOURNAL_TRANSITION_INVALID",
                "attach journal state transition is invalid",
            ));
        }
        record.state = state;
        record.updated_at_ms = now_ms;
        record.last_error_code = error;
        Ok(self
            .journal_store
            .compare_and_swap_locked(guard, expected_revision, &snapshot.value, None)?
            .revision)
    }

    pub fn recover_pending(&self, now_ms: u64, max_records: usize) -> Result<RecoveryReport> {
        if max_records == 0 {
            return Err(AppError::new(
                "ATTACH_RECOVERY_BOUND_INVALID",
                "recovery bound must be positive",
            ));
        }
        let guard = self.lock.acquire_exclusive()?;
        let snapshot = self.journal_store.load_locked(&guard)?;
        validate_collection(&snapshot.value)?;
        let active = snapshot
            .value
            .records
            .iter()
            .filter(|record| record.state.is_active())
            .take(max_records)
            .cloned()
            .collect::<Vec<_>>();
        let mut recovered = 0;
        for record in active {
            self.recover_record(&guard, &record, now_ms)?;
            recovered += 1;
        }
        let remaining = self
            .journal_store
            .load_locked(&guard)?
            .value
            .records
            .iter()
            .filter(|record| record.state.is_active())
            .count();
        Ok(RecoveryReport {
            recovered,
            remaining,
        })
    }

    fn recover_record(
        &self,
        guard: &InstallationLockGuard,
        record: &AttachJournalRecord,
        now_ms: u64,
    ) -> Result<()> {
        let bindings = self.binding_store.load_locked(guard)?;
        let current_binding = bindings
            .value
            .bindings
            .iter()
            .find(|item| item.profile_id == record.profile_id);
        let binding_committed = match record.operation {
            AttachOperation::Attach | AttachOperation::Rebind => {
                record.prepared_binding.is_some()
                    && current_binding == record.prepared_binding.as_ref()
            }
            AttachOperation::Detach => current_binding.is_none(),
        };
        let binding_precommit = current_binding == record.previous_binding.as_ref();
        if !binding_committed && !binding_precommit {
            return self.mark_manual(guard, record, now_ms, "ATTACH_RECOVERY_BINDING_CONFLICT");
        }
        let path = Path::new(&record.config_path);
        let current_hash = config_hash(&fs::read(path)?);
        if binding_committed {
            if current_hash != record.config_intended_hash {
                return self.mark_manual(guard, record, now_ms, "ATTACH_RECOVERY_CONFIG_CONFLICT");
            }
            if let Some(update) = &record.provider_update {
                update.reconcile(&self.provider_store, guard, true)?;
            }
            let mut revision = self.journal_store.load_locked(guard)?.revision;
            let mut state = record.state;
            if state == AttachJournalState::Prepared {
                revision = self.transition_journal_locked(
                    guard,
                    revision,
                    &record.operation_id,
                    AttachJournalState::ConfigCommitted,
                    now_ms,
                    None,
                )?;
                state = AttachJournalState::ConfigCommitted;
            }
            if state == AttachJournalState::ConfigCommitted {
                revision = self.transition_journal_locked(
                    guard,
                    revision,
                    &record.operation_id,
                    AttachJournalState::BindingCommitted,
                    now_ms,
                    None,
                )?;
                state = AttachJournalState::BindingCommitted;
            }
            if state != AttachJournalState::BindingCommitted {
                return self.mark_manual(guard, record, now_ms, "ATTACH_RECOVERY_STATE_CONFLICT");
            }
            if let Some(reference) = record.superseded_gateway_binding_ref.as_deref() {
                self.gateway.revoke(guard, reference)?;
            }
            self.transition_journal_locked(
                guard,
                revision,
                &record.operation_id,
                AttachJournalState::Completed,
                now_ms,
                None,
            )?;
            return Ok(());
        }

        if record.state == AttachJournalState::BindingCommitted {
            return self.mark_manual(guard, record, now_ms, "ATTACH_RECOVERY_BINDING_CONFLICT");
        }
        if current_hash == record.config_intended_hash {
            self.rollback_record_config(record)?;
        } else if current_hash != record.config_before_hash {
            return self.mark_manual(guard, record, now_ms, "ATTACH_RECOVERY_CONFIG_CONFLICT");
        }
        if let Some(reference) = record.new_gateway_binding_ref.as_deref() {
            self.gateway.revoke(guard, reference)?;
        }
        if let Some(update) = &record.provider_update {
            update.reconcile(&self.provider_store, guard, false)?;
        }
        let revision = self.journal_store.load_locked(guard)?.revision;
        self.transition_journal_locked(
            guard,
            revision,
            &record.operation_id,
            AttachJournalState::RolledBack,
            now_ms,
            record.last_error_code.clone(),
        )?;
        Ok(())
    }

    fn rollback_record_config(&self, record: &AttachJournalRecord) -> Result<()> {
        let path = Path::new(&record.config_path);
        if let Some(update) = &record.provider_update {
            return update.restore_config(
                path,
                &record.config_intended_hash,
                &record.config_before_hash,
            );
        }
        let source = fs::read_to_string(path)?;
        let projection = record.managed_projection.as_ref().ok_or_else(|| {
            AppError::new(
                "ATTACH_RECOVERY_JOURNAL_INVALID",
                "journal is missing managed projection",
            )
        })?;
        let restored = match record.operation {
            AttachOperation::Attach | AttachOperation::Rebind => {
                let unmanaged = detach_projection(&source, projection)?;
                if let Some(previous) = &record.previous_binding {
                    reapply_managed_projection(
                        &unmanaged,
                        &binding_to_config_projection(
                            &previous.config_projection,
                            &previous.provider_id,
                        ),
                    )?
                } else {
                    unmanaged
                }
            }
            AttachOperation::Detach => {
                let previous = record.previous_binding.as_ref().ok_or_else(|| {
                    AppError::new(
                        "ATTACH_RECOVERY_JOURNAL_INVALID",
                        "detach journal is missing previous binding",
                    )
                })?;
                reapply_managed_projection(
                    &source,
                    &binding_to_config_projection(
                        &previous.config_projection,
                        &previous.provider_id,
                    ),
                )?
            }
        };
        replace_config_file(path, &record.config_intended_hash, &restored)
    }

    fn mark_manual(
        &self,
        guard: &InstallationLockGuard,
        record: &AttachJournalRecord,
        now_ms: u64,
        reason: &str,
    ) -> Result<()> {
        let revision = self.journal_store.load_locked(guard)?.revision;
        self.transition_journal_locked(
            guard,
            revision,
            &record.operation_id,
            AttachJournalState::ManualIntervention,
            now_ms,
            Some(reason.into()),
        )?;
        Err(AppError::new(
            "ATTACH_RECOVERY_OWNERSHIP_CONFLICT",
            "recovery found unknown config or binding ownership",
        ))
    }
}

fn build_prepared_binding(
    plan: &ProfileAttachPlan,
    previous: Option<&ProfileProviderBinding>,
    projection: &ConfigManagedProjection,
    gateway_reference: Option<String>,
    now_ms: u64,
) -> ProfileProviderBinding {
    let mut managed = config_to_binding_projection(plan.config_path.clone(), projection);
    if let Some(previous) = previous {
        managed.before_hash = previous.config_projection.before_hash.clone();
        managed.previous_values = previous.config_projection.previous_values.clone();
        managed.provider_table_created_by_lam =
            previous.config_projection.provider_table_created_by_lam;
    }
    let provider_bytes = serde_json::to_vec(&plan.route.provider).expect("serializable provider");
    ProfileProviderBinding {
        profile_id: plan.profile_id.clone(),
        provider_id: plan.route.provider_id.clone(),
        selected_model: plan.route.selected_model.clone(),
        route_kind: plan.route.route_kind,
        gateway_binding_id: gateway_reference,
        provider_revision: plan.expected_provider_store_revision,
        provider_fingerprint: hex::encode(Sha256::digest(provider_bytes)),
        config_projection: managed,
        revision: previous.map_or(1, |item| item.revision + 1),
        created_at: previous
            .map(|item| item.created_at.clone())
            .unwrap_or_else(|| now_ms.to_string()),
        updated_at: now_ms.to_string(),
    }
}

fn config_to_binding_projection(
    path: String,
    projection: &ConfigManagedProjection,
) -> ManagedConfigProjection {
    ManagedConfigProjection {
        config_path: path,
        ownership: ProjectionOwnership::Managed,
        before_hash: projection.before_hash.clone(),
        applied_hash: projection.applied_hash.clone(),
        previous_values: projection.previous_values.clone(),
        managed_values: projection.managed_values.clone(),
        provider_table_created_by_lam: projection.provider_table_created_by_lam,
    }
}

fn binding_to_config_projection(
    projection: &ManagedConfigProjection,
    provider_id: &str,
) -> ConfigManagedProjection {
    ConfigManagedProjection {
        before_hash: projection.before_hash.clone(),
        applied_hash: projection.applied_hash.clone(),
        provider_id: provider_id.to_string(),
        previous_values: projection.previous_values.clone(),
        managed_values: projection.managed_values.clone(),
        provider_table_created_by_lam: projection.provider_table_created_by_lam,
    }
}

fn stale_plan() -> AppError {
    AppError::new("ATTACH_PLAN_STALE", "execution state changed after dry-run")
}

fn interrupted(point: &str) -> AppError {
    AppError::new(
        "ATTACH_TRANSACTION_INTERRUPTED",
        format!("transaction interrupted {point}"),
    )
}
