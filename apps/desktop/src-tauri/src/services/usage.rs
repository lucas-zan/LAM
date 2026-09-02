use super::account::{list_accounts, CodexAccount};
use super::quota::spawn_codex_app_server;
use crate::{AppError, Result};
use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDate, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::Mutex;

const PARSER_ADAPTER_VERSION: &str = "lam-codex-jsonl-v1";
const CODEX_APP_SERVER_USAGE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
static REFRESH_LOCK: Mutex<()> = Mutex::new(());
const COLLECT_AFFECTED_USAGE_RECORDS_SQL: &str = "
    INSERT OR IGNORE INTO affected_usage_records (record_id)
    SELECT events.record_id
    FROM affected_usage_threads affected
    CROSS JOIN usage_events events INDEXED BY idx_usage_events_named_partition
    WHERE events.workspace_id IS NULLIF(affected.workspace_id, '')
      AND events.attributed_account_id IS NULLIF(affected.account_id, '')
      AND events.thread_name = affected.logical_thread_key
    UNION ALL
    SELECT events.record_id
    FROM affected_usage_threads affected
    CROSS JOIN usage_events events INDEXED BY idx_usage_events_session_partition
    WHERE events.workspace_id IS NULLIF(affected.workspace_id, '')
      AND events.attributed_account_id IS NULLIF(affected.account_id, '')
      AND events.thread_name IS NULL
      AND events.session_id = affected.logical_thread_key
";
const COLLECT_AFFECTED_MODEL_RECORDS_SQL: &str = "
    INSERT OR IGNORE INTO affected_usage_model_records (record_id)
    SELECT events.record_id
    FROM affected_usage_models affected
    CROSS JOIN usage_events events INDEXED BY idx_usage_events_unknown_model
    WHERE events.pricing_confidence = 'unknown'
      AND events.model = affected.model_name
    UNION ALL
    SELECT events.record_id
    FROM affected_usage_models affected
    CROSS JOIN usage_events events INDEXED BY idx_usage_events_unknown_model
    WHERE affected.model_name = 'unknown'
      AND events.pricing_confidence = 'unknown'
      AND events.model IS NULL
";
const COLLECT_MISMATCHED_WORKSPACE_METADATA_SQL: &str = "
    INSERT OR IGNORE INTO mismatched_workspace_metadata (
        record_id, source_file, old_workspace_id, old_account_id, old_thread_key,
        old_model, workspace_id, workspace_label, workspace_home, account_id,
        account_label, attribution_source
    )
    SELECT
        events.record_id,
        events.source_file,
        events.workspace_id,
        events.attributed_account_id,
        COALESCE(events.thread_name, events.session_id),
        events.model,
        metadata.workspace_id,
        metadata.workspace_label,
        metadata.workspace_home,
        metadata.account_id,
        metadata.account_label,
        metadata.attribution_source
    FROM refresh_workspace_metadata metadata
    LEFT JOIN source_files sources ON sources.source_file = metadata.source_file
    CROSS JOIN usage_events events INDEXED BY idx_usage_events_source
    WHERE events.source_file = metadata.source_file
      AND (sources.source_file IS NULL
        OR sources.workspace_id IS NULL
        OR sources.workspace_id != metadata.workspace_id
        OR sources.workspace_home IS NULL
        OR sources.workspace_home != metadata.workspace_home)
      AND (events.workspace_id IS NULL
        OR events.workspace_id != metadata.workspace_id
        OR events.workspace_label IS NULL
        OR events.workspace_label != metadata.workspace_label
        OR events.workspace_home IS NULL
        OR events.workspace_home != metadata.workspace_home
        OR COALESCE(events.attributed_account_id, '') != COALESCE(metadata.account_id, '')
        OR COALESCE(events.attributed_account_label, '') != COALESCE(metadata.account_label, '')
        OR events.attribution_source IS NULL
        OR events.attribution_source != metadata.attribution_source)
";
const REBUILD_AFFECTED_MODEL_FACTS_SQL: &str = "
    INSERT INTO aggregate_diagnostic_facts (
        record_id, fact_type, fact_name, fact_category, event_count, confidence,
        first_event_timestamp, last_event_timestamp, first_source_line,
        last_source_line, evidence_scope, raw_content_included
    )
    SELECT
        'unknown-model:' || COALESCE(events.model, 'unknown'),
        'pricing',
        COALESCE(events.model, 'unknown'),
        'unknown_model',
        COUNT(*),
        1.0,
        MIN(events.event_timestamp),
        MAX(events.event_timestamp),
        MIN(events.line_number),
        MAX(events.line_number),
        'aggregate',
        0
    FROM affected_usage_model_records affected
    CROSS JOIN usage_events events
    WHERE events.record_id = affected.record_id
    GROUP BY COALESCE(events.model, 'unknown')
";
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
    pub scope_id: Option<String>,
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageDashboardRequest {
    pub window: UsageWindow,
    pub include_archived: bool,
    pub scope_id: Option<String>,
    pub account_id: Option<String>,
    pub search: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub pricing_confidence: Option<String>,
    pub sort_key: Option<String>,
    pub sort_direction: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageScope {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub account_id: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDashboardResponse {
    pub scopes: Vec<UsageScope>,
    pub active_scope_id: String,
    pub dashboard: UsageDashboard,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageScopesResponse {
    pub scopes: Vec<UsageScope>,
    pub active_scope_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageInsights {
    pub fast_mode_percent: Option<f64>,
    pub most_used_reasoning: Option<String>,
    pub most_used_reasoning_percent: Option<f64>,
    pub skills_explored: usize,
    pub total_skills_used: usize,
    pub total_threads: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsagePagedResponse<T> {
    pub rows: Vec<T>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub next_offset: Option<usize>,
}

impl<T> Default for UsagePagedResponse<T> {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            total: 0,
            limit: 0,
            offset: 0,
            next_offset: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRateCardEntry {
    pub model: String,
    pub pricing_model: String,
    pub context_window: String,
    pub estimated: bool,
    pub input_per_million: f64,
    pub cached_input_per_million: f64,
    pub output_per_million: f64,
    pub notes: Option<String>,
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
    pub headline_stats: UsageHeadlineStats,
    pub activity_buckets: Vec<UsageActivityBucket>,
    pub top_threads: Vec<UsageThreadSummary>,
    pub recent_calls: Vec<UsageCallRow>,
    pub insights: Option<UsageInsights>,
    pub calls_page: Option<UsagePagedResponse<UsageCallRow>>,
    pub threads_page: Option<UsagePagedResponse<UsageThreadSummary>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageHeadlineStats {
    pub lifetime_tokens: Option<i64>,
    pub peak_daily_tokens: Option<i64>,
    pub longest_running_turn_sec: Option<i64>,
    pub current_streak_days: Option<i64>,
    pub longest_streak_days: Option<i64>,
    pub source: String,
    pub local_total_tokens: i64,
    pub codex_total_tokens: Option<i64>,
    pub token_delta: Option<i64>,
    pub token_delta_percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageActivityBucket {
    pub date: String,
    pub calls: i64,
    pub tokens: i64,
    pub cumulative_calls: i64,
    pub cumulative_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageDashboard {
    pub scope: Option<UsageScope>,
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
    pub workspace_id: Option<String>,
    pub workspace_label: Option<String>,
    pub workspace_home: Option<String>,
    pub attributed_account_id: Option<String>,
    pub attributed_account_label: Option<String>,
    pub attribution_source: Option<String>,
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
    workspace_id: String,
    workspace_label: String,
    workspace_home: PathBuf,
    account_id: Option<String>,
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
    workspace_id: String,
    workspace_label: String,
    workspace_home: PathBuf,
    account_id: Option<String>,
}

struct UsageWorkspace {
    id: String,
    label: String,
    home: PathBuf,
    account_id: Option<String>,
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
    crate::services::lam_paths::LamPaths::for_home(home_root).usage_db_path()
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

pub fn refresh_account_usage_snapshot_index(home_root: &Path) -> Result<()> {
    let _guard = REFRESH_LOCK
        .lock()
        .map_err(|_| AppError::new("USAGE_REFRESH_LOCK", "usage refresh lock is poisoned"))?;
    let db_path = usage_db_path(home_root);
    prepare_usage_dir(&db_path)?;
    let conn = open_usage_db(&db_path)?;
    init_usage_db(&conn)?;
    let mut diagnostics = BTreeMap::new();
    refresh_account_usage_snapshot(home_root, &conn, &mut diagnostics);
    Ok(())
}

fn refresh_usage_index_unlocked(
    home_root: &Path,
    include_archived: bool,
) -> Result<UsageRefreshResult> {
    let db_path = usage_db_path(home_root);
    prepare_usage_dir(&db_path)?;
    let mut conn = open_usage_db(&db_path)?;
    init_usage_db(&conn)?;

    let workspaces = discover_usage_workspaces(home_root)?;
    let mut session_index = HashMap::new();
    for workspace in &workspaces {
        session_index.extend(load_session_index(&workspace.home));
    }
    let logs = find_session_logs(&workspaces, include_archived)?;
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
        apply_parsed_sources(&mut conn, &logs, &parsed, logs.len(), skipped_events)?;
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
           AND (?3 IS NULL OR event_timestamp < ?3)
           AND (?4 IS NULL OR workspace_id = ?4)
           AND (?5 IS NULL OR attributed_account_id = ?5)",
            params![
                req.include_archived,
                filter.from.as_deref(),
                filter.to.as_deref(),
                filter.workspace_id.as_deref(),
                filter.account_id.as_deref()
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
    let activity_buckets = query_activity_buckets(&conn, &req, &filter)?;
    let mut headline_stats = query_local_headline_stats(&conn, &req, &filter, totals.1)?;
    if filter.workspace_id.is_none() && filter.account_id.is_none() {
        apply_latest_account_usage_snapshot(
            &conn,
            &mut headline_stats,
            totals.1,
            &mut diagnostics,
        )?;
    }
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
        headline_stats,
        activity_buckets,
        top_threads,
        recent_calls,
        insights: None,
        calls_page: None,
        threads_page: None,
    })
}

pub fn get_usage_dashboard(home_root: &Path, req: UsageDashboardRequest) -> Result<UsageDashboard> {
    let summary_req = UsageSummaryRequest {
        window: req.window,
        include_archived: req.include_archived,
        scope_id: req.scope_id.clone(),
        account_id: req.account_id.clone(),
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
        scope: None,
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

pub fn get_usage_dashboard_response(
    home_root: &Path,
    req: UsageDashboardRequest,
) -> Result<UsageDashboardResponse> {
    let scope_response = get_usage_scopes(home_root, req.clone())?;
    let active_scope = scope_response
        .scopes
        .iter()
        .find(|scope| scope.id == scope_response.active_scope_id)
        .cloned();
    let mut scoped_req = req;
    scoped_req.scope_id = Some(scope_response.active_scope_id.clone());
    let mut dashboard = get_usage_dashboard(home_root, scoped_req)?;
    dashboard.scope = active_scope;
    Ok(UsageDashboardResponse {
        scopes: scope_response.scopes,
        active_scope_id: scope_response.active_scope_id,
        dashboard,
    })
}

pub fn get_usage_scopes(
    home_root: &Path,
    req: UsageDashboardRequest,
) -> Result<UsageScopesResponse> {
    let scopes = usage_scopes(home_root)?;
    let active_scope_id = req
        .scope_id
        .clone()
        .filter(|id| scopes.iter().any(|scope| &scope.id == id))
        .or_else(|| {
            scopes
                .iter()
                .find(|scope| scope.is_default)
                .map(|scope| scope.id.clone())
        })
        .unwrap_or_else(|| "total".to_string());
    Ok(UsageScopesResponse {
        scopes,
        active_scope_id,
    })
}

pub fn get_usage_overview(home_root: &Path, req: UsageDashboardRequest) -> Result<UsageDashboard> {
    let db_path = usage_db_path(home_root);
    if !db_path.exists() {
        return Ok(UsageDashboard::default());
    }
    let conn = open_usage_db(&db_path)?;
    init_usage_db(&conn)?;
    let summary_req = UsageSummaryRequest {
        window: req.window.clone(),
        include_archived: req.include_archived,
        scope_id: req.scope_id.clone(),
        account_id: req.account_id.clone(),
    };
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
    let filter = SummaryFilter::new(&summary_req);
    let totals = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(total_tokens),0), COALESCE(SUM(input_tokens),0),
                COALESCE(SUM(cached_input_tokens),0), COALESCE(SUM(uncached_input_tokens),0),
                COALESCE(SUM(output_tokens),0), COALESCE(SUM(reasoning_output_tokens),0)
             FROM usage_events
             WHERE (?1 OR is_archived = 0)
               AND (?2 IS NULL OR event_timestamp >= ?2)
               AND (?3 IS NULL OR event_timestamp < ?3)
               AND (?4 IS NULL OR workspace_id = ?4)
               AND (?5 IS NULL OR attributed_account_id = ?5)",
            params![
                summary_req.include_archived,
                filter.from.as_deref(),
                filter.to.as_deref(),
                filter.workspace_id.as_deref(),
                filter.account_id.as_deref()
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
    let (estimated_cost_usd, pricing_coverage) =
        estimate_summary_cost(&model_totals(&conn, &summary_req, &filter)?);
    let mut diagnostics = usage_diagnostics(&conn, &summary_req, &filter, &[], &[])?;
    diagnostics.skipped_events = skipped_events;
    let mut headline_stats = query_local_headline_stats(&conn, &summary_req, &filter, totals.1)?;
    if filter.workspace_id.is_none() && filter.account_id.is_none() {
        apply_latest_account_usage_snapshot(
            &conn,
            &mut headline_stats,
            totals.1,
            &mut diagnostics,
        )?;
    }
    let (model_options, effort_options, pricing_confidence_options) =
        query_usage_options(&conn, &summary_req, &filter)?;
    Ok(UsageDashboard {
        scope: None,
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
                value: skipped_events.to_string(),
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
        summary: UsageSummary {
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
            headline_stats,
            activity_buckets: Vec::new(),
            top_threads: Vec::new(),
            recent_calls: Vec::new(),
            insights: None,
            calls_page: None,
            threads_page: None,
        },
        model_options,
        effort_options,
        pricing_confidence_options,
    })
}

pub fn get_usage_activity(
    home_root: &Path,
    req: UsageDashboardRequest,
) -> Result<Vec<UsageActivityBucket>> {
    let db_path = usage_db_path(home_root);
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let conn = open_usage_db(&db_path)?;
    init_usage_db(&conn)?;
    let summary_req = UsageSummaryRequest {
        window: req.window.clone(),
        include_archived: req.include_archived,
        scope_id: req.scope_id.clone(),
        account_id: req.account_id.clone(),
    };
    let filter = SummaryFilter::new(&summary_req);
    query_activity_buckets(&conn, &summary_req, &filter)
}

pub fn get_usage_insights(home_root: &Path, req: UsageDashboardRequest) -> Result<UsageInsights> {
    let db_path = usage_db_path(home_root);
    if !db_path.exists() {
        return Ok(UsageInsights::default());
    }
    let conn = open_usage_db(&db_path)?;
    init_usage_db(&conn)?;
    let summary_req = UsageSummaryRequest {
        window: req.window.clone(),
        include_archived: req.include_archived,
        scope_id: req.scope_id.clone(),
        account_id: req.account_id.clone(),
    };
    let filter = SummaryFilter::new(&summary_req);
    query_usage_insights(&conn, &req, &filter)
}

pub fn get_usage_rate_card() -> Vec<UsageRateCardEntry> {
    usage_rate_card()
}

pub fn get_usage_calls(
    home_root: &Path,
    req: UsageDashboardRequest,
) -> Result<UsagePagedResponse<UsageCallRow>> {
    let db_path = usage_db_path(home_root);
    if !db_path.exists() {
        return Ok(UsagePagedResponse::default());
    }
    let conn = open_usage_db(&db_path)?;
    init_usage_db(&conn)?;
    let summary_req = UsageSummaryRequest {
        window: req.window.clone(),
        include_archived: req.include_archived,
        scope_id: req.scope_id.clone(),
        account_id: req.account_id.clone(),
    };
    let filter = SummaryFilter::new(&summary_req);
    query_recent_calls_for_dashboard(&conn, &req, &filter)
}

pub fn get_usage_threads(
    home_root: &Path,
    req: UsageDashboardRequest,
) -> Result<UsagePagedResponse<UsageThreadSummary>> {
    let db_path = usage_db_path(home_root);
    if !db_path.exists() {
        return Ok(UsagePagedResponse::default());
    }
    let conn = open_usage_db(&db_path)?;
    init_usage_db(&conn)?;
    let summary_req = UsageSummaryRequest {
        window: req.window.clone(),
        include_archived: req.include_archived,
        scope_id: req.scope_id.clone(),
        account_id: req.account_id.clone(),
    };
    let filter = SummaryFilter::new(&summary_req);
    query_threads_page(&conn, &req, &filter)
}

pub fn get_usage_diagnostics(
    home_root: &Path,
    req: UsageDashboardRequest,
) -> Result<UsageDiagnostics> {
    let db_path = usage_db_path(home_root);
    if !db_path.exists() {
        return Ok(UsageDiagnostics::default());
    }
    let conn = open_usage_db(&db_path)?;
    init_usage_db(&conn)?;
    let summary_req = UsageSummaryRequest {
        window: req.window,
        include_archived: req.include_archived,
        scope_id: req.scope_id,
        account_id: req.account_id,
    };
    let filter = SummaryFilter::new(&summary_req);
    usage_diagnostics(&conn, &summary_req, &filter, &[], &[])
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
            workspace_id TEXT,
            workspace_label TEXT,
            workspace_home TEXT,
            attributed_account_id TEXT,
            attributed_account_label TEXT,
            attribution_source TEXT,
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
            workspace_id TEXT,
            workspace_home TEXT,
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
            workspace_id TEXT,
            workspace_label TEXT,
            attributed_account_id TEXT,
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
        CREATE TABLE IF NOT EXISTS account_usage_snapshot (
            snapshot_id TEXT PRIMARY KEY,
            fetched_at TEXT NOT NULL,
            source TEXT NOT NULL,
            lifetime_tokens INTEGER,
            peak_daily_tokens INTEGER,
            longest_running_turn_sec INTEGER,
            current_streak_days INTEGER,
            longest_streak_days INTEGER,
            raw_daily_bucket_count INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS account_usage_daily_buckets (
            snapshot_id TEXT NOT NULL,
            start_date TEXT NOT NULL,
            tokens INTEGER NOT NULL,
            PRIMARY KEY(snapshot_id, start_date)
        );
        ",
    )
    .map_err(db_error)?;
    let usage_event_columns = [
        ("is_archived", "INTEGER NOT NULL DEFAULT 0"),
        ("workspace_id", "TEXT"),
        ("workspace_label", "TEXT"),
        ("workspace_home", "TEXT"),
        ("attributed_account_id", "TEXT"),
        ("attributed_account_label", "TEXT"),
        ("attribution_source", "TEXT"),
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
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_usage_events_workspace ON usage_events(workspace_id)",
        [],
    )
    .map_err(db_error)?;
    for sql in [
        "CREATE INDEX IF NOT EXISTS idx_usage_events_scope_time ON usage_events(workspace_id, attributed_account_id, event_timestamp)",
        "CREATE INDEX IF NOT EXISTS idx_usage_events_model_time ON usage_events(model, event_timestamp)",
        "CREATE INDEX IF NOT EXISTS idx_usage_events_effort_time ON usage_events(effort, event_timestamp)",
        "CREATE INDEX IF NOT EXISTS idx_usage_events_pricing_time ON usage_events(pricing_confidence, event_timestamp)",
        "CREATE INDEX IF NOT EXISTS idx_usage_events_thread_time ON usage_events(thread_key, event_timestamp)",
        "CREATE INDEX IF NOT EXISTS idx_usage_events_named_partition ON usage_events(workspace_id, attributed_account_id, thread_name)",
        "CREATE INDEX IF NOT EXISTS idx_usage_events_session_partition ON usage_events(workspace_id, attributed_account_id, session_id) WHERE thread_name IS NULL",
        "CREATE INDEX IF NOT EXISTS idx_usage_events_unknown_model ON usage_events(model) WHERE pricing_confidence = 'unknown'",
    ] {
        conn.execute(sql, []).map_err(db_error)?;
    }
    for (column, definition) in [
        ("source_hash", "TEXT"),
        ("workspace_id", "TEXT"),
        ("workspace_home", "TEXT"),
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
    for (column, definition) in [
        ("workspace_id", "TEXT"),
        ("workspace_label", "TEXT"),
        ("attributed_account_id", "TEXT"),
    ] {
        ensure_column(
            conn,
            "thread_summaries",
            column,
            &format!("ALTER TABLE thread_summaries ADD COLUMN {column} {definition}"),
        )?;
    }
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_thread_summaries_scope ON thread_summaries(workspace_id, attributed_account_id, latest_event_timestamp)",
        [],
    )
    .map_err(db_error)?;
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
        if let Err(err) = conn.execute(sql, []) {
            let app_error = db_error(err);
            if !is_duplicate_column_error(&app_error) {
                return Err(app_error);
            }
        }
    }
    Ok(())
}

fn is_duplicate_column_error(error: &AppError) -> bool {
    error.code == "USAGE_DB_ERROR" && error.message.contains("duplicate column name")
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
                workspace_id: log.workspace_id.clone(),
                workspace_label: log.workspace_label.clone(),
                workspace_home: log.workspace_home.clone(),
                account_id: log.account_id.clone(),
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
                workspace_id: log.workspace_id.clone(),
                workspace_label: log.workspace_label.clone(),
                workspace_home: log.workspace_home.clone(),
                account_id: log.account_id.clone(),
                start_byte: previous_byte as u64,
                start_line: previous_line,
                initial_state: state.unwrap_or_default(),
                replace_existing: false,
            });
        } else {
            plans.push(SourceParsePlan {
                path: path.clone(),
                is_archived: log.is_archived,
                workspace_id: log.workspace_id.clone(),
                workspace_label: log.workspace_label.clone(),
                workspace_home: log.workspace_home.clone(),
                account_id: log.account_id.clone(),
                start_byte: 0,
                start_line: 0,
                initial_state: ParserState::default(),
                replace_existing: true,
            });
        }
    }
    Ok(plans)
}

fn backfill_workspace_metadata(
    tx: &rusqlite::Transaction<'_>,
    logs: &[SourceLog],
) -> Result<usize> {
    let mut metadata_stmt = tx
        .prepare(
            "INSERT OR REPLACE INTO refresh_workspace_metadata (
                source_file, workspace_id, workspace_label, workspace_home,
                account_id, account_label, attribution_source
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .map_err(db_error)?;
    for log in logs {
        let source_file = log.path.to_string_lossy().to_string();
        let workspace_home = log.workspace_home.to_string_lossy().to_string();
        let account_label = log.account_id.as_deref().map(account_label_from_id);
        metadata_stmt
            .execute(params![
                source_file,
                log.workspace_id,
                log.workspace_label,
                workspace_home,
                log.account_id.as_deref(),
                account_label.as_deref(),
                "profile_workspace"
            ])
            .map_err(db_error)?;
    }
    drop(metadata_stmt);

    let metadata_changed = tx
        .query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM refresh_workspace_metadata metadata
                LEFT JOIN source_files sources ON sources.source_file = metadata.source_file
                WHERE sources.source_file IS NULL
                   OR sources.workspace_id IS NULL
                   OR sources.workspace_id != metadata.workspace_id
                   OR sources.workspace_home IS NULL
                   OR sources.workspace_home != metadata.workspace_home
            )",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(db_error)?;
    if !metadata_changed {
        return Ok(0);
    }

    tx.execute(COLLECT_MISMATCHED_WORKSPACE_METADATA_SQL, [])
        .map_err(db_error)?;
    tx.execute_batch(
        "
        INSERT OR IGNORE INTO affected_usage_threads
            (workspace_id, account_id, logical_thread_key)
        SELECT COALESCE(old_workspace_id, ''), COALESCE(old_account_id, ''), old_thread_key
        FROM mismatched_workspace_metadata
        UNION ALL
        SELECT COALESCE(workspace_id, ''), COALESCE(account_id, ''), old_thread_key
        FROM mismatched_workspace_metadata;

        INSERT OR IGNORE INTO affected_usage_models (model_name)
        SELECT COALESCE(old_model, 'unknown')
        FROM mismatched_workspace_metadata;

        UPDATE usage_events
        SET workspace_id = (
                SELECT workspace_id FROM mismatched_workspace_metadata
                WHERE record_id = usage_events.record_id
            ),
            workspace_label = (
                SELECT workspace_label FROM mismatched_workspace_metadata
                WHERE record_id = usage_events.record_id
            ),
            workspace_home = (
                SELECT workspace_home FROM mismatched_workspace_metadata
                WHERE record_id = usage_events.record_id
            ),
            attributed_account_id = (
                SELECT account_id FROM mismatched_workspace_metadata
                WHERE record_id = usage_events.record_id
            ),
            attributed_account_label = (
                SELECT account_label FROM mismatched_workspace_metadata
                WHERE record_id = usage_events.record_id
            ),
            attribution_source = (
                SELECT attribution_source FROM mismatched_workspace_metadata
                WHERE record_id = usage_events.record_id
            )
        WHERE record_id IN (SELECT record_id FROM mismatched_workspace_metadata);

        UPDATE source_files
        SET workspace_id = (
                SELECT workspace_id FROM refresh_workspace_metadata
                WHERE source_file = source_files.source_file
            ),
            workspace_home = (
                SELECT workspace_home FROM refresh_workspace_metadata
                WHERE source_file = source_files.source_file
            )
        WHERE source_file IN (SELECT source_file FROM refresh_workspace_metadata)
          AND (workspace_id IS NULL
            OR workspace_id != (
                SELECT workspace_id FROM refresh_workspace_metadata
                WHERE source_file = source_files.source_file
            )
            OR workspace_home IS NULL
            OR workspace_home != (
                SELECT workspace_home FROM refresh_workspace_metadata
                WHERE source_file = source_files.source_file
            ));
        ",
    )
    .map_err(db_error)?;
    let updated = tx
        .query_row(
            "SELECT COUNT(*) FROM mismatched_workspace_metadata",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(db_error)?;
    Ok(updated as usize)
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
    let input_tokens = usage_int(last_usage, "input_tokens").unwrap_or(0);
    let cached_input_tokens = usage_int(last_usage, "cached_input_tokens").unwrap_or(0);
    let uncached_input_tokens = (input_tokens - cached_input_tokens).max(0);
    let output_tokens = usage_int(last_usage, "output_tokens").unwrap_or(0);
    let reasoning_output_tokens = usage_int(last_usage, "reasoning_output_tokens").unwrap_or(0);
    let total_tokens = usage_int(last_usage, "total_tokens").unwrap_or(0);
    if total_tokens <= 0 {
        increment(diagnostics, "missing_last_token_total");
        increment(diagnostics, "skipped_events");
        return;
    }
    if cumulative_total_tokens <= state.last_cumulative_total {
        increment(diagnostics, "cumulative_reset_accepted");
    }
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
        workspace_id: None,
        workspace_label: None,
        workspace_home: None,
        attributed_account_id: None,
        attributed_account_label: None,
        attribution_source: None,
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

fn collect_changed_usage_partitions(tx: &rusqlite::Transaction<'_>) -> Result<()> {
    let has_changes = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM changed_usage_records)
                 OR EXISTS(SELECT 1 FROM replaced_usage_sources)",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(db_error)?;
    if !has_changes {
        return Ok(());
    }
    tx.execute_batch(
        "
        INSERT OR IGNORE INTO affected_usage_threads
            (workspace_id, account_id, logical_thread_key)
        SELECT
            COALESCE(workspace_id, ''),
            COALESCE(attributed_account_id, ''),
            COALESCE(thread_name, session_id)
        FROM usage_events
        WHERE record_id IN (SELECT record_id FROM changed_usage_records)
           OR source_file IN (SELECT source_file FROM replaced_usage_sources);

        INSERT OR IGNORE INTO affected_usage_models (model_name)
        SELECT COALESCE(model, 'unknown')
        FROM usage_events
        WHERE record_id IN (SELECT record_id FROM changed_usage_records)
           OR source_file IN (SELECT source_file FROM replaced_usage_sources);
        ",
    )
    .map_err(db_error)?;
    Ok(())
}

fn begin_usage_refresh_transaction(conn: &mut Connection) -> Result<rusqlite::Transaction<'_>> {
    conn.transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db_error)
}

fn apply_parsed_sources(
    conn: &mut Connection,
    logs: &[SourceLog],
    parsed: &[(SourceParsePlan, ParsedSource)],
    scanned_files: usize,
    skipped_events: usize,
) -> Result<(usize, usize)> {
    let tx = begin_usage_refresh_transaction(conn)?;
    tx.execute_batch(
        "
        CREATE TEMP TABLE IF NOT EXISTS changed_usage_records (
            record_id TEXT PRIMARY KEY
        );
        CREATE TEMP TABLE IF NOT EXISTS replaced_usage_sources (
            source_file TEXT PRIMARY KEY
        );
        CREATE TEMP TABLE IF NOT EXISTS affected_usage_threads (
            workspace_id TEXT NOT NULL,
            account_id TEXT NOT NULL,
            logical_thread_key TEXT NOT NULL,
            PRIMARY KEY (workspace_id, account_id, logical_thread_key)
        );
        CREATE TEMP TABLE IF NOT EXISTS affected_usage_models (
            model_name TEXT PRIMARY KEY
        );
        CREATE TEMP TABLE IF NOT EXISTS affected_usage_records (
            record_id TEXT PRIMARY KEY
        );
        CREATE TEMP TABLE IF NOT EXISTS affected_usage_model_records (
            record_id TEXT PRIMARY KEY
        );
        CREATE TEMP TABLE IF NOT EXISTS refresh_workspace_metadata (
            source_file TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            workspace_label TEXT NOT NULL,
            workspace_home TEXT NOT NULL,
            account_id TEXT,
            account_label TEXT,
            attribution_source TEXT NOT NULL
        );
        CREATE TEMP TABLE IF NOT EXISTS mismatched_workspace_metadata (
            record_id TEXT PRIMARY KEY,
            source_file TEXT NOT NULL,
            old_workspace_id TEXT,
            old_account_id TEXT,
            old_thread_key TEXT NOT NULL,
            old_model TEXT,
            workspace_id TEXT NOT NULL,
            workspace_label TEXT NOT NULL,
            workspace_home TEXT NOT NULL,
            account_id TEXT,
            account_label TEXT,
            attribution_source TEXT NOT NULL
        );
        DELETE FROM changed_usage_records;
        DELETE FROM replaced_usage_sources;
        DELETE FROM affected_usage_threads;
        DELETE FROM affected_usage_models;
        DELETE FROM affected_usage_records;
        DELETE FROM affected_usage_model_records;
        DELETE FROM refresh_workspace_metadata;
        DELETE FROM mismatched_workspace_metadata;
        ",
    )
    .map_err(db_error)?;
    for (plan, source) in parsed {
        if plan.replace_existing {
            tx.execute(
                "INSERT OR IGNORE INTO replaced_usage_sources (source_file) VALUES (?)",
                [source.path.to_string_lossy().to_string()],
            )
            .map_err(db_error)?;
        }
        for event in &source.events {
            tx.execute(
                "INSERT OR IGNORE INTO changed_usage_records (record_id) VALUES (?)",
                [&event.record_id],
            )
            .map_err(db_error)?;
        }
    }
    collect_changed_usage_partitions(&tx)?;
    backfill_workspace_metadata(&tx, logs)?;
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
                    source_file, workspace_id, workspace_label, workspace_home,
                    attributed_account_id, attributed_account_label, attribution_source,
                    is_archived, line_number, turn_id, turn_timestamp, cwd, model,
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
                    ?44, ?45, ?46, ?47, ?48, ?49, ?50, ?51, ?52, ?53, ?54, ?55, ?56, ?57,
                    ?58, ?59)
                ON CONFLICT(record_id) DO UPDATE SET
                    thread_name=excluded.thread_name,
                    session_updated_at=excluded.session_updated_at,
                    event_timestamp=excluded.event_timestamp,
                    workspace_id=excluded.workspace_id,
                    workspace_label=excluded.workspace_label,
                    workspace_home=excluded.workspace_home,
                    attributed_account_id=excluded.attributed_account_id,
                    attributed_account_label=excluded.attributed_account_label,
                    attribution_source=excluded.attribution_source,
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
                    plan.workspace_id,
                    plan.workspace_label,
                    plan.workspace_home.to_string_lossy().to_string(),
                    plan.account_id.as_deref(),
                    plan.account_id
                        .as_deref()
                        .map(account_label_from_id),
                    "profile_workspace",
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
                workspace_id, workspace_home, last_indexed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(source_file) DO UPDATE SET
                is_archived=excluded.is_archived,
                workspace_id=excluded.workspace_id,
                workspace_home=excluded.workspace_home,
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
                plan.workspace_id,
                plan.workspace_home.to_string_lossy().to_string(),
                now
            ],
        )
        .map_err(db_error)?;
    }
    collect_changed_usage_partitions(&tx)?;
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
    let has_threads = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM affected_usage_threads)",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(db_error)?;
    if has_threads {
        tx.execute(COLLECT_AFFECTED_USAGE_RECORDS_SQL, [])
            .map_err(db_error)?;
        tx.execute(
            "UPDATE usage_events
             SET thread_key = COALESCE(thread_name, session_id),
                 thread_call_index = NULL,
                 previous_record_id = NULL,
                 next_record_id = NULL
             WHERE record_id IN (SELECT record_id FROM affected_usage_records)",
            [],
        )
        .map_err(db_error)?;

        let rows = {
            let mut stmt = tx
                .prepare(
                    "SELECT events.record_id,
                        COALESCE(events.workspace_id, '') || '|' ||
                            COALESCE(events.attributed_account_id, '') || '|' ||
                            COALESCE(events.thread_name, events.session_id)
                     FROM affected_usage_records affected
                     CROSS JOIN usage_events events
                     WHERE events.record_id = affected.record_id
                     ORDER BY COALESCE(events.workspace_id, ''),
                        COALESCE(events.attributed_account_id, ''),
                        COALESCE(events.thread_name, events.session_id),
                        events.event_timestamp,
                        events.record_id",
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

        tx.execute(
            "DELETE FROM thread_summaries
             WHERE thread_key IN (
                 SELECT workspace_id || '|' || account_id || '|' || logical_thread_key
                 FROM affected_usage_threads
             )",
            [],
        )
        .map_err(db_error)?;
        tx.execute(
            "
            INSERT INTO thread_summaries (
                thread_key, workspace_id, workspace_label, attributed_account_id,
                is_archived_scope, thread_label, first_event_timestamp,
                latest_event_timestamp, call_count, session_count, input_tokens,
                cached_input_tokens, uncached_input_tokens, output_tokens,
                reasoning_output_tokens, total_tokens, estimated_cost_usd, usage_credits,
                avg_cache_ratio, max_context_window_percent, max_recommendation_score,
                primary_recommendation, call_initiator_summary, archived_call_count, updated_at
            )
            SELECT
                COALESCE(events.workspace_id, '') || '|' ||
                    COALESCE(events.attributed_account_id, '') || '|' ||
                    COALESCE(events.thread_name, events.session_id),
                events.workspace_id,
                MAX(events.workspace_label),
                events.attributed_account_id,
                MAX(events.is_archived),
                COALESCE(events.thread_name, events.session_id),
                MIN(events.event_timestamp),
                MAX(events.event_timestamp),
                COUNT(*),
                COUNT(DISTINCT events.session_id),
                COALESCE(SUM(events.input_tokens), 0),
                COALESCE(SUM(events.cached_input_tokens), 0),
                COALESCE(SUM(events.uncached_input_tokens), 0),
                COALESCE(SUM(events.output_tokens), 0),
                COALESCE(SUM(events.reasoning_output_tokens), 0),
                COALESCE(SUM(events.total_tokens), 0),
                COALESCE(SUM(events.estimated_cost_usd), 0),
                COALESCE(SUM(events.usage_credits), 0),
                CASE WHEN COALESCE(SUM(events.input_tokens), 0) > 0
                    THEN CAST(COALESCE(SUM(events.cached_input_tokens), 0) AS REAL) /
                        COALESCE(SUM(events.input_tokens), 0)
                    ELSE 0 END,
                MAX(events.context_window_percent),
                MAX(CASE
                    WHEN events.input_tokens >= 50000 AND events.cache_ratio < 0.2 THEN 90
                    WHEN events.context_window_percent >= 0.8 THEN 75
                    ELSE 0
                END),
                CASE
                    WHEN MAX(events.context_window_percent) >= 0.8
                        THEN 'Inspect high context usage'
                    WHEN COALESCE(SUM(events.input_tokens), 0) >= 50000
                        AND CAST(COALESCE(SUM(events.cached_input_tokens), 0) AS REAL) /
                            COALESCE(SUM(events.input_tokens), 1) < 0.2
                        THEN 'Inspect low cache reuse'
                    ELSE NULL
                END,
                MAX(events.call_initiator),
                SUM(CASE WHEN events.is_archived != 0 THEN 1 ELSE 0 END),
                ?1
            FROM affected_usage_records affected
            CROSS JOIN usage_events events
            WHERE events.record_id = affected.record_id
            GROUP BY events.workspace_id,
                events.attributed_account_id,
                COALESCE(events.thread_name, events.session_id)",
            [now],
        )
        .map_err(db_error)?;
    }

    let has_models = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM affected_usage_models)",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(db_error)?;
    if has_models {
        tx.execute(COLLECT_AFFECTED_MODEL_RECORDS_SQL, [])
            .map_err(db_error)?;
        tx.execute(
            "DELETE FROM aggregate_diagnostic_facts
             WHERE fact_category = 'unknown_model'
               AND EXISTS (
                   SELECT 1
                   FROM affected_usage_models affected
                   WHERE aggregate_diagnostic_facts.record_id =
                       'unknown-model:' || affected.model_name
               )",
            [],
        )
        .map_err(db_error)?;
        tx.execute(REBUILD_AFFECTED_MODEL_FACTS_SQL, [])
            .map_err(db_error)?;
    }
    Ok(())
}

struct SummaryFilter {
    from: Option<String>,
    to: Option<String>,
    workspace_id: Option<String>,
    account_id: Option<String>,
}

impl SummaryFilter {
    fn new(req: &UsageSummaryRequest) -> Self {
        let now = Local::now();
        let date = now.date_naive();
        let workspace_id = req
            .scope_id
            .as_deref()
            .and_then(|id| id.strip_prefix("workspace:"))
            .map(|id| format!("workspace:{id}"));
        let account_id = req.account_id.clone();
        let (from, to) = match req.window.preset.as_str() {
            "today" => (
                Some(day_start(date)),
                Some(day_start(date + ChronoDuration::days(1))),
            ),
            "this-week" => {
                let first =
                    date - ChronoDuration::days(date.weekday().num_days_from_monday() as i64);
                (
                    Some(day_start(first)),
                    Some(day_start(date + ChronoDuration::days(1))),
                )
            }
            "7d" | "last-7-days" => (
                Some(day_start(date - ChronoDuration::days(6))),
                Some(day_start(date + ChronoDuration::days(1))),
            ),
            "30d" => (
                Some(day_start(date - ChronoDuration::days(29))),
                Some(day_start(date + ChronoDuration::days(1))),
            ),
            "month" | "this-month" => {
                let first = NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap_or(date);
                let next = if date.month() == 12 {
                    NaiveDate::from_ymd_opt(date.year() + 1, 1, 1).unwrap_or(first)
                } else {
                    NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1).unwrap_or(first)
                };
                (Some(day_start(first)), Some(day_start(next)))
            }
            "custom" => (
                req.window.from.as_deref().and_then(custom_start),
                req.window.to.as_deref().and_then(custom_end),
            ),
            _ => (None, None),
        };
        Self {
            from,
            to,
            workspace_id,
            account_id,
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

fn query_local_headline_stats(
    conn: &Connection,
    req: &UsageSummaryRequest,
    filter: &SummaryFilter,
    local_total_tokens: i64,
) -> Result<UsageHeadlineStats> {
    let peak_daily_tokens = conn
        .query_row(
            "SELECT MAX(day_tokens) FROM (
                SELECT substr(event_timestamp, 1, 10) AS activity_date,
                       SUM(total_tokens) AS day_tokens
                FROM usage_events
                WHERE (?1 OR is_archived = 0)
                  AND (?2 IS NULL OR event_timestamp >= ?2)
                  AND (?3 IS NULL OR event_timestamp < ?3)
                  AND (?4 IS NULL OR workspace_id = ?4)
                  AND (?5 IS NULL OR attributed_account_id = ?5)
                GROUP BY activity_date
            )",
            params![
                req.include_archived,
                filter.from.as_deref(),
                filter.to.as_deref(),
                filter.workspace_id.as_deref(),
                filter.account_id.as_deref()
            ],
            |row| row.get::<_, Option<i64>>(0),
        )
        .map_err(db_error)?
        .unwrap_or(0);
    let longest_running_turn_sec = conn
        .query_row(
            "SELECT COALESCE(MAX(duration_sec), 0) FROM (
                SELECT CASE WHEN COUNT(*) > 1
                    THEN CAST((julianday(MAX(event_timestamp)) - julianday(MIN(event_timestamp))) * 86400 AS INTEGER)
                    ELSE 0
                END AS duration_sec
                FROM usage_events
                WHERE (?1 OR is_archived = 0)
                  AND (?2 IS NULL OR event_timestamp >= ?2)
                  AND (?3 IS NULL OR event_timestamp < ?3)
                  AND (?4 IS NULL OR workspace_id = ?4)
                  AND (?5 IS NULL OR attributed_account_id = ?5)
                GROUP BY COALESCE(turn_id, record_id)
            )",
            params![
                req.include_archived,
                filter.from.as_deref(),
                filter.to.as_deref(),
                filter.workspace_id.as_deref(),
                filter.account_id.as_deref()
            ],
            |row| row.get::<_, i64>(0),
        )
        .map_err(db_error)?;
    let dates = query_activity_dates(conn, req, filter)?;
    let current_streak_days = current_streak_len(&dates);
    let longest_streak_days = longest_streak_len(&dates);
    Ok(UsageHeadlineStats {
        lifetime_tokens: Some(local_total_tokens),
        peak_daily_tokens: Some(peak_daily_tokens),
        longest_running_turn_sec: Some(longest_running_turn_sec),
        current_streak_days: Some(current_streak_days),
        longest_streak_days: Some(longest_streak_days),
        source: "local_sqlite".to_string(),
        local_total_tokens,
        codex_total_tokens: None,
        token_delta: None,
        token_delta_percent: None,
    })
}

fn query_activity_buckets(
    conn: &Connection,
    req: &UsageSummaryRequest,
    filter: &SummaryFilter,
) -> Result<Vec<UsageActivityBucket>> {
    let mut stmt = conn
        .prepare(
            "SELECT substr(event_timestamp, 1, 10) AS activity_date,
                    COUNT(*) AS calls,
                    COALESCE(SUM(total_tokens), 0) AS tokens
             FROM usage_events
             WHERE (?1 OR is_archived = 0)
               AND (?2 IS NULL OR event_timestamp >= ?2)
               AND (?3 IS NULL OR event_timestamp < ?3)
             GROUP BY activity_date
             ORDER BY activity_date ASC",
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
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .map_err(db_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(db_error)?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let Some(first) = parse_activity_date(&rows[0].0) else {
        return Ok(Vec::new());
    };
    let Some(last) = parse_activity_date(&rows[rows.len() - 1].0) else {
        return Ok(Vec::new());
    };
    let by_date = rows
        .into_iter()
        .map(|(date, calls, tokens)| (date, (calls, tokens)))
        .collect::<BTreeMap<_, _>>();
    let mut buckets = Vec::new();
    let mut cumulative_calls = 0;
    let mut cumulative_tokens = 0;
    let mut date = first;
    while date <= last {
        let key = date.format("%Y-%m-%d").to_string();
        let (calls, tokens) = by_date.get(&key).copied().unwrap_or((0, 0));
        cumulative_calls += calls;
        cumulative_tokens += tokens;
        buckets.push(UsageActivityBucket {
            date: key,
            calls,
            tokens,
            cumulative_calls,
            cumulative_tokens,
        });
        date += ChronoDuration::days(1);
    }
    Ok(buckets)
}

fn query_activity_dates(
    conn: &Connection,
    req: &UsageSummaryRequest,
    filter: &SummaryFilter,
) -> Result<Vec<NaiveDate>> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT substr(event_timestamp, 1, 10) AS activity_date
             FROM usage_events
             WHERE (?1 OR is_archived = 0)
               AND (?2 IS NULL OR event_timestamp >= ?2)
               AND (?3 IS NULL OR event_timestamp < ?3)
             ORDER BY activity_date ASC",
        )
        .map_err(db_error)?;
    let dates = stmt
        .query_map(
            params![
                req.include_archived,
                filter.from.as_deref(),
                filter.to.as_deref()
            ],
            |row| row.get::<_, String>(0),
        )
        .map_err(db_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(db_error)?
        .into_iter()
        .filter_map(|value| parse_activity_date(&value))
        .collect();
    Ok(dates)
}

fn parse_activity_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

fn current_streak_len(dates: &[NaiveDate]) -> i64 {
    let mut iter = dates.iter().rev();
    let Some(mut prev) = iter.next().copied() else {
        return 0;
    };
    let mut count = 1;
    for date in iter {
        if *date == prev - ChronoDuration::days(1) {
            count += 1;
            prev = *date;
        } else {
            break;
        }
    }
    count
}

fn longest_streak_len(dates: &[NaiveDate]) -> i64 {
    let mut longest = 0;
    let mut current = 0;
    let mut prev: Option<NaiveDate> = None;
    for date in dates {
        current = if Some(*date - ChronoDuration::days(1)) == prev {
            current + 1
        } else {
            1
        };
        longest = longest.max(current);
        prev = Some(*date);
    }
    longest
}

fn apply_latest_account_usage_snapshot(
    conn: &Connection,
    stats: &mut UsageHeadlineStats,
    local_total_tokens: i64,
    diagnostics: &mut UsageDiagnostics,
) -> Result<()> {
    let snapshot = conn
        .query_row(
            "SELECT lifetime_tokens, peak_daily_tokens, longest_running_turn_sec,
                    current_streak_days, longest_streak_days
             FROM account_usage_snapshot
             ORDER BY fetched_at DESC
             LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            },
        )
        .optional()
        .map_err(db_error)?;
    if let Some((lifetime, peak, longest_task, current_streak, longest_streak)) = snapshot {
        let has_meaningful_headline =
            [lifetime, peak, longest_task, current_streak, longest_streak]
                .into_iter()
                .flatten()
                .any(|value| value > 0);
        if !has_meaningful_headline {
            diagnostics
                .parser_diagnostics
                .entry("account_usage_empty".to_string())
                .or_insert(1);
            return Ok(());
        }
        stats.lifetime_tokens = lifetime;
        stats.peak_daily_tokens = peak;
        stats.longest_running_turn_sec = longest_task;
        stats.current_streak_days = current_streak;
        stats.longest_streak_days = longest_streak;
        stats.source = "codex_account_usage".to_string();
        stats.codex_total_tokens = lifetime;
        stats.token_delta = lifetime.map(|total| total - local_total_tokens);
        stats.token_delta_percent = lifetime.and_then(|total| {
            if total > 0 {
                Some((total - local_total_tokens) as f64 / total as f64)
            } else {
                None
            }
        });
    } else {
        diagnostics
            .parser_diagnostics
            .entry("account_usage_unavailable".to_string())
            .or_insert(1);
    }
    Ok(())
}

#[derive(Debug)]
struct AccountUsageSnapshot {
    lifetime_tokens: Option<i64>,
    peak_daily_tokens: Option<i64>,
    longest_running_turn_sec: Option<i64>,
    current_streak_days: Option<i64>,
    longest_streak_days: Option<i64>,
    daily_buckets: Vec<(String, i64)>,
}

fn refresh_account_usage_snapshot(
    home_root: &Path,
    conn: &Connection,
    diagnostics: &mut BTreeMap<String, i64>,
) {
    let Some(account) = list_accounts(home_root)
        .ok()
        .and_then(|accounts| accounts.into_iter().find(|account| account.has_auth))
    else {
        increment(diagnostics, "account_usage_unavailable");
        return;
    };
    match try_codex_app_server_account_usage(home_root, &account)
        .and_then(|snapshot| store_account_usage_snapshot(conn, &snapshot))
    {
        Ok(()) => {}
        Err(_) => increment(diagnostics, "account_usage_unavailable"),
    }
}

fn try_codex_app_server_account_usage(
    home_root: &Path,
    account: &CodexAccount,
) -> Result<AccountUsageSnapshot> {
    let mut child = spawn_codex_app_server(home_root, &account.codex_home)?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(
                b"{\"id\":1,\"method\":\"initialize\",\"params\":{\"clientInfo\":{\"name\":\"lam\",\"version\":\"0.1\"},\"capabilities\":{\"experimentalApi\":true}}}\n",
            )
            .map_err(|err| AppError::new("CODEX_APP_SERVER_WRITE_FAILED", err.to_string()))?;
        stdin
            .write_all(b"{\"id\":2,\"method\":\"account/usage/read\",\"params\":null}\n")
            .map_err(|err| AppError::new("CODEX_APP_SERVER_WRITE_FAILED", err.to_string()))?;
        stdin
            .flush()
            .map_err(|err| AppError::new("CODEX_APP_SERVER_WRITE_FAILED", err.to_string()))?;
    }
    let Some(stdout) = child.stdout.take() else {
        terminate_usage_child(&mut child);
        return Err(AppError::new(
            "CODEX_APP_SERVER_NO_STDOUT",
            "stdout not available",
        ));
    };
    let Some(stderr) = child.stderr.take() else {
        terminate_usage_child(&mut child);
        return Err(AppError::new(
            "CODEX_APP_SERVER_NO_STDERR",
            "stderr not available",
        ));
    };
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let (tx_err, rx_err) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(std::result::Result::ok) {
            let _ = tx.send(line);
        }
    });
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(std::result::Result::ok) {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                let _ = tx_err.send(trimmed.to_string());
            }
        }
    });
    let deadline = std::time::Instant::now() + CODEX_APP_SERVER_USAGE_TIMEOUT;
    let mut last_stderr: Option<String> = None;
    loop {
        while let Ok(line) = rx_err.try_recv() {
            last_stderr = Some(line);
        }
        while let Ok(line) = rx.try_recv() {
            if let Some(snapshot) = parse_account_usage_snapshot_line(&line) {
                terminate_usage_child(&mut child);
                return Ok(snapshot);
            }
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
                    format!("app-server exited before account usage was parsed{stderr_hint}"),
                )
            } else {
                AppError::new(
                    "CODEX_APP_SERVER_FAILED",
                    format!("app-server exited before account usage could be read{stderr_hint}"),
                )
            });
        }
        if std::time::Instant::now() > deadline {
            terminate_usage_child(&mut child);
            let stderr_hint = last_stderr
                .as_deref()
                .map(|line| format!(" ({line})"))
                .unwrap_or_default();
            return Err(AppError::new(
                "CODEX_APP_SERVER_TIMEOUT",
                format!("app-server account usage request timed out{stderr_hint}"),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

fn terminate_usage_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn parse_account_usage_snapshot_line(line: &str) -> Option<AccountUsageSnapshot> {
    let value: Value = serde_json::from_str(line).ok()?;
    let result = value.get("result").unwrap_or(&value);
    let summary = result.get("summary")?;
    let daily_buckets = result
        .get("dailyUsageBuckets")
        .or_else(|| result.get("daily_usage_buckets"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let date = item
                        .get("startDate")
                        .or_else(|| item.get("start_date"))
                        .and_then(Value::as_str)?;
                    let tokens = item.get("tokens").and_then(Value::as_i64)?;
                    Some((date.to_string(), tokens))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Some(AccountUsageSnapshot {
        lifetime_tokens: json_i64_alias(summary, &["lifetimeTokens", "lifetime_tokens"]),
        peak_daily_tokens: json_i64_alias(summary, &["peakDailyTokens", "peak_daily_tokens"]),
        longest_running_turn_sec: json_i64_alias(
            summary,
            &["longestRunningTurnSec", "longest_running_turn_sec"],
        ),
        current_streak_days: json_i64_alias(summary, &["currentStreakDays", "current_streak_days"]),
        longest_streak_days: json_i64_alias(summary, &["longestStreakDays", "longest_streak_days"]),
        daily_buckets,
    })
}

fn json_i64_alias(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| value.get(*key)).and_then(|v| {
        v.as_i64()
            .or_else(|| v.as_u64().and_then(|raw| i64::try_from(raw).ok()))
    })
}

fn store_account_usage_snapshot(conn: &Connection, snapshot: &AccountUsageSnapshot) -> Result<()> {
    let snapshot_id = format!("account-usage-{}", Utc::now().timestamp_millis());
    let fetched_at = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT OR REPLACE INTO account_usage_snapshot (
            snapshot_id, fetched_at, source, lifetime_tokens, peak_daily_tokens,
            longest_running_turn_sec, current_streak_days, longest_streak_days,
            raw_daily_bucket_count
        ) VALUES (?1, ?2, 'codex_account_usage', ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            snapshot_id,
            fetched_at,
            snapshot.lifetime_tokens,
            snapshot.peak_daily_tokens,
            snapshot.longest_running_turn_sec,
            snapshot.current_streak_days,
            snapshot.longest_streak_days,
            snapshot.daily_buckets.len() as i64,
        ],
    )
    .map_err(db_error)?;
    for (date, tokens) in &snapshot.daily_buckets {
        conn.execute(
            "INSERT OR REPLACE INTO account_usage_daily_buckets (snapshot_id, start_date, tokens)
             VALUES (?1, ?2, ?3)",
            params![snapshot_id, date, tokens],
        )
        .map_err(db_error)?;
    }
    Ok(())
}

fn query_recent_calls(
    conn: &Connection,
    req: &UsageSummaryRequest,
    filter: &SummaryFilter,
) -> Result<Vec<UsageCallRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT record_id, session_id, thread_name, session_updated_at, event_timestamp,
                source_file, workspace_id, workspace_label, workspace_home,
                attributed_account_id, attributed_account_label, attribution_source,
                line_number, turn_id, turn_timestamp, cwd, model, effort,
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
               AND (?4 IS NULL OR workspace_id = ?4)
               AND (?5 IS NULL OR attributed_account_id = ?5)
             ORDER BY event_timestamp DESC, record_id ASC",
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map(
            params![
                req.include_archived,
                filter.from.as_deref(),
                filter.to.as_deref(),
                filter.workspace_id.as_deref(),
                filter.account_id.as_deref()
            ],
            read_call_row,
        )
        .map_err(db_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(db_error)?;
    Ok(rows)
}

fn query_recent_calls_for_dashboard(
    conn: &Connection,
    req: &UsageDashboardRequest,
    filter: &SummaryFilter,
) -> Result<UsagePagedResponse<UsageCallRow>> {
    let limit = req.limit.unwrap_or(100).min(5000);
    let offset = req.offset.unwrap_or(0);
    if limit == 0 {
        return Ok(UsagePagedResponse {
            rows: Vec::new(),
            total: 0,
            limit,
            offset,
            next_offset: None,
        });
    }
    let sort_expr = match req.sort_key.as_deref().unwrap_or("time") {
        "total" | "usage" => "total_tokens",
        "cached" => "cached_input_tokens",
        "uncached" => "uncached_input_tokens",
        "output" => "output_tokens",
        "reasoning" => "reasoning_output_tokens",
        "cost" => "estimated_cost_usd",
        "cache" => "cache_ratio",
        "context" => "context_window_percent",
        "model" => "model",
        "effort" => "effort",
        "thread" => "COALESCE(thread_name, session_id)",
        "initiator" => "call_initiator",
        _ => "event_timestamp",
    };
    let direction = if req.sort_direction.as_deref() == Some("asc") {
        "ASC"
    } else {
        "DESC"
    };
    let search_like = req
        .search
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("%{}%", value.to_ascii_lowercase()));
    let where_sql = "FROM usage_events
         WHERE (?1 OR is_archived = 0)
           AND (?2 IS NULL OR event_timestamp >= ?2)
           AND (?3 IS NULL OR event_timestamp < ?3)
           AND (?4 IS NULL OR workspace_id = ?4)
           AND (?5 IS NULL OR attributed_account_id = ?5)
           AND (?6 IS NULL OR model = ?6)
           AND (?7 IS NULL OR effort = ?7)
           AND (?8 IS NULL OR pricing_confidence = ?8)
           AND (?9 IS NULL OR lower(COALESCE(thread_name, session_id) || ' ' || COALESCE(cwd, '') || ' ' || COALESCE(model, '')) LIKE ?9)";
    let total = conn
        .query_row(
            &format!("SELECT COUNT(*) {where_sql}"),
            params![
                req.include_archived,
                filter.from.as_deref(),
                filter.to.as_deref(),
                filter.workspace_id.as_deref(),
                filter.account_id.as_deref(),
                req.model.as_deref().filter(|value| !value.is_empty()),
                req.effort.as_deref().filter(|value| !value.is_empty()),
                req.pricing_confidence
                    .as_deref()
                    .filter(|value| !value.is_empty()),
                search_like.as_deref(),
            ],
            |row| row.get::<_, i64>(0),
        )
        .map_err(db_error)? as usize;
    let sql = format!(
        "SELECT record_id, session_id, thread_name, session_updated_at, event_timestamp,
            source_file, workspace_id, workspace_label, workspace_home,
            attributed_account_id, attributed_account_label, attribution_source,
            line_number, turn_id, turn_timestamp, cwd, model, effort,
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
         {where_sql}
         ORDER BY {sort_expr} {direction}, record_id ASC
         LIMIT ?10 OFFSET ?11"
    );
    let mut stmt = conn.prepare(&sql).map_err(db_error)?;
    let rows = stmt
        .query_map(
            params![
                req.include_archived,
                filter.from.as_deref(),
                filter.to.as_deref(),
                filter.workspace_id.as_deref(),
                filter.account_id.as_deref(),
                req.model.as_deref().filter(|value| !value.is_empty()),
                req.effort.as_deref().filter(|value| !value.is_empty()),
                req.pricing_confidence
                    .as_deref()
                    .filter(|value| !value.is_empty()),
                search_like.as_deref(),
                limit as i64,
                offset as i64,
            ],
            read_call_row,
        )
        .map_err(db_error)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(db_error)?;
    let next_offset = (offset + rows.len() < total).then_some(offset + rows.len());
    Ok(UsagePagedResponse {
        rows,
        total,
        limit,
        offset,
        next_offset,
    })
}

fn query_top_threads(
    conn: &Connection,
    req: &UsageSummaryRequest,
    filter: &SummaryFilter,
) -> Result<Vec<UsageThreadSummary>> {
    let page_req = UsageDashboardRequest {
        window: req.window.clone(),
        include_archived: req.include_archived,
        scope_id: req.scope_id.clone(),
        account_id: req.account_id.clone(),
        search: None,
        model: None,
        effort: None,
        pricing_confidence: None,
        sort_key: Some("total".to_string()),
        sort_direction: Some("desc".to_string()),
        limit: Some(50),
        offset: Some(0),
    };
    Ok(query_threads_page(conn, &page_req, filter)?.rows)
}

fn query_threads_page(
    conn: &Connection,
    req: &UsageDashboardRequest,
    filter: &SummaryFilter,
) -> Result<UsagePagedResponse<UsageThreadSummary>> {
    let limit = req.limit.unwrap_or(50).min(5000);
    let offset = req.offset.unwrap_or(0);
    if limit == 0 {
        return Ok(UsagePagedResponse {
            rows: Vec::new(),
            total: 0,
            limit,
            offset,
            next_offset: None,
        });
    }
    let search_like = req
        .search
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("%{}%", value.to_ascii_lowercase()));
    let sort_expr = match req.sort_key.as_deref().unwrap_or("total") {
        "time" => "latest_event_timestamp",
        "calls" => "call_count",
        "cached" => "cached_input_tokens",
        "uncached" => "uncached_input_tokens",
        "output" => "output_tokens",
        "reasoning" => "reasoning_output_tokens",
        "cost" => "estimated_cost_usd",
        "cache" => "avg_cache_ratio",
        _ => "total_tokens",
    };
    let direction = if req.sort_direction.as_deref() == Some("asc") {
        "ASC"
    } else {
        "DESC"
    };
    let where_sql = "FROM thread_summaries
             WHERE (?1 OR is_archived_scope = 0)
               AND (?2 IS NULL OR latest_event_timestamp >= ?2)
               AND (?3 IS NULL OR latest_event_timestamp < ?3)
               AND (?4 IS NULL OR workspace_id = ?4)
               AND (?5 IS NULL OR attributed_account_id = ?5)
               AND (?6 IS NULL OR lower(thread_label) LIKE ?6)";
    let total = conn
        .query_row(
            &format!("SELECT COUNT(*) {where_sql}"),
            params![
                req.include_archived,
                filter.from.as_deref(),
                filter.to.as_deref(),
                filter.workspace_id.as_deref(),
                filter.account_id.as_deref(),
                search_like.as_deref(),
            ],
            |row| row.get::<_, i64>(0),
        )
        .map_err(db_error)? as usize;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT thread_key, thread_label, first_event_timestamp, call_count,
                session_count, total_tokens, input_tokens, cached_input_tokens,
                uncached_input_tokens, output_tokens, reasoning_output_tokens,
                latest_event_timestamp, is_archived_scope, max_context_window_percent,
                archived_call_count, call_initiator_summary, estimated_cost_usd,
                usage_credits, avg_cache_ratio, max_recommendation_score,
                primary_recommendation, updated_at
             {where_sql}
             ORDER BY {sort_expr} {direction}, thread_key ASC
             LIMIT ?7 OFFSET ?8"
        ))
        .map_err(db_error)?;
    let rows = stmt
        .query_map(
            params![
                req.include_archived,
                filter.from.as_deref(),
                filter.to.as_deref(),
                filter.workspace_id.as_deref(),
                filter.account_id.as_deref(),
                search_like.as_deref(),
                limit as i64,
                offset as i64,
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
                    max_context_window_percent: row.get(13)?,
                    max_recommendation_score: row.get(19)?,
                    primary_recommendation: row.get(20)?,
                    call_initiator_summary: row.get(15)?,
                    archived_call_count: row.get::<_, i64>(14)? as usize,
                    updated_at: row.get(21)?,
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
    let next_offset = (offset + rows.len() < total).then_some(offset + rows.len());
    Ok(UsagePagedResponse {
        rows,
        total,
        limit,
        offset,
        next_offset,
    })
}

fn query_usage_insights(
    conn: &Connection,
    req: &UsageDashboardRequest,
    filter: &SummaryFilter,
) -> Result<UsageInsights> {
    let search_like = req
        .search
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| format!("%{}%", value.to_ascii_lowercase()));
    let where_sql = "FROM usage_events
         WHERE (?1 OR is_archived = 0)
           AND (?2 IS NULL OR event_timestamp >= ?2)
           AND (?3 IS NULL OR event_timestamp < ?3)
           AND (?4 IS NULL OR workspace_id = ?4)
           AND (?5 IS NULL OR attributed_account_id = ?5)
           AND (?6 IS NULL OR model = ?6)
           AND (?7 IS NULL OR effort = ?7)
           AND (?8 IS NULL OR pricing_confidence = ?8)
           AND (?9 IS NULL OR lower(COALESCE(thread_name, session_id) || ' ' || COALESCE(cwd, '') || ' ' || COALESCE(model, '')) LIKE ?9)";
    let (total_calls, fast_calls, skills_explored, total_skills_used, total_threads) = conn
        .query_row(
            &format!(
                "SELECT COUNT(*),
                    SUM(CASE WHEN lower(COALESCE(effort,'')) IN ('fast','low','minimal','none') THEN 1 ELSE 0 END),
                    COUNT(DISTINCT lower(COALESCE(subagent_type, agent_role, agent_nickname))),
                    SUM(CASE WHEN COALESCE(subagent_type, agent_role, agent_nickname) IS NULL THEN 0 ELSE 1 END),
                    COUNT(DISTINCT COALESCE(thread_key, thread_name, session_id))
                 {where_sql}"
            ),
            params![
                req.include_archived,
                filter.from.as_deref(),
                filter.to.as_deref(),
                filter.workspace_id.as_deref(),
                filter.account_id.as_deref(),
                req.model.as_deref().filter(|value| !value.is_empty()),
                req.effort.as_deref().filter(|value| !value.is_empty()),
                req.pricing_confidence
                    .as_deref()
                    .filter(|value| !value.is_empty()),
                search_like.as_deref(),
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .map_err(db_error)?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT effort, COUNT(*) AS count
             {where_sql}
             AND effort IS NOT NULL
             AND trim(effort) != ''
             GROUP BY lower(effort), effort
             ORDER BY count DESC, effort ASC
             LIMIT 1"
        ))
        .map_err(db_error)?;
    let top_reasoning = stmt
        .query_row(
            params![
                req.include_archived,
                filter.from.as_deref(),
                filter.to.as_deref(),
                filter.workspace_id.as_deref(),
                filter.account_id.as_deref(),
                req.model.as_deref().filter(|value| !value.is_empty()),
                req.effort.as_deref().filter(|value| !value.is_empty()),
                req.pricing_confidence
                    .as_deref()
                    .filter(|value| !value.is_empty()),
                search_like.as_deref(),
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(db_error)?;
    Ok(UsageInsights {
        fast_mode_percent: (total_calls > 0).then_some(fast_calls as f64 / total_calls as f64),
        most_used_reasoning: top_reasoning
            .as_ref()
            .map(|(effort, _)| title_label(effort)),
        most_used_reasoning_percent: top_reasoning
            .as_ref()
            .and_then(|(_, count)| (total_calls > 0).then_some(*count as f64 / total_calls as f64)),
        skills_explored: skills_explored as usize,
        total_skills_used: total_skills_used as usize,
        total_threads: total_threads as usize,
    })
}

fn title_label(value: &str) -> String {
    value
        .split([' ', '_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str().to_lowercase()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn read_call_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<UsageCallRow> {
    let mut item = UsageCallRow {
        record_id: row.get(0)?,
        session_id: row.get(1)?,
        thread_name: row.get(2)?,
        session_updated_at: row.get(3)?,
        event_timestamp: row.get(4)?,
        source_file: row.get(5)?,
        workspace_id: row.get(6)?,
        workspace_label: row.get(7)?,
        workspace_home: row.get(8)?,
        attributed_account_id: row.get(9)?,
        attributed_account_label: row.get(10)?,
        attribution_source: row.get(11)?,
        line_number: row.get(12)?,
        turn_id: row.get(13)?,
        turn_timestamp: row.get(14)?,
        cwd: row.get(15)?,
        model: row.get(16)?,
        effort: row.get(17)?,
        current_date: row.get(18)?,
        timezone: row.get(19)?,
        call_initiator: row.get(20)?,
        call_initiator_reason: row.get(21)?,
        call_initiator_confidence: row.get(22)?,
        input_tokens: row.get(23)?,
        cached_input_tokens: row.get(24)?,
        uncached_input_tokens: row.get(25)?,
        output_tokens: row.get(26)?,
        reasoning_output_tokens: row.get(27)?,
        total_tokens: row.get(28)?,
        cumulative_total_tokens: row.get(29)?,
        cache_ratio: row.get(30)?,
        is_archived: row.get::<_, i64>(31)? != 0,
        thread_key: row.get(32)?,
        thread_call_index: row.get(33)?,
        previous_record_id: row.get(34)?,
        next_record_id: row.get(35)?,
        thread_source: row.get(36)?,
        subagent_type: row.get(37)?,
        agent_role: row.get(38)?,
        agent_nickname: row.get(39)?,
        parent_session_id: row.get(40)?,
        parent_thread_name: row.get(41)?,
        parent_session_updated_at: row.get(42)?,
        model_context_window: row.get(43)?,
        context_window_percent: row.get(44)?,
        rate_limit_plan_type: row.get(45)?,
        rate_limit_limit_id: row.get(46)?,
        rate_limit_primary_used_percent: row.get(47)?,
        rate_limit_primary_window_minutes: row.get(48)?,
        rate_limit_primary_resets_at: row.get(49)?,
        rate_limit_secondary_used_percent: row.get(50)?,
        rate_limit_secondary_window_minutes: row.get(51)?,
        rate_limit_secondary_resets_at: row.get(52)?,
        reasoning_output_ratio: row.get(53)?,
        estimated_cost_usd: row.get(54)?,
        usage_credits: row.get(55)?,
        pricing_model: row.get(56)?,
        pricing_estimated: row.get::<_, i64>(57)? != 0,
        pricing_confidence: row.get(58)?,
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

fn query_usage_options(
    conn: &Connection,
    req: &UsageSummaryRequest,
    filter: &SummaryFilter,
) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
    fn distinct_values(
        conn: &Connection,
        column: &str,
        req: &UsageSummaryRequest,
        filter: &SummaryFilter,
    ) -> Result<Vec<String>> {
        let sql = format!(
            "SELECT DISTINCT {column}
             FROM usage_events
             WHERE (?1 OR is_archived = 0)
               AND (?2 IS NULL OR event_timestamp >= ?2)
               AND (?3 IS NULL OR event_timestamp < ?3)
               AND (?4 IS NULL OR workspace_id = ?4)
               AND (?5 IS NULL OR attributed_account_id = ?5)
               AND {column} IS NOT NULL
               AND {column} != ''
             ORDER BY {column} ASC"
        );
        let mut stmt = conn.prepare(&sql).map_err(db_error)?;
        let values = stmt
            .query_map(
                params![
                    req.include_archived,
                    filter.from.as_deref(),
                    filter.to.as_deref(),
                    filter.workspace_id.as_deref(),
                    filter.account_id.as_deref()
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(db_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(db_error)?;
        Ok(values)
    }
    Ok((
        distinct_values(conn, "model", req, filter)?,
        distinct_values(conn, "effort", req, filter)?,
        distinct_values(conn, "pricing_confidence", req, filter)?,
    ))
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
               AND (?4 IS NULL OR workspace_id = ?4)
               AND (?5 IS NULL OR attributed_account_id = ?5)
             GROUP BY model",
        )
        .map_err(db_error)?;
    let rows = stmt
        .query_map(
            params![
                req.include_archived,
                filter.from.as_deref(),
                filter.to.as_deref(),
                filter.workspace_id.as_deref(),
                filter.account_id.as_deref()
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

fn usage_rate_card() -> Vec<UsageRateCardEntry> {
    [
        ("gpt-5.5", "short <=128k", false, 5.0, 0.5, 30.0, None),
        ("gpt-5.5", "long >128k", false, 10.0, 1.0, 45.0, None),
        (
            "gpt-5.5-pro",
            "short <=128k",
            false,
            30.0,
            30.0,
            180.0,
            None,
        ),
        ("gpt-5.5-pro", "long >128k", false, 60.0, 60.0, 270.0, None),
        ("gpt-5.4", "short <=128k", false, 2.5, 0.25, 15.0, None),
        ("gpt-5.4", "long >128k", false, 5.0, 0.5, 22.5, None),
        ("gpt-5.4-mini", "all", false, 0.75, 0.075, 4.5, None),
        ("gpt-5.4-nano", "all", false, 0.2, 0.02, 1.25, None),
        (
            "gpt-5.4-pro",
            "short <=128k",
            false,
            30.0,
            30.0,
            180.0,
            None,
        ),
        ("gpt-5.4-pro", "long >128k", false, 60.0, 60.0, 270.0, None),
        ("gpt-5.3-codex", "all", false, 4.375, 0.4375, 35.0, None),
        ("gpt-5.2", "all", false, 4.375, 0.4375, 35.0, None),
        ("gpt-5", "all", false, 4.375, 0.4375, 35.0, None),
        (
            "codex-auto-review",
            "all",
            true,
            4.375,
            0.4375,
            35.0,
            Some("Estimated with gpt-5.3-codex rates".to_string()),
        ),
    ]
    .into_iter()
    .map(
        |(
            model,
            context_window,
            estimated,
            input_per_million,
            cached_input_per_million,
            output_per_million,
            notes,
        )| UsageRateCardEntry {
            model: model.to_string(),
            pricing_model: if model == "codex-auto-review" {
                "gpt-5.3-codex".to_string()
            } else {
                model.to_string()
            },
            context_window: context_window.to_string(),
            estimated,
            input_per_million,
            cached_input_per_million,
            output_per_million,
            notes,
        },
    )
    .collect()
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

fn usage_scopes(home_root: &Path) -> Result<Vec<UsageScope>> {
    let mut scopes = vec![UsageScope {
        id: "total".to_string(),
        label: "Total".to_string(),
        kind: "total".to_string(),
        account_id: None,
        is_default: true,
    }];
    scopes.extend(
        discover_usage_workspaces(home_root)?
            .into_iter()
            .map(|workspace| UsageScope {
                id: workspace.id,
                label: workspace.label,
                kind: "workspace".to_string(),
                account_id: workspace.account_id,
                is_default: false,
            }),
    );
    Ok(scopes)
}

fn discover_usage_workspaces(home_root: &Path) -> Result<Vec<UsageWorkspace>> {
    let mut seen = BTreeSet::new();
    let mut workspaces = Vec::new();
    for account in list_accounts(home_root)? {
        if !has_usage_workspace_signal(&account.codex_home) {
            continue;
        }
        let key = canonical_key(&account.codex_home);
        if !seen.insert(key) {
            continue;
        }
        workspaces.push(UsageWorkspace {
            id: format!("workspace:{}", account.id),
            label: account.display_name,
            home: account.codex_home,
            account_id: Some(account.id),
        });
    }
    workspaces.sort_by(|a, b| {
        let rank_a = if a.account_id.as_deref() == Some("main") {
            0
        } else {
            1
        };
        let rank_b = if b.account_id.as_deref() == Some("main") {
            0
        } else {
            1
        };
        rank_a.cmp(&rank_b).then_with(|| a.label.cmp(&b.label))
    });
    Ok(workspaces)
}

fn canonical_key(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn has_usage_workspace_signal(path: &Path) -> bool {
    path.join("sessions").exists()
        || path.join("archived_sessions").exists()
        || path.join("auth.json").exists()
        || path.join("config.toml").exists()
}

fn account_label_from_id(id: &str) -> String {
    if id == "main" {
        "main".to_string()
    } else {
        format!("codex-{id}")
    }
}

pub fn reset_usage_index(home_root: &Path) -> Result<()> {
    let _guard = REFRESH_LOCK
        .lock()
        .map_err(|_| AppError::new("USAGE_REFRESH_LOCK", "usage refresh lock is poisoned"))?;
    let usage_dir = crate::services::lam_paths::LamPaths::for_home(home_root).usage_root();
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

pub fn delete_usage_sources(home_root: &Path, source_paths: &[PathBuf]) -> Result<()> {
    if source_paths.is_empty() {
        return Ok(());
    }
    let _guard = REFRESH_LOCK
        .lock()
        .map_err(|_| AppError::new("USAGE_REFRESH_LOCK", "usage refresh lock is poisoned"))?;
    let db_path = usage_db_path(home_root);
    if !db_path.exists() {
        return Ok(());
    }
    let mut conn = open_usage_db(&db_path)?;
    init_usage_db(&conn)?;
    let tx = begin_usage_refresh_transaction(&mut conn)?;
    tx.execute_batch(
        "
        CREATE TEMP TABLE IF NOT EXISTS replaced_usage_sources (source_file TEXT PRIMARY KEY);
        CREATE TEMP TABLE IF NOT EXISTS changed_usage_records (record_id TEXT PRIMARY KEY);
        CREATE TEMP TABLE IF NOT EXISTS affected_usage_threads (
            workspace_id TEXT NOT NULL,
            account_id TEXT NOT NULL,
            logical_thread_key TEXT NOT NULL,
            PRIMARY KEY (workspace_id, account_id, logical_thread_key)
        );
        CREATE TEMP TABLE IF NOT EXISTS affected_usage_models (model_name TEXT PRIMARY KEY);
        CREATE TEMP TABLE IF NOT EXISTS affected_usage_records (record_id TEXT PRIMARY KEY);
        CREATE TEMP TABLE IF NOT EXISTS affected_usage_model_records (record_id TEXT PRIMARY KEY);
        DELETE FROM replaced_usage_sources;
        DELETE FROM changed_usage_records;
        DELETE FROM affected_usage_threads;
        DELETE FROM affected_usage_models;
        DELETE FROM affected_usage_records;
        DELETE FROM affected_usage_model_records;
        ",
    )
    .map_err(db_error)?;
    for path in source_paths {
        tx.execute(
            "INSERT OR IGNORE INTO replaced_usage_sources (source_file) VALUES (?1)",
            [path.to_string_lossy().to_string()],
        )
        .map_err(db_error)?;
    }
    collect_changed_usage_partitions(&tx)?;
    tx.execute(
        "DELETE FROM source_files WHERE source_file IN (SELECT source_file FROM replaced_usage_sources)",
        [],
    )
    .map_err(db_error)?;
    tx.execute(
        "DELETE FROM usage_events WHERE source_file IN (SELECT source_file FROM replaced_usage_sources)",
        [],
    )
    .map_err(db_error)?;
    let now = chrono::Utc::now().to_rfc3339();
    rebuild_usage_aggregates(&tx, &now)?;
    let count: i64 = tx
        .query_row("SELECT COUNT(*) FROM usage_events", [], |row| row.get(0))
        .map_err(db_error)?;
    set_meta_tx(&tx, "parsed_events", &count.to_string())?;
    tx.commit().map_err(db_error)
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

fn find_session_logs(
    workspaces: &[UsageWorkspace],
    include_archived: bool,
) -> Result<Vec<SourceLog>> {
    let mut paths = Vec::new();
    for workspace in workspaces {
        collect_jsonl(
            &workspace.home.join("sessions"),
            workspace,
            false,
            &mut paths,
        )?;
        if include_archived {
            collect_jsonl(
                &workspace.home.join("archived_sessions"),
                workspace,
                true,
                &mut paths,
            )?;
        }
    }
    paths.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(paths)
}

fn collect_jsonl(
    dir: &Path,
    workspace: &UsageWorkspace,
    is_archived: bool,
    paths: &mut Vec<SourceLog>,
) -> Result<()> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, workspace, is_archived, paths)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("jsonl") {
            paths.push(SourceLog {
                path,
                is_archived,
                workspace_id: workspace.id.clone(),
                workspace_label: workspace.label.clone(),
                workspace_home: workspace.home.clone(),
                account_id: workspace.account_id.clone(),
            });
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
    use crate::services::account::{execute_create_account, CreateAccountRequest};
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
            scope_id: None,
            account_id: None,
        }
    }

    fn dashboard_request(preset: &str) -> UsageDashboardRequest {
        UsageDashboardRequest {
            window: UsageWindow {
                preset: preset.to_string(),
                from: None,
                to: None,
            },
            include_archived: false,
            scope_id: None,
            account_id: None,
            search: None,
            model: None,
            effort: None,
            pricing_confidence: None,
            sort_key: Some("time".to_string()),
            sort_direction: Some("desc".to_string()),
            limit: None,
            offset: None,
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

    fn install_mutation_audit(conn: &Connection) {
        conn.execute_batch(
            "
            CREATE TABLE mutation_audit (
                table_name TEXT NOT NULL,
                action TEXT NOT NULL,
                identity TEXT NOT NULL
            );
            CREATE TRIGGER audit_usage_events_insert AFTER INSERT ON usage_events BEGIN
                INSERT INTO mutation_audit VALUES (
                    'usage_events', 'insert',
                    COALESCE(NEW.workspace_id, '') || '|' ||
                        COALESCE(NEW.attributed_account_id, '') || '|' ||
                        COALESCE(NEW.thread_name, NEW.session_id) || '|' ||
                        COALESCE(NEW.model, 'unknown')
                );
            END;
            CREATE TRIGGER audit_usage_events_update AFTER UPDATE ON usage_events BEGIN
                INSERT INTO mutation_audit VALUES (
                    'usage_events', 'update',
                    COALESCE(NEW.workspace_id, '') || '|' ||
                        COALESCE(NEW.attributed_account_id, '') || '|' ||
                        COALESCE(NEW.thread_name, NEW.session_id) || '|' ||
                        COALESCE(NEW.model, 'unknown')
                );
            END;
            CREATE TRIGGER audit_usage_events_delete AFTER DELETE ON usage_events BEGIN
                INSERT INTO mutation_audit VALUES (
                    'usage_events', 'delete',
                    COALESCE(OLD.workspace_id, '') || '|' ||
                        COALESCE(OLD.attributed_account_id, '') || '|' ||
                        COALESCE(OLD.thread_name, OLD.session_id) || '|' ||
                        COALESCE(OLD.model, 'unknown')
                );
            END;
            CREATE TRIGGER audit_thread_summaries_insert AFTER INSERT ON thread_summaries BEGIN
                INSERT INTO mutation_audit VALUES (
                    'thread_summaries', 'insert', NEW.thread_key
                );
            END;
            CREATE TRIGGER audit_thread_summaries_update AFTER UPDATE ON thread_summaries BEGIN
                INSERT INTO mutation_audit VALUES (
                    'thread_summaries', 'update', NEW.thread_key
                );
            END;
            CREATE TRIGGER audit_thread_summaries_delete AFTER DELETE ON thread_summaries BEGIN
                INSERT INTO mutation_audit VALUES (
                    'thread_summaries', 'delete', OLD.thread_key
                );
            END;
            CREATE TRIGGER audit_aggregate_facts_insert
            AFTER INSERT ON aggregate_diagnostic_facts BEGIN
                INSERT INTO mutation_audit VALUES (
                    'aggregate_diagnostic_facts', 'insert', NEW.record_id
                );
            END;
            CREATE TRIGGER audit_aggregate_facts_update
            AFTER UPDATE ON aggregate_diagnostic_facts BEGIN
                INSERT INTO mutation_audit VALUES (
                    'aggregate_diagnostic_facts', 'update', NEW.record_id
                );
            END;
            CREATE TRIGGER audit_aggregate_facts_delete
            AFTER DELETE ON aggregate_diagnostic_facts BEGIN
                INSERT INTO mutation_audit VALUES (
                    'aggregate_diagnostic_facts', 'delete', OLD.record_id
                );
            END;
            ",
        )
        .unwrap();
    }

    fn mutation_audit(conn: &Connection) -> Vec<(String, String, String)> {
        let mut stmt = conn
            .prepare(
                "SELECT table_name, action, identity
                 FROM mutation_audit
                 ORDER BY rowid",
            )
            .unwrap();
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap()
    }

    fn write_session_index(home: &Path, sessions: &[(&str, &str)]) {
        fs::create_dir_all(home.join(".codex")).unwrap();
        let body = sessions
            .iter()
            .map(|(id, thread)| json!({"id": id, "thread_name": thread}).to_string())
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(home.join(".codex/session_index.jsonl"), format!("{body}\n")).unwrap();
    }

    fn append_usage_event(path: &Path, timestamp: &str, cumulative_input: i64) {
        let mut body = fs::read_to_string(path).unwrap();
        body.push_str(
            &json!({
                "type": "event_msg",
                "timestamp": timestamp,
                "payload": {
                    "type": "token_count",
                    "info": {
                        "last_token_usage": {
                            "input_tokens": 20,
                            "cached_input_tokens": 5,
                            "output_tokens": 4,
                            "reasoning_output_tokens": 1,
                            "total_tokens": 24
                        },
                        "total_token_usage": {
                            "input_tokens": cumulative_input,
                            "cached_input_tokens": 10,
                            "output_tokens": 14,
                            "reasoning_output_tokens": 4,
                            "total_tokens": cumulative_input + 14
                        }
                    }
                }
            })
            .to_string(),
        );
        body.push('\n');
        fs::write(path, body).unwrap();
    }

    #[test]
    fn affected_partition_queries_use_targeted_indexes() {
        let temp = TempDir::new().unwrap();
        let db_path = usage_db_path(temp.path());
        prepare_usage_dir(&db_path).unwrap();
        let conn = open_usage_db(&db_path).unwrap();
        init_usage_db(&conn).unwrap();
        conn.execute_batch(
            "
            CREATE TEMP TABLE affected_usage_threads (
                workspace_id TEXT NOT NULL,
                account_id TEXT NOT NULL,
                logical_thread_key TEXT NOT NULL,
                PRIMARY KEY (workspace_id, account_id, logical_thread_key)
            );
            CREATE TEMP TABLE affected_usage_models (model_name TEXT PRIMARY KEY);
            CREATE TEMP TABLE affected_usage_records (record_id TEXT PRIMARY KEY);
            CREATE TEMP TABLE affected_usage_model_records (record_id TEXT PRIMARY KEY);
            CREATE TEMP TABLE refresh_workspace_metadata (
                source_file TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL,
                workspace_label TEXT NOT NULL,
                workspace_home TEXT NOT NULL,
                account_id TEXT,
                account_label TEXT,
                attribution_source TEXT NOT NULL
            );
            CREATE TEMP TABLE mismatched_workspace_metadata (
                record_id TEXT PRIMARY KEY,
                source_file TEXT NOT NULL,
                old_workspace_id TEXT,
                old_account_id TEXT,
                old_thread_key TEXT NOT NULL,
                old_model TEXT,
                workspace_id TEXT NOT NULL,
                workspace_label TEXT NOT NULL,
                workspace_home TEXT NOT NULL,
                account_id TEXT,
                account_label TEXT,
                attribution_source TEXT NOT NULL
            );
            ",
        )
        .unwrap();

        for (sql, indexes) in [
            (
                COLLECT_AFFECTED_USAGE_RECORDS_SQL,
                &[
                    "idx_usage_events_named_partition",
                    "idx_usage_events_session_partition",
                ][..],
            ),
            (
                COLLECT_AFFECTED_MODEL_RECORDS_SQL,
                &["idx_usage_events_unknown_model"][..],
            ),
            (
                COLLECT_MISMATCHED_WORKSPACE_METADATA_SQL,
                &["idx_usage_events_source"][..],
            ),
        ] {
            let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
            let plan = stmt
                .query_map([], |row| row.get::<_, String>(3))
                .unwrap()
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap();
            for index in indexes {
                assert!(plan.iter().any(|line| line.contains(index)), "{plan:?}");
            }
            assert!(
                plan.iter().all(|line| !line.contains("SCAN events")),
                "{plan:?}"
            );
        }
    }

    #[test]
    fn refresh_transaction_reserves_writer_before_reading() {
        let temp = TempDir::new().unwrap();
        let db_path = usage_db_path(temp.path());
        prepare_usage_dir(&db_path).unwrap();
        let mut refresh_conn = open_usage_db(&db_path).unwrap();
        init_usage_db(&refresh_conn).unwrap();
        let competing_conn = open_usage_db(&db_path).unwrap();
        competing_conn
            .busy_timeout(std::time::Duration::ZERO)
            .unwrap();

        let tx = begin_usage_refresh_transaction(&mut refresh_conn).unwrap();
        tx.query_row("SELECT COUNT(*) FROM usage_events", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap();
        let err = competing_conn
            .execute(
                "INSERT INTO refresh_meta (key, value) VALUES ('competing', 'write')",
                [],
            )
            .unwrap_err();
        match err {
            rusqlite::Error::SqliteFailure(error, _) => {
                assert_eq!(error.code, rusqlite::ErrorCode::DatabaseBusy)
            }
            other => panic!("unexpected error: {other}"),
        }
        tx.rollback().unwrap();
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
    fn ensure_column_tolerates_duplicate_column_migration_race() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE example (workspace_id TEXT)", [])
            .unwrap();

        let result = ensure_column(
            &conn,
            "example",
            "workspace_id_race_marker",
            "ALTER TABLE example ADD COLUMN workspace_id TEXT",
        );

        assert!(result.is_ok());
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
        assert!(!table_columns(&conn, "account_usage_snapshot").is_empty());
        assert!(!table_columns(&conn, "account_usage_daily_buckets").is_empty());
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
    fn usage_calls_section_applies_filter_and_limit_in_query() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".codex")).unwrap();
        write_log_at(
            temp.path(),
            "sessions",
            "rollout-keep-a",
            &fixture_at("keep-a", "2026-06-28T00:00:02Z", 100, 50, "gpt-5.4"),
        );
        write_log_at(
            temp.path(),
            "sessions",
            "rollout-keep-b",
            &fixture_at("keep-b", "2026-06-28T00:00:03Z", 100, 50, "gpt-5.4"),
        );
        write_log_at(
            temp.path(),
            "sessions",
            "rollout-skip",
            &fixture_at("skip-c", "2026-06-28T00:00:04Z", 100, 50, "gpt-5"),
        );
        refresh_usage_index(temp.path()).unwrap();

        let mut req = dashboard_request("all");
        req.search = Some("keep".to_string());
        req.model = Some("gpt-5.4".to_string());
        req.limit = Some(1);
        let calls = get_usage_calls(temp.path(), req).unwrap();

        assert_eq!(calls.total, 2);
        assert_eq!(calls.rows.len(), 1);
        assert_eq!(calls.next_offset, Some(1));
        assert_eq!(calls.rows[0].model.as_deref(), Some("gpt-5.4"));
        assert!(calls.rows[0].session_id.contains("keep"));
    }

    #[test]
    fn usage_insights_and_paged_sections_use_full_filtered_data() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join(".codex")).unwrap();
        for index in 0..3 {
            let session_id = format!("session-{index}");
            let body = format!(
                "{}\n{}\n{}\n",
                json!({"type":"session_meta","timestamp":"2026-06-28T00:00:00Z","payload":{"id":session_id,"thread_name":format!("thread-{index}")}}),
                json!({"type":"turn_context","timestamp":"2026-06-28T00:00:01Z","payload":{"turn_id":"turn-1","cwd":"/repo/LAM","model":"gpt-5","effort":if index == 0 { "low" } else { "medium" },"current_date":"2026-06-28","timezone":"Asia/Shanghai"}}),
                json!({"type":"event_msg","timestamp":format!("2026-06-28T00:00:0{}Z", index + 2),"payload":{"type":"token_count","subagent_type":if index < 2 { "review" } else { "planner" },"info":{"last_token_usage":{"input_tokens":100 + index,"cached_input_tokens":0,"output_tokens":10,"reasoning_output_tokens":3,"total_tokens":110 + index},"total_token_usage":{"input_tokens":100 + index,"cached_input_tokens":0,"output_tokens":10,"reasoning_output_tokens":3,"total_tokens":110 + index}}}})
            );
            write_log_at(
                temp.path(),
                "sessions",
                &format!("rollout-page-{index}"),
                &body,
            );
        }
        refresh_usage_index(temp.path()).unwrap();

        let mut req = dashboard_request("all");
        req.limit = Some(2);
        let calls = get_usage_calls(temp.path(), req.clone()).unwrap();
        assert_eq!(calls.total, 3);
        assert_eq!(calls.rows.len(), 2);
        assert_eq!(calls.next_offset, Some(2));

        req.offset = Some(2);
        let second_page = get_usage_calls(temp.path(), req.clone()).unwrap();
        assert_eq!(second_page.total, 3);
        assert_eq!(second_page.rows.len(), 1);
        assert_eq!(second_page.next_offset, None);

        let threads = get_usage_threads(temp.path(), req.clone()).unwrap();
        assert_eq!(threads.total, 3);
        assert_eq!(threads.rows.len(), 1);

        let insights = get_usage_insights(temp.path(), dashboard_request("all")).unwrap();
        assert_eq!(insights.total_threads, 3);
        assert_eq!(insights.skills_explored, 2);
        assert_eq!(insights.total_skills_used, 3);
        assert_eq!(insights.most_used_reasoning.as_deref(), Some("Medium"));
        assert_eq!(insights.most_used_reasoning_percent, Some(2.0 / 3.0));
        assert_eq!(insights.fast_mode_percent, Some(1.0 / 3.0));
    }

    #[test]
    fn usage_workspace_response_aggregates_and_filters_workspaces() {
        let temp = TempDir::new().unwrap();
        write_log(temp.path(), &fixture(100, 50));

        let c_path = temp.path().join(".codex-c/sessions/2026/06/28/c.jsonl");
        fs::create_dir_all(c_path.parent().unwrap()).unwrap();
        fs::write(
            &c_path,
            fixture_at(
                "00000000-0000-0000-0000-0000000000c0",
                "2026-06-28T00:00:03Z",
                200,
                70,
                "gpt-5",
            ),
        )
        .unwrap();

        refresh_usage_index(temp.path()).unwrap();
        let response = get_usage_dashboard_response(
            temp.path(),
            UsageDashboardRequest {
                window: UsageWindow::default(),
                include_archived: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            response
                .scopes
                .iter()
                .map(|scope| scope.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Total", "main", "codex-c"]
        );
        assert_eq!(response.active_scope_id, "total");
        assert_eq!(response.dashboard.summary.total_calls, 2);

        let c_response = get_usage_dashboard_response(
            temp.path(),
            UsageDashboardRequest {
                scope_id: Some("workspace:c".to_string()),
                window: UsageWindow::default(),
                include_archived: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(c_response.dashboard.summary.total_calls, 1);
        assert_eq!(c_response.dashboard.summary.input_tokens, 70);
        assert_eq!(
            c_response.dashboard.summary.recent_calls[0]
                .workspace_label
                .as_deref(),
            Some("codex-c")
        );
    }

    #[test]
    fn scoped_headline_stats_apply_workspace_filter() {
        let temp = TempDir::new().unwrap();
        write_log(
            temp.path(),
            &fixture_at(
                "00000000-0000-0000-0000-000000000001",
                "2026-06-28T00:00:02Z",
                100,
                50,
                "gpt-5",
            ),
        );

        let c_path = temp.path().join(".codex-c/sessions/2026/06/28/c.jsonl");
        fs::create_dir_all(c_path.parent().unwrap()).unwrap();
        fs::write(
            &c_path,
            format!(
                "{}\n{}\n{}\n{}\n",
                json!({"type":"session_meta","timestamp":"2026-06-28T00:00:00Z","payload":{"id":"00000000-0000-0000-0000-0000000000c0"}}),
                json!({"type":"turn_context","timestamp":"2026-06-28T00:00:01Z","payload":{"turn_id":"long-turn","cwd":"/repo/LAM","model":"gpt-5","effort":"medium","current_date":"2026-06-28"}}),
                json!({"type":"event_msg","timestamp":"2026-06-28T00:00:02Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":1000,"cached_input_tokens":0,"output_tokens":10,"reasoning_output_tokens":0,"total_tokens":1010},"total_token_usage":{"input_tokens":1000,"cached_input_tokens":0,"output_tokens":10,"reasoning_output_tokens":0,"total_tokens":1010}}}}),
                json!({"type":"event_msg","timestamp":"2026-06-28T00:02:02Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":2000,"cached_input_tokens":0,"output_tokens":10,"reasoning_output_tokens":0,"total_tokens":2010},"total_token_usage":{"input_tokens":3000,"cached_input_tokens":0,"output_tokens":20,"reasoning_output_tokens":0,"total_tokens":3020}}}})
            ),
        )
        .unwrap();

        refresh_usage_index(temp.path()).unwrap();
        let mut req = summary_request("all");
        req.scope_id = Some("workspace:main".to_string());
        let summary = get_usage_summary(temp.path(), req).unwrap();

        assert_eq!(summary.total_tokens, 60);
        assert_eq!(summary.headline_stats.peak_daily_tokens, Some(60));
        assert_eq!(summary.headline_stats.longest_running_turn_sec, Some(0));
    }

    #[test]
    fn refresh_is_idempotent_for_unchanged_source() {
        let temp = TempDir::new().unwrap();
        write_log(temp.path(), &fixture(100, 50));
        assert_eq!(refresh_usage_index(temp.path()).unwrap().parsed_files, 1);
        let conn = open_usage_db(&usage_db_path(temp.path())).unwrap();
        install_mutation_audit(&conn);
        drop(conn);

        assert_eq!(refresh_usage_index(temp.path()).unwrap().parsed_files, 0);
        assert_eq!(
            get_usage_summary(temp.path(), summary_request("all"))
                .unwrap()
                .total_calls,
            1
        );
        let conn = open_usage_db(&usage_db_path(temp.path())).unwrap();
        assert!(mutation_audit(&conn).is_empty());
    }

    #[test]
    fn append_refresh_mutates_only_affected_thread_and_model() {
        let temp = TempDir::new().unwrap();
        let session_a = "00000000-0000-0000-0000-00000000000a";
        let session_b = "00000000-0000-0000-0000-00000000000b";
        write_session_index(
            temp.path(),
            &[(session_a, "thread-a"), (session_b, "thread-b")],
        );
        let path_a = write_log_at(
            temp.path(),
            "sessions",
            &format!("source-a-{session_a}"),
            &fixture_at(
                session_a,
                "2026-06-28T00:00:02Z",
                100,
                50,
                "unknown-model-a",
            ),
        );
        write_log_at(
            temp.path(),
            "sessions",
            &format!("source-b-{session_b}"),
            &fixture_at(
                session_b,
                "2026-06-28T00:00:03Z",
                200,
                60,
                "unknown-model-b",
            ),
        );
        refresh_usage_index(temp.path()).unwrap();
        let conn = open_usage_db(&usage_db_path(temp.path())).unwrap();
        install_mutation_audit(&conn);
        drop(conn);

        append_usage_event(&path_a, "2026-06-28T00:00:04Z", 120);
        refresh_usage_index(temp.path()).unwrap();

        let conn = open_usage_db(&usage_db_path(temp.path())).unwrap();
        let audit = mutation_audit(&conn);
        assert!(!audit.is_empty());
        assert!(
            audit
                .iter()
                .all(|(_, _, identity)| !identity.contains("thread-b")
                    && !identity.contains("unknown-model-b")),
            "{audit:?}"
        );
    }

    #[test]
    fn refresh_usage_index_does_not_sync_account_usage_snapshot() {
        let temp = TempDir::new().unwrap();
        write_log(temp.path(), &fixture(100, 50));

        let result = refresh_usage_index(temp.path()).unwrap();

        assert!(!result
            .parser_diagnostics
            .contains_key("account_usage_unavailable"));
    }

    #[test]
    fn refresh_backfills_workspace_metadata_for_unchanged_sources() {
        let temp = TempDir::new().unwrap();
        write_session_index(
            temp.path(),
            &[("00000000-0000-0000-0000-000000000001", "metadata-thread")],
        );
        write_log(temp.path(), &fixture(100, 50));
        refresh_usage_index(temp.path()).unwrap();

        let conn = open_usage_db(&usage_db_path(temp.path())).unwrap();
        conn.execute(
            "UPDATE usage_events
             SET workspace_id = 'workspace:old',
                 workspace_label = 'old',
                 workspace_home = NULL,
                 attributed_account_id = NULL,
                 attributed_account_label = NULL,
                 attribution_source = NULL",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE source_files
             SET workspace_id = 'workspace:old', workspace_home = NULL",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE thread_summaries
             SET thread_key = 'workspace:old||metadata-thread',
                 workspace_id = 'workspace:old',
                 workspace_label = 'old'",
            [],
        )
        .unwrap();
        drop(conn);

        let result = refresh_usage_index(temp.path()).unwrap();
        assert_eq!(result.parsed_files, 0);
        assert_eq!(
            get_usage_summary(temp.path(), summary_request("all"))
                .unwrap()
                .total_calls,
            1
        );

        let mut req = summary_request("all");
        req.scope_id = Some("workspace:main".to_string());
        let summary = get_usage_summary(temp.path(), req).unwrap();
        assert_eq!(summary.total_calls, 1);
        assert_eq!(
            summary.recent_calls[0].workspace_label.as_deref(),
            Some("main")
        );
        let conn = open_usage_db(&usage_db_path(temp.path())).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM thread_summaries
                 WHERE thread_key = 'workspace:old||metadata-thread'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM thread_summaries
                 WHERE thread_key = 'workspace:main|main|metadata-thread'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
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
    fn thread_summaries_are_scoped_by_workspace() {
        let temp = TempDir::new().unwrap();
        let body = fixture_at("shared-session", "2026-06-28T00:00:00Z", 100, 50, "gpt-5");
        write_log(temp.path(), &body);
        execute_create_account(
            temp.path(),
            &CreateAccountRequest {
                name: "luna002".to_string(),
                copy_config_from: None,
                overwrite_wrapper: false,
            },
        )
        .unwrap();
        let other_path = temp.path().join(
            ".codex-luna002/sessions/2026/06/28/rollout-test-2026-06-28T00-00-00-00000000-0000-0000-0000-000000000001.jsonl",
        );
        fs::create_dir_all(other_path.parent().unwrap()).unwrap();
        fs::write(&other_path, body).unwrap();

        refresh_usage_index(temp.path()).unwrap();

        let mut main_req = dashboard_request("all");
        main_req.scope_id = Some("workspace:main".to_string());
        let main_threads = get_usage_threads(temp.path(), main_req).unwrap();
        assert_eq!(main_threads.total, 1);

        let mut other_req = dashboard_request("all");
        other_req.scope_id = Some("workspace:luna002".to_string());
        let other_threads = get_usage_threads(temp.path(), other_req).unwrap();
        assert_eq!(other_threads.total, 1);
    }

    #[test]
    fn append_relinks_complete_thread_across_source_files() {
        let temp = TempDir::new().unwrap();
        let session_a = "00000000-0000-0000-0000-00000000000a";
        let session_b = "00000000-0000-0000-0000-00000000000b";
        write_session_index(
            temp.path(),
            &[(session_a, "shared-thread"), (session_b, "shared-thread")],
        );
        let path_a = write_log_at(
            temp.path(),
            "sessions",
            &format!("source-a-{session_a}"),
            &fixture_at(session_a, "2026-06-28T00:00:02Z", 100, 50, "gpt-5"),
        );
        write_log_at(
            temp.path(),
            "sessions",
            &format!("source-b-{session_b}"),
            &fixture_at(session_b, "2026-06-28T00:00:03Z", 200, 60, "gpt-5"),
        );
        refresh_usage_index(temp.path()).unwrap();

        append_usage_event(&path_a, "2026-06-28T00:00:04Z", 120);
        refresh_usage_index(temp.path()).unwrap();

        let conn = open_usage_db(&usage_db_path(temp.path())).unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT record_id, thread_call_index, previous_record_id, next_record_id
                 FROM usage_events
                 WHERE thread_name = 'shared-thread'
                 ORDER BY event_timestamp, record_id",
            )
            .unwrap();
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            })
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].1, 0);
        assert_eq!(rows[0].2, None);
        assert_eq!(rows[0].3.as_deref(), Some(rows[1].0.as_str()));
        assert_eq!(rows[1].1, 1);
        assert_eq!(rows[1].2.as_deref(), Some(rows[0].0.as_str()));
        assert_eq!(rows[1].3.as_deref(), Some(rows[2].0.as_str()));
        assert_eq!(rows[2].1, 2);
        assert_eq!(rows[2].2.as_deref(), Some(rows[1].0.as_str()));
        assert_eq!(rows[2].3, None);
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
        let session_id = "00000000-0000-0000-0000-000000000001";
        write_session_index(temp.path(), &[(session_id, "old-thread")]);
        let path = write_log(
            temp.path(),
            &fixture_at(
                session_id,
                "2026-06-28T00:00:02Z",
                100,
                50,
                "unknown-old-model",
            ),
        );
        refresh_usage_index(temp.path()).unwrap();
        write_session_index(temp.path(), &[(session_id, "new-thread")]);
        fs::write(
            path,
            fixture_at(
                session_id,
                "2026-06-28T00:00:03Z",
                8,
                8,
                "unknown-new-model",
            ),
        )
        .unwrap();
        refresh_usage_index(temp.path()).unwrap();
        let summary = get_usage_summary(temp.path(), summary_request("all")).unwrap();
        assert_eq!(summary.total_calls, 1);
        assert_eq!(summary.recent_calls[0].input_tokens, 8);
        let conn = open_usage_db(&usage_db_path(temp.path())).unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM thread_summaries
                 WHERE thread_key LIKE '%|old-thread'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM thread_summaries
                 WHERE thread_key LIKE '%|new-thread'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM aggregate_diagnostic_facts
                 WHERE record_id = 'unknown-model:unknown-old-model'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM aggregate_diagnostic_facts
                 WHERE record_id = 'unknown-model:unknown-new-model'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            1
        );
    }

    #[test]
    fn refresh_failure_rolls_back_source_event_and_derived_changes() {
        let temp = TempDir::new().unwrap();
        let path = write_log(
            temp.path(),
            &fixture_at(
                "00000000-0000-0000-0000-000000000001",
                "2026-06-28T00:00:02Z",
                100,
                50,
                "unknown-rollback-model",
            ),
        );
        refresh_usage_index(temp.path()).unwrap();
        let conn = open_usage_db(&usage_db_path(temp.path())).unwrap();
        let before = conn
            .query_row(
                "SELECT
                    (SELECT parsed_until_byte FROM source_files),
                    (SELECT COUNT(*) FROM usage_events),
                    (SELECT call_count FROM thread_summaries),
                    (SELECT event_count FROM aggregate_diagnostic_facts
                     WHERE record_id = 'unknown-model:unknown-rollback-model')",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .unwrap();
        conn.execute_batch(
            "
            CREATE TRIGGER force_thread_summary_failure
            BEFORE INSERT ON thread_summaries BEGIN
                SELECT RAISE(ABORT, 'forced thread summary failure');
            END;
            ",
        )
        .unwrap();
        drop(conn);

        append_usage_event(&path, "2026-06-28T00:00:03Z", 120);
        assert!(refresh_usage_index(temp.path()).is_err());

        let conn = open_usage_db(&usage_db_path(temp.path())).unwrap();
        let after = conn
            .query_row(
                "SELECT
                    (SELECT parsed_until_byte FROM source_files),
                    (SELECT COUNT(*) FROM usage_events),
                    (SELECT call_count FROM thread_summaries),
                    (SELECT event_count FROM aggregate_diagnostic_facts
                     WHERE record_id = 'unknown-model:unknown-rollback-model')",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(after, before);
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
            crate::services::lam_paths::LamPaths::for_home(temp.path()).usage_db_path()
        );
        assert!(usage_db_path(temp.path()).exists());
        // The usage database must never live under the Codex home directory,
        // so Codex session scanning cannot mistake it for conversation data.
        let codex_root = temp.path().join(".codex");
        if codex_root.exists() {
            for entry in fs::read_dir(&codex_root).unwrap() {
                let path = entry.unwrap().path();
                assert_ne!(
                    path.file_name().and_then(|v| v.to_str()),
                    Some("usage.sqlite3")
                );
            }
        }
        let lam_usage = crate::services::lam_paths::LamPaths::for_home(temp.path()).usage_root();
        assert_eq!(
            fs::read_dir(&lam_usage).unwrap().count(),
            1,
            "usage root contains exactly the sqlite database"
        );
        assert!(lam_usage.join("usage.sqlite3").exists());
    }

    #[test]
    fn discovery_ignores_lam_usage_directory() {
        let temp = TempDir::new().unwrap();
        let lam_path = crate::services::lam_paths::LamPaths::for_home(temp.path())
            .usage_root()
            .join("fake.jsonl");
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
        let conn = open_usage_db(&usage_db_path(temp.path())).unwrap();
        install_mutation_audit(&conn);
        drop(conn);

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
        let conn = open_usage_db(&usage_db_path(temp.path())).unwrap();
        let audit = mutation_audit(&conn);
        assert!(
            audit
                .iter()
                .all(|(_, _, identity)| !identity.contains("00000000-0000-0000-0000-000000000001")),
            "{audit:?}"
        );
        drop(conn);
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
                    scope_id: None,
                    account_id: None,
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

        assert!(!crate::services::lam_paths::LamPaths::for_home(temp.path())
            .usage_root()
            .exists());
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
    fn usage_rate_card_exposes_backend_builtin_prices() {
        let rate_card = get_usage_rate_card();

        assert!(rate_card.iter().any(|entry| {
            entry.model == "gpt-5"
                && entry.context_window == "all"
                && entry.input_per_million == 4.375
                && entry.output_per_million == 35.0
        }));
        assert!(rate_card.iter().any(|entry| {
            entry.model == "codex-auto-review"
                && entry.pricing_model == "gpt-5.3-codex"
                && entry.estimated
        }));
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
    fn parser_accepts_positive_last_usage_after_cumulative_reset() {
        let temp = TempDir::new().unwrap();
        write_log(
            temp.path(),
            &format!(
                "{}\n{}\n{}\n{}\n",
                json!({"type":"session_meta","timestamp":"2026-06-28T00:00:00Z","payload":{"id":"00000000-0000-0000-0000-000000000001"}}),
                json!({"type":"turn_context","timestamp":"2026-06-28T00:00:01Z","payload":{"turn_id":"turn-1","cwd":"/repo/LAM","model":"gpt-5","current_date":"2026-06-28"}}),
                json!({"type":"event_msg","timestamp":"2026-06-28T00:00:02Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":100,"cached_input_tokens":10,"output_tokens":20,"reasoning_output_tokens":5,"total_tokens":120},"total_token_usage":{"input_tokens":1000,"cached_input_tokens":100,"output_tokens":20,"reasoning_output_tokens":5,"total_tokens":1020}}}}),
                json!({"type":"event_msg","timestamp":"2026-06-28T00:00:03Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":50,"cached_input_tokens":5,"output_tokens":10,"reasoning_output_tokens":2,"total_tokens":60},"total_token_usage":{"input_tokens":500,"cached_input_tokens":50,"output_tokens":10,"reasoning_output_tokens":2,"total_tokens":510}}}})
            ),
        );
        refresh_usage_index(temp.path()).unwrap();
        let summary = get_usage_summary(temp.path(), summary_request("all")).unwrap();
        assert_eq!(summary.total_calls, 2);
        assert_eq!(summary.total_tokens, 180);
        assert_eq!(
            summary
                .diagnostics
                .parser_diagnostics
                .get("cumulative_reset_accepted"),
            Some(&1)
        );
    }

    #[test]
    fn local_headline_and_activity_buckets_follow_local_usage() {
        let temp = TempDir::new().unwrap();
        write_log(
            temp.path(),
            &format!(
                "{}\n{}\n{}\n{}\n{}\n{}\n{}\n",
                json!({"type":"session_meta","timestamp":"2026-06-28T00:00:00Z","payload":{"id":"00000000-0000-0000-0000-000000000001"}}),
                json!({"type":"turn_context","timestamp":"2026-06-26T00:00:01Z","payload":{"turn_id":"turn-1","cwd":"/repo/LAM","model":"gpt-5","current_date":"2026-06-26"}}),
                json!({"type":"event_msg","timestamp":"2026-06-26T00:00:02Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":30,"cached_input_tokens":0,"output_tokens":10,"reasoning_output_tokens":0,"total_tokens":40},"total_token_usage":{"input_tokens":30,"cached_input_tokens":0,"output_tokens":10,"reasoning_output_tokens":0,"total_tokens":40}}}}),
                json!({"type":"turn_context","timestamp":"2026-06-28T00:00:03Z","payload":{"turn_id":"turn-2","cwd":"/repo/LAM","model":"gpt-5","current_date":"2026-06-28"}}),
                json!({"type":"event_msg","timestamp":"2026-06-28T00:00:04Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":40,"cached_input_tokens":0,"output_tokens":10,"reasoning_output_tokens":0,"total_tokens":50},"total_token_usage":{"input_tokens":70,"cached_input_tokens":0,"output_tokens":20,"reasoning_output_tokens":0,"total_tokens":90}}}}),
                json!({"type":"event_msg","timestamp":"2026-06-28T00:01:04Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":10,"cached_input_tokens":0,"output_tokens":10,"reasoning_output_tokens":0,"total_tokens":20},"total_token_usage":{"input_tokens":80,"cached_input_tokens":0,"output_tokens":30,"reasoning_output_tokens":0,"total_tokens":110}}}}),
                json!({"type":"event_msg","timestamp":"2026-06-28T00:02:04Z","payload":{"type":"agent_message","message":"done"}})
            ),
        );
        refresh_usage_index(temp.path()).unwrap();
        let summary = get_usage_summary(temp.path(), summary_request("all")).unwrap();
        assert_eq!(summary.headline_stats.source, "local_sqlite");
        assert_eq!(summary.headline_stats.lifetime_tokens, Some(110));
        assert_eq!(summary.headline_stats.peak_daily_tokens, Some(70));
        assert_eq!(summary.headline_stats.current_streak_days, Some(1));
        assert_eq!(summary.headline_stats.longest_streak_days, Some(1));
        assert_eq!(summary.activity_buckets.len(), 3);
        assert_eq!(summary.activity_buckets[1].date, "2026-06-27");
        assert_eq!(summary.activity_buckets[1].calls, 0);
        assert_eq!(summary.activity_buckets[2].cumulative_tokens, 110);
    }

    #[test]
    fn usage_activity_uses_event_timestamp_for_calendar_days() {
        let temp = TempDir::new().unwrap();
        write_log(
            temp.path(),
            &format!(
                "{}\n{}\n{}\n{}\n{}\n",
                json!({"type":"session_meta","timestamp":"2026-06-30T00:00:00Z","payload":{"id":"00000000-0000-0000-0000-000000000001"}}),
                json!({"type":"turn_context","timestamp":"2026-06-30T00:00:01Z","payload":{"turn_id":"turn-1","cwd":"/repo/LAM","model":"gpt-5","current_date":"2026-07-06"}}),
                json!({"type":"event_msg","timestamp":"2026-06-30T00:00:02Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":30,"cached_input_tokens":0,"output_tokens":10,"reasoning_output_tokens":0,"total_tokens":40},"total_token_usage":{"input_tokens":30,"cached_input_tokens":0,"output_tokens":10,"reasoning_output_tokens":0,"total_tokens":40}}}}),
                json!({"type":"turn_context","timestamp":"2026-07-06T00:00:01Z","payload":{"turn_id":"turn-2","cwd":"/repo/LAM","model":"gpt-5","current_date":"2026-07-06"}}),
                json!({"type":"event_msg","timestamp":"2026-07-06T00:00:02Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":50,"cached_input_tokens":0,"output_tokens":10,"reasoning_output_tokens":0,"total_tokens":60},"total_token_usage":{"input_tokens":80,"cached_input_tokens":0,"output_tokens":20,"reasoning_output_tokens":0,"total_tokens":100}}}})
            ),
        );
        refresh_usage_index(temp.path()).unwrap();
        let summary = get_usage_summary(temp.path(), summary_request("all")).unwrap();

        assert_eq!(summary.headline_stats.peak_daily_tokens, Some(60));
        assert_eq!(summary.activity_buckets.first().unwrap().date, "2026-06-30");
        assert_eq!(summary.activity_buckets.last().unwrap().date, "2026-07-06");
        assert_eq!(summary.activity_buckets.first().unwrap().tokens, 40);
        assert_eq!(summary.activity_buckets.last().unwrap().tokens, 60);
    }

    #[test]
    fn account_usage_snapshot_overrides_headlines_and_keeps_delta() {
        let temp = TempDir::new().unwrap();
        write_log(temp.path(), &fixture(100, 50));
        refresh_usage_index(temp.path()).unwrap();
        let conn = open_usage_db(&usage_db_path(temp.path())).unwrap();
        store_account_usage_snapshot(
            &conn,
            &AccountUsageSnapshot {
                lifetime_tokens: Some(5_900_000_000),
                peak_daily_tokens: Some(193_000_000),
                longest_running_turn_sec: Some(8_880),
                current_streak_days: Some(8),
                longest_streak_days: Some(19),
                daily_buckets: vec![("2026-06-28".to_string(), 12345)],
            },
        )
        .unwrap();
        let summary = get_usage_summary(temp.path(), summary_request("all")).unwrap();
        assert_eq!(summary.headline_stats.source, "codex_account_usage");
        assert_eq!(summary.headline_stats.lifetime_tokens, Some(5_900_000_000));
        assert_eq!(summary.headline_stats.peak_daily_tokens, Some(193_000_000));
        assert_eq!(summary.headline_stats.longest_running_turn_sec, Some(8_880));
        assert_eq!(summary.headline_stats.current_streak_days, Some(8));
        assert_eq!(summary.headline_stats.longest_streak_days, Some(19));
        assert_eq!(summary.headline_stats.local_total_tokens, 60);
        assert_eq!(summary.headline_stats.token_delta, Some(5_899_999_940));
    }

    #[test]
    fn account_usage_empty_snapshot_does_not_override_local_headlines() {
        let temp = TempDir::new().unwrap();
        write_log(temp.path(), &fixture(100, 50));
        refresh_usage_index(temp.path()).unwrap();
        let conn = open_usage_db(&usage_db_path(temp.path())).unwrap();
        store_account_usage_snapshot(
            &conn,
            &AccountUsageSnapshot {
                lifetime_tokens: Some(0),
                peak_daily_tokens: Some(0),
                longest_running_turn_sec: Some(0),
                current_streak_days: Some(0),
                longest_streak_days: Some(0),
                daily_buckets: Vec::new(),
            },
        )
        .unwrap();

        let summary = get_usage_summary(temp.path(), summary_request("all")).unwrap();

        assert_eq!(summary.headline_stats.source, "local_sqlite");
        assert_eq!(summary.headline_stats.lifetime_tokens, Some(60));
        assert_eq!(summary.headline_stats.peak_daily_tokens, Some(60));
        assert_eq!(summary.headline_stats.current_streak_days, Some(1));
        assert_eq!(summary.headline_stats.codex_total_tokens, None);
        assert_eq!(
            summary
                .diagnostics
                .parser_diagnostics
                .get("account_usage_empty"),
            Some(&1)
        );
    }

    #[test]
    fn scoped_usage_headlines_are_not_overwritten_by_global_account_snapshot() {
        let temp = TempDir::new().unwrap();
        write_log(temp.path(), &fixture(100, 50));
        refresh_usage_index(temp.path()).unwrap();
        let conn = open_usage_db(&usage_db_path(temp.path())).unwrap();
        store_account_usage_snapshot(
            &conn,
            &AccountUsageSnapshot {
                lifetime_tokens: Some(5_900_000_000),
                peak_daily_tokens: Some(193_000_000),
                longest_running_turn_sec: Some(8_880),
                current_streak_days: Some(8),
                longest_streak_days: Some(19),
                daily_buckets: vec![("2026-06-28".to_string(), 12345)],
            },
        )
        .unwrap();

        let mut req = summary_request("all");
        req.scope_id = Some("workspace:main".to_string());
        let summary = get_usage_summary(temp.path(), req).unwrap();

        assert_eq!(summary.headline_stats.source, "local_sqlite");
        assert_eq!(summary.headline_stats.lifetime_tokens, Some(60));
        assert_eq!(summary.headline_stats.peak_daily_tokens, Some(60));
        assert_eq!(summary.headline_stats.current_streak_days, Some(1));
        assert_eq!(summary.headline_stats.longest_streak_days, Some(1));
        assert_eq!(summary.headline_stats.codex_total_tokens, None);
        assert_eq!(summary.headline_stats.token_delta, None);
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
            crate::services::lam_paths::LamPaths::for_home(&home).usage_db_path()
        );
        assert!(usage_db_path(&home).exists());
        assert!(summary.total_calls >= result.inserted_or_updated_events);
        assert!(!home.join(".codex/usage.sqlite3").exists());
        assert!(!home.join(".codex/sessions/usage.sqlite3").exists());
        assert!(!home.join(".codex/logs/usage.sqlite3").exists());
        assert!(!home.join(".codex/cache/usage.sqlite3").exists());
    }

    #[test]
    #[ignore]
    fn real_home_usage_refresh_performance_smoke() {
        use std::time::Instant;

        let home = PathBuf::from(std::env::var("HOME").expect("HOME is required"));
        let started = Instant::now();
        let refresh = refresh_usage_index(&home).unwrap();
        println!(
            "refresh_index: elapsed={}ms scanned_files={} parsed_files={} parsed_events={} inserted_or_updated={}",
            started.elapsed().as_millis(),
            refresh.scanned_files,
            refresh.parsed_files,
            refresh.parsed_events,
            refresh.inserted_or_updated_events
        );
    }

    #[test]
    #[ignore]
    fn real_home_usage_section_performance_smoke() {
        use std::time::Instant;

        let home = PathBuf::from(std::env::var("HOME").expect("HOME is required"));
        let mut req = dashboard_request("all");
        req.limit = Some(100);

        fn measure<T, F>(label: &str, mut f: F)
        where
            F: FnMut() -> Result<T>,
        {
            let mut samples = Vec::new();
            for _ in 0..7 {
                let started = Instant::now();
                f().unwrap();
                samples.push(started.elapsed().as_millis());
            }
            samples.sort_unstable();
            let min = samples[0];
            let p50 = samples[samples.len() / 2];
            let p95 =
                samples[((samples.len() as f64 * 0.95).ceil() as usize - 1).min(samples.len() - 1)];
            let max = samples[samples.len() - 1];
            println!(
                "{label}: min={min}ms p50={p50}ms p95={p95}ms max={max}ms samples={samples:?}"
            );
        }

        println!("usage_db={}", usage_db_path(&home).display());
        measure("scopes", || get_usage_scopes(&home, req.clone()));
        measure("overview", || get_usage_overview(&home, req.clone()));
        measure("activity", || get_usage_activity(&home, req.clone()));

        let mut calls_50 = req.clone();
        calls_50.limit = Some(50);
        measure("calls_50", || get_usage_calls(&home, calls_50.clone()));

        let mut calls_100 = req.clone();
        calls_100.limit = Some(100);
        measure("calls_100", || get_usage_calls(&home, calls_100.clone()));

        measure("threads", || get_usage_threads(&home, req.clone()));
        measure("diagnostics", || get_usage_diagnostics(&home, req.clone()));

        let started = Instant::now();
        let refresh = refresh_usage_index(&home).unwrap();
        println!(
            "refresh_index: elapsed={}ms scanned_files={} parsed_files={} parsed_events={} inserted_or_updated={}",
            started.elapsed().as_millis(),
            refresh.scanned_files,
            refresh.parsed_files,
            refresh.parsed_events,
            refresh.inserted_or_updated_events
        );
    }
}
