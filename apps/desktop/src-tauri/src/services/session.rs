use super::account::find_account;
use super::account::CodexAccount;
use super::error::{AppError, Result};
use super::types::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_SESSION_PAGE_SIZE: usize = 5;
pub const MAX_SESSION_PAGE_SIZE: usize = 50;
pub const MIN_RETAINED_ACTIVE_SESSIONS: usize = 20;
pub const MIN_ARCHIVE_AGE_DAYS: u64 = 7;
const MIN_ARCHIVE_AGE_SECS: u64 = MIN_ARCHIVE_AGE_DAYS * 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexSession {
    pub id: String,
    pub account_id: String,
    pub path: PathBuf,
    pub modified_at: u64,
    pub size_bytes: u64,
    pub cwd: Option<String>,
    pub summary: Option<String>,
    pub first_user_message: Option<String>,
    pub thread_name: Option<String>,
    pub model: Option<String>,
    pub original_provider_id: Option<String>,
    pub original_model: Option<String>,
    pub current_provider_id: Option<String>,
    pub current_model: Option<String>,
    pub provider_mismatch: bool,
    pub deletable: bool,
    pub deletion_protection_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionPageCursor {
    pub modified_at: u64,
    #[serde(default)]
    pub size_bytes: Option<u64>,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SessionSort {
    #[default]
    Newest,
    Largest,
    Smallest,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SessionAgeFilter {
    #[default]
    All,
    Last7Days,
    Last30Days,
    OlderThan30Days,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionQueryRequest {
    pub limit: Option<usize>,
    pub cursor: Option<SessionPageCursor>,
    pub sort: Option<SessionSort>,
    pub age: Option<SessionAgeFilter>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionSelectionRequest {
    pub age: Option<SessionAgeFilter>,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionPageRequest {
    pub limit: Option<usize>,
    pub cursor: Option<SessionPageCursor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionPage {
    pub items: Vec<CodexSession>,
    pub next_cursor: Option<SessionPageCursor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionStorageSummary {
    pub evaluated_at: u64,
    pub active_count: usize,
    pub active_bytes: u64,
    pub eligible_count: usize,
    pub eligible_bytes: u64,
    pub retained_recent_count: usize,
    pub minimum_age_days: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSessionsRequest {
    pub profile_id: String,
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSessionsResult {
    pub deleted_count: usize,
    pub deleted_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionFile {
    path: PathBuf,
    modified_at: u64,
    size_bytes: u64,
}

pub fn list_sessions(home_root: &Path, profile_id: &str) -> Result<Vec<CodexSession>> {
    let account = find_account(home_root, profile_id)?;
    let thread_names = read_session_index_thread_names(&account.codex_home)?;
    let now = current_epoch_secs();
    session_files_with_metadata(&account)?
        .into_iter()
        .enumerate()
        .map(|(index, file)| {
            let reason = archive_protection_reason(index, &file, now);
            build_session_with_protection(&account, &thread_names, &file, reason)
        })
        .collect()
}

pub fn list_sessions_page(
    home_root: &Path,
    profile_id: &str,
    request: &SessionPageRequest,
) -> Result<SessionPage> {
    query_sessions_page(
        home_root,
        profile_id,
        &SessionQueryRequest {
            limit: request.limit,
            cursor: request.cursor.clone(),
            sort: Some(SessionSort::Newest),
            age: Some(SessionAgeFilter::All),
        },
    )
}

pub fn query_sessions_page(
    home_root: &Path,
    profile_id: &str,
    request: &SessionQueryRequest,
) -> Result<SessionPage> {
    let account = find_account(home_root, profile_id)?;
    let limit = normalized_page_limit(request.limit)?;
    let thread_names = read_session_index_thread_names(&account.codex_home)?;
    let sort = request.sort.unwrap_or_default();
    let age = request.age.unwrap_or_default();
    let now = current_epoch_secs();
    let active = session_files_with_metadata(&account)?;
    let protection = active
        .iter()
        .enumerate()
        .filter_map(|(index, file)| {
            archive_protection_reason(index, file, now).map(|reason| (file.path.clone(), reason))
        })
        .collect::<HashMap<_, _>>();
    let mut files = active
        .into_iter()
        .filter(|file| session_matches_age(file, age, now))
        .collect::<Vec<_>>();
    sort_session_files(&mut files, sort);
    let start = query_page_start(&files, request.cursor.as_ref(), sort)?;
    page_sessions(
        &account,
        &thread_names,
        &files,
        start,
        limit,
        Some(&protection),
    )
}

pub fn session_storage_summary(
    home_root: &Path,
    profile_id: &str,
) -> Result<SessionStorageSummary> {
    let account = find_account(home_root, profile_id)?;
    let active = session_files_with_metadata(&account)?;
    let now = current_epoch_secs();
    let eligible = active
        .iter()
        .enumerate()
        .filter(|(index, file)| archive_protection_reason(*index, file, now).is_none())
        .map(|(_, file)| file);
    let (eligible_count, eligible_bytes) = eligible.fold((0usize, 0u64), |(count, bytes), file| {
        (count + 1, bytes.saturating_add(file.size_bytes))
    });
    Ok(SessionStorageSummary {
        evaluated_at: now,
        active_count: active.len(),
        active_bytes: active.iter().map(|file| file.size_bytes).sum(),
        eligible_count,
        eligible_bytes,
        retained_recent_count: MIN_RETAINED_ACTIVE_SESSIONS,
        minimum_age_days: MIN_ARCHIVE_AGE_DAYS,
    })
}

pub fn query_deletable_session_paths(
    home_root: &Path,
    profile_id: &str,
    request: &SessionSelectionRequest,
) -> Result<Vec<PathBuf>> {
    let account = find_account(home_root, profile_id)?;
    let thread_names = read_session_index_thread_names(&account.codex_home)?;
    let age = request.age.unwrap_or_default();
    let query = request
        .query
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    let now = current_epoch_secs();
    let active = session_files_with_metadata(&account)?;
    let mut paths = Vec::new();
    for (index, file) in active.iter().enumerate() {
        if archive_protection_reason(index, file, now).is_some()
            || !session_matches_age(file, age, now)
        {
            continue;
        }
        if query.is_empty()
            || session_matches_query(
                &build_session_with_protection(&account, &thread_names, file, None)?,
                &query,
            )
        {
            paths.push(file.path.clone());
        }
    }
    Ok(paths)
}

pub fn delete_sessions(
    home_root: &Path,
    request: &DeleteSessionsRequest,
) -> Result<DeleteSessionsResult> {
    if request.paths.is_empty() {
        return Err(AppError::new(
            "EMPTY_SESSION_SELECTION",
            "Select at least one session to delete",
        ));
    }
    let account = find_account(home_root, &request.profile_id)?;
    let active_root = account.codex_home.join("sessions").canonicalize()?;
    let active = session_files_with_metadata(&account)?;
    let thread_names = read_session_index_thread_names(&account.codex_home)?;
    let now = current_epoch_secs();
    let mut seen = HashSet::new();
    let mut selected = Vec::with_capacity(request.paths.len());
    for requested_path in &request.paths {
        let source = requested_path.canonicalize().map_err(|_| {
            AppError::new("INVALID_SESSION_PATH", requested_path.display().to_string())
        })?;
        if !source.starts_with(&active_root) || !seen.insert(source.clone()) {
            return Err(AppError::new(
                "INVALID_SESSION_PATH",
                requested_path.display().to_string(),
            ));
        }
        let Some((index, file)) = active
            .iter()
            .enumerate()
            .find(|(_, file)| file.path.canonicalize().ok().as_ref() == Some(&source))
        else {
            return Err(AppError::new(
                "INVALID_SESSION_PATH",
                requested_path.display().to_string(),
            ));
        };
        if let Some(reason) = archive_protection_reason(index, file, now) {
            return Err(AppError::new("SESSION_DELETE_PROTECTED", reason));
        }
        let session = build_session_with_protection(&account, &thread_names, file, None)?;
        selected.push((source, file.path.clone(), file.size_bytes, session.id));
    }

    let staging_root = account
        .codex_home
        .join("lam/session-delete-staging")
        .join(format!("{}-{}", std::process::id(), current_epoch_nanos()));
    let moves = selected
        .iter()
        .enumerate()
        .map(|(index, (source, _, size, _))| {
            (
                source.clone(),
                staging_root.join(format!("{index}.jsonl")),
                *size,
            )
        })
        .collect::<Vec<_>>();
    let source_paths = selected
        .iter()
        .map(|(_, database_path, _, _)| database_path.clone())
        .collect::<Vec<_>>();
    let session_ids = selected
        .iter()
        .map(|(_, _, _, id)| id.clone())
        .collect::<Vec<_>>();
    let index_snapshot = read_session_index_snapshot(&account.codex_home)?;
    move_sessions_atomically(&moves)?;
    if let Err(error) = remove_session_index_entries(&account.codex_home, &session_ids) {
        return Err(rollback_session_delete(
            home_root,
            &account.codex_home,
            &moves,
            &staging_root,
            &index_snapshot,
            false,
            error,
        ));
    }
    if let Err(error) = super::usage::delete_usage_sources(home_root, &source_paths) {
        return Err(rollback_session_delete(
            home_root,
            &account.codex_home,
            &moves,
            &staging_root,
            &index_snapshot,
            false,
            error,
        ));
    }
    if let Err(error) =
        delete_codex_thread_records(&account.codex_home, &session_ids, &source_paths)
    {
        return Err(rollback_session_delete(
            home_root,
            &account.codex_home,
            &moves,
            &staging_root,
            &index_snapshot,
            true,
            error,
        ));
    }
    let _ = fs::remove_dir_all(&staging_root);
    Ok(DeleteSessionsResult {
        deleted_count: selected.len(),
        deleted_bytes: selected.iter().map(|(_, _, size, _)| size).sum(),
    })
}

fn session_matches_query(session: &CodexSession, query: &str) -> bool {
    [
        Some(session.id.as_str()),
        session.thread_name.as_deref(),
        session.cwd.as_deref(),
        session.summary.as_deref(),
        session.path.to_str(),
        session.model.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|value| value.to_lowercase().contains(query))
}

fn delete_codex_thread_records(
    codex_home: &Path,
    session_ids: &[String],
    source_paths: &[PathBuf],
) -> Result<()> {
    let db_path = codex_home.join("state_5.sqlite");
    if !db_path.exists() {
        return Ok(());
    }
    let mut conn = rusqlite::Connection::open(&db_path).map_err(codex_db_error)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(codex_db_error)?;
    let has_threads = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='threads')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(codex_db_error)?;
    if !has_threads {
        return Ok(());
    }
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(codex_db_error)?;
    for (id, path) in session_ids.iter().zip(source_paths) {
        tx.execute(
            "DELETE FROM thread_spawn_edges WHERE parent_thread_id = ?1 OR child_thread_id = ?1",
            [id],
        )
        .map_err(codex_db_error)?;
        tx.execute(
            "DELETE FROM thread_dynamic_tools WHERE thread_id = ?1",
            [id],
        )
        .map_err(codex_db_error)?;
        tx.execute(
            "DELETE FROM threads WHERE id = ?1 OR rollout_path = ?2",
            rusqlite::params![id, path.to_string_lossy().to_string()],
        )
        .map_err(codex_db_error)?;
    }
    tx.commit().map_err(codex_db_error)
}

fn codex_db_error(error: rusqlite::Error) -> AppError {
    AppError::new("CODEX_SESSION_DB_ERROR", error.to_string())
}

fn read_session_index_snapshot(codex_home: &Path) -> Result<Option<Vec<u8>>> {
    let path = codex_home.join("session_index.jsonl");
    if path.exists() {
        fs::read(path).map(Some).map_err(Into::into)
    } else {
        Ok(None)
    }
}

fn restore_session_index_snapshot(codex_home: &Path, snapshot: &Option<Vec<u8>>) -> Result<()> {
    let path = codex_home.join("session_index.jsonl");
    match snapshot {
        Some(body) => {
            let temp = path.with_extension(format!("jsonl.lam-restore-{}", current_epoch_nanos()));
            fs::write(&temp, body)?;
            fs::rename(temp, path)?;
        }
        None if path.exists() => fs::remove_file(path)?,
        None => {}
    }
    Ok(())
}

fn rollback_session_delete(
    home_root: &Path,
    codex_home: &Path,
    moves: &[(PathBuf, PathBuf, u64)],
    staging_root: &Path,
    index_snapshot: &Option<Vec<u8>>,
    refresh_usage: bool,
    original: AppError,
) -> AppError {
    let mut recovery_errors = Vec::new();
    for (source, staged, _) in moves.iter().rev() {
        if staged.exists() {
            if let Err(error) = fs::rename(staged, source) {
                recovery_errors.push(format!("restore {}: {error}", source.display()));
            }
        }
    }
    if let Err(error) = restore_session_index_snapshot(codex_home, index_snapshot) {
        recovery_errors.push(format!("restore session index: {error}"));
    }
    if refresh_usage {
        if let Err(error) = super::usage::refresh_usage_index(home_root) {
            recovery_errors.push(format!("restore usage index: {error}"));
        }
    }
    if staging_root.exists() {
        if let Err(error) = fs::remove_dir_all(staging_root) {
            recovery_errors.push(format!("remove deletion staging: {error}"));
        }
    }
    if recovery_errors.is_empty() {
        original
    } else {
        AppError::new(
            "SESSION_DELETE_RECOVERY_FAILED",
            format!(
                "{}; recovery also failed: {}",
                original,
                recovery_errors.join("; ")
            ),
        )
    }
}

fn remove_session_index_entries(codex_home: &Path, session_ids: &[String]) -> Result<()> {
    let path = codex_home.join("session_index.jsonl");
    if !path.exists() {
        return Ok(());
    }
    let body = fs::read_to_string(&path)?;
    let ids = session_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let retained = body
        .lines()
        .filter(|line| {
            serde_json::from_str::<Value>(line)
                .ok()
                .and_then(|value| value.get("id").and_then(Value::as_str).map(str::to_string))
                .is_none_or(|id| !ids.contains(id.as_str()))
        })
        .collect::<Vec<_>>();
    let updated = if retained.is_empty() {
        String::new()
    } else {
        format!("{}\n", retained.join("\n"))
    };
    let temp = path.with_extension(format!("jsonl.lam-delete-{}", current_epoch_nanos()));
    fs::write(&temp, updated)?;
    fs::rename(temp, path)?;
    Ok(())
}

fn page_sessions(
    account: &CodexAccount,
    thread_names: &HashMap<String, String>,
    files: &[SessionFile],
    start: usize,
    limit: usize,
    protection: Option<&HashMap<PathBuf, String>>,
) -> Result<SessionPage> {
    let end = start.saturating_add(limit).min(files.len());
    let items = files[start..end]
        .iter()
        .map(|file| {
            let reason = protection
                .and_then(|reasons| reasons.get(&file.path))
                .cloned();
            build_session_with_protection(account, thread_names, file, reason)
        })
        .collect::<Result<Vec<_>>>()?;
    let next_cursor = if end < files.len() {
        files
            .get(end.saturating_sub(1))
            .map(|file| SessionPageCursor {
                modified_at: file.modified_at,
                size_bytes: Some(file.size_bytes),
                path: file.path.clone(),
            })
    } else {
        None
    };
    Ok(SessionPage { items, next_cursor })
}

fn current_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn current_epoch_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn session_matches_age(file: &SessionFile, age: SessionAgeFilter, now: u64) -> bool {
    let age_seconds = now.saturating_sub(file.modified_at);
    match age {
        SessionAgeFilter::All => true,
        SessionAgeFilter::Last7Days => age_seconds <= 7 * 24 * 60 * 60,
        SessionAgeFilter::Last30Days => age_seconds <= 30 * 24 * 60 * 60,
        SessionAgeFilter::OlderThan30Days => age_seconds > 30 * 24 * 60 * 60,
    }
}

fn sort_session_files(files: &mut [SessionFile], sort: SessionSort) {
    files.sort_by(|left, right| match sort {
        SessionSort::Newest => compare_session_files(left, right),
        SessionSort::Largest => right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left.path.cmp(&right.path)),
        SessionSort::Smallest => left
            .size_bytes
            .cmp(&right.size_bytes)
            .then_with(|| left.path.cmp(&right.path)),
    });
}

fn query_page_start(
    files: &[SessionFile],
    cursor: Option<&SessionPageCursor>,
    sort: SessionSort,
) -> Result<usize> {
    let Some(cursor) = cursor else { return Ok(0) };
    if !matches!(sort, SessionSort::Newest) && cursor.size_bytes.is_none() {
        return Err(AppError::new(
            "INVALID_SESSION_CURSOR",
            "Size-sorted session cursor is missing its size key",
        ));
    }
    if let Some(index) = files.iter().position(|file| {
        file.modified_at == cursor.modified_at
            && file.path == cursor.path
            && (matches!(sort, SessionSort::Newest) || Some(file.size_bytes) == cursor.size_bytes)
    }) {
        return Ok(index.saturating_add(1));
    }
    Ok(files
        .iter()
        .take_while(|file| match sort {
            SessionSort::Newest => {
                file.modified_at > cursor.modified_at
                    || (file.modified_at == cursor.modified_at && file.path < cursor.path)
            }
            SessionSort::Largest => {
                file.size_bytes > cursor.size_bytes.unwrap_or_default()
                    || (Some(file.size_bytes) == cursor.size_bytes && file.path < cursor.path)
            }
            SessionSort::Smallest => {
                file.size_bytes < cursor.size_bytes.unwrap_or_default()
                    || (Some(file.size_bytes) == cursor.size_bytes && file.path < cursor.path)
            }
        })
        .count())
}

fn archive_protection_reason(index: usize, file: &SessionFile, now: u64) -> Option<String> {
    if index < MIN_RETAINED_ACTIVE_SESSIONS {
        return Some(format!(
            "The latest {MIN_RETAINED_ACTIVE_SESSIONS} sessions are protected"
        ));
    }
    if now.saturating_sub(file.modified_at) < MIN_ARCHIVE_AGE_SECS {
        return Some(format!(
            "Sessions modified within {MIN_ARCHIVE_AGE_DAYS} days are protected"
        ));
    }
    None
}

fn move_sessions_atomically(moves: &[(PathBuf, PathBuf, u64)]) -> Result<()> {
    let mut completed: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (source, target, _) in moves {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Err(error) = fs::rename(source, target) {
            for (moved_source, moved_target) in completed.iter().rev() {
                let _ = fs::rename(moved_target, moved_source);
            }
            return Err(error.into());
        }
        completed.push((source.clone(), target.clone()));
    }
    Ok(())
}

fn normalized_page_limit(limit: Option<usize>) -> Result<usize> {
    let limit = limit.unwrap_or(DEFAULT_SESSION_PAGE_SIZE);
    if limit == 0 || limit > MAX_SESSION_PAGE_SIZE {
        return Err(AppError::new(
            "INVALID_SESSION_PAGE_LIMIT",
            format!("Session page limit must be between 1 and {MAX_SESSION_PAGE_SIZE}"),
        ));
    }
    Ok(limit)
}

fn session_files_with_metadata(account: &CodexAccount) -> Result<Vec<SessionFile>> {
    session_files_with_metadata_at(&account.codex_home.join("sessions"))
}

fn session_files_with_metadata_at(root: &Path) -> Result<Vec<SessionFile>> {
    let mut files = session_files(root)?
        .into_iter()
        .map(|path| {
            let metadata = fs::metadata(&path)?;
            Ok(SessionFile {
                path,
                modified_at: metadata.modified().ok().map(system_secs).unwrap_or(0),
                size_bytes: metadata.len(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    files.sort_by(compare_session_files);
    Ok(files)
}

fn compare_session_files(left: &SessionFile, right: &SessionFile) -> Ordering {
    right
        .modified_at
        .cmp(&left.modified_at)
        .then_with(|| left.path.cmp(&right.path))
}

fn normalize_cwd(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let raw = if let Some(rest) = trimmed.strip_prefix("file://") {
        rest.strip_prefix("localhost").unwrap_or(rest)
    } else {
        trimmed
    };
    // percent-decode URI escapes (e.g. %20 -> space), keep other chars intact.
    let mut out = String::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push((h << 4 | l) as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn build_session_with_protection(
    account: &CodexAccount,
    thread_names: &HashMap<String, String>,
    file: &SessionFile,
    deletion_protection_reason: Option<String>,
) -> Result<CodexSession> {
    let first_line = read_first_line(&file.path)?;
    let snippet = read_tail(&file.path, 256 * 1024)?;
    let fallback_id = file
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let original_provider_id = extract_json_string(
        &snippet,
        &[
            "provider_id",
            "providerId",
            "original_provider_id",
            "originalProviderId",
        ],
    )
    .or_else(|| account.provider_id.clone());
    let original_model =
        extract_json_string(&snippet, &["model", "original_model", "originalModel"]);
    let current_provider_id = account.provider_id.clone();
    let current_model = account.model.clone();
    let provider_mismatch = match (&original_provider_id, &current_provider_id) {
        (Some(original), Some(current)) => original != current,
        _ => false,
    };
    let id = extract_session_meta_payload_string(&first_line, "id")
        .or_else(|| {
            extract_json_string(
                &snippet,
                &[
                    "session_id",
                    "sessionId",
                    "conversation_id",
                    "conversationId",
                    "id",
                ],
            )
        })
        .unwrap_or(fallback_id);

    Ok(CodexSession {
        thread_name: thread_names.get(&id).cloned(),
        id,
        account_id: account.id.clone(),
        path: file.path.clone(),
        modified_at: file.modified_at,
        size_bytes: file.size_bytes,
        cwd: extract_json_string(
            &snippet,
            &[
                "cwd",
                "workdir",
                "working_directory",
                "workingDirectory",
                "current_dir",
            ],
        )
        .and_then(|value| normalize_cwd(&value)),
        summary: extract_json_string(
            &snippet,
            &["summary", "title", "text", "content", "message"],
        ),
        first_user_message: extract_json_string(
            &snippet,
            &["first_user_message", "firstUserMessage"],
        ),
        model: original_model.clone().or_else(|| current_model.clone()),
        original_provider_id,
        original_model,
        current_provider_id,
        current_model,
        provider_mismatch,
        deletable: deletion_protection_reason.is_none(),
        deletion_protection_reason,
    })
}

fn read_session_index_thread_names(codex_home: &Path) -> Result<HashMap<String, String>> {
    let path = codex_home.join("session_index.jsonl");
    let mut names = HashMap::new();
    if !path.exists() {
        return Ok(names);
    }

    let body = fs::read_to_string(path)?;
    for line in body.lines() {
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let Some(id) = value.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(thread_name) = value.get("thread_name").and_then(Value::as_str) else {
            continue;
        };
        let id = id.trim();
        let thread_name = thread_name.trim();
        if id.is_empty() || thread_name.is_empty() {
            continue;
        }
        names.insert(id.to_string(), short_text(thread_name, 240));
    }
    Ok(names)
}

fn extract_session_meta_payload_string(line: &str, key: &str) -> Option<String> {
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }
    value
        .get("payload")
        .and_then(|payload| payload.get(key))
        .and_then(Value::as_str)
        .map(|value| short_text(value, 240))
}

fn extract_json_string(snippet: &str, keys: &[&str]) -> Option<String> {
    for key in keys {
        let needle = format!("\"{key}\"");
        if let Some(pos) = snippet.rfind(&needle) {
            let after = &snippet[pos + needle.len()..];
            let colon = after.find(':')?;
            let value = after[colon + 1..].trim_start();
            if let Some(stripped) = value.strip_prefix('"') {
                return parse_json_string_value(stripped);
            }
        }
    }
    None
}

fn parse_json_string_value(input: &str) -> Option<String> {
    let mut out = String::new();
    let mut escaped = false;
    for c in input.chars() {
        if escaped {
            out.push(match c {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '"' => '"',
                '\\' => '\\',
                '/' => '/',
                other => other,
            });
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            return Some(short_text(&out, 240));
        } else {
            out.push(c);
        }
    }
    None
}

#[cfg(test)]
mod cwd_normalize_tests {
    use super::normalize_cwd;

    #[test]
    fn file_uri_is_converted_to_plain_path() {
        assert_eq!(
            normalize_cwd("file:///Users/zhanhd/Projects/My%20App"),
            Some("/Users/zhanhd/Projects/My App".to_string())
        );
        assert_eq!(
            normalize_cwd("file://localhost/Users/zhanhd/a"),
            Some("/Users/zhanhd/a".to_string())
        );
    }

    #[test]
    fn plain_paths_pass_through() {
        assert_eq!(
            normalize_cwd("/Users/zhanhd/code"),
            Some("/Users/zhanhd/code".to_string())
        );
        assert_eq!(normalize_cwd(""), None);
        assert_eq!(normalize_cwd("   "), None);
    }
}
