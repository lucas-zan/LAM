# Todo: cmux Handoff Socket Error

> Executor instructions: Fix this as a small TDD bugfix. Add/update tests first,
> confirm the current launcher shape is wrong, then implement and verify.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: terminal target setting implementation
- **Category**: bugfix
- **Planned at**: current workspace

## Why this matters

**Background**: With cmux selected in Settings, clicking Handoff prints a cmux socket error and no relay terminal appears.

**Current state**: LAM invokes the legacy `cmux new-workspace` form and only checks process status. It does not silence cmux's legacy notice or return stderr details to the UI.

**Impact**: Handoff looks like it does nothing and the user has to inspect the dev terminal to see the real cmux error.

**What improves**: LAM uses canonical cmux workspace creation and returns actionable cmux errors when launch fails.

## Scope

**In scope**:
- cmux launcher args.
- cmux stderr capture and error propagation.
- Focused Rust tests.

**Out of scope**:
- Changing cmux socket authentication or cmux app internals.

## Design

- Use `cmux workspace create --name "LAM Handoff" --command <command> --focus true`.
- Set `CMUX_QUIET=1`.
- Use `Command::output()` and include stderr/stdout in the `AppError` message on non-zero exit.

## Tasks

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Canonical cmux launcher | cmux args use `workspace create` and failures include stderr | 验证成功 |

### T1: Canonical cmux launcher

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Test design**:
- Update cmux args unit test to expect canonical `workspace create`.
- Add unit test for formatting stderr in a failed launch error.

**Acceptance**:
- `cargo test -p localagentmanager-core cmux_launcher`
- `cargo test -p localagentmanager-core terminal_target`

## Done criteria

- [x] T1 status is `验证成功`
- [x] Focused tests pass
