use crate::AppError;
use std::process::Command;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityModelQuota {
    pub label: String,
    pub remaining_fraction: Option<f64>,
    pub reset_time: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AntigravityQuotaResponse {
    pub ok: bool,
    pub models: Vec<AntigravityModelQuota>,
    pub error: Option<String>,
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
        .map_err(|err| AppError::new("PROCESS_SCAN_FAILED", format!("Failed to run ps: {}", err)))?;

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
        let is_language_server = lower_cmd.contains("language_server")
            || lower_cmd.contains("language_server_macos");

        if !is_language_server {
            continue;
        }

        // Determine if it's the standalone app or IDE extension
        let is_standalone = cmd.contains("--standalone")
            || lower_cmd.contains("/antigravity.app/")
            || (cmd.contains("--app_data_dir") && cmd.contains("--app_data_dir antigravity ")
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

/// Try to query the Antigravity language server for model quota info.
/// Tries HTTPS first (with -k for self-signed certs), then falls back to HTTP.
fn query_antigravity_quota(port: u16, csrf_token: &str) -> Result<Vec<AntigravityModelQuota>, AppError> {
    // The language server uses HTTPS with a self-signed certificate.
    // We try HTTPS first; if that fails, we fall back to HTTP.
    let schemes = ["https", "http"];
    let mut last_err = None;

    for scheme in &schemes {
        let url = format!(
            "{}://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/GetUserStatus",
            scheme, port
        );

        let mut cmd = Command::new("curl");
        if *scheme == "https" {
            cmd.arg("-k"); // allow self-signed certs
        }
        cmd.arg("-s")
            .arg("--noproxy").arg("*")
            .arg("--connect-timeout").arg("3")
            .arg("--max-time").arg("8")
            .arg("-X").arg("POST")
            .arg("-H").arg("Content-Type: application/json")
            .arg("-H").arg("Connect-Protocol-Version: 1")
            .arg("-H").arg(format!("X-Codeium-Csrf-Token: {}", csrf_token))
            .arg("-d").arg("{}")
            .arg(&url);

        let output = match cmd.output() {
            Ok(o) => o,
            Err(err) => {
                last_err = Some(AppError::new("CURL_FAILED", format!("Failed to execute curl: {}", err)));
                continue;
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            last_err = Some(AppError::new(
                "CURL_HTTP_ERROR",
                format!("curl {} exited with code {}. stderr: {}", scheme, output.status.code().unwrap_or(-1), stderr.trim()),
            ));
            continue;
        }

        let body = String::from_utf8_lossy(&output.stdout);

        // Try to parse the JSON response
        let v: serde_json::Value = match serde_json::from_str(&body) {
            Ok(val) => val,
            Err(err) => {
                last_err = Some(AppError::new(
                    "PARSE_JSON_FAILED",
                    format!("Failed to parse JSON from {} response: {}. Body (first 200 chars): {}", scheme, err, &body[..body.len().min(200)]),
                ));
                continue;
            }
        };

        // The response structure is:
        //   { "userStatus": { "cascadeModelConfigData": { "clientModelConfigs": [...] } } }
        // Try the nested path first, then fall back to the flat path
        let configs = v.pointer("/userStatus/cascadeModelConfigData/clientModelConfigs")
            .or_else(|| v.pointer("/cascadeModelConfigData/clientModelConfigs"))
            .and_then(|v| v.as_array());

        let configs = match configs {
            Some(c) => c,
            None => {
                last_err = Some(AppError::new(
                    "NO_MODELS_FOUND",
                    format!("No clientModelConfigs in {} response. Top-level keys: {:?}",
                        scheme,
                        v.as_object().map(|o| o.keys().collect::<Vec<_>>()).unwrap_or_default()
                    ),
                ));
                continue;
            }
        };

        let mut models = Vec::new();
        for config in configs {
            let label = config.get("label").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if label.is_empty() {
                continue;
            }
            let remaining_fraction = config.pointer("/quotaInfo/remainingFraction").and_then(|v| v.as_f64());
            let reset_time = config.pointer("/quotaInfo/resetTime").and_then(|v| v.as_str()).map(|s| s.to_string());
            models.push(AntigravityModelQuota {
                label,
                remaining_fraction,
                reset_time,
            });
        }

        if models.is_empty() {
            last_err = Some(AppError::new("NO_MODELS_FOUND", format!("clientModelConfigs array was empty via {}", scheme)));
            continue;
        }

        return Ok(models);
    }

    Err(last_err.unwrap_or_else(|| AppError::new("UNKNOWN", "Failed to query quota via any scheme")))
}

pub fn get_live_antigravity_quota() -> Result<AntigravityQuotaResponse, AppError> {
    let processes = match find_antigravity_processes() {
        Ok(res) => res,
        Err(err) => {
            return Ok(AntigravityQuotaResponse {
                ok: false,
                models: Vec::new(),
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
                format!("Process {} (standalone={}) is not listening on any ports", proc.pid, proc.is_standalone),
            ));
            continue;
        }

        for port in ports {
            match query_antigravity_quota(port, &proc.csrf_token) {
                Ok(models) => {
                    return Ok(AntigravityQuotaResponse {
                        ok: true,
                        models,
                        error: None,
                    });
                }
                Err(err) => {
                    last_err = Some(err);
                }
            }
        }
    }

    Ok(AntigravityQuotaResponse {
        ok: false,
        models: Vec::new(),
        error: Some(format!(
            "Failed to query all ports. Last error: {}",
            last_err.map(|e| e.message).unwrap_or_else(|| "Unknown".to_string())
        )),
    })
}
