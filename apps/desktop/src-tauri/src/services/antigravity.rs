use crate::AppError;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, OnceLock, TryLockError};
use std::time::{Duration, Instant};

const LSOF_PORT_SCAN_TIMEOUT: Duration = Duration::from_secs(2);
const FAILED_EXPLICIT_PORT_COOLDOWN: Duration = Duration::from_secs(10 * 60);

static ANTIGRAVITY_REFRESH_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static ANTIGRAVITY_PORT_CACHE: OnceLock<Mutex<Option<AntigravityPortCache>>> = OnceLock::new();
static FAILED_EXPLICIT_PORTS: OnceLock<Mutex<Vec<FailedExplicitPort>>> = OnceLock::new();

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityModelQuota {
    pub label: String,
    pub remaining_fraction: Option<f64>,
    pub reset_time: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityQuotaBucket {
    pub bucket_id: Option<String>,
    pub display_name: String,
    pub description: Option<String>,
    pub window: Option<String>,
    pub remaining_fraction: Option<f64>,
    pub reset_time: Option<String>,
    pub disabled: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityQuotaGroup {
    pub display_name: String,
    pub description: Option<String>,
    pub buckets: Vec<AntigravityQuotaBucket>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityQuotaResponse {
    pub ok: bool,
    pub models: Vec<AntigravityModelQuota>,
    pub description: Option<String>,
    pub groups: Vec<AntigravityQuotaGroup>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
struct AntigravityQuotaSummary {
    description: Option<String>,
    groups: Vec<AntigravityQuotaGroup>,
}

/// A discovered Antigravity language server process.
#[derive(Debug)]
struct AntigravityProcess {
    pid: u32,
    csrf_token: String,
    ports: Vec<u16>,
    /// true = standalone Antigravity app, false = IDE extension
    is_standalone: bool,
}

#[derive(Debug, Clone)]
struct AntigravityPortCache {
    pid: u32,
    csrf_token: String,
    port: u16,
    response: AntigravityQuotaResponse,
}

#[derive(Debug, Clone)]
struct FailedExplicitPort {
    pid: u32,
    csrf_token: String,
    port: u16,
    failed_at: Instant,
}

fn refresh_lock() -> &'static Mutex<()> {
    ANTIGRAVITY_REFRESH_LOCK.get_or_init(|| Mutex::new(()))
}

fn port_cache() -> &'static Mutex<Option<AntigravityPortCache>> {
    ANTIGRAVITY_PORT_CACHE.get_or_init(|| Mutex::new(None))
}

fn failed_explicit_ports() -> &'static Mutex<Vec<FailedExplicitPort>> {
    FAILED_EXPLICIT_PORTS.get_or_init(|| Mutex::new(Vec::new()))
}

fn read_port_cache() -> Option<AntigravityPortCache> {
    port_cache().lock().ok().and_then(|cache| cache.clone())
}

fn store_port_cache(proc: &AntigravityProcess, port: u16, response: &AntigravityQuotaResponse) {
    if let Ok(mut cache) = port_cache().lock() {
        *cache = Some(AntigravityPortCache {
            pid: proc.pid,
            csrf_token: proc.csrf_token.clone(),
            port,
            response: response.clone(),
        });
    }
}

fn clear_port_cache() {
    if let Ok(mut cache) = port_cache().lock() {
        *cache = None;
    }
}

fn cached_port_for_process(proc: &AntigravityProcess, cache: &AntigravityPortCache) -> Option<u16> {
    if proc.pid == cache.pid && proc.csrf_token == cache.csrf_token {
        Some(cache.port)
    } else {
        None
    }
}

fn antigravity_single_flight_response(
    cache: Option<&AntigravityPortCache>,
) -> AntigravityQuotaResponse {
    cache
        .map(|cache| cache.response.clone())
        .unwrap_or_else(|| AntigravityQuotaResponse {
            ok: false,
            models: Vec::new(),
            description: None,
            groups: Vec::new(),
            error: Some("Antigravity refresh already in progress".to_string()),
        })
}

fn filter_available_explicit_ports(
    pid: u32,
    csrf_token: &str,
    ports: &[u16],
    failures: &[FailedExplicitPort],
    now: Instant,
    cooldown: Duration,
) -> Vec<u16> {
    ports
        .iter()
        .copied()
        .filter(|port| {
            !failures.iter().any(|failure| {
                failure.pid == pid
                    && failure.csrf_token == csrf_token
                    && failure.port == *port
                    && failure.failed_at + cooldown > now
            })
        })
        .collect()
}

fn current_available_explicit_ports(proc: &AntigravityProcess) -> Vec<u16> {
    let failures = failed_explicit_ports()
        .lock()
        .map(|failures| failures.clone())
        .unwrap_or_default();
    filter_available_explicit_ports(
        proc.pid,
        &proc.csrf_token,
        &proc.ports,
        &failures,
        Instant::now(),
        FAILED_EXPLICIT_PORT_COOLDOWN,
    )
}

fn mark_failed_explicit_ports(proc: &AntigravityProcess, ports: &[u16]) {
    if ports.is_empty() {
        return;
    }
    let now = Instant::now();
    if let Ok(mut failures) = failed_explicit_ports().lock() {
        failures.retain(|failure| failure.failed_at + FAILED_EXPLICIT_PORT_COOLDOWN > now);
        for port in ports {
            if let Some(existing) = failures.iter_mut().find(|failure| {
                failure.pid == proc.pid
                    && failure.csrf_token == proc.csrf_token
                    && failure.port == *port
            }) {
                existing.failed_at = now;
            } else {
                failures.push(FailedExplicitPort {
                    pid: proc.pid,
                    csrf_token: proc.csrf_token.clone(),
                    port: *port,
                    failed_at: now,
                });
            }
        }
    }
}

fn extract_arg_value(cmd: &str, arg: &str) -> Option<String> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    for i in 0..parts.len() {
        if parts[i] == arg && i + 1 < parts.len() {
            return Some(parts[i + 1].to_string());
        }
        if parts[i].starts_with(arg) && parts[i].contains('=') {
            let subparts: Vec<&str> = parts[i].split('=').collect();
            if subparts.len() > 1 {
                return Some(subparts[1].to_string());
            }
        }
    }
    None
}

fn extract_antigravity_ports(cmd: &str) -> Vec<u16> {
    let mut ports = Vec::new();
    for arg in [
        "--https_server_port",
        "--http_server_port",
        "--extension_server_port",
    ] {
        collect_arg_ports(cmd, arg, &mut ports);
    }
    ports
}

fn collect_arg_ports(cmd: &str, arg: &str, ports: &mut Vec<u16>) {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    for i in 0..parts.len() {
        let value = if parts[i] == arg && i + 1 < parts.len() {
            Some(parts[i + 1])
        } else {
            parts[i].strip_prefix(&format!("{arg}="))
        };

        let Some(value) = value else {
            continue;
        };
        let Ok(port) = value.parse::<u16>() else {
            continue;
        };
        if port != 0 && !ports.contains(&port) {
            ports.push(port);
        }
    }
}

fn run_command_with_timeout(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<Output, AppError> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            AppError::new(
                "PORT_SCAN_FAILED",
                format!("Failed to run {program}: {err}"),
            )
        })?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_reader = stdout.map(|mut stdout| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stdout.read_to_end(&mut bytes);
            bytes
        })
    });
    let stderr_reader = stderr.map(|mut stderr| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stderr.read_to_end(&mut bytes);
            bytes
        })
    });

    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().map_err(|err| {
            AppError::new(
                "PORT_SCAN_FAILED",
                format!("Failed to wait for {program}: {err}"),
            )
        })? {
            let stdout = stdout_reader
                .and_then(|reader| reader.join().ok())
                .unwrap_or_default();
            let stderr = stderr_reader
                .and_then(|reader| reader.join().ok())
                .unwrap_or_default();
            return Ok(Output {
                status,
                stdout,
                stderr,
            });
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.and_then(|reader| reader.join().ok());
            let _ = stderr_reader.and_then(|reader| reader.join().ok());
            return Err(AppError::new(
                "PORT_SCAN_TIMEOUT",
                format!("{program} timed out after {}ms", timeout.as_millis()),
            ));
        }

        std::thread::sleep(Duration::from_millis(20));
    }
}

fn find_antigravity_processes() -> Result<Vec<AntigravityProcess>, AppError> {
    let output = Command::new("ps")
        .args(["-ww", "-eo", "pid,args"])
        .output()
        .map_err(|err| {
            AppError::new("PROCESS_SCAN_FAILED", format!("Failed to run ps: {}", err))
        })?;

    if !output.status.success() {
        return Err(AppError::new("PROCESS_SCAN_FAILED", "ps command failed"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut processes = Vec::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.trim().splitn(2, char::is_whitespace).collect();
        if parts.len() < 2 {
            continue;
        }

        let pid: u32 = match parts[0].trim().parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let cmd = parts[1].trim();

        // Must contain a csrf_token argument to be a usable language server
        let token = match extract_arg_value(cmd, "--csrf_token") {
            Some(t) => t,
            None => continue,
        };

        // Check if this is an Antigravity-related language server process
        let lower_cmd = cmd.to_lowercase();
        let is_language_server =
            lower_cmd.contains("language_server") || lower_cmd.contains("language_server_macos");

        if !is_language_server {
            continue;
        }

        // Determine if it's the standalone app or IDE extension
        let is_standalone = cmd.contains("--standalone")
            || lower_cmd.contains("/antigravity.app/")
            || (cmd.contains("--app_data_dir")
                && cmd.contains("--app_data_dir antigravity ")
                && !cmd.contains("--app_data_dir antigravity-ide"));

        if !processes.iter().any(|p: &AntigravityProcess| p.pid == pid) {
            processes.push(AntigravityProcess {
                pid,
                csrf_token: token,
                ports: extract_antigravity_ports(cmd),
                is_standalone,
            });
        }
    }

    // Sort: standalone processes first (preferred), then IDE extensions
    processes.sort_by(|a, b| b.is_standalone.cmp(&a.is_standalone));

    if processes.is_empty() {
        return Err(AppError::new(
            "ANTIGRAVITY_PROCESS_NOT_FOUND",
            "Antigravity language server process not found",
        ));
    }

    Ok(processes)
}

fn find_listening_ports(pid: u32) -> Result<Vec<u16>, AppError> {
    let output = run_command_with_timeout(
        "lsof",
        &["-Pan", "-p", &pid.to_string(), "-i"],
        LSOF_PORT_SCAN_TIMEOUT,
    )?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut ports = Vec::new();
    for line in stdout.lines() {
        if !line.contains("(LISTEN)") || !line.contains("127.0.0.1:") {
            continue;
        }

        if let Some(pos) = line.find("127.0.0.1:") {
            let rest = &line[pos + "127.0.0.1:".len()..];
            let port_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(port) = port_str.parse::<u16>() {
                if !ports.contains(&port) {
                    ports.push(port);
                }
            }
        }
    }

    Ok(ports)
}

/// POST a JSON payload to the Antigravity language server.
/// Tries HTTPS first (with -k for self-signed certs), then falls back to HTTP.
fn post_language_server_json(
    port: u16,
    csrf_token: &str,
    method: &str,
) -> Result<String, AppError> {
    let schemes = ["https", "http"];
    let mut last_err = None;

    for scheme in &schemes {
        let url = format!(
            "{}://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/{}",
            scheme, port, method
        );

        let mut cmd = Command::new("curl");
        if *scheme == "https" {
            cmd.arg("-k"); // allow self-signed certs
        }
        cmd.arg("-s")
            .arg("--noproxy")
            .arg("*")
            .arg("--connect-timeout")
            .arg("3")
            .arg("--max-time")
            .arg("8")
            .arg("-X")
            .arg("POST")
            .arg("-H")
            .arg("Content-Type: application/json")
            .arg("-H")
            .arg("Connect-Protocol-Version: 1")
            .arg("-H")
            .arg(format!("X-Codeium-Csrf-Token: {}", csrf_token))
            .arg("-d")
            .arg("{}")
            .arg(&url);

        let output = match cmd.output() {
            Ok(o) => o,
            Err(err) => {
                last_err = Some(AppError::new(
                    "CURL_FAILED",
                    format!("Failed to execute curl: {}", err),
                ));
                continue;
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            last_err = Some(AppError::new(
                "CURL_HTTP_ERROR",
                format!(
                    "curl {} exited with code {}. stderr: {}",
                    scheme,
                    output.status.code().unwrap_or(-1),
                    stderr.trim()
                ),
            ));
            continue;
        }

        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }

    Err(last_err.unwrap_or_else(|| {
        AppError::new(
            "UNKNOWN",
            format!("Failed to query {} via any scheme", method),
        )
    }))
}

fn parse_antigravity_model_quota_response(
    body: &str,
) -> Result<Vec<AntigravityModelQuota>, AppError> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|err| {
        AppError::new(
            "PARSE_JSON_FAILED",
            format!(
                "Failed to parse GetUserStatus JSON: {}. Body (first 200 chars): {}",
                err,
                &body[..body.len().min(200)]
            ),
        )
    })?;

    let configs = v
        .pointer("/userStatus/cascadeModelConfigData/clientModelConfigs")
        .or_else(|| v.pointer("/cascadeModelConfigData/clientModelConfigs"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            AppError::new(
                "NO_MODELS_FOUND",
                format!(
                    "No clientModelConfigs in GetUserStatus response. Top-level keys: {:?}",
                    v.as_object()
                        .map(|o| o.keys().collect::<Vec<_>>())
                        .unwrap_or_default()
                ),
            )
        })?;

    let models: Vec<_> = configs.iter().filter_map(parse_model_quota).collect();

    if models.is_empty() {
        return Err(AppError::new(
            "NO_MODELS_FOUND",
            "clientModelConfigs array was empty",
        ));
    }

    Ok(models)
}

fn parse_model_quota(config: &serde_json::Value) -> Option<AntigravityModelQuota> {
    let label = config.get("label")?.as_str()?.to_string();
    if label.is_empty() {
        return None;
    }

    Some(AntigravityModelQuota {
        label,
        remaining_fraction: config
            .pointer("/quotaInfo/remainingFraction")
            .and_then(|v| v.as_f64()),
        reset_time: config
            .pointer("/quotaInfo/resetTime")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    })
}

fn parse_quota_summary_response(body: &str) -> Result<AntigravityQuotaSummary, AppError> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|err| {
        AppError::new(
            "PARSE_JSON_FAILED",
            format!(
                "Failed to parse RetrieveUserQuotaSummary JSON: {}. Body (first 200 chars): {}",
                err,
                &body[..body.len().min(200)]
            ),
        )
    })?;

    let description = v
        .pointer("/response/description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let groups = v
        .pointer("/response/groups")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AppError::new("NO_QUOTA_GROUPS_FOUND", "No quota groups found"))?;

    let groups: Vec<_> = groups.iter().filter_map(parse_quota_group).collect();
    if groups.is_empty() {
        return Err(AppError::new(
            "NO_QUOTA_GROUPS_FOUND",
            "Quota groups were empty",
        ));
    }

    Ok(AntigravityQuotaSummary {
        description,
        groups,
    })
}

fn parse_quota_group(group: &serde_json::Value) -> Option<AntigravityQuotaGroup> {
    let display_name = group.get("displayName")?.as_str()?.to_string();
    let buckets = group
        .get("buckets")
        .and_then(|v| v.as_array())?
        .iter()
        .filter_map(parse_quota_bucket)
        .collect::<Vec<_>>();

    if display_name.is_empty() || buckets.is_empty() {
        return None;
    }

    Some(AntigravityQuotaGroup {
        display_name,
        description: optional_string(group, "description"),
        buckets,
    })
}

fn parse_quota_bucket(bucket: &serde_json::Value) -> Option<AntigravityQuotaBucket> {
    let display_name = bucket.get("displayName")?.as_str()?.to_string();
    if display_name.is_empty() {
        return None;
    }

    Some(AntigravityQuotaBucket {
        bucket_id: optional_string(bucket, "bucketId"),
        display_name,
        description: optional_string(bucket, "description"),
        window: optional_string(bucket, "window"),
        remaining_fraction: bucket.get("remainingFraction").and_then(|v| v.as_f64()),
        reset_time: optional_string(bucket, "resetTime"),
        disabled: bucket.get("disabled").and_then(|v| v.as_bool()),
    })
}

fn optional_string(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn query_antigravity_quota(
    port: u16,
    csrf_token: &str,
) -> Result<Vec<AntigravityModelQuota>, AppError> {
    let body = post_language_server_json(port, csrf_token, "GetUserStatus")?;
    parse_antigravity_model_quota_response(&body)
}

fn query_antigravity_quota_summary(
    port: u16,
    csrf_token: &str,
) -> Result<AntigravityQuotaSummary, AppError> {
    let body = post_language_server_json(port, csrf_token, "RetrieveUserQuotaSummary")?;
    parse_quota_summary_response(&body)
}

fn query_ports_for_quota(
    ports: &[u16],
    csrf_token: &str,
    last_err: &mut Option<AppError>,
) -> Option<(u16, AntigravityQuotaResponse)> {
    for port in ports {
        let summary = query_antigravity_quota_summary(*port, csrf_token);
        let models = query_antigravity_quota(*port, csrf_token);

        match (summary, models) {
            (Ok(summary), models_result) => {
                return Some((
                    *port,
                    AntigravityQuotaResponse {
                        ok: true,
                        models: models_result.unwrap_or_default(),
                        description: summary.description,
                        groups: summary.groups,
                        error: None,
                    },
                ));
            }
            (Err(_summary_err), Ok(models)) => {
                return Some((
                    *port,
                    AntigravityQuotaResponse {
                        ok: true,
                        models,
                        description: None,
                        groups: Vec::new(),
                        error: None,
                    },
                ));
            }
            (Err(summary_err), Err(model_err)) => {
                *last_err = Some(AppError::new(
                    "ANTIGRAVITY_QUOTA_QUERY_FAILED",
                    format!(
                        "summary failed: {}; models failed: {}",
                        summary_err.message, model_err.message
                    ),
                ));
            }
        }
    }
    None
}

pub fn get_live_antigravity_quota() -> Result<AntigravityQuotaResponse, AppError> {
    let _refresh_guard = match refresh_lock().try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => {
            let cache = read_port_cache();
            return Ok(antigravity_single_flight_response(cache.as_ref()));
        }
        Err(TryLockError::Poisoned(_)) => {
            return Ok(AntigravityQuotaResponse {
                ok: false,
                models: Vec::new(),
                description: None,
                groups: Vec::new(),
                error: Some("Antigravity refresh lock poisoned".to_string()),
            });
        }
    };
    get_live_antigravity_quota_inner()
}

fn get_live_antigravity_quota_inner() -> Result<AntigravityQuotaResponse, AppError> {
    let processes = match find_antigravity_processes() {
        Ok(res) => res,
        Err(err) => {
            return Ok(AntigravityQuotaResponse {
                ok: false,
                models: Vec::new(),
                description: None,
                groups: Vec::new(),
                error: Some(format!("Failed to find process: {}", err.message)),
            });
        }
    };

    let mut last_err = None;
    if let Some(cache) = read_port_cache() {
        for proc in &processes {
            if let Some(port) = cached_port_for_process(proc, &cache) {
                if let Some((port, response)) =
                    query_ports_for_quota(&[port], &proc.csrf_token, &mut last_err)
                {
                    store_port_cache(proc, port, &response);
                    return Ok(response);
                }
                clear_port_cache();
                break;
            }
        }
    }

    for proc in processes {
        let explicit_ports = current_available_explicit_ports(&proc);
        if !explicit_ports.is_empty() {
            if let Some((port, response)) =
                query_ports_for_quota(&explicit_ports, &proc.csrf_token, &mut last_err)
            {
                store_port_cache(&proc, port, &response);
                return Ok(response);
            }
            mark_failed_explicit_ports(&proc, &explicit_ports);
        }

        let ports = match find_listening_ports(proc.pid) {
            Ok(res) => res,
            Err(err) => {
                if proc.ports.is_empty() {
                    last_err = Some(err);
                }
                continue;
            }
        };

        if ports.is_empty() {
            if proc.ports.is_empty() {
                last_err = Some(AppError::new(
                    "NO_PORTS_FOUND",
                    format!(
                        "Process {} (standalone={}) is not listening on any ports",
                        proc.pid, proc.is_standalone
                    ),
                ));
            }
            continue;
        }

        if let Some((port, response)) =
            query_ports_for_quota(&ports, &proc.csrf_token, &mut last_err)
        {
            store_port_cache(&proc, port, &response);
            return Ok(response);
        }
    }

    Ok(AntigravityQuotaResponse {
        ok: false,
        models: Vec::new(),
        description: None,
        groups: Vec::new(),
        error: Some(format!(
            "Failed to query all ports. Last error: {}",
            last_err
                .map(|e| e.message)
                .unwrap_or_else(|| "Unknown".to_string())
        )),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn antigravity_quota_summary_parses_weekly_and_five_hour_buckets() {
        let body = r#"{
          "response": {
            "description": "Within each group, models share a weekly limit and a 5-hour limit.",
            "groups": [
              {
                "displayName": "Gemini Models",
                "description": "Models within this group: Gemini Flash, Gemini Pro",
                "buckets": [
                  {
                    "bucketId": "gemini-weekly",
                    "displayName": "Weekly Limit",
                    "description": "Refreshes in 3 days",
                    "window": "weekly",
                    "remainingFraction": 0.91,
                    "resetTime": "2026-07-07T01:21:15Z"
                  },
                  {
                    "bucketId": "gemini-5h",
                    "displayName": "5h",
                    "window": "5h",
                    "remainingFraction": 0.82,
                    "resetTime": "2026-07-03T07:06:36Z"
                  }
                ]
              }
            ]
          }
        }"#;

        let summary = parse_quota_summary_response(body).unwrap();

        assert_eq!(
            summary.description.as_deref(),
            Some("Within each group, models share a weekly limit and a 5-hour limit.")
        );
        assert_eq!(summary.groups.len(), 1);
        assert_eq!(summary.groups[0].display_name, "Gemini Models");
        assert_eq!(summary.groups[0].buckets.len(), 2);
        assert_eq!(summary.groups[0].buckets[0].display_name, "Weekly Limit");
        assert_eq!(
            summary.groups[0].buckets[0].window.as_deref(),
            Some("weekly")
        );
        assert_eq!(summary.groups[0].buckets[1].display_name, "5h");
        assert_eq!(summary.groups[0].buckets[1].remaining_fraction, Some(0.82));
    }

    #[test]
    fn antigravity_quota_summary_reports_missing_groups() {
        let err = parse_quota_summary_response(r#"{"response":{}}"#).unwrap_err();

        assert_eq!(err.code, "NO_QUOTA_GROUPS_FOUND");
    }

    #[test]
    fn antigravity_command_timeout_kills_slow_process() {
        let err = run_command_with_timeout(
            "sh",
            &["-c", "sleep 2"],
            std::time::Duration::from_millis(50),
        )
        .unwrap_err();

        assert_eq!(err.code, "PORT_SCAN_TIMEOUT");
    }

    #[test]
    fn antigravity_command_timeout_captures_fast_stdout() {
        let output = run_command_with_timeout(
            "sh",
            &["-c", "printf 'ready'"],
            std::time::Duration::from_secs(1),
        )
        .unwrap();

        assert_eq!(String::from_utf8_lossy(&output.stdout), "ready");
    }

    #[test]
    fn antigravity_explicit_port_parses_space_and_equals_forms() {
        let ports = extract_antigravity_ports(
            "--https_server_port 57362 --http_server_port=57363 --extension_server_port 57364",
        );

        assert_eq!(ports, vec![57362, 57363, 57364]);
    }

    #[test]
    fn antigravity_explicit_port_ignores_zero_invalid_and_duplicates() {
        let ports = extract_antigravity_ports(
            "--https_server_port 0 --http_server_port nope --extension_server_port 57362 --https_server_port=57362",
        );

        assert_eq!(ports, vec![57362]);
    }

    fn sample_antigravity_response() -> AntigravityQuotaResponse {
        AntigravityQuotaResponse {
            ok: true,
            models: vec![AntigravityModelQuota {
                label: "Gemini".to_string(),
                remaining_fraction: Some(0.5),
                reset_time: None,
            }],
            description: None,
            groups: Vec::new(),
            error: None,
        }
    }

    #[test]
    fn antigravity_cache_matches_same_pid_and_token() {
        let proc = AntigravityProcess {
            pid: 42,
            csrf_token: "token-a".to_string(),
            ports: Vec::new(),
            is_standalone: true,
        };
        let cache = AntigravityPortCache {
            pid: 42,
            csrf_token: "token-a".to_string(),
            port: 57362,
            response: sample_antigravity_response(),
        };

        assert_eq!(cached_port_for_process(&proc, &cache), Some(57362));
    }

    #[test]
    fn antigravity_cache_rejects_mismatched_pid_or_token() {
        let cache = AntigravityPortCache {
            pid: 42,
            csrf_token: "token-a".to_string(),
            port: 57362,
            response: sample_antigravity_response(),
        };
        let wrong_pid = AntigravityProcess {
            pid: 43,
            csrf_token: "token-a".to_string(),
            ports: Vec::new(),
            is_standalone: true,
        };
        let wrong_token = AntigravityProcess {
            pid: 42,
            csrf_token: "token-b".to_string(),
            ports: Vec::new(),
            is_standalone: true,
        };

        assert_eq!(cached_port_for_process(&wrong_pid, &cache), None);
        assert_eq!(cached_port_for_process(&wrong_token, &cache), None);
    }

    #[test]
    fn antigravity_single_flight_returns_cached_response_when_available() {
        let cache = AntigravityPortCache {
            pid: 42,
            csrf_token: "token-a".to_string(),
            port: 57362,
            response: sample_antigravity_response(),
        };

        let response = antigravity_single_flight_response(Some(&cache));

        assert!(response.ok);
        assert_eq!(response.models[0].label, "Gemini");
    }

    #[test]
    fn antigravity_single_flight_reports_in_progress_without_cache() {
        let response = antigravity_single_flight_response(None);

        assert!(!response.ok);
        assert_eq!(
            response.error.as_deref(),
            Some("Antigravity refresh already in progress")
        );
    }

    #[test]
    fn antigravity_negative_port_filters_active_failures() {
        let now = Instant::now();
        let failures = vec![FailedExplicitPort {
            pid: 42,
            csrf_token: "token-a".to_string(),
            port: 57362,
            failed_at: now,
        }];

        let ports = filter_available_explicit_ports(
            42,
            "token-a",
            &[57362, 57363],
            &failures,
            now + Duration::from_secs(60),
            Duration::from_secs(600),
        );

        assert_eq!(ports, vec![57363]);
    }

    #[test]
    fn antigravity_negative_port_allows_expired_failures() {
        let now = Instant::now();
        let failures = vec![FailedExplicitPort {
            pid: 42,
            csrf_token: "token-a".to_string(),
            port: 57362,
            failed_at: now,
        }];

        let ports = filter_available_explicit_ports(
            42,
            "token-a",
            &[57362, 57363],
            &failures,
            now + Duration::from_secs(601),
            Duration::from_secs(600),
        );

        assert_eq!(ports, vec![57362, 57363]);
    }
}
