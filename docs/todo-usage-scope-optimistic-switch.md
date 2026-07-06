# Todo: Usage Scope Optimistic Switch

> Executor instructions: Follow this todo step by step. Generate tests from the "Test design" section before implementation. Run each verification command and confirm the expected result before moving to the next task.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: Usage segmented scope tabs
- **Category**: UX bugfix
- **Planned at**: current dirty workspace

## Why this matters

**Background**: Usage scope tabs are now visually segmented, but clicking a scope waits for the query to complete before the active tab changes.

**Current state**: `activeScopeId` is owned by the Usage store and is updated by `loadUsageSummary` only after `getUsageScopes/getUsageOverview/getUsageActivity` complete. `App.setUsageScope` calls `loadUsageSummary` directly, so the selected tab does not change immediately.

**Impact**: The UI feels frozen and does not acknowledge the click while a potentially slow query is running.

**What improves**: Clicking a scope immediately updates the selected tab and then loads the matching data with section loading indicators.

## Scope

**In scope**:
- `apps/desktop/src/stores/usage.ts`
- `apps/desktop/src/stores/usage.test.ts`
- `apps/desktop/src/App.tsx`
- `apps/desktop/src/App.handoff.test.tsx`

**Out of scope**:
- Changing backend query behavior.
- Redesigning loading skeletons.

## Design

Expected behavior:
- On scope click, `activeScopeId` is set immediately before the async query completes.
- The async summary load still updates summary/scopes from real API results.
- If a query fails, the selected scope can remain as the user's intended selection while the error is surfaced through existing error handling.
- Route-gated Usage loading and section-level Refresh behavior remain unchanged.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Optimistically select Usage scope before loading data | Store exposes immediate scope selection; App tab selected state changes before deferred query resolves | 验证成功 |

### T1: Optimistically select Usage scope before loading data

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The user should get immediate visual feedback when choosing a scope.

**What to do**:
- Add a small store action for setting active scope immediately.
- Call it in `App.setUsageScope` before `loadUsageSummary`.
- Add tests for store behavior and App behavior with deferred Usage APIs.

**Logic design**:
- Keep `loadUsageSummary` as the data loading path.
- `selectUsageScope(scopeId)` only mutates `activeScopeId`; no API call.
- `setUsageScope` sets loaded request key, selects scope, then starts loading summary with that scope.

**Test design**:
- Store test: calling `selectUsageScope('workspace:main')` updates `activeScopeId` without calling Usage APIs.
- App test: with a deferred `getUsageOverview`, click a scope tab and assert `aria-selected=true` immediately before resolving the query.

**Acceptance**:
- `cd apps/desktop && npm test -- src/stores/usage.test.ts src/App.handoff.test.tsx`
- `cd apps/desktop && npm test -- src/App.handoff.test.tsx src/routes/usage.test.tsx src/stores/usage.test.ts`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Related verification command passes
- [x] Task status is updated to `验证成功`

## Test plan

- Store unit test for optimistic selection.
- App integration test for immediate selected tab while data request is pending.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused tests | `cd apps/desktop && npm test -- src/stores/usage.test.ts src/App.handoff.test.tsx` | New tests fail before implementation, pass after |
| Related suite | `cd apps/desktop && npm test -- src/App.handoff.test.tsx src/routes/usage.test.tsx src/stores/usage.test.ts` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:
- Usage scope is no longer owned by `useUsageStore`.
