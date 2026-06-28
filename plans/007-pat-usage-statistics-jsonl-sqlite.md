# Plan 007: Add PAT-mode Codex usage statistics from JSONL into local SQLite

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If a STOP condition occurs, stop and report; do not improvise.
> When done, update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**:
> `git diff --stat 600ecc7..HEAD -- apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.lock apps/desktop/src-tauri/src apps/desktop/src apps/desktop/package.json docs/codex_usage_tracker_jsonl_rust_rearchitecture_spec.md`
>
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts below against the live code before proceeding.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: none
- **Category**: direction
- **Planned at**: commit `600ecc7`, 2026-06-28
- **Execution status**: DONE at implementation commit `43ac142`, 2026-06-28

## Why this matters

LAM already manages Codex accounts, PAT switching, and quota refresh. The next useful PAT-mode surface is local usage visibility: scan existing Codex JSONL logs on first launch, write aggregate usage rows into SQLite, then refresh incrementally on the same cadence as quota. The reference project at `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker` is the implementation source of truth for parser/store semantics and reusable dashboard logic. Reuse its code and contracts where they fit, but adapt them into LAM's current Tauri/Rust/React framework and use LAM's own styling.

## Product decisions

- Archived sessions: add an all-history toggle. The default view may remain
  active sessions only, but the archived-session path must still be incremental:
  unchanged archived JSONL files are skipped, appended complete lines continue
  from their stored cursor, and rewrites/truncations replace rows for that
  source only.
- Refresh lifecycle: refresh from tray/background too, on the same cadence as
  quota. Use one app/Rust-owned scheduler instead of a React-window-owned
  interval:
  - The scheduler must read the same persisted auth mode source used by the UI
    and run usage refresh only while PAT mode is active.
  - React should not own a second periodic refresh. The window should load the
    current summary for display and call the same refresh command for manual
    `Stats -> Refresh`.
  - Tray/background and manual refresh must share one refresh path. Add a
    single-process guard around SQLite refresh so overlapping scheduled/manual
    refreshes do not parse or write the DB concurrently.
  - The scheduler cadence must reuse the quota cadence source. If quota cadence
    changes later, usage refresh should follow without adding another config
    surface.
  - Window-hidden and tray-only operation must still refresh usage while PAT
    mode is active.
- DB cleanup: add a Settings reset action for the LAM-owned usage DB, plus
  automatic SQLite compaction. Reset must delete only `~/.codex/lam/usage/`
  tracker state, never Codex-owned session/log/cache files.
- Time windows: add Today, 7d, 30d, monthly, and custom-date filtering for the
  statistics view, following the reference project's filtering behavior where
  it fits LAM's aggregate-only data model.
- Reference parity: implement **Full statistics parity plus pricing and
  diagnostics subset**. This means Plan 007 should port the reference project's
  highest-value statistics behavior into a LAM-native surface, not merely show
  a small summary table.
  - Statistics parity includes parser/store semantics, aggregate schema where
    fields overlap, time filters, thread/call grouping, model and effort
    breakdowns, top threads/calls, cache/context attention signals, sorting,
    formatting, and summary calculations that can run on aggregate rows.
  - Pricing is in scope as a local estimate: port/adapt the reference rate-card
    model and cost calculation paths that work from aggregate token counters.
    Keep pricing data local and deterministic. Do not add network pricing
    lookups or external billing integrations.
  - Diagnostics is in scope as aggregate diagnostics: parser diagnostics,
    skipped/unknown event counts, low-cache/context-window attention, and
    diagnostic summaries that do not require raw transcript storage.
  - Raw evidence is out of scope: do not persist prompts, assistant text, tool
    output, command text, tool arguments, or raw JSON payloads. Do not port the
    raw evidence investigator unless a later plan explicitly adds a separate
    privacy model.
  - Product-shell parity is out of scope: do not port the MCP server, plugin
    installer, standalone dashboard shell, reference CSS/HTML shell, or CSV
    export in Plan 007.
  - The UI must still use LAM's existing React/Tauri architecture and LAM CSS
    tokens. Reference frontend code should be reused as pure helper logic where
    possible, not copied as a second application shell.

## Current state

- `/Users/micro/Documents/Code/Rust/LAM/docs/codex_usage_tracker_jsonl_rust_rearchitecture_spec.md` says the core pipeline is `Raw JSONL -> parser state machine -> aggregate events -> SQLite aggregate index`, not a line-by-line CLI/dashboard translation.
- The same spec requires append-only cursor state: `parsed_until_byte`, `parsed_until_line`, `parser_state_json`, and a parser adapter version, because `turn_context`, `session_meta`, cumulative de-dup, call origin, and diagnostic segments can cross refresh boundaries.
- `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker/src/codex_usage_tracker/parser.py` contains reusable parser decisions: `PARSER_DIAGNOSTIC_KEYS`, `KNOWN_NON_TOKEN_EVENT_MSG_TYPES`, `ParserState`, `load_session_index`, `find_session_logs`, and `parse_usage_events_from_file_with_state`. Port these semantics to Rust instead of inventing a different parser contract.
- `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker/src/codex_usage_tracker/store_sources.py` contains reusable incremental refresh planning: unchanged `size_bytes + mtime_ns` skips, append-only continuation from `parsed_until_byte`, and full replacement on adapter/state/truncate/rewrite mismatch. Port this decision table to Rust.
- `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker/src/codex_usage_tracker/schema.py` defines aggregate `usage_events` columns such as `record_id`, `session_id`, `thread_name`, `event_timestamp`, `source_file`, `line_number`, `turn_id`, `cwd`, `model`, `effort`, token counters, cumulative token counters, rate-limit fields, `uncached_input_tokens`, `cache_ratio`, `reasoning_output_ratio`, and `context_window_percent`.
- `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker/src/codex_usage_tracker/store_schema.py` also uses `source_files`, `refresh_meta`, `thread_summaries`, and optionally `call_diagnostic_facts`. For this LAM plan, implement `usage_events`, `source_files`, `refresh_meta`, query-time thread summaries, local pricing estimates, and aggregate diagnostics. Defer raw evidence, MCP, CSV export, plugin shell, and transcript-level diagnostics.
- `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker/src/codex_usage_tracker/plugin_data/dashboard/dashboard_data.js`, `dashboard_analysis.js`, and `dashboard_format.js` contain reusable pure frontend logic for token getters, uncached/cached math, thread grouping, sorting, compact number formatting, timestamps, and low-cache/context attention scores. Port/adapt these into TypeScript helper modules instead of recreating their behavior inline in `App.tsx`.
- `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker/src/codex_usage_tracker/costing.py`, `pricing_config.py`, `pricing.py`, `pricing_estimates.py`, and `plugin_data/rate_cards/codex-credit-rates.json` contain the reference local pricing model. Port the aggregate-only estimate path, not network billing.
- `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker/src/codex_usage_tracker/diagnostic_reports.py`, `diagnostic_facts.py`, `diagnostics.py`, and dashboard diagnostics helpers contain richer diagnostics. For Plan 007, port aggregate summaries and parser/skip/context signals only; do not port raw evidence investigators or transcript-backed snapshots.
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/Cargo.toml` currently has `serde`, `serde_json`, `tauri`, `chrono`, and `uuid`, but no SQLite dependency. Add only `rusqlite = { version = "0.32", features = ["bundled"] }` unless the build proves a newer already-compatible version is required.
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/src/services/types.rs:49-50` has `config_root(home_root) -> home_root.join(".config/agent-workspace")`, but this plan must not use that location for usage SQLite. Put usage tracker state under Codex's own home in a LAM-owned subdirectory: `home_root.join(".codex/lam/usage/usage.sqlite3")`. This keeps it under `~/.codex` while not mixing it with Codex-owned `sessions`, `logs`, `cache`, `auth.json`, `config.toml`, or other root files.
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/src/services/mod.rs:1-22` exports small service modules. Add one module: `usage`.
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/src/commands/mod.rs:34-42` wraps blocking Rust work with `run_blocking`. New usage refresh/query commands should follow that pattern.
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/src/main.rs:44-93` registers Tauri commands in `generate_handler!`. Register the new usage commands there.
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/src/tray.rs:230-295` already owns a tray/background quota refresh path with a `TRAY_BUSY` guard and a five-minute loop. Add usage refresh there instead of creating a React interval; keep the existing quota behavior unchanged.
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/lib/api.ts:1-246` is the only frontend Tauri invoke wrapper. Add usage wrappers here and type them in `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/lib/types.ts`.
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/stores/quota.ts:8` sets `QUOTA_REFRESH_INTERVAL_MS = 2 * 60_000`; `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/stores/quota.ts:118-124` starts quota auto-refresh. Usage refresh must use the same cadence source, but its periodic owner should be the app/Rust scheduler rather than a React window interval.
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/routes/views.tsx:977` renders `Settings`. Add reset/compact controls there rather than creating a new route.
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/App.tsx:543-553` renders the titlebar center PAT Mode toggle. Add a stats button next to it only when `authMode === 'pat'`.
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/components/shell.tsx:31-58` has `Shell.Modal` with `wide`; use it for the statistics page instead of adding a route.
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/App.handoff.test.tsx:12-35` mocks API calls and `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/App.handoff.test.tsx:119-215` resets stores. Follow this test style for the PAT stats modal.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Frontend unit tests | `cd apps/desktop && npm test -- App.handoff.test.tsx` | exit 0 |
| Frontend helper tests | `cd apps/desktop && npm test -- usage-dashboard.test.ts` | exit 0 |
| Frontend build | `cd apps/desktop && npm run build` | exit 0 |
| Rust focused tests | `cd apps/desktop/src-tauri && cargo test usage` | exit 0 |
| Rust full tests | `cd apps/desktop/src-tauri && cargo test` | exit 0 |
| Rust formatting | `cd apps/desktop/src-tauri && cargo fmt -- --check` | exit 0 |
| Rust lint | `cd apps/desktop/src-tauri && cargo clippy -- -D warnings` | exit 0 |
| Repo gate | `make check` | exit 0 |

## Scope

**In scope**:
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/Cargo.toml`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/Cargo.lock`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/src/services/usage.rs` (create)
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/src/services/mod.rs`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/src/commands/mod.rs`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/src/main.rs`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/src/tray.rs`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/lib/types.ts`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/lib/api.ts`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/lib/usage-dashboard-data.ts` (create by adapting reference `dashboard_data.js`)
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/lib/usage-dashboard-analysis.ts` (create by adapting reference `dashboard_analysis.js`)
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/lib/usage-dashboard-format.ts` (create by adapting reference `dashboard_format.js`)
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/lib/usage-pricing.ts` (create by adapting reference pricing/rate-card logic)
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/lib/usage-diagnostics.ts` (create by adapting aggregate diagnostics logic)
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/stores/usage.ts` (create)
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/stores/app.ts`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/App.tsx`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/routes/views.tsx`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/styles.css`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/App.handoff.test.tsx`

**Out of scope**:
- Do not implement raw evidence/context loading.
- Do not store prompts, assistant text, tool output, command text, tool arguments, or raw JSON payloads in SQLite or frontend state.
- Do not port the Python dashboard assets, locale system, MCP server, CLI, CSV export, raw evidence investigator, standalone dashboard shell, or plugin installer.
- Do not port diagnostics snapshots that require raw transcript storage. Aggregate diagnostics are in scope.
- Do not copy the reference project's CSS/HTML shell. Frontend behavior/helpers may be ported; visual styling must be written in `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/styles.css` using this repo's tokens and UI conventions.
- Do not add charting/table dependencies. Use React, CSS, and existing UI primitives.
- Do not change quota behavior except to share its refresh interval constant or cadence.

## Git workflow

- Branch: `codex/007-pat-usage-statistics`
- Commit message style observed in this repo: `feat: ...`, for example `feat: prevent Codex process orphans on restart and account switch`
- Do not push or open a PR unless explicitly instructed.

## Target UI

ASCII mockup for the titlebar and modal:

```text
┌──────────────────────────────────────────────────────────────────────┐
│ LAM  Overview              PAT Mode  [on]  [Stats]   Refresh New ... │
└──────────────────────────────────────────────────────────────────────┘

┌─ Codex Usage ────────────────────────────────────────────────────────┐
│ [Refresh]  Updated 14:22 · 18 files · 96 calls                        │
│                                                                      │
│ ┌────────────┐ ┌────────────┐ ┌────────────┐ ┌────────────┐          │
│ │ Calls      │ │ Total      │ │ Cached     │ │ Uncached   │          │
│ │ 96         │ │ 12.4M tok  │ │ 9.1M tok   │ │ 3.3M tok   │          │
│ └────────────┘ └────────────┘ └────────────┘ └────────────┘          │
│                                                                      │
│ Tabs: [Insights] [Calls] [Threads]                                   │
│                                                                      │
│ Top threads                                                          │
│ Thread                     Calls   Total tokens   Cache              │
│ workspace/LAM              24      4.8M           78%                │
│ usage parser spike          9      1.1M           41%                │
└──────────────────────────────────────────────────────────────────────┘
```

Use a compact operational layout. No marketing hero, no nested cards, no heavy decorative gradients.

## Reuse policy

Default to reuse before writing new logic:

- Parser/store semantics: translate the reference Python algorithms from `parser.py`, `store_sources.py`, `schema.py`, and the relevant `store.py` refresh/upsert/query paths into Rust. Keep names and persisted column contracts aligned where practical.
- Frontend logic: port the pure functions from `dashboard_data.js`, `dashboard_analysis.js`, `dashboard_format.js`, and the reference pricing/diagnostics helpers into small TypeScript modules under `apps/desktop/src/lib/`. Remove i18n, DOM, CSV, URL-state, plugin-shell, and raw-evidence branches that are out of scope, but do not hand-roll substitute grouping/sorting/formatting/pricing/diagnostics while reusable reference code exists.
- Frontend rendering and styles: implement as LAM React components/modal content and LAM CSS only. Do not import or paste reference dashboard CSS/HTML as the product shell.
- If a reference function depends on out-of-scope fields, keep a narrow compatible subset and document the omitted fields in the helper file's tests.

## Steps

### Step 1: Add the Rust usage service and schema

Create `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/src/services/usage.rs`.

Start by translating the relevant reference algorithms from:
- `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker/src/codex_usage_tracker/parser.py`
- `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker/src/codex_usage_tracker/store_sources.py`
- `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker/src/codex_usage_tracker/schema.py`
- `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker/src/codex_usage_tracker/store.py`

Do not start from a blank parser/store design unless a reference behavior is out of scope or impossible in Rust.

Keep the module single-file for this plan. It should contain:
- data structs with `#[serde(rename_all = "camelCase")]`:
  - `UsageRefreshResult { scanned_files, parsed_files, parsed_events, inserted_or_updated_events, skipped_events, db_path, parser_diagnostics }`
  - `UsageSummaryRequest { window, include_archived }`
  - `UsageWindow { preset, from, to }`, where `preset` is one of `all`, `today`, `7d`, `30d`, `month`, or `custom`
  - `UsageSummary { refreshed_at, scanned_files, parsed_events, skipped_events, total_calls, total_tokens, input_tokens, cached_input_tokens, uncached_input_tokens, output_tokens, reasoning_output_tokens, estimated_cost_usd, pricing_coverage, diagnostics, top_threads, recent_calls }`
  - `UsagePricingCoverage { priced_tokens, unpriced_tokens, priced_token_ratio, unknown_models }`
  - `UsageDiagnosticsSummary { parser_diagnostics, skipped_events, unknown_models, low_cache_threads, high_context_calls, last_refresh_error }`
  - `UsageThreadSummary { thread_key, thread_label, call_count, total_tokens, input_tokens, cached_input_tokens, uncached_input_tokens, output_tokens, latest_event_timestamp, cache_ratio, estimated_cost_usd, is_archived }`
  - `UsageCallRow { record_id, session_id, thread_name, event_timestamp, source_file, line_number, cwd, model, effort, input_tokens, cached_input_tokens, uncached_input_tokens, output_tokens, reasoning_output_tokens, total_tokens, cumulative_total_tokens, cache_ratio, estimated_cost_usd, pricing_model, pricing_estimated, is_archived }`
- internal structs:
  - `ParserState { session_id, current_turn, last_cumulative_total }`
  - `CurrentTurn { turn_id, turn_timestamp, cwd, model, effort, current_date, timezone }`
  - `SourceParsePlan { path, is_archived, start_byte, start_line, initial_state, replace_existing }`

Use `rusqlite` and create these tables:
- `usage_events` with only the fields needed by `UsageCallRow` plus cumulative counters and `is_archived`. Keep names compatible with the reference project where fields overlap.
- `source_files` with `source_file`, `is_archived`, `size_bytes`, `mtime_ns`, `parsed_until_line`, `parsed_until_byte`, `parser_adapter`, `parser_state_json`, `parser_diagnostics_json`, `last_indexed_at`.
- `refresh_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)`.

Use `PARSER_ADAPTER_VERSION = "lam-codex-jsonl-v1"`.

Implement:
- `usage_db_path(home_root: &Path) -> PathBuf`
- `refresh_usage_index(home_root: &Path, include_archived: bool) -> Result<UsageRefreshResult>`
- `get_usage_summary(home_root: &Path, req: UsageSummaryRequest) -> Result<UsageSummary>`
- `reset_usage_index(home_root: &Path) -> Result<()>`
- `compact_usage_db(home_root: &Path) -> Result<()>`
- `init_usage_db(conn: &Connection) -> Result<()>`

`usage_db_path(home_root)` must return:

```rust
home_root.join(".codex/lam/usage/usage.sqlite3")
```

Create the parent directory with private permissions before opening the DB. Do not write tracker state directly into `~/.codex`, `~/.codex/sessions`, `~/.codex/logs`, `~/.codex/cache`, or any existing Codex-owned path.

Discovery rules:
- Always read `home_root/.codex/sessions/**/*.jsonl`.
- When `include_archived` is true, also read `home_root/.codex/archived_sessions/*.jsonl`, matching the reference `find_session_logs(..., include_archived=True)` behavior.
- Include `home_root/.codex/session_index.jsonl` if present for `session_id -> thread_name`.
- Ignore `home_root/.codex/lam/**` during discovery even if any `.jsonl` file appears there later.
- Set `is_archived` on `usage_events` and `source_files` based on whether the source file came from `archived_sessions`.
- The all-history toggle is a query/refresh option, not a separate DB. Archived source cursoring must use the same `source_files` incremental rules as active sessions.

Parser rules:
- Stream with `BufRead::read_until(b'\n')`; do not `fs::read()` whole logs.
- Commit `parsed_until_byte` only at complete newline boundaries.
- If the EOF line has no trailing newline, increment `partial_trailing_line` and do not commit that line or byte offset.
- Decode only `session_meta`, `turn_context`, and `event_msg` with `payload.type == "token_count"`.
- For other events, drop the decoded value after deciding it is not token usage.
- Do not write raw JSON into DB.
- Skip token events missing `info.total_token_usage`, `info.last_token_usage`, or cumulative `total_tokens`; increment diagnostics.
- Skip duplicate/decreasing cumulative totals per source using `last_cumulative_total`.
- Derive `uncached_input_tokens = max(input_tokens - cached_input_tokens, 0)`.
- Derive `cache_ratio = cached_input_tokens / input_tokens`, or 0 when input is 0.
- Build `record_id` deterministically from `source_file + line_number + cumulative_total_tokens` with a simple stable string format. Do not add `sha2`; it is unnecessary for LAM v1.

Refresh rules:
- Plan from `source_files`: unchanged `size_bytes + mtime_ns` skips; append-only growth continues from `parsed_until_byte` with persisted state; truncate/rewrite/full mismatch replaces old rows for that source.
- Parse outside a write transaction.
- Apply delete/upsert events, source cursor, and `refresh_meta` in one SQLite transaction.
- Use `PRAGMA busy_timeout = 5000`, `PRAGMA foreign_keys = ON`, and attempt `PRAGMA journal_mode = WAL`.
- `compact_usage_db` must run SQLite compaction outside any write transaction. Use `VACUUM` for the explicit Settings reset path and after refreshes that deleted/replaced source rows; use `PRAGMA optimize` after ordinary refreshes. Do not compact Codex-owned files.

**Verify**: `cd apps/desktop/src-tauri && cargo test usage` -> failing at first is acceptable until Step 2 adds tests; after Step 2 it must pass.

### Step 2: Add focused Rust tests

Add tests inside `usage.rs` under `#[cfg(test)]`.

Minimum tests:
- `parses_basic_token_count_fixture`: create a temp `home/.codex/sessions/...jsonl` with `session_meta`, `turn_context`, one token event, and `session_index.jsonl`; refresh; assert one row and expected model/cwd/token fields.
- `refresh_is_idempotent_for_unchanged_source`: run refresh twice; assert `total_calls` remains 1 and second refresh parses 0 files.
- `append_refresh_uses_cursor_and_state`: first write prefix ending after `turn_context`, refresh; append token event; refresh; assert inherited model/cwd and one row.
- `partial_trailing_line_is_not_committed`: write a final JSON line without newline; refresh; assert 0 calls and `partial_trailing_line` diagnostic is nonzero; append newline and refresh; assert row appears once.
- `rewrite_replaces_source_rows`: write one token event, refresh, then rewrite file with another token event and changed mtime/size; refresh; assert only the replacement row remains.
- `normal_db_does_not_store_raw_content`: include obvious fake prompt/tool text in a non-token event; refresh; read `usage.sqlite3` bytes or query all text columns and assert the fake raw text is absent.
- `usage_db_lives_under_lam_codex_subdir`: assert `usage_db_path(temp_home)` equals `temp_home/.codex/lam/usage/usage.sqlite3`, and refresh creates that file without writing any DB file directly under `temp_home/.codex`.
- `discovery_ignores_lam_usage_directory`: place a `.jsonl` fixture under `temp_home/.codex/lam/usage/` and assert refresh does not parse it.
- `all_history_includes_archived_incrementally`: place one fixture under `.codex/sessions/...` and one under `.codex/archived_sessions/...`; refresh with `include_archived=false` and assert only the active row appears; refresh with `include_archived=true`, assert both appear and archived rows have `is_archived=true`; run a second all-history refresh and assert unchanged archived files are skipped.
- `summary_window_filters_rows`: create rows across today, 7d, 30d, month, and an older date; assert `UsageSummaryRequest` presets and custom `{ from, to }` include the expected counts.
- `reset_usage_index_removes_only_lam_usage_state`: create a usage DB plus fake Codex-owned files under `.codex/sessions`, `.codex/logs`, and `.codex/cache`; call `reset_usage_index`; assert only `.codex/lam/usage` tracker state is removed and Codex-owned files remain.
- `compact_usage_db_preserves_rows`: refresh a fixture, call `compact_usage_db`, then assert `PRAGMA integrity_check` returns `ok` and row counts are unchanged.
- `refresh_guard_serializes_overlapping_calls`: call the guarded refresh path from two threads against the same temp home; assert both calls return without duplicate rows or SQLite busy errors.

Use `tempfile` already present in dev-dependencies. Do not add test frameworks.

**Verify**: `cd apps/desktop/src-tauri && cargo test usage` -> exit 0.

### Step 3: Expose Tauri commands and TypeScript API

Update:
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/src/services/mod.rs`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/src/commands/mod.rs`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/src/main.rs`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/lib/types.ts`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/lib/api.ts`

Commands:
- `refresh_usage_index(include_archived: bool) -> UsageRefreshResult`
- `get_usage_summary(req: UsageSummaryRequest) -> UsageSummary`
- `reset_usage_index() -> ()`
- `compact_usage_db() -> ()`

All commands must use `run_blocking` in `commands/mod.rs`.

Frontend API:
- `refreshUsageIndex(includeArchived = false): Promise<UsageRefreshResult>`
- `getUsageSummary(req: UsageSummaryRequest): Promise<UsageSummary>`
- `resetUsageIndex(): Promise<void>`
- `compactUsageDb(): Promise<void>`

Browser preview fallback:
- `getUsageSummary` returns:

```ts
{
  refreshedAt: null,
  scannedFiles: 0,
  parsedEvents: 0,
  skippedEvents: 0,
  totalCalls: 0,
  totalTokens: 0,
  inputTokens: 0,
  cachedInputTokens: 0,
  uncachedInputTokens: 0,
  outputTokens: 0,
  reasoningOutputTokens: 0,
  estimatedCostUsd: 0,
  pricingCoverage: { pricedTokens: 0, unpricedTokens: 0, pricedTokenRatio: 0, unknownModels: [] },
  diagnostics: {
    parserDiagnostics: {},
    skippedEvents: 0,
    unknownModels: [],
    lowCacheThreads: [],
    highContextCalls: [],
    lastRefreshError: null,
  },
  topThreads: [],
  recentCalls: [],
}
```

- `refreshUsageIndex` returns `{ scannedFiles: 0, parsedFiles: 0, parsedEvents: 0, insertedOrUpdatedEvents: 0, skippedEvents: 0, dbPath: '', parserDiagnostics: {} }`.
- `resetUsageIndex` and `compactUsageDb` resolve without work.

**Verify**: `cd apps/desktop && npm run build` -> TypeScript exits 0.

### Step 4: Port reusable dashboard helper logic into TypeScript

Create:
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/lib/usage-dashboard-data.ts`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/lib/usage-dashboard-analysis.ts`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/lib/usage-dashboard-format.ts`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/lib/usage-pricing.ts`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/lib/usage-diagnostics.ts`

Port/adapt from the reference project:
- `dashboard_data.js`: `rowInputTokens`, `cachedInputTokens`, `uncachedInputTokens`, `outputTokens`, `resolveThreadAttachment`, `chronological`, `buildCallAdjacencyIndex`, `adjacentThreadCalls`, `compactListSummary`, and thread label/model summary helpers.
- `dashboard_analysis.js`: `compareCalls`, `sortedThreadCalls`, thread grouping/sorting, low-cache and token/context attention scoring. Keep cost-aware attention inputs when they can be computed from local pricing estimates.
- `dashboard_format.js`: number formatting, compact number formatting, percent formatting, timestamp formatting, duration formatting, sort label, and `compareValues`.
- Reference pricing modules/rate cards: local rate-card lookup, model matching, token-counter-to-cost calculations, and summary cost formatting. Keep the implementation deterministic and offline.
- Reference diagnostics modules: aggregate parser diagnostics, skipped/unknown event summaries, low-cache/context-window signals, and model/thread diagnostic rollups that do not require raw transcripts.

Do not port DOM rendering, URL state, CSV export, i18n, tooltips, raw evidence call investigator, plugin shell, or reference CSS.

Add focused frontend tests if the existing Vitest setup can cover pure helper modules cheaply:
- uncached tokens clamp to 0 when cached > input;
- thread grouping uses thread name before session id;
- call sorting falls back to timestamp then record id;
- compact number/timestamp helpers do not throw on null/malformed values.
- pricing helpers estimate cost from aggregate token counters and local rate cards;
- diagnostics helpers summarize parser/skip/context signals without raw content.

**Verify**:
- `cd apps/desktop/src-tauri && cargo test usage` -> includes scheduler auth-mode gating, quota-cadence reuse, and overlapping refresh serialization; exits 0.
- `cd apps/desktop && npm run build` -> exit 0.

### Step 5: Add usage state and an app-owned refresh scheduler

Create `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/stores/usage.ts`.

State:
- `summary: UsageSummary | null`
- `refreshing: boolean`
- `loadUsageSummary(): Promise<void>`
- `refreshUsage(): Promise<void>`

Implementation:
- `loadUsageSummary` calls `api.getUsageSummary`.
- `refreshUsage` calls `api.refreshUsageIndex`, then `api.getUsageSummary`.
- Do not start a React-owned interval from this store. Periodic refresh belongs
  to the app/Rust-owned scheduler so tray/background refresh continues while
  the window is hidden.

App/Rust scheduler:
- Put the scheduler in `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/src/tray.rs`, next to the existing tray/background quota refresh loop. Do not create a second frontend interval.
- Use a Rust interval of `Duration::from_secs(120)` to match the existing frontend quota cadence of `QUOTA_REFRESH_INTERVAL_MS = 2 * 60_000`. Add a short `ponytail:` comment that this constant mirrors quota cadence until quota refresh has a Rust-owned shared cadence.
- Read PAT/OAuth mode with `localagentmanager_core::types::get_auth_mode(&home)` and refresh usage only when it returns `pat`.
- Call the same `services::usage::refresh_usage_index` path as manual `refreshUsage`.
- Guard the refresh path inside `services::usage`, not only in `tray.rs`, with one static in-process `Mutex<()>` so scheduled refresh and manual refresh cannot parse/write SQLite concurrently.
- If a scheduled refresh finds the lock already held, it may skip that tick. Manual refresh should wait for the lock and then run.

Wire `App.tsx`:
- On mount, when `authMode === 'pat'`, load the current summary for display.
- Do not create or stop a periodic usage interval in React.
- Do not refresh usage in OAuth mode from the UI.

**Verify**: `cd apps/desktop && npm run build` -> exit 0.

### Step 5A: Add Settings reset and SQLite compaction controls

Update:
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/src/services/usage.rs`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/src/commands/mod.rs`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src-tauri/src/main.rs`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/lib/api.ts`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/lib/types.ts`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/routes/views.tsx`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/App.tsx`
- `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/styles.css`

Behavior:
- Add a Settings action named `Reset Usage Statistics`.
- The action calls `resetUsageIndex`, deletes only the LAM-owned usage tracker
  state under `~/.codex/lam/usage/`, then reloads the usage summary.
- Keep Codex-owned files under `.codex/sessions`, `.codex/logs`, `.codex/cache`,
  `.codex/auth.json`, and `.codex/config.toml` untouched.
- Add a compact action if useful in the existing Settings layout, or run compact
  automatically after reset and source-rewrite refreshes without adding another
  visible button. Do not add a settings subpage.
- Compaction must run through the blocking Rust command path and must not run
  inside an open SQLite transaction.

**Verify**:
- `cd apps/desktop/src-tauri && cargo test usage` -> includes reset/compact tests and exits 0.
- `cd apps/desktop && npm run build` -> exit 0.

### Step 6: Add the PAT-mode statistics button and modal

Update `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/App.tsx`.

In titlebar center, change:

```tsx
<div className="titlebarCenter">
  <label className="authModeToggle">...</label>
</div>
```

to a centered cluster containing the existing toggle plus a stats button:

```tsx
<div className="titlebarCenter">
  <div className="titlebarCenterCluster">
    <label className="authModeToggle">...</label>
    {authMode === 'pat' ? (
      <UIButton size="sm" className="toolbarBtn" onClick={() => openModal('usageStats')}>
        Stats
      </UIButton>
    ) : null}
  </div>
</div>
```

If an existing icon fits in `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/components/icons.tsx`, use it. If not, text `Stats` is acceptable; do not add an icon library.

Update `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/stores/app.ts` modal type:
- add `'usageStats'`.

Render:
- `modal === 'usageStats' ? <Shell.Modal title="Codex Usage" wide close={closeModal}>...</Shell.Modal> : null`

Modal content:
- Top row: Refresh button, refreshed time, scanned files, parsed events, skipped events.
- Filter row: Today, 7d, 30d, Month, Custom date range, and an `All history` toggle. `All history` maps to `includeArchived=true`; custom dates map to `UsageWindow { preset: 'custom', from, to }`.
- Summary metrics: calls, total tokens, cached input, uncached input, output, estimated cost.
- Tabs or segmented buttons in local state: `Insights`, `Calls`, `Threads`.
- `Insights`: use the adapted `usage-dashboard-analysis.ts`, `usage-pricing.ts`, and `usage-diagnostics.ts` helpers to show top threads by attention/total tokens/cost, low-cache notes (`cache_ratio < 0.2` and `input_tokens >= 50000`), and aggregate diagnostics.
- `Calls`: use adapted sorting/formatting/pricing helpers for a recent call table with time, thread, model, total, cached, uncached, output, and estimated cost.
- `Threads`: use adapted grouping/formatting/pricing helpers for a thread table with thread label, calls, total tokens, cache ratio, latest event, and estimated cost.
- `Diagnostics`: either a compact section in `Insights` or a fourth tab. Show parser diagnostics, skipped events, unknown model/rate-card misses, low-cache/context-window signals, and last refresh status. Do not show raw prompt/tool text.

Keep formatting local:
- prefer the adapted `usage-dashboard-format.ts`;
- reuse existing helpers from `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/lib/format.ts` only if they already fit LAM naming/UI.

State:
- Keep selected window and all-history toggle local to `App.tsx` unless an existing store pattern is clearly better.
- Every filter change calls `getUsageSummary({ window, includeArchived })`.
- Manual `Refresh` calls `refreshUsageIndex(includeArchived)` first, then reloads the filtered summary.

**Verify**: `cd apps/desktop && npm run build` -> exit 0.

### Step 7: Style the modal with existing tokens

Update `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/styles.css`.

Add only classes needed by Step 6, for example:
- `.titlebarCenterCluster`
- `.usageStatsToolbar`
- `.usageMetricGrid`
- `.usageMetric`
- `.usageTabs`
- `.usageTable`
- `.usageEmpty`

Constraints:
- No nested card-in-card layouts.
- Border radius 8px or less for repeated metric/table surfaces.
- Use existing CSS variables: `--surface`, `--surface2`, `--line`, `--text`, `--muted`, `--accent`, `--green`, `--amber`, `--red`.
- Do not copy CSS from `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker/src/codex_usage_tracker/plugin_data/dashboard/`.
- Keep text small enough for the modal and avoid viewport-scaled font sizes.
- Ensure the modal content scrolls inside `.modalBody` if needed instead of overflowing the app.

**Verify**: `cd apps/desktop && npm run build` -> exit 0.

### Step 8: Add frontend regression coverage

Update `/Users/micro/Documents/Code/Rust/LAM/apps/desktop/src/App.handoff.test.tsx`.

Extend the API mock with:
- `getUsageSummary`
- `refreshUsageIndex`

Add tests:
- PAT mode shows the `Stats` button; OAuth mode does not.
- Clicking `Stats` opens `Codex Usage` modal and renders mocked totals/top threads.
- Clicking modal `Refresh` calls `refreshUsageIndex` and reloads summary.
- Toggling `All history` reloads summary with `includeArchived=true` and manual refresh passes the same flag.
- Selecting Today, 7d, 30d, Month, and Custom date range calls `getUsageSummary` with the expected `UsageWindow`.
- Settings `Reset Usage Statistics` calls `resetUsageIndex` and reloads the current summary.
- Starting in PAT mode loads the current usage summary without creating a React-owned usage interval.
- Mocked summaries render estimated cost and aggregate diagnostics without raw prompt/tool text.

Mock summary should use fake aggregate values only. Do not include raw prompt/tool text.

**Verify**: `cd apps/desktop && npm test -- App.handoff.test.tsx` -> exit 0.

### Step 9: Run final gates

Run:

```bash
cd apps/desktop && npm run build
cd apps/desktop && npm test -- App.handoff.test.tsx
cd apps/desktop && npm test -- usage-dashboard.test.ts
cd apps/desktop/src-tauri && cargo fmt -- --check
cd apps/desktop/src-tauri && cargo clippy -- -D warnings
cd apps/desktop/src-tauri && cargo test
make check
```

Expected:
- all commands exit 0;
- no raw fixture prompt/tool text appears in `~/.codex/lam/usage/usage.sqlite3`;
- no tracker DB is created directly under `~/.codex`, `~/.codex/sessions`, `~/.codex/logs`, or `~/.codex/cache`;
- `git status --short` shows only in-scope files plus `plans/README.md` status update.

## Test plan

- Rust unit tests in `usage.rs` cover parser, SQLite, cursor, idempotence, partial trailing line, source rewrite, and privacy.
- Rust unit tests also cover DB placement under `.codex/lam/usage` and exclusion of that directory from JSONL discovery.
- Frontend helper tests cover the adapted reference logic for token math, grouping, sorting, formatting, local pricing, and aggregate diagnostics.
- Frontend tests in `App.handoff.test.tsx` cover PAT-only button visibility, modal rendering, manual refresh, and absence of React-owned auto-refresh.
- Frontend tests cover all-history and time-window request wiring, plus Settings reset wiring.
- Rust/app tests cover scheduler auth-mode gating, quota-cadence reuse, and overlapping refresh serialization.
- Existing repo gates cover TypeScript build, UI smoke, Rust fmt, clippy, and Rust tests.

## Done criteria

- [x] First PAT-mode app startup initializes `~/.codex/lam/usage/usage.sqlite3` from existing `~/.codex/sessions/**/*.jsonl`.
- [x] No usage DB or tracker metadata is written directly into Codex-owned root files or directories except the LAM-owned `~/.codex/lam/usage/` subtree.
- [x] Repeated refreshes skip unchanged files and append only new complete JSONL lines.
- [x] Partial trailing JSONL lines are not committed and are picked up after completion.
- [x] Usage refresh runs on the same cadence source as quota refresh while PAT mode is active.
- [x] Usage refresh also runs from tray/background using one shared scheduler and one guarded SQLite refresh path.
- [x] PAT mode titlebar center shows a `Stats` button; OAuth mode does not.
- [x] `Stats` opens a wide statistics modal with summary metrics, estimated cost, Calls, Threads, Insights, and aggregate diagnostics.
- [x] `Stats` supports Today, 7d, 30d, Month, Custom, and All history filters.
- [x] Parser/store behavior and frontend grouping/sorting/formatting/pricing/diagnostics are ported from the reference project where in scope, with raw-evidence branches removed.
- [x] Settings can reset the LAM-owned usage DB and compaction runs without touching Codex-owned files.
- [x] Modal styles are implemented with LAM CSS only; reference dashboard CSS is not copied.
- [x] Normal DB/frontend state contains aggregate fields only, not prompt, assistant text, tool output, command text, tool args, or raw JSON.
- [x] `make check` exits 0.
- [x] `plans/README.md` marks Plan 007 as DONE or BLOCKED with one-line reason.

## Execution result

- **Implementation commit**: `43ac142` on branch `codex/007-pat-usage-statistics`.
- **Deep review result**: fixed reviewer findings for pricing model fields,
  mixed-model thread cost, context-window diagnostics, local-date windows,
  known non-token event diagnostics, and date-stable window tests.
- **Verification passed**: `npm run build`, `npm test -- App.handoff.test.tsx`,
  `npm test -- usage-dashboard.test.ts`, `npm run test:ui`, `cargo test usage`,
  `cargo fmt -- --check`, `cargo clippy -- -D warnings`, `cargo test`,
  `cargo test real_home_usage_smoke -- --ignored`, and `make check`.
- **Real-home validation**: `~/.codex/lam/usage/usage.sqlite3` integrity check
  returned `ok` after indexing 41,559 usage events from 259 source files; no
  `usage.sqlite3` was found directly under `.codex`, `.codex/sessions`,
  `.codex/logs`, or `.codex/cache`.

## STOP conditions

Stop and report back if:
- Current code at the paths listed in "Current state" does not match the cited roles or line anchors closely enough to apply the plan safely.
- Current Codex JSONL `token_count` shape in local fixtures does not match the spec/reference project enough to extract `info.total_token_usage` and `info.last_token_usage`.
- `rusqlite` with `bundled` cannot compile in this repo without broad toolchain changes.
- Implementing initial ingestion requires touching sync/session/relay behavior outside the in-scope files.
- A reference parser/store/frontend/pricing/diagnostics helper behavior cannot be ported without large new dependencies, raw-content persistence, or a standalone plugin/dashboard shell. Stop and report the exact helper/behavior instead of replacing it silently.
- A test fixture proves raw prompt/tool text would need to be stored to satisfy the UI. It must not be stored.
- The app/Rust scheduler cannot be added in `tray.rs` without changing existing quota refresh semantics. Keep quota behavior intact and report the conflict.
- Settings reset cannot be implemented while limiting deletion to `~/.codex/lam/usage/`. Do not write a broader delete.
- The titlebar placement is not what the product owner meant by "顶部弹出的菜单下面中间"; keep the implementation behind the modal/store boundary and ask for exact placement rather than moving unrelated UI.

## Maintenance notes

- This is intentionally not a full product-surface port of `codex-usage-tracker`. Reuse parser/store semantics, statistics helpers, pricing helpers, and aggregate diagnostics now; defer raw evidence investigator, CSV export, plugin shell, and MCP.
- Keep all LAM-owned usage state inside `~/.codex/lam/usage/`; future cleanup/import/export code should treat that subtree as tracker-owned and never as Codex session input.
- The most important review point is cursor correctness: `parsed_until_byte` must point to the last complete line and must be committed with `parser_state_json`.
- Keep diagnostics aggregate-only unless a later plan explicitly adds a separate raw-evidence privacy model; normal persistence must remain aggregate-only.
