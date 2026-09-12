//! Provider and model-catalog changes committed with an account rebind.
use super::*;
use crate::services::gateway::catalog::{
    build_codex_model_catalog_with_defaults, CodexModelDefaultsCatalog, CODEX_MODEL_CATALOG_FILE,
};
use std::io::Write;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttachedProviderUpdate {
    pub previous_provider: ProviderProfileV2,
    next_provider: ProviderProfileV2,
    catalog_path: PathBuf,
    config_backup_path: PathBuf,
    previous_catalog: Option<String>,
    next_catalog: String,
}

impl AttachedProviderUpdate {
    pub(super) fn validate(&self, record: &AttachJournalRecord) -> Result<()> {
        let config = Path::new(&record.config_path);
        let backup_name = self
            .config_backup_path
            .file_name()
            .and_then(|name| name.to_str());
        if self.catalog_path != config.with_file_name(CODEX_MODEL_CATALOG_FILE)
            || self.config_backup_path.parent() != config.parent()
            || !backup_name.is_some_and(|name| name.starts_with("config.toml.backup.model-update-"))
            || self.previous_provider.id != record.provider_id
            || self.next_provider.id != record.provider_id
            || self.next_provider.default_model != record.selected_model
            || record.operation != AttachOperation::Rebind
        {
            return Err(AppError::new(
                "ATTACH_JOURNAL_INVALID",
                "model-update journal has inconsistent ownership",
            ));
        }
        Ok(())
    }

    fn prepare(previous: &ProviderProfileV2, plan: &ProfileAttachPlan) -> Result<Self> {
        if previous.id != plan.route.provider.id || plan.expected_binding_revision.is_none() {
            return Err(stale_plan());
        }
        let catalog_path = Path::new(&plan.config_path).with_file_name(CODEX_MODEL_CATALOG_FILE);
        let previous_catalog = read_catalog(&catalog_path)?;
        let catalog = build_codex_model_catalog_with_defaults(
            &plan.route.provider.models,
            &CodexModelDefaultsCatalog::builtin()?,
        )?;
        let next_catalog = format!(
            "{}\n",
            serde_json::to_string_pretty(&catalog).map_err(|_| AppError::new(
                "CODEX_MODEL_CATALOG_INVALID",
                "catalog serialization failed"
            ))?
        );
        Ok(Self {
            previous_provider: previous.clone(),
            next_provider: plan.route.provider.clone(),
            catalog_path,
            previous_catalog,
            next_catalog,
            config_backup_path: Path::new(&plan.config_path).with_file_name(format!(
                "config.toml.backup.model-update-{}",
                Uuid::new_v4()
            )),
        })
    }

    pub(super) fn backup_config(&self, source: &str) -> Result<()> {
        write_private_atomic(&self.config_backup_path, source)
    }

    pub(super) fn restore_config(
        &self,
        path: &Path,
        expected_hash: &str,
        original_hash: &str,
    ) -> Result<()> {
        let original = fs::read_to_string(&self.config_backup_path)?;
        if config_hash(original.as_bytes()) != original_hash {
            return Err(AppError::new(
                "ATTACH_RECOVERY_CONFIG_CONFLICT",
                "model-update config backup changed",
            ));
        }
        replace_config_file(path, expected_hash, &original)
    }

    pub(super) fn apply(
        &self,
        store: &VersionedFileStore<ProviderCollection>,
        guard: &InstallationLockGuard,
        revision: u64,
    ) -> Result<()> {
        let mut snapshot = store.load_locked(guard)?;
        if snapshot.revision != revision {
            return Err(stale_plan());
        }
        let provider = snapshot
            .value
            .providers
            .iter_mut()
            .find(|p| p.id == self.previous_provider.id)
            .ok_or_else(stale_plan)?;
        if *provider != self.previous_provider {
            return Err(stale_plan());
        }
        *provider = self.next_provider.clone();
        store.compare_and_swap_locked(guard, revision, &snapshot.value, None)?;
        self.reconcile_catalog(true)
    }

    pub(super) fn reconcile(
        &self,
        store: &VersionedFileStore<ProviderCollection>,
        guard: &InstallationLockGuard,
        committed: bool,
    ) -> Result<()> {
        let mut snapshot = store.load_locked(guard)?;
        let provider = snapshot
            .value
            .providers
            .iter_mut()
            .find(|p| p.id == self.previous_provider.id)
            .ok_or_else(stale_plan)?;
        if *provider != self.previous_provider && *provider != self.next_provider {
            return Err(AppError::new(
                "ATTACH_RECOVERY_PROVIDER_CONFLICT",
                "Provider changed during model-update recovery",
            ));
        }
        self.reconcile_catalog(committed)?;
        let target = if committed {
            &self.next_provider
        } else {
            &self.previous_provider
        };
        if provider != target {
            *provider = target.clone();
            store.compare_and_swap_locked(guard, snapshot.revision, &snapshot.value, None)?;
        }
        Ok(())
    }

    fn reconcile_catalog(&self, committed: bool) -> Result<()> {
        let current = read_catalog(&self.catalog_path)?;
        let next = Some(self.next_catalog.as_str());
        if current.as_deref() != self.previous_catalog.as_deref() && current.as_deref() != next {
            return Err(AppError::new(
                "ATTACH_RECOVERY_CATALOG_CONFLICT",
                "model catalog changed during recovery",
            ));
        }
        let target = if committed {
            next
        } else {
            self.previous_catalog.as_deref()
        };
        if current.as_deref() == target {
            return Ok(());
        }
        match target {
            Some(body) => write_private_atomic(&self.catalog_path, body),
            None => {
                fs::remove_file(&self.catalog_path)?;
                Ok(())
            }
        }
    }
}

impl<G: GatewayBindingLifecycle> AttachTransactionCoordinator<G> {
    pub fn execute_provider_update(
        &self,
        registry: &mut DryRunRegistry,
        ticket: &str,
        plan: &ProfileAttachPlan,
        previous: &ProviderProfileV2,
        now_ms: u64,
        fault: Option<TransactionFault>,
    ) -> Result<AttachTransactionOutcome> {
        let guard = self.lock.acquire_exclusive()?;
        let existing_operations = self.model_update_operation_ids(&guard)?;
        let update = AttachedProviderUpdate::prepare(previous, plan)?;
        let result = self.execute_attach_locked(
            &guard,
            registry,
            ticket,
            plan,
            now_ms,
            AttachCommitOptions {
                fault,
                provider_update: Some(update),
            },
        );
        let Err(error) = result else {
            return result;
        };
        if error.code == "ATTACH_TRANSACTION_INTERRUPTED" {
            return Err(error);
        }
        self.recover_failed_update(&guard, plan, &existing_operations, error)
    }

    fn model_update_operation_ids(
        &self,
        guard: &InstallationLockGuard,
    ) -> Result<BTreeSet<String>> {
        Ok(self
            .journal_store
            .load_locked(guard)?
            .value
            .records
            .into_iter()
            .map(|record| record.operation_id)
            .collect())
    }

    fn recover_failed_update(
        &self,
        guard: &InstallationLockGuard,
        plan: &ProfileAttachPlan,
        existing_operations: &BTreeSet<String>,
        error: AppError,
    ) -> Result<AttachTransactionOutcome> {
        let records = self.journal_store.load_locked(guard)?.value.records;
        let record = records.iter().rev().find(|r| {
            r.profile_id == plan.profile_id
                && !existing_operations.contains(&r.operation_id)
                && r.new_gateway_binding_ref == plan.gateway_binding_id
                && r.provider_update.is_some()
        });
        let Some(record) = record else {
            return Err(error);
        };
        self.recover_update_record(guard, record, &error.code)?;
        let final_state = self
            .journal_store
            .load_locked(guard)?
            .value
            .records
            .into_iter()
            .find(|r| r.operation_id == record.operation_id)
            .map(|r| r.state);
        if final_state == Some(AttachJournalState::Completed) {
            return Ok(AttachTransactionOutcome {
                operation_id: Some(record.operation_id.clone()),
                state: AttachJournalState::Completed,
                idempotent: false,
            });
        }
        Err(error)
    }
    fn recover_update_record(
        &self,
        guard: &InstallationLockGuard,
        record: &AttachJournalRecord,
        code: &str,
    ) -> Result<()> {
        if record.state == AttachJournalState::RolledBack {
            record.provider_update.as_ref().unwrap().reconcile(
                &self.provider_store,
                guard,
                false,
            )?;
        } else if record.state.is_active() {
            let mut failed = record.clone();
            failed.last_error_code = Some(code.into());
            self.recover_record(guard, &failed, record.updated_at_ms)?;
        }
        Ok(())
    }
}

fn read_catalog(path: &Path) -> Result<Option<String>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() || metadata.len() > 256 * 1024 {
        return Err(AppError::new(
            "CODEX_MODEL_CATALOG_INVALID",
            "catalog must be a regular file smaller than 256 KiB",
        ));
    }
    fs::read_to_string(path).map(Some).map_err(Into::into)
}

fn write_private_atomic(path: &Path, body: &str) -> Result<()> {
    let temp = path.with_file_name(format!(".models.{}.tmp", Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut options = fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(body.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        fs::File::open(path.parent().ok_or_else(stale_plan)?)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
