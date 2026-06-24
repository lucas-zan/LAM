use super::error::{AppError, Result};
use std::fs;
use std::io::{BufRead, Read, Write};
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
    home_root.join(".config/agent-workspace")
}

pub(crate) fn auth_metadata_dir(home_root: &Path) -> PathBuf {
    config_root(home_root).join("auth-metadata")
}

pub(crate) fn auth_metadata_path(home_root: &Path, profile_id: &str) -> PathBuf {
    auth_metadata_dir(home_root).join(format!("{}.json", profile_id))
}

/// Returns the PAT accounts directory
pub(crate) fn pat_accounts_dir(home_root: &Path) -> PathBuf {
    config_root(home_root).join("pat-accounts")
}

/// Returns auth file path for a PAT account
pub(crate) fn pat_auth_path(home_root: &Path, account_id: &str) -> PathBuf {
    pat_accounts_dir(home_root).join(format!("auth-{}.json", account_id))
}

/// Returns metadata file path for a PAT account
pub(crate) fn pat_metadata_path(home_root: &Path, account_id: &str) -> PathBuf {
    pat_accounts_dir(home_root).join(format!("metadata-{}.json", account_id))
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
        if trimmed.starts_with('#') {
            continue;
        }
        let (left, right) = trimmed.split_once('=')?;
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
            } else if file_type.is_file() {
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
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    let start = buf.len().saturating_sub(max_bytes);
    Ok(String::from_utf8_lossy(&buf[start..]).to_string())
}

pub(crate) fn read_first_line(path: &Path) -> Result<String> {
    let file = fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(line)
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

/// Returns the path to the settings file
pub(crate) fn settings_file_path(home_root: &Path) -> PathBuf {
    config_root(home_root).join("settings.json")
}

/// Gets the current auth mode (oauth or pat)
pub fn get_auth_mode(home_root: &Path) -> Result<String> {
    let settings_path = settings_file_path(home_root);
    
    if !settings_path.exists() {
        // Default to oauth if no settings file
        return Ok("oauth".to_string());
    }
    
    let content = fs::read_to_string(&settings_path)
        .map_err(|e| AppError::new("READ_SETTINGS_FAILED", format!("Failed to read settings: {}", e)))?;
    
    let settings: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| AppError::new("PARSE_SETTINGS_FAILED", format!("Failed to parse settings: {}", e)))?;
    
    Ok(settings.get("authMode")
        .and_then(|v| v.as_str())
        .unwrap_or("oauth")
        .to_string())
}

/// Sets the auth mode (oauth or pat)
pub fn set_auth_mode(home_root: &Path, mode: &str) -> Result<()> {
    if mode != "oauth" && mode != "pat" {
        return Err(AppError::new("INVALID_AUTH_MODE", format!("Auth mode must be 'oauth' or 'pat', got: {}", mode)));
    }
    
    let settings_path = settings_file_path(home_root);
    let config_dir = config_root(home_root);
    
    // Ensure config directory exists
    fs::create_dir_all(&config_dir)
        .map_err(|e| AppError::new("CREATE_DIR_FAILED", format!("Failed to create config dir: {}", e)))?;
    
    let settings = serde_json::json!({
        "authMode": mode
    });
    
    write_file_private(&settings_path, &settings.to_string())?;
    
    Ok(())
}
