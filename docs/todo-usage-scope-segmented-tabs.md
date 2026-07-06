# Todo: Usage Scope Segmented Tabs

> Executor instructions: Follow this todo step by step. Generate tests from the "Test design" section before implementation. Run each verification command and confirm the expected result before moving to the next task.

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: Usage scope tabs already rendered as `role=tablist`
- **Category**: UI polish
- **Planned at**: current dirty workspace

## Why this matters

**Background**: The Usage scope selector currently reads like concatenated text such as `Totalmaincodex-ccodex-luna002`. The desired presentation is a segmented tab control similar to the provided Codex/Antigravity screenshot.

**Current state**: `UsagePage` renders scope tabs with `className="usageScopeTabs"`, but no matching dedicated CSS exists. Buttons only receive `active`, so the browser/default/global styles do not create a segmented control.

**Impact**: The scope selector is hard to parse and looks broken, especially with multiple accounts.

**What improves**: Scope selection becomes a clear segmented control while preserving keyboard/a11y semantics.

## Scope

**In scope**:
- `apps/desktop/src/routes/usage.tsx`
- `apps/desktop/src/routes/usage.test.tsx`
- `apps/desktop/src/styles.css`

**Out of scope**:
- Changing scope data, labels, or filtering behavior.
- Redesigning other Usage tabs.

## Design

Expected behavior:
- The scope tablist uses a dedicated segmented class.
- Scope buttons keep `role="tab"` and `aria-selected`.
- Active scope has an explicit active class and visual treatment.
- The container supports horizontal overflow for many accounts without wrapping into unreadable text.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Render Usage scopes as segmented tabs | Test verifies segmented class and active tab semantics; visual CSS exists | 验证成功 |

### T1: Render Usage scopes as segmented tabs

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The current scope selector lacks dedicated styling and is visually ambiguous.

**What to do**:
- Add `usageScopeTabs--segmented` to the scope tablist.
- Add stable class names for active/inactive scope tab buttons.
- Add CSS for segmented container, active tab, hover, and overflow.

**Logic design**:
- Keep the existing `setUsageScope(scope.id)` behavior unchanged.
- Use CSS class composition only; no new state.
- Scope label text remains the source label returned by the backend.

**Test design**:
- Render `UsagePage` with multiple scopes.
- Assert the scope tablist has `usageScopeTabs--segmented`.
- Assert the active tab has `usageScopeTab--active` and `aria-selected="true"`.
- Assert clicking another scope still calls `setUsageScope` with its scope id.

**Acceptance**:
- `cd apps/desktop && npm test -- src/routes/usage.test.tsx`
- `cd apps/desktop && npm test -- src/App.handoff.test.tsx src/routes/usage.test.tsx`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Related verification command passes
- [x] Task status is updated to `验证成功`

## Test plan

- Component-level test for scope tab CSS contract and click behavior.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused tests | `cd apps/desktop && npm test -- src/routes/usage.test.tsx` | New test fails before implementation, passes after |
| Related suite | `cd apps/desktop && npm test -- src/App.handoff.test.tsx src/routes/usage.test.tsx` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:
- The existing Usage scope markup has been removed or moved outside `UsagePage`.
