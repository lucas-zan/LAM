# Todo: cmux Terminal Detection

> Executor instructions: Fix this as a small TDD bugfix. Add the test first,
> confirm it fails for the current detection logic, then implement and verify.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: terminal target setting implementation
- **Category**: bugfix
- **Planned at**: current workspace

## Why this matters

**Background**: Settings only shows Terminal.app and Codex app on a machine that has cmux installed under `/Applications/cmux.app`.

**Current state**: `list_terminal_targets` marks cmux installed only when `cmux` is available through the process `PATH`. macOS GUI apps often do not inherit the user's login shell PATH.

**Impact**: Profile relay/login cannot select cmux even though it is locally installed.

**What improves**: LAM detects cmux from the app bundle as well as from CLI PATH.

## Scope

**In scope**:
- cmux installed detection.
- Focused Rust test for target construction.

**Out of scope**:
- Changing the Settings UI layout.
- Deep cmux handoff API integration beyond selecting it as the command launch target.

## Design

- Build terminal target rows from probe booleans.
- Mark cmux installed when either app bundle or CLI probe succeeds.
- Check both lowercase and capitalized macOS bundle names.

## Tasks

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Detect cmux app bundle | cmux target installed when app probe is true even if CLI probe is false | 验证成功 |

### T1: Detect cmux app bundle

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Test design**:
- Add a relay unit test that constructs terminal targets with `cmux_app=true` and `cmux_cmd=false`.
- Expected initial failure: cmux is not marked installed.

**Acceptance**:
- `cargo test -p localagentmanager-core cmux_app_bundle_marks_target_installed`
- `cargo test -p localagentmanager-core terminal_target`

## Done criteria

- [x] T1 status is `验证成功`
- [x] Focused tests pass
