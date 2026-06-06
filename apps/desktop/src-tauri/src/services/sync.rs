use super::account::find_account;
use super::error::{AppError, Result};
use super::types::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncRequest {
    pub from_profile_id: String,
    pub to_profile_id: String,
    pub sync_sessions: bool,
    pub backup_target_sessions: bool,
    pub sidecar_backup_history: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncOperation {
    pub kind: String,
    pub from: Option<PathBuf>,
    pub to: Option<PathBuf>,
    pub rel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncPlan {
    pub from_profile_id: String,
    pub to_profile_id: String,
    pub operations: Vec<SyncOperation>,
    pub warnings: Vec<String>,
    pub blocked_files: Vec<String>,
    pub policy_blocked_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub copied: usize,
    pub skipped: usize,
    pub backup_path: Option<PathBuf>,
    pub manifest_path: PathBuf,
    pub warnings: Vec<String>,
}

pub fn sync_plan(home_root: &Path, req: &SyncRequest) -> Result<SyncPlan> {
    let from = find_account(home_root, &req.from_profile_id)?;
    let to = find_account(home_root, &req.to_profile_id)?;
    if same_path(&from.codex_home, &to.codex_home) {
        return Err(AppError::new(
            "SYNC_SAME_PROFILE",
            "source and target must differ",
        ));
    }
    let mut operations = Vec::new();
    let mut warnings = Vec::new();
    let policy_blocked_files = blocked_files();
    let blocked_files = seen_blocked_files(&from.codex_home)?;
    if !to.is_relay {
        warnings.push("Target is a primary profile; relay profile is recommended.".into());
    }
    match (&from.provider_id, &to.provider_id) {
        (Some(source), Some(target)) if source != target => warnings.push(format!(
            "Provider mismatch: source uses {source}, target uses {target}."
        )),
        (None, _) | (_, None) => warnings.push(
            "Provider mismatch check is incomplete because one side has unknown provider.".into(),
        ),
        _ => {}
    }
    if req.backup_target_sessions && to.codex_home.join("sessions").exists() {
        operations.push(SyncOperation {
            kind: "backup_dir".into(),
            from: Some(to.codex_home.join("sessions")),
            to: Some(to.codex_home.join("sessions.backup.<timestamp>")),
            rel: Some("sessions".into()),
        });
    }
    if req.sync_sessions {
        let from_sessions = from.codex_home.join("sessions");
        let to_sessions = to.codex_home.join("sessions");
        let files = session_files(&from_sessions)?;
        if files.len() > 5_000 {
            warnings.push(format!(
                "Large session directory: {} files will be evaluated.",
                files.len()
            ));
        }
        for file in files {
            let rel_path = file
                .strip_prefix(&from_sessions)
                .map_err(|_| AppError::new("PATH_ERROR", "session path outside source"))?;
            let rel = rel_path.to_string_lossy().to_string();
            let dst = to_sessions.join(rel_path);
            let kind = if should_skip_copy(&file, &dst)? {
                "skip_file"
            } else {
                "copy_file"
            };
            operations.push(SyncOperation {
                kind: kind.into(),
                from: Some(file),
                to: Some(dst),
                rel: Some(rel),
            });
        }
    }
    if req.sidecar_backup_history {
        operations.push(SyncOperation {
            kind: "copy_history_sidecar".into(),
            from: Some(from.codex_home.join("history.jsonl")),
            to: Some(
                to.codex_home
                    .join(format!("history.from-{}.jsonl", from.id)),
            ),
            rel: Some("history.jsonl".into()),
        });
    }
    Ok(SyncPlan {
        from_profile_id: req.from_profile_id.clone(),
        to_profile_id: req.to_profile_id.clone(),
        operations,
        warnings,
        blocked_files,
        policy_blocked_files,
    })
}

pub fn execute_sync(home_root: &Path, req: &SyncRequest) -> Result<SyncResult> {
    let plan = sync_plan(home_root, req)?;
    let from = find_account(home_root, &req.from_profile_id)?;
    let to = find_account(home_root, &req.to_profile_id)?;
    let mut copied = 0;
    let mut skipped = 0;
    let mut backup_path = None;
    if req.backup_target_sessions && to.codex_home.join("sessions").exists() {
        let backup = to
            .codex_home
            .join(format!("sessions.backup.{}", timestamp_yyyymmdd_hhmmss()));
        copy_dir_recursive(&to.codex_home.join("sessions"), &backup)?;
        backup_path = Some(backup);
    }
    for op in plan
        .operations
        .iter()
        .filter(|op| op.kind == "copy_file" || op.kind == "skip_file")
    {
        if op.kind == "skip_file" {
            skipped += 1;
            continue;
        }
        let src = op
            .from
            .as_ref()
            .ok_or_else(|| AppError::new("SYNC_PLAN_INVALID", "copy operation missing source"))?;
        let dst = op
            .to
            .as_ref()
            .ok_or_else(|| AppError::new("SYNC_PLAN_INVALID", "copy operation missing target"))?;
        if should_skip_copy(src, dst)? {
            skipped += 1;
            continue;
        }
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dst)?;
        copied += 1;
    }
    if req.sidecar_backup_history {
        let src = from.codex_home.join("history.jsonl");
        let dst = to
            .codex_home
            .join(format!("history.from-{}.jsonl", from.id));
        if src.exists() {
            fs::copy(src, dst)?;
        }
    }
    let manifest_dir = home_root.join(".config/agent-workspace/sync-manifests");
    fs::create_dir_all(&manifest_dir)?;
    let manifest_path = manifest_dir.join(format!("{}.json", Uuid::new_v4()));
    write_file_private(
        &manifest_path,
        &sync_manifest_json(&plan, copied, skipped, backup_path.as_ref()),
    )?;
    Ok(SyncResult {
        copied,
        skipped,
        backup_path,
        manifest_path,
        warnings: plan.warnings,
    })
}

fn same_path(a: &Path, b: &Path) -> bool {
    fs::canonicalize(a).ok() == fs::canonicalize(b).ok()
}

fn blocked_files() -> Vec<String> {
    vec![
        "auth.json",
        "config.toml",
        "history.jsonl",
        "*.sqlite*",
        "cache/",
        "tmp/",
        "log/",
        "logs/",
        "installation_id",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn should_skip_copy(src: &Path, dst: &Path) -> Result<bool> {
    if !dst.exists() {
        return Ok(false);
    }
    let src_meta = fs::metadata(src)?;
    let dst_meta = fs::metadata(dst)?;
    let same_size = src_meta.len() == dst_meta.len();
    let dst_newer_or_same = dst_meta.modified().ok() >= src_meta.modified().ok();
    Ok(same_size && dst_newer_or_same)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    if !src.exists() {
        return Ok(());
    }
    for file in session_files(src)? {
        let rel = file
            .strip_prefix(src)
            .map_err(|_| AppError::new("PATH_ERROR", "copy path outside source"))?;
        let target = dst.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(file, target)?;
    }
    Ok(())
}

fn sync_manifest_json(
    plan: &SyncPlan,
    copied: usize,
    skipped: usize,
    backup: Option<&PathBuf>,
) -> String {
    let operations = plan
        .operations
        .iter()
        .map(|op| {
            format!(
                "{{\"kind\":\"{}\",\"from\":{},\"to\":{},\"rel\":{}}}",
                json_escape(&op.kind),
                op.from
                    .as_ref()
                    .map(|p| format!("\"{}\"", json_escape(&p.to_string_lossy())))
                    .unwrap_or_else(|| "null".into()),
                op.to
                    .as_ref()
                    .map(|p| format!("\"{}\"", json_escape(&p.to_string_lossy())))
                    .unwrap_or_else(|| "null".into()),
                op.rel
                    .as_ref()
                    .map(|s| format!("\"{}\"", json_escape(s)))
                    .unwrap_or_else(|| "null".into())
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{{\n  \"fromProfileId\": \"{}\",\n  \"toProfileId\": \"{}\",\n  \"timestamp\": \"{}\",\n  \"copied\": {},\n  \"skipped\": {},\n  \"backupPath\": {},\n  \"operations\": [{}],\n  \"blockedFiles\": [{}],\n  \"policyBlockedFiles\": [{}],\n  \"warnings\": [{}]\n}}\n",
        json_escape(&plan.from_profile_id),
        json_escape(&plan.to_profile_id),
        timestamp_yyyymmdd_hhmmss(),
        copied,
        skipped,
        backup
            .map(|p| format!("\"{}\"", json_escape(&p.to_string_lossy())))
            .unwrap_or_else(|| "null".into()),
        operations,
        plan.blocked_files
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect::<Vec<_>>()
            .join(", "),
        plan.policy_blocked_files
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect::<Vec<_>>()
            .join(", "),
        plan.warnings
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn seen_blocked_files(home: &Path) -> Result<Vec<String>> {
    let mut seen = Vec::new();
    for name in [
        "auth.json",
        "config.toml",
        "history.jsonl",
        "installation_id",
    ] {
        if home.join(name).exists() {
            seen.push(name.to_string());
        }
    }
    for dir in ["cache", "tmp", "log", "logs"] {
        if home.join(dir).exists() {
            seen.push(format!("{dir}/"));
        }
    }
    collect_matching_blocked(home, home, &mut seen)?;
    seen.sort();
    seen.dedup();
    Ok(seen)
}

fn collect_matching_blocked(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_matching_blocked(root, &path, out)?;
        } else if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            if name.ends_with(".sqlite")
                || name.ends_with(".sqlite-shm")
                || name.ends_with(".sqlite-wal")
                || (name.starts_with("state_") && name.contains(".sqlite"))
            {
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();
                out.push(rel);
            }
        }
    }
    Ok(())
}
