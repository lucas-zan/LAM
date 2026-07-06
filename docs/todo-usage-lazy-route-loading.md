# Todo: Usage Lazy Route Loading

> Executor instructions: Follow this todo step by step. Generate tests from
> the "Test design" section before implementation. Run each verification command
> and confirm the expected result before moving to the next task. If a STOP
> condition occurs, stop and report instead of improvising.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: partitioned Usage APIs and current Usage page section loading
- **Category**: bugfix/performance
- **Planned at**: current dirty workspace

## Why this matters

**Background**: Opening LAM currently starts Usage summary loading even when the user stays on the Overview route. Users observe that the app is slow and Usage tabs are temporarily hard to click.

**Current state**: `App.tsx` has an unconditional effect that calls `loadUsageSummary(usageRequest)` on mount and whenever the request changes. Usage detail tabs already load lazily, but the overview/scopes/activity section is not gated by the active route.

**Impact**: Startup competes with normal home rendering and may make the app feel blocked by a page the user did not open.

**What improves**: Usage work starts only when the user enters Usage. Usage layout remains clickable while each section loads independently. Refresh remains section-scoped.

## Scope

**In scope**:
- `apps/desktop/src/App.tsx`
- `apps/desktop/src/App.handoff.test.tsx`
- Usage route loading semantics and route-gated tests

**Out of scope**:
- Backend SQL performance
- Thread summary materialization
- Global navigation redesign

## Design

Expected behavior:
- Startup on non-Usage routes must not call `getUsageScopes`, `getUsageOverview`, or `getUsageActivity`.
- Entering `usage` route triggers exactly the summary sections required for the first Usage screen: scopes, overview, activity.
- Changing Usage filters while not on Usage should not load Usage data; the next entry to Usage should load with the latest request.
- Calls/Threads/Diagnostics continue to load from `UsagePage` when those tabs become active.
- Refresh button remains section-scoped and does not call the full index refresh unless the existing explicit full refresh path is used elsewhere.

Implementation boundary:
- Keep store APIs unchanged where possible.
- Move the route gate into `App.tsx`, because route knowledge belongs there.
- Do not block rendering on Usage data; `UsagePage` already accepts null summary and loading states.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Gate startup Usage summary loading by route | App startup on Overview calls no Usage section APIs | 验证成功 |
| T2 | Load Usage first-screen sections only when entering Usage | Clicking Usage starts scopes/overview/activity; clicking Calls remains tab-specific | 验证成功 |

### T1: Gate startup Usage summary loading by route

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Startup should not run unrelated Usage queries.

**What to do**:
- Add a component test that renders App on Overview and asserts Usage section APIs are not called after boot.
- Change the App effect to return early unless `route === 'usage'`.

**Logic design**:
- Use existing `route` from app store as the gate.
- Keep `usageRequest` memoization unchanged.
- Avoid adding new global state.

**Test design**:
- Mock boot APIs as today.
- Render `<App />`, wait for Overview/account content, then assert `getUsageScopes`, `getUsageOverview`, and `getUsageActivity` were not called.

**Acceptance**:
- `cd apps/desktop && npm test -- src/App.handoff.test.tsx`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Task status is updated to `验证成功`

### T2: Load Usage first-screen sections only when entering Usage

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The Usage page should still populate when opened, but that work must be route scoped.

**What to do**:
- Add a component test that clicks the global Usage navigation and asserts summary section APIs are called.
- Ensure Calls/Threads/Diagnostics lazy loading behavior remains covered by existing UsagePage tests.

**Logic design**:
- The route-gated effect runs when `route` becomes `usage`.
- The first Usage render shows layout immediately with section loading state from the store.
- Do not call `refreshUsageIndex` from normal route entry.

**Test design**:
- Render `<App />` on Overview.
- Click the Usage nav button.
- Assert `getUsageScopes`, `getUsageOverview`, and `getUsageActivity` are called, and `refreshUsageIndex` is not called.

**Acceptance**:
- `cd apps/desktop && npm test -- src/App.handoff.test.tsx src/routes/usage.test.tsx src/stores/usage.test.ts`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Task status is updated to `验证成功`

## Test plan

- App startup regression test for no Usage API calls.
- App route-entry regression test for Usage summary API calls.
- Existing UsagePage tests cover tab-specific lazy loading and callback stability.
- Existing store tests cover section refresh.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Startup focused test | `cd apps/desktop && npm test -- src/App.handoff.test.tsx` | New startup test fails before implementation, passes after |
| Related suite | `cd apps/desktop && npm test -- src/App.handoff.test.tsx src/routes/usage.test.tsx src/stores/usage.test.ts` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:
- App tests cannot navigate to Usage with the existing BottomNav labels.
- The route store cannot be safely observed in App tests.
