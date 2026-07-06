# Todo: Usage Peak Fallback And Terminal Target

> Executor instructions: Follow this todo step by step. Generate tests from
> the "Test design" section before implementation. Run each verification command
> and confirm the expected result before moving to the next task.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: existing usage SQLite summary and relay launcher code
- **Category**: bugfix/feature
- **Planned at**: current workspace

## Why this matters

**Background**: The Usage dashboard can show `Peak Daily Tokens = 0` in the Total scope while child account/workspace scopes show real values. The user also wants relay/handoff to use a configurable local terminal target instead of hard-coded Terminal.app.

**Current state**: `apply_latest_account_usage_snapshot` overwrites global headline stats with the latest Codex account usage snapshot. If that snapshot contains only zero or null headline values, local peak and streak fallback values are lost. Relay launch paths in `services/relay.rs` use `terminal_applescript` for Terminal.app only.

**Impact**: Total Usage headline cards can display misleading zeros. Relay/profile flows cannot respect users who use Ghostty, cmux, or Codex app workflows.

**What improves**: Empty upstream headline snapshots will no longer erase local usage headline values. Settings will show installed launch targets and route profile login/relay commands through the selected target.

## Scope

**In scope**:
- Usage headline fallback behavior and tests.
- Terminal target discovery, persisted selection, Settings UI, and launcher integration for login/relay command opening.

**Out of scope**:
- Deep Codex app automation beyond exposing a selectable installed target and failing clearly if unsupported.
- Shell integration for every third-party terminal on every OS.

## Design

- Treat a Codex account usage snapshot as usable only if at least one headline field has a meaningful positive value.
- Preserve local fallback stats if the latest snapshot is empty/zero, and record a diagnostic.
- Store terminal target id in LAM `settings.json`.
- Discover targets from macOS application paths and PATH commands, always including Terminal.app as default.
- Launch commands by target:
  - Terminal.app via existing AppleScript.
  - Ghostty via `open -a Ghostty --args -e <shell> -lc <command>`.
  - cmux via `cmux new-session <command>`.
  - Codex app target is discoverable/configurable but command launch returns a clear unsupported error until a stable handoff API is available.
- Frontend loads terminal targets/settings and uses backend launcher through `openTerminalWithCommand`.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Preserve local peak fallback | Empty upstream snapshot does not force peak to 0 | 验证成功 |
| T2 | Add terminal target setting | Settings can choose discovered launch target and relay/login uses it | 验证成功 |

### T1: Preserve local peak fallback

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Total Usage headline cards should not show zero when local usage clearly has daily token activity.

**What to do**:
- Add a Rust usage test for an all-zero account usage snapshot.
- Update snapshot apply logic to ignore empty/zero snapshots.

**Logic design**:
- Define snapshot usable when any of lifetime, peak, longest task, current streak, longest streak is greater than zero.
- If not usable, keep local stats and add `account_usage_empty` diagnostic.

**Test design**:
- Existing local summary peak should be 60.
- Store snapshot with `Some(0)` fields.
- Assert summary source remains `local_sqlite` and peak remains 60.
- Initial failure: current implementation changes source to `codex_account_usage` and peak to 0.

**Acceptance**:
- `cargo test -p localagentmanager-core account_usage_empty_snapshot_does_not_override_local_headlines`
- Relevant usage tests pass.

### T2: Add terminal target setting

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Profile mode relay/login should respect the user's installed terminal/app workflow instead of forcing Terminal.app.

**What to do**:
- Add Rust types and commands for listing terminal targets, reading target id, and setting target id.
- Add launcher logic for selected target.
- Add TypeScript types/API/store state.
- Add Settings selector.
- Update tests for persistence/discovery and frontend API calls.

**Logic design**:
- Use `terminalTargetId` in settings.
- Validate selected id against known target ids plus `terminal`.
- `open_terminal_with_command` reads settings and dispatches to selected target.
- `open_terminal_for_login` and `open_terminal_with_resume` reuse the selected-target launcher.

**Test design**:
- Rust test persists target id while preserving existing settings.
- Rust launcher-script test covers Terminal app escaping and target resolution behavior.
- Frontend test renders Settings target selector from mocked discovered targets and saves changes.

**Acceptance**:
- Focused Rust tests pass.
- Focused Vitest Settings test passes.
- Existing handoff tests continue to call `openTerminalWithCommand`.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused Rust usage | `cargo test -p localagentmanager-core account_usage_empty_snapshot_does_not_override_local_headlines` | exit 0 after implementation |
| Focused Rust terminal | `cargo test -p localagentmanager-core terminal_target` | exit 0 after implementation |
| Focused frontend | `pnpm --dir apps/desktop test -- App.handoff.test.tsx -t terminal` | exit 0 after implementation |
| Relevant build | `pnpm --dir apps/desktop build` | exit 0 |

## Done criteria

- [x] Every task status is `验证成功`
- [x] Tests were added before implementation or exceptions documented
- [x] Verification commands pass or failures are reported with cause

## STOP conditions

- Required launcher behavior would need unsupported Codex app private API.
- Existing dirty workspace changes conflict with the requested behavior.
