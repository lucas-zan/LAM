use super::error::{AppError, Result};
use super::provider_credentials::{validate_codex_security_options, DirectCodexAuth};
use super::provider_v2::CodexProviderOptions;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use toml_edit::{value, Array, DocumentMut, InlineTable, Item, Table};
use uuid::Uuid;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ConfigProjectionSpec {
    pub provider_id: String,
    pub model: String,
    pub display_name: String,
    pub base_url: String,
    pub auth: DirectCodexAuth,
    pub codex: CodexProviderOptions,
    pub gateway: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ConfigManagedProjection {
    pub before_hash: String,
    pub applied_hash: String,
    pub provider_id: String,
    pub previous_values: BTreeMap<String, Option<String>>,
    pub managed_values: BTreeMap<String, String>,
    pub provider_table_created_by_lam: bool,
}

#[derive(Debug, Clone)]
pub struct AppliedProjection {
    pub contents: String,
    pub projection: ConfigManagedProjection,
    pub backup_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigWriteFault {
    BeforeRename,
}

pub fn config_hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn apply_projection(
    source: &str,
    expected_hash: &str,
    spec: &ConfigProjectionSpec,
) -> Result<AppliedProjection> {
    if config_hash(source.as_bytes()) != expected_hash {
        return Err(AppError::new(
            "CODEX_CONFIG_CONFLICT",
            "config hash changed",
        ));
    }
    if source.contains("experimental_bearer_token") {
        return Err(AppError::new(
            "CODEX_CONFIG_SECRET_CONFLICT",
            "secret-bearing auth value is not manageable",
        ));
    }
    validate_codex_security_options(&spec.codex)?;
    let mut doc = source
        .parse::<DocumentMut>()
        .map_err(|e| AppError::new("CODEX_CONFIG_INVALID", e.to_string()))?;
    if doc
        .get("model_providers")
        .and_then(Item::as_table_like)
        .and_then(|providers| providers.get(&spec.provider_id))
        .and_then(Item::as_table_like)
        .is_some_and(|provider| provider.get("http_headers").is_some())
    {
        return Err(AppError::new(
            "CODEX_CONFIG_STATIC_HEADERS_UNSUPPORTED",
            "MVP does not take ownership of static Provider headers",
        ));
    }
    let created = doc
        .get("model_providers")
        .and_then(Item::as_table_like)
        .and_then(|t| t.get(&spec.provider_id))
        .is_none();
    ensure_provider_table(&mut doc, &spec.provider_id);
    if doc["model_providers"][&spec.provider_id]
        .get("auth")
        .is_some()
    {
        return Err(AppError::new(
            "CODEX_CONFIG_AUTH_CONFLICT",
            "pre-existing auth table requires explicit adoption",
        ));
    }
    let keys = managed_keys(spec);
    let previous_values = keys
        .iter()
        .map(|key| (key.clone(), item_string(&doc, &spec.provider_id, key)))
        .collect();
    set_top(&mut doc, "model", &spec.model);
    set_top(&mut doc, "model_provider", &spec.provider_id);
    set_provider(
        &mut doc,
        &spec.provider_id,
        "name",
        value(&spec.display_name),
    );
    set_provider(
        &mut doc,
        &spec.provider_id,
        "base_url",
        value(&spec.base_url),
    );
    set_provider(&mut doc, &spec.provider_id, "wire_api", value("responses"));
    clear_auth(&mut doc, &spec.provider_id);
    apply_auth(&mut doc, &spec.provider_id, &spec.auth)?;
    apply_options(&mut doc, &spec.provider_id, spec);
    let contents = doc.to_string();
    contents
        .parse::<toml::Value>()
        .map_err(|e| AppError::new("CODEX_CONFIG_INVALID", e.to_string()))?;
    let managed_values = keys
        .iter()
        .filter_map(|key| item_string(&doc, &spec.provider_id, key).map(|v| (key.clone(), v)))
        .collect();
    Ok(AppliedProjection {
        projection: ConfigManagedProjection {
            before_hash: expected_hash.into(),
            applied_hash: config_hash(contents.as_bytes()),
            provider_id: spec.provider_id.clone(),
            previous_values,
            managed_values,
            provider_table_created_by_lam: created,
        },
        contents,
        backup_path: None,
    })
}

pub fn detach_projection(source: &str, projection: &ConfigManagedProjection) -> Result<String> {
    let mut doc = source
        .parse::<DocumentMut>()
        .map_err(|e| AppError::new("CODEX_CONFIG_INVALID", e.to_string()))?;
    for (key, expected) in &projection.managed_values {
        if item_string(&doc, &projection.provider_id, key).as_deref() != Some(expected) {
            return Err(AppError::new("CODEX_CONFIG_OWNERSHIP_CONFLICT", key));
        }
    }
    for (key, previous) in &projection.previous_values {
        restore_item(&mut doc, &projection.provider_id, key, previous.as_deref())?;
    }
    if projection.provider_table_created_by_lam {
        if let Some(table) = doc["model_providers"][&projection.provider_id].as_table() {
            if table.is_empty() {
                doc["model_providers"]
                    .as_table_mut()
                    .map(|t| t.remove(&projection.provider_id));
            }
        }
    }
    if doc
        .get("model_providers")
        .and_then(Item::as_table)
        .is_some_and(Table::is_empty)
    {
        doc.as_table_mut().remove("model_providers");
    }
    Ok(doc.to_string())
}

pub fn validate_managed_projection(
    source: &str,
    provider_id: &str,
    managed_values: &BTreeMap<String, String>,
) -> Result<()> {
    let doc = source
        .parse::<DocumentMut>()
        .map_err(|error| AppError::new("CODEX_CONFIG_INVALID", error.to_string()))?;
    for (key, expected) in managed_values {
        if item_string(&doc, provider_id, key).as_deref() != Some(expected) {
            return Err(AppError::new("CODEX_CONFIG_OWNERSHIP_CONFLICT", key));
        }
    }
    Ok(())
}

pub fn reapply_managed_projection(
    source: &str,
    projection: &ConfigManagedProjection,
) -> Result<String> {
    let mut doc = source
        .parse::<DocumentMut>()
        .map_err(|error| AppError::new("CODEX_CONFIG_INVALID", error.to_string()))?;
    ensure_provider_table(&mut doc, &projection.provider_id);
    for (key, managed) in &projection.managed_values {
        restore_item(&mut doc, &projection.provider_id, key, Some(managed))?;
    }
    let contents = doc.to_string();
    contents
        .parse::<toml::Value>()
        .map_err(|error| AppError::new("CODEX_CONFIG_INVALID", error.to_string()))?;
    Ok(contents)
}

pub fn apply_projection_file(
    path: &Path,
    expected_hash: &str,
    spec: &ConfigProjectionSpec,
    fault: Option<ConfigWriteFault>,
) -> Result<AppliedProjection> {
    let source = match fs::read_to_string(path) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let mut applied = apply_projection(&source, expected_hash, spec)?;
    let backup = path.with_file_name(format!("config.toml.backup.{}", Uuid::new_v4()));
    write_private(&backup, source.as_bytes())?;
    let temp = path.with_file_name(format!(".config.{}.tmp", Uuid::new_v4()));
    write_private(&temp, applied.contents.as_bytes())?;
    if fault == Some(ConfigWriteFault::BeforeRename) {
        let _ = fs::remove_file(&temp);
        return Err(AppError::new(
            "CODEX_CONFIG_FAULT_INJECTED",
            "before rename",
        ));
    }
    fs::rename(&temp, path)?;
    sync_parent(path)?;
    applied.backup_path = Some(backup);
    Ok(applied)
}

pub fn replace_config_file(path: &Path, expected_hash: &str, contents: &str) -> Result<()> {
    let current = match fs::read(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    if config_hash(&current) != expected_hash {
        return Err(AppError::new(
            "CODEX_CONFIG_CONFLICT",
            "config hash changed",
        ));
    }
    contents
        .parse::<toml::Value>()
        .map_err(|error| AppError::new("CODEX_CONFIG_INVALID", error.to_string()))?;
    let temp = path.with_file_name(format!(".config.{}.tmp", Uuid::new_v4()));
    write_private(&temp, contents.as_bytes())?;
    fs::rename(&temp, path)?;
    sync_parent(path)
}

pub fn replace_config_file_with_backup(
    path: &Path,
    expected_hash: &str,
    contents: &str,
) -> Result<PathBuf> {
    let current = match fs::read(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    if config_hash(&current) != expected_hash {
        return Err(AppError::new(
            "CODEX_CONFIG_CONFLICT",
            "config hash changed",
        ));
    }
    let backup = path.with_file_name(format!("config.toml.backup.{}", Uuid::new_v4()));
    write_private(&backup, &current)?;
    replace_config_file(path, expected_hash, contents)?;
    Ok(backup)
}

fn managed_keys(spec: &ConfigProjectionSpec) -> Vec<String> {
    let mut keys = vec![
        "model".into(),
        "model_provider".into(),
        "name".into(),
        "base_url".into(),
        "wire_api".into(),
    ];
    match &spec.auth {
        DirectCodexAuth::EnvKey { .. } => keys.push("env_key".into()),
        DirectCodexAuth::EnvHeader { .. } => keys.push("env_http_headers".into()),
        DirectCodexAuth::AuthCommand { .. } => keys.push("auth".into()),
        DirectCodexAuth::NativeApiKey => {
            keys.push("cli_auth_credentials_store".into());
            keys.push("requires_openai_auth".into());
        }
        DirectCodexAuth::Keychain { .. }
        | DirectCodexAuth::ApprovedCommand { .. }
        | DirectCodexAuth::Gateway => keys.push("unresolved_auth".into()),
        DirectCodexAuth::None => {}
    }
    if spec.gateway {
        keys.extend(["request_max_retries".into(), "stream_max_retries".into()]);
    } else {
        if spec.codex.direct_request_max_retries.is_some() {
            keys.push("request_max_retries".into());
        }
        if spec.codex.direct_stream_max_retries.is_some() {
            keys.push("stream_max_retries".into());
        }
    }
    if spec.codex.stream_idle_timeout_ms.is_some() {
        keys.push("stream_idle_timeout_ms".into());
    }
    if !spec.codex.query_params.is_empty() {
        keys.push("query_params".into());
    }
    if !spec.codex.env_http_headers.is_empty() && !keys.iter().any(|k| k == "env_http_headers") {
        keys.push("env_http_headers".into());
    }
    keys
}
fn ensure_provider_table(doc: &mut DocumentMut, id: &str) {
    if doc
        .get("model_providers")
        .and_then(Item::as_table)
        .is_none()
    {
        doc["model_providers"] = Item::Table(Table::new());
    }
    if doc["model_providers"].get(id).is_none() {
        doc["model_providers"][id] = Item::Table(Table::new());
    }
}
fn set_top(doc: &mut DocumentMut, key: &str, value_: &str) {
    doc[key] = value(value_);
}
fn set_provider(doc: &mut DocumentMut, id: &str, key: &str, item: Item) {
    doc["model_providers"][id][key] = item;
}
fn clear_auth(doc: &mut DocumentMut, id: &str) {
    if let Some(t) = doc["model_providers"][id].as_table_mut() {
        for k in [
            "env_key",
            "env_http_headers",
            "auth",
            "requires_openai_auth",
        ] {
            t.remove(k);
        }
    }
}
fn apply_auth(doc: &mut DocumentMut, id: &str, auth: &DirectCodexAuth) -> Result<()> {
    match auth {
        DirectCodexAuth::EnvKey { env_key } => set_provider(doc, id, "env_key", value(env_key)),
        DirectCodexAuth::EnvHeader { name, env_key } => {
            let mut t = InlineTable::new();
            t.insert(name, toml_edit::Value::from(env_key));
            set_provider(doc, id, "env_http_headers", value(t));
        }
        DirectCodexAuth::AuthCommand {
            command,
            args: command_args,
            ..
        } => {
            let mut t = Table::new();
            t["command"] = value(command);
            let mut args = Array::new();
            for arg in command_args {
                args.push(arg.as_str());
            }
            t["args"] = value(args);
            t["timeout_ms"] = value(5_000);
            t["refresh_interval_ms"] = value(0);
            set_provider(doc, id, "auth", Item::Table(t));
        }
        DirectCodexAuth::NativeApiKey => {
            set_top(doc, "cli_auth_credentials_store", "file");
            set_provider(doc, id, "requires_openai_auth", value(true));
        }
        DirectCodexAuth::Keychain { .. }
        | DirectCodexAuth::ApprovedCommand { .. }
        | DirectCodexAuth::Gateway => {
            return Err(AppError::new(
                "CODEX_AUTH_NOT_MATERIALIZED",
                "Codex auth intent must be materialized before editing config",
            ))
        }
        DirectCodexAuth::None => {}
    }
    Ok(())
}
fn apply_options(doc: &mut DocumentMut, id: &str, spec: &ConfigProjectionSpec) {
    if spec.gateway {
        set_provider(doc, id, "request_max_retries", value(0));
        set_provider(doc, id, "stream_max_retries", value(0));
    } else {
        if let Some(v) = spec.codex.direct_request_max_retries {
            set_provider(doc, id, "request_max_retries", value(i64::from(v)));
        }
        if let Some(v) = spec.codex.direct_stream_max_retries {
            set_provider(doc, id, "stream_max_retries", value(i64::from(v)));
        }
    }
    if let Some(v) = spec.codex.stream_idle_timeout_ms {
        set_provider(doc, id, "stream_idle_timeout_ms", value(v as i64));
    }
    if !spec.codex.query_params.is_empty() {
        let mut table = InlineTable::new();
        for (k, v) in &spec.codex.query_params {
            table.insert(k, toml_edit::Value::from(v));
        }
        set_provider(doc, id, "query_params", value(table));
    }
    if !spec.codex.env_http_headers.is_empty() {
        let mut table = doc["model_providers"][id]
            .get("env_http_headers")
            .and_then(Item::as_inline_table)
            .cloned()
            .unwrap_or_default();
        for (k, v) in &spec.codex.env_http_headers {
            table.insert(k, toml_edit::Value::from(v));
        }
        set_provider(doc, id, "env_http_headers", value(table));
    }
}
fn item_string(doc: &DocumentMut, id: &str, key: &str) -> Option<String> {
    let item = if matches!(
        key,
        "model" | "model_provider" | "cli_auth_credentials_store"
    ) {
        doc.get(key)
    } else {
        doc.get("model_providers")?.get(id)?.get(key)
    };
    item.map(|value| value.to_string().trim().to_string())
}
fn restore_item(doc: &mut DocumentMut, id: &str, key: &str, previous: Option<&str>) -> Result<()> {
    let target = if matches!(
        key,
        "model" | "model_provider" | "cli_auth_credentials_store"
    ) {
        &mut doc[key]
    } else {
        &mut doc["model_providers"][id][key]
    };
    match previous {
        Some(v) => {
            if key == "auth" {
                let mut wrapped = format!("[auth]\n{v}\n")
                    .parse::<DocumentMut>()
                    .map_err(|e| AppError::new("CODEX_CONFIG_INVALID", format!("{e}")))?;
                *target = wrapped
                    .as_table_mut()
                    .remove("auth")
                    .ok_or_else(|| AppError::new("CODEX_CONFIG_INVALID", "missing auth table"))?;
            } else {
                *target = Item::Value(
                    v.parse()
                        .map_err(|e| AppError::new("CODEX_CONFIG_INVALID", format!("{e}")))?,
                );
            }
        }
        None => {
            if matches!(
                key,
                "model" | "model_provider" | "cli_auth_credentials_store"
            ) {
                doc.as_table_mut().remove(key);
            } else if let Some(t) = doc["model_providers"][id].as_table_mut() {
                t.remove(key);
            }
        }
    }
    Ok(())
}
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut o = OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        o.mode(0o600);
    }
    let mut f = o.open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

fn sync_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::new("CODEX_CONFIG_PATH_INVALID", "config has no parent"))?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
