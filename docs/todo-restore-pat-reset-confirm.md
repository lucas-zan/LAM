# Todo: Restore PAT Reset Confirm Behavior

> Executor instructions: Follow this todo step by step. Generate tests from the
> "Test design" section before implementation. Run each verification command and
> confirm the expected result before moving to the next task.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: current merge conflict resolution
- **Category**: bugfix
- **Planned at**: codex/008-usage-statistics-dashboard

## Why this matters

**Background**: During manual merge resolution, the release branch UI was kept, but two behavior details from the 008 branch need to be restored.

**Current state**: The reset quota button is disabled when reset credits are absent, but the test still expects the old clickable behavior. `@tauri-apps/plugin-dialog` remains wired in dependencies and Tauri setup, but `views.tsx` still uses `window.confirm`.

**Impact**: The app has unnecessary dialog wiring unless the Tauri confirm flow is restored, and the test suite contradicts the intended disabled behavior.

**What improves**: PAT reset confirmation uses the native Tauri dialog in-app with browser fallback, and tests match the restored no-credit disabled behavior.

## Scope

**In scope**:
- `apps/desktop/src/routes/views.tsx`
- `apps/desktop/src/routes/handoff.test.tsx`

**Out of scope**:
- Settings page tests from the release redesign.
- Backend quota/reset implementation.

## Design

Use `@tauri-apps/plugin-dialog` `confirm` when `inTauri()` is true. If Tauri dialog fails or the app is not in Tauri, fall back to `window.confirm`. Keep no-credit reset buttons disabled and update tests to assert that behavior.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Restore confirm flow and tests | Focused handoff route tests pass | 验证成功 |

### T1: Restore confirm flow and tests

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The merge kept release UI but dropped native confirm usage and kept a stale test expectation.

**What to do**:
- Add test coverage for Tauri confirm usage on reset.
- Change zero-credit test to expect disabled and no reset call.
- Restore `confirmResetQuota` helper and imports in `views.tsx`.

**Logic design**:
- `confirmResetQuota(displayName)` builds the existing reset message.
- If `inTauri()` returns true, call `tauriConfirm(message, { title: 'Reset Quota', kind: 'warning' })`.
- On Tauri dialog error, log a warning and use `window.confirm`.
- Only call `resetAccountQuota` after confirmation returns true.

**Test design**:
- Mock `@tauri-apps/plugin-dialog`.
- With reset credits available, clicking reset should call `tauriConfirm` and then `resetAccountQuota`.
- With reset credits absent, the button should be disabled and neither `window.confirm`, `tauriConfirm`, nor `resetAccountQuota` should be called.

**Acceptance**:
- `npm run test -- src/routes/handoff.test.tsx` exits 0.
- `npm run build` exits 0.

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build command passes
- [x] This task status is updated to `验证成功`

## Test plan

- `src/routes/handoff.test.tsx`: PAT reset uses Tauri confirm.
- `src/routes/handoff.test.tsx`: zero reset credits disables reset.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused tests | `npm run test -- src/routes/handoff.test.tsx` | exit 0 |
| Build | `npm run build` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if focused tests require broader Settings-page redesign changes.
