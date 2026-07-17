use crate::services::error::{AppError, Result};
use crate::services::provider_attach_transaction::GatewayBindingLifecycle;
use crate::services::provider_binding::RouteKind;
use crate::services::provider_credentials::SecretValue;
use crate::services::provider_keychain::{
    KeychainBackend, KeychainCredentialReference, KeychainCredentialService,
};
use crate::services::provider_planner::ProfileAttachPlan;
use crate::services::provider_v2::ProviderProfileV2;
use crate::services::provider_v2::ProviderProtocol;
use crate::services::storage::{InstallationLockGuard, StoreSnapshot, VersionedFileStore};
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const GATEWAY_BINDING_SCHEMA_REVISION: u32 = 1;
const TOKEN_PREFIX: &str = "lam_gw_";
const TOKEN_RANDOM_BYTES: usize = 32;

pub fn binding_requires_gateway(binding: &GatewayBinding) -> bool {
    binding.revoked_at.is_none() && binding.provider.protocol == ProviderProtocol::ChatCompletions
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GatewayBindingCollection {
    pub bindings: Vec<GatewayBinding>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GatewayBinding {
    pub schema_revision: u32,
    pub binding_id: String,
    pub profile_id: String,
    pub provider_id: String,
    pub selected_model: String,
    pub provider_revision: u64,
    pub provider: ProviderProfileV2,
    pub token_hash: String,
    pub credential_reference: KeychainCredentialReference,
    pub generation: u64,
    pub prepared_operation_id: Option<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
    pub revocation_pending: bool,
}

#[derive(Debug, Clone)]
pub struct GatewayTokenRequest {
    pub profile_id: String,
    pub provider: ProviderProfileV2,
    pub provider_revision: u64,
    pub selected_model: String,
    pub expires_at: Option<String>,
    pub now: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayBindingProvision {
    pub binding_id: String,
    pub credential_reference: KeychainCredentialReference,
    pub generation: u64,
    pub store_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayRevocationOutcome {
    pub store_revision: u64,
    pub revocation_pending: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayBindingSnapshot {
    pub binding_id: String,
    pub profile_id: String,
    pub provider_id: String,
    pub selected_model: String,
    pub provider_revision: u64,
    pub provider: ProviderProfileV2,
    pub credential_reference: KeychainCredentialReference,
    pub generation: u64,
}

#[derive(Clone)]
pub struct GatewayBindingService<B: KeychainBackend> {
    store: VersionedFileStore<GatewayBindingCollection>,
    credentials: KeychainCredentialService<B>,
}

impl<B: KeychainBackend> GatewayBindingService<B> {
    pub fn new(
        store: VersionedFileStore<GatewayBindingCollection>,
        credentials: KeychainCredentialService<B>,
    ) -> Self {
        Self { store, credentials }
    }

    pub fn load(&self) -> Result<StoreSnapshot<GatewayBindingCollection>> {
        self.store.load_or_default()
    }

    pub fn provision(
        &self,
        expected_revision: u64,
        request: GatewayTokenRequest,
    ) -> Result<GatewayBindingProvision> {
        validate_request(&request)?;
        let mut snapshot = self.load()?;
        if snapshot
            .value
            .bindings
            .iter()
            .any(|binding| binding.profile_id == request.profile_id && binding.revoked_at.is_none())
        {
            return Err(AppError::new(
                "GATEWAY_BINDING_PROFILE_EXISTS",
                "profile already has an active Gateway binding",
            ));
        }
        if snapshot.revision != expected_revision {
            return Err(revision_conflict());
        }
        let binding_id = Uuid::new_v4().to_string();
        let generation = snapshot
            .value
            .bindings
            .iter()
            .filter(|binding| binding.profile_id == request.profile_id)
            .map(|binding| binding.generation)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| {
                AppError::new(
                    "GATEWAY_BINDING_GENERATION_OVERFLOW",
                    "binding generation overflow",
                )
            })?;
        let (token, credential_reference) = generate_token(&binding_id, generation)?;
        self.credentials.write_exact(
            &credential_reference,
            SecretValue::from_sensitive(token.clone()),
        )?;
        snapshot.value.bindings.push(GatewayBinding {
            schema_revision: GATEWAY_BINDING_SCHEMA_REVISION,
            binding_id: binding_id.clone(),
            profile_id: request.profile_id,
            provider_id: request.provider.id.clone(),
            selected_model: request.selected_model,
            provider_revision: request.provider_revision,
            provider: request.provider,
            token_hash: token_hash(&token),
            credential_reference: credential_reference.clone(),
            generation,
            prepared_operation_id: None,
            created_at: request.now,
            expires_at: request.expires_at,
            revoked_at: None,
            revocation_pending: false,
        });
        sort_bindings(&mut snapshot.value.bindings);
        let committed = match self
            .store
            .compare_and_swap(expected_revision, &snapshot.value)
        {
            Ok(committed) => committed,
            Err(error) => {
                let _ = self.credentials.revoke(&credential_reference);
                return Err(error);
            }
        };
        Ok(GatewayBindingProvision {
            binding_id,
            credential_reference,
            generation,
            store_revision: committed.revision,
        })
    }

    pub fn rotate(
        &self,
        expected_revision: u64,
        binding_id: &str,
        provider: ProviderProfileV2,
        provider_revision: u64,
        selected_model: &str,
        now: &str,
    ) -> Result<GatewayBindingProvision> {
        validate_timestamp(now)?;
        validate_selected_model(&provider, selected_model)?;
        let mut snapshot = self.load()?;
        if snapshot.revision != expected_revision {
            return Err(revision_conflict());
        }
        let old_index = snapshot
            .value
            .bindings
            .iter()
            .position(|binding| binding.binding_id == binding_id)
            .ok_or_else(binding_not_found)?;
        if snapshot.value.bindings[old_index].revoked_at.is_some() {
            return Err(binding_revoked());
        }
        let old = snapshot.value.bindings[old_index].clone();
        let generation = old.generation.checked_add(1).ok_or_else(|| {
            AppError::new(
                "GATEWAY_BINDING_GENERATION_OVERFLOW",
                "binding generation overflow",
            )
        })?;
        let new_id = Uuid::new_v4().to_string();
        let (token, credential_reference) = generate_token(&new_id, generation)?;
        self.credentials.write_exact(
            &credential_reference,
            SecretValue::from_sensitive(token.clone()),
        )?;
        snapshot.value.bindings[old_index].revoked_at = Some(now.into());
        snapshot.value.bindings.push(GatewayBinding {
            schema_revision: GATEWAY_BINDING_SCHEMA_REVISION,
            binding_id: new_id.clone(),
            profile_id: old.profile_id,
            provider_id: provider.id.clone(),
            selected_model: selected_model.into(),
            provider_revision,
            provider,
            token_hash: token_hash(&token),
            credential_reference: credential_reference.clone(),
            generation,
            prepared_operation_id: None,
            created_at: now.into(),
            expires_at: old.expires_at,
            revoked_at: None,
            revocation_pending: false,
        });
        sort_bindings(&mut snapshot.value.bindings);
        let committed = match self
            .store
            .compare_and_swap(expected_revision, &snapshot.value)
        {
            Ok(committed) => committed,
            Err(error) => {
                let _ = self.credentials.revoke(&credential_reference);
                return Err(error);
            }
        };
        let final_revision = if self.credentials.revoke(&old.credential_reference).is_err() {
            self.mark_revocation_pending(committed.revision, &old.binding_id)?
        } else {
            committed.revision
        };
        Ok(GatewayBindingProvision {
            binding_id: new_id,
            credential_reference,
            generation,
            store_revision: final_revision,
        })
    }

    pub fn revoke(
        &self,
        expected_revision: u64,
        binding_id: &str,
        now: &str,
    ) -> Result<GatewayRevocationOutcome> {
        validate_timestamp(now)?;
        let mut snapshot = self.load()?;
        if snapshot.revision != expected_revision {
            return Err(revision_conflict());
        }
        let binding = snapshot
            .value
            .bindings
            .iter_mut()
            .find(|binding| binding.binding_id == binding_id)
            .ok_or_else(binding_not_found)?;
        if binding.revoked_at.is_some() {
            return Ok(GatewayRevocationOutcome {
                store_revision: snapshot.revision,
                revocation_pending: binding.revocation_pending,
            });
        }
        binding.revoked_at = Some(now.into());
        let reference = binding.credential_reference.clone();
        let committed = self
            .store
            .compare_and_swap(expected_revision, &snapshot.value)?;
        let (store_revision, revocation_pending) = if self.credentials.revoke(&reference).is_err() {
            (
                self.mark_revocation_pending(committed.revision, binding_id)?,
                true,
            )
        } else {
            (committed.revision, false)
        };
        Ok(GatewayRevocationOutcome {
            store_revision,
            revocation_pending,
        })
    }

    pub fn prepare_candidate(
        &self,
        expected_revision: u64,
        operation_id: &str,
        request: GatewayTokenRequest,
    ) -> Result<GatewayBindingProvision> {
        validate_request(&request)?;
        if operation_id.is_empty()
            || operation_id.len() > 128
            || operation_id.chars().any(char::is_control)
        {
            return Err(AppError::new(
                "GATEWAY_OPERATION_ID_INVALID",
                "Gateway operation identifier is invalid",
            ));
        }
        let mut snapshot = self.load()?;
        if let Some(existing) = snapshot
            .value
            .bindings
            .iter()
            .find(|binding| binding.prepared_operation_id.as_deref() == Some(operation_id))
        {
            if existing.profile_id != request.profile_id
                || existing.provider_id != request.provider.id
                || existing.selected_model != request.selected_model
            {
                return Err(AppError::new(
                    "GATEWAY_OPERATION_CONFLICT",
                    "Gateway operation was already prepared with different inputs",
                ));
            }
            return Ok(GatewayBindingProvision {
                binding_id: existing.binding_id.clone(),
                credential_reference: existing.credential_reference.clone(),
                generation: existing.generation,
                store_revision: snapshot.revision,
            });
        }
        if snapshot.revision != expected_revision {
            return Err(revision_conflict());
        }
        let generation = snapshot
            .value
            .bindings
            .iter()
            .filter(|binding| binding.profile_id == request.profile_id)
            .map(|binding| binding.generation)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| {
                AppError::new(
                    "GATEWAY_BINDING_GENERATION_OVERFLOW",
                    "binding generation overflow",
                )
            })?;
        let binding_id = Uuid::new_v4().to_string();
        let (token, credential_reference) = generate_token(&binding_id, generation)?;
        self.credentials.write_exact(
            &credential_reference,
            SecretValue::from_sensitive(token.clone()),
        )?;
        snapshot.value.bindings.push(GatewayBinding {
            schema_revision: GATEWAY_BINDING_SCHEMA_REVISION,
            binding_id: binding_id.clone(),
            profile_id: request.profile_id,
            provider_id: request.provider.id.clone(),
            selected_model: request.selected_model,
            provider_revision: request.provider_revision,
            provider: request.provider,
            token_hash: token_hash(&token),
            credential_reference: credential_reference.clone(),
            generation,
            prepared_operation_id: Some(operation_id.into()),
            created_at: request.now,
            expires_at: request.expires_at,
            revoked_at: None,
            revocation_pending: false,
        });
        sort_bindings(&mut snapshot.value.bindings);
        let committed = match self
            .store
            .compare_and_swap(expected_revision, &snapshot.value)
        {
            Ok(committed) => committed,
            Err(error) => {
                let _ = self.credentials.revoke(&credential_reference);
                return Err(error);
            }
        };
        Ok(GatewayBindingProvision {
            binding_id,
            credential_reference,
            generation,
            store_revision: committed.revision,
        })
    }

    pub fn token_for_helper(&self, profile_id: &str, binding_id: &str) -> Result<String> {
        let snapshot = self.load()?;
        let binding = snapshot
            .value
            .bindings
            .iter()
            .find(|binding| binding.binding_id == binding_id)
            .ok_or_else(binding_not_found)?;
        if binding.profile_id != profile_id {
            return Err(AppError::new(
                "GATEWAY_AUTH_PROFILE_MISMATCH",
                "Gateway binding does not belong to the requested profile",
            ));
        }
        if binding.revoked_at.is_some() || binding.revocation_pending {
            return Err(binding_revoked());
        }
        self.credentials
            .with_secret(&binding.credential_reference, str::to_owned)
    }

    pub fn authenticate(&self, authorization: &str, now: &str) -> Result<GatewayBindingSnapshot> {
        let now = parse_timestamp(now)?;
        let token = parse_bearer(authorization)?;
        let digest = token_hash_bytes(token);
        let snapshot = self.load()?;
        let matching = snapshot.value.bindings.iter().find(|binding| {
            decode_hash(&binding.token_hash)
                .is_some_and(|stored| constant_time_eq(&stored, &digest))
        });
        let Some(binding) = matching else {
            return Err(auth_invalid());
        };
        if binding.revoked_at.is_some() || binding.revocation_pending {
            return Err(auth_invalid());
        }
        if binding
            .expires_at
            .as_deref()
            .map(parse_timestamp)
            .transpose()?
            .is_some_and(|expires| now >= expires)
        {
            return Err(AppError::new(
                "GATEWAY_AUTH_EXPIRED",
                "Gateway bearer credential has expired",
            ));
        }
        Ok(binding.into())
    }

    pub fn authenticate_for_profile(
        &self,
        profile_id: &str,
        authorization: &str,
        now: &str,
    ) -> Result<GatewayBindingSnapshot> {
        let snapshot = self.authenticate(authorization, now)?;
        if snapshot.profile_id != profile_id {
            return Err(AppError::new(
                "GATEWAY_AUTH_PROFILE_MISMATCH",
                "Gateway bearer credential belongs to another profile",
            ));
        }
        Ok(snapshot)
    }

    fn mark_revocation_pending(&self, expected_revision: u64, binding_id: &str) -> Result<u64> {
        let mut snapshot = self.load()?;
        if snapshot.revision != expected_revision {
            return Err(revision_conflict());
        }
        let binding = snapshot
            .value
            .bindings
            .iter_mut()
            .find(|binding| binding.binding_id == binding_id)
            .ok_or_else(binding_not_found)?;
        binding.revocation_pending = true;
        Ok(self
            .store
            .compare_and_swap(expected_revision, &snapshot.value)?
            .revision)
    }

    fn prepare_candidate_locked(
        &self,
        guard: &InstallationLockGuard,
        operation_id: &str,
        planned_binding_id: &str,
        request: GatewayTokenRequest,
    ) -> Result<GatewayBindingProvision> {
        validate_request(&request)?;
        if operation_id.is_empty()
            || operation_id.len() > 128
            || operation_id.chars().any(char::is_control)
        {
            return Err(AppError::new(
                "GATEWAY_OPERATION_ID_INVALID",
                "Gateway operation identifier is invalid",
            ));
        }
        if uuid::Uuid::parse_str(planned_binding_id).is_err() {
            return Err(AppError::new(
                "GATEWAY_BINDING_ID_INVALID",
                "planned Gateway binding identifier is invalid",
            ));
        }
        let mut snapshot = self.store.load_locked(guard)?;
        if let Some(existing) = snapshot
            .value
            .bindings
            .iter()
            .find(|binding| binding.prepared_operation_id.as_deref() == Some(operation_id))
        {
            if existing.profile_id != request.profile_id
                || existing.provider_id != request.provider.id
                || existing.selected_model != request.selected_model
            {
                return Err(AppError::new(
                    "GATEWAY_OPERATION_CONFLICT",
                    "Gateway operation was already prepared with different inputs",
                ));
            }
            return Ok(GatewayBindingProvision {
                binding_id: existing.binding_id.clone(),
                credential_reference: existing.credential_reference.clone(),
                generation: existing.generation,
                store_revision: snapshot.revision,
            });
        }
        let generation = snapshot
            .value
            .bindings
            .iter()
            .filter(|binding| binding.profile_id == request.profile_id)
            .map(|binding| binding.generation)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| {
                AppError::new(
                    "GATEWAY_BINDING_GENERATION_OVERFLOW",
                    "binding generation overflow",
                )
            })?;
        if snapshot
            .value
            .bindings
            .iter()
            .any(|binding| binding.binding_id == planned_binding_id)
        {
            return Err(AppError::new(
                "GATEWAY_BINDING_ID_CONFLICT",
                "planned Gateway binding identifier already exists",
            ));
        }
        let binding_id = planned_binding_id.to_owned();
        let (token, credential_reference) = generate_token(&binding_id, generation)?;
        self.credentials.write_exact(
            &credential_reference,
            SecretValue::from_sensitive(token.clone()),
        )?;
        snapshot.value.bindings.push(GatewayBinding {
            schema_revision: GATEWAY_BINDING_SCHEMA_REVISION,
            binding_id: binding_id.clone(),
            profile_id: request.profile_id,
            provider_id: request.provider.id.clone(),
            selected_model: request.selected_model,
            provider_revision: request.provider_revision,
            provider: request.provider,
            token_hash: token_hash(&token),
            credential_reference: credential_reference.clone(),
            generation,
            prepared_operation_id: Some(operation_id.into()),
            created_at: request.now,
            expires_at: request.expires_at,
            revoked_at: None,
            revocation_pending: false,
        });
        sort_bindings(&mut snapshot.value.bindings);
        let committed = match self.store.compare_and_swap_locked(
            guard,
            snapshot.revision,
            &snapshot.value,
            None,
        ) {
            Ok(committed) => committed,
            Err(error) => {
                let _ = self.credentials.revoke(&credential_reference);
                return Err(error);
            }
        };
        Ok(GatewayBindingProvision {
            binding_id,
            credential_reference,
            generation,
            store_revision: committed.revision,
        })
    }

    fn revoke_locked(
        &self,
        guard: &InstallationLockGuard,
        binding_id: &str,
        now: &str,
    ) -> Result<()> {
        let mut snapshot = self.store.load_locked(guard)?;
        let binding = snapshot
            .value
            .bindings
            .iter_mut()
            .find(|binding| binding.binding_id == binding_id)
            .ok_or_else(binding_not_found)?;
        if binding.revoked_at.is_some() && !binding.revocation_pending {
            return Ok(());
        }
        binding.revoked_at.get_or_insert_with(|| now.into());
        let reference = binding.credential_reference.clone();
        let committed =
            self.store
                .compare_and_swap_locked(guard, snapshot.revision, &snapshot.value, None)?;
        if self.credentials.revoke(&reference).is_err() {
            let mut pending = self.store.load_locked(guard)?;
            let binding = pending
                .value
                .bindings
                .iter_mut()
                .find(|binding| binding.binding_id == binding_id)
                .ok_or_else(binding_not_found)?;
            binding.revocation_pending = true;
            self.store
                .compare_and_swap_locked(guard, committed.revision, &pending.value, None)?;
        }
        Ok(())
    }
}

impl<B: KeychainBackend> GatewayBindingLifecycle for GatewayBindingService<B> {
    fn prepare(
        &self,
        guard: &InstallationLockGuard,
        operation_id: &str,
        plan: &ProfileAttachPlan,
    ) -> Result<Option<String>> {
        if plan.route.route_kind == RouteKind::Direct {
            return Ok(None);
        }
        let provision = self.prepare_candidate_locked(
            guard,
            operation_id,
            plan.gateway_binding_id.as_deref().ok_or_else(|| {
                AppError::new(
                    "GATEWAY_BINDING_NOT_FOUND",
                    "attach plan has no prepared Gateway binding identifier",
                )
            })?,
            GatewayTokenRequest {
                profile_id: plan.profile_id.clone(),
                provider: plan.route.provider.clone(),
                provider_revision: plan.expected_provider_store_revision,
                selected_model: plan.route.selected_model.clone(),
                expires_at: None,
                now: Utc::now().to_rfc3339(),
            },
        )?;
        Ok(Some(provision.binding_id))
    }

    fn revoke(&self, guard: &InstallationLockGuard, reference: &str) -> Result<()> {
        self.revoke_locked(guard, reference, &Utc::now().to_rfc3339())
    }
}

impl From<&GatewayBinding> for GatewayBindingSnapshot {
    fn from(binding: &GatewayBinding) -> Self {
        Self {
            binding_id: binding.binding_id.clone(),
            profile_id: binding.profile_id.clone(),
            provider_id: binding.provider_id.clone(),
            selected_model: binding.selected_model.clone(),
            provider_revision: binding.provider_revision,
            provider: binding.provider.clone(),
            credential_reference: binding.credential_reference.clone(),
            generation: binding.generation,
        }
    }
}

fn validate_request(request: &GatewayTokenRequest) -> Result<()> {
    validate_timestamp(&request.now)?;
    if let Some(expires_at) = &request.expires_at {
        if parse_timestamp(expires_at)? <= parse_timestamp(&request.now)? {
            return Err(AppError::new(
                "GATEWAY_BINDING_EXPIRY_INVALID",
                "Gateway binding expiry must be after creation",
            ));
        }
    }
    if request.profile_id.trim().is_empty()
        || request.profile_id.len() > 128
        || request.profile_id.chars().any(char::is_control)
    {
        return Err(AppError::new(
            "GATEWAY_BINDING_PROFILE_INVALID",
            "Gateway profile identifier is invalid",
        ));
    }
    validate_selected_model(&request.provider, &request.selected_model)
}

fn validate_selected_model(provider: &ProviderProfileV2, selected_model: &str) -> Result<()> {
    if !provider
        .models
        .iter()
        .any(|model| model.id == selected_model)
    {
        return Err(AppError::new(
            "GATEWAY_BINDING_MODEL_INVALID",
            "selected model is not approved by the Provider snapshot",
        ));
    }
    Ok(())
}

fn generate_token(
    binding_id: &str,
    generation: u64,
) -> Result<(String, KeychainCredentialReference)> {
    let mut bytes = [0_u8; TOKEN_RANDOM_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let token = format!("{TOKEN_PREFIX}{}", hex::encode(bytes));
    let credential_id = format!("gateway-{}", binding_id);
    let reference = KeychainCredentialReference::new(&credential_id, generation)?;
    Ok((token, reference))
}

fn parse_bearer(value: &str) -> Result<&str> {
    let token = value.strip_prefix("Bearer ").ok_or_else(auth_invalid)?;
    if token.len() < TOKEN_PREFIX.len() + TOKEN_RANDOM_BYTES * 2
        || !token.starts_with(TOKEN_PREFIX)
        || token.chars().any(char::is_whitespace)
    {
        return Err(auth_invalid());
    }
    Ok(token)
}

fn token_hash(value: &str) -> String {
    hex::encode(token_hash_bytes(value))
}

fn token_hash_bytes(value: &str) -> [u8; 32] {
    Sha256::digest(value.as_bytes()).into()
}

fn decode_hash(value: &str) -> Option<[u8; 32]> {
    let bytes = hex::decode(value).ok()?;
    bytes.try_into().ok()
}

fn constant_time_eq(left: &[u8; 32], right: &[u8; 32]) -> bool {
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn validate_timestamp(value: &str) -> Result<()> {
    parse_timestamp(value).map(|_| ())
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| {
            AppError::new(
                "GATEWAY_BINDING_TIMESTAMP_INVALID",
                "Gateway binding timestamp must be RFC 3339",
            )
        })
}

fn sort_bindings(bindings: &mut [GatewayBinding]) {
    bindings.sort_by(|left, right| {
        left.profile_id
            .cmp(&right.profile_id)
            .then_with(|| left.generation.cmp(&right.generation))
            .then_with(|| left.binding_id.cmp(&right.binding_id))
    });
}

fn revision_conflict() -> AppError {
    AppError::new(
        "STORE_REVISION_CONFLICT",
        "Gateway binding store revision changed",
    )
}

fn binding_not_found() -> AppError {
    AppError::new("GATEWAY_BINDING_NOT_FOUND", "Gateway binding was not found")
}

fn binding_revoked() -> AppError {
    AppError::new("GATEWAY_BINDING_REVOKED", "Gateway binding is revoked")
}

fn auth_invalid() -> AppError {
    AppError::new(
        "GATEWAY_AUTH_INVALID",
        "Gateway bearer credential is missing or invalid",
    )
}
