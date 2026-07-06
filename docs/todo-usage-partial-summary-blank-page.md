# Todo: Usage Partial Summary Blank Page

> Executor instructions: Follow this todo step by step. Generate tests from the "Test design" section before implementation. Run each verification command and confirm the expected result before moving to the next task.

## Status

- **Priority**: P0
- **Effort**: S
- **Risk**: LOW
- **Depends on**: partitioned Usage loading
- **Category**: bugfix
- **Planned at**: current dirty workspace

## Why this matters

**Background**: Usage data now loads by independent sections. The activity section can return before overview.

**Current state**: `mergeSummary(null, { activityBuckets })` can temporarily create a partial summary without `pricingCoverage`. `UsagePage` dereferences `summary.pricingCoverage.*` in diagnostics helpers and tables.

**Impact**: A fast activity response can throw a runtime exception and make the app window blank.

**What improves**: Usage page remains renderable while individual sections load in any order.

## Scope

**In scope**:
- `apps/desktop/src/routes/usage.tsx`
- `apps/desktop/src/routes/usage.test.tsx`

**Out of scope**:
- Store architecture changes.
- CSS changes.

## Design

- Treat nested summary fields as optional in the UI.
- Use optional chaining/defaults for `pricingCoverage`.
- Add a regression test that renders UsagePage with only `activityBuckets`.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Guard partial Usage summaries | Test proves partial summary does not throw | 验证成功 |

### T1: Guard partial Usage summaries

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The app can blank during normal asynchronous section loading.

**What to do**:
- Add a render test for a summary containing only `activityBuckets`.
- Guard `summary.pricingCoverage` reads with safe defaults.

**Logic design**:
- Keep current data flow.
- Do not assume overview has arrived before activity/calls/diagnostics.
- Keep labels and styles unchanged.

**Test design**:
- Render `UsagePage` with `summary={{ activityBuckets: [...] } as UsageDashboard}`.
- Assert rendering does not throw and the activity cell exists.

**Acceptance**:
- `cd apps/desktop && npm test -- src/routes/usage.test.tsx`
- `cd apps/desktop && npx tsc --noEmit`
- `cd apps/desktop && npm run build`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Test plan

- UsagePage partial summary regression test.
- Existing UsagePage suite.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Usage route | `cd apps/desktop && npm test -- src/routes/usage.test.tsx` | exit 0 |
| Typecheck | `cd apps/desktop && npx tsc --noEmit` | exit 0 |
| Build | `cd apps/desktop && npm run build` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:
- Fixing this requires changing section loading architecture.
