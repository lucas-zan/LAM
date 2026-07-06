# Todo: Usage Dashboard Partitioned Loading

> Executor instructions: Follow this todo step by step. Generate tests from the Test design section before implementation. Keep this work scoped to usage dashboard query/loading performance, not pricing logic.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: `docs/usage-workspace-account-attribution-design.md`
- **Category**: refactor/performance
- **Planned at**: current dirty LAM working tree

## Why this matters

**Background**: The Usage page currently waits on a single large dashboard response. With tens of thousands of calls, normal viewing and filtering feel slow even though only one panel may need data.

**Current state**: `getUsageDashboardResponse` returns scopes, aggregate overview, activity buckets, top threads, recent calls, diagnostics, and filter options in one response. Several filters are applied after fetching rows, and `refreshUsage` rebuilds the index before reloading the entire page.

**Impact**: First paint is blocked by the slowest query. Chart/cards/table refreshes block each other. Filter changes can fetch far more rows than needed.

**What improves**: The page renders immediately, each panel has its own loading state, ordinary queries use SQL filters/limits, and refresh actions update the relevant panel instead of rebuilding all data blindly.

## Scope

**In scope**:
- Usage Rust service/query functions and Tauri commands.
- Desktop TypeScript API wrappers and usage store.
- Usage page loading/refresh UI states.
- Focused tests for sectioned loading and query downshift.

**Out of scope**:
- Price/rate-card changes.
- Remote sync or cloud APIs.
- Changing the parser/index refresh algorithm beyond adding indexes.

## Design

- Keep the existing full dashboard API for compatibility, but introduce section APIs:
  - scopes response
  - overview response
  - activity buckets response
  - calls page response
  - threads page response
  - diagnostics response
- Convert dashboard filters into a shared backend filter plan and apply `search`, `model`, `effort`, `pricingConfidence`, `sort`, and `limit` inside SQL for calls and option lists.
- Add lightweight indexes for common filter/sort dimensions.
- Store state tracks section loading flags independently. Initial load fetches scopes/overview/activity in parallel and defers calls/threads/diagnostics until their tab is opened.
- Refresh button calls the refresh function for the current section; a separate index refresh remains manual and can still be slow.
- UI renders panels immediately with small loading notes/spinners instead of waiting for all sections.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Section API contracts and tests | Tests fail first for missing section APIs and tab refresh behavior | 验证成功 |
| T2 | Backend SQL-downshifted section queries | Rust/TS tests pass for filtered limited calls and section commands | 验证成功 |
| T3 | Frontend partitioned loading UI | React/store tests pass for independent loading and current-tab refresh | 验证成功 |

### T1: Section API contracts and tests

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The old big response hides the performance contract. Tests must lock in the desired split before implementation.

**What to do**:
- Update usage store tests to expect separate API calls for scopes, overview, activity, and tab-specific data.
- Add tests that `refreshCurrentUsageSection('calls')` only reloads calls data.
- Add backend-focused test coverage for calls limit/filter behavior.

**Logic design**:
- Tests mock API functions by section.
- Store tests should not require actual timing; call ordering and state flags are enough.
- Backend tests should assert returned row count and filtered model/search values.

**Test design**:
- Normal: initial load calls scopes/overview/activity and does not call calls/threads/diagnostics.
- State: loading flags toggle per section.
- Refresh: current tab refresh only calls its matching section loader.
- Backend: calls request with `model`, `search`, and `limit` returns only matching limited rows.

**Acceptance**:
- `cd apps/desktop && npm test -- src/stores/usage.test.ts` initially fails for missing API functions.
- Rust focused test initially fails or cannot compile until section functions exist.

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T2: Backend SQL-downshifted section queries

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The main speed gain comes from not reading all calls for every dashboard view.

**What to do**:
- Add section response structs and exported service functions.
- Add Tauri commands and API wrappers.
- Add SQL predicates for `search/model/effort/pricingConfidence`.
- Add `ORDER BY` mapping and `LIMIT` in SQL.
- Add common indexes in `init_usage_db`.

**Logic design**:
- Reuse `UsageDashboardRequest` to avoid duplicating request contracts.
- Convert scope id and account id through existing `SummaryFilter`.
- Keep old `get_usage_dashboard_response` implemented through existing behavior for compatibility.
- Validate sort keys by mapping to known SQL expressions; unknown keys fall back to time.

**Test design**:
- Backend unit test uses existing refresh/index helpers to create fixture rows.
- Assert filtered calls never exceed requested limit.
- Assert nonmatching model/search rows are absent.

**Acceptance**:
- Focused Rust usage tests pass.

**Done criteria**:
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T3: Frontend partitioned loading UI

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Even with faster backend queries, the page must not wait for unrelated sections and must show local refresh progress.

**What to do**:
- Extend usage store with `sectionLoading` and section data setters.
- Initial load fetches scopes/overview/activity concurrently.
- Tab changes lazily load calls/threads/diagnostics.
- Refresh button refreshes the active section request; index refresh remains separate.
- Usage page shows section loading indicators and keeps layout stable.

**Logic design**:
- Keep `summary` shape for component compatibility, but merge section responses into it.
- Keep old `refreshing` for index refresh compatibility; add section booleans for UI.
- Use request snapshots so every section uses the current filters.

**Test design**:
- Store test verifies initial section calls.
- Store test verifies `refreshUsageSection('calls')`.
- React test verifies calls loading indicator appears while calls section is loading.

**Acceptance**:
- `cd apps/desktop && npm test -- src/stores/usage.test.ts src/routes/usage.test.tsx src/App.handoff.test.tsx` passes.

**Done criteria**:
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Store/UI tests | `cd apps/desktop && npm test -- src/stores/usage.test.ts src/routes/usage.test.tsx` | exit 0 |
| App usage tests | `cd apps/desktop && npm test -- src/App.handoff.test.tsx` | exit 0 |
| Rust usage tests | `cargo test usage` or project Makefile equivalent when available | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if the existing dirty tree changes the Usage API contract incompatibly, or if Rust verification requires unavailable local toolchain support.
