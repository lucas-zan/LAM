# Todo: cmux AppleScript fallback

> Executor instructions: Follow this todo step by step. Generate tests from the
> "Test design" section before implementation. Run each verification command and
> confirm the expected result before moving to the next task.

## Status

- **Priority**: P0
- **Effort**: S
- **Risk**: MEDIUM
- **Depends on**: existing cmux terminal target
- **Category**: bugfix
- **Planned at**: current working tree

## Why this matters

**Background**: LAM currently launches cmux by running `cmux workspace create`. That is the cmux CLI, but the cmux CLI talks to the running GUI through a local Unix socket. When cmux is configured with `socketControlMode=cmuxOnly`, external apps like LAM are rejected even though manual commands work inside cmux terminals.

**Current state**: `open_cmux_with_command` returns a terminal launch error when the socket rejects LAM. The user can still run Codex manually inside cmux, so the visible failure looks like a LAM bug.

**Impact**: The `cmux` handoff target is unusable from LAM unless the user changes cmux automation settings.

**What improves**: LAM can fall back to cmux AppleScript automation when the cmux socket rejects the CLI command, keeping the cmux target useful without requiring full socket access.

## Scope

**In scope**:
- `apps/desktop/src-tauri/src/services/relay.rs` cmux launcher behavior.
- Unit tests for cmux socket rejection fallback and AppleScript script generation.
- Manual smoke test with a harmless cmux tab command.

**Out of scope**:
- Editing the user's cmux configuration.
- Changing the public Settings UI.
- Adding a separate cmux socket/password configuration flow.

## Design

The cmux launcher should still prefer the canonical CLI/socket path because it supports workspace title, cwd, and focus directly. If the CLI fails with an authorization/socket-control message such as "only processes started inside cmux can connect" or "Access denied", LAM should run an AppleScript fallback:

- Activate cmux.
- Create a new tab.
- Get the focused terminal in the new tab.
- Paste the shell command plus a newline via cmux's `input text` scripting command.

If the CLI fails for other reasons and fallback also fails, report both errors. Shell command construction remains unchanged.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Add cmux AppleScript fallback | Socket access-denied errors use AppleScript fallback; tests and smoke pass | 验证成功 |

### T1: Add cmux AppleScript fallback

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
cmux's secure default can reject LAM even when cmux itself and Codex are working.

**What to do**:
- Add a script builder for cmux AppleScript fallback.
- Detect socket authorization failures from cmux CLI stderr/stdout.
- Call fallback before returning an error for those cases.
- Preserve existing error messages for non-auth failures.

**Logic design**:
- CLI success returns `Ok(())`.
- CLI failure with access-denied/socket-control text tries AppleScript.
- AppleScript success returns `Ok(())`.
- AppleScript failure returns an error that includes both cmux CLI and AppleScript detail.

**Test design**:
- Unit test AppleScript escaping for quotes/backslashes/newlines.
- Unit test access denied text is classified as fallback-eligible.
- Unit test fallback error combines CLI and AppleScript detail through pure helper functions.

**Acceptance**:
- `cargo test -p localagentmanager-core cmux`
- Manual: a harmless AppleScript smoke command creates a cmux tab and runs `echo LAM_CMUX_APPLESCRIPT_TEST`.

**Done criteria**:
- [x] Tests were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Manual smoke check passes
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Test plan

- Script generation and escaping.
- Fallback eligibility.
- Error composition.
- Existing cmux command args tests.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused Rust tests | `cargo test -p localagentmanager-core cmux` | exit 0 |
| Manual smoke | `osascript ...` | cmux creates a tab and echoes marker |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

- cmux AppleScript cannot create a tab or input text from an external process.
- Adding fallback would require storing cmux passwords or weakening cmux security settings.
