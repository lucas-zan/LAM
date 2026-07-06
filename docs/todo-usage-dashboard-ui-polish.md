# Todo: Usage Dashboard UI Polish

> Executor instructions: Follow this todo step by step. Generate tests from the Test design section before implementation. Keep changes scoped to the LAM usage dashboard UI rendering performance.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: MEDIUM
- **Depends on**: `docs/usage-workspace-account-attribution-design.md`
- **Category**: bugfix/feature
- **Planned at**: current working tree with existing uncommitted LAM changes

## Why this matters

**Background**: The usage dashboard added workspace/account attribution, but the visible page is slow, uses abbreviated `tok`, and the view tabs are not clearly visible or accessible. Unit-price changes are explicitly out of scope for this pass.

**Current state**: `apps/desktop/src/routes/usage.tsx` renders large call/thread tables directly and uses `tok` in metric labels. Usage data refresh is manual: `refreshUsage` calls `refreshUsageIndex`, then reloads `getUsageDashboardResponse`; it is not a live streaming or polling refresh.

**Impact**: Users can hit sluggish rendering when many calls are loaded. Hidden or visually weak tabs make the page feel broken, and abbreviated token labels reduce clarity.

**What improves**: The page becomes clearer and lighter to render without changing pricing behavior.

## Scope

**In scope**:
- Usage dashboard React rendering in `apps/desktop/src/routes/usage.tsx`.
- Focused React/unit tests covering the visible UX.

**Out of scope**:
- Backend schema or parser changes.
- New account-attribution backend logic.
- Unit-price/rate-card behavior changes.
- Nginx, packaging, or app shell changes.

## Design

- Limit expensive visible table rendering to a bounded client-side display count while preserving the selected backend load limit and showing a note when results are capped.
- Replace abbreviated `tok` with `tokens` in visible labels and heatmap text.
- Make view tabs always visible with explicit `role="tab"`, `aria-selected`, and active class. Remove duplicated heatmap header markup that can disturb layout.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Usage dashboard clarity and performance | Tests show tokens wording, visible tabs, and bounded rows | 验证成功 |

### T1: Usage dashboard clarity and performance

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The current page is hard to scan and can render too much data at once.

**What to do**:
- Update tests to require `tokens` wording, no visible `tok`, visible usage tabs, and capped calls/threads tables.
- Cap visible rows for calls and threads while preserving the backend load-limit semantics.
- Ensure tab buttons expose `role="tab"` and active/aria state.

**Logic design**:
- Derive `visibleCalls` and `visibleThreads` from filtered collections using fixed caps.
- Keep filters and selected-call behavior based on visible rows to avoid hidden row selection surprises.
- Use `tokens` in all user-facing unit suffixes.

**Test design**:
- React test renders Usage page and verifies:
  - `Total Tokens` value ends with `tokens`, not `tok`.
  - Usage view tabs are role tabs and show active state after click.
  - Large call/thread lists render capped notes instead of all rows.

**Acceptance**:
- `cd apps/desktop && npm test -- src/routes/usage.test.tsx` passes.
- Existing relevant usage tests continue to pass.

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

**Verification note**:
- First focused run failed because the `1.3M tokens` assertion assumed a single match while the page correctly shows total and range token values. The implementation behavior was acceptable; the test assertion was tightened to allow multiple matching token labels.

## Test plan

- `apps/desktop/src/routes/usage.test.tsx`: visible dashboard behavior.
- Manual refresh behavior remains unchanged and non-realtime.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused tests | `cd apps/desktop && npm test -- src/routes/usage.test.tsx` | exit 0 after implementation; expected failure before implementation |
| Existing app tests | `cd apps/desktop && npm test -- src/App.handoff.test.tsx src/stores/usage.test.ts` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if existing dirty changes materially change the usage dashboard API contract, or if the test environment cannot run without unrelated dependency failures.
