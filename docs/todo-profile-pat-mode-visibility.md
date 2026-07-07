# Todo: Profile and PAT Mode Visibility

> Executor instructions: Follow this todo step by step. Generate tests from
> the "Test design" section before implementation. Run each verification command
> and confirm the expected result before moving to the next task. If a STOP
> condition occurs, stop and report instead of improvising.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: none
- **Category**: feature
- **Planned at**: current workspace

## Why this matters

**Background**: Profile mode treats accounts as isolated `CODEX_HOME` spaces. The current account card always shows `Switch`, which implies a profile-level switch but either opens login in OAuth mode or overwrites main auth in PAT mode.

**Current state**: `apps/desktop/src/routes/views.tsx` renders `Switch` for every account card. `apps/desktop/src/App.tsx` always renders top-level Profile/PAT mode tabs and persists `authMode` via backend settings. Settings has no control for whether mode switching is available.

**Impact**: Users can interpret Profile mode as destructive or global account switching, which conflicts with the profile design. Users who only want one mode still see irrelevant mode controls and information.

**What improves**: Profile mode no longer exposes the misleading `Switch` action. Settings can constrain the product to Profile Only, PAT Only, or Profile & PAT, and the titlebar reflects that constraint.

## Scope

**In scope**:
- Frontend account card visibility for `Switch`.
- Frontend mode availability setting stored locally.
- Settings UI for `Profile Only`, `PAT Only`, `Profile & PAT`.
- Tests for the new behavior.

**Out of scope**:
- Backend auth switching behavior.
- Main `auth.json` backup or restore.
- Tray popover mode behavior unless required by existing tests.

## Design

Add a local UI preference with values `profile`, `pat`, and `both`. Keep backend `authMode` as the active runtime mode. When mode availability is `profile`, force active mode to `oauth` and show only a non-interactive `Profile Mode` indicator in the titlebar. When availability is `pat`, force active mode to `pat` and show only `PAT Mode`. When availability is `both`, keep the current two-tab switcher.

In account cards, render `Switch` only when `authMode === 'pat'`. This keeps PAT activation available but removes the action from Profile mode entirely.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Hide Switch in Profile mode | Profile mode account cards do not render `Switch`; PAT mode still does | 验证成功 |
| T2 | Add Settings mode availability | Settings controls Profile Only, PAT Only, Profile & PAT and titlebar follows it | 验证成功 |

### T1: Hide Switch in Profile mode

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Profile mode should represent independent profile spaces, not an auth overwrite or activation action.

**What to do**:
- Update `AccountsGrid` in `apps/desktop/src/routes/views.tsx` so `Switch` renders only for PAT mode.
- Keep PAT mode disabled rules unchanged for `main` and active account.

**Logic design**:
- Wrap the `Switch` button in `authMode === 'pat'`.
- Do not call `switchAccount` from Profile mode because there is no button.
- Keep all other Profile actions intact.

**Test design**:
- Add or update React tests that render Overview in OAuth/Profile mode and assert no `Switch to this account` button exists.
- Add/keep PAT mode test asserting `Switch to this account` buttons are present with existing disabled rules.
- Expected initial failure: Profile mode test finds switch buttons in current implementation.

**Acceptance**:
- Focused command: `npm test -- --run apps/desktop/src/routes/handoff.test.tsx`
- Expected after implementation: command exits 0.

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T2: Add Settings mode availability

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Users should be able to run LAM as Profile-only, PAT-only, or combined mode, with irrelevant controls hidden.

**What to do**:
- Add local preference handling in `apps/desktop/src/App.tsx`.
- Add Settings select in `apps/desktop/src/routes/views.tsx`.
- In Profile Only, titlebar shows only `Profile Mode` and active auth mode is OAuth.
- In PAT Only, titlebar shows only `PAT Mode` and active auth mode is PAT.
- In Profile & PAT, keep current top tab switcher.

**Logic design**:
- Define `modeAvailability` values: `profile`, `pat`, `both`.
- Persist to localStorage using a stable key.
- On mount, load backend `authMode` and local availability, then clamp active mode if needed.
- When Settings changes availability, update localStorage and clamp/persist active `authMode`.
- Pass availability props into Settings.

**Test design**:
- Add App tests for Settings select changing to Profile Only, asserting only Profile Mode is shown and PAT tab is hidden.
- Add App tests for PAT Only, asserting active mode becomes PAT and titlebar only shows PAT Mode.
- Add App test or assertion for Profile & PAT showing both tabs.
- Expected initial failure: Settings select does not exist and titlebar always shows both tabs.

**Acceptance**:
- Focused command: `npm test -- --run apps/desktop/src/App.handoff.test.tsx`
- Relevant command: `npm test -- --run apps/desktop/src/routes/handoff.test.tsx apps/desktop/src/App.handoff.test.tsx`
- Expected after implementation: commands exit 0.

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Test plan

- Profile mode Overview hides `Switch`.
- PAT mode Overview still shows `Switch` with disabled rules.
- Settings has a mode availability select.
- Profile Only clamps active mode and hides PAT titlebar tab.
- PAT Only clamps active mode and hides Profile titlebar tab.
- Profile & PAT shows current switcher behavior.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Overview mode tests | `npm test -- --run apps/desktop/src/routes/handoff.test.tsx` | exit 0 after implementation; expected failure before T1 implementation |
| App settings tests | `npm test -- --run apps/desktop/src/App.handoff.test.tsx` | exit 0 after implementation; expected failure before T2 implementation |
| Relevant suite | `npm test -- --run apps/desktop/src/routes/handoff.test.tsx apps/desktop/src/App.handoff.test.tsx` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:

- Existing tests show the titlebar or Settings structure differs materially from this plan.
- The localStorage preference conflicts with backend mode persistence in a way that cannot be resolved frontend-only.
- Verification still fails after the configured fix loop.
- Completing the task would require backend auth semantics changes.
