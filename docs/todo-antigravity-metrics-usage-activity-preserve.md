# Todo: Antigravity Metrics and Usage Activity Preserve

> Executor instructions: Follow this todo step by step. Generate tests from the "Test design" section before implementation. Run each verification command and confirm the expected result before moving to the next task.

## Status

- **Priority**: P0
- **Effort**: S
- **Risk**: MEDIUM
- **Depends on**: current Antigravity grouped quota and Usage section loading
- **Category**: bugfix
- **Planned at**: current dirty workspace

## Why this matters

**Background**: Antigravity metrics now show quota group counts as model counts. Usage activity can briefly render and then disappear after another first-screen section resolves.

**Current state**:
- Overview Antigravity metrics derive `Models` and `Models usable` from groups when groups exist.
- Usage store merges overview responses with all fields, including empty section arrays such as `activityBuckets`.

**Impact**:
- Antigravity top cards show misleading `2/2` model counts.
- Activity chart can be overwritten by a later overview response with empty activity data.

**What improves**:
- Antigravity metrics use model count for model cards and group count for groups.
- Activity data is preserved unless the activity section itself refreshes.

## Scope

**In scope**:
- `apps/desktop/src/routes/views.tsx`
- `apps/desktop/src/routes/handoff.test.tsx`
- `apps/desktop/src/stores/usage.ts`
- `apps/desktop/src/stores/usage.test.ts`

**Out of scope**:
- CSS/style changes.
- Backend query changes.

## Design

- Antigravity:
  - `Models` metric shows usable model count / total model count.
  - `Sessions` metric label becomes `Groups` on the Antigravity tab.
  - `Models usable` shows usable model count.
  - A model is usable when it belongs to a group with any bucket remaining, or when fallback model remaining fraction is positive.
- Usage:
  - Add an overview merge helper that strips independent section arrays from overview payloads.
  - Overview responses must not overwrite `activityBuckets`, `recentCalls`, or `topThreads`.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Fix Antigravity metric semantics | Overview test shows model counts and group count separately | 验证成功 |
| T2 | Preserve independently loaded activity | Store test proves late overview does not clear activity buckets | 验证成功 |

### T1: Fix Antigravity metric semantics

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The current metric cards label group counts as models.

**What to do**:
- Update Antigravity metric derivation in `Overview`.
- Use grouped model membership for usable model count.
- Change second metric label to `Groups` when Antigravity is active.

**Logic design**:
- Total models: `quota.models.length`.
- Usable models: count models in groups with remaining quota.
- Groups: `quota.groups.length`.

**Test design**:
- Render Antigravity overview with two groups and multiple models.
- Assert `Models` displays model count, `Groups` displays group count, and `Models usable` displays usable models.

**Acceptance**:
- `cd apps/desktop && npm test -- src/routes/handoff.test.tsx`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T2: Preserve independently loaded activity

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
First-screen section requests can resolve in any order; overview should not clear activity.

**What to do**:
- Add a store test where activity resolves before overview.
- Change overview merge path to preserve independent section arrays.

**Logic design**:
- `getUsageOverview` results update headline/totals/options but do not replace `activityBuckets`, `recentCalls`, or `topThreads`.
- `getUsageActivity` remains the only first-screen path that updates activity buckets.

**Test design**:
- Start `loadUsageSummary`.
- Resolve activity with one bucket.
- Resolve overview with `activityBuckets: []`.
- Assert the bucket remains.

**Acceptance**:
- `cd apps/desktop && npm test -- src/stores/usage.test.ts`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Test plan

- Focused React test for Antigravity metrics.
- Focused Usage store test for late overview merge.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Antigravity metrics | `cd apps/desktop && npm test -- src/routes/handoff.test.tsx` | exit 0 |
| Usage store | `cd apps/desktop && npm test -- src/stores/usage.test.ts` | exit 0 |
| Typecheck | `cd apps/desktop && npx tsc --noEmit` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:
- Fixing the issue requires CSS changes.
- The backend no longer returns model rows for Antigravity.
