# Todo: Usage insights, pagination, and rate card

> Executor instructions: Follow this todo step by step. Generate tests from
> the "Test design" section before implementation. Run each verification command
> and confirm the expected result before moving to the next task.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: existing Usage SQLite index
- **Category**: bugfix/refactor

## Why this matters

**Background**: Usage Insights derived values from loaded call/thread rows, and Calls/Threads used SQL `LIMIT` as truncation rather than a list pagination contract. Settings also did not expose the built-in Usage price table, while the frontend carried a stale duplicate estimator.

**Current state**: The work now exposes backend aggregate Insights, paged Calls/Threads contracts, and a backend-owned rate card for Settings.

**Impact**: Without this, Insight totals can display partial values such as `Total threads = 50`, large tabs cannot page, and price display can drift across frontend/backend implementations.

**What improves**: Insights is a true aggregate; Calls/Threads have explicit page metadata; Settings shows the current built-in price table; frontend no longer estimates costs from a duplicate table.

## Scope

**In scope**:
- Usage Rust service contracts and SQL queries.
- Usage TS types, API calls, store merge behavior, and page rendering logic.
- Settings display of the backend built-in Usage rate card.
- Focused Rust and React/Zustand tests.

**Out of scope**:
- Visual redesign or CSS changes.
- Remote publishing.
- Repricing or claiming official billing accuracy.

## Design

- Add `UsageInsights` returned from `get_usage_insights`.
- Add paged response wrappers for Calls and Threads: `{ rows, total, nextOffset, limit, offset }`.
- Extend `UsageDashboardRequest` with `offset`.
- Make Calls return filtered `COUNT(*)` and current page rows.
- Read Threads from `thread_summaries`; add workspace/account columns so scope filters remain correct.
- Load Insights independently of Calls/Threads rows.
- Expose `get_usage_rate_card` from the backend and remove frontend cost estimation rates.

## Tasks

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Backend aggregate and pagination contracts | Rust tests prove insights are full aggregates and calls/threads return paged metadata | 验证成功 |
| T2 | Frontend store/API/page wiring | React/store tests prove insights do not use list lengths and lists show page metadata | 验证成功 |
| T3 | Settings rate card and single price authority | Settings shows backend rate card; frontend no longer estimates costs from a duplicate table | 验证成功 |
| T4 | Migration idempotency and rate card readability | Concurrent/duplicate column migrations do not break Usage; Settings table is not trapped in `.rows` grid | 验证成功 |
| T5 | Section refresh indexes before querying | Usage Refresh writes new Codex jsonl data into SQLite before refreshing current section | 验证成功 |
| T6 | Thread summaries are scoped | Usage index refresh succeeds when different workspaces share the same thread key | 验证成功 |
| T7 | Visible Usage auto sync | Usage page auto indexes every two minutes while preserving manual Refresh | 验证成功 |

### T1: Backend aggregate and pagination contracts

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Backend must expose correct aggregate and paged contracts so UI cannot confuse truncated lists with totals.

**What to do**:
- Add Rust response structs for insights and paged rows.
- Add `offset` to `UsageDashboardRequest`.
- Implement `get_usage_insights`.
- Change `get_usage_calls` and `get_usage_threads` to return paged responses.
- Add workspace/account columns to `thread_summaries` and rebuild query.

**Logic design**:
- Summary filters apply to all aggregate/list queries.
- Search/model/effort/pricing filters apply to Calls.
- Threads supports scope/window/archive/search/sort/offset/limit from the materialized table.
- `nextOffset` is present only if more rows exist.
- Thread summaries are grouped by scope fields to avoid cross-workspace collisions.

**Test design**:
- Rust test: insights returns total thread count greater than the page limit and computes reasoning/skills from all matching rows.
- Rust test: calls page 1/page 2 returns different rows with `total` and `nextOffset`.
- Rust test: threads reads materialized summaries with scope filters and pagination.

**Acceptance**:
- `cd apps/desktop/src-tauri && cargo test usage::tests`

**Done criteria**:
- [x] Tests were written before implementation
- [x] New tests failed for expected missing contract
- [x] Implementation follows the logic design
- [x] Focused verification passes
- [x] Relevant suite passes
- [x] This task status is `验证成功`

### T2: Frontend store/API/page wiring

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Frontend must consume aggregate and paged contracts without blocking tab switching or deriving totals from truncated rows.

**What to do**:
- Update TS types and API wrappers.
- Add `insights` section loading in the store.
- Store paged calls/threads metadata.
- Render Insights from `summary.insights`.
- Render Calls/Threads current page ranges from backend page metadata.

**Logic design**:
- Insights tab loads `insights` independently.
- Calls/Threads requests include `offset`.
- Existing styles/classes stay unchanged.

**Test design**:
- Store test: `loadUsageSection('insights')` calls `getUsageInsights`, not Calls/Threads.
- Page test: Insights uses aggregate values even when `recentCalls/topThreads` are empty/truncated.
- Page test: Calls/Threads notices use backend `total`, not loaded row count.

**Acceptance**:
- `cd apps/desktop && npm test -- src/stores/usage.test.ts src/routes/usage.test.tsx src/App.handoff.test.tsx src/lib/usage-dashboard.test.ts`
- `cd apps/desktop && npx tsc --noEmit`

**Done criteria**:
- [x] Tests were written before implementation
- [x] New tests failed for expected missing contract
- [x] Implementation follows the logic design
- [x] Focused verification passes
- [x] Relevant suite passes
- [x] This task status is `验证成功`

### T3: Settings rate card and single price authority

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Users need to inspect the built-in prices that drive Usage cost estimates. Keeping a second frontend price table risks showing or computing stale values.

**What to do**:
- Expose backend built-in rate card through a read-only Tauri command.
- Load and render the rate card in Settings.
- Remove frontend `estimateUsageCost` and keep only `formatCost`.

**Logic design**:
- Backend remains the authority for prices and cost estimation.
- Settings displays model, context window, input, cached input, output, pricing model, and estimated marker.
- No CSS/style changes are required.

**Test design**:
- Rust test: backend rate card exposes representative built-in entries and estimated mapping.
- App test: Settings renders a backend-provided rate card entry.
- Lib test: frontend pricing module only formats backend-estimated values.

**Acceptance**:
- `cd apps/desktop/src-tauri && cargo test usage::tests`
- `cd apps/desktop && npm test -- src/stores/usage.test.ts src/routes/usage.test.tsx src/App.handoff.test.tsx src/lib/usage-dashboard.test.ts`
- `cd apps/desktop && npx tsc --noEmit`

**Done criteria**:
- [x] Tests were written before implementation
- [x] New tests failed for expected missing contract
- [x] Implementation follows the logic design
- [x] Focused verification passes
- [x] Relevant suite passes
- [x] This task status is `验证成功`

### T4: Migration idempotency and rate card readability

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Usage sections load independently and can initialize the SQLite schema concurrently. A duplicate `ALTER TABLE ... ADD COLUMN workspace_id` must be treated as an idempotent migration outcome. The Settings rate card also rendered inside `.rows`, whose descendant `div` rule compressed the table.

**What to do**:
- Make `ensure_column` tolerate duplicate-column races.
- Keep normal migration failures visible.
- Move the Settings rate card table outside the `.rows` grid while preserving existing table styles.

**Logic design**:
- If the column exists before `ALTER`, skip it.
- If a concurrent initializer adds the column between the check and `ALTER`, ignore only the duplicate-column error.
- Settings keeps the existing rows for scalar settings, then renders the rate card as a separate `usageSection`.

**Test design**:
- Rust test: a simulated stale column check that hits duplicate-column on `ALTER` returns success.
- App test: Settings rate card wrapper is not nested inside `.rows`, preventing the global rows grid from applying to it.

**Acceptance**:
- `cd apps/desktop/src-tauri && cargo test usage::tests`
- `cd apps/desktop && npm test -- src/App.handoff.test.tsx`
- `cd apps/desktop && npx tsc --noEmit`

**Done criteria**:
- [x] Tests were written before implementation
- [x] New tests failed for expected missing contract
- [x] Implementation follows the logic design
- [x] Focused verification passes
- [x] Relevant suite passes
- [x] This task status is `验证成功`

### T5: Section refresh indexes before querying

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Usage section refresh was changed to reload only the current section. That preserved UI interactivity, but it skipped `refresh_usage_index`, so newly appended Codex jsonl records stayed outside SQLite and cards appeared stale.

**What to do**:
- Make `refreshUsageSection` call `refreshUsageIndex` before loading the requested section.
- Preserve section-level loading behavior.
- Avoid indexing multiple times when the Insights refresh reloads insights, overview, and activity together.

**Logic design**:
- Add a store-level `refreshUsageSections(sections, req)` operation.
- It sets `refreshing`, runs one `refreshUsageIndex(req.includeArchived)`, then loads each requested section.
- Existing `refreshUsageSection` delegates to the batch operation for one section.

**Test design**:
- Store test: refreshing Calls invokes `refreshUsageIndex` before `getUsageCalls`.
- Store test: refreshing Insights group indexes once, then loads insights/overview/activity.

**Acceptance**:
- `cd apps/desktop && npm test -- src/stores/usage.test.ts src/routes/usage.test.tsx`
- `cd apps/desktop && npx tsc --noEmit`

**Done criteria**:
- [x] Tests were written before implementation
- [x] New tests failed for expected missing contract
- [x] Implementation follows the logic design
- [x] Focused verification passes
- [x] Relevant suite passes
- [x] This task status is `验证成功`

### T6: Thread summaries are scoped

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Real home refresh failed with `UNIQUE constraint failed: thread_summaries.thread_key`. Thread summaries are grouped by workspace/account, but the materialized table primary key remained only `thread_key`.

**What to do**:
- Make materialized thread summary keys unique per workspace/account.
- Preserve scoped thread queries and pagination.
- Keep refresh idempotent for existing databases.

**Logic design**:
- Store a scoped key in `thread_summaries.thread_key` by prefixing workspace/account around the base thread key.
- Keep `thread_label` as the user-facing thread name.
- Existing queries continue filtering by workspace/account and ordering by the stored key.

**Test design**:
- Rust test: create two workspaces with the same session/thread key, refresh succeeds, and each scoped query returns its own thread summary.

**Acceptance**:
- `cd apps/desktop/src-tauri && cargo test usage::tests`
- `cd apps/desktop/src-tauri && cargo test usage::tests::real_home_usage_smoke -- --ignored --nocapture`

**Done criteria**:
- [x] Tests were written before implementation
- [x] New tests failed for expected missing contract
- [x] Implementation follows the logic design
- [x] Focused verification passes
- [x] Relevant suite passes
- [x] This task status is `验证成功`

### T7: Visible Usage auto sync

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Manual-only Usage refresh makes fresh Codex usage look stale. A low-frequency background sync improves freshness without running constantly outside Usage.

**What to do**:
- Add a 120-second interval while `UsagePage` is mounted.
- Reuse the same `refreshUsageSections` path as manual Refresh.
- Skip a tick while a refresh or active section load is already running.
- Keep the manual Refresh button unchanged.

**Logic design**:
- Usage page owns the timer so route unmount clears it.
- The timer refreshes the current tab's active section set.
- Insights refreshes `insights`, `overview`, and `activity`; other tabs refresh only their active section.

**Test design**:
- Page test: after 120 seconds, auto sync calls `refreshUsageSections` for the active tab.
- Page test: auto sync skips while `refreshing=true`.

**Acceptance**:
- `cd apps/desktop && npm test -- src/routes/usage.test.tsx src/stores/usage.test.ts`
- `cd apps/desktop && npx tsc --noEmit`

**Done criteria**:
- [x] Tests were written before implementation
- [x] New tests failed for expected missing contract
- [x] Implementation follows the logic design
- [x] Focused verification passes
- [x] Relevant suite passes
- [x] This task status is `验证成功`

## Verification commands

| Purpose | Command | Expected |
|---------|---------|----------|
| Rust suite | `cd apps/desktop/src-tauri && cargo test usage::tests` | pass |
| Frontend focused | `cd apps/desktop && npm test -- src/stores/usage.test.ts src/routes/usage.test.tsx src/App.handoff.test.tsx src/lib/usage-dashboard.test.ts` | pass |
| Typecheck | `cd apps/desktop && npx tsc --noEmit` | pass |

## Done criteria

- [x] Every task is `验证成功`.
- [x] No style-only files were changed.
- [x] Calls and Threads expose explicit pagination metadata.
- [x] Insights is independent from loaded list rows.
- [x] Settings shows the backend built-in Usage rate card.
- [x] Frontend no longer contains a duplicate cost estimator.
