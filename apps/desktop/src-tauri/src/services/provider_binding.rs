use super::error::{AppError, Result};
use super::storage::{StoreSnapshot, VersionedFileStore};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteKind {
    Direct,
    Gateway,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionOwnership {
    Managed,
    Adopted,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedConfigProjection {
    pub config_path: String,
    pub ownership: ProjectionOwnership,
    pub before_hash: String,
    pub applied_hash: String,
    pub previous_values: BTreeMap<String, Option<String>>,
    pub managed_values: BTreeMap<String, String>,
    pub provider_table_created_by_lam: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileProviderBinding {
    pub profile_id: String,
    pub provider_id: String,
    pub selected_model: String,
    pub route_kind: RouteKind,
    pub gateway_binding_id: Option<String>,
    pub provider_revision: u64,
    pub provider_fingerprint: String,
    pub config_projection: ManagedConfigProjection,
    pub revision: u64,
    pub created_at: String,
    pub updated_at: String,
}

impl ProfileProviderBinding {
    pub fn new_for_test(
        profile_id: &str,
        provider_id: &str,
        model: &str,
        projection: ManagedConfigProjection,
        provider_revision: u64,
        fingerprint: &str,
    ) -> Self {
        Self {
            profile_id: profile_id.into(),
            provider_id: provider_id.into(),
            selected_model: model.into(),
            route_kind: RouteKind::Direct,
            gateway_binding_id: None,
            provider_revision,
            provider_fingerprint: fingerprint.into(),
            config_projection: projection,
            revision: 1,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProfileBindingCollection {
    pub bindings: Vec<ProfileProviderBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingState {
    Adopted,
    Managed,
    Drifted { managed_keys: Vec<String> },
    StaleProvider,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingConfigBinding {
    pub provider_id: String,
    pub selected_model: String,
    pub config_path: String,
    pub config_hash: String,
    pub auth_supported: bool,
    pub manageable: bool,
    pub ambiguous: bool,
}

#[derive(Clone)]
pub struct ProfileBindingRepository {
    store: VersionedFileStore<ProfileBindingCollection>,
}

impl ProfileBindingRepository {
    pub fn new(store: VersionedFileStore<ProfileBindingCollection>) -> Self {
        Self { store }
    }
    pub fn load(&self) -> Result<StoreSnapshot<ProfileBindingCollection>> {
        self.store.load_or_default()
    }

    pub fn adopt(
        &self,
        expected_store_revision: u64,
        profile_id: &str,
        observation: ExistingConfigBinding,
        provider_revision: u64,
        fingerprint: &str,
        now: &str,
    ) -> Result<StoreSnapshot<ProfileBindingCollection>> {
        validate_observation(&observation)?;
        let mut snapshot = self.load()?;
        if let Some(existing) = snapshot
            .value
            .bindings
            .iter()
            .find(|item| item.profile_id == profile_id)
        {
            if existing.provider_id == observation.provider_id
                && existing.selected_model == observation.selected_model
                && existing.config_projection.ownership == ProjectionOwnership::Adopted
                && existing.config_projection.applied_hash == observation.config_hash
            {
                return Ok(snapshot);
            }
            return Err(binding_conflict(0, existing.revision));
        }
        snapshot.value.bindings.push(ProfileProviderBinding {
            profile_id: profile_id.into(),
            provider_id: observation.provider_id,
            selected_model: observation.selected_model,
            route_kind: RouteKind::Direct,
            gateway_binding_id: None,
            provider_revision,
            provider_fingerprint: fingerprint.into(),
            config_projection: ManagedConfigProjection {
                config_path: observation.config_path,
                ownership: ProjectionOwnership::Adopted,
                before_hash: observation.config_hash.clone(),
                applied_hash: observation.config_hash,
                previous_values: BTreeMap::new(),
                managed_values: BTreeMap::new(),
                provider_table_created_by_lam: false,
            },
            revision: 1,
            created_at: now.into(),
            updated_at: now.into(),
        });
        sort_bindings(&mut snapshot.value.bindings);
        self.store
            .compare_and_swap(expected_store_revision, &snapshot.value)
    }

    pub fn used_by(&self, provider_id: &str) -> Result<Vec<String>> {
        let mut ids = self
            .load()?
            .value
            .bindings
            .into_iter()
            .filter(|item| item.provider_id == provider_id)
            .map(|item| item.profile_id)
            .collect::<Vec<_>>();
        ids.sort();
        Ok(ids)
    }

    pub fn rename_profile(
        &self,
        expected_store_revision: u64,
        from: &str,
        to: &str,
        expected_binding_revision: u64,
        now: &str,
    ) -> Result<StoreSnapshot<ProfileBindingCollection>> {
        let mut snapshot = self.load()?;
        if snapshot
            .value
            .bindings
            .iter()
            .any(|item| item.profile_id == to)
        {
            return Err(AppError::new("PROFILE_BINDING_EXISTS", to));
        }
        let binding = snapshot
            .value
            .bindings
            .iter_mut()
            .find(|item| item.profile_id == from)
            .ok_or_else(|| AppError::new("PROFILE_BINDING_NOT_FOUND", from))?;
        if binding.revision != expected_binding_revision {
            return Err(binding_conflict(
                expected_binding_revision,
                binding.revision,
            ));
        }
        binding.profile_id = to.into();
        binding.revision += 1;
        binding.updated_at = now.into();
        sort_bindings(&mut snapshot.value.bindings);
        self.store
            .compare_and_swap(expected_store_revision, &snapshot.value)
    }

    pub fn detach(
        &self,
        expected_store_revision: u64,
        profile_id: &str,
        expected_binding_revision: u64,
    ) -> Result<StoreSnapshot<ProfileBindingCollection>> {
        let mut snapshot = self.load()?;
        let binding = snapshot
            .value
            .bindings
            .iter()
            .find(|item| item.profile_id == profile_id)
            .ok_or_else(|| AppError::new("PROFILE_BINDING_NOT_FOUND", profile_id))?;
        if binding.revision != expected_binding_revision {
            return Err(binding_conflict(
                expected_binding_revision,
                binding.revision,
            ));
        }
        snapshot
            .value
            .bindings
            .retain(|item| item.profile_id != profile_id);
        self.store
            .compare_and_swap(expected_store_revision, &snapshot.value)
    }

    pub fn ensure_profile_can_delete(&self, profile_id: &str) -> Result<()> {
        if self
            .load()?
            .value
            .bindings
            .iter()
            .any(|item| item.profile_id == profile_id)
        {
            Err(AppError::new("PROFILE_HAS_PROVIDER_BINDING", profile_id))
        } else {
            Ok(())
        }
    }
}

pub fn reconcile_binding(
    binding: &ProfileProviderBinding,
    current_values: &BTreeMap<String, String>,
    provider_revision: u64,
    fingerprint: &str,
) -> BindingState {
    if binding.provider_revision != provider_revision || binding.provider_fingerprint != fingerprint
    {
        return BindingState::StaleProvider;
    }
    if binding.config_projection.ownership == ProjectionOwnership::Adopted {
        return BindingState::Adopted;
    }
    let mut drift = binding
        .config_projection
        .managed_values
        .iter()
        .filter(|(key, value)| current_values.get(*key) != Some(*value))
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    drift.sort();
    if drift.is_empty() {
        BindingState::Managed
    } else {
        BindingState::Drifted {
            managed_keys: drift,
        }
    }
}

fn validate_observation(value: &ExistingConfigBinding) -> Result<()> {
    if value.ambiguous {
        return Err(AppError::new(
            "PROFILE_CONFIG_AMBIGUOUS",
            "config ownership is ambiguous",
        ));
    }
    if !value.manageable {
        return Err(AppError::new(
            "PROFILE_CONFIG_UNMANAGEABLE",
            "config cannot be managed safely",
        ));
    }
    if !value.auth_supported {
        return Err(AppError::new(
            "PROFILE_CONFIG_AUTH_UNSUPPORTED",
            "config auth is unsupported",
        ));
    }
    if value.provider_id.trim().is_empty() {
        return Err(AppError::new(
            "PROVIDER_NOT_FOUND",
            "config Provider is unknown",
        ));
    }
    Ok(())
}

fn binding_conflict(expected: u64, actual: u64) -> AppError {
    AppError {
        code: "PROFILE_BINDING_CONFLICT".into(),
        message: format!("expected binding revision {expected}, found {actual}"),
        recoverable: true,
        details: Some(serde_json::json!({"expected": expected, "actual": actual})),
    }
}
fn sort_bindings(items: &mut [ProfileProviderBinding]) {
    items.sort_by(|a, b| a.profile_id.cmp(&b.profile_id));
}
