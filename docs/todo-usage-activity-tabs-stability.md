# Todo: Usage Activity Range And Tab Stability

> Executor instructions: Follow this todo step by step. Generate tests from
> the "Test design" section before implementation. Run each verification command
> and confirm the expected result before moving to the next task. If a STOP
> condition occurs, stop and report instead of improvising.

## Status

- **Priority**: P0
- **Effort**: S
- **Risk**: MEDIUM
- **Depends on**: existing Usage dashboard partitioned loading work
- **Category**: bugfix
- **Planned at**: current dirty workspace

## Why this matters

**Background**: The Usage activity heatmap appears to stop at June 30 even when the current date is July 3, 2026, and switching into the Calls tab can freeze the page.

**Current state**: `apps/desktop/src/routes/usage.tsx` builds the heatmap from the last activity bucket and always fills the previous 364 days. If the last bucket is June 30, the visible axis ends there. The lazy detail-section `useEffect` depends on a callback prop that can be recreated by the parent on every render, so selecting Calls can repeatedly trigger section loads.

**Impact**: Users can reasonably think the time filter is not applied or that recent days are missing. Repeated Calls loading can make the page unresponsive.

**What improves**: Activity rendering will make the selected time window explicit and include today for open-ended/all-time views. Lazy tab loading will be tied to the selected tab and request values instead of callback identity churn.

## Scope

**In scope**:
- `apps/desktop/src/routes/usage.tsx`
- `apps/desktop/src/routes/usage.test.tsx`
- `apps/desktop/src/styles.css` if dynamic month columns need CSS support

**Out of scope**:
- Backend query semantics and thread materialized summaries
- Global bottom dock navigation behavior beyond explaining its purpose
- Pricing or account attribution logic

## Design

Activity display should derive an explicit inclusive date range from `usageWindow`:
- `custom`: use `from` and `to` when present.
- `today`: show the current local day.
- `last-7-days`: show the last seven days ending today.
- `this-week`: show Monday through today.
- `this-month`: show the first day of the month through today.
- `all`: show the last 365 days ending at max(today, last bucket date), so empty recent days are visible.

The heatmap should fill missing days with zeroes, keep cumulative values stable, and render month labels from the same computed range. The detail-section loader should call the latest provided `loadUsageSection` callback through a ref, while the effect is driven by `usageTab` and `detailRequest` only.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Make activity range follow the selected window and include today | Custom range renders only selected days; all-time with stale last bucket includes July 3 | 验证成功 |
| T2 | Stop Calls tab repeated lazy-load loops | Rerendering with a new callback identity does not refetch Calls unless tab/request changes | 验证成功 |

### T1: Make activity range follow the selected window and include today

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The current chart is anchored to the last activity bucket, which hides empty recent days and ignores short selected ranges.

**What to do**:
- Add or refactor pure date-range helpers in `UsagePage`.
- Add test-visible date metadata to heatmap cells.
- Make month labels use the computed range and dynamic column count.

**Logic design**:
- Normalize dates as UTC day strings for deterministic tests.
- Use today as a fallback and as the end boundary for open-ended current ranges.
- Swap invalid custom `from`/`to` ranges defensively or collapse to valid available boundary.

**Test design**:
- Render all-time activity where the last bucket is `2026-06-30` while system time is `2026-07-03`; assert a heatmap cell for `2026-07-03` is present.
- Render custom `2026-07-01` to `2026-07-03`; assert cells for July 1-3 are present and June 30 is absent.

**Acceptance**:
- `cd apps/desktop && npm test -- src/routes/usage.test.tsx`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Task status is updated to `验证成功`

### T2: Stop Calls tab repeated lazy-load loops

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The parent currently passes inline loader callbacks. If the child effect depends on callback identity, each load-triggered store render can trigger another load.

**What to do**:
- Keep the latest loader function in a ref.
- Drive lazy section loading from `usageTab` and `detailRequest` changes only.

**Logic design**:
- Update the ref whenever the loader prop changes.
- The loading effect calls `loadUsageSectionRef.current` for calls/threads/diagnostics.
- No change to manual refresh semantics.

**Test design**:
- Render Calls tab with one loader callback, rerender Calls tab with a new callback and unchanged request; assert the new callback is not called.
- Change from Insights to Calls; assert exactly one call load happens.

**Acceptance**:
- `cd apps/desktop && npm test -- src/routes/usage.test.tsx`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Task status is updated to `验证成功`

## Test plan

- Component tests for activity range visibility and absent out-of-range dates.
- Component tests for lazy section loading stability across parent callback identity changes.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused tests | `cd apps/desktop && npm test -- src/routes/usage.test.tsx` | New tests fail before implementation and pass after implementation |
| Related suite | `cd apps/desktop && npm test -- src/routes/usage.test.tsx src/App.handoff.test.tsx` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:
- The route test harness cannot control current date deterministically.
- The parent route state model differs enough that the lazy-load loop is not testable in UsagePage.
