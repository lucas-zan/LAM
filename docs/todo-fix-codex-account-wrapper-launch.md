# Todo: 修复 API 账号 Codex wrapper 启动

> Executor instructions: Follow this todo step by step. Generate tests from
> the "Test design" section before implementation. Run each verification command
> and confirm the expected result before moving to the next task. If a STOP
> condition occurs, stop and report instead of improvising.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: existing Provider Gateway launcher and package manifest
- **Category**: bugfix
- **Planned at**: `codex/202607071457-ui-optimize-20260710102915-remote-provider-gateway`

## Why this matters

**Background**: LAM creates `~/bin/codex-<account>` for API accounts, but the generated wrapper executes the bare command `lam`. On macOS that name resolves to the system `/usr/bin/lam` text utility.

**Current state**: `codex-idragon3` reaches its wrapper and fails with `lam: codex: No such file or directory`. Development startup also does not build the launcher, Gateway, and auth-helper sibling binaries, while generated profile configuration can reference those development binaries.

**Impact**: A successfully created API account cannot be launched from a project directory. Credentials and upstream API configuration are never reached.

**What improves**: `codex-<account>` uses a verified absolute LAM launcher, existing managed wrappers are repaired at application startup, and development startup prepares all runtime components.

## Scope

**In scope**:
- Absolute launcher discovery behind a runtime boundary.
- Managed wrapper generation and startup repair.
- Development component build wiring.
- Focused Rust and Node tests plus a real `codex-idragon3 --version` smoke test.

**Out of scope**:
- Adding an `idragon3` alias.
- Changing Provider credentials, upstream endpoints, or Codex protocol behavior.
- Reworking the Gateway architecture.

## Design

- Input: a validated profile id and an executable LAM launcher resolved from an explicit debug override or the verified application installation.
- Output: an executable `~/bin/codex-<profile>` that exports the matching `CODEX_HOME` and execs the absolute launcher with all arguments preserved.
- Never resolve the launcher by the bare name `lam`; this prevents collision with `/usr/bin/lam`.
- Keep discovery in `provider_runtime`, separate from account filesystem operations.
- On startup, repair only LAM-managed, non-main wrappers. Do not touch unmanaged profiles.
- Development startup must build `lam`, `lam-provider-gateway`, and `lam-auth-helper` before Tauri launches.
- Missing or invalid launch components must fail explicitly rather than creating another broken wrapper.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Absolute launcher wrapper contract | Generated wrappers contain an absolute launcher and never `exec 'lam'` | 验证成功 |
| T2 | Runtime preparation and existing-account recovery | Dev components exist, startup repairs managed wrappers, and `codex-idragon3 --version` launches | 验证成功 |

### T1: Absolute launcher wrapper contract

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: The current wrapper dispatches to the unrelated macOS `/usr/bin/lam` command.

**What to do**:
- Add an absolute launcher resolution contract in `provider_runtime`.
- Make account, relay, rename, and session-profile wrapper writes use the resolved path.
- Keep argument forwarding and `CODEX_HOME` behavior unchanged.

**Logic design**:
- Debug resolution accepts a validated `LAM_LAUNCHER_EXECUTABLE` override, then checks sibling build locations.
- Release resolution uses the signed install manifest's launcher component.
- Wrapper rendering accepts the resolved absolute path and rejects invalid profile ids/arguments through the existing planner.
- Account mutations propagate resolution errors without partial wrapper success.

**Test design**:
- Normal: wrapper contains an absolute launcher and forwards `"$@"`.
- Edge: launcher paths containing spaces are shell quoted.
- Invalid input: invalid profile ids remain rejected.
- Error: wrapper never contains the bare `exec 'lam'` form.

**Acceptance**:
- `cargo test --test provider_codex_launch_planner --test phase1_core wrapper`
- `cargo fmt --all -- --check`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T2: Runtime preparation and existing-account recovery

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Existing wrappers retain the broken command, and Tauri development startup can project helper paths without ensuring all sibling executables exist.

**What to do**:
- Add an idempotent repair operation for LAM-managed wrappers and call it during application startup.
- Add a development packaging mode that builds all three runtime binaries before `tauri dev`.
- Repair the current `idragon3` wrapper through the same production function and smoke-test it.

**Logic design**:
- Scan accounts, select only `managed && id != main`, compare expected wrapper contents, and atomically rewrite only missing/stale wrappers.
- Preserve unmanaged files and profiles.
- Development preparation invokes Cargo with explicit binary targets and no release staging/signing.
- CLI and desktop Gateway startup share a short private control-socket path that fits Darwin's 104-byte Unix path limit.

**Test design**:
- Normal: stale managed wrapper is replaced with the absolute launcher.
- Edge/state: already-current wrapper is idempotent; missing managed wrapper is recreated.
- Conflict: unmanaged profile/wrapper is not changed.
- Build contract: Node test proves development mode invokes all required Cargo binary targets.
- macOS path: a representative Darwin temporary root produces a control socket shorter than 104 bytes.
- Common install: an npm/bun `codex` symlink resolves to the owner-controlled native platform binary before the sanitized launch.
- Smoke: current `codex-idragon3 --version` reaches Codex through LAM rather than `/usr/bin/lam`.

**Acceptance**:
- `cargo test --test phase1_core repair_managed_wrappers`
- `node --test scripts/package-gateway-components.test.mjs`
- `codex-idragon3 --version`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Test plan

- Verify absolute and shell-safe launcher rendering.
- Verify invalid profile rejection and absence of a bare `lam` dispatch.
- Verify stale, missing, current, and unmanaged wrapper recovery states.
- Verify development component build arguments.
- Verify the real account wrapper reaches Codex CLI.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Planner/account tests | `cargo test --test provider_codex_launch_planner --test phase1_core wrapper` | exit 0 after implementation; expected assertion failure before implementation |
| Recovery test | `cargo test --test phase1_core repair_managed_wrappers` | exit 0 |
| Packaging test | `node --test scripts/package-gateway-components.test.mjs` | exit 0 |
| Rust formatting | `cargo fmt --all -- --check` | exit 0 |
| Real smoke | `codex-idragon3 --version` | prints Codex version, not `/usr/bin/lam` error |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

- Existing user changes overlap in a way that cannot be preserved.
- Signed package verification cannot expose an absolute launcher safely.
- Required tests cannot isolate user credentials or would send an upstream API request.
- Verification still fails after five fix iterations.
- Completion requires modifying credentials or Provider endpoints.
