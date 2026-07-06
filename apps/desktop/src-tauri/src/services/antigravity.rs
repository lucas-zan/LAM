use crate::AppError;
use serde::{Deserialize, Serialize};
use std::process::Command;

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
    /// true = standalone Antigravity app, false = IDE extension
    is_standalone: bool,
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
    let output = Command::new("lsof")
        .args(["-Pan", "-p", &pid.to_string(), "-i"])
        .output()
        .map_err(|err| AppError::new("PORT_SCAN_FAILED", format!("Failed to run lsof: {}", err)))?;

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

pub fn get_live_antigravity_quota() -> Result<AntigravityQuotaResponse, AppError> {
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
    for proc in processes {
        let ports = match find_listening_ports(proc.pid) {
            Ok(res) => res,
            Err(err) => {
                last_err = Some(err);
                continue;
            }
        };

        if ports.is_empty() {
            last_err = Some(AppError::new(
                "NO_PORTS_FOUND",
                format!(
                    "Process {} (standalone={}) is not listening on any ports",
                    proc.pid, proc.is_standalone
                ),
            ));
            continue;
        }

        for port in ports {
            let summary = query_antigravity_quota_summary(port, &proc.csrf_token);
            let models = query_antigravity_quota(port, &proc.csrf_token);

            match (summary, models) {
                (Ok(summary), models_result) => {
                    return Ok(AntigravityQuotaResponse {
                        ok: true,
                        models: models_result.unwrap_or_default(),
                        description: summary.description,
                        groups: summary.groups,
                        error: None,
                    });
                }
                (Err(_summary_err), Ok(models)) => {
                    return Ok(AntigravityQuotaResponse {
                        ok: true,
                        models,
                        description: None,
                        groups: Vec::new(),
                        error: None,
                    });
                }
                (Err(summary_err), Err(model_err)) => {
                    last_err = Some(AppError::new(
                        "ANTIGRAVITY_QUOTA_QUERY_FAILED",
                        format!(
                            "summary failed: {}; models failed: {}",
                            summary_err.message, model_err.message
                        ),
                    ));
                }
            }
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
}
