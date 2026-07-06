# Todo: Handoff error feedback

> Executor instructions: Follow this todo step by step. Generate tests from the
> "Test design" section before implementation. Run each verification command and
> confirm the expected result before moving to the next task.

## Status

- **Priority**: P0
- **Effort**: S
- **Risk**: MEDIUM
- **Depends on**: terminal target selection already wired
- **Category**: bugfix
- **Planned at**: current working tree

## Why this matters

**Background**: Clicking Relay/Handoff while `terminalTargetId=cmux` can fail when the cmux socket returns `Broken pipe`. The UI currently looks like nothing happened.

**Current state**: `relaySessionTo` catches terminal launch errors and writes `app.error`, but `Start Handoff` always closes the modal and calls `refresh()`. `refresh()` clears the error, so the failure can disappear immediately.

**Impact**: Users cannot tell whether the button uses the selected terminal, whether the action started, or why no terminal opened.

**What improves**: Relay/Handoff failures remain visible and the modal stays open when the terminal launch fails.

## Scope

**In scope**:
- Account/session relay state return value.
- Handoff modal success/failure control flow.
- Focused frontend tests for failed and successful handoff.

**Out of scope**:
- Restarting or repairing cmux itself.
- Changing terminal selection storage.

## Design

`relaySessionTo` should return a boolean success value. Known errors continue to be formatted into app error state, but callers can distinguish success from failure. The Handoff modal should only close and refresh when `relaySessionTo` returns `true`; when it returns `false`, keep the modal open so the visible error is not cleared by refresh.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Preserve failed Handoff error | Tests prove failed terminal launch keeps modal open and error visible | 验证成功 |

### T1: Preserve failed Handoff error

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
cmux socket failures must be visible instead of being erased by the automatic refresh after a failed Handoff.

**What to do**:
- Make `relaySessionTo` and `relayResumeTo` report success/failure.
- Update the Handoff modal click handler to close/refresh only on success.
- Keep existing same-account resume and Relay Latest behavior intact.

**Logic design**:
- Return `true` after same-account `openResume` succeeds.
- Return `true` after relay/copy and terminal command launch succeeds.
- Return `false` after a caught error and leave `app.error` populated.
- For no active source session, return `false`.

**Test design**:
- Mock `openTerminalWithCommand` rejection and verify `Start Handoff` keeps the modal open and displays the error.
- Mock a successful handoff and verify the modal closes.

**Acceptance**:
- `node_modules/.bin/vitest run App.handoff.test.tsx -t "keeps the handoff modal open"`
- Existing handoff tests still pass.

**Done criteria**:
- [x] Tests were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Test plan

- Failed cmux/terminal launch path.
- Successful handoff path.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused test | `node_modules/.bin/vitest run App.handoff.test.tsx -t "keeps the handoff modal open"` | exit 0 after implementation |
| Handoff suite | `node_modules/.bin/vitest run App.handoff.test.tsx` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

- Cannot write to the repository.
- Tests cannot run due sandbox restrictions that cannot be escalated.
