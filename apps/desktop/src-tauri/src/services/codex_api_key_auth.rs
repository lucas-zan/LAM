use super::error::{AppError, Result};
use super::provider_credentials::SecretValue;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

#[derive(Deserialize, Serialize)]
struct CodexApiKeyAuth {
    auth_mode: String,
    #[serde(rename = "OPENAI_API_KEY")]
    api_key: String,
}

pub fn write_codex_api_key(codex_home: &Path, secret: &SecretValue) -> Result<()> {
    let api_key = secret.with_exposed(|value| value.trim().to_owned());
    if api_key.is_empty() {
        return Err(AppError::new("CODEX_API_KEY_EMPTY", "API key is empty"));
    }
    create_private_directory(codex_home)?;
    let bytes = serde_json::to_vec_pretty(&CodexApiKeyAuth {
        auth_mode: "apikey".into(),
        api_key,
    })
    .map_err(|_| {
        AppError::new(
            "CODEX_API_KEY_SERIALIZATION",
            "Codex API key auth could not be serialized",
        )
    })?;
    replace_private_file(&codex_home.join("auth.json"), &bytes)
}

pub fn inspect_codex_api_key(codex_home: &Path) -> Result<bool> {
    let path = codex_home.join("auth.json");
    match fs::read(path) {
        Ok(bytes) => parse_api_key(&bytes).map(|_| true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

pub fn read_codex_api_key(codex_home: &Path) -> Result<SecretValue> {
    let bytes = fs::read(codex_home.join("auth.json")).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            AppError::new("CODEX_API_KEY_MISSING", "Codex API key is not configured")
        } else {
            error.into()
        }
    })?;
    parse_api_key(&bytes).map(SecretValue::from_sensitive)
}

pub fn remove_codex_api_key(codex_home: &Path) -> Result<()> {
    match fs::remove_file(codex_home.join("auth.json")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn parse_api_key(bytes: &[u8]) -> Result<String> {
    let auth: CodexApiKeyAuth = serde_json::from_slice(bytes).map_err(|_| {
        AppError::new(
            "CODEX_API_KEY_AUTH_INVALID",
            "Codex API key auth file is invalid",
        )
    })?;
    if auth.auth_mode != "apikey" || auth.api_key.trim().is_empty() {
        return Err(AppError::new(
            "CODEX_API_KEY_AUTH_INVALID",
            "Codex API key auth file is invalid",
        ));
    }
    Ok(auth.api_key)
}

fn create_private_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn replace_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = path.with_file_name(format!(".auth.json.{}.tmp", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    #[cfg(unix)]
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}
