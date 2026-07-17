# Todo: Restore the official-account direct launch boundary

> Executor instructions: Follow this todo step by step. Generate tests from the
> Test design sections before implementation, confirm the expected failure, and
> stop if a listed STOP condition occurs.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: HIGH
- **Depends on**: existing Provider binding route planner
- **Category**: bugfix/refactor
- **Planned at**: `20260716-gateway-external-provider` / `5485db0`

## Why this matters

**Background**: Before the External API Gateway work, managed official Codex
accounts used a self-contained wrapper that exported `CODEX_HOME`, resolved the
user's `codex` executable, and executed it directly. The Gateway change replaced
all managed wrappers with `lam codex --profile`, including official accounts.

**Current state**: Missing bindings are correctly classified as
`RouteKind::Direct`, but every wrapper enters `lam`; `lam` verifies the complete
Gateway component manifest before it reaches the Direct branch. A stale Gateway
manifest therefore prevents official accounts from launching.

**Impact**: An External API component rebuild, signing problem, manifest drift,
or Gateway installation failure creates a shared failure domain and disables
unrelated official accounts.

**What improves**: Official accounts regain their previous independent launch
path. Only profiles whose binding route is Gateway depend on LAM Gateway
components. Gateway integrity verification remains mandatory for Gateway routes.

## Scope

**In scope**:
- Restore the historical direct wrapper for official/Direct accounts.
- Make the LAM wrapper explicitly Gateway-only.
- Create and repair managed wrappers according to the planned/stored route.
- Ensure Direct wrapper creation and repair do not resolve Gateway components.
- Add regression tests for both dependency boundaries.

**Out of scope**:
- Weakening Gateway manifest, hash, identity, or signing verification.
- Changing provider protocol routing decisions.
- Redesigning Provider binding persistence.

## Design

- `CodexLaunchPlanner` owns only the Gateway wrapper format.
- `account` owns the historical Direct wrapper because it is part of ordinary
  account lifecycle and has no Gateway dependency.
- Ordinary account/session/relay creation defaults to Direct.
- External API account creation passes its already-computed `RouteKind` into an
  internal route-aware account creation function.
- Startup wrapper repair resolves each managed profile route and writes either
  the Direct or Gateway wrapper. Launcher discovery is lazy and occurs only for
  Gateway profiles.
- `lam codex` retains strict verification for Gateway calls; official wrappers
  never enter that executable.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Restore distinct wrapper contracts | Direct wrapper bypasses LAM; Gateway wrapper uses LAM | 验证成功 |
| T2 | Apply route-aware lifecycle behavior | Official creation/repair is independent; External API uses planned route | 验证成功 |
| T3 | Enforce the boundary inside `lam` | Direct requests cannot reach manifest verification | 验证成功 |

### T1: Restore distinct wrapper contracts

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: A single wrapper contract incorrectly couples two independent account
types and makes route checks happen after Gateway-only preconditions.

**What to do**:
- Restore the historical `CODEX_HOME`/`CODEX_BIN` Direct wrapper.
- Rename or narrow the planner wrapper API so it is unambiguously Gateway-only.
- Preserve argument forwarding and shell-safe profile handling.

**Logic design**:
- Direct wrapper resolves `CODEX_BIN`, falling back to `command -v codex`, emits
  an actionable error with exit 127, then `exec`s Codex with unchanged arguments.
- Gateway wrapper invokes the verified LAM launcher with profile identity.

**Test design**:
- Change ordinary account tests to require `exec "$CODEX_BIN" "$@"` and reject
  `lam codex`.
- Keep a dedicated Gateway wrapper test requiring `lam codex --profile`.
- Confirm the changed ordinary-account test fails against the current code.

**Acceptance**:
- `cargo test --test phase1_core creates_managed_account_with_plan_and_safe_wrapper`
- `cargo test --test provider_codex_launch_planner wrapper`

**Done criteria**:
- [x] Tests were written before implementation
- [x] Expected failure was confirmed
- [x] Implementation follows the design
- [x] Focused tests pass
- [x] Relevant suite passes
- [x] Status is `验证成功`

### T2: Apply route-aware lifecycle behavior

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Restoring the wrapper string alone is insufficient unless creation and
automatic repair consistently select the correct contract.

**What to do**:
- Add route-aware internal account creation while keeping public ordinary account
  creation Direct by default.
- Pass the planned External API route during API account execution.
- Repair existing managed wrappers based on persisted bindings with lazy Gateway
  launcher resolution.

**Logic design**:
- A missing binding means Direct.
- Direct creation/repair cannot call launcher resolution.
- Gateway creation/repair must fail closed if the launcher cannot be resolved.
- API account compensation semantics remain unchanged.

**Test design**:
- Ordinary wrapper repair restores Direct wrappers and remains idempotent.
- Gateway-bound wrapper repair writes the Gateway wrapper.
- API account execution produces a wrapper consistent with its planned route.
- Confirm at least the ordinary repair assertion fails before implementation.

**Acceptance**:
- `cargo test --test phase1_core repair_managed_wrappers`
- `cargo test --test provider_phase4_api_account`
- Full relevant Rust test suite and formatting check.

**Done criteria**:
- [x] Tests were written before implementation
- [x] Expected failure was confirmed
- [x] Implementation follows the design
- [x] Focused tests pass
- [x] Relevant suite passes
- [x] Status is `验证成功`

### T3: Enforce the boundary inside `lam`

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Existing wrappers, manual `lam codex` calls, or interrupted migrations
must not reintroduce the shared Gateway failure domain.

**What to do**:
- Introduce a Direct launcher that has no `VerifiedInstallation` dependency.
- Branch in `lam` before reading the Gateway manifest.
- Keep the existing verified launcher exclusive to Gateway routes.

**Logic design**:
- Direct launcher requires an explicit owner-controlled Codex executable and
  validates `CODEX_HOME`, cwd, arguments, and environment as before.
- Gateway requests are rejected by the Direct launcher.
- `lam` constructs the common launch request, returns immediately for Direct,
  and only then resolves/verifies the Gateway installation for Gateway.

**Test design**:
- Launch a fake external Codex through the Direct launcher without constructing
  any install manifest or verified installation.
- Assert argument, cwd, `CODEX_HOME`, and exit-code preservation.
- Assert a Gateway request cannot use the Direct launcher.

**Acceptance**:
- `cargo test --test provider_gateway_launcher direct_launcher`
- `cargo test --bin lam`

First validation found only a macOS `/var` versus `/private/var` cwd
canonicalization mismatch in the new assertion; launcher behavior and route
rejection were otherwise correct.

**Done criteria**:
- [x] Tests were written before implementation
- [x] Expected failure was confirmed
- [x] Implementation follows the design
- [x] Focused tests pass
- [x] Relevant suite passes
- [x] Status is `验证成功`

## Test plan

- Official account creation produces the historical Direct wrapper.
- Direct wrapper preserves arguments and supports `CODEX_BIN` override.
- Gateway wrapper remains routed through `lam codex --profile`.
- Repair chooses Direct without Gateway launcher dependency.
- Repair chooses Gateway for a Gateway binding.
- External API creation uses the route frozen into its plan.
- Direct `lam` invocation works without any Gateway installation manifest.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused Direct tests | `cargo test --test phase1_core creates_managed_account_with_plan_and_safe_wrapper repair_managed_wrappers` | exit 0 |
| Focused route tests | `cargo test --test provider_codex_launch_planner` | exit 0 |
| API account tests | `cargo test --test provider_phase4_api_account` | exit 0 |
| Full relevant suite | `cargo test` | exit 0 |
| Format | `cargo fmt --check` | exit 0 |

## Done criteria

- [x] Every task is `验证成功`
- [x] Every task checklist is complete
- [x] Full relevant tests pass
- [x] No STOP condition remains

## Verification result

- All focused boundary, wrapper lifecycle, API account, launcher, formatting,
  packaging-process, and remaining Rust tests passed.
- A full unfiltered run exposed one unrelated existing supervisor source-string
  assertion (`production_supervisor_no_longer_uses_pid_existence_as_health`).
  Re-running the complete suite with only that test excluded passed. Its source
  and test were already part of separate uncommitted supervisor work and were
  intentionally left unchanged by this task.

## STOP conditions

- The External API account route is not available before account creation.
- Route-aware wrapper repair would require weakening Gateway verification.
- Existing unrelated worktree changes conflict with the required functions.
- Tests reveal official accounts require Gateway-owned configuration at runtime.
