# Todo: Usage Section Independent Loading

> Executor instructions: Follow this todo step by step. Generate tests from the "Test design" section before implementation. Run each verification command and confirm the expected result before moving to the next task.

## Status

- **Priority**: P0
- **Effort**: S
- **Risk**: MEDIUM
- **Depends on**: existing section APIs and Usage store
- **Category**: bugfix
- **Planned at**: current dirty workspace

## Why this matters

**Background**: Usage already has section APIs (`scopes`, `overview`, `activity`, `calls`, `threads`, `diagnostics`), but the first three are still loaded through one `Promise.all` and committed together.

**Current state**: Entering the Usage route calls `loadUsageSummary`, which waits for `getUsageScopes`, `getUsageOverview`, and `getUsageActivity` to all complete before updating state.

**Impact**: A slow activity query delays fast scope and overview data, so the page still feels like one blocking load.

**What improves**: Scopes, overview cards, and activity can appear independently as soon as each section returns. Each section keeps its own loading state.

## Scope

**In scope**:
- `apps/desktop/src/stores/usage.ts`
- focused store tests
- fix UsagePage test props only if required by type checking

**Out of scope**:
- Any CSS/style changes.
- Backend query changes.
- Changing index rebuild behavior.

## Design

- `loadUsageSummary` starts `scopes`, `overview`, and `activity` requests immediately.
- Each request updates only its own data and clears only its own loading flag when it completes.
- A failure in one section records an app error and does not prevent other sections from rendering.
- The returned promise resolves after all three requests settle.
- Existing `loadUsageSection` behavior remains unchanged for lazy tabs.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Independent first-screen Usage sections | Store test proves scopes can render before overview/activity finish | 验证成功 |

### T1: Independent first-screen Usage sections

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The first-screen store action is still blocking on the slowest section.

**What to do**:
- Add a store test with deferred promises.
- Refactor `loadUsageSummary` so each section sets state independently.
- Keep per-section loading flags accurate.

**Logic design**:
- Set `scopes`, `overview`, and `activity` loading to true at the start.
- Create three independent tasks.
- Each task handles success/error/finally for one section.
- Await `Promise.allSettled(tasks)` at the end.

**Test design**:
- Mock three API calls as deferred promises.
- Call `loadUsageSummary`.
- Resolve only scopes; assert `scopes` and `activeScopeId` are updated while `overview/activity` remain loading.
- Resolve overview; assert summary appears while activity remains loading.
- Resolve activity; assert `activityBuckets` merge and all three loading flags are false.

**Acceptance**:
- `cd apps/desktop && npm test -- src/stores/usage.test.ts`
- `cd apps/desktop && npm test -- src/routes/usage.test.tsx`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Test plan

- Store unit test for independent first-screen section resolution.
- Existing UsagePage tests to ensure props and lazy tab behavior still hold.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Store focused | `cd apps/desktop && npm test -- src/stores/usage.test.ts` | exit 0 |
| Usage route | `cd apps/desktop && npm test -- src/routes/usage.test.tsx` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:
- The latest Usage store no longer uses section APIs.
- The requested behavior requires CSS changes.
