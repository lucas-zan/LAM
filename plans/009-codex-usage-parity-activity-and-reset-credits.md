# Plan 009: Align Usage Statistics with Codex Account Usage, Activity Heatmap, and Reset Credits

> **Executor instructions**: Follow this plan step by step. Run every
> verification command before moving to the next step. If a STOP condition
> occurs, stop and report instead of improvising. When done, update the Plan 009
> row in `plans/README.md`.
>
> **Drift check (run first)**:
>
> ```bash
> git diff --stat 1cf495f..HEAD -- apps/desktop/src apps/desktop/src-tauri docs/codex_sqlite_event_sourcing_hybrid.md plans/009-codex-usage-parity-activity-and-reset-credits.md plans/README.md
> ```
>
> If any in-scope source file changed since `1cf495f`, compare the Current State
> excerpts below with live code before proceeding. If the usage dashboard or
> quota service has been rewritten, stop and ask for a refreshed plan.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: Plan 008 completed
- **Category**: product / correctness / tests
- **Planned at**: commit `1cf495f`, 2026-06-29
- **Status**: DONE

## Why This Matters

LAM now has a local JSONL-to-SQLite usage dashboard, but it still misses the
Codex account-level usage stats the user wants to see: lifetime tokens, peak
tokens, longest task, current streak, and longest streak. The current total
token number can also be lower than Codex's own count because LAM only sums
parsed local `token_count` rows. Codex's upstream app-server protocol already
defines `account/usage/read` for the exact five headline stats plus daily
buckets, and `account/rateLimits/read` for reset-credit counts. This plan adds
those data surfaces without rebuilding the whole ingestion pipeline.

The implementation must keep LAM's local SQLite read model as the durable UI
cache, following `docs/codex_sqlite_event_sourcing_hybrid.md`: JSONL remains
the replayable local fact source; SQLite remains the query/read-model layer;
Codex account usage is stored as an explicit upstream snapshot so differences
between local and Codex totals are visible instead of hidden.

## Domain Decisions

- `Codex Account Usage` is the upstream account-level source for the five
  headline stats: lifetime tokens, peak daily tokens, longest task, current
  streak, and longest streak.
- `LAM Local Usage` is the local JSONL/SQLite-derived source for calls, threads,
  local token activity, and the activity heatmap. Heatmap `Calls` and `Tokens`
  must use the same local source so `Daily`, `Weekly`, and `Cumulative` modes do
  not mix incompatible domains.
- `Usage Parity Delta` compares Codex lifetime tokens with LAM local total
  tokens. A delta is a diagnostic surface, not a reason to replace heatmap
  buckets with Codex daily token buckets.
- The local session/event parser should be aligned with the Codex reference
  usage model wherever the source files contain equivalent facts. The goal is
  for LAM Local Usage to count valid per-call token rows consistently with
  Codex's logic, while still remaining a replay of local session files rather
  than a clone of upstream account aggregation.

## Resolved Grilling Decisions

1. `call` means one accepted per-call token usage record: a valid model
   inference usage with positive `last_token_usage.total_tokens`. It does not
   mean one user turn or one session. If one turn has multiple model inference
   usage records, it counts as multiple calls.
2. Headline `Longest task` uses Codex
   `longest_running_turn_sec` when `Codex Account Usage` is available. Local
   fallback uses the duration between the first and last event for the same
   `turn_id`, not whole-session wall time.
3. Headline `Peak tokens` means peak daily tokens. Use Codex
   `peak_daily_tokens` when available; local fallback groups accepted per-call
   tokens by `current_date` or event date and takes the largest day.
4. Local fallback `Current streak` ends at the most recent local activity date
   in scope, not necessarily today. UI may show an `as of YYYY-MM-DD` title when
   useful.
5. Heatmap `Daily`, `Weekly`, and `Cumulative` modes all use `LAM Local Usage`
   accepted per-call rows. Do not mix Codex daily token buckets into heatmap
   tokens.
6. Do not force local totals to equal Codex totals. Show `Usage Parity Delta`
   and diagnostics instead.
7. Parser alignment should borrow Codex's proven usage semantics, especially
   `last_token_usage` append behavior and `TokenCount` replay attribution, but
   must not import the whole `rollout-trace` architecture into LAM.
8. Reset-credit expiry from API is only valid after live probe proves a stable
   field. If API expiry is absent, use the local manual override file. If both
   are absent, render hollow/outlined unknown-expiry dots.
9. Reset-credit dots live on the account/card name row. One dot represents one
   available reset credit, capped with `+N` for large counts.
10. Reset-credit color phases stay: blue `>24d`, green `19-24d`, yellow
    `13-18d`, red `7-12d`, black `0-6d/expired`, and hollow/outlined for
    unknown expiry.
11. Usage refresh may refresh local replay and Codex account usage. Quota
    refresh may refresh rate limits and reset credits. Keep those UI meanings
    separate; reading reset-credit metadata must not consume a reset credit.
12. Never persist raw private response bodies for probes. Record only redacted
    field inventories, field paths, types, and diagnostics.
13. If Codex app-server account usage is unavailable, the Usage page must still
    work with local fallback stats and an unavailable diagnostic.
14. Required tests must cover parser reset acceptance, exact duplicate
    suppression, one-turn-many-calls counting, heatmap grouping, manual expiry
    override, and unknown-expiry hollow-dot rendering.

## Current State

Relevant LAM files:

- `apps/desktop/src-tauri/src/services/usage.rs` - local usage parser, SQLite
  schema, dashboard query, and tests.
- `apps/desktop/src-tauri/src/services/quota.rs` - quota refresh paths and
  `UsageQuotaSnapshot`.
- `apps/desktop/src-tauri/src/commands/mod.rs` - Tauri commands for usage and
  quota.
- `apps/desktop/src/lib/types.ts` - frontend usage and quota API types.
- `apps/desktop/src/routes/usage.tsx` - Usage page UI.
- `apps/desktop/src/stores/usage.ts` - Usage dashboard load/refresh store.
- `apps/desktop/src/lib/api.ts` - Tauri invoke wrappers and browser-preview
  fallback objects.
- `apps/desktop/src/App.handoff.test.tsx`,
  `apps/desktop/src/lib/usage-dashboard.test.ts`,
  `apps/desktop/src/stores/quota.test.ts`, and
  `apps/desktop/src/lib/quota.test.ts` - existing test patterns.

Current LAM backend summary shape:

```rust
// apps/desktop/src-tauri/src/services/usage.rs:87
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
```

Current local aggregate query sums only local `usage_events` rows:

```rust
// apps/desktop/src-tauri/src/services/usage.rs:389
"SELECT COUNT(*), COALESCE(SUM(total_tokens),0), COALESCE(SUM(input_tokens),0),
    COALESCE(SUM(cached_input_tokens),0), COALESCE(SUM(uncached_input_tokens),0),
    COALESCE(SUM(output_tokens),0), COALESCE(SUM(reasoning_output_tokens),0)
 FROM usage_events
 WHERE (?1 OR is_archived = 0)
   AND (?2 IS NULL OR event_timestamp >= ?2)
   AND (?3 IS NULL OR event_timestamp < ?3)"
```

Current SQLite schema has `usage_events`, `thread_summaries`,
`aggregate_diagnostic_facts`, `diagnostic_snapshots`, and `refresh_meta`, but
no account-usage snapshot table, no daily activity bucket table, and no
precomputed streak cache:

```rust
// apps/desktop/src-tauri/src/services/usage.rs:561
CREATE TABLE IF NOT EXISTS usage_events (
    record_id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    thread_name TEXT,
    event_timestamp TEXT NOT NULL,
    total_tokens INTEGER NOT NULL,
    cumulative_total_tokens INTEGER NOT NULL,
    ...
);
```

Current token parsing uses `last_token_usage.total_tokens` for per-call rows and
skips rows when `total_token_usage.total_tokens` does not increase:

```rust
// apps/desktop/src-tauri/src/services/usage.rs:1006
let Some(cumulative_total_tokens) = usage_int(total_usage, "total_tokens") else {
    increment(diagnostics, "missing_cumulative_total");
    increment(diagnostics, "skipped_events");
    return;
};
if cumulative_total_tokens <= state.last_cumulative_total {
    increment(diagnostics, "duplicate_cumulative_total");
    return;
}
let total_tokens = usage_int(last_usage, "total_tokens").unwrap_or(0);
```

Current Usage page headline cards do not include lifetime/peak/task/streak and
there is no heatmap:

```tsx
// apps/desktop/src/routes/usage.tsx:304
[
  ['Visible Calls', formatCompactNumber(calls.length)],
  ['Total Tokens', `${formatCompactNumber(summary?.totalTokens)} tok`],
  ['Cached Input', `${formatCompactNumber(summary?.cachedInputTokens)} tok`],
  ['Uncached Input', `${formatCompactNumber(summary?.uncachedInputTokens)} tok`],
  ['Reasoning Output', `${formatCompactNumber(summary?.reasoningOutputTokens)} tok`],
  ['Estimated Cost', formatCost(summary?.estimatedCostUsd)],
  ['Codex Credits', formatCost(summary?.estimatedCostUsd)],
]
```

Current quota snapshot lacks reset-credit metadata:

```rust
// apps/desktop/src-tauri/src/services/quota.rs:36
pub struct UsageQuotaSnapshot {
    pub profile_id: String,
    pub source: String,
    pub fetched_at: u64,
    pub staleness: String,
    pub plan_type: Option<String>,
    pub activity_tokens: Option<u64>,
    pub primary_used_percent: Option<u8>,
    pub primary_window_duration_mins: Option<u64>,
    pub secondary_used_percent: Option<u8>,
    pub secondary_window_duration_mins: Option<u64>,
    pub remaining_percent: Option<u8>,
    pub reset_at: Option<String>,
    pub secondary_reset_at: Option<String>,
    pub alerts: Vec<String>,
    pub suggested_actions: Vec<String>,
}
```

Current ChatGPT direct quota parser reads `wham/usage` but only extracts rate
limit windows:

```rust
// apps/desktop/src-tauri/src/services/quota.rs:467
let rate_limit = value
    .get("rate_limit")
    .or_else(|| value.get("rateLimit"))
    .ok_or_else(|| AppError::new("CHATGPT_USAGE_INVALID", "missing rate_limit"))?;
```

Codex reference source to follow:

```rust
// /Users/micro/Documents/Code/Rust/GitHub_Project/codex/codex-rs/app-server-protocol/src/protocol/v2/account.rs:278
pub struct GetAccountRateLimitsResponse {
    pub rate_limits: RateLimitSnapshot,
    pub rate_limits_by_limit_id: Option<HashMap<String, RateLimitSnapshot>>,
    pub rate_limit_reset_credits: Option<RateLimitResetCreditsSummary>,
}

pub struct RateLimitResetCreditsSummary {
    pub available_count: i64,
}
```

```rust
// /Users/micro/Documents/Code/Rust/GitHub_Project/codex/codex-rs/app-server-protocol/src/protocol/v2/account.rs:352
pub struct AccountTokenUsageSummary {
    pub lifetime_tokens: Option<i64>,
    pub peak_daily_tokens: Option<i64>,
    pub longest_running_turn_sec: Option<i64>,
    pub current_streak_days: Option<i64>,
    pub longest_streak_days: Option<i64>,
}

pub struct AccountTokenUsageDailyBucket {
    pub start_date: String,
    pub tokens: i64,
}
```

The Codex app-server README says `account/rateLimits/read` fetches reset count
and the reset count is snapshot-only. It also documents this response shape:

```json
{
  "rateLimits": {
    "primary": { "usedPercent": 25, "windowDurationMins": 15, "resetsAt": 1730947200 },
    "secondary": null,
    "rateLimitReachedType": null
  },
  "rateLimitResetCredits": { "availableCount": 2 }
}
```

## Reference Code Probe Findings

These findings were checked against
`/Users/micro/Documents/Code/Rust/GitHub_Project/codex` before this plan was
executed:

- `account/usage/read` is real in the app-server protocol and maps to
  `GetAccountTokenUsageResponse`. The app-server processor requires Codex
  backend auth, calls `BackendClient::get_token_usage_profile()`, then maps
  backend fields to `AccountTokenUsageSummary` and `dailyUsageBuckets`.
  Evidence:
  `/Users/micro/Documents/Code/Rust/GitHub_Project/codex/codex-rs/app-server/src/request_processors/account_processor.rs:939`
  and
  `/Users/micro/Documents/Code/Rust/GitHub_Project/codex/codex-rs/app-server/src/request_processors/account_processor.rs:1008`.
- The concrete backend URL for token activity is
  `GET /api/codex/profiles/me` for Codex API path style and
  `GET /wham/profiles/me` for ChatGPT backend path style. Evidence:
  `/Users/micro/Documents/Code/Rust/GitHub_Project/codex/codex-rs/backend-client/src/client.rs:321`
  and the expected-path test at
  `/Users/micro/Documents/Code/Rust/GitHub_Project/codex/codex-rs/backend-client/src/client.rs:948`.
- The backend token profile type contains exactly the requested headline stats
  plus optional daily buckets: `lifetime_tokens`, `peak_daily_tokens`,
  `longest_running_turn_sec`, `current_streak_days`, `longest_streak_days`,
  and `daily_usage_buckets`. Evidence:
  `/Users/micro/Documents/Code/Rust/GitHub_Project/codex/codex-rs/backend-client/src/types.rs:474`.
- Codex session token accounting appends per-call `last_token_usage` into
  `total_token_usage`; it does not require a globally monotonic cumulative value
  before accepting a usage fact. Evidence:
  `/Users/micro/Documents/Code/Rust/GitHub_Project/codex/codex-rs/protocol/src/protocol.rs:2079`
  and
  `/Users/micro/Documents/Code/Rust/GitHub_Project/codex/codex-rs/core/src/context_manager/history.rs:281`.
- Codex replay of persisted session usage treats `TokenCount` as a snapshot to
  re-attach to the active turn. It finds the latest persisted `TokenCount`,
  attributes it by explicit turn id when possible, and falls back to rebuilt
  turn position when ids are regenerated. Evidence:
  `/Users/micro/Documents/Code/Rust/GitHub_Project/codex/codex-rs/app-server/src/request_processors/token_usage_replay.rs:69`.
- Codex's app-server thread notification shape preserves both total and last
  usage breakdowns. That supports using local `last_token_usage` as the
  per-call fact while keeping cumulative totals as a snapshot/diagnostic
  surface. Evidence:
  `/Users/micro/Documents/Code/Rust/GitHub_Project/codex/codex-rs/app-server-protocol/src/protocol/v2/thread.rs:1381`.
- `account/rateLimits/read` maps reset credits to
  `rateLimitResetCredits.availableCount`; the referenced type only contains
  `available_count` and has no expiry field. Evidence:
  `/Users/micro/Documents/Code/Rust/GitHub_Project/codex/codex-rs/backend-client/src/types.rs:21`
  and
  `/Users/micro/Documents/Code/Rust/GitHub_Project/codex/codex-rs/app-server/src/request_processors/account_processor.rs:931`.
- A repository search for `entitlements`, `usage_state`, `usage-state`, and
  `backend-api/(entitlements|usage_state)` under the Codex reference repo found
  no code references. Therefore those endpoints are not established by the
  reference code; they remain live-probe candidates for reset-credit expiry
  only.

## Commands You Will Need

| Purpose | Command | Expected on success |
|---|---|---|
| Frontend build | `cd apps/desktop && npm run build` | exit 0 |
| Frontend focused tests | `cd apps/desktop && npm test -- --run src/App.handoff.test.tsx src/lib/usage-dashboard.test.ts src/lib/quota.test.ts src/stores/quota.test.ts` | exit 0, selected tests pass |
| UI smoke | `cd apps/desktop && npm run test:ui` | exit 0 |
| Rust usage tests | `cd apps/desktop/src-tauri && cargo test usage` | exit 0 |
| Rust quota tests | `cd apps/desktop/src-tauri && cargo test quota` | exit 0 |
| Rust fmt | `cd apps/desktop/src-tauri && cargo fmt -- --check` | exit 0 |
| Rust clippy | `cd apps/desktop/src-tauri && cargo clippy -- -D warnings` | exit 0 |
| Full check | `make check` | exit 0 |

## Scope

In scope:

- `apps/desktop/src-tauri/src/services/usage.rs`
- `apps/desktop/src-tauri/src/services/quota.rs`
- `apps/desktop/src-tauri/src/services/types.rs` only if `UsageQuotaSnapshot`
  is re-exported or mirrored there in the live tree.
- `apps/desktop/src-tauri/src/commands/mod.rs` only if a new command is needed.
- `apps/desktop/src/lib/types.ts`
- `apps/desktop/src/lib/api.ts`
- `apps/desktop/src/lib/usage-dashboard-*.ts`
- `apps/desktop/src/stores/usage.ts`
- `apps/desktop/src/routes/usage.tsx`
- `apps/desktop/src/components/tray-quota-panel.tsx` and
  `apps/desktop/src/routes/views.tsx` only for reset-credit dots on account
  cards.
- `apps/desktop/src/styles.css`
- Existing focused tests listed in Commands You Will Need.

Out of scope:

- Do not rewrite the JSONL ingestion pipeline.
- Do not add a new database file; keep using `~/.codex/lam/usage/usage.sqlite3`
  for usage read models and existing quota cache for quota snapshots.
- Do not persist raw transcript text or request/response payloads.
- Do not consume reset credits. This plan only reads and displays available
  reset-credit count and expiry state.
- Do not assume `/entitlements` or `/usage_state` are the right reset-credit
  expiry sources without a live probe. The probe itself is in scope because the
  user explicitly asked to check these endpoints.
- Do not build a UI for manually editing reset-credit expiry overrides in this
  plan. A simple local config file is acceptable when no API expiry is exposed.
- Do not change non-PAT auth behavior.

## Git Workflow

- Branch: `codex/009-usage-parity-reset-credits`
- Commit style: match recent repo history, for example
  `feat: enhance usage statistics with real data integration and improved UI`.
- Do not push or open a PR unless instructed.

## UI Target

ASCII mockup for the Usage page:

```text
Usage                                                     [Refresh]
Updated 10:42 | 31 files | 4,820 calls | Codex parity: -1.8%

[5.9B Lifetime] [193M Peak day] [2h 28m Longest task]
[8 days Current streak] [19 days Longest streak]

Activity [Daily | Weekly | Cumulative]  Metric [Calls | Tokens]
Mon  Tue  Wed  Thu  Fri  Sat  Sun
░    ▒    █    ▓    ░    ·    ▒     week 1
▒    ▓    █    █    ░    ░    ·     week 2

Insights | Calls | Threads | Diagnostics
```

ASCII mockup for account reset-credit dots:

```text
Yas  ● ●  Pro
     reset credits: 2 | expires in 11d
```

Dot color phases by days until expiry:

- Blue: more than 24 days.
- Green: 19-24 days.
- Yellow: 13-18 days.
- Red: 7-12 days.
- Black: 0-6 days or expired.
- Hollow/outlined dot: count exists but expiry is unknown.

If the live probe proves no reset-credit expiry exists in the supported
responses and no manual override is configured, render hollow/outlined dots and
a title such as `2 reset credits; expiry unknown`, and surface that limitation
in diagnostics. Do not silently skip the probe.

## Steps

### Step 1: Add Codex account-usage data structures and SQLite read models

In `apps/desktop/src-tauri/src/services/usage.rs`, add serializable structs
next to `UsageSummary`:

- `UsageHeadlineStats`
  - `lifetime_tokens: Option<i64>`
  - `peak_daily_tokens: Option<i64>`
  - `longest_running_turn_sec: Option<i64>`
  - `current_streak_days: Option<i64>`
  - `longest_streak_days: Option<i64>`
  - `source: String` (`"codex_account_usage"` or `"local_sqlite"`)
  - `local_total_tokens: i64`
  - `codex_total_tokens: Option<i64>`
  - `token_delta: Option<i64>`
  - `token_delta_percent: Option<f64>`
- `UsageActivityBucket`
  - `date: String`
  - `calls: i64`
  - `tokens: i64`
  - `cumulative_calls: i64`
  - `cumulative_tokens: i64`
- `UsageHeatmapMetric` enum-like string handling on the frontend only; keep
  Rust data metric-neutral.

Extend `UsageSummary` with:

- `headline_stats: UsageHeadlineStats`
- `activity_buckets: Vec<UsageActivityBucket>`

In `init_usage_db`, add these tables:

```sql
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
```

Do not remove the current `usage_events` aggregate path. The new snapshot
tables are additive.

**Verify**:

```bash
cd apps/desktop/src-tauri && cargo test usage_events_contains_required_parity_columns
```

Expected: exit 0. If this test name no longer exists, run
`cargo test usage -- --list 2>&1 | head -c 4000` and choose the closest schema
test before continuing.

### Step 2: Compute local fallback headline stats and activity buckets

Still in `usage.rs`, add local SQL helpers:

- `query_local_headline_stats(conn, req, filter) -> UsageHeadlineStats`
- `query_activity_buckets(conn, req, filter) -> Vec<UsageActivityBucket>`

Local fallback semantics:

- Lifetime tokens: `SUM(total_tokens)` over the same active/all-history scope.
- Peak tokens: max daily `SUM(total_tokens)` grouped by local date. Use
  `date(event_timestamp)` unless `current_date` is non-empty; prefer
  `current_date` when present because Codex session events carry user-local
  date.
- Longest task: max seconds between first and last event per
  `COALESCE(turn_id, record_id)`, ignoring one-row turns as 0 seconds.
- Current streak: consecutive active dates ending at the most recent active
  date in the selected scope.
- Longest streak: longest consecutive run of active dates.
- Daily buckets: one row per date with `COUNT(*)` and `SUM(total_tokens)`.
  Fill missing dates between min and max with zeroes so the heatmap grid is
  stable.
- Cumulative values: running totals over the returned daily buckets.

Return local stats with `source = "local_sqlite"` when there is no Codex
account-usage snapshot.

The local `tokens` values used by these helpers must come from the optimized
session/event parser after Step 4. Do not compute heatmap token buckets from
Codex `dailyUsageBuckets`, because that endpoint has token-only account buckets
and cannot provide call activity on the same domain basis.

**Verify**:

```bash
cd apps/desktop/src-tauri && cargo test usage
```

Expected: exit 0, including new tests for:

- daily bucket grouping by `current_date`
- streak calculation with a gap
- longest task duration from turn timestamps
- cumulative bucket totals

### Step 3: Add Codex `account/usage/read` refresh and parity diagnostics

Implement a read-only Codex app-server request for account usage. Prefer the
same local app-server JSON-RPC mechanism already used by
`try_codex_app_server_quota`; do not call ChatGPT private HTTP endpoints first.
The reference code shows this RPC ultimately reads the backend token profile
from `/api/codex/profiles/me` or `/wham/profiles/me`; use that only as a
diagnostic clue, not as the first production path.

Expected Codex response shape from the reference project:

```json
{
  "summary": {
    "lifetimeTokens": 5900000000,
    "peakDailyTokens": 193000000,
    "longestRunningTurnSec": 8880,
    "currentStreakDays": 8,
    "longestStreakDays": 19
  },
  "dailyUsageBuckets": [
    { "startDate": "2026-06-28", "tokens": 12345 }
  ]
}
```

Store each successful response in `account_usage_snapshot` and
`account_usage_daily_buckets`. `get_usage_summary` should prefer the latest
Codex account snapshot for the five headline stats, but keep local totals
available in `local_total_tokens`. Set:

- `codex_total_tokens = lifetime_tokens`
- `token_delta = codex_total_tokens - local_total_tokens`
- `token_delta_percent = token_delta / codex_total_tokens` when non-zero

If `account/usage/read` is unavailable, return local fallback stats and add a
diagnostic entry such as `account_usage_unavailable`. Do not fail the Usage page
because this endpoint is absent.

The user's example values should be representable after this step:

- `5.9B Lifetime tokens`
- `193M Peak tokens`
- `2h 28m Longest task`
- `8 days Current streak`
- `19 days Longest streak`

**Verify**:

```bash
cd apps/desktop/src-tauri && cargo test account_usage
```

Expected: exit 0. Add tests with a fake app-server response containing the five
fields above and daily buckets.

### Step 4: Correct the local token undercount before blaming Codex parity

Audit `parse_token_count_event` against Codex's own usage model. Codex's
`rollout-trace` model records per-inference `input_tokens`,
`cached_input_tokens`, `output_tokens`, and `reasoning_output_tokens` rather
than relying on only one local cumulative counter. LAM currently skips any row
where `total_token_usage.total_tokens <= state.last_cumulative_total`, which is
valid for duplicate cumulative events inside one monotonic stream but can
undercount when:

- a file contains multiple independent streams with reset cumulative counters;
- a compacted or resumed thread restarts cumulative totals;
- `last_token_usage.total_tokens` is valid while cumulative total is repeated
  or missing.

Use the Codex reference project as the implementation guide for token semantics
and field precedence. The acceptance target is not merely "closer to Codex";
it is that LAM keeps every locally present per-call token record that Codex's
usage model would treat as a valid usage fact, while rejecting exact duplicate
records.

Make the smallest safe correction:

1. Keep duplicate suppression for exact duplicate records.
2. Use `last_token_usage.total_tokens` as the per-call source of truth when it
   is positive.
3. Accept a record with positive `last_token_usage.total_tokens` even when
   `total_token_usage.total_tokens` is repeated, decreased, or reset, provided
   the record identity is not an exact duplicate. This follows Codex's
   `TokenUsageInfo::append_last_usage` model, where last usage is the usage fact
   and total usage is the accumulated snapshot.
4. Reset the monotonic cumulative guard when the parser sees a new turn/session
   boundary that represents a new stream. Do not globally drop valid last-usage
   rows solely because cumulative decreased after a session boundary.
5. Add diagnostics that distinguish `duplicate_cumulative_total` from
   `cumulative_reset_accepted`.

Do not invent token counts when both last usage and cumulative usage are
missing.

**Verify**:

```bash
cd apps/desktop/src-tauri && cargo test usage
```

Expected: exit 0 with new tests proving valid per-call rows survive after a
cumulative reset and exact duplicate rows are still skipped.

### Step 5: Add heatmap controls and rendering

In `apps/desktop/src/lib/types.ts`, mirror the new Rust fields:

- `UsageHeadlineStats`
- `UsageActivityBucket`
- `headlineStats` on `UsageSummary`
- `activityBuckets` on `UsageSummary`

In `apps/desktop/src/lib/api.ts`, update `emptyUsageSummary()` and
`emptyUsageDashboard()`.

In `apps/desktop/src/routes/usage.tsx`, add:

- Headline cards for lifetime, peak, longest task, current streak, longest
  streak.
- A parity chip showing local-vs-Codex token delta when `codexTotalTokens` is
  present.
- Heatmap controls:
  - metric segmented control: `Calls | Tokens`
  - mode segmented control: `Daily | Weekly | Cumulative`
- Heatmap renderer:
  - `Daily`: color by each bucket's `calls` or `tokens`.
  - `Weekly`: group buckets by ISO week and color by weekly sum.
  - `Cumulative`: color by running `cumulative_calls` or
    `cumulative_tokens`.

Keep this in the existing Usage page; do not create a new route. Use ordinary
HTML buttons with existing CSS conventions from `usageTabs` and
`usageMetricGrid`; do not add a charting dependency.

**Verify**:

```bash
cd apps/desktop && npm test -- --run src/App.handoff.test.tsx src/lib/usage-dashboard.test.ts
```

Expected: exit 0 with tests asserting the five headline cards, the metric/mode
controls, and at least one heatmap cell render from mocked buckets.

### Step 6: Probe reset-credit expiry sources, then extend quota refresh metadata

In `apps/desktop/src-tauri/src/services/quota.rs`, extend
`UsageQuotaSnapshot` with:

- `reset_credit_count: Option<i64>`
- `reset_credit_expires_at: Option<String>`
- `reset_credit_expiry_source: Option<String>` with values such as `"api"`,
  `"manual_config"`, or `"unknown"` if this is needed by frontend/tooltips.

Use serde camelCase names so the frontend receives `resetCreditCount` and
`resetCreditExpiresAt`.

For the Codex app-server quota path, parse
`rateLimitResetCredits.availableCount` from `account/rateLimits/read`.

The reference code probe did not find `/entitlements` or `/usage_state` in the
Codex repo, and `rateLimitResetCredits` only proves `availableCount`, not
expiry. Before implementing any expiry display, run a local authenticated probe
against the two user-suspected endpoints using the same `auth-f.json`
access-token pattern already used by the `wham/usage` fallback:

- `GET https://chatgpt.com/backend-api/entitlements`
- `GET https://chatgpt.com/backend-api/usage_state`

Probe requirements:

- Do not print access tokens, session tokens, or full raw response bodies.
- Save no raw response body in the repo.
- Record only a redacted field inventory in a test fixture or plan note, for
  example `rate_limit_reset_credits.available_count: number` and
  `rate_limit_reset_credits.expires_at: timestamp`.
- If either endpoint has a stable reset-credit expiry field, map it to
  `reset_credit_expires_at`.
- If neither endpoint exposes expiry, try the local manual override file
  described below.
- If neither endpoint nor manual config exposes expiry, keep
  `reset_credit_expires_at = None`, add a diagnostic/warning that expiry was not
  present in probed responses, and render hollow/outlined dots as the explicit
  unknown-expiry state.

Manual expiry override:

- Support a small JSON file at `~/.codex/lam/reset-credit-expiry.json`.
- Do not add UI for editing this file in this plan.
- The file is operator-maintained and read-only from LAM's perspective.
- API-provided expiry wins over manual config. Manual config is only a fallback
  when probed/supported APIs do not expose reset-credit expiry.
- Keep the shape simple and profile-scoped:

```json
{
  "profiles": {
    "profile-id-or-email": {
      "resetCreditExpiresAt": "2026-07-12T00:00:00Z",
      "note": "manual expiry from account page"
    }
  }
}
```

- Match by `profile_id` first. If the live quota snapshot only has a display
  name/email for the card, allow that as a fallback key but record a diagnostic
  such as `reset_credit_expiry_manual_key_fallback`.
- Validate only at the file boundary: invalid JSON or invalid timestamp should
  be ignored with a diagnostic; do not fail quota refresh.

For the ChatGPT `wham/usage` fallback, parse reset count only if the payload
has a clear field such as `rate_limit_reset_credits.available_count`,
`rateLimitResetCredits.availableCount`, or `reset_credits.available_count`.
If no expiry field is present, leave `reset_credit_expires_at = None`.

Do not make these endpoints mandatory production dependencies unless the live
response contains stable reset-credit count and expiry fields. If a probe is
kept in production code, it must be behind an explicit function used only after
the supported app-server/wham paths fail, and it must not log token values or
raw response bodies.

**Verify**:

```bash
cd apps/desktop/src-tauri && cargo test quota
```

Expected: exit 0 with tests for:

- app-server `rateLimitResetCredits.availableCount`
- wham-style reset-credit count when present
- no failure when count/expiry are absent
- manual expiry override is used only when API expiry is absent
- invalid manual expiry config does not fail quota refresh
- quota cache round-trip preserving the new fields
- a redacted fixture or parser unit test for whichever probed endpoint contains
  expiry, or a test proving the unknown-expiry warning path when neither does

### Step 7: Display reset-credit dots on account cards

In frontend types, add:

- `resetCreditCount?: number | null`
- `resetCreditExpiresAt?: string | null`
- `resetCreditExpirySource?: 'api' | 'manual_config' | 'unknown' | null`

Update account-card quota display in `apps/desktop/src/routes/views.tsx` and
tray display in `apps/desktop/src/components/tray-quota-panel.tsx` if both use
`UsageQuotaSnapshot`.

Rendering rules:

- Show one small dot per available reset credit, capped at 5 visible dots with
  `+N` text for larger counts.
- Dot color comes from days until `resetCreditExpiresAt` in 6-day phases:
  blue, green, yellow, red, black.
- If count exists but expiry is unknown, use hollow/outlined dots rather than
  grey filled dots.
- If expiry comes from manual config, keep the same phase colors but expose the
  source in title text, for example `2 reset credits; manual expiry 2026-07-12`.
- If count is zero or null, render no dots.
- The account/card name row should carry the dots, per user request.
- Do not add a settings screen, modal, or form for editing manual expiry config.

**Verify**:

```bash
cd apps/desktop && npm test -- --run src/App.handoff.test.tsx src/components/tray-quota-panel.test.tsx src/lib/quota.test.ts src/stores/quota.test.ts
```

Expected: exit 0 with tests for dot count, phase colors, manual-config expiry,
unknown hollow-dot state, and zero-count hidden state.

### Step 8: Final verification and manual smoke

Run:

```bash
cd apps/desktop && npm run build
cd apps/desktop && npm run test:ui
cd apps/desktop/src-tauri && cargo fmt -- --check
cd apps/desktop/src-tauri && cargo clippy -- -D warnings
cd apps/desktop/src-tauri && cargo test
make check
```

Follow-up execution result on 2026-06-29:

- Implemented per-credit reset expiry details and nearest-expiry ordering. Live
  probe confirmed `GET /backend-api/wham/rate-limit-reset-credits` returns
  `credits[].expires_at`; API detail expiry displays in Asia/Shanghai, while
  manual config expiry keeps the operator-provided UTC/RFC3339 text.
- Implemented `Reset quota` through app-server
  `account/rateLimitResetCredit/consume` with per-account locking, persisted
  idempotency key, unknown-outcome retry reuse, and forced quota refresh.
- Added frontend account-card reset action and read-only manual expiry rows.
- Verified with fake app-server success, `noCredit`, and unknown transport
  paths; live app-server `account/rateLimits/read` smoke passed for both main
  and PAT homes without consuming a reset credit.
- Did not run live consume/reset because it would spend a real reset credit.
- Final commands passed: `cargo test quota`, targeted Vitest, full Vitest,
  `npm run build`, `npm run test:ui`, `cargo fmt -- --check`,
  `cargo clippy -- -D warnings`, `cargo test`, and `make check`.

Expected: every command exits 0.

Optional live smoke, only if the operator wants real account verification:

```bash
LAM_HOME="$HOME" make start
```

Expected manually:

- Usage page shows the five headline stats.
- Activity heatmap changes when toggling Calls/Tokens and
  Daily/Weekly/Cumulative.
- Token parity chip shows local-vs-Codex delta instead of silently replacing
  local totals.
- Quota refresh shows reset-credit dots when the backend reports available
  credits.

## Test Plan

Add/extend Rust tests in `apps/desktop/src-tauri/src/services/usage.rs`:

- schema includes `account_usage_snapshot` and
  `account_usage_daily_buckets`.
- local fallback headline stats compute lifetime, peak daily, longest task,
  current streak, and longest streak.
- activity buckets fill missing dates and cumulative values.
- fake `account/usage/read` response overrides headline stats and records
  parity delta.
- token parser accepts valid rows after cumulative reset but still suppresses
  exact duplicates.

Add/extend Rust tests in `apps/desktop/src-tauri/src/services/quota.rs`:

- parse app-server reset-credit count.
- parse wham reset-credit count when present.
- load manual reset-credit expiry only when API expiry is absent.
- ignore invalid manual reset-credit expiry config without failing quota refresh.
- preserve new fields in cache.

Add/extend frontend tests:

- `apps/desktop/src/App.handoff.test.tsx`: Usage page renders headline stats,
  parity chip, and heatmap controls.
- `apps/desktop/src/lib/usage-dashboard.test.ts`: bucket grouping helpers for
  daily/weekly/cumulative modes.
- `apps/desktop/src/lib/quota.test.ts` or
  `apps/desktop/src/components/tray-quota-panel.test.tsx`: reset-credit phase
  color, manual-config source, unknown hollow-dot state, and dot count.

## Done Criteria

All must hold:

- [x] `UsageDashboard` JSON includes `headlineStats` and `activityBuckets`.
- [x] Usage page displays Lifetime tokens, Peak tokens, Longest task, Current
  streak, and Longest streak.
- [x] Heatmap supports Calls/Tokens and Daily/Weekly/Cumulative without a new
  chart dependency.
- [x] `account/usage/read` data, when available, is represented separately from
  local SQLite totals and exposes a token delta.
- [x] Local token parsing no longer drops valid per-call rows merely because a
  cumulative counter reset at a session/turn boundary.
- [x] Quota refresh captures reset-credit count from Codex
  `account/rateLimits/read` and displays dots on account cards.
- [x] `/entitlements` and `/usage_state` are explicitly probed for reset-credit
  expiry; the plan result records either the mapped expiry field or the
  unknown-expiry diagnostic path.
- [x] If API expiry is absent, `~/.codex/lam/reset-credit-expiry.json` can
  provide a manual expiry override without adding any new UI.
- [x] All commands in Step 8 pass.
- [x] `plans/README.md` marks Plan 009 status appropriately. Maintained in the parent workspace index.

## STOP Conditions

Stop and report if:

- The Codex reference project no longer contains `account/usage/read` or the
  `AccountTokenUsageSummary` fields cited above.
- `account/usage/read` requires a new auth flow separate from the existing
  quota app-server path.
- The only way to get reset-credit expiry is to persist raw private response
  bodies.
- Manual reset-credit expiry cannot be keyed to a stable profile identifier in
  the existing quota snapshot.
- Fixing token undercount requires a full rewrite of `parse_source_file` or
  storing raw transcripts.
- The UI change requires adding a charting library.
- Any verification command fails twice after a reasonable scoped fix attempt.

## Maintenance Notes

Reviewers should check that the UI labels distinguish Codex account totals from
LAM local SQLite totals. A higher Codex lifetime value is not automatically a
LAM bug; it may include usage outside local retained JSONL. Future changes to
Codex app-server protocol should update the cited response structs first, then
LAM's parser and fallback diagnostics.

## Execution Notes

- Implemented reset-credit expiry probing for
  `GET https://chatgpt.com/backend-api/entitlements` and
  `GET https://chatgpt.com/backend-api/usage_state` in the `auth-f.json` quota
  path. The probe parses only field-level JSON and never stores raw private
  response bodies.
- Local redacted live probe on 2026-06-29 could not authenticate because
  `~/.codex/auth-f.json` was absent in the executor environment. Therefore no
  API expiry field was confirmed live in this run.
- Unknown expiry remains explicit: quota refresh records the unknown-expiry path
  when probes do not return a stable field, and
  `~/.codex/lam/reset-credit-expiry.json` is supported as the manual fallback.

## Follow-up Extension: Manual reset expiry list and Reset quota

This follow-up implements the reset-credit detail and reset action requested
after the original Plan 009 delivery. It supersedes the original read-only
"do not consume reset credits" boundary only for the explicit `Reset quota`
flow.

Source design note:

- `docs/TAURI2_RUST_CODEX_RESET_EXPIRY_AND_QUOTA_RESET.md` distinguishes quota
  window reset time, manual reset-credit expiry, and subscription expiry.
- Do not derive manual reset-credit expiry from quota window reset,
  subscription expiry, or grant date.
- API detail expiry from `credits[].expiresAt` / `credits[].expires_at` displays
  in Asia/Shanghai. Manual reset expiry keeps the original operator-provided
  UTC/RFC3339 text and is not converted to GMT+8.

Implementation rules:

- Keep aggregate fields `resetCreditCount`, `resetCreditExpiresAt`, and
  `resetCreditExpirySource`.
- Add per-credit details with optional `id`, `status`, `expiresAt`, and
  `source`, plus detail status/error diagnostics.
- Accept detail wrappers named `credits`, `rate_limit_reset_credits.credits`,
  and `rateLimitResetCredits.credits`.
- Sort visible reset-credit rows by nearest valid `expiresAt` first; credits
  without expiry sort last. Label the sorted rows as `Reset 1`, `Reset 2`, etc.
- API detail wins over manual config. Manual config at
  `~/.codex/lam/reset-credit-expiry.json` fills only absent expiry data and has
  no UI editor in this plan.
- Detail endpoint failure, unsupported schema, or invalid manual config must not
  fail quota refresh. Keep aggregate count and render unknown expiry when no
  valid expiry exists.
- Do not log raw token values, raw private response bodies, or full credit IDs.

Reset quota rules:

- Use Codex app-server JSON-RPC
  `account/rateLimitResetCredit/consume` with an idempotency key.
- Treat `reset`, `alreadyRedeemed`, `nothingToReset`, and `noCredit` as resolved
  outcomes, then force refresh quota instead of locally decrementing count.
- Persist one operation UUID per account while pending or outcome is unknown.
- Retry uncertain transport outcomes only with the same UUID.
- Use a per-account lock so concurrent clicks cannot spend two credits.
- Show `Reset quota` only when `resetCreditCount > 0`, require confirmation,
  disable it while pending, and replace UI state with the fresh snapshot.

Follow-up verification:

- Rust tests cover per-credit parser wrappers, API detail conversion to
  Asia/Shanghai, manual UTC preservation, invalid expiry, nearest-expiry
  sorting, detail failure fallback, manual fallback, reset operation UUID reuse,
  and app-server outcomes.
- Frontend tests cover manual expiry rows, sorted rows, no-expiry row ordering,
  reset button disabled/hidden state, confirmation, and state replacement from
  fresh snapshots.
- Final commands:

```bash
cd apps/desktop/src-tauri && cargo test quota
cd apps/desktop && npm test -- --run src/lib/quota.test.ts src/stores/quota.test.ts src/App.handoff.test.tsx
cd apps/desktop && npm run build
cd apps/desktop/src-tauri && cargo fmt -- --check
cd apps/desktop/src-tauri && cargo clippy -- -D warnings
make check
```
