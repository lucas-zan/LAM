# Usage Workspace and Account Attribution Design

## Goal

LAM usage should report two related but different views:

- **Workspace usage**: exact local usage grouped by real `CODEX_HOME` directories.
- **Account-attributed usage**: usage attributed to an account when LAM can observe the account context.

This design assumes users follow LAM-managed workflows. Manual edits to `auth.json`, manual token switching, or direct unmanaged Codex launch paths are out of scope.

## Current Code Facts

- Profile accounts already use wrapper commands. `execute_create_account` writes `~/bin/codex-{name}` and `wrapper_script` sets `CODEX_HOME="$HOME/.codex-{name}"`.
- PAT mode switches are LAM controlled. Main window and tray call `switch_to_pat_account`, then restart Codex.
- `switch_to_pat_account` copies the selected profile `auth.json` into `~/.codex/auth.json`.
- Usage currently reads only `home_root/.codex/sessions` and optional `home_root/.codex/archived_sessions`.
- Usage events already contain `event_timestamp`, `source_file`, `session_id`, token fields, model, cwd, and cost estimate fields.

## Product Semantics

### Workspace Usage

Workspace usage answers:

> How much usage happened in this local Codex workspace directory?

Scopes:

- `Total`: all discovered Codex workspaces, de-duplicated by real path.
- `main`: `~/.codex`
- `codex-c`: `~/.codex-c`
- `codex-luna002`: `~/.codex-luna002`

This is always accurate for local files. It does not claim billing-account attribution.

### Account-Attributed Usage

Account-attributed usage answers:

> Which LAM account should this usage be attributed to, based on LAM-observed context?

Sources:

- `profile_workspace`: event came from an isolated profile workspace.
- `pat_timeline`: event came from shared `~/.codex` during a LAM-recorded PAT active-account window.
- `unknown`: LAM cannot attribute the event.

When a user follows LAM workflows, profile attribution is exact and PAT attribution is reliable for the recorded active windows.

## Data Model

### Usage Scope

Frontend should not hard-code PAT/Profile behavior. Backend returns scopes derived from local workspaces.

```ts
type UsageScope = {
  id: string;              // total | workspace:main | workspace:c
  label: string;           // Total | main | codex-c
  kind: 'total' | 'workspace';
  accountId?: string | null;
  codexHome?: string | null;
  isDefault: boolean;
};
```

### Usage Request

```ts
type UsageDashboardRequest = {
  scopeId?: string | null;
  accountId?: string | null; // optional account-attribution filter
  window: UsageWindow;
  includeArchived: boolean;
  search?: string | null;
  model?: string | null;
  effort?: string | null;
  pricingConfidence?: string | null;
  sortKey?: string;
  sortDirection?: 'asc' | 'desc';
  limit?: number | null;
};
```

Rules:

- Missing `scopeId` means backend default, usually `total` if multiple workspaces exist, otherwise the only workspace.
- `accountId` is optional and filters attributed account usage.
- Workspace scopes and account attribution filters are separate dimensions.

### Usage Response

```ts
type UsageDashboardResponse = {
  scopes: UsageScope[];
  activeScopeId: string;
  dashboard: UsageDashboard;
};
```

`UsageDashboard` keeps the current metrics and adds scope metadata:

```ts
type UsageDashboard = {
  scope: UsageScope;
  // existing dashboard fields...
};
```

### Event Attribution Columns

Add columns to `usage_events`:

```sql
workspace_id TEXT;
workspace_label TEXT;
workspace_home TEXT;
attributed_account_id TEXT;
attributed_account_label TEXT;
attribution_source TEXT NOT NULL DEFAULT 'unknown';
```

Recommended indexes:

```sql
CREATE INDEX IF NOT EXISTS idx_usage_events_workspace ON usage_events(workspace_id);
CREATE INDEX IF NOT EXISTS idx_usage_events_attribution ON usage_events(attributed_account_id, attribution_source);
```

### Source File Columns

Add columns to `source_files`:

```sql
workspace_id TEXT;
workspace_home TEXT;
```

This keeps incremental parsing scoped to the actual source workspace.

### PAT Timeline Store

Persist PAT active-account windows in LAM-owned storage:

```txt
~/.config/agent-workspace/pat-usage-timeline.json
```

Schema:

```json
{
  "version": 1,
  "events": [
    {
      "id": "uuid-or-timestamp",
      "startedAt": "2026-07-03T10:20:00+08:00",
      "endedAt": null,
      "accountId": "c",
      "accountLabel": "codex-c",
      "workspaceId": "workspace:main",
      "workspaceHome": "/Users/name/.codex",
      "source": "switch_to_pat_account"
    }
  ]
}
```

On each PAT switch:

1. Close the previous open event by setting `endedAt`.
2. Append a new open event for the selected account.
3. Continue copying auth and restarting Codex.

## Backend Implementation Plan

### 1. Discover Usage Workspaces

Add a helper in `usage.rs`:

```rust
struct UsageWorkspace {
    id: String,
    label: String,
    account_id: Option<String>,
    codex_home: PathBuf,
}
```

Discovery:

- Reuse `list_accounts(home_root)` or a lower-level scanner to find real Codex homes.
- De-duplicate by canonical path.
- Include `main` for `home_root/.codex` when it has Codex signals or session files.
- Sort `main` first, then account id/name.

### 2. Refresh All Workspaces

Replace the current fixed:

```rust
let codex_home = home_root.join(".codex");
let logs = find_session_logs(&codex_home, include_archived)?;
```

with workspace iteration:

```rust
for workspace in usage_workspaces(home_root)? {
    let session_index = load_session_index(&workspace.codex_home);
    let logs = find_session_logs(&workspace.codex_home, include_archived)?;
    ...
}
```

Each `SourceParsePlan` should carry workspace metadata so parsed events are enriched before insert.

### 3. Attribute Parsed Events

For each parsed token event:

- Always set workspace fields from the source workspace.
- If workspace maps to an isolated profile, set:
  - `attributed_account_id = account_id`
  - `attribution_source = 'profile_workspace'`
- If workspace is `main` / shared PAT workspace, match `event_timestamp` against the PAT timeline:
  - match found: `attributed_account_id = timeline.accountId`, `attribution_source = 'pat_timeline'`
  - no match: `attribution_source = 'unknown'`

Timestamp matching:

- Treat `startedAt <= event_timestamp < endedAt`.
- Open-ended events match through current time.
- Store and compare RFC3339 timestamps with offsets parsed to UTC.

### 4. Query by Scope

Add scope filtering to summary queries:

- `total`: no workspace filter.
- `workspace:*`: `WHERE workspace_id = ?`.

All existing summary, dashboard, top threads, recent calls, activity buckets, diagnostics, and model totals queries must accept the same scope filter.

### 5. Query by Account Attribution

Optional second filter:

- `accountId` present: `WHERE attributed_account_id = ?`.
- Account-attributed totals should show confidence/source breakdown:
  - profile workspace total
  - PAT timeline total
  - unknown excluded by default

This can be a follow-up if scope tabs are implemented first.

### 6. Keep Existing PAT Behavior

Do not remove existing PAT switch semantics:

- Still copy auth to `~/.codex/auth.json`.
- Still restart Codex.
- Add timeline recording inside `switch_to_pat_account` after auth verification succeeds.

If timeline recording fails, return an error. Since users rely on attribution, switching without recording would create silent incorrect stats.

## Frontend Implementation Plan

### 1. Usage Scope Tabs

Usage page should render tabs from backend `scopes`.

Rules:

- If `scopes.length <= 1`, hide tabs.
- Default scope is `response.activeScopeId`.
- Clicking a tab requests the same dashboard with `scopeId`.

Tabs:

```txt
Total | main | codex-c | codex-luna002
```

### 2. Preserve Current Dashboard Fields

The dashboard UI should use the same metrics for every scope:

- headline totals
- cost estimate
- cache metrics
- heatmap
- top threads
- recent calls
- diagnostics

### 3. Add Workspace/Account Columns Where Useful

For `Total`, recent calls and top threads should show a compact `Workspace` column.

When account attribution is enabled, add:

- `Account`
- `Attribution`

For single workspace scope, these columns can be hidden or shown as chips.

### 4. Naming

Use workspace wording:

- `Total`
- `Workspace`
- `Profile workspace`
- `Shared workspace`

Avoid implying official billing:

- Use `Estimated cost`
- Add tooltip: `Estimated from local session logs, not official billing.`

## Migration Strategy

1. Add nullable columns to existing SQLite tables.
2. Existing rows without workspace metadata should be backfilled on next refresh:
   - Determine workspace by `source_file` path prefix.
   - If no match, mark `workspace_id = 'unknown'`, `attribution_source = 'unknown'`.
3. Full refresh can repair old rows by re-parsing source files if needed.
4. Do not delete existing usage DB automatically.

## Test Plan

### Rust Tests

- Workspace discovery returns `main` and managed profiles, de-duplicated.
- Refresh indexes events from `~/.codex` and `~/.codex-c`.
- `Total` aggregates both workspaces.
- `workspace:c` returns only `~/.codex-c` usage.
- PAT switch records a closed previous timeline event and an open new event.
- Shared workspace events are attributed via PAT timeline.
- Events outside timeline are marked `unknown`.
- Migration adds workspace and attribution columns to old DBs.

### Frontend Tests

- Usage renders no tabs when backend returns one scope.
- Usage renders `Total` and workspace tabs when backend returns multiple scopes.
- Clicking a scope tab reloads dashboard with that `scopeId`.
- Dashboard fields render identically across scopes.
- `Total` recent calls show workspace labels.

## Acceptance Criteria

- Current local-only usage still works when only `~/.codex` exists.
- Multiple profile workspaces produce `Total | main | codex-*` tabs.
- PAT switch continues to work and records active-account timeline.
- Workspace totals are exact for local files.
- Account attribution is explicit about source: profile workspace, PAT timeline, or unknown.
- No UI labels claim official billing accuracy.

## Recommended Delivery Order

1. Add backend workspace metadata columns and workspace discovery.
2. Make refresh parse all workspace directories.
3. Add scoped dashboard response and frontend scope tabs.
4. Add PAT timeline recording.
5. Add attribution fields and account-attributed filters.
6. Polish UI columns and explanatory tooltips.

This order keeps the first milestone useful even before PAT attribution is complete: workspace `Total` becomes correct immediately.

## Development Execution Specification

This section is intended as the implementation contract for the next development pass.

### Phase 1: API Contract

#### TypeScript contract

Update `apps/desktop/src/lib/types.ts`.

Add:

```ts
export type UsageScopeKind = 'total' | 'workspace';

export type UsageScope = {
  id: string;
  label: string;
  kind: UsageScopeKind;
  accountId?: string | null;
  codexHome?: string | null;
  isDefault: boolean;
};

export type UsageDashboardResponse = {
  scopes: UsageScope[];
  activeScopeId: string;
  dashboard: UsageDashboard;
};
```

Extend:

```ts
export type UsageDashboardRequest = {
  scopeId?: string | null;
  accountId?: string | null;
  // existing fields stay unchanged
};

export type UsageDashboard = {
  scope?: UsageScope | null;
  // existing fields stay unchanged
};

export type UsageCallRow = {
  workspaceId?: string | null;
  workspaceLabel?: string | null;
  workspaceHome?: string | null;
  attributedAccountId?: string | null;
  attributedAccountLabel?: string | null;
  attributionSource?: 'profile_workspace' | 'pat_timeline' | 'unknown' | null;
  // existing fields stay unchanged
};
```

Add an invoke wrapper:

```ts
export async function getUsageDashboardResponse(
  req: UsageDashboardRequest,
): Promise<UsageDashboardResponse> {
  if (!inTauri()) {
    return {
      scopes: [{ id: 'total', label: 'Total', kind: 'total', isDefault: true }],
      activeScopeId: 'total',
      dashboard: emptyUsageDashboard(),
    };
  }
  return invoke<UsageDashboardResponse>('get_usage_dashboard_response', { req });
}
```

Compatibility:

- Keep current `getUsageDashboard(req): Promise<UsageDashboard>` during migration.
- Internally it may call `get_usage_dashboard_response` and return `.dashboard`.
- Existing tests that mock `getUsageDashboard` should continue to work until the frontend switches over.

#### Rust contract

Update `apps/desktop/src-tauri/src/services/usage.rs`.

Add serde structs:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UsageScope {
    pub id: String,
    pub label: String,
    pub kind: String, // "total" | "workspace"
    pub account_id: Option<String>,
    pub codex_home: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDashboardResponse {
    pub scopes: Vec<UsageScope>,
    pub active_scope_id: String,
    pub dashboard: UsageDashboard,
}
```

Extend existing request:

```rust
pub struct UsageDashboardRequest {
    pub scope_id: Option<String>,
    pub account_id: Option<String>,
    // existing fields stay unchanged
}
```

Extend `UsageCallRow` and `UsageDashboard` with the fields listed in the TypeScript contract.

Add command:

```rust
#[tauri::command]
pub async fn get_usage_dashboard_response(
    req: UsageDashboardRequest,
) -> Result<UsageDashboardResponse, AppError>
```

Keep existing `get_usage_dashboard` command as compatibility:

```rust
pub fn get_usage_dashboard(home_root: &Path, req: UsageDashboardRequest) -> Result<UsageDashboard> {
    Ok(get_usage_dashboard_response(home_root, req)?.dashboard)
}
```

### Phase 2: Workspace Discovery

Add to `usage.rs`:

```rust
#[derive(Debug, Clone)]
struct UsageWorkspace {
    id: String,
    label: String,
    account_id: Option<String>,
    codex_home: PathBuf,
}
```

Function:

```rust
fn discover_usage_workspaces(home_root: &Path) -> Result<Vec<UsageWorkspace>>
```

Algorithm:

1. Call `list_accounts(home_root)?`.
2. For each account:
   - use `account.codex_home`
   - skip if the path does not exist
   - include if it has `sessions`, `archived_sessions`, `session_index.jsonl`, `auth.json`, `config.toml`, or existing Codex signals already accepted by account scanning
3. Canonicalize paths where possible.
4. De-duplicate by canonical path.
5. Build IDs:
   - `workspace:main` for account id `main`
   - `workspace:<account.id>` for others
6. Labels:
   - `main`
   - `account.display_name` for others
7. Sort:
   - main first
   - then by label

Scope helper:

```rust
fn usage_scopes(workspaces: &[UsageWorkspace]) -> Vec<UsageScope>
```

Rules:

- If `workspaces.len() > 1`, include `Total` first:
  - `id = "total"`
  - `kind = "total"`
  - `is_default = true`
- If only one workspace, make that workspace scope default and omit `Total`.
- For multiple workspaces, workspace scopes have `is_default = false`.

Active scope resolver:

```rust
fn resolve_usage_scope<'a>(
    requested: Option<&str>,
    scopes: &'a [UsageScope],
) -> &'a UsageScope
```

Rules:

- Requested matching scope wins.
- Otherwise `is_default` wins.
- Otherwise first scope wins.

### Phase 3: SQLite Migration

Update `init_usage_db`.

For `usage_events`, add nullable/default columns through existing `ensure_columns` pattern:

```rust
("workspace_id", "TEXT"),
("workspace_label", "TEXT"),
("workspace_home", "TEXT"),
("attributed_account_id", "TEXT"),
("attributed_account_label", "TEXT"),
("attribution_source", "TEXT NOT NULL DEFAULT 'unknown'")
```

For `source_files`:

```rust
("workspace_id", "TEXT"),
("workspace_home", "TEXT")
```

Indexes:

```sql
CREATE INDEX IF NOT EXISTS idx_usage_events_workspace
ON usage_events(workspace_id);

CREATE INDEX IF NOT EXISTS idx_usage_events_attribution
ON usage_events(attributed_account_id, attribution_source);
```

Backfill helper:

```rust
fn backfill_usage_workspace_metadata(
    conn: &Connection,
    workspaces: &[UsageWorkspace],
) -> Result<()>
```

Behavior:

- For rows with `workspace_id IS NULL`, match `source_file` prefix against workspace `codex_home`.
- Update workspace fields.
- If workspace maps to a profile account, set `attribution_source = 'profile_workspace'` and `attributed_account_id`.
- If no match, leave `workspace_id = 'unknown'` and `attribution_source = 'unknown'`.

Run after `init_usage_db` and before dashboard queries. Refresh can perform stronger metadata assignment during insert.

### Phase 4: Refresh Pipeline

Current refresh parses only `home_root/.codex`. Replace with workspace iteration.

Modify `SourceLog`:

```rust
struct SourceLog {
    path: PathBuf,
    is_archived: bool,
    workspace_id: String,
    workspace_label: String,
    workspace_home: PathBuf,
    account_id: Option<String>,
}
```

Modify `find_session_logs`:

```rust
fn find_session_logs(workspace: &UsageWorkspace, include_archived: bool) -> Result<Vec<SourceLog>>
```

Modify `SourceParsePlan` to carry `SourceLog` metadata, or keep `SourceLog` inside the plan.

Refresh algorithm:

```rust
let workspaces = discover_usage_workspaces(home_root)?;
for workspace in &workspaces {
    let session_index = load_session_index(&workspace.codex_home);
    let logs = find_session_logs(workspace, include_archived)?;
    let plans = source_logs_requiring_parse(&conn, &logs)?;
    for plan in plans {
        let parsed = parse_source_file(&plan, &session_index)?;
        ...
    }
}
```

`source_logs_requiring_parse` must include workspace columns when reading/writing `source_files`.

Event enrichment:

Before `apply_parsed_sources`, each `UsageCallRow` should include:

- workspace fields from `SourceLog`
- profile attribution when `SourceLog.account_id` exists and workspace is not shared
- PAT attribution when source workspace is `workspace:main` and timeline matches

### Phase 5: PAT Timeline

Add timeline types in `account.rs` or a new `usage_attribution.rs` module:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatUsageTimeline {
    version: u32,
    events: Vec<PatUsageTimelineEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatUsageTimelineEvent {
    id: String,
    started_at: String,
    ended_at: Option<String>,
    account_id: String,
    account_label: String,
    workspace_id: String,
    workspace_home: String,
    source: String,
}
```

Path:

```rust
fn pat_usage_timeline_path(home_root: &Path) -> PathBuf {
    config_root(home_root).join("pat-usage-timeline.json")
}
```

Function:

```rust
fn record_pat_usage_switch(home_root: &Path, account: &CodexAccount) -> Result<()>
```

Called from `switch_to_pat_account` only after auth copy verification succeeds.

Rules:

- Load existing file or create version 1.
- Set `endedAt` on the current open event.
- Append a new event:
  - `startedAt = now RFC3339 with local or UTC offset`
  - `accountId = account.id`
  - `accountLabel = account.display_name`
  - `workspaceId = "workspace:main"`
  - `workspaceHome = home_root/.codex`
  - `source = "switch_to_pat_account"`
- Write atomically with private permissions.

Attribution lookup:

```rust
fn find_pat_attribution(
    timeline: &PatUsageTimeline,
    event_timestamp: &str,
) -> Option<PatUsageTimelineEvent>
```

Parse timestamps as RFC3339. If event timestamp cannot be parsed, return `None`.

### Phase 6: Query Filter Design

Introduce:

```rust
struct UsageQueryFilter {
    include_archived: bool,
    from: Option<String>,
    to: Option<String>,
    scope_id: Option<String>,
    account_id: Option<String>,
}
```

SQL predicate shape:

```sql
WHERE (?1 OR is_archived = 0)
  AND (?2 IS NULL OR event_timestamp >= ?2)
  AND (?3 IS NULL OR event_timestamp < ?3)
  AND (?4 IS NULL OR workspace_id = ?4)
  AND (?5 IS NULL OR attributed_account_id = ?5)
```

For `total`, pass `NULL` as workspace filter.

Every query that currently uses `SummaryFilter` must include the scope/account filter:

- totals query in `get_usage_summary`
- `query_top_threads`
- `query_recent_calls`
- `query_activity_buckets`
- `model_totals`
- `estimate_thread_cost`
- `usage_diagnostics`
- headline stats queries

Avoid building ad hoc SQL strings where possible. Prefer fixed predicates and nullable params for consistency.

### Phase 7: Frontend Integration

Update `apps/desktop/src/stores/usage.ts`.

State:

```ts
interface UsageState {
  summary: UsageDashboard | null;
  scopes: UsageScope[];
  activeScopeId: string | null;
  refreshing: boolean;
  loadUsageSummary: (req?: UsageDashboardRequest) => Promise<void>;
  refreshUsage: (req?: UsageDashboardRequest) => Promise<void>;
}
```

Behavior:

- `loadUsageSummary` calls `getUsageDashboardResponse`.
- Store `response.scopes`, `response.activeScopeId`, and `response.dashboard`.
- `refreshUsage` calls `refreshUsageIndex`, then `getUsageDashboardResponse`.

Update `apps/desktop/src/App.tsx`.

- Remove `authMode === 'pat'` gate for usage loading.
- Include `scopeId: activeScopeId` in `usageRequest` after scopes load.
- Usage route should load in all modes.

Update `apps/desktop/src/routes/usage.tsx`.

Props:

```ts
scopes: UsageScope[];
activeScopeId: string | null;
setUsageScope: (scopeId: string) => void;
```

Rendering:

- Remove `authMode !== 'pat'` empty state.
- If `scopes.length > 1`, render tabs above filters.
- On tab click, set selected scope and reload.
- Show dashboard scope in header:
  - `Total workspace usage`
  - `main workspace usage`

Tables:

- In `Total`, show `Workspace` column for recent calls.
- In single workspace, hide `Workspace` column unless useful for debugging.
- Keep all existing metric cards and filters.

### Phase 8: Compatibility and Rollout

Keep these unchanged until all frontend call sites migrate:

- `refresh_usage_index(include_archived)`
- `get_usage_dashboard(req) -> UsageDashboard`
- `getUsageDashboard(req)`

Add new API beside them:

- `get_usage_dashboard_response(req) -> UsageDashboardResponse`
- `getUsageDashboardResponse(req)`

Once usage page uses the response API and tests pass, old `getUsageDashboard` can remain as a compatibility wrapper.

### Phase 9: Verification Matrix

Focused backend commands:

```bash
cargo test usage_workspace
cargo test pat_usage_timeline
cargo test usage_scope
```

Focused frontend commands:

```bash
npm test -- --run src/App.handoff.test.tsx src/routes/usage.test.tsx src/stores/usage.test.ts
npm test -- --run src/lib/usage-dashboard.test.ts
```

General checks:

```bash
npm run lint
npm run build
cargo test usage
```

Manual acceptance:

1. With only `~/.codex`, Usage opens without PAT mode and shows one dashboard with no tabs.
2. With `~/.codex` and `~/.codex-c`, Usage shows `Total | main | codex-c`.
3. `Total` equals the sum of workspace rows for local files.
4. `codex-c` tab only shows `~/.codex-c` events.
5. PAT switch from `codex-c` to `codex-luna002` records timeline events.
6. New shared `~/.codex` usage after switch is attributed to the selected account.
7. Rows without attribution are marked `unknown`, not silently assigned.

### Phase 10: STOP Conditions

Stop implementation and revisit design if:

- Codex JSONL no longer contains reliable `timestamp` on token events.
- Account scanning misses real `CODEX_HOME` directories needed for usage.
- Existing usage DB migration cannot preserve current rows.
- PAT switch can succeed while timeline write fails.
- Query performance becomes unacceptable on existing user-scale session logs.
