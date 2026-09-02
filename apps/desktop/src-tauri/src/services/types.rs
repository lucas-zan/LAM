use super::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const NEW_MARKER: &str = ".managed-by-agent-workspace.json";
pub(crate) const OLD_MARKER: &str = ".managed-by-codex-session-manager.json";

pub(crate) fn json_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub(crate) fn json_option(value: Option<&str>) -> String {
    value
        .map(|s| format!("\"{}\"", json_escape(s)))
        .unwrap_or_else(|| "null".into())
}

pub(crate) fn shell_quote<S: AsRef<str>>(value: S) -> String {
    format!("'{}'", value.as_ref().replace('\'', "'\\''"))
}

pub(crate) fn short_text(input: &str, max_chars: usize) -> String {
    let compact = input.split_whitespace().collect::<Vec<_>>().join(" ");
    let char_count = compact.chars().count();
    if char_count > max_chars {
        let truncated: String = compact.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{truncated}...")
    } else {
        compact
    }
}

pub(crate) fn system_secs(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn timestamp() -> String {
    system_secs(SystemTime::now()).to_string()
}

pub(crate) fn timestamp_yyyymmdd_hhmmss() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

pub(crate) fn config_root(home_root: &Path) -> PathBuf {
    super::lam_paths::LamPaths::for_home(home_root).config_root()
}

pub(crate) fn auth_metadata_dir(home_root: &Path) -> PathBuf {
    config_root(home_root).join("auth-metadata")
}

pub(crate) fn auth_metadata_path(home_root: &Path, profile_id: &str) -> PathBuf {
    auth_metadata_dir(home_root).join(format!("{}.json", profile_id))
}

pub(crate) fn write_file_private(path: &Path, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(path)?;
    file.write_all(body.as_bytes())?;
    set_file_private(path)?;
    Ok(())
}

pub(crate) fn write_executable(path: &Path, body: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, body)?;
    set_file_executable(path)?;
    Ok(())
}

#[cfg(unix)]
pub(crate) fn set_dir_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn set_dir_private(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
pub(crate) fn set_file_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn set_file_private(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
pub(crate) fn set_file_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn set_file_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[derive(Debug, Default)]
pub(crate) struct CodexConfigBinding {
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub auth_mode: Option<String>,
}

/// Reads top-level keys from a Codex config.toml.
/// Top-level keys (model, model_provider) are still line-parseable even with
/// [model_providers.<id>] sections, because TOML top-level keys appear before sections.
/// TODO: Migrate to `toml` crate for full TOML support once the dependency is added.
pub(crate) fn parse_codex_config(path: &Path) -> Result<CodexConfigBinding> {
    if !path.exists() {
        return Ok(CodexConfigBinding::default());
    }
    let body = fs::read_to_string(path)?;
    let provider_id = parse_toml_like_string(&body, "model_provider")
        .or_else(|| parse_toml_like_string(&body, "provider"));
    let model = parse_toml_like_string(&body, "model");
    let auth_mode = if provider_id.is_some() {
        Some("config".into())
    } else {
        None
    };
    Ok(CodexConfigBinding {
        provider_id,
        model,
        auth_mode,
    })
}

pub(crate) fn parse_toml_like_string(body: &str, key: &str) -> Option<String> {
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('[') {
            continue;
        }
        let Some((left, right)) = trimmed.split_once('=') else {
            continue;
        };
        if left.trim() != key {
            continue;
        }
        let value = right.trim().trim_matches('"').trim_matches('\'').trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

pub(crate) fn session_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        extension.eq_ignore_ascii_case("jsonl")
                            || extension.eq_ignore_ascii_case("json")
                    })
            {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

pub(crate) fn modified_secs(path: &Path) -> Result<u64> {
    Ok(fs::metadata(path)?
        .modified()
        .ok()
        .map(system_secs)
        .unwrap_or(0))
}

pub(crate) fn read_tail(path: &Path, max_bytes: usize) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let file_len = file.metadata()?.len();
    let start = file_len.saturating_sub(max_bytes as u64);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::with_capacity((file_len - start) as usize);
    file.read_to_end(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).to_string())
}

pub(crate) fn read_first_line(path: &Path) -> Result<String> {
    let file = fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut line = Vec::new();
    reader.read_until(b'\n', &mut line)?;
    Ok(String::from_utf8_lossy(&line).to_string())
}

pub(crate) fn validate_profile_name(input: &str) -> Result<String> {
    let name = input.trim();
    if name.is_empty() || name == "main" || name == "default" || name.len() > 32 {
        return Err(AppError::new("INVALID_ACCOUNT_NAME", input));
    }
    let mut chars = name.chars();
    let first = chars
        .next()
        .ok_or_else(|| AppError::new("INVALID_ACCOUNT_NAME", input))?;
    if !first.is_ascii_alphanumeric() {
        return Err(AppError::new("INVALID_ACCOUNT_NAME", input));
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(AppError::new("INVALID_ACCOUNT_NAME", input));
    }
    if name.contains("..") || name.contains('/') || name.contains('~') || name.contains(' ') {
        return Err(AppError::new("INVALID_ACCOUNT_NAME", input));
    }
    Ok(name.to_string())
}

pub(crate) fn validate_profile_id(input: &str) -> Result<String> {
    let name = input.trim();
    if name == "main" {
        Ok(name.to_string())
    } else {
        validate_profile_name(name)
    }
}

/// Returns the path to the settings file
pub(crate) fn settings_file_path(home_root: &Path) -> PathBuf {
    config_root(home_root).join("settings.json")
}

pub const DEFAULT_GATEWAY_FIRST_RESPONSE_TIMEOUT_SECONDS: u64 = 60;
pub const MIN_GATEWAY_FIRST_RESPONSE_TIMEOUT_SECONDS: u64 = 10;
pub const MAX_GATEWAY_FIRST_RESPONSE_TIMEOUT_SECONDS: u64 = 600;

/// Per-request hard deadline for a single Gateway proxied call (queue wait +
/// upstream handling + streaming). Defaults to 20 minutes; a long coding task
/// spans many independent requests, so this only bounds a single call.
pub const DEFAULT_GATEWAY_REQUEST_TIMEOUT_SECONDS: u64 = 20 * 60;
pub const MIN_GATEWAY_REQUEST_TIMEOUT_SECONDS: u64 = 5 * 60;
pub const MAX_GATEWAY_REQUEST_TIMEOUT_SECONDS: u64 = 60 * 60;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CodexLaunchPermissionPreset {
    #[default]
    AskForApproval,
    ApproveForMe,
    FullAccess,
}

pub fn codex_launch_permission_preset(home_root: &Path) -> CodexLaunchPermissionPreset {
    let Ok(content) = fs::read_to_string(settings_file_path(home_root)) else {
        return CodexLaunchPermissionPreset::default();
    };
    let Ok(settings) = serde_json::from_str::<serde_json::Value>(&content) else {
        return CodexLaunchPermissionPreset::default();
    };
    settings
        .get("codexLaunchPermissionPreset")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

pub fn set_codex_launch_permission_preset(
    home_root: &Path,
    preset: CodexLaunchPermissionPreset,
) -> Result<()> {
    let settings_path = settings_file_path(home_root);
    fs::create_dir_all(config_root(home_root))?;
    let mut settings = fs::read_to_string(&settings_path)
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(object) = settings.as_object_mut() {
        object.insert(
            "codexLaunchPermissionPreset".into(),
            serde_json::to_value(preset).map_err(|error| {
                AppError::new("CODEX_PERMISSION_CONFIG_INVALID", error.to_string())
            })?,
        );
    }
    write_file_private(&settings_path, &settings.to_string())
}

pub fn gateway_first_response_timeout_seconds(home_root: &Path) -> u64 {
    let settings_path = settings_file_path(home_root);
    let Ok(content) = fs::read_to_string(settings_path) else {
        return DEFAULT_GATEWAY_FIRST_RESPONSE_TIMEOUT_SECONDS;
    };
    let Ok(settings) = serde_json::from_str::<serde_json::Value>(&content) else {
        return DEFAULT_GATEWAY_FIRST_RESPONSE_TIMEOUT_SECONDS;
    };
    settings
        .get("gatewayFirstResponseTimeoutSeconds")
        .and_then(serde_json::Value::as_u64)
        .filter(|value| {
            (MIN_GATEWAY_FIRST_RESPONSE_TIMEOUT_SECONDS
                ..=MAX_GATEWAY_FIRST_RESPONSE_TIMEOUT_SECONDS)
                .contains(value)
        })
        .unwrap_or(DEFAULT_GATEWAY_FIRST_RESPONSE_TIMEOUT_SECONDS)
}

pub fn set_gateway_first_response_timeout_seconds(home_root: &Path, seconds: u64) -> Result<()> {
    if !(MIN_GATEWAY_FIRST_RESPONSE_TIMEOUT_SECONDS..=MAX_GATEWAY_FIRST_RESPONSE_TIMEOUT_SECONDS)
        .contains(&seconds)
    {
        return Err(AppError::new(
            "GATEWAY_TIMEOUT_CONFIG_INVALID",
            "Gateway first response timeout must be between 10 and 600 seconds",
        ));
    }
    let settings_path = settings_file_path(home_root);
    let config_dir = config_root(home_root);
    fs::create_dir_all(&config_dir)?;
    let mut settings = if settings_path.exists() {
        fs::read_to_string(&settings_path)
            .ok()
            .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
            .unwrap_or_else(|| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    if let Some(object) = settings.as_object_mut() {
        object.insert(
            "gatewayFirstResponseTimeoutSeconds".into(),
            serde_json::Value::from(seconds),
        );
    }
    write_file_private(&settings_path, &settings.to_string())
}

pub fn gateway_request_timeout_seconds(home_root: &Path) -> u64 {
    let settings_path = settings_file_path(home_root);
    let Ok(content) = fs::read_to_string(settings_path) else {
        return DEFAULT_GATEWAY_REQUEST_TIMEOUT_SECONDS;
    };
    let Ok(settings) = serde_json::from_str::<serde_json::Value>(&content) else {
        return DEFAULT_GATEWAY_REQUEST_TIMEOUT_SECONDS;
    };
    settings
        .get("gatewayRequestTimeoutSeconds")
        .and_then(serde_json::Value::as_u64)
        .filter(|value| {
            (MIN_GATEWAY_REQUEST_TIMEOUT_SECONDS..=MAX_GATEWAY_REQUEST_TIMEOUT_SECONDS)
                .contains(value)
        })
        .unwrap_or(DEFAULT_GATEWAY_REQUEST_TIMEOUT_SECONDS)
}

pub fn set_gateway_request_timeout_seconds(home_root: &Path, seconds: u64) -> Result<()> {
    if !(MIN_GATEWAY_REQUEST_TIMEOUT_SECONDS..=MAX_GATEWAY_REQUEST_TIMEOUT_SECONDS)
        .contains(&seconds)
    {
        return Err(AppError::new(
            "GATEWAY_TIMEOUT_CONFIG_INVALID",
            "Gateway request timeout must be between 300 and 3600 seconds",
        ));
    }
    let settings_path = settings_file_path(home_root);
    let config_dir = config_root(home_root);
    fs::create_dir_all(&config_dir)?;
    let mut settings = if settings_path.exists() {
        fs::read_to_string(&settings_path)
            .ok()
            .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
            .unwrap_or_else(|| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    if let Some(object) = settings.as_object_mut() {
        object.insert(
            "gatewayRequestTimeoutSeconds".into(),
            serde_json::Value::from(seconds),
        );
    }
    write_file_private(&settings_path, &settings.to_string())
}

pub fn antigravity_port(home_root: &Path) -> Option<u16> {
    let content = fs::read_to_string(settings_file_path(home_root)).ok()?;
    let settings = serde_json::from_str::<serde_json::Value>(&content).ok()?;
    settings
        .get("antigravityPort")
        .and_then(serde_json::Value::as_u64)
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port != 0)
}

pub fn set_antigravity_port(home_root: &Path, port: Option<u64>) -> Result<()> {
    if matches!(port, Some(value) if !(1..=u16::MAX as u64).contains(&value)) {
        return Err(AppError::new(
            "ANTIGRAVITY_PORT_CONFIG_INVALID",
            "Antigravity port must be between 1 and 65535",
        ));
    }

    let settings_path = settings_file_path(home_root);
    fs::create_dir_all(config_root(home_root))?;
    let mut settings = fs::read_to_string(&settings_path)
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    let object = settings
        .as_object_mut()
        .expect("settings must be an object");
    if let Some(port) = port {
        object.insert("antigravityPort".into(), serde_json::Value::from(port));
    } else {
        object.remove("antigravityPort");
    }
    write_file_private(&settings_path, &settings.to_string())
}

/// Gets the current auth mode (oauth or pat)
pub fn get_auth_mode(home_root: &Path) -> Result<String> {
    let settings_path = settings_file_path(home_root);

    if !settings_path.exists() {
        // Default to oauth if no settings file
        return Ok("oauth".to_string());
    }

    let content = fs::read_to_string(&settings_path).map_err(|e| {
        AppError::new(
            "READ_SETTINGS_FAILED",
            format!("Failed to read settings: {}", e),
        )
    })?;

    let settings: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
        AppError::new(
            "PARSE_SETTINGS_FAILED",
            format!("Failed to parse settings: {}", e),
        )
    })?;

    Ok(settings
        .get("authMode")
        .and_then(|v| v.as_str())
        .unwrap_or("oauth")
        .to_string())
}

/// Sets the auth mode (oauth or pat)
pub fn set_auth_mode(home_root: &Path, mode: &str) -> Result<()> {
    if mode != "oauth" && mode != "pat" {
        return Err(AppError::new(
            "INVALID_AUTH_MODE",
            format!("Auth mode must be 'oauth' or 'pat', got: {}", mode),
        ));
    }

    let settings_path = settings_file_path(home_root);
    let config_dir = config_root(home_root);

    // Ensure config directory exists
    fs::create_dir_all(&config_dir).map_err(|e| {
        AppError::new(
            "CREATE_DIR_FAILED",
            format!("Failed to create config dir: {}", e),
        )
    })?;

    // Incremental merge to settings.json
    let mut settings = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path).unwrap_or_default();
        serde_json::from_str::<serde_json::Value>(&content)
            .unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if let Some(obj) = settings.as_object_mut() {
        obj.insert(
            "authMode".to_string(),
            serde_json::Value::String(mode.to_string()),
        );
    }

    write_file_private(&settings_path, &settings.to_string())?;

    Ok(())
}

/// Gets the current hide dock icon setting (defaults to false)
pub fn get_hide_dock_icon(home_root: &Path) -> bool {
    let settings_path = settings_file_path(home_root);

    if !settings_path.exists() {
        return false;
    }

    let content = match fs::read_to_string(&settings_path) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let settings: serde_json::Value = match serde_json::from_str(&content) {
        Ok(s) => s,
        Err(_) => return false,
    };

    settings
        .get("hideDockIcon")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Sets the hide dock icon setting
pub fn set_hide_dock_icon(home_root: &Path, hide: bool) -> Result<()> {
    let settings_path = settings_file_path(home_root);
    let config_dir = config_root(home_root);

    // Ensure config directory exists
    fs::create_dir_all(&config_dir).map_err(|e| {
        AppError::new(
            "CREATE_DIR_FAILED",
            format!("Failed to create config dir: {}", e),
        )
    })?;

    // Incremental merge to settings.json
    let mut settings = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path).unwrap_or_default();
        serde_json::from_str::<serde_json::Value>(&content)
            .unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if let Some(obj) = settings.as_object_mut() {
        obj.insert("hideDockIcon".to_string(), serde_json::Value::Bool(hide));
    }

    write_file_private(&settings_path, &settings.to_string())?;

    Ok(())
}

pub fn selected_terminal_target_id(home_root: &Path) -> String {
    let settings_path = settings_file_path(home_root);

    if !settings_path.exists() {
        return "terminal".to_string();
    }

    let content = match fs::read_to_string(&settings_path) {
        Ok(c) => c,
        Err(_) => return "terminal".to_string(),
    };

    let settings: serde_json::Value = match serde_json::from_str(&content) {
        Ok(s) => s,
        Err(_) => return "terminal".to_string(),
    };

    settings
        .get("terminalTargetId")
        .and_then(|v| v.as_str())
        .filter(|id| is_known_terminal_target_id(id))
        .unwrap_or("terminal")
        .to_string()
}

pub fn set_selected_terminal_target_id(home_root: &Path, target_id: &str) -> Result<()> {
    if !is_known_terminal_target_id(target_id) {
        return Err(AppError::new(
            "INVALID_TERMINAL_TARGET",
            format!("Unknown terminal target: {target_id}"),
        ));
    }

    let settings_path = settings_file_path(home_root);
    let config_dir = config_root(home_root);
    fs::create_dir_all(&config_dir).map_err(|e| {
        AppError::new(
            "CREATE_DIR_FAILED",
            format!("Failed to create config dir: {}", e),
        )
    })?;

    let mut settings = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path).unwrap_or_default();
        serde_json::from_str::<serde_json::Value>(&content)
            .unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if let Some(obj) = settings.as_object_mut() {
        obj.insert(
            "terminalTargetId".to_string(),
            serde_json::Value::String(target_id.to_string()),
        );
    }

    write_file_private(&settings_path, &settings.to_string())?;
    Ok(())
}

fn is_known_terminal_target_id(target_id: &str) -> bool {
    matches!(target_id, "terminal" | "ghostty" | "cmux" | "codex_app")
}
