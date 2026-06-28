use crate::{AppError, Result};
use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDate, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const PARSER_ADAPTER_VERSION: &str = "lam-codex-jsonl-v1";
static REFRESH_LOCK: Mutex<()> = Mutex::new(());
const KNOWN_NON_TOKEN_EVENT_MSG_TYPES: &[&str] = &[
    "agent_message",
    "context_compacted",
    "image_generation_end",
    "item_completed",
    "mcp_tool_call_begin",
    "mcp_tool_call_end",
    "patch_apply_end",
    "skill_completed",
    "skill_invoked",
    "skill_selected",
    "skill_started",
    "skill_used",
    "task_complete",
    "task_started",
    "thread_goal_updated",
    "thread_rolled_back",
    "turn_aborted",
    "user_message",
    "web_search_end",
];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageRefreshResult {
    pub scanned_files: usize,
    pub parsed_files: usize,
    pub parsed_events: usize,
    pub inserted_or_updated_events: usize,
    pub skipped_events: usize,
    pub db_path: String,
    pub parser_diagnostics: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummaryRequest {
    pub window: UsageWindow,
    pub include_archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageDashboardRequest {
    pub window: UsageWindow,
    pub include_archived: bool,
    pub search: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub pricing_confidence: Option<String>,
    pub sort_key: Option<String>,
    pub sort_direction: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    pub preset: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

impl Default for UsageWindow {
    fn default() -> Self {
        Self {
            preset: "all".to_string(),
            from: None,
            to: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub refreshed_at: Option<String>,
    pub scanned_files: usize,
    pub parsed_events: usize,
    pub skipped_events: usize,
    pub total_calls: usize,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub uncached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub estimated_cost_usd: f64,
    pub pricing_coverage: UsagePricingCoverage,
    pub diagnostics: UsageDiagnostics,
    pub top_threads: Vec<UsageThreadSummary>,
    pub recent_calls: Vec<UsageCallRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageDashboard {
    #[serde(flatten)]
    pub summary: UsageSummary,
    pub model_options: Vec<String>,
    pub effort_options: Vec<String>,
    pub pricing_confidence_options: Vec<String>,
    pub status_chips: Vec<UsageStatusChip>,
    pub investigation_presets: Vec<UsageInvestigationPreset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageStatusChip {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageInvestigationPreset {
    pub id: String,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsagePricingCoverage {
    pub priced_tokens: i64,
    pub unpriced_tokens: i64,
    pub priced_token_ratio: f64,
    pub unknown_models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageDiagnostics {
    pub parser_diagnostics: BTreeMap<String, i64>,
    pub skipped_events: usize,
    pub unknown_models: Vec<String>,
    pub low_cache_threads: Vec<UsageThreadSummary>,
    pub high_context_calls: Vec<UsageCallRow>,
    pub last_refresh_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageThreadSummary {
    pub thread_key: String,
    pub is_archived_scope: bool,
    pub thread_label: String,
    pub first_event_timestamp: Option<String>,
    pub call_count: usize,
    pub session_count: usize,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub uncached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub latest_event_timestamp: Option<String>,
    pub avg_cache_ratio: f64,
    pub max_context_window_percent: Option<f64>,
    pub max_recommendation_score: f64,
    pub primary_recommendation: Option<String>,
    pub call_initiator_summary: Option<String>,
    pub archived_call_count: usize,
    pub updated_at: Option<String>,
    pub estimated_cost_usd: f64,
    pub usage_credits: f64,
    pub cache_ratio: f64,
    pub is_archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageCallRow {
    pub record_id: String,
    pub session_id: String,
    pub thread_name: Option<String>,
    pub session_updated_at: Option<String>,
    pub event_timestamp: String,
    pub source_file: String,
    pub line_number: i64,
    pub turn_id: Option<String>,
    pub turn_timestamp: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub current_date: Option<String>,
    pub timezone: Option<String>,
    pub call_initiator: Option<String>,
    pub call_initiator_reason: Option<String>,
    pub call_initiator_confidence: Option<f64>,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub uncached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
    pub cumulative_total_tokens: i64,
    pub cache_ratio: f64,
    pub is_archived: bool,
    pub thread_key: Option<String>,
    pub thread_call_index: Option<i64>,
    pub previous_record_id: Option<String>,
    pub next_record_id: Option<String>,
    pub thread_source: Option<String>,
    pub subagent_type: Option<String>,
    pub agent_role: Option<String>,
    pub agent_nickname: Option<String>,
    pub parent_session_id: Option<String>,
    pub parent_thread_name: Option<String>,
    pub parent_session_updated_at: Option<String>,
    pub model_context_window: Option<i64>,
    pub context_window_percent: Option<f64>,
    pub rate_limit_plan_type: Option<String>,
    pub rate_limit_limit_id: Option<String>,
    pub rate_limit_primary_used_percent: Option<f64>,
    pub rate_limit_primary_window_minutes: Option<i64>,
    pub rate_limit_primary_resets_at: Option<String>,
    pub rate_limit_secondary_used_percent: Option<f64>,
    pub rate_limit_secondary_window_minutes: Option<i64>,
    pub rate_limit_secondary_resets_at: Option<String>,
    pub reasoning_output_ratio: f64,
    pub estimated_cost_usd: f64,
    pub usage_credits: f64,
    pub pricing_model: Option<String>,
    pub pricing_estimated: bool,
    pub pricing_confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ParserState {
    session_id: Option<String>,
    current_turn: Option<CurrentTurn>,
    last_cumulative_total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct CurrentTurn {
    turn_id: Option<String>,
    turn_timestamp: Option<String>,
    cwd: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    current_date: Option<String>,
    timezone: Option<String>,
}

struct SourceParsePlan {
    path: PathBuf,
    is_archived: bool,
    start_byte: u64,
    start_line: i64,
    initial_state: ParserState,
    replace_existing: bool,
}

struct ParsedSource {
    path: PathBuf,
    events: Vec<UsageCallRow>,
    diagnostics: BTreeMap<String, i64>,
    state: ParserState,
    parsed_until_byte: u64,
    parsed_until_line: i64,
}

struct SourceLog {
    path: PathBuf,
    is_archived: bool,
}

#[derive(Clone, Copy)]
struct UsageRate {
    pricing_model: &'static str,
    estimated: bool,
    input_per_million: f64,
    cached_input_per_million: f64,
    output_per_million: f64,
}

struct UsageCostEstimate {
    estimated_cost_usd: f64,
    pricing_model: String,
    pricing_estimated: bool,
}

pub fn usage_db_path(home_root: &Path) -> PathBuf {
    home_root.join(".codex/lam/usage/usage.sqlite3")
}

pub fn refresh_usage_index(home_root: &Path) -> Result<UsageRefreshResult> {
    refresh_usage_index_with_options(home_root, false)
}

pub fn refresh_usage_index_with_options(
    home_root: &Path,
    include_archived: bool,
) -> Result<UsageRefreshResult> {
    let _guard = REFRESH_LOCK
        .lock()
        .map_err(|_| AppError::new("USAGE_REFRESH_LOCK", "usage refresh lock is poisoned"))?;
    refresh_usage_index_unlocked(home_root, include_archived)
}

pub fn try_refresh_usage_index_with_options(
    home_root: &Path,
    include_archived: bool,
) -> Result<Option<UsageRefreshResult>> {
    let Ok(_guard) = REFRESH_LOCK.try_lock() else {
        return Ok(None);
    };
    refresh_usage_index_unlocked(home_root, include_archived).map(Some)
}

fn refresh_usage_index_unlocked(
    home_root: &Path,
    include_archived: bool,
) -> Result<UsageRefreshResult> {
    let db_path = usage_db_path(home_root);
    prepare_usage_dir(&db_path)?;
    let mut conn = open_usage_db(&db_path)?;
    init_usage_db(&conn)?;

    let codex_home = home_root.join(".codex");
    let session_index = load_session_index(&codex_home);
    let logs = find_session_logs(&codex_home, include_archived)?;
    let plans = source_logs_requiring_parse(&conn, &logs)?;
    let mut parsed = Vec::new();
    let mut diagnostics = BTreeMap::new();

    for plan in plans {
        let result = parse_source_file(&plan, &session_index)?;
        merge_diagnostics(&mut diagnostics, &result.diagnostics);
        parsed.push((plan, result));
    }

    let parsed_files = parsed.len();
    let parsed_events = parsed
        .iter()
        .map(|(_, source)| source.events.len())
        .sum::<usize>();
    let skipped_events = diagnostics.get("skipped_events").copied().unwrap_or(0) as usize;
    let (inserted_or_updated_events, deleted_rows) =
        apply_parsed_sources(&mut conn, &parsed, logs.len(), skipped_events)?;
    compact_usage_db_after_refresh(&mut conn, deleted_rows > 0)?;

    Ok(UsageRefreshResult {
        scanned_files: logs.len(),
        parsed_files,
        parsed_events,
        inserted_or_updated_events,
        skipped_events,
        db_path: db_path.to_string_lossy().to_string(),
        parser_diagnostics: diagnostics,
    })
}

pub fn get_usage_summary(home_root: &Path, req: UsageSummaryRequest) -> Result<UsageSummary> {
    let db_path = usage_db_path(home_root);
    if !db_path.exists() {
        return Ok(UsageSummary::default());
    }
    let conn = open_usage_db(&db_path)?;
    init_usage_db(&conn)?;
    let refreshed_at = get_meta(&conn, "refreshed_at")?;
    let scanned_files = get_meta(&conn, "scanned_files")?
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let parsed_events = get_meta(&conn, "parsed_events")?
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let skipped_events = get_meta(&conn, "skipped_events")?
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let filter = SummaryFilter::new(&req);
    let totals = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(total_tokens),0), COALESCE(SUM(input_tokens),0),
            COALESCE(SUM(cached_input_tokens),0), COALESCE(SUM(uncached_input_tokens),0),
            COALESCE(SUM(output_tokens),0), COALESCE(SUM(reasoning_output_tokens),0)
         FROM usage_events
         WHERE (?1 OR is_archived = 0)
           AND (?2 IS NULL OR event_timestamp >= ?2)
           AND (?3 IS NULL OR event_timestamp < ?3)",
            params![
                req.include_archived,
                filter.from.as_deref(),
                filter.to.as_deref()
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        )
        .map_err(db_error)?;
    let top_threads = query_top_threads(&conn, &req, &filter)?;
    let recent_calls = query_recent_calls(&conn, &req, &filter)?;
    let (estimated_cost_usd, pricing_coverage) =
        estimate_summary_cost(&model_totals(&conn, &req, &filter)?);
    let mut diagnostics = usage_diagnostics(&conn, &req, &filter, &top_threads, &recent_calls)?;
    diagnostics.skipped_events = skipped_events;
    Ok(UsageSummary {
        refreshed_at,
        scanned_files,
        parsed_events,
        skipped_events,
        total_calls: totals.0 as usize,
        total_tokens: totals.1,
        input_tokens: totals.2,
        cached_input_tokens: totals.3,
        uncached_input_tokens: totals.4,
        output_tokens: totals.5,
        reasoning_output_tokens: totals.6,
        estimated_cost_usd,
        pricing_coverage,
        diagnostics,
        top_threads,
        recent_calls,
    })
}

pub fn get_usage_dashboard(home_root: &Path, req: UsageDashboardRequest) -> Result<UsageDashboard> {
    let summary_req = UsageSummaryRequest {
        window: req.window,
        include_archived: req.include_archived,
    };
    let mut summary = get_usage_summary(home_root, summary_req)?;
    if let Some(model) = req.model.as_deref().filter(|value| !value.is_empty()) {
        summary
            .recent_calls
            .retain(|row| row.model.as_deref() == Some(model));
    }
    if let Some(effort) = req.effort.as_deref().filter(|value| !value.is_empty()) {
        summary
            .recent_calls
            .retain(|row| row.effort.as_deref() == Some(effort));
    }
    if let Some(confidence) = req
        .pricing_confidence
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        summary
            .recent_calls
            .retain(|row| row.pricing_confidence == confidence);
    }
    if let Some(search) = req
        .search
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let needle = search.to_ascii_lowercase();
        summary.recent_calls.retain(|row| {
            row.thread_name
                .as_deref()
                .unwrap_or(&row.session_id)
                .to_ascii_lowercase()
                .contains(&needle)
                || row
                    .cwd
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains(&needle)
                || row
                    .model
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains(&needle)
        });
    }
    if let Some(limit) = req.limit.filter(|limit| *limit > 0) {
        summary.recent_calls.truncate(limit);
    }
    let mut model_options = summary
        .recent_calls
        .iter()
        .filter_map(|row| row.model.clone())
        .collect::<Vec<_>>();
    model_options.sort();
    model_options.dedup();
    let mut effort_options = summary
        .recent_calls
        .iter()
        .filter_map(|row| row.effort.clone())
        .collect::<Vec<_>>();
    effort_options.sort();
    effort_options.dedup();
    let mut pricing_confidence_options = summary
        .recent_calls
        .iter()
        .map(|row| row.pricing_confidence.clone())
        .collect::<Vec<_>>();
    pricing_confidence_options.sort();
    pricing_confidence_options.dedup();
    Ok(UsageDashboard {
        status_chips: vec![
            UsageStatusChip {
                label: "Pricing source".to_string(),
                value: "local rate card".to_string(),
            },
            UsageStatusChip {
                label: "Privacy mode".to_string(),
                value: "aggregate only".to_string(),
            },
            UsageStatusChip {
                label: "Parser diagnostics".to_string(),
                value: summary.skipped_events.to_string(),
            },
        ],
        investigation_presets: vec![
            UsageInvestigationPreset {
                id: "low-cache".to_string(),
                label: "Low cache reuse".to_string(),
                description: "Threads with large uncached input".to_string(),
            },
            UsageInvestigationPreset {
                id: "high-context".to_string(),
                label: "High context".to_string(),
                description: "Calls near the model context window".to_string(),
            },
            UsageInvestigationPreset {
                id: "unknown-models".to_string(),
                label: "Unknown pricing".to_string(),
                description: "Models missing a local price".to_string(),
            },
        ],
        summary,
        model_options,
        effort_options,
        pricing_confidence_options,
    })
}

pub fn init_usage_db(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        PRAGMA busy_timeout = 5000;
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = WAL;
        CREATE TABLE IF NOT EXISTS usage_events (
            record_id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            thread_name TEXT,
            session_updated_at TEXT,
            event_timestamp TEXT NOT NULL,
            source_file TEXT NOT NULL,
            is_archived INTEGER NOT NULL DEFAULT 0,
            line_number INTEGER NOT NULL,
            turn_id TEXT,
            turn_timestamp TEXT,
            cwd TEXT,
            model TEXT,
            effort TEXT,
            current_date TEXT,
            timezone TEXT,
            call_initiator TEXT,
            call_initiator_reason TEXT,
            call_initiator_confidence REAL,
            input_tokens INTEGER NOT NULL,
            cached_input_tokens INTEGER NOT NULL,
            uncached_input_tokens INTEGER NOT NULL,
            output_tokens INTEGER NOT NULL,
            reasoning_output_tokens INTEGER NOT NULL,
            total_tokens INTEGER NOT NULL,
            cumulative_input_tokens INTEGER NOT NULL,
            cumulative_cached_input_tokens INTEGER NOT NULL,
            cumulative_output_tokens INTEGER NOT NULL,
            cumulative_reasoning_output_tokens INTEGER NOT NULL,
            cumulative_total_tokens INTEGER NOT NULL,
            cache_ratio REAL NOT NULL,
            thread_key TEXT,
            thread_call_index INTEGER,
            previous_record_id TEXT,
            next_record_id TEXT,
            thread_source TEXT,
            subagent_type TEXT,
            agent_role TEXT,
            agent_nickname TEXT,
            parent_session_id TEXT,
            parent_thread_name TEXT,
            parent_session_updated_at TEXT,
            model_context_window INTEGER,
            context_window_percent REAL,
            rate_limit_plan_type TEXT,
            rate_limit_limit_id TEXT,
            rate_limit_primary_used_percent REAL,
            rate_limit_primary_window_minutes INTEGER,
            rate_limit_primary_resets_at TEXT,
            rate_limit_secondary_used_percent REAL,
            rate_limit_secondary_window_minutes INTEGER,
            rate_limit_secondary_resets_at TEXT,
            reasoning_output_ratio REAL NOT NULL DEFAULT 0,
            estimated_cost_usd REAL NOT NULL DEFAULT 0,
            usage_credits REAL NOT NULL DEFAULT 0,
            pricing_model TEXT,
            pricing_estimated INTEGER NOT NULL DEFAULT 0,
            pricing_confidence TEXT NOT NULL DEFAULT 'unknown'
        );
        CREATE INDEX IF NOT EXISTS idx_usage_events_source ON usage_events(source_file);
        CREATE INDEX IF NOT EXISTS idx_usage_events_time ON usage_events(event_timestamp);
        CREATE TABLE IF NOT EXISTS source_files (
            source_file TEXT PRIMARY KEY,
            is_archived INTEGER NOT NULL DEFAULT 0,
            size_bytes INTEGER NOT NULL,
            mtime_ns INTEGER NOT NULL,
            parsed_until_line INTEGER NOT NULL,
            parsed_until_byte INTEGER NOT NULL,
            parser_adapter TEXT NOT NULL,
            parser_state_json TEXT NOT NULL,
            parser_diagnostics_json TEXT NOT NULL,
            source_hash TEXT,
            parser_cursor_json TEXT,
            parser_state TEXT,
            archive_scope TEXT,
            last_indexed_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS thread_summaries (
            thread_key TEXT PRIMARY KEY,
            is_archived_scope INTEGER NOT NULL DEFAULT 0,
            thread_label TEXT NOT NULL,
            first_event_timestamp TEXT,
            latest_event_timestamp TEXT,
            call_count INTEGER NOT NULL DEFAULT 0,
            session_count INTEGER NOT NULL DEFAULT 0,
            input_tokens INTEGER NOT NULL DEFAULT 0,
            cached_input_tokens INTEGER NOT NULL DEFAULT 0,
            uncached_input_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            reasoning_output_tokens INTEGER NOT NULL DEFAULT 0,
            total_tokens INTEGER NOT NULL DEFAULT 0,
            estimated_cost_usd REAL NOT NULL DEFAULT 0,
            usage_credits REAL NOT NULL DEFAULT 0,
            avg_cache_ratio REAL NOT NULL DEFAULT 0,
            max_context_window_percent REAL,
            max_recommendation_score REAL NOT NULL DEFAULT 0,
            primary_recommendation TEXT,
            call_initiator_summary TEXT,
            archived_call_count INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS aggregate_diagnostic_facts (
            record_id TEXT PRIMARY KEY,
            fact_type TEXT NOT NULL,
            fact_name TEXT NOT NULL,
            fact_category TEXT NOT NULL,
            event_count INTEGER NOT NULL DEFAULT 0,
            confidence REAL NOT NULL DEFAULT 0,
            first_event_timestamp TEXT,
            last_event_timestamp TEXT,
            first_source_line INTEGER,
            last_source_line INTEGER,
            evidence_scope TEXT,
            raw_content_included INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS diagnostic_snapshots (
            snapshot_id TEXT PRIMARY KEY,
            created_at TEXT NOT NULL,
            parser_diagnostics_json TEXT NOT NULL,
            pricing_coverage_json TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS refresh_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        ",
    )
    .map_err(db_error)?;
    let usage_event_columns = [
        ("is_archived", "INTEGER NOT NULL DEFAULT 0"),
        ("session_updated_at", "TEXT"),
        ("turn_timestamp", "TEXT"),
        ("current_date", "TEXT"),
        ("timezone", "TEXT"),
        ("call_initiator", "TEXT"),
        ("call_initiator_reason", "TEXT"),
        ("call_initiator_confidence", "REAL"),
        ("thread_key", "TEXT"),
        ("thread_call_index", "INTEGER"),
        ("previous_record_id", "TEXT"),
        ("next_record_id", "TEXT"),
        ("thread_source", "TEXT"),
        ("subagent_type", "TEXT"),
        ("agent_role", "TEXT"),
        ("agent_nickname", "TEXT"),
        ("parent_session_id", "TEXT"),
        ("parent_thread_name", "TEXT"),
        ("parent_session_updated_at", "TEXT"),
        ("model_context_window", "INTEGER"),
        ("context_window_percent", "REAL"),
        ("rate_limit_plan_type", "TEXT"),
        ("rate_limit_limit_id", "TEXT"),
        ("rate_limit_primary_used_percent", "REAL"),
        ("rate_limit_primary_window_minutes", "INTEGER"),
        ("rate_limit_primary_resets_at", "TEXT"),
        ("rate_limit_secondary_used_percent", "REAL"),
        ("rate_limit_secondary_window_minutes", "INTEGER"),
        ("rate_limit_secondary_resets_at", "TEXT"),
        ("reasoning_output_ratio", "REAL NOT NULL DEFAULT 0"),
        ("estimated_cost_usd", "REAL NOT NULL DEFAULT 0"),
        ("usage_credits", "REAL NOT NULL DEFAULT 0"),
        ("pricing_model", "TEXT"),
        ("pricing_estimated", "INTEGER NOT NULL DEFAULT 0"),
        ("pricing_confidence", "TEXT NOT NULL DEFAULT 'unknown'"),
    ];
    for (column, definition) in usage_event_columns {
        ensure_column(
            conn,
            "usage_events",
            column,
            &format!("ALTER TABLE usage_events ADD COLUMN {column} {definition}"),
        )?;
    }
    for (column, definition) in [
        ("source_hash", "TEXT"),
        ("parser_cursor_json", "TEXT"),
        ("parser_state", "TEXT"),
        ("archive_scope", "TEXT"),
    ] {
        ensure_column(
            conn,
            "source_files",
            column,
            &format!("ALTER TABLE source_files ADD COLUMN {column} {definition}"),
        )?;
    }
    Ok(())
}

fn ensure_column(conn: &Connection, table: &str, column: &str, sql: &str) -> Result<()> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(db_error)?;
    let exists = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(db_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(db_error)?
        .iter()
        .any(|name| name == column);
    if !exists {
        conn.execute(sql, []).map_err(db_error)?;
    }
    Ok(())
}

fn prepare_usage_dir(db_path: &Path) -> Result<()> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        }
    }
    Ok(())
}

fn open_usage_db(path: &Path) -> Result<Connection> {
    Connection::open(path).map_err(db_error)
}

fn source_logs_requiring_parse(
    conn: &Connection,
    logs: &[SourceLog],
) -> Result<Vec<SourceParsePlan>> {
    let mut plans = Vec::new();
    for log in logs {
        let path = &log.path;
        let metadata = source_metadata(path)?;
        let row = conn
            .query_row(
                "SELECT size_bytes, mtime_ns, parsed_until_line, parsed_until_byte,
                    parser_adapter, parser_state_json FROM source_files WHERE source_file = ?",
                [path.to_string_lossy().to_string()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(db_error)?;
        let Some((
            previous_size,
            previous_mtime_ns,
            previous_line,
            previous_byte,
            adapter,
            state_json,
        )) = row
        else {
            plans.push(SourceParsePlan {
                path: path.clone(),
                is_archived: log.is_archived,
                start_byte: 0,
                start_line: 0,
                initial_state: ParserState::default(),
                replace_existing: true,
            });
            continue;
        };
        if previous_size == metadata.size_bytes && previous_mtime_ns == metadata.mtime_ns {
            continue;
        }
        let state = serde_json::from_str::<ParserState>(&state_json).ok();
        if adapter == PARSER_ADAPTER_VERSION
            && state.is_some()
            && metadata.size_bytes > previous_size
            && previous_byte > 0
            && previous_byte <= previous_size
        {
            plans.push(SourceParsePlan {
                path: path.clone(),
                is_archived: log.is_archived,
                start_byte: previous_byte as u64,
                start_line: previous_line,
                initial_state: state.unwrap_or_default(),
                replace_existing: false,
            });
        } else {
            plans.push(SourceParsePlan {
                path: path.clone(),
                is_archived: log.is_archived,
                start_byte: 0,
                start_line: 0,
                initial_state: ParserState::default(),
                replace_existing: true,
            });
        }
    }
    Ok(plans)
}

fn parse_source_file(
    plan: &SourceParsePlan,
    session_index: &HashMap<String, String>,
) -> Result<ParsedSource> {
    let mut reader = BufReader::new(File::open(&plan.path)?);
    if plan.start_byte > 0 {
        reader.seek(SeekFrom::Start(plan.start_byte))?;
    }
    let mut state = plan.initial_state.clone();
    let file_session_id = session_id_from_path(&plan.path);
    if state.session_id.is_none() {
        state.session_id = file_session_id;
    }
    let mut events = Vec::new();
    let mut diagnostics = BTreeMap::new();
    let mut byte_offset = plan.start_byte;
    let mut committed_byte = plan.start_byte;
    let mut line_number = plan.start_line;
    let mut committed_line = plan.start_line;
    loop {
        let mut raw = Vec::new();
        let read = reader.read_until(b'\n', &mut raw)?;
        if read == 0 {
            break;
        }
        byte_offset += read as u64;
        if !raw.ends_with(b"\n") {
            increment(&mut diagnostics, "partial_trailing_line");
            break;
        }
        line_number += 1;
        committed_line = line_number;
        committed_byte = byte_offset;
        let Ok(value) = serde_json::from_slice::<Value>(&raw) else {
            increment(&mut diagnostics, "invalid_json");
            continue;
        };
        parse_envelope(
            &plan.path,
            line_number,
            &value,
            &mut state,
            session_index,
            &mut events,
            &mut diagnostics,
        );
    }
    Ok(ParsedSource {
        path: plan.path.clone(),
        events,
        diagnostics,
        state,
        parsed_until_byte: committed_byte,
        parsed_until_line: committed_line,
    })
}

fn parse_envelope(
    path: &Path,
    line_number: i64,
    value: &Value,
    state: &mut ParserState,
    session_index: &HashMap<String, String>,
    events: &mut Vec<UsageCallRow>,
    diagnostics: &mut BTreeMap<String, i64>,
) {
    let Some(payload) = value.get("payload").and_then(Value::as_object) else {
        increment(diagnostics, "missing_payload");
        return;
    };
    let entry_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let timestamp = value
        .get("timestamp")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if entry_type == "session_meta" {
        if state.session_id.is_none() {
            state.session_id = payload
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        return;
    }
    if entry_type == "turn_context" {
        state.current_turn = Some(CurrentTurn {
            turn_id: payload
                .get("turn_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            turn_timestamp: Some(timestamp),
            cwd: payload
                .get("cwd")
                .and_then(Value::as_str)
                .map(str::to_string),
            model: payload
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_string),
            effort: payload
                .get("effort")
                .and_then(Value::as_str)
                .map(str::to_string),
            current_date: payload
                .get("current_date")
                .and_then(Value::as_str)
                .map(str::to_string),
            timezone: payload
                .get("timezone")
                .and_then(Value::as_str)
                .map(str::to_string),
        });
        return;
    }
    if entry_type != "event_msg" {
        return;
    }
    let payload_type = payload
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if payload_type != "token_count" {
        if !KNOWN_NON_TOKEN_EVENT_MSG_TYPES.contains(&payload_type) {
            increment(diagnostics, "unknown_event_shape");
        }
        return;
    }
    let Some(info) = payload.get("info").and_then(Value::as_object) else {
        increment(diagnostics, "missing_info");
        increment(diagnostics, "skipped_events");
        return;
    };
    let Some(total_usage) = info.get("total_token_usage").and_then(Value::as_object) else {
        increment(diagnostics, "missing_total_token_usage");
        increment(diagnostics, "skipped_events");
        return;
    };
    let Some(last_usage) = info.get("last_token_usage").and_then(Value::as_object) else {
        increment(diagnostics, "missing_last_token_usage");
        increment(diagnostics, "skipped_events");
        return;
    };
    let Some(cumulative_total_tokens) = usage_int(total_usage, "total_tokens") else {
        increment(diagnostics, "missing_cumulative_total");
        increment(diagnostics, "skipped_events");
        return;
    };
    if cumulative_total_tokens <= state.last_cumulative_total {
        increment(diagnostics, "duplicate_cumulative_total");
        return;
    }
    let input_tokens = usage_int(last_usage, "input_tokens").unwrap_or(0);
    let cached_input_tokens = usage_int(last_usage, "cached_input_tokens").unwrap_or(0);
    let uncached_input_tokens = (input_tokens - cached_input_tokens).max(0);
    let output_tokens = usage_int(last_usage, "output_tokens").unwrap_or(0);
    let reasoning_output_tokens = usage_int(last_usage, "reasoning_output_tokens").unwrap_or(0);
    let total_tokens = usage_int(last_usage, "total_tokens").unwrap_or(0);
    let model_context_window = nullable_usage_int(
        info.get("model_context_window"),
        diagnostics,
        "invalid_model_context_window",
    );
    let context_window_percent = model_context_window.map(|window| {
        if window > 0 {
            input_tokens as f64 / window as f64
        } else {
            0.0
        }
    });
    let session_id = state
        .session_id
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let turn = state.current_turn.clone().unwrap_or_default();
    let model = turn.model.clone();
    let pricing = estimate_cost(
        model.as_deref(),
        input_tokens,
        cached_input_tokens,
        output_tokens,
    );
    let rate_limits = info.get("rate_limits").or_else(|| info.get("rate_limit"));
    let primary = rate_limit_number(rate_limits, &["primary_used_percent", "primaryUsedPercent"]);
    let secondary = rate_limit_number(
        rate_limits,
        &["secondary_used_percent", "secondaryUsedPercent"],
    );
    events.push(UsageCallRow {
        record_id: format!(
            "{}:{}:{}",
            path.to_string_lossy(),
            line_number,
            cumulative_total_tokens
        ),
        session_id: session_id.clone(),
        thread_name: session_index.get(&session_id).cloned(),
        session_updated_at: None,
        event_timestamp: timestamp,
        source_file: path.to_string_lossy().to_string(),
        line_number,
        turn_id: turn.turn_id,
        turn_timestamp: turn.turn_timestamp,
        cwd: turn.cwd,
        model,
        effort: turn.effort,
        current_date: turn.current_date,
        timezone: turn.timezone,
        call_initiator: payload
            .get("call_initiator")
            .or_else(|| payload.get("callInitiator"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| Some("codex".to_string())),
        call_initiator_reason: payload
            .get("call_initiator_reason")
            .or_else(|| payload.get("callInitiatorReason"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| Some("token_count".to_string())),
        call_initiator_confidence: payload
            .get("call_initiator_confidence")
            .or_else(|| payload.get("callInitiatorConfidence"))
            .and_then(Value::as_f64)
            .or(Some(0.75)),
        input_tokens,
        cached_input_tokens,
        uncached_input_tokens,
        output_tokens,
        reasoning_output_tokens,
        total_tokens,
        cumulative_total_tokens,
        cache_ratio: if input_tokens > 0 {
            cached_input_tokens as f64 / input_tokens as f64
        } else {
            0.0
        },
        is_archived: false,
        thread_key: None,
        thread_call_index: None,
        previous_record_id: None,
        next_record_id: None,
        thread_source: Some(if session_index.contains_key(&session_id) {
            "session_index".to_string()
        } else {
            "session_id".to_string()
        }),
        subagent_type: payload
            .get("subagent_type")
            .or_else(|| payload.get("subagentType"))
            .and_then(Value::as_str)
            .map(str::to_string),
        agent_role: payload
            .get("agent_role")
            .or_else(|| payload.get("agentRole"))
            .and_then(Value::as_str)
            .map(str::to_string),
        agent_nickname: payload
            .get("agent_nickname")
            .or_else(|| payload.get("agentNickname"))
            .and_then(Value::as_str)
            .map(str::to_string),
        parent_session_id: payload
            .get("parent_session_id")
            .or_else(|| payload.get("parentSessionId"))
            .and_then(Value::as_str)
            .map(str::to_string),
        parent_thread_name: payload
            .get("parent_thread_name")
            .or_else(|| payload.get("parentThreadName"))
            .and_then(Value::as_str)
            .map(str::to_string),
        parent_session_updated_at: payload
            .get("parent_session_updated_at")
            .or_else(|| payload.get("parentSessionUpdatedAt"))
            .and_then(Value::as_str)
            .map(str::to_string),
        model_context_window,
        context_window_percent,
        rate_limit_plan_type: rate_limit_text(rate_limits, &["plan_type", "planType"]),
        rate_limit_limit_id: rate_limit_text(rate_limits, &["limit_id", "limitId"]),
        rate_limit_primary_used_percent: primary,
        rate_limit_primary_window_minutes: rate_limit_int(
            rate_limits,
            &["primary_window_minutes", "primaryWindowMinutes"],
        ),
        rate_limit_primary_resets_at: rate_limit_text(
            rate_limits,
            &["primary_resets_at", "primaryResetsAt"],
        ),
        rate_limit_secondary_used_percent: secondary,
        rate_limit_secondary_window_minutes: rate_limit_int(
            rate_limits,
            &["secondary_window_minutes", "secondaryWindowMinutes"],
        ),
        rate_limit_secondary_resets_at: rate_limit_text(
            rate_limits,
            &["secondary_resets_at", "secondaryResetsAt"],
        ),
        reasoning_output_ratio: if output_tokens > 0 {
            reasoning_output_tokens as f64 / output_tokens as f64
        } else {
            0.0
        },
        estimated_cost_usd: pricing
            .as_ref()
            .map(|estimate| estimate.estimated_cost_usd)
            .unwrap_or(0.0),
        usage_credits: pricing
            .as_ref()
            .map(|estimate| estimate.estimated_cost_usd)
            .unwrap_or(0.0),
        pricing_model: pricing
            .as_ref()
            .map(|estimate| estimate.pricing_model.clone()),
        pricing_estimated: pricing
            .as_ref()
            .map(|estimate| estimate.pricing_estimated)
            .unwrap_or(false),
        pricing_confidence: if pricing.is_some() {
            "priced".to_string()
        } else {
            "unknown".to_string()
        },
    });
    state.last_cumulative_total = cumulative_total_tokens;
}

fn apply_parsed_sources(
    conn: &mut Connection,
    parsed: &[(SourceParsePlan, ParsedSource)],
    scanned_files: usize,
    skipped_events: usize,
) -> Result<(usize, usize)> {
    let tx = conn.transaction().map_err(db_error)?;
    let now = chrono::Utc::now().to_rfc3339();
    let mut inserted = 0;
    let mut deleted = 0;
    for (plan, source) in parsed {
        if plan.replace_existing {
            deleted += tx
                .execute(
                    "DELETE FROM usage_events WHERE source_file = ?",
                    [source.path.to_string_lossy().to_string()],
                )
                .map_err(db_error)?;
        }
        for event in &source.events {
            tx.execute(
                "INSERT INTO usage_events (
                    record_id, session_id, thread_name, session_updated_at, event_timestamp,
                    source_file, is_archived, line_number, turn_id, turn_timestamp, cwd, model,
                    effort, current_date, timezone, call_initiator, call_initiator_reason,
                    call_initiator_confidence, input_tokens, cached_input_tokens,
                    uncached_input_tokens, output_tokens, reasoning_output_tokens, total_tokens,
                    cumulative_input_tokens, cumulative_cached_input_tokens, cumulative_output_tokens,
                    cumulative_reasoning_output_tokens, cumulative_total_tokens, cache_ratio,
                    thread_source, subagent_type, agent_role, agent_nickname, parent_session_id,
                    parent_thread_name, parent_session_updated_at, model_context_window,
                    context_window_percent, rate_limit_plan_type, rate_limit_limit_id,
                    rate_limit_primary_used_percent, rate_limit_primary_window_minutes,
                    rate_limit_primary_resets_at, rate_limit_secondary_used_percent,
                    rate_limit_secondary_window_minutes, rate_limit_secondary_resets_at,
                    reasoning_output_ratio, estimated_cost_usd, usage_credits, pricing_model,
                    pricing_estimated, pricing_confidence
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                    ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29,
                    ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42, ?43,
                    ?44, ?45, ?46, ?47, ?48, ?49, ?50, ?51, ?52, ?53)
                ON CONFLICT(record_id) DO UPDATE SET
                    thread_name=excluded.thread_name,
                    session_updated_at=excluded.session_updated_at,
                    event_timestamp=excluded.event_timestamp,
                    is_archived=excluded.is_archived,
                    turn_id=excluded.turn_id,
                    turn_timestamp=excluded.turn_timestamp,
                    cwd=excluded.cwd,
                    model=excluded.model,
                    effort=excluded.effort,
                    current_date=excluded.current_date,
                    timezone=excluded.timezone,
                    call_initiator=excluded.call_initiator,
                    call_initiator_reason=excluded.call_initiator_reason,
                    call_initiator_confidence=excluded.call_initiator_confidence,
                    input_tokens=excluded.input_tokens,
                    cached_input_tokens=excluded.cached_input_tokens,
                    uncached_input_tokens=excluded.uncached_input_tokens,
                    output_tokens=excluded.output_tokens,
                    reasoning_output_tokens=excluded.reasoning_output_tokens,
                    total_tokens=excluded.total_tokens,
                    cumulative_total_tokens=excluded.cumulative_total_tokens,
                    cache_ratio=excluded.cache_ratio,
                    thread_source=excluded.thread_source,
                    subagent_type=excluded.subagent_type,
                    agent_role=excluded.agent_role,
                    agent_nickname=excluded.agent_nickname,
                    parent_session_id=excluded.parent_session_id,
                    parent_thread_name=excluded.parent_thread_name,
                    parent_session_updated_at=excluded.parent_session_updated_at,
                    model_context_window=excluded.model_context_window,
                    context_window_percent=excluded.context_window_percent,
                    rate_limit_plan_type=excluded.rate_limit_plan_type,
                    rate_limit_limit_id=excluded.rate_limit_limit_id,
                    rate_limit_primary_used_percent=excluded.rate_limit_primary_used_percent,
                    rate_limit_primary_window_minutes=excluded.rate_limit_primary_window_minutes,
                    rate_limit_primary_resets_at=excluded.rate_limit_primary_resets_at,
                    rate_limit_secondary_used_percent=excluded.rate_limit_secondary_used_percent,
                    rate_limit_secondary_window_minutes=excluded.rate_limit_secondary_window_minutes,
                    rate_limit_secondary_resets_at=excluded.rate_limit_secondary_resets_at,
                    reasoning_output_ratio=excluded.reasoning_output_ratio,
                    estimated_cost_usd=excluded.estimated_cost_usd,
                    usage_credits=excluded.usage_credits,
                    pricing_model=excluded.pricing_model,
                    pricing_estimated=excluded.pricing_estimated,
                    pricing_confidence=excluded.pricing_confidence",
                params![
                    event.record_id,
                    event.session_id,
                    event.thread_name,
                    event.session_updated_at,
                    event.event_timestamp,
                    event.source_file,
                    plan.is_archived,
                    event.line_number,
                    event.turn_id,
                    event.turn_timestamp,
                    event.cwd,
                    event.model,
                    event.effort,
                    event.current_date,
                    event.timezone,
                    event.call_initiator,
                    event.call_initiator_reason,
                    event.call_initiator_confidence,
                    event.input_tokens,
                    event.cached_input_tokens,
                    event.uncached_input_tokens,
                    event.output_tokens,
                    event.reasoning_output_tokens,
                    event.total_tokens,
                    event.input_tokens,
                    event.cached_input_tokens,
                    event.output_tokens,
                    event.reasoning_output_tokens,
                    event.cumulative_total_tokens,
                    event.cache_ratio,
                    event.thread_source,
                    event.subagent_type,
                    event.agent_role,
                    event.agent_nickname,
                    event.parent_session_id,
                    event.parent_thread_name,
                    event.parent_session_updated_at,
                    event.model_context_window,
                    event.context_window_percent,
                    event.rate_limit_plan_type,
                    event.rate_limit_limit_id,
                    event.rate_limit_primary_used_percent,
                    event.rate_limit_primary_window_minutes,
                    event.rate_limit_primary_resets_at,
                    event.rate_limit_secondary_used_percent,
                    event.rate_limit_secondary_window_minutes,
                    event.rate_limit_secondary_resets_at,
                    event.reasoning_output_ratio,
                    event.estimated_cost_usd,
                    event.usage_credits,
                    event.pricing_model,
                    event.pricing_estimated,
                    event.pricing_confidence
                ],
            )
            .map_err(db_error)?;
            inserted += 1;
        }
        let metadata = source_metadata(&source.path)?;
        tx.execute(
            "INSERT INTO source_files (
                source_file, is_archived, size_bytes, mtime_ns, parsed_until_line,
                parsed_until_byte, parser_adapter, parser_state_json, parser_diagnostics_json,
                last_indexed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(source_file) DO UPDATE SET
                is_archived=excluded.is_archived,
                size_bytes=excluded.size_bytes,
                mtime_ns=excluded.mtime_ns,
                parsed_until_line=excluded.parsed_until_line,
                parsed_until_byte=excluded.parsed_until_byte,
                parser_adapter=excluded.parser_adapter,
                parser_state_json=excluded.parser_state_json,
                parser_diagnostics_json=excluded.parser_diagnostics_json,
                last_indexed_at=excluded.last_indexed_at",
            params![
                source.path.to_string_lossy().to_string(),
                plan.is_archived,
                metadata.size_bytes,
                metadata.mtime_ns,
                source.parsed_until_line,
                source.parsed_until_byte,
                PARSER_ADAPTER_VERSION,
                serde_json::to_string(&source.state).map_err(json_error)?,
                serde_json::to_string(&source.diagnostics).map_err(json_error)?,
                now
            ],
        )
        .map_err(db_error)?;
    }
    rebuild_usage_aggregates(&tx, &now)?;
    set_meta_tx(&tx, "refreshed_at", &now)?;
    set_meta_tx(&tx, "scanned_files", &scanned_files.to_string())?;
    set_meta_tx(&tx, "skipped_events", &skipped_events.to_string())?;
    let count: i64 = tx
        .query_row("SELECT COUNT(*) FROM usage_events", [], |row| row.get(0))
        .map_err(db_error)?;
    set_meta_tx(&tx, "parsed_events", &count.to_string())?;
    tx.commit().map_err(db_error)?;
    Ok((inserted, deleted))
}

fn rebuild_usage_aggregates(tx: &rusqlite::Transaction<'_>, now: &str) -> Result<()> {
    tx.execute_batch(
        "
        UPDATE usage_events
        SET thread_key = COALESCE(thread_name, session_id),
            thread_call_index = NULL,
            previous_record_id = NULL,
            next_record_id = NULL;
        ",
    )
    .map_err(db_error)?;

    let rows = {
        let mut stmt = tx
            .prepare(
                "SELECT record_id, COALESCE(thread_key, thread_name, session_id)
                 FROM usage_events
                 ORDER BY COALESCE(thread_key, thread_name, session_id), event_timestamp, record_id",
            )
            .map_err(db_error)?;
        let collected = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(db_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(db_error)?;
        collected
    };
    let mut previous_thread = String::new();
    let mut previous_record: Option<String> = None;
    let mut index = 0_i64;
    for (record_id, thread_key) in rows {
        if thread_key != previous_thread {
            previous_thread = thread_key;
            previous_record = None;
            index = 0;
        }
        tx.execute(
            "UPDATE usage_events
             SET thread_call_index = ?2, previous_record_id = ?3
             WHERE record_id = ?1",
            params![record_id, index, previous_record],
        )
        .map_err(db_error)?;
        if let Some(prev) = previous_record {
            tx.execute(
                "UPDATE usage_events SET next_record_id = ?2 WHERE record_id = ?1",
                params![prev, record_id],
            )
            .map_err(db_error)?;
        }
        previous_record = Some(record_id);
        index += 1;
    }

    tx.execute("DELETE FROM thread_summaries", [])
        .map_err(db_error)?;
    tx.execute(
        "
        INSERT INTO thread_summaries (
            thread_key, is_archived_scope, thread_label, first_event_timestamp,
            latest_event_timestamp, call_count, session_count, input_tokens,
            cached_input_tokens, uncached_input_tokens, output_tokens,
            reasoning_output_tokens, total_tokens, estimated_cost_usd, usage_credits,
            avg_cache_ratio, max_context_window_percent, max_recommendation_score,
            primary_recommendation, call_initiator_summary, archived_call_count, updated_at
        )
        SELECT
            COALESCE(thread_key, thread_name, session_id),
            MAX(is_archived),
            COALESCE(thread_name, session_id),
            MIN(event_timestamp),
            MAX(event_timestamp),
            COUNT(*),
            COUNT(DISTINCT session_id),
            COALESCE(SUM(input_tokens), 0),
            COALESCE(SUM(cached_input_tokens), 0),
            COALESCE(SUM(uncached_input_tokens), 0),
            COALESCE(SUM(output_tokens), 0),
            COALESCE(SUM(reasoning_output_tokens), 0),
            COALESCE(SUM(total_tokens), 0),
            COALESCE(SUM(estimated_cost_usd), 0),
            COALESCE(SUM(usage_credits), 0),
            CASE WHEN COALESCE(SUM(input_tokens), 0) > 0
                THEN CAST(COALESCE(SUM(cached_input_tokens), 0) AS REAL) / COALESCE(SUM(input_tokens), 0)
                ELSE 0 END,
            MAX(context_window_percent),
            MAX(CASE
                WHEN input_tokens >= 50000 AND cache_ratio < 0.2 THEN 90
                WHEN context_window_percent >= 0.8 THEN 75
                ELSE 0
            END),
            CASE
                WHEN MAX(context_window_percent) >= 0.8 THEN 'Inspect high context usage'
                WHEN COALESCE(SUM(input_tokens), 0) >= 50000
                    AND CAST(COALESCE(SUM(cached_input_tokens), 0) AS REAL) / COALESCE(SUM(input_tokens), 1) < 0.2
                    THEN 'Inspect low cache reuse'
                ELSE NULL
            END,
            MAX(call_initiator),
            SUM(CASE WHEN is_archived != 0 THEN 1 ELSE 0 END),
            ?1
        FROM usage_events
        GROUP BY COALESCE(thread_key, thread_name, session_id)",
        [now],
    )
    .map_err(db_error)?;

    tx.execute("DELETE FROM aggregate_diagnostic_facts", [])
        .map_err(db_error)?;
    tx.execute(
        "
        INSERT INTO aggregate_diagnostic_facts (
            record_id, fact_type, fact_name, fact_category, event_count, confidence,
            first_event_timestamp, last_event_timestamp, first_source_line,
            last_source_line, evidence_scope, raw_content_included
        )
        SELECT
            'unknown-model:' || COALESCE(model, 'unknown'),
            'pricing',
            COALESCE(model, 'unknown'),
            'unknown_model',
            COUNT(*),
            1.0,
            MIN(event_timestamp),
            MAX(event_timestamp),
            MIN(line_number),
            MAX(line_number),
            'aggregate',
            0
        FROM usage_events
        WHERE pricing_confidence = 'unknown'
        GROUP BY COALESCE(model, 'unknown')",
        [],
    )
    .map_err(db_error)?;
    Ok(())
}

struct SummaryFilter {
    from: Option<String>,
    to: Option<String>,
}

impl SummaryFilter {
    fn new(req: &UsageSummaryRequest) -> Self {
        let now = Local::now();
        let date = now.date_naive();
        match req.window.preset.as_str() {
            "today" => Self {
                from: Some(day_start(date)),
                to: Some(day_start(date + ChronoDuration::days(1))),
            },
            "this-week" => {
                let first =
                    date - ChronoDuration::days(date.weekday().num_days_from_monday() as i64);
                Self {
                    from: Some(day_start(first)),
                    to: Some(day_start(date + ChronoDuration::days(1))),
                }
            }
            "7d" | "last-7-days" => Self {
                from: Some(day_start(date - ChronoDuration::days(6))),
                to: Some(day_start(date + ChronoDuration::days(1))),
            },
            "30d" => Self {
                from: Some(day_start(date - ChronoDuration::days(29))),
                to: Some(day_start(date + ChronoDuration::days(1))),
            },
            "month" | "this-month" => {
                let first = NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap_or(date);
                let next = if date.month() == 12 {
                    NaiveDate::from_ymd_opt(date.year() + 1, 1, 1).unwrap_or(first)
                } else {
                    NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1).unwrap_or(first)
                };
                Self {
                    from: Some(day_start(first)),
                    to: Some(day_start(next)),
                }
            }
            "custom" => Self {
                from: req.window.from.as_deref().and_then(custom_start),
                to: req.window.to.as_deref().and_then(custom_end),
            },
            _ => Self {
                from: None,
                to: None,
            },
        }
    }
}

fn day_start(date: NaiveDate) -> String {
    Local
        .with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
        .single()
        .unwrap_or_else(Local::now)
        .with_timezone(&Utc)
        .to_rfc3339()
}

fn custom_start(value: &str) -> Option<String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .map(day_start)
}

fn custom_end(value: &str) -> Option<String> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .map(|date| day_start(date + ChronoDuration::days(1)))
}

fn query_recent_calls(
    conn: &Connection,
    req: &UsageSummaryRequest,
    filter: &SummaryFilter,
) -> Result<Vec<UsageCallRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT record_id, session_id, thread_name, session_updated_at, event_timestamp,
                source_file, line_number, turn_id, turn_timestamp, cwd, model, effort,
                current_date, timezone, call_initiator, call_initiator_reason,
                call_initiator_confidence, input_tokens, cached_input_tokens,
                uncached_input_tokens, output_tokens, reasoning_output_tokens, total_tokens,
                cumulative_total_tokens, cache_ratio, is_archived, thread_key,
                thread_call_index, previous_record_id, next_record_id, thread_source,
                subagent_type, agent_role, agent_nickname, parent_session_id,
                parent_thread_name, parent_session_updated_at, model_context_window,
                context_window_percent, rate_limit_plan_type, rate_limit_limit_id,
                rate_limit_primary_used_percent, rate_limit_primary_window_minutes,
                rate_limit_primary_resets_at, rate_limit_secondary_used_percent,
                rate_limit_secondary_window_minutes, rate_limit_secondary_resets_at,
                reasoning_output_ratio, estimated_cost_usd, usage_credits, pricing_model,
                pricing_estimated, pricing_confidence
             FROM usage_events
             WHERE (?1 OR is_archived = 0)
               AND (?2 IS NULL OR event_timestamp >= ?2)
               AND (?3 IS NULL OR event_timestamp < ?3)
             ORDER BY event_timestamp DESC, record_id ASC",
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map(
            params![
                req.include_archived,
                filter.from.as_deref(),
                filter.to.as_deref()
            ],
            read_call_row,
        )
        .map_err(db_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(db_error)?;
    Ok(rows)
}

fn query_top_threads(
    conn: &Connection,
    req: &UsageSummaryRequest,
    filter: &SummaryFilter,
) -> Result<Vec<UsageThreadSummary>> {
    let mut stmt = conn
        .prepare(
            "SELECT COALESCE(thread_key, thread_name, session_id) AS key,
                COALESCE(thread_name, session_id) AS label, MIN(event_timestamp), COUNT(*),
                COUNT(DISTINCT session_id), SUM(total_tokens),
                SUM(input_tokens), SUM(cached_input_tokens), SUM(uncached_input_tokens),
                SUM(output_tokens), SUM(reasoning_output_tokens), MAX(event_timestamp),
                MAX(is_archived), MAX(context_window_percent),
                SUM(CASE WHEN is_archived != 0 THEN 1 ELSE 0 END),
                MAX(call_initiator),
                COALESCE(SUM(estimated_cost_usd), 0.0),
                COALESCE(SUM(usage_credits), 0.0)
             FROM usage_events
             WHERE (?1 OR is_archived = 0)
               AND (?2 IS NULL OR event_timestamp >= ?2)
               AND (?3 IS NULL OR event_timestamp < ?3)
             GROUP BY key, label
             ORDER BY SUM(total_tokens) DESC
             LIMIT 50",
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map(
            params![
                req.include_archived,
                filter.from.as_deref(),
                filter.to.as_deref()
            ],
            |row| {
                let key = row.get::<_, String>(0)?;
                let label = row.get::<_, String>(1)?;
                let input_tokens = row.get::<_, i64>(6)?;
                let cached_input_tokens = row.get::<_, i64>(7)?;
                let cache_ratio = if input_tokens > 0 {
                    cached_input_tokens as f64 / input_tokens as f64
                } else {
                    0.0
                };
                let max_context_window_percent = row.get::<_, Option<f64>>(13)?;
                let primary_recommendation = if max_context_window_percent.unwrap_or(0.0) >= 0.8 {
                    Some("Inspect high context usage".to_string())
                } else if input_tokens >= 50_000 && cache_ratio < 0.2 {
                    Some("Inspect low cache reuse".to_string())
                } else {
                    None
                };
                Ok(UsageThreadSummary {
                    thread_key: key,
                    is_archived_scope: row.get::<_, i64>(12)? != 0,
                    thread_label: label.clone(),
                    first_event_timestamp: row.get(2)?,
                    call_count: row.get::<_, i64>(3)? as usize,
                    session_count: row.get::<_, i64>(4)? as usize,
                    total_tokens: row.get::<_, i64>(5)?,
                    input_tokens,
                    cached_input_tokens,
                    uncached_input_tokens: row.get::<_, i64>(8)?,
                    output_tokens: row.get(9)?,
                    reasoning_output_tokens: row.get(10)?,
                    latest_event_timestamp: row.get(11)?,
                    avg_cache_ratio: cache_ratio,
                    max_context_window_percent,
                    max_recommendation_score: if primary_recommendation.is_some() {
                        90.0
                    } else {
                        0.0
                    },
                    primary_recommendation,
                    call_initiator_summary: row.get(15)?,
                    archived_call_count: row.get::<_, i64>(14)? as usize,
                    updated_at: None,
                    estimated_cost_usd: row.get(16)?,
                    usage_credits: row.get(17)?,
                    cache_ratio,
                    is_archived: row.get::<_, i64>(12)? != 0,
                })
            },
        )
        .map_err(db_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(db_error)?;
    let mut rows = rows;
    for row in &mut rows {
        row.estimated_cost_usd = estimate_thread_cost(conn, req, filter, &row.thread_label)?;
    }
    Ok(rows)
}

fn read_call_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<UsageCallRow> {
    let mut item = UsageCallRow {
        record_id: row.get(0)?,
        session_id: row.get(1)?,
        thread_name: row.get(2)?,
        session_updated_at: row.get(3)?,
        event_timestamp: row.get(4)?,
        source_file: row.get(5)?,
        line_number: row.get(6)?,
        turn_id: row.get(7)?,
        turn_timestamp: row.get(8)?,
        cwd: row.get(9)?,
        model: row.get(10)?,
        effort: row.get(11)?,
        current_date: row.get(12)?,
        timezone: row.get(13)?,
        call_initiator: row.get(14)?,
        call_initiator_reason: row.get(15)?,
        call_initiator_confidence: row.get(16)?,
        input_tokens: row.get(17)?,
        cached_input_tokens: row.get(18)?,
        uncached_input_tokens: row.get(19)?,
        output_tokens: row.get(20)?,
        reasoning_output_tokens: row.get(21)?,
        total_tokens: row.get(22)?,
        cumulative_total_tokens: row.get(23)?,
        cache_ratio: row.get(24)?,
        is_archived: row.get::<_, i64>(25)? != 0,
        thread_key: row.get(26)?,
        thread_call_index: row.get(27)?,
        previous_record_id: row.get(28)?,
        next_record_id: row.get(29)?,
        thread_source: row.get(30)?,
        subagent_type: row.get(31)?,
        agent_role: row.get(32)?,
        agent_nickname: row.get(33)?,
        parent_session_id: row.get(34)?,
        parent_thread_name: row.get(35)?,
        parent_session_updated_at: row.get(36)?,
        model_context_window: row.get(37)?,
        context_window_percent: row.get(38)?,
        rate_limit_plan_type: row.get(39)?,
        rate_limit_limit_id: row.get(40)?,
        rate_limit_primary_used_percent: row.get(41)?,
        rate_limit_primary_window_minutes: row.get(42)?,
        rate_limit_primary_resets_at: row.get(43)?,
        rate_limit_secondary_used_percent: row.get(44)?,
        rate_limit_secondary_window_minutes: row.get(45)?,
        rate_limit_secondary_resets_at: row.get(46)?,
        reasoning_output_ratio: row.get(47)?,
        estimated_cost_usd: row.get(48)?,
        usage_credits: row.get(49)?,
        pricing_model: row.get(50)?,
        pricing_estimated: row.get::<_, i64>(51)? != 0,
        pricing_confidence: row.get(52)?,
    };
    if let Some(estimate) = estimate_cost(
        item.model.as_deref(),
        item.input_tokens,
        item.cached_input_tokens,
        item.output_tokens,
    ) {
        item.estimated_cost_usd = estimate.estimated_cost_usd;
        item.pricing_model = Some(estimate.pricing_model);
        item.pricing_estimated = estimate.pricing_estimated;
    }
    Ok(item)
}

struct ModelTokenTotals {
    model: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    total_tokens: i64,
    estimated_cost_usd: f64,
}

fn model_totals(
    conn: &Connection,
    req: &UsageSummaryRequest,
    filter: &SummaryFilter,
) -> Result<Vec<ModelTokenTotals>> {
    let mut stmt = conn
        .prepare(
            "SELECT model, COALESCE(SUM(input_tokens),0),
                COALESCE(SUM(output_tokens),0), COALESCE(SUM(total_tokens),0),
                COALESCE(SUM(estimated_cost_usd),0.0)
             FROM usage_events
             WHERE (?1 OR is_archived = 0)
               AND (?2 IS NULL OR event_timestamp >= ?2)
               AND (?3 IS NULL OR event_timestamp < ?3)
             GROUP BY model",
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map(
            params![
                req.include_archived,
                filter.from.as_deref(),
                filter.to.as_deref()
            ],
            |row| {
                Ok(ModelTokenTotals {
                    model: row.get(0)?,
                    input_tokens: row.get(1)?,
                    output_tokens: row.get(2)?,
                    total_tokens: row.get(3)?,
                    estimated_cost_usd: row.get(4)?,
                })
            },
        )
        .map_err(db_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(db_error)?;
    Ok(rows)
}

fn estimate_summary_cost(totals: &[ModelTokenTotals]) -> (f64, UsagePricingCoverage) {
    let mut cost = 0.0;
    let mut priced_tokens = 0;
    let mut unpriced_tokens = 0;
    let mut unknown_models = Vec::new();
    for item in totals {
        if item
            .model
            .as_deref()
            .and_then(|m| rate_for_model(m, 0))
            .is_some()
        {
            cost += item.estimated_cost_usd;
            priced_tokens += item.input_tokens + item.output_tokens;
        } else {
            unpriced_tokens += item.total_tokens;
            unknown_models.push(item.model.clone().unwrap_or_else(|| "unknown".to_string()));
        }
    }
    unknown_models.sort();
    unknown_models.dedup();
    let total = priced_tokens + unpriced_tokens;
    (
        cost,
        UsagePricingCoverage {
            priced_tokens,
            unpriced_tokens,
            priced_token_ratio: if total > 0 {
                priced_tokens as f64 / total as f64
            } else {
                0.0
            },
            unknown_models,
        },
    )
}

fn estimate_cost(
    model: Option<&str>,
    input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
) -> Option<UsageCostEstimate> {
    let rate = rate_for_model(model?, input_tokens)?;
    let uncached = (input_tokens - cached_input_tokens).max(0);
    let cost = (uncached as f64 * rate.input_per_million
        + cached_input_tokens as f64 * rate.cached_input_per_million
        + output_tokens as f64 * rate.output_per_million)
        / 1_000_000.0;
    Some(UsageCostEstimate {
        estimated_cost_usd: cost,
        pricing_model: rate.pricing_model.to_string(),
        pricing_estimated: rate.estimated,
    })
}

fn rate_for_model(model: &str, input_tokens: i64) -> Option<UsageRate> {
    let normalized = model.to_ascii_lowercase();
    let estimated = normalized == "codex-auto-review";
    let model = if estimated {
        "gpt-5.3-codex"
    } else {
        normalized.as_str()
    };

    // Short context is <= 128k (128,000) tokens, Long context is > 128k
    let is_long_context = input_tokens > 128_000;

    match model {
        "gpt-5.5" => Some(UsageRate {
            pricing_model: "gpt-5.5",
            estimated,
            input_per_million: if is_long_context { 10.0 } else { 5.0 },
            cached_input_per_million: if is_long_context { 1.0 } else { 0.5 },
            output_per_million: if is_long_context { 45.0 } else { 30.0 },
        }),
        "gpt-5.5-pro" => Some(UsageRate {
            pricing_model: "gpt-5.5-pro",
            estimated,
            input_per_million: if is_long_context { 60.0 } else { 30.0 },
            cached_input_per_million: if is_long_context { 60.0 } else { 30.0 },
            output_per_million: if is_long_context { 270.0 } else { 180.0 },
        }),
        "gpt-5.4" => Some(UsageRate {
            pricing_model: "gpt-5.4",
            estimated,
            input_per_million: if is_long_context { 5.0 } else { 2.5 },
            cached_input_per_million: if is_long_context { 0.5 } else { 0.25 },
            output_per_million: if is_long_context { 22.5 } else { 15.0 },
        }),
        "gpt-5.4-mini" => Some(UsageRate {
            pricing_model: "gpt-5.4-mini",
            estimated,
            input_per_million: 0.75,
            cached_input_per_million: 0.075,
            output_per_million: 4.50,
        }),
        "gpt-5.4-nano" => Some(UsageRate {
            pricing_model: "gpt-5.4-nano",
            estimated,
            input_per_million: 0.20,
            cached_input_per_million: 0.02,
            output_per_million: 1.25,
        }),
        "gpt-5.4-pro" => Some(UsageRate {
            pricing_model: "gpt-5.4-pro",
            estimated,
            input_per_million: if is_long_context { 60.0 } else { 30.0 },
            cached_input_per_million: if is_long_context { 60.0 } else { 30.0 },
            output_per_million: if is_long_context { 270.0 } else { 180.0 },
        }),
        "gpt-5.3-codex" => Some(UsageRate {
            pricing_model: "gpt-5.3-codex",
            estimated,
            input_per_million: 4.375,
            cached_input_per_million: 0.4375,
            output_per_million: 35.0,
        }),
        "gpt-5.2" => Some(UsageRate {
            pricing_model: "gpt-5.2",
            estimated,
            input_per_million: 4.375,
            cached_input_per_million: 0.4375,
            output_per_million: 35.0,
        }),
        "gpt-5" => Some(UsageRate {
            pricing_model: "gpt-5",
            estimated,
            input_per_million: 4.375,
            cached_input_per_million: 0.4375,
            output_per_million: 35.0,
        }),
        _ => None,
    }
}

fn estimate_thread_cost(
    conn: &Connection,
    req: &UsageSummaryRequest,
    filter: &SummaryFilter,
    label: &str,
) -> Result<f64> {
    let mut stmt = conn
        .prepare(
            "SELECT model, COALESCE(SUM(input_tokens),0),
                COALESCE(SUM(output_tokens),0), COALESCE(SUM(total_tokens),0),
                COALESCE(SUM(estimated_cost_usd),0.0)
             FROM usage_events
             WHERE (?1 OR is_archived = 0)
               AND (?2 IS NULL OR event_timestamp >= ?2)
               AND (?3 IS NULL OR event_timestamp < ?3)
               AND COALESCE(thread_name, session_id) = ?4
             GROUP BY model",
        )
        .map_err(db_error)?;
    let totals = stmt
        .query_map(
            params![
                req.include_archived,
                filter.from.as_deref(),
                filter.to.as_deref(),
                label,
            ],
            |row| {
                Ok(ModelTokenTotals {
                    model: row.get(0)?,
                    input_tokens: row.get(1)?,
                    output_tokens: row.get(2)?,
                    total_tokens: row.get(3)?,
                    estimated_cost_usd: row.get(4)?,
                })
            },
        )
        .map_err(db_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(db_error)?;
    Ok(estimate_summary_cost(&totals).0)
}

fn usage_diagnostics(
    conn: &Connection,
    req: &UsageSummaryRequest,
    filter: &SummaryFilter,
    top_threads: &[UsageThreadSummary],
    recent_calls: &[UsageCallRow],
) -> Result<UsageDiagnostics> {
    let mut parser_diagnostics = BTreeMap::new();
    let mut stmt = conn
        .prepare("SELECT parser_diagnostics_json FROM source_files WHERE (?1 OR is_archived = 0)")
        .map_err(db_error)?;
    for json in stmt
        .query_map([req.include_archived], |row| row.get::<_, String>(0))
        .map_err(db_error)?
    {
        let json = json.map_err(db_error)?;
        if let Ok(map) = serde_json::from_str::<BTreeMap<String, i64>>(&json) {
            merge_diagnostics(&mut parser_diagnostics, &map);
        }
    }
    let (_, coverage) = estimate_summary_cost(&model_totals(conn, req, filter)?);
    Ok(UsageDiagnostics {
        skipped_events: parser_diagnostics
            .get("skipped_events")
            .copied()
            .unwrap_or(0) as usize,
        parser_diagnostics,
        unknown_models: coverage.unknown_models,
        low_cache_threads: top_threads
            .iter()
            .filter(|thread| thread.cache_ratio < 0.2 && thread.input_tokens >= 50_000)
            .take(10)
            .cloned()
            .collect(),
        high_context_calls: recent_calls
            .iter()
            .filter(|call| call.context_window_percent.unwrap_or(0.0) >= 0.8)
            .take(10)
            .cloned()
            .collect(),
        last_refresh_error: get_meta(conn, "last_refresh_error")?,
    })
}

pub fn reset_usage_index(home_root: &Path) -> Result<()> {
    let _guard = REFRESH_LOCK
        .lock()
        .map_err(|_| AppError::new("USAGE_REFRESH_LOCK", "usage refresh lock is poisoned"))?;
    let usage_dir = home_root.join(".codex/lam/usage");
    if usage_dir.exists() {
        fs::remove_dir_all(usage_dir)?;
    }
    Ok(())
}

pub fn compact_usage_db(home_root: &Path) -> Result<()> {
    let _guard = REFRESH_LOCK
        .lock()
        .map_err(|_| AppError::new("USAGE_REFRESH_LOCK", "usage refresh lock is poisoned"))?;
    let db_path = usage_db_path(home_root);
    if !db_path.exists() {
        return Ok(());
    }
    let mut conn = open_usage_db(&db_path)?;
    init_usage_db(&conn)?;
    compact_usage_db_after_refresh(&mut conn, true)
}

fn compact_usage_db_after_refresh(conn: &mut Connection, vacuum: bool) -> Result<()> {
    if vacuum {
        conn.execute_batch("VACUUM").map_err(db_error)?;
    } else {
        conn.execute_batch("PRAGMA optimize").map_err(db_error)?;
    }
    Ok(())
}

fn load_session_index(codex_home: &Path) -> HashMap<String, String> {
    let path = codex_home.join("session_index.jsonl");
    let Ok(file) = File::open(path) else {
        return HashMap::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(std::result::Result::ok)
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .filter_map(|value| {
            Some((
                value.get("id")?.as_str()?.to_string(),
                value.get("thread_name")?.as_str()?.to_string(),
            ))
        })
        .collect()
}

fn find_session_logs(codex_home: &Path, include_archived: bool) -> Result<Vec<SourceLog>> {
    let mut paths = Vec::new();
    collect_jsonl(&codex_home.join("sessions"), false, &mut paths)?;
    if include_archived {
        collect_jsonl(&codex_home.join("archived_sessions"), true, &mut paths)?;
    }
    paths.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(paths)
}

fn collect_jsonl(dir: &Path, is_archived: bool, paths: &mut Vec<SourceLog>) -> Result<()> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, is_archived, paths)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            paths.push(SourceLog { path, is_archived });
        }
    }
    Ok(())
}

struct SourceMetadata {
    size_bytes: i64,
    mtime_ns: i64,
}

fn source_metadata(path: &Path) -> Result<SourceMetadata> {
    let metadata = fs::metadata(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(SourceMetadata {
            size_bytes: metadata.len() as i64,
            mtime_ns: metadata.mtime() * 1_000_000_000 + metadata.mtime_nsec(),
        })
    }
    #[cfg(not(unix))]
    {
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos() as i64)
            .unwrap_or(0);
        Ok(SourceMetadata {
            size_bytes: metadata.len() as i64,
            mtime_ns: modified,
        })
    }
}

fn usage_int(map: &serde_json::Map<String, Value>, key: &str) -> Option<i64> {
    map.get(key).and_then(Value::as_i64)
}

fn nullable_usage_int(
    value: Option<&Value>,
    diagnostics: &mut BTreeMap<String, i64>,
    invalid_key: &str,
) -> Option<i64> {
    match value {
        None | Some(Value::Null) => None,
        Some(value) => value.as_i64().or_else(|| {
            increment(diagnostics, invalid_key);
            if invalid_key != "partial_field_count" {
                increment(diagnostics, "partial_field_count");
            }
            None
        }),
    }
}

fn rate_limit_text(value: Option<&Value>, keys: &[&str]) -> Option<String> {
    let obj = value?.as_object()?;
    keys.iter()
        .find_map(|key| obj.get(*key).and_then(Value::as_str).map(str::to_string))
}

fn rate_limit_int(value: Option<&Value>, keys: &[&str]) -> Option<i64> {
    let obj = value?.as_object()?;
    keys.iter()
        .find_map(|key| obj.get(*key).and_then(Value::as_i64))
}

fn rate_limit_number(value: Option<&Value>, keys: &[&str]) -> Option<f64> {
    let obj = value?.as_object()?;
    keys.iter()
        .find_map(|key| obj.get(*key).and_then(Value::as_f64))
}

fn session_id_from_path(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?.strip_suffix(".jsonl")?;
    let candidate = name.get(name.len().checked_sub(36)?..)?;
    let valid = candidate.chars().enumerate().all(|(index, ch)| {
        if matches!(index, 8 | 13 | 18 | 23) {
            ch == '-'
        } else {
            ch.is_ascii_hexdigit()
        }
    });
    valid.then(|| candidate.to_string())
}

fn increment(stats: &mut BTreeMap<String, i64>, key: &str) {
    *stats.entry(key.to_string()).or_insert(0) += 1;
}

fn merge_diagnostics(target: &mut BTreeMap<String, i64>, source: &BTreeMap<String, i64>) {
    for (key, value) in source {
        *target.entry(key.clone()).or_insert(0) += value;
    }
}

fn get_meta(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM refresh_meta WHERE key = ?",
        [key],
        |row| row.get(0),
    )
    .optional()
    .map_err(db_error)
}

fn set_meta_tx(tx: &rusqlite::Transaction<'_>, key: &str, value: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO refresh_meta (key, value) VALUES (?1, ?2)
        ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [key, value],
    )
    .map_err(db_error)?;
    Ok(())
}

fn db_error(err: rusqlite::Error) -> AppError {
    AppError::new("USAGE_DB_ERROR", err.to_string())
}

fn json_error(err: serde_json::Error) -> AppError {
    AppError::new("USAGE_JSON_ERROR", err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn write_log(home: &Path, body: &str) -> PathBuf {
        let path = home.join(".codex/sessions/2026/06/28/rollout-test-2026-06-28T00-00-00-00000000-0000-0000-0000-000000000001.jsonl");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, body).unwrap();
        path
    }

    fn write_log_at(home: &Path, root: &str, name: &str, body: &str) -> PathBuf {
        let path = home.join(format!(".codex/{root}/2026/06/28/{name}.jsonl"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, body).unwrap();
        path
    }

    fn fixture(total: i64, last: i64) -> String {
        fixture_at(
            "00000000-0000-0000-0000-000000000001",
            "2026-06-28T00:00:02Z",
            total,
            last,
            "gpt-5",
        )
    }

    fn fixture_at(session_id: &str, timestamp: &str, total: i64, last: i64, model: &str) -> String {
        format!(
            "{}\n{}\n{}\n",
            json!({"type":"session_meta","timestamp":"2026-06-28T00:00:00Z","payload":{"id":session_id}}),
            json!({"type":"turn_context","timestamp":"2026-06-28T00:00:01Z","payload":{"turn_id":"turn-1","cwd":"/repo/LAM","model":model,"effort":"medium","current_date":"2026-06-28","timezone":"Asia/Shanghai"}}),
            json!({"type":"event_msg","timestamp":timestamp,"payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":last,"cached_input_tokens":last / 2,"output_tokens":10,"reasoning_output_tokens":3,"total_tokens":last + 10},"total_token_usage":{"input_tokens":total,"cached_input_tokens":total / 2,"output_tokens":10,"reasoning_output_tokens":3,"total_tokens":total + 10}}}})
        )
    }

    fn summary_request(preset: &str) -> UsageSummaryRequest {
        UsageSummaryRequest {
            window: UsageWindow {
                preset: preset.to_string(),
                from: None,
                to: None,
            },
            include_archived: false,
        }
    }

    fn local_noon_timestamp(date: NaiveDate) -> String {
        Local
            .with_ymd_and_hms(date.year(), date.month(), date.day(), 12, 0, 0)
            .single()
            .unwrap()
            .with_timezone(&Utc)
            .to_rfc3339()
    }

    fn table_columns(conn: &Connection, table: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        stmt.query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap()
    }

    #[test]
    fn old_usage_db_migrates_to_parity_schema() {
        let temp = TempDir::new().unwrap();
        let db_path = usage_db_path(temp.path());
        prepare_usage_dir(&db_path).unwrap();
        let conn = open_usage_db(&db_path).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE usage_events (
                record_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                thread_name TEXT,
                event_timestamp TEXT NOT NULL,
                source_file TEXT NOT NULL,
                line_number INTEGER NOT NULL,
                turn_id TEXT,
                cwd TEXT,
                model TEXT,
                effort TEXT,
                input_tokens INTEGER NOT NULL,
                cached_input_tokens INTEGER NOT NULL,
                uncached_input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                reasoning_output_tokens INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                cumulative_input_tokens INTEGER NOT NULL,
                cumulative_cached_input_tokens INTEGER NOT NULL,
                cumulative_output_tokens INTEGER NOT NULL,
                cumulative_reasoning_output_tokens INTEGER NOT NULL,
                cumulative_total_tokens INTEGER NOT NULL,
                cache_ratio REAL NOT NULL
            );
            CREATE TABLE source_files (
                source_file TEXT PRIMARY KEY,
                is_archived INTEGER NOT NULL DEFAULT 0,
                size_bytes INTEGER NOT NULL,
                mtime_ns INTEGER NOT NULL,
                parsed_until_line INTEGER NOT NULL,
                parsed_until_byte INTEGER NOT NULL,
                parser_adapter TEXT NOT NULL,
                parser_state_json TEXT NOT NULL,
                parser_diagnostics_json TEXT NOT NULL,
                last_indexed_at TEXT NOT NULL
            );
            ",
        )
        .unwrap();
        init_usage_db(&conn).unwrap();
        let columns = table_columns(&conn, "usage_events");
        for column in [
            "current_date",
            "timezone",
            "call_initiator",
            "thread_key",
            "previous_record_id",
            "model_context_window",
            "rate_limit_plan_type",
            "reasoning_output_ratio",
            "pricing_confidence",
        ] {
            assert!(columns.contains(&column.to_string()), "{column}");
        }
        assert!(table_columns(&conn, "thread_summaries").contains(&"usage_credits".to_string()));
        assert!(table_columns(&conn, "aggregate_diagnostic_facts")
            .contains(&"raw_content_included".to_string()));
    }

    #[test]
    fn usage_events_contains_required_parity_columns() {
        let temp = TempDir::new().unwrap();
        let db_path = usage_db_path(temp.path());
        prepare_usage_dir(&db_path).unwrap();
        let conn = open_usage_db(&db_path).unwrap();
        init_usage_db(&conn).unwrap();
        let columns = table_columns(&conn, "usage_events");
        for column in [
            "record_id",
            "session_id",
            "thread_name",
            "session_updated_at",
            "event_timestamp",
            "source_file",
            "line_number",
            "turn_id",
            "turn_timestamp",
            "cwd",
            "model",
            "effort",
            "current_date",
            "timezone",
            "call_initiator",
            "call_initiator_reason",
            "call_initiator_confidence",
            "is_archived",
            "thread_key",
            "thread_call_index",
            "previous_record_id",
            "next_record_id",
            "thread_source",
            "subagent_type",
            "agent_role",
            "agent_nickname",
            "parent_session_id",
            "parent_thread_name",
            "parent_session_updated_at",
            "model_context_window",
            "input_tokens",
            "cached_input_tokens",
            "output_tokens",
            "reasoning_output_tokens",
            "total_tokens",
            "cumulative_input_tokens",
            "cumulative_cached_input_tokens",
            "cumulative_output_tokens",
            "cumulative_reasoning_output_tokens",
            "cumulative_total_tokens",
            "rate_limit_plan_type",
            "rate_limit_limit_id",
            "rate_limit_primary_used_percent",
            "rate_limit_primary_window_minutes",
            "rate_limit_primary_resets_at",
            "rate_limit_secondary_used_percent",
            "rate_limit_secondary_window_minutes",
            "rate_limit_secondary_resets_at",
            "uncached_input_tokens",
            "cache_ratio",
            "reasoning_output_ratio",
            "context_window_percent",
        ] {
            assert!(columns.contains(&column.to_string()), "{column}");
        }
    }

    #[test]
    fn no_raw_prompt_like_columns_exist() {
        let temp = TempDir::new().unwrap();
        let db_path = usage_db_path(temp.path());
        prepare_usage_dir(&db_path).unwrap();
        let conn = open_usage_db(&db_path).unwrap();
        init_usage_db(&conn).unwrap();
        let columns = table_columns(&conn, "usage_events").join(",");
        assert!(!columns.contains("prompt"));
        assert!(!columns.contains("content"));
        assert!(!columns.contains("transcript"));
    }

    #[test]
    fn parses_basic_token_count_fixture() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".codex")).unwrap();
        fs::write(
            temp.path().join(".codex/session_index.jsonl"),
            format!(
                "{}\n",
                json!({"id":"00000000-0000-0000-0000-000000000001","thread_name":"usage thread"})
            ),
        )
        .unwrap();
        write_log(temp.path(), &fixture(100, 50));
        refresh_usage_index(temp.path()).unwrap();
        let summary = get_usage_summary(temp.path(), summary_request("all")).unwrap();
        assert_eq!(summary.total_calls, 1);
        assert_eq!(summary.recent_calls[0].model.as_deref(), Some("gpt-5"));
        assert_eq!(summary.recent_calls[0].cwd.as_deref(), Some("/repo/LAM"));
        assert_eq!(
            summary.recent_calls[0].thread_name.as_deref(),
            Some("usage thread")
        );
        assert_eq!(summary.recent_calls[0].input_tokens, 50);
    }

    #[test]
    fn refresh_is_idempotent_for_unchanged_source() {
        let temp = TempDir::new().unwrap();
        write_log(temp.path(), &fixture(100, 50));
        assert_eq!(refresh_usage_index(temp.path()).unwrap().parsed_files, 1);
        assert_eq!(refresh_usage_index(temp.path()).unwrap().parsed_files, 0);
        assert_eq!(
            get_usage_summary(temp.path(), summary_request("all"))
                .unwrap()
                .total_calls,
            1
        );
    }

    #[test]
    fn append_refresh_uses_cursor_and_state() {
        let temp = TempDir::new().unwrap();
        let path = write_log(
            temp.path(),
            &format!(
                "{}\n{}\n",
                json!({"type":"session_meta","timestamp":"2026-06-28T00:00:00Z","payload":{"id":"00000000-0000-0000-0000-000000000001"}}),
                json!({"type":"turn_context","timestamp":"2026-06-28T00:00:01Z","payload":{"turn_id":"turn-1","cwd":"/repo/LAM","model":"gpt-5","effort":"medium"}})
            ),
        );
        refresh_usage_index(temp.path()).unwrap();
        let mut body = fs::read_to_string(&path).unwrap();
        body.push_str(&format!("{}\n", json!({"type":"event_msg","timestamp":"2026-06-28T00:00:02Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":20,"cached_input_tokens":5,"output_tokens":4,"reasoning_output_tokens":1,"total_tokens":24},"total_token_usage":{"input_tokens":20,"cached_input_tokens":5,"output_tokens":4,"reasoning_output_tokens":1,"total_tokens":24}}}})));
        fs::write(path, body).unwrap();
        refresh_usage_index(temp.path()).unwrap();
        let summary = get_usage_summary(temp.path(), summary_request("all")).unwrap();
        assert_eq!(summary.total_calls, 1);
        assert_eq!(summary.recent_calls[0].model.as_deref(), Some("gpt-5"));
    }

    #[test]
    fn partial_trailing_line_is_not_committed() {
        let temp = TempDir::new().unwrap();
        let partial = json!({"type":"event_msg","timestamp":"2026-06-28T00:00:02Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":20,"cached_input_tokens":5,"output_tokens":4,"reasoning_output_tokens":1,"total_tokens":24},"total_token_usage":{"input_tokens":20,"cached_input_tokens":5,"output_tokens":4,"reasoning_output_tokens":1,"total_tokens":24}}}}).to_string();
        let path = write_log(temp.path(), &partial);
        let result = refresh_usage_index(temp.path()).unwrap();
        assert_eq!(
            get_usage_summary(temp.path(), summary_request("all"))
                .unwrap()
                .total_calls,
            0
        );
        assert_eq!(
            result.parser_diagnostics.get("partial_trailing_line"),
            Some(&1)
        );
        fs::write(&path, format!("{partial}\n")).unwrap();
        refresh_usage_index(temp.path()).unwrap();
        assert_eq!(
            get_usage_summary(temp.path(), summary_request("all"))
                .unwrap()
                .total_calls,
            1
        );
    }

    #[test]
    fn rewrite_replaces_source_rows() {
        let temp = TempDir::new().unwrap();
        let path = write_log(temp.path(), &fixture(100, 50));
        refresh_usage_index(temp.path()).unwrap();
        fs::write(
            path,
            format!("{}\n", json!({"type":"event_msg","timestamp":"2026-06-28T00:00:02Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":8,"cached_input_tokens":3,"output_tokens":2,"reasoning_output_tokens":1,"total_tokens":10},"total_token_usage":{"input_tokens":8,"cached_input_tokens":3,"output_tokens":2,"reasoning_output_tokens":1,"total_tokens":10}}}})),
        )
        .unwrap();
        refresh_usage_index(temp.path()).unwrap();
        let summary = get_usage_summary(temp.path(), summary_request("all")).unwrap();
        assert_eq!(summary.total_calls, 1);
        assert_eq!(summary.recent_calls[0].input_tokens, 8);
    }

    #[test]
    fn normal_db_does_not_store_raw_content() {
        let temp = TempDir::new().unwrap();
        let raw = "fake prompt secret tool output";
        write_log(
            temp.path(),
            &format!(
                "{}\n{}",
                json!({"type":"event_msg","timestamp":"2026-06-28T00:00:00Z","payload":{"type":"user_message","message":raw}}),
                fixture(100, 50)
            ),
        );
        refresh_usage_index(temp.path()).unwrap();
        let bytes = fs::read(usage_db_path(temp.path())).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains(raw));
    }

    #[test]
    fn usage_db_lives_under_lam_codex_subdir() {
        let temp = TempDir::new().unwrap();
        write_log(temp.path(), &fixture(100, 50));
        refresh_usage_index(temp.path()).unwrap();
        assert_eq!(
            usage_db_path(temp.path()),
            temp.path().join(".codex/lam/usage/usage.sqlite3")
        );
        assert!(usage_db_path(temp.path()).exists());
        let root_entries = fs::read_dir(temp.path().join(".codex")).unwrap();
        for entry in root_entries {
            let path = entry.unwrap().path();
            assert_ne!(
                path.file_name().and_then(|v| v.to_str()),
                Some("usage.sqlite3")
            );
        }
    }

    #[test]
    fn discovery_ignores_lam_usage_directory() {
        let temp = TempDir::new().unwrap();
        let lam_path = temp.path().join(".codex/lam/usage/fake.jsonl");
        fs::create_dir_all(lam_path.parent().unwrap()).unwrap();
        fs::write(lam_path, fixture(100, 50)).unwrap();
        let result = refresh_usage_index(temp.path()).unwrap();
        assert_eq!(result.scanned_files, 0);
        assert_eq!(
            get_usage_summary(temp.path(), summary_request("all"))
                .unwrap()
                .total_calls,
            0
        );
    }

    #[test]
    fn all_history_includes_archived_incrementally() {
        let temp = TempDir::new().unwrap();
        write_log_at(
            temp.path(),
            "sessions",
            "active-00000000-0000-0000-0000-000000000001",
            &fixture_at(
                "00000000-0000-0000-0000-000000000001",
                "2026-06-28T00:00:02Z",
                100,
                50,
                "gpt-5",
            ),
        );
        write_log_at(
            temp.path(),
            "archived_sessions",
            "archived-00000000-0000-0000-0000-000000000002",
            &fixture_at(
                "00000000-0000-0000-0000-000000000002",
                "2026-06-28T00:00:03Z",
                200,
                60,
                "gpt-5.3-codex",
            ),
        );

        refresh_usage_index(temp.path()).unwrap();
        assert_eq!(
            get_usage_summary(temp.path(), summary_request("all"))
                .unwrap()
                .total_calls,
            1
        );

        let result = refresh_usage_index_with_options(temp.path(), true).unwrap();
        let mut req = summary_request("all");
        req.include_archived = true;
        let summary = get_usage_summary(temp.path(), req).unwrap();
        assert_eq!(summary.total_calls, 2);
        assert!(summary.recent_calls.iter().any(|row| row.is_archived));
        assert!(summary.top_threads.iter().any(|row| row.is_archived));
        let archived = summary
            .recent_calls
            .iter()
            .find(|row| row.model.as_deref() == Some("gpt-5.3-codex"))
            .unwrap();
        assert_eq!(archived.pricing_model.as_deref(), Some("gpt-5.3-codex"));
        assert!(!archived.pricing_estimated);
        assert_eq!(result.parsed_files, 1);
        assert_eq!(
            refresh_usage_index_with_options(temp.path(), true)
                .unwrap()
                .parsed_files,
            0
        );
    }

    #[test]
    fn summary_window_filters_rows() {
        let temp = TempDir::new().unwrap();
        let today = Local::now().date_naive();
        let week = today - ChronoDuration::days(3);
        let month_or_30d = today - ChronoDuration::days(20);
        let old = today - ChronoDuration::days(70);
        let rows = [
            ("today", today, 100, 10),
            ("week", week, 200, 20),
            ("month", month_or_30d, 300, 30),
            ("old", old, 400, 40),
        ];
        for (index, (name, date, total, last)) in rows.iter().enumerate() {
            let session_id = format!("00000000-0000-0000-0000-{:012}", index + 1);
            write_log_at(
                temp.path(),
                "sessions",
                &format!("{name}-{session_id}"),
                &fixture_at(
                    &session_id,
                    &local_noon_timestamp(*date),
                    *total,
                    *last,
                    "gpt-5",
                ),
            );
        }
        refresh_usage_index(temp.path()).unwrap();

        assert_eq!(
            get_usage_summary(temp.path(), summary_request("today"))
                .unwrap()
                .total_calls,
            1
        );
        assert_eq!(
            get_usage_summary(temp.path(), summary_request("7d"))
                .unwrap()
                .total_calls,
            2
        );
        assert_eq!(
            get_usage_summary(temp.path(), summary_request("30d"))
                .unwrap()
                .total_calls,
            3
        );
        assert_eq!(
            get_usage_summary(temp.path(), summary_request("month"))
                .unwrap()
                .total_calls,
            rows.iter()
                .filter(
                    |(_, date, _, _)| date.year() == today.year() && date.month() == today.month()
                )
                .count()
        );
        assert_eq!(
            get_usage_summary(
                temp.path(),
                UsageSummaryRequest {
                    window: UsageWindow {
                        preset: "custom".to_string(),
                        from: Some(week.format("%Y-%m-%d").to_string()),
                        to: Some(today.format("%Y-%m-%d").to_string()),
                    },
                    include_archived: false,
                },
            )
            .unwrap()
            .total_calls,
            2
        );
    }

    #[test]
    fn reset_usage_index_removes_only_lam_usage_state() {
        let temp = TempDir::new().unwrap();
        write_log(temp.path(), &fixture(100, 50));
        refresh_usage_index(temp.path()).unwrap();
        for dir in ["sessions", "logs", "cache"] {
            let path = temp.path().join(format!(".codex/{dir}/keep.txt"));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "keep").unwrap();
        }

        reset_usage_index(temp.path()).unwrap();

        assert!(!temp.path().join(".codex/lam/usage").exists());
        for dir in ["sessions", "logs", "cache"] {
            assert!(temp.path().join(format!(".codex/{dir}/keep.txt")).exists());
        }
    }

    #[test]
    fn compact_usage_db_preserves_rows() {
        let temp = TempDir::new().unwrap();
        write_log(temp.path(), &fixture(100, 50));
        refresh_usage_index(temp.path()).unwrap();
        compact_usage_db(temp.path()).unwrap();
        let conn = open_usage_db(&usage_db_path(temp.path())).unwrap();
        let ok: String = conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .unwrap();
        assert_eq!(ok, "ok");
        assert_eq!(
            get_usage_summary(temp.path(), summary_request("all"))
                .unwrap()
                .total_calls,
            1
        );
    }

    #[test]
    fn codex_auto_review_pricing_is_marked_estimated() {
        let temp = TempDir::new().unwrap();
        write_log(
            temp.path(),
            &fixture_at(
                "00000000-0000-0000-0000-000000000001",
                "2026-06-28T00:00:02Z",
                100,
                50,
                "codex-auto-review",
            ),
        );
        refresh_usage_index(temp.path()).unwrap();
        let summary = get_usage_summary(temp.path(), summary_request("all")).unwrap();
        assert_eq!(
            summary.recent_calls[0].pricing_model.as_deref(),
            Some("gpt-5.3-codex")
        );
        assert!(summary.recent_calls[0].pricing_estimated);
    }

    #[test]
    fn model_context_window_drives_high_context_diagnostics() {
        let temp = TempDir::new().unwrap();
        write_log(
            temp.path(),
            &format!(
                "{}\n{}\n",
                json!({"type":"turn_context","timestamp":"2026-06-28T00:00:01Z","payload":{"turn_id":"turn-1","cwd":"/repo/LAM","model":"gpt-5","effort":"medium"}}),
                json!({"type":"event_msg","timestamp":"2026-06-28T00:00:02Z","payload":{"type":"token_count","info":{"model_context_window":100,"last_token_usage":{"input_tokens":90,"cached_input_tokens":0,"output_tokens":4,"reasoning_output_tokens":1,"total_tokens":94},"total_token_usage":{"input_tokens":90,"cached_input_tokens":0,"output_tokens":4,"reasoning_output_tokens":1,"total_tokens":94}}}})
            ),
        );
        refresh_usage_index(temp.path()).unwrap();
        let summary = get_usage_summary(temp.path(), summary_request("all")).unwrap();
        assert_eq!(summary.recent_calls[0].context_window_percent, Some(0.9));
        assert_eq!(summary.diagnostics.high_context_calls.len(), 1);
    }

    #[test]
    fn refresh_guard_serializes_overlapping_calls() {
        let temp = TempDir::new().unwrap();
        write_log(temp.path(), &fixture(100, 50));
        let home_a = temp.path().to_path_buf();
        let home_b = temp.path().to_path_buf();
        let a = std::thread::spawn(move || refresh_usage_index(&home_a).unwrap());
        let b = std::thread::spawn(move || refresh_usage_index(&home_b).unwrap());
        a.join().unwrap();
        b.join().unwrap();
        assert_eq!(
            get_usage_summary(temp.path(), summary_request("all"))
                .unwrap()
                .total_calls,
            1
        );
    }

    #[test]
    #[ignore]
    fn real_home_usage_smoke() {
        let home = PathBuf::from(std::env::var("HOME").expect("HOME is required"));
        let result = refresh_usage_index(&home).unwrap();
        let summary = get_usage_summary(&home, summary_request("all")).unwrap();
        assert_eq!(
            usage_db_path(&home),
            home.join(".codex/lam/usage/usage.sqlite3")
        );
        assert!(usage_db_path(&home).exists());
        assert!(summary.total_calls >= result.inserted_or_updated_events);
        assert!(!home.join(".codex/usage.sqlite3").exists());
        assert!(!home.join(".codex/sessions/usage.sqlite3").exists());
        assert!(!home.join(".codex/logs/usage.sqlite3").exists());
        assert!(!home.join(".codex/cache/usage.sqlite3").exists());
    }
}
