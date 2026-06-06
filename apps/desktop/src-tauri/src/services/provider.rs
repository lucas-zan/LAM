use super::account::{find_account, list_accounts, OperationPlan};
use super::error::{AppError, Result};
use super::types::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfile {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub wire_api: String,
    pub default_model: String,
    pub env_key: Option<String>,
    pub secret_storage: String,
    pub health: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreateProviderRequest {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub wire_api: String,
    pub default_model: String,
    pub env_key: Option<String>,
    pub secret: Option<SecretInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProviderRequest {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub wire_api: String,
    pub default_model: String,
    pub env_key: Option<String>,
    pub secret: Option<SecretInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SecretInput {
    Env { env_key: String },
    Keychain { secret: String },
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttachProviderRequest {
    pub profile_id: String,
    pub provider_id: String,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttachProviderResult {
    pub profile_id: String,
    pub provider_id: String,
    pub config_path: PathBuf,
    pub backup_path: PathBuf,
    pub operations: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn list_providers(home_root: &Path) -> Result<Vec<ProviderProfile>> {
    read_provider_store(home_root)
}

pub fn create_provider(home_root: &Path, req: &CreateProviderRequest) -> Result<ProviderProfile> {
    let id = validate_provider_id(&req.id)?;
    let mut providers = read_provider_store(home_root)?;
    if providers.iter().any(|provider| provider.id == id) {
        return Err(AppError::new("PROVIDER_ALREADY_EXISTS", id));
    }
    let provider = provider_from_create_request(id, req)?;
    persist_secret(home_root, &provider.id, req.secret.as_ref())?;
    providers.push(provider.clone());
    providers.sort_by(|a, b| a.id.cmp(&b.id));
    write_provider_store(home_root, &providers)?;
    Ok(provider)
}

pub fn update_provider(home_root: &Path, req: &UpdateProviderRequest) -> Result<ProviderProfile> {
    let id = validate_provider_id(&req.id)?;
    let mut providers = read_provider_store(home_root)?;
    let pos = providers
        .iter()
        .position(|provider| provider.id == id)
        .ok_or_else(|| AppError::new("PROVIDER_NOT_FOUND", &id))?;
    let create = CreateProviderRequest {
        id: id.clone(),
        name: req.name.clone(),
        base_url: req.base_url.clone(),
        wire_api: req.wire_api.clone(),
        default_model: req.default_model.clone(),
        env_key: req.env_key.clone(),
        secret: req.secret.clone(),
    };
    let provider = provider_from_create_request(id, &create)?;
    persist_secret(home_root, &provider.id, req.secret.as_ref())?;
    providers[pos] = provider.clone();
    providers.sort_by(|a, b| a.id.cmp(&b.id));
    write_provider_store(home_root, &providers)?;
    Ok(provider)
}

pub fn delete_provider(home_root: &Path, provider_id: &str) -> Result<bool> {
    let bound = profiles_using_provider(home_root, provider_id)?;
    if !bound.is_empty() {
        return Err(AppError {
            code: "PROVIDER_IN_USE".into(),
            message: format!("provider {provider_id} is attached to profiles"),
            recoverable: true,
            details: Some(serde_json::json!({ "profiles": bound })),
        });
    }
    let mut providers = read_provider_store(home_root)?;
    let before = providers.len();
    providers.retain(|provider| provider.id != provider_id);
    if providers.len() == before {
        return Ok(false);
    }
    write_provider_store(home_root, &providers)?;
    Ok(true)
}

pub fn test_provider(home_root: &Path, provider_id: &str) -> Result<ProviderProfile> {
    let providers = read_provider_store(home_root)?;
    let mut provider = providers
        .into_iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| AppError::new("PROVIDER_NOT_FOUND", provider_id))?;
    provider.health = if provider.secret_storage == "env"
        && provider
            .env_key
            .as_ref()
            .map(|key| std::env::var(key).is_ok())
            .unwrap_or(false)
    {
        "available".into()
    } else if provider.secret_storage == "keychain" {
        "stored".into()
    } else {
        "metadata_only".into()
    };
    Ok(provider)
}

pub fn plan_attach_provider_to_profile(
    home_root: &Path,
    req: &AttachProviderRequest,
) -> Result<OperationPlan> {
    let account = find_account(home_root, &req.profile_id)?;
    let provider = find_provider(home_root, &req.provider_id)?;
    Ok(OperationPlan {
        operations: vec![
            format!("backup config.toml {}", account.codex_home.display()),
            format!("write provider reference {} -> {}", account.id, provider.id),
        ],
        warnings: vec![
            "Attach writes provider metadata only; secrets remain in env or Keychain.".into(),
        ],
        blocked: vec!["api_key".into(), "auth.json".into()],
    })
}

pub fn attach_provider_to_profile(
    home_root: &Path,
    req: &AttachProviderRequest,
) -> Result<AttachProviderResult> {
    execute_attach_provider_to_profile(home_root, req)
}

pub fn execute_attach_provider_to_profile(
    home_root: &Path,
    req: &AttachProviderRequest,
) -> Result<AttachProviderResult> {
    let plan = plan_attach_provider_to_profile(home_root, req)?;
    let account = find_account(home_root, &req.profile_id)?;
    let provider = find_provider(home_root, &req.provider_id)?;
    let config_path = account.codex_home.join("config.toml");
    let backup_path = account.codex_home.join(format!(
        "config.toml.backup.{}",
        timestamp_yyyymmdd_hhmmss()
    ));
    if config_path.exists() {
        fs::copy(&config_path, &backup_path)?;
    } else {
        write_file_private(&backup_path, "")?;
    }
    let model = req
        .model
        .clone()
        .unwrap_or_else(|| provider.default_model.clone());
    let body = format!(
        "model = \"{}\"\nmodel_provider = \"{}\"\nprovider_base_url = \"{}\"\nprovider_wire_api = \"{}\"\nenv_key = {}\n",
        json_escape(&model),
        json_escape(&provider.id),
        json_escape(&provider.base_url),
        json_escape(&provider.wire_api),
        provider
            .env_key
            .as_ref()
            .map(|key| format!("\"{}\"", json_escape(key)))
            .unwrap_or_else(|| "null".into())
    );
    write_file_private(&config_path, &body)?;
    Ok(AttachProviderResult {
        profile_id: req.profile_id.clone(),
        provider_id: req.provider_id.clone(),
        config_path,
        backup_path,
        operations: plan.operations,
        warnings: plan.warnings,
    })
}

fn provider_store_path(home_root: &Path) -> PathBuf {
    config_root(home_root).join("providers.json")
}

fn read_provider_store(home_root: &Path) -> Result<Vec<ProviderProfile>> {
    let path = provider_store_path(home_root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let body = fs::read_to_string(path)?;
    serde_json::from_str(&body)
        .map_err(|err| AppError::new("PROVIDER_STORE_INVALID", err.to_string()))
}

fn write_provider_store(home_root: &Path, providers: &[ProviderProfile]) -> Result<()> {
    let body = serde_json::to_string_pretty(providers)
        .map_err(|err| AppError::new("PROVIDER_STORE_INVALID", err.to_string()))?;
    write_file_private(&provider_store_path(home_root), &format!("{body}\n"))
}

fn provider_from_create_request(
    id: String,
    req: &CreateProviderRequest,
) -> Result<ProviderProfile> {
    let secret_storage = match &req.secret {
        Some(SecretInput::Env { env_key }) => {
            if req.env_key.as_deref() != Some(env_key.as_str()) {
                return Err(AppError::new(
                    "PROVIDER_ENV_MISMATCH",
                    "secret env key must match provider env_key",
                ));
            }
            "env"
        }
        Some(SecretInput::Keychain { .. }) => "keychain",
        Some(SecretInput::None) | None => "none",
    };
    if req.name.trim().is_empty()
        || req.base_url.trim().is_empty()
        || req.wire_api.trim().is_empty()
        || req.default_model.trim().is_empty()
    {
        return Err(AppError::new(
            "PROVIDER_INVALID",
            "required provider field is empty",
        ));
    }
    Ok(ProviderProfile {
        id,
        name: req.name.trim().to_string(),
        base_url: req.base_url.trim().to_string(),
        wire_api: req.wire_api.trim().to_string(),
        default_model: req.default_model.trim().to_string(),
        env_key: req.env_key.clone(),
        secret_storage: secret_storage.into(),
        health: "untested".into(),
    })
}

fn persist_secret(home_root: &Path, provider_id: &str, secret: Option<&SecretInput>) -> Result<()> {
    match secret {
        Some(SecretInput::Keychain { secret }) => store_keychain_secret(provider_id, secret),
        Some(SecretInput::Env { .. }) | Some(SecretInput::None) | None => {
            fs::create_dir_all(config_root(home_root))?;
            Ok(())
        }
    }
}

fn store_keychain_secret(provider_id: &str, secret: &str) -> Result<()> {
    if secret.trim().is_empty() {
        return Err(AppError::new("PROVIDER_SECRET_EMPTY", "secret is empty"));
    }
    let status = std::process::Command::new("/usr/bin/security")
        .arg("add-generic-password")
        .arg("-U")
        .arg("-s")
        .arg("agent-workspace-manager")
        .arg("-a")
        .arg(format!("provider:{provider_id}"))
        .arg("-w")
        .arg(secret)
        .status()
        .map_err(|err| AppError::new("KEYCHAIN_UNAVAILABLE", err.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::new(
            "KEYCHAIN_WRITE_FAILED",
            "macOS Keychain did not accept the provider secret",
        ))
    }
}

fn validate_provider_id(input: &str) -> Result<String> {
    let id = input.trim();
    if id.is_empty() || id.len() > 64 {
        return Err(AppError::new("PROVIDER_INVALID_ID", input));
    }
    let mut chars = id.chars();
    let first = chars
        .next()
        .ok_or_else(|| AppError::new("PROVIDER_INVALID_ID", input))?;
    if !first.is_ascii_alphanumeric() {
        return Err(AppError::new("PROVIDER_INVALID_ID", input));
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.') {
        return Err(AppError::new("PROVIDER_INVALID_ID", input));
    }
    Ok(id.to_string())
}

fn find_provider(home_root: &Path, provider_id: &str) -> Result<ProviderProfile> {
    read_provider_store(home_root)?
        .into_iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| AppError::new("PROVIDER_NOT_FOUND", provider_id))
}

fn profiles_using_provider(home_root: &Path, provider_id: &str) -> Result<Vec<String>> {
    let mut bound = Vec::new();
    for account in list_accounts(home_root)? {
        if account.provider_id.as_deref() == Some(provider_id) {
            bound.push(account.id);
        }
    }
    bound.sort();
    Ok(bound)
}
