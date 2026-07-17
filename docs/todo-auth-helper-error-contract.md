# Todo: Auth Helper 错误码契约修复

> Executor instructions: Use the existing failing contract test as the Red phase,
> change only the inconsistent error mapping, then rerun the focused and full Rust suites.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: bugfix
- **Planned at**: current workspace, 2026-07-16

## Why this matters

**Background**: `materialize_codex_auth()` validates the helper through `validate_helper_executable()`. Missing-helper resolution already uses `AUTH_HELPER_EXECUTABLE_INVALID`, but direct runtime validation returns `PROVIDER_AUTH_HELPER_INVALID`.

**Current state**: `provider_auth_runtime::unresolved_or_unsafe_auth_runtime_fails_closed` fails because actual and documented/tested error codes differ.

**Impact**: callers cannot rely on one stable recovery code for unavailable or unsafe auth helper executables; the complete Rust suite is red.

**What improves**: all auth-helper executable validation paths return `AUTH_HELPER_EXECUTABLE_INVALID`.

## Scope

**In scope**: change the error code passed by `validate_helper_executable()` and verify existing tests.

**Out of scope**: error message changes, helper discovery redesign, Provider revision behavior, Gateway scheduling.

## Design

Reuse the existing `AUTH_HELPER_EXECUTABLE_INVALID` contract. Do not change validation behavior or broaden accepted executables.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|---|---|---|---|
| T1 | Unify helper validation error code | Focused contract and full Rust tests pass | 验证成功 |

### T1: Unify helper validation error code

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Two paths for the same invalid helper condition expose different stable codes.

**What to do**: Change only `validate_helper_executable()` to use `AUTH_HELPER_EXECUTABLE_INVALID`.

**Logic design**: Keep fail-closed path, absolute-path checks, owner/permission checks and canonicalization unchanged.

**Test design**: Use existing `unresolved_or_unsafe_auth_runtime_fails_closed`; it already failed with actual `PROVIDER_AUTH_HELPER_INVALID` versus expected `AUTH_HELPER_EXECUTABLE_INVALID`.

**Acceptance**:

- `cargo test --test provider_auth_runtime unresolved_or_unsafe_auth_runtime_fails_closed`
- `cargo test --tests -- --test-threads=1`

**Done criteria**:

- [x] Existing test was confirmed failing for the expected error-code mismatch
- [x] Only the inconsistent code mapping changed
- [x] Focused test passes
- [x] Full Rust suite passes
- [x] Task status is `验证成功`

## Test plan

- Relative helper path fails closed with `AUTH_HELPER_EXECUTABLE_INVALID`.
- Missing Gateway binding continues to return `GATEWAY_BINDING_NOT_FOUND`.
- Full suite detects any unintended contract change.

## Verification commands

| Purpose | Command | Expected |
|---|---|---|
| Focused | `cd apps/desktop/src-tauri && cargo test --test provider_auth_runtime unresolved_or_unsafe_auth_runtime_fails_closed` | exit 0 |
| Full | `cd apps/desktop/src-tauri && cargo test --tests -- --test-threads=1` | exit 0 |

## Done criteria

- [x] T1 is `验证成功`
- [x] No STOP condition remains

## STOP conditions

- Another documented caller requires `PROVIDER_AUTH_HELPER_INVALID`.
- Focused behavior changes beyond the stable error code.
