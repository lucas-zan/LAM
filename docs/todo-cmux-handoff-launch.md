# Todo: cmux Handoff Launch

> Executor instructions: Fix this as a small TDD bugfix. Add the test first,
> confirm it fails for the current launcher args, then implement and verify.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: terminal target setting implementation
- **Category**: bugfix
- **Planned at**: current workspace

## Why this matters

**Background**: Settings can select cmux, but clicking Handoff does not open a terminal/workspace.

**Current state**: The cmux launcher calls `cmux new-session <command>`. The installed cmux CLI help does not expose `new-session`; it supports `new-workspace --command <text>`.

**Impact**: Relay/resume/login commands fail silently from the user's perspective when cmux is selected.

**What improves**: cmux handoff creates a focused cmux workspace and runs the generated Codex command there.

## Scope

**In scope**:
- cmux launcher command construction.
- Focused Rust test for cmux args.

**Out of scope**:
- Frontend UI redesign.
- Advanced cmux pane/surface targeting.

## Design

- Build cmux launcher args with `new-workspace --name "LAM Handoff" --command <command> --focus true`.
- Keep bundled app binary resolution from the previous cmux detection fix.

## Tasks

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Use cmux new-workspace | cmux launcher args use supported `new-workspace --command` form | 验证成功 |

### T1: Use cmux new-workspace

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Test design**:
- Add a unit test for `cmux_command_args`.
- Expected initial failure: args contain `new-session`.

**Acceptance**:
- `cargo test -p localagentmanager-core cmux_launcher_uses_new_workspace_command`
- `cargo test -p localagentmanager-core terminal_target`

## Done criteria

- [x] T1 status is `验证成功`
- [x] Focused tests pass
