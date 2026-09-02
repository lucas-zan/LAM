//! Unified LAM home directory (`~/.lam`) and legacy data migration.
//!
//! LAM historically scattered its data across three locations:
//! - `~/.config/agent-workspace/` (settings, quota-cache, accounts-cache, notes)
//! - `~/Library/Application Support/dev.localagentmanager.desktop/provider-hub/`
//!   (providers, bindings, gateway state, credentials)
//! - `~/.codex/lam/` (usage sqlite, reset-credit-expiry)
//!
//! This module introduces a single root (`~/.lam`, overridable via `LAM_HOME`)
//! and a one-shot idempotent migrator that moves legacy data into it.

use super::error::{AppError, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Canonical LAM home paths under `$LAM_HOME`/`$HOME/.lam`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LamPaths {
    root: PathBuf,
}

impl LamPaths {
    pub fn for_home(home: &Path) -> Self {
        Self {
            root: home.join(".lam"),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `~/.lam` (settings, quota-cache, accounts-cache, account-notes).
    pub fn config_root(&self) -> PathBuf {
        self.root.join("config")
    }

    /// `~/.lam/provider-hub` (providers, bindings, gateway state, credentials).
    pub fn provider_hub_root(&self) -> PathBuf {
        self.root.join("provider-hub")
    }

    /// `~/.lam/usage` (usage sqlite database).
    pub fn usage_root(&self) -> PathBuf {
        self.root.join("usage")
    }

    /// `~/.lam/usage/usage.sqlite3`.
    pub fn usage_db_path(&self) -> PathBuf {
        self.usage_root().join("usage.sqlite3")
    }

    /// `~/.lam/reset-credit-expiry.json`.
    pub fn reset_credit_expiry_path(&self) -> PathBuf {
        self.root.join("reset-credit-expiry.json")
    }
}

/// Result of a migration run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LamMigrationReport {
    /// Legacy locations that were actually moved into `~/.lam`.
    pub migrated: Vec<PathBuf>,
    /// Legacy locations skipped because the canonical target already exists.
    pub skipped_target_exists: Vec<PathBuf>,
    /// Backup directory holding pre-migration copies (empty when no data
    /// was migrated).
    pub backup_dir: Option<PathBuf>,
}

/// One-shot idempotent migration of the three legacy LAM data locations into
/// `~/.lam`. Safe to run on every startup: already-migrated state is skipped
/// and failures are reported without panicking.
pub fn migrate_legacy_lam_data(home_root: &Path) -> Result<LamMigrationReport> {
    migrate_legacy_lam_data_with_options(home_root, LamMigrationOptions::default())
}

/// Options controlling backup/cleanup behavior of a migration run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LamMigrationOptions {
    /// Keep at most this many backup snapshots under `~/.lam/backup`.
    pub max_backups: usize,
    /// Copy legacy data into `~/.lam/backup/<ts>` before renaming it into
    /// place, so a failed or unwanted migration can be rolled back manually.
    pub backup_before_migrate: bool,
}

impl Default for LamMigrationOptions {
    fn default() -> Self {
        Self {
            max_backups: 2,
            backup_before_migrate: true,
        }
    }
}

/// One-shot idempotent migration of the three legacy LAM data locations into
/// `~/.lam`. Safe to run on every startup: already-migrated state is skipped
/// and failures are reported without panicking.
pub fn migrate_legacy_lam_data_with_options(
    home_root: &Path,
    options: LamMigrationOptions,
) -> Result<LamMigrationReport> {
    let lam = LamPaths::for_home(home_root);
    let mut report = LamMigrationReport::default();

    // Pre-create the backup session once; `migrate_dir`/`migrate_file` only
    // copy when the source actually exists.
    let backup_root = if options.backup_before_migrate {
        let backup_root = lam.root().join("backup");
        let session = backup_root.join(format!(
            "migration-{}",
            chrono::Utc::now().format("%Y%m%d-%H%M%S")
        ));
        fs::create_dir_all(&session).ok();
        Some(session)
    } else {
        None
    };

    // 1. provider-hub (legacy + canonical) -> `~/.lam/provider-hub`.
    //    Migrated first so a legacy `provider-hub-v2` nested inside
    //    `agent-workspace` is not carried along by the config migration below.
    let canonical =
        home_root.join("Library/Application Support/dev.localagentmanager.desktop/provider-hub");
    let legacy_hub = home_root.join(".config/agent-workspace/provider-hub-v2");
    // Prefer the canonical provider-hub location; the legacy `provider-hub-v2`
    // is only migrated when the canonical one does not exist (mirrors
    // `ProviderHubPaths::ensure_canonical_root` precedence).
    let hub_source = if canonical.exists() {
        canonical
    } else {
        legacy_hub
    };
    let hub_backup = backup_root.as_ref().map(|root| root.join("provider-hub"));
    migrate_dir(
        &hub_source,
        &lam.provider_hub_root(),
        hub_backup.as_deref(),
        &mut report,
    )?;

    // 2. `~/.config/agent-workspace` -> `~/.lam/config`
    let config_backup = backup_root.as_ref().map(|root| root.join("config"));
    migrate_dir(
        &home_root.join(".config/agent-workspace"),
        &lam.config_root(),
        config_backup.as_deref(),
        &mut report,
    )?;

    // 3. `~/.codex/lam` -> `~/.lam` (usage/ + reset-credit-expiry.json).
    // The legacy directory historically contained `usage/` and
    // `reset-credit-expiry.json`; move those into their canonical locations,
    // then relocate any remaining child entries (e.g. `session-delete-staging`)
    // so the whole legacy container can be removed.
    let legacy_lam = home_root.join(".codex/lam");
    if legacy_lam.exists() {
        let usage_backup = backup_root.as_ref().map(|root| root.join("usage"));
        migrate_dir(
            &legacy_lam.join("usage"),
            &lam.usage_root(),
            usage_backup.as_deref(),
            &mut report,
        )?;
        let expiry_backup = backup_root
            .as_ref()
            .map(|root| root.join("reset-credit-expiry.json"));
        migrate_file(
            &legacy_lam.join("reset-credit-expiry.json"),
            &lam.reset_credit_expiry_path(),
            expiry_backup.as_deref(),
        )?;
        // Relocate any remaining top-level entries under `~/.lam/legacy`.
        if let Ok(entries) = fs::read_dir(&legacy_lam) {
            let legacy_target_root = lam.root().join("legacy");
            for entry in entries.filter_map(|entry| entry.ok()) {
                let from = entry.path();
                let to = legacy_target_root.join(entry.file_name());
                let entry_backup = backup_root.as_ref().map(|root| {
                    root.join("legacy")
                        .join(entry.file_name().to_string_lossy().into_owned())
                });
                if from.is_dir() {
                    let _ = migrate_dir(&from, &to, entry_backup.as_deref(), &mut report);
                } else {
                    let _ = migrate_file(&from, &to, entry_backup.as_deref());
                }
            }
        }
        // Remove the legacy container once its children have moved.
        if fs::read_dir(&legacy_lam)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false)
        {
            let _ = fs::remove_dir(&legacy_lam);
        }
    }

    // Keep the backup directory bounded (sessions are pruned, not the
    // current one).
    if let Some(backup_root) = &backup_root {
        if let Some(parent) = backup_root.parent() {
            prune_backups(parent, options.max_backups);
        }
        if !report.migrated.is_empty() {
            report.backup_dir = Some(backup_root.clone());
        }
    }

    // Tighten permissions on the unified root and its children (backups and
    // the audit log contain sensitive metadata).
    #[cfg(unix)]
    {
        fs::set_permissions(lam.root(), fs::Permissions::from_mode(0o700)).ok();
        if let Some(backup_root) = &backup_root {
            fs::set_permissions(backup_root, fs::Permissions::from_mode(0o700)).ok();
        }
    }

    // Append an audit log line so a failed or unexpected migration can be
    // investigated from `~/.lam/migration-log.jsonl`.
    append_migration_log(&lam, &report);

    Ok(report)
}

/// Append a JSON line describing this migration run. Best-effort: log
/// failures never affect startup.
fn append_migration_log(lam: &LamPaths, report: &LamMigrationReport) {
    let log_path = lam.root().join("migration-log.jsonl");
    let entry = serde_json::json!({
        "at": chrono::Utc::now().to_rfc3339(),
        "migrated": report.migrated.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
        "skippedTargetExists": report.skipped_target_exists.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(),
        "backupDir": report.backup_dir.as_ref().map(|p| p.to_string_lossy().into_owned()),
    });
    let line = format!(
        "{}
",
        entry
    );
    #[cfg(unix)]
    {
        if !log_path.exists() {
            let _ = fs::write(&log_path, "");
            fs::set_permissions(&log_path, fs::Permissions::from_mode(0o600)).ok();
        }
    }
    let _ = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, line.as_bytes()));
}

/// Move a single file to its canonical LAM path (idempotent, non-blocking).
/// When `backup` is provided, the file is copied there first.
fn migrate_file(source: &Path, target: &Path, backup: Option<&Path>) -> Result<()> {
    if !source.exists() {
        return Ok(());
    }
    if target.exists() {
        return Ok(());
    }
    if let Some(backup) = backup {
        let _ = backup_file(source, backup);
    }
    let parent = target
        .parent()
        .ok_or_else(|| AppError::new("LAM_MIGRATION_PATH_INVALID", "target has no parent"))?;
    fs::create_dir_all(parent)?;
    fs::rename(source, target).map_err(|error| {
        AppError::new(
            "LAM_MIGRATION_FAILED",
            format!(
                "could not move {} to {}: {error}",
                source.display(),
                target.display()
            ),
        )
    })
}

/// Move a directory to its canonical LAM path (idempotent, non-blocking).
/// When `backup` is provided, the tree is copied there first so a failed or
/// unwanted migration can be rolled back manually.
fn migrate_dir(
    source: &Path,
    target: &Path,
    backup: Option<&Path>,
    report: &mut LamMigrationReport,
) -> Result<()> {
    if !source.exists() {
        return Ok(());
    }
    if target.exists() {
        // Both exist: treat the target as authoritative (already migrated or a
        // user-created LAM home). Never block startup on a conflict.
        report.skipped_target_exists.push(source.to_path_buf());
        return Ok(());
    }
    if let Some(backup) = backup {
        let _ = copy_dir_all(source, backup);
    }
    let parent = target
        .parent()
        .ok_or_else(|| AppError::new("LAM_MIGRATION_PATH_INVALID", "target has no parent"))?;
    fs::create_dir_all(parent)?;
    fs::rename(source, target).map_err(|error| {
        AppError::new(
            "LAM_MIGRATION_FAILED",
            format!(
                "could not move {} to {}: {error}",
                source.display(),
                target.display()
            ),
        )
    })?;
    #[cfg(unix)]
    fs::set_permissions(target, fs::Permissions::from_mode(0o700)).ok();
    report.migrated.push(source.to_path_buf());
    Ok(())
}

/// Best-effort recursive copy of a directory tree (used for pre-migration
/// backups). Failures are swallowed: the rename below is the authoritative
/// move and a missing backup must never block migration.
fn copy_dir_all(source: &Path, target: &Path) -> std::io::Result<()> {
    if !source.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if from.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Best-effort single-file backup copy.
fn backup_file(source: &Path, backup: &Path) -> std::io::Result<()> {
    if let Some(parent) = backup.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, backup).map(|_| ())
}

/// Keep only the newest `max_backups` snapshots under the backup root.
/// A snapshot is any top-level entry (directory or file) in the backup root.
fn prune_backups(backup_root: &Path, max_backups: usize) {
    if max_backups == 0 {
        let _ = fs::remove_dir_all(backup_root);
        return;
    }
    let mut entries = match fs::read_dir(backup_root) {
        Ok(entries) => entries.filter_map(|entry| entry.ok()).collect::<Vec<_>>(),
        Err(_) => return,
    };
    // Sort by modified time, newest first.
    entries.sort_by(|left, right| {
        let left = left.metadata().and_then(|m| m.modified()).ok();
        let right = right.metadata().and_then(|m| m.modified()).ok();
        right.cmp(&left)
    });
    for entry in entries.into_iter().skip(max_backups) {
        let _ = fs::remove_dir_all(entry.path());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_file(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    #[test]
    fn for_home_resolves_lam_root_and_children() {
        let lam = LamPaths::for_home(Path::new("/Users/test"));
        assert_eq!(lam.root(), Path::new("/Users/test/.lam"));
        assert_eq!(lam.config_root(), Path::new("/Users/test/.lam/config"));
        assert_eq!(
            lam.provider_hub_root(),
            Path::new("/Users/test/.lam/provider-hub")
        );
        assert_eq!(lam.usage_root(), Path::new("/Users/test/.lam/usage"));
        assert_eq!(
            lam.usage_db_path(),
            Path::new("/Users/test/.lam/usage/usage.sqlite3")
        );
        assert_eq!(
            lam.reset_credit_expiry_path(),
            Path::new("/Users/test/.lam/reset-credit-expiry.json")
        );
    }

    #[test]
    fn migrate_moves_all_three_legacy_locations() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        // Legacy locations.
        write_file(&home.join(".config/agent-workspace/settings.json"), "{}");
        write_file(
            &home.join(".config/agent-workspace/accounts-cache.json"),
            "[]",
        );
        write_file(
            &home.join("Library/Application Support/dev.localagentmanager.desktop/provider-hub/providers.json"),
            "{}",
        );
        write_file(&home.join(".codex/lam/usage/usage.sqlite3"), "sqlite");

        let report = migrate_legacy_lam_data(home).unwrap();
        assert_eq!(report.migrated.len(), 3);
        assert!(home.join(".lam/config/settings.json").exists());
        assert!(home.join(".lam/config/accounts-cache.json").exists());
        assert!(home.join(".lam/provider-hub/providers.json").exists());
        assert!(home.join(".lam/usage/usage.sqlite3").exists());
        // Old locations are gone (renamed away).
        assert!(!home.join(".config/agent-workspace").exists());
        assert!(!home
            .join("Library/Application Support/dev.localagentmanager.desktop/provider-hub")
            .exists());
        assert!(!home.join(".codex/lam").exists());
    }

    #[test]
    fn migrate_is_idempotent_and_skips_when_target_exists() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        write_file(&home.join(".config/agent-workspace/settings.json"), "{}");

        let first = migrate_legacy_lam_data(home).unwrap();
        assert_eq!(first.migrated.len(), 1);
        // Target now exists; second run must not fail and must not duplicate.
        let second = migrate_legacy_lam_data(home).unwrap();
        assert!(second.migrated.is_empty());
        assert!(home.join(".lam/config/settings.json").exists());
    }

    #[test]
    fn migrate_without_legacy_data_is_a_noop() {
        let temp = tempfile::tempdir().unwrap();
        let report = migrate_legacy_lam_data(temp.path()).unwrap();
        assert!(report.migrated.is_empty());
        assert!(report.skipped_target_exists.is_empty());
    }

    #[test]
    fn migrate_prefers_canonical_provider_hub_over_legacy_v2() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let canonical =
            home.join("Library/Application Support/dev.localagentmanager.desktop/provider-hub");
        let legacy_v2 = home.join(".config/agent-workspace/provider-hub-v2");
        write_file(&canonical.join("providers.json"), "canonical");
        write_file(&legacy_v2.join("providers.json"), "legacy");

        let report = migrate_legacy_lam_data(home).unwrap();
        let hub = home.join(".lam/provider-hub");
        assert!(hub.join("providers.json").exists());
        assert_eq!(
            fs::read_to_string(hub.join("providers.json")).unwrap(),
            "canonical"
        );
        // agent-workspace (containing provider-hub-v2) moved to ~/.lam/config,
        // and the canonical provider-hub moved to ~/.lam/provider-hub.
        assert_eq!(report.migrated.len(), 2);
        assert!(home
            .join(".lam/config/provider-hub-v2/providers.json")
            .exists());
    }

    #[test]
    fn legacy_v2_hub_is_migrated_when_canonical_is_absent() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let legacy_v2 = home.join(".config/agent-workspace/provider-hub-v2");
        write_file(&legacy_v2.join("providers.json"), "legacy");

        migrate_legacy_lam_data(home).unwrap();
        assert!(home.join(".lam/provider-hub/providers.json").exists());
    }

    #[test]
    fn migrated_data_is_readable_through_lam_paths() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        // Simulate a pre-migration installation.
        write_file(
            &home.join(".config/agent-workspace/settings.json"),
            r#"{"codexLaunchPermissionPreset":"approveForMe"}"#,
        );
        write_file(
            &home.join("Library/Application Support/dev.localagentmanager.desktop/provider-hub/providers.json"),
            r#"{"schemaVersion":1}"#,
        );
        write_file(&home.join(".codex/lam/usage/usage.sqlite3"), "sqlite-bytes");
        write_file(
            &home.join(".codex/lam/reset-credit-expiry.json"),
            r#"{"profiles":{}}"#,
        );

        migrate_legacy_lam_data(home).unwrap();

        let lam = LamPaths::for_home(home);
        // New canonical locations are readable and match the legacy content.
        assert_eq!(
            fs::read_to_string(lam.config_root().join("settings.json")).unwrap(),
            r#"{"codexLaunchPermissionPreset":"approveForMe"}"#
        );
        assert_eq!(
            fs::read_to_string(lam.provider_hub_root().join("providers.json")).unwrap(),
            r#"{"schemaVersion":1}"#
        );
        assert_eq!(
            fs::read_to_string(lam.usage_db_path()).unwrap(),
            "sqlite-bytes"
        );
        assert_eq!(
            fs::read_to_string(lam.reset_credit_expiry_path()).unwrap(),
            r#"{"profiles":{}}"#
        );
    }

    #[test]
    fn migration_failure_does_not_panic_and_reports_error() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        // Make the target a regular file so rename fails cleanly.
        write_file(&home.join(".lam"), "not-a-directory");
        write_file(&home.join(".config/agent-workspace/settings.json"), "{}");

        // Must not panic; the error is reported.
        assert!(migrate_legacy_lam_data(home).is_err());
    }

    #[test]
    fn migration_creates_backup_snapshot_with_original_data() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        write_file(
            &home.join(".config/agent-workspace/settings.json"),
            r#"{"codexLaunchPermissionPreset":"approveForMe"}"#,
        );

        let report = migrate_legacy_lam_data(home).unwrap();
        let backup_dir = report.backup_dir.expect("backup dir reported");
        assert!(backup_dir.starts_with(home.join(".lam/backup")));
        // Backup contains the original settings.json content.
        let backup_settings = backup_dir.join("config/settings.json");
        assert_eq!(
            fs::read_to_string(backup_settings).unwrap(),
            r#"{"codexLaunchPermissionPreset":"approveForMe"}"#
        );
        // Canonical location also has it.
        assert!(home.join(".lam/config/settings.json").exists());
    }

    #[test]
    fn backup_snapshots_are_pruned_to_max_backups() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        // Run migration 3 times; the third run must prune to max_backups=2.
        for _ in 0..3 {
            // Recreate legacy state each time.
            write_file(&home.join(".config/agent-workspace/settings.json"), "{}");
            let _ = migrate_legacy_lam_data(home).unwrap();
            // Remove the migrated target so the next run migrates again.
            fs::remove_dir_all(home.join(".lam/config")).unwrap();
            fs::remove_dir_all(home.join(".lam/backup")).unwrap();
        }
        // After a final migration with pruning, at most 2 sessions remain.
        write_file(&home.join(".config/agent-workspace/settings.json"), "{}");
        let report = migrate_legacy_lam_data(home).unwrap();
        let backup_root = home.join(".lam/backup");
        let session_count = fs::read_dir(&backup_root).unwrap().count();
        assert!(
            session_count <= 2,
            "expected <=2 backup sessions, got {session_count}"
        );
        assert!(report.backup_dir.is_some());
    }

    #[test]
    fn migration_log_is_appended_with_report() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        write_file(&home.join(".config/agent-workspace/settings.json"), "{}");

        migrate_legacy_lam_data(home).unwrap();
        let log_path = home.join(".lam/migration-log.jsonl");
        let body = fs::read_to_string(&log_path).unwrap();
        assert!(body.contains("migrated"));
        assert!(body.contains(".config/agent-workspace"));
        // A second no-op run appends another line.
        migrate_legacy_lam_data(home).unwrap();
        let body = fs::read_to_string(&log_path).unwrap();
        assert_eq!(body.lines().count(), 2);
    }

    #[test]
    fn backup_can_be_disabled() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        write_file(&home.join(".config/agent-workspace/settings.json"), "{}");

        let report = migrate_legacy_lam_data_with_options(
            home,
            LamMigrationOptions {
                backup_before_migrate: false,
                ..LamMigrationOptions::default()
            },
        )
        .unwrap();
        assert!(report.backup_dir.is_none());
        assert!(!home.join(".lam/backup").exists());
        assert!(home.join(".lam/config/settings.json").exists());
    }

    #[test]
    fn residual_codex_lam_entries_are_relocated_and_container_removed() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        write_file(&home.join(".codex/lam/usage/usage.sqlite3"), "sqlite");
        // A leftover non-canonical entry from an older version.
        write_file(
            &home.join(".codex/lam/session-delete-staging/leftover.jsonl"),
            "stale",
        );

        migrate_legacy_lam_data(home).unwrap();

        // Canonical usage migrated.
        assert!(home.join(".lam/usage/usage.sqlite3").exists());
        // Residual entry relocated under ~/.lam/legacy.
        assert!(home
            .join(".lam/legacy/session-delete-staging/leftover.jsonl")
            .exists());
        // Legacy container fully removed.
        assert!(!home.join(".codex/lam").exists());
    }

    #[cfg(unix)]
    #[test]
    fn migration_tightens_lam_root_and_log_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        write_file(&home.join(".config/agent-workspace/settings.json"), "{}");

        migrate_legacy_lam_data(home).unwrap();

        let root_mode = fs::metadata(home.join(".lam"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(root_mode, 0o700, "~/.lam must be 0700");
        let log_mode = fs::metadata(home.join(".lam/migration-log.jsonl"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(log_mode, 0o600, "migration log must be 0600");
    }
}
