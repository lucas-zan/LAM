use super::account::{list_accounts, quota_account, CodexAccount};
use super::error::{AppError, Result};
use super::types::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::SystemTime;

const CODEX_APP_SERVER_QUOTA_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

struct QuotaAuthHome {
    path: PathBuf,
}

impl QuotaAuthHome {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for QuotaAuthHome {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageQuotaSnapshot {
    pub profile_id: String,
    pub source: String,
    pub fetched_at: u64,
    pub staleness: String,
    pub plan_type: Option<String>,
    pub activity_tokens: Option<u64>,
    pub primary_used_percent: Option<u8>,
    #[serde(default)]
    pub primary_window_duration_mins: Option<u64>,
    pub secondary_used_percent: Option<u8>,
    #[serde(default)]
    pub secondary_window_duration_mins: Option<u64>,
    pub remaining_percent: Option<u8>,
    pub reset_at: Option<String>,
    pub secondary_reset_at: Option<String>,
    pub alerts: Vec<String>,
    pub suggested_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuotaRefreshResult {
    pub snapshots: Vec<UsageQuotaSnapshot>,
    pub warnings: Vec<String>,
}

pub fn get_profile_quota(
    home_root: &Path,
    profile_id: &str,
    force_refresh: bool,
) -> Result<UsageQuotaSnapshot> {
    let account = quota_account(home_root, profile_id)?;
    if force_refresh && app_server_quota_enabled() {
        match try_codex_app_server_quota(home_root, &account) {
            Ok(snapshot) => {
                write_quota_cache(home_root, &snapshot)?;
                return Ok(snapshot);
            }
            Err(err) => {
                if let Some(mut cached) = read_quota_cache(home_root, profile_id)? {
                    cached.alerts.push(format!(
                        "Codex app-server quota unavailable: {}",
                        err.message
                    ));
                    return Ok(cached);
                }
                return Ok(unavailable_quota_snapshot(
                    profile_id,
                    Some(format!(
                        "Codex app-server quota unavailable: {}",
                        err.message
                    )),
                ));
            }
        }
    }
    if let Some(cached) = read_quota_cache(home_root, profile_id)? {
        return Ok(cached);
    }
    Ok(unavailable_quota_snapshot(profile_id, None))
}

pub fn list_cached_quotas(
    home_root: &Path,
    profile_ids: Option<Vec<String>>,
) -> Result<Vec<UsageQuotaSnapshot>> {
    let requested = if let Some(profile_ids) = profile_ids {
        profile_ids
    } else {
        list_accounts(home_root)?
            .iter()
            .map(|account| account.id.clone())
            .collect()
    };
    let mut snapshots = Vec::new();
    for profile_id in requested {
        if let Some(snapshot) = read_quota_cache(home_root, &profile_id)? {
            snapshots.push(snapshot);
        }
    }
    snapshots.sort_by(|a, b| a.profile_id.cmp(&b.profile_id));
    Ok(snapshots)
}

pub fn refresh_all_quotas(
    home_root: &Path,
    profile_ids: Option<Vec<String>>,
) -> Result<QuotaRefreshResult> {
    let accounts = list_accounts(home_root)?;
    let requested = profile_ids.unwrap_or_else(|| accounts.iter().map(|a| a.id.clone()).collect());
    let mut snapshots = Vec::new();
    let mut warnings = Vec::new();
    for profile_id in requested {
        match get_profile_quota(home_root, &profile_id, true) {
            Ok(snapshot) => {
                if snapshot.staleness != "fresh" {
                    warnings.push(quota_fallback_warning(&profile_id, &snapshot));
                }
                snapshots.push(snapshot);
            }
            Err(err) => warnings.push(format!("{profile_id}: {}", err.message)),
        }
    }
    Ok(QuotaRefreshResult {
        snapshots,
        warnings,
    })
}

fn quota_fallback_warning(profile_id: &str, snapshot: &UsageQuotaSnapshot) -> String {
    let detail = snapshot
        .alerts
        .iter()
        .find(|alert| !alert.trim().is_empty())
        .map(|alert| format!(" ({alert})"))
        .unwrap_or_default();
    format!(
        "{profile_id}: realtime quota unavailable; using {} quota{detail}",
        snapshot.staleness
    )
}

fn unavailable_quota_snapshot(profile_id: &str, alert: Option<String>) -> UsageQuotaSnapshot {
    UsageQuotaSnapshot {
        profile_id: profile_id.to_string(),
        source: "usage_unavailable".into(),
        fetched_at: system_secs(SystemTime::now()),
        staleness: "unavailable".into(),
        plan_type: None,
        activity_tokens: None,
        primary_used_percent: None,
        primary_window_duration_mins: None,
        secondary_used_percent: None,
        secondary_window_duration_mins: None,
        remaining_percent: None,
        reset_at: None,
        secondary_reset_at: None,
        alerts: alert.into_iter().collect(),
        suggested_actions: Vec::new(),
    }
}

fn app_server_quota_enabled() -> bool {
    if let Ok(value) = std::env::var("LAM_ENABLE_CODEX_APP_SERVER_QUOTA") {
        return value == "1" || value.eq_ignore_ascii_case("true");
    }
    if let Ok(value) = std::env::var("LAM_DISABLE_CODEX_APP_SERVER_QUOTA") {
        return !(value == "1" || value.eq_ignore_ascii_case("true"));
    }
    true
}

fn resolve_codex_bin(home_root: &Path) -> Option<std::path::PathBuf> {
    if let Ok(val) = std::env::var("LAM_CODEX_BIN") {
        if !val.is_empty() {
            return Some(std::path::PathBuf::from(val));
        }
    }

    for path in codex_bin_candidates(home_root) {
        if path.exists() {
            return Some(path);
        }
    }

    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let p = dir.join("codex");
            if p.exists() {
                return Some(p);
            }
        }
    }

    None
}

fn codex_bin_candidates(home_root: &Path) -> Vec<std::path::PathBuf> {
    let mut candidates = user_runtime_path_dirs(home_root)
        .into_iter()
        .map(|dir| dir.join("codex"))
        .collect::<Vec<_>>();
    if std::env::var("CARGO_MANIFEST_DIR").is_err() {
        candidates.extend([
            std::path::PathBuf::from("/opt/homebrew/bin/codex"),
            std::path::PathBuf::from("/usr/local/bin/codex"),
        ]);
    }
    candidates
}

fn user_runtime_path_dirs(home_root: &Path) -> Vec<std::path::PathBuf> {
    [
        ".bun/bin",
        ".local/bin",
        ".npm-global/bin",
        ".npm/bin",
        ".yarn/bin",
        ".volta/bin",
        ".asdf/shims",
        ".mise/shims",
    ]
    .iter()
    .map(|dir| home_root.join(dir))
    .collect()
}

fn codex_app_server_path(home_root: &Path) -> Result<OsString> {
    let mut dirs = user_runtime_path_dirs(home_root);
    if std::env::var("CARGO_MANIFEST_DIR").is_err() {
        dirs.extend([
            std::path::PathBuf::from("/opt/homebrew/bin"),
            std::path::PathBuf::from("/usr/local/bin"),
        ]);
    }
    if let Some(path_var) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path_var));
    }
    std::env::join_paths(dirs).map_err(|err| AppError::new("PATH_ERROR", err.to_string()))
}

fn try_codex_app_server_quota(
    home_root: &Path,
    account: &CodexAccount,
) -> Result<UsageQuotaSnapshot> {
    if let Some(snapshot) = try_chatgpt_usage_quota(account)? {
        return Ok(snapshot);
    }
    let quota_auth_home = prepare_quota_auth_home(account)?;
    let codex_home = quota_auth_home
        .as_ref()
        .map(QuotaAuthHome::path)
        .unwrap_or(&account.codex_home);
    let mut child = spawn_codex_app_server(home_root, codex_home)?;
    if let Some(stdin) = child.stdin.as_mut() {
        if let Err(err) = stdin.write_all(
            b"{\"id\":1,\"method\":\"initialize\",\"params\":{\"clientInfo\":{\"name\":\"lam\",\"version\":\"0.1\"},\"capabilities\":{\"experimentalApi\":true}}}\n",
        ) {
            terminate_child(&mut child);
            return Err(AppError::new(
                "CODEX_APP_SERVER_WRITE_FAILED",
                err.to_string(),
            ));
        }
        if let Err(err) =
            stdin.write_all(b"{\"id\":2,\"method\":\"account/rateLimits/read\",\"params\":null}\n")
        {
            terminate_child(&mut child);
            return Err(AppError::new(
                "CODEX_APP_SERVER_WRITE_FAILED",
                err.to_string(),
            ));
        }
        if let Err(err) = stdin.flush() {
            terminate_child(&mut child);
            return Err(AppError::new(
                "CODEX_APP_SERVER_WRITE_FAILED",
                err.to_string(),
            ));
        }
    } else {
        terminate_child(&mut child);
        return Err(AppError::new(
            "CODEX_APP_SERVER_NO_STDIN",
            "stdin not available",
        ));
    }
    let Some(stdout) = child.stdout.take() else {
        terminate_child(&mut child);
        return Err(AppError::new(
            "CODEX_APP_SERVER_NO_STDOUT",
            "stdout not available",
        ));
    };
    let Some(stderr) = child.stderr.take() else {
        terminate_child(&mut child);
        return Err(AppError::new(
            "CODEX_APP_SERVER_NO_STDERR",
            "stderr not available",
        ));
    };
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let (tx_err, rx_err) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let _ = tx.send(line.clone());
                }
                Err(_) => break,
            }
        }
    });
    std::thread::spawn(move || {
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(stderr);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let trimmed = line.trim().to_string();
                    if !trimmed.is_empty() {
                        let _ = tx_err.send(trimmed);
                    }
                }
                Err(_) => break,
            }
        }
    });
    let deadline = std::time::Instant::now() + CODEX_APP_SERVER_QUOTA_TIMEOUT;
    let mut parsed: Option<UsageQuotaSnapshot> = None;
    let mut last_stderr: Option<String> = None;
    loop {
        while let Ok(line) = rx_err.try_recv() {
            last_stderr = Some(line);
        }
        if parsed.is_none() {
            while let Ok(line) = rx.try_recv() {
                if let Some(snapshot) = parse_rate_limit_snapshot_line(&line, &account.id) {
                    parsed = Some(snapshot);
                    break;
                }
            }
        }
        if let Some(snapshot) = parsed.clone() {
            terminate_child(&mut child);
            return Ok(snapshot);
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|err| AppError::new("CODEX_APP_SERVER_WAIT_FAILED", err.to_string()))?
        {
            let _ = child.wait();
            let stderr_hint = last_stderr
                .as_deref()
                .map(|line| format!(" ({line})"))
                .unwrap_or_default();
            return Err(if status.success() {
                AppError::new(
                    "CODEX_APP_SERVER_PROTOCOL_UNRESOLVED",
                    format!(
                        "app-server exited before a rate-limit response was parsed{}",
                        stderr_hint
                    ),
                )
            } else {
                AppError::new(
                    "CODEX_APP_SERVER_FAILED",
                    format!(
                        "app-server exited before quota could be read{}",
                        stderr_hint
                    ),
                )
            });
        }
        if std::time::Instant::now() > deadline {
            terminate_child(&mut child);
            let stderr_hint = last_stderr
                .as_deref()
                .map(|line| format!(" ({line})"))
                .unwrap_or_default();
            return Err(AppError::new(
                "CODEX_APP_SERVER_TIMEOUT",
                format!("app-server quota request timed out{}", stderr_hint),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

fn try_chatgpt_usage_quota(account: &CodexAccount) -> Result<Option<UsageQuotaSnapshot>> {
    let auth_f_path = account.codex_home.join("auth-f.json");
    if !auth_f_path.exists() {
        return Ok(None);
    }
    let auth_content = fs::read_to_string(&auth_f_path).map_err(|err| {
        AppError::new(
            "QUOTA_AUTH_READ_FAILED",
            format!("Failed to read auth-f.json: {err}"),
        )
    })?;
    let auth_json: Value = serde_json::from_str(&auth_content).map_err(|err| {
        AppError::new("QUOTA_AUTH_INVALID", format!("Invalid auth-f.json: {err}"))
    })?;
    let Some(access_token) = auth_string_alias(&auth_json, &["access_token", "accessToken"]) else {
        return Ok(None);
    };
    let curl_config =
        std::env::temp_dir().join(format!("lam-chatgpt-usage-{}.curl", uuid::Uuid::new_v4()));
    write_file_private(
        &curl_config,
        &format!(
            "header = \"Authorization: Bearer {}\"\n",
            access_token.replace('"', "\\\"")
        ),
    )?;
    let output = Command::new("curl")
        .args([
            "-sS",
            "--fail",
            "--max-time",
            "12",
            "--config",
            &curl_config.to_string_lossy(),
            "https://chatgpt.com/backend-api/wham/usage",
        ])
        .output();
    let _ = fs::remove_file(&curl_config);
    let output =
        output.map_err(|err| AppError::new("CHATGPT_USAGE_UNAVAILABLE", err.to_string()))?;
    if !output.status.success() {
        return Err(AppError::new(
            "CHATGPT_USAGE_FAILED",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let value: Value = serde_json::from_slice(&output.stdout).map_err(|err| {
        AppError::new(
            "CHATGPT_USAGE_INVALID",
            format!("Invalid ChatGPT usage response: {err}"),
        )
    })?;
    Ok(Some(parse_chatgpt_usage_snapshot(&value, &account.id)?))
}

fn parse_chatgpt_usage_snapshot(value: &Value, profile_id: &str) -> Result<UsageQuotaSnapshot> {
    let rate_limit = value
        .get("rate_limit")
        .or_else(|| value.get("rateLimit"))
        .ok_or_else(|| AppError::new("CHATGPT_USAGE_INVALID", "missing rate_limit"))?;
    let primary = find_window(rate_limit, &["primary", "primary_window"])
        .ok_or_else(|| AppError::new("CHATGPT_USAGE_INVALID", "missing primary rate limit"))?;
    let primary_used = extract_percent(primary)
        .ok_or_else(|| AppError::new("CHATGPT_USAGE_INVALID", "missing primary used percent"))?;
    let secondary = find_window(rate_limit, &["secondary", "secondary_window"]);
    let secondary_used = secondary.and_then(extract_percent);
    let plan_type = extract_string(value, &["plan_type", "planType"]);

    Ok(UsageQuotaSnapshot {
        profile_id: profile_id.to_string(),
        source: "chatgpt_wham_usage".into(),
        fetched_at: system_secs(SystemTime::now()),
        staleness: "fresh".into(),
        plan_type,
        activity_tokens: None,
        primary_used_percent: Some(primary_used),
        primary_window_duration_mins: extract_window_duration_mins(primary),
        secondary_used_percent: secondary_used,
        secondary_window_duration_mins: secondary.and_then(extract_window_duration_mins),
        remaining_percent: Some(100_u8.saturating_sub(primary_used)),
        reset_at: extract_reset(primary),
        secondary_reset_at: secondary.and_then(extract_reset),
        alerts: Vec::new(),
        suggested_actions: if primary_used >= 90 {
            vec!["Session quota is high; switch profile or use relay.".into()]
        } else {
            Vec::new()
        },
    })
}

fn prepare_quota_auth_home(account: &CodexAccount) -> Result<Option<QuotaAuthHome>> {
    let auth_f_path = account.codex_home.join("auth-f.json");
    match auth_f_path.try_exists() {
        Ok(false) => return Ok(None),
        Ok(true) => {}
        Err(err) => {
            return Err(AppError::new(
                "QUOTA_AUTH_METADATA_FAILED",
                format!("Failed to inspect auth-f.json: {err}"),
            ));
        }
    }

    let auth_content = fs::read_to_string(&auth_f_path).map_err(|err| {
        AppError::new(
            "QUOTA_AUTH_READ_FAILED",
            format!("Failed to read auth-f.json: {err}"),
        )
    })?;
    let auth_json: Value = serde_json::from_str(&auth_content).map_err(|err| {
        AppError::new("QUOTA_AUTH_INVALID", format!("Invalid auth-f.json: {err}"))
    })?;
    if !auth_json.is_object() {
        return Err(AppError::new(
            "QUOTA_AUTH_INVALID",
            "auth-f.json must contain a JSON object",
        ));
    }

    let path = std::env::temp_dir().join(format!(
        "lam-quota-auth-{}-{}",
        account.id,
        uuid::Uuid::new_v4()
    ));
    fs::create_dir(&path).map_err(|err| {
        AppError::new(
            "QUOTA_AUTH_STAGE_FAILED",
            format!("Failed to create quota auth home: {err}"),
        )
    })?;
    set_dir_private(&path)?;
    let staged_auth = normalize_quota_auth_json(auth_json)?;
    write_file_private(&path.join("auth.json"), &staged_auth)?;
    Ok(Some(QuotaAuthHome { path }))
}

fn normalize_quota_auth_json(mut auth_json: Value) -> Result<String> {
    if auth_json.get("tokens").is_none() {
        let mut tokens = serde_json::Map::new();
        for (target, aliases) in [
            ("id_token", &["id_token", "idToken"][..]),
            ("access_token", &["access_token", "accessToken"][..]),
            ("refresh_token", &["refresh_token", "refreshToken"][..]),
            (
                "account_id",
                &["account_id", "accountId", "chatgpt_account_id"][..],
            ),
        ] {
            if let Some(value) = auth_string_alias(&auth_json, aliases) {
                tokens.insert(target.to_string(), Value::String(value));
            }
        }
        if !tokens.is_empty() {
            auth_json["tokens"] = Value::Object(tokens);
        }
    }
    if auth_json.get("last_refresh").is_none() {
        if let Some(value) = auth_string_alias(&auth_json, &["lastRefresh"]) {
            auth_json["last_refresh"] = Value::String(value);
        }
    }
    if auth_json.get("OPENAI_API_KEY").is_none() {
        auth_json["OPENAI_API_KEY"] = Value::Null;
    }
    if auth_json.get("auth_mode").is_none() {
        auth_json["auth_mode"] = Value::String("chatgpt".to_string());
    }
    serde_json::to_string_pretty(&auth_json).map_err(|err| {
        AppError::new(
            "QUOTA_AUTH_INVALID",
            format!("Failed to serialize quota auth: {err}"),
        )
    })
}

fn auth_string_alias(auth: &Value, aliases: &[&str]) -> Option<String> {
    aliases
        .iter()
        .find_map(|alias| auth.get(*alias))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn spawn_codex_app_server(home_root: &Path, codex_home: &Path) -> Result<Child> {
    // Always launch codex via login shell so that the user's full PATH (including
    // node, bun, etc.) is available.  DMG-installed apps inherit only a minimal
    // PATH from macOS (/usr/bin:/bin:/usr/sbin:/sbin) which lacks the node runtime
    // required by the codex CLI script (#!/usr/bin/env node).
    let path_env = codex_app_server_path(home_root)?;
    let path_prefix = format!(
        "export PATH={}:\"$PATH\"; ",
        shell_quote(&path_env.to_string_lossy())
    );
    let shell_arg = if let Some(codex_bin) = resolve_codex_bin(home_root) {
        format!(
            "{}exec {} app-server",
            path_prefix,
            shell_quote(&codex_bin.to_string_lossy())
        )
    } else {
        format!("{}exec codex app-server", path_prefix)
    };

    let shell = std::env::var("LAM_CODEX_LOGIN_SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let mut command = Command::new(shell);
    command.args(["-lc", &shell_arg]);

    command
        .env("PATH", path_env)
        .env("CODEX_HOME", codex_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| AppError::new("CODEX_APP_SERVER_UNAVAILABLE", err.to_string()))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn parse_rate_limit_snapshot_line(line: &str, profile_id: &str) -> Option<UsageQuotaSnapshot> {
    let value: Value = serde_json::from_str(line).ok()?;
    let result = value.get("result").unwrap_or(&value);
    let primary = find_window(result, &["primary", "primary_window"])?;
    let secondary = find_window(result, &["secondary", "secondary_window"]);
    let primary_used = extract_percent(primary)?;
    let secondary_used = secondary.and_then(extract_percent);
    let primary_window_duration_mins = extract_window_duration_mins(primary);
    let secondary_window_duration_mins = secondary.and_then(extract_window_duration_mins);
    let reset_at = extract_reset(primary);
    let secondary_reset_at = secondary.and_then(extract_reset);
    let plan_type = extract_string(result, &["plan_type", "planType"]).or_else(|| {
        result
            .get("rateLimits")
            .or_else(|| result.get("rate_limits"))
            .and_then(|value| extract_string(value, &["plan_type", "planType"]))
    });

    Some(UsageQuotaSnapshot {
        profile_id: profile_id.to_string(),
        source: "app_server_rate_limits".into(),
        fetched_at: system_secs(SystemTime::now()),
        staleness: "fresh".into(),
        plan_type,
        activity_tokens: None,
        primary_used_percent: Some(primary_used),
        primary_window_duration_mins,
        secondary_used_percent: secondary_used,
        secondary_window_duration_mins,
        remaining_percent: Some(100_u8.saturating_sub(primary_used)),
        reset_at,
        secondary_reset_at,
        alerts: Vec::new(),
        suggested_actions: if primary_used >= 90 {
            vec!["Session quota is high; switch profile or use relay.".into()]
        } else {
            Vec::new()
        },
    })
}

fn find_window<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    for key in keys {
        if let Some(window) = value.get(key) {
            return Some(window);
        }
    }
    if let Some(rate_limit) = value.get("rateLimit").or_else(|| value.get("rate_limit")) {
        for key in keys {
            if let Some(window) = rate_limit.get(key) {
                return Some(window);
            }
        }
    }
    if let Some(rate_limits_obj) = value
        .get("rateLimits")
        .or_else(|| value.get("rate_limits"))
        .and_then(|v| v.as_object())
    {
        for key in keys {
            if let Some(window) = rate_limits_obj.get(*key) {
                return Some(window);
            }
        }
    }
    if let Some(limits) = value
        .get("rateLimits")
        .or_else(|| value.get("rate_limits"))
        .and_then(|v| v.as_array())
    {
        for item in limits {
            for key in keys {
                if let Some(window) = item.get(key) {
                    return Some(window);
                }
            }
        }
    }
    None
}

fn extract_window_duration_mins(value: &Value) -> Option<u64> {
    value
        .get("windowDurationMins")
        .or_else(|| value.get("window_duration_mins"))
        .or_else(|| value.get("windowDurationMinutes"))
        .or_else(|| value.get("limit_window_seconds"))
        .and_then(|v| {
            if let Some(raw) = v.as_u64() {
                if value.get("limit_window_seconds").is_some() {
                    return Some(raw / 60);
                }
                return Some(raw);
            }
            if let Some(raw) = v.as_f64() {
                if raw.is_finite() && raw >= 0.0 {
                    if value.get("limit_window_seconds").is_some() {
                        return Some((raw / 60.0).round() as u64);
                    }
                    return Some(raw.round() as u64);
                }
            }
            None
        })
}

fn extract_percent(value: &Value) -> Option<u8> {
    let raw = value
        .get("used_percent")
        .or_else(|| value.get("usedPercent"))
        .or_else(|| value.get("used_percentage"))?;
    if let Some(v) = raw.as_u64() {
        return Some((v.min(100)) as u8);
    }
    if let Some(v) = raw.as_f64() {
        return Some(v.round().clamp(0.0, 100.0) as u8);
    }
    None
}

fn extract_reset(value: &Value) -> Option<String> {
    value
        .get("reset_at")
        .or_else(|| value.get("resetAt"))
        .or_else(|| value.get("resetsAt"))
        .and_then(|v| {
            if let Some(s) = v.as_str() {
                return Some(s.to_string());
            }
            if let Some(epoch) = v.as_i64() {
                return Some(epoch.to_string());
            }
            if let Some(epoch) = v.as_u64() {
                return Some(epoch.to_string());
            }
            None
        })
}

fn extract_string(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(s) = value.get(key).and_then(|v| v.as_str()) {
            return Some(s.to_string());
        }
    }
    None
}

fn quota_cache_dir(home_root: &Path) -> std::path::PathBuf {
    config_root(home_root).join("quota-cache")
}

fn write_quota_cache(home_root: &Path, snapshot: &UsageQuotaSnapshot) -> Result<()> {
    let path = quota_cache_dir(home_root).join(format!("{}.json", snapshot.profile_id));
    let body = serde_json::to_string_pretty(snapshot)
        .map_err(|err| AppError::new("QUOTA_CACHE_INVALID", err.to_string()))?;
    write_file_private(&path, &format!("{body}\n"))
}

fn read_quota_cache(home_root: &Path, profile_id: &str) -> Result<Option<UsageQuotaSnapshot>> {
    let path = quota_cache_dir(home_root).join(format!("{profile_id}.json"));
    if !path.exists() {
        return Ok(None);
    }
    let body = fs::read_to_string(path)?;
    let mut snapshot: UsageQuotaSnapshot = serde_json::from_str(&body)
        .map_err(|err| AppError::new("QUOTA_CACHE_INVALID", err.to_string()))?;
    if snapshot.source != "app_server_rate_limits" && snapshot.source != "chatgpt_wham_usage" {
        return Ok(None);
    }
    snapshot.staleness = "cached".into();
    Ok(Some(snapshot))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quota_auth_normalizes_chatgpt_session_tokens() {
        let auth = serde_json::json!({
            "accessToken": "at-session",
            "idToken": "id-session",
            "refreshToken": "rt-session",
            "accountId": "account-session",
            "lastRefresh": "2026-06-24T00:00:00+00:00",
            "user": {"email": "yas@example.com"}
        });

        let normalized: Value =
            serde_json::from_str(&normalize_quota_auth_json(auth).unwrap()).unwrap();

        assert_eq!(normalized["tokens"]["access_token"], "at-session");
        assert_eq!(normalized["tokens"]["id_token"], "id-session");
        assert_eq!(normalized["tokens"]["refresh_token"], "rt-session");
        assert_eq!(normalized["tokens"]["account_id"], "account-session");
        assert_eq!(normalized["last_refresh"], "2026-06-24T00:00:00+00:00");
        assert_eq!(normalized["OPENAI_API_KEY"], Value::Null);
        assert_eq!(normalized["auth_mode"], "chatgpt");
        assert_eq!(normalized["accessToken"], "at-session");
    }

    #[test]
    fn quota_auth_preserves_existing_tokens() {
        let auth = serde_json::json!({
            "tokens": {"access_token": "at-existing"},
            "accessToken": "at-session"
        });

        let normalized: Value =
            serde_json::from_str(&normalize_quota_auth_json(auth).unwrap()).unwrap();

        assert_eq!(normalized["tokens"]["access_token"], "at-existing");
    }

    #[test]
    fn parses_chatgpt_wham_usage_quota() {
        let usage = serde_json::json!({
            "plan_type": "plus",
            "rate_limit": {
                "primary_window": {
                    "used_percent": 72,
                    "limit_window_seconds": 18000,
                    "reset_at": 1782553772
                },
                "secondary_window": {
                    "used_percent": 28,
                    "limit_window_seconds": 604800,
                    "reset_at": 1782847747
                }
            }
        });

        let snapshot = parse_chatgpt_usage_snapshot(&usage, "Yas").unwrap();

        assert_eq!(snapshot.source, "chatgpt_wham_usage");
        assert_eq!(snapshot.staleness, "fresh");
        assert_eq!(snapshot.plan_type.as_deref(), Some("plus"));
        assert_eq!(snapshot.primary_used_percent, Some(72));
        assert_eq!(snapshot.primary_window_duration_mins, Some(300));
        assert_eq!(snapshot.secondary_used_percent, Some(28));
        assert_eq!(snapshot.secondary_window_duration_mins, Some(10080));
        assert_eq!(snapshot.remaining_percent, Some(28));
    }
}
