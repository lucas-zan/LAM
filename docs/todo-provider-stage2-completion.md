# Todo: Provider / Gateway Stage 2 completion

> Executor instructions: Follow this todo step by step. Generate tests from the
> "Test design" section before implementation. Run each verification command and
> confirm the expected result before moving to the next task. If a STOP condition
> occurs, stop and report instead of improvising.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: HIGH
- **Depends on**: Stage 1 completed; existing authenticated Gateway health proof and verified install manifest
- **Category**: bugfix/reliability/security
- **Planned at**: current working tree on 2026-07-16

## Why this matters

**Background**: Stage 2 combines the already completed Attach/Detach plan clock fix with the remaining Gateway Supervisor reliability work. The production desktop supervisor currently treats `kill(pid, 0)` as sufficient proof that the claimed Gateway is healthy.

**Current state**: The dialog has a bounded internal clock and focused tests. `monitor_packaged_gateway` still checks only PID existence and therefore cannot distinguish a real Gateway from PID reuse. It also has no explicit Healthy/Suspect/Degraded/Recovering/Stopped/Starting state machine and no start preflight for the state claim, stable port, and control socket.

**Impact**: A reused PID can permanently block Gateway recovery. Conversely, an unsafe recovery implementation could kill an unrelated process or create two Gateway instances after a transient health failure.

**What improves**: Supervisor decisions become testable and identity-aware. UID/executable mismatches are treated as stale claims without signaling the foreign process; authenticated install/instance/protocol health proof identifies the managed Gateway; transient failures are retried without starting a second process; and every launch is gated by state/port/socket checks.

## Scope

**In scope**:
- Verify the already implemented plan countdown and preserve its behavior.
- Add ProcessInspector and GatewayIdentityProbe abstractions.
- Add an explicit bounded Supervisor state machine and transition reason logging.
- Integrate UID, executable, install ID, instance ID, protocol version, and state schema checks into the packaged desktop Supervisor.
- Cache verified installation identity and reload only when the manifest changes.
- Gate launch on empty state claim, available stable port, and non-live control socket.
- Add focused fault-injection tests for PID reuse, transient probe failure, authenticated identity mismatch, unknown-process no-kill behavior, and split-brain prevention.

**Out of scope**:
- Stage 3 semaphore ordering or any Gateway request scheduling/capacity change.
- Admission Controller, fairness, burst, metrics, or Provider API file splitting.
- Gateway first-response timeout Settings UI work.
- Killing a process whose managed Gateway identity cannot be proven.

## Design

- `ProcessInspector` returns no process or a lightweight `{ uid, executable }` identity. Inspection errors are transient/unknown and never authorize kill or launch.
- `GatewayIdentityProbe` verifies the authenticated `/healthz` proof with the installation key and compares `install_id`, `instance_id`, control protocol version, state schema, component version, and readiness. A cryptographically valid but different identity is distinct from timeout/unavailable.
- `GatewaySupervisorMachine` owns one state and consecutive failure count. One transient failure moves Healthy to Suspect; repeated transient failures move to Degraded but never authorize replacement. Repeated authenticated identity mismatch reaches Recovering only after the configured threshold.
- PID absent or an explicit UID/executable mismatch moves to Stopped. The stale state claim is cleared with CAS, but an unrelated process is never signaled.
- Recovering may terminate only after process UID/executable match and the identity probe returned an authenticated mismatch. Termination remains bounded and exit must be confirmed.
- Starting is allowed only after the state claim is empty, the stable loopback port can be reserved, and the control socket has no live listener. A blocked preflight keeps the Supervisor out of Starting.
- The install manifest/codesign result is cached. File metadata changes trigger re-verification; normal polling uses only the cached verified Gateway path/version.
- Existing `SupervisorPolicy` remains the bounded launch retry/backoff policy. The new identity safety rules cannot be bypassed.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Verify live plan clock | Countdown expires, disables execution, and cleans its timer | 验证成功 |
| T2 | Add identity abstractions and Supervisor state machine | Fault-injection tests prove bounded transitions and no replacement after transient failure | 验证成功 |
| T3 | Integrate safe recovery and launch preflight | Production monitor handles PID reuse without kill and prevents split-brain | 验证成功 |
| T4 | Run Stage 2 regression gate and update documentation | Focused frontend/Rust suites, build, and formatting pass | 验证成功 |

### T1: Verify live plan clock

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: This is the first half of Stage 2 and was completed immediately before the Stage 1 completion pass.

**What to do**: Preserve the internal ticking clock, deterministic `nowMs` injection, expiry disablement, and interval cleanup.

**Logic design**: Use internal time only when no injected time exists and a live plan is present.

**Test design**: Fake-time expiry, injected-time determinism, and unmount cleanup.

**Acceptance**: `cd apps/desktop && npm test -- --run src/components/provider-binding-dialog.test.tsx` and `npm run build`.

**Done criteria**:
- [x] Tests were written before implementation and failed for frozen time
- [x] Focused tests pass
- [x] Frontend build passes
- [x] Task overview row and task status are `验证成功`

### T2: Add identity abstractions and Supervisor state machine

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: PID existence cannot express process ownership, authenticated Gateway identity, or bounded handling of transient failures.

**What to do**:
- Add `ProcessInspector` and `GatewayIdentityProbe` contracts.
- Add typed process/identity observations and Supervisor states/actions.
- Add a bounded state machine with consecutive-failure tracking and redacted transition reasons.
- Implement a production health-proof identity probe.

**Logic design**:
- UID/path mismatch is definitive stale-claim evidence but never permission to signal that PID.
- Valid proof plus exact expected identity is Healthy.
- Timeout, I/O, invalid proof, or unknown inspection is transient and holds the existing instance.
- Authenticated mismatch must repeat to the threshold before Recovering.
- State transitions reset failure count only after a healthy result or a confirmed stop.

**Test design**:
- Healthy identity remains Healthy with zero failures.
- One unavailable probe becomes Suspect; repeated failures become Degraded; action remains Hold.
- A subsequent healthy proof returns to Healthy.
- Repeated authenticated mismatch reaches Recovering only at the threshold.
- PID missing and UID/path mismatch produce stale/stop actions without a terminate action.
- Process inspection/probe errors are classified as transient and never authorize start or recovery.
- Every transition exposes a bounded static reason suitable for redacted production logging.
- Expected initial failure: the public abstractions/state machine do not exist and the production monitor only calls `process_exists`.

**Acceptance**:
- `cd apps/desktop/src-tauri && cargo test --test provider_gateway_supervisor -- --skip production_supervisor_no_longer_uses_pid_existence_as_health --test-threads=1`
- The production wiring assertion remains owned by T3 and is intentionally excluded until T3 is implemented.

**Done criteria**:
- [x] Tests listed above were written before implementation
- [x] New tests failed for the expected missing API/behavior
- [x] Implementation matches the state/identity design
- [x] Focused tests pass
- [x] Formatting passes
- [x] Task overview row and task status are `验证成功`

### T3: Integrate safe recovery and launch preflight

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Correct classification is insufficient unless the production loop clears stale claims safely, refuses split-brain starts, and only terminates a verified managed process.

**What to do**:
- Cache/reload the verified installation contract.
- Replace PID-only production logic with process inspection and authenticated identity probing.
- Clear stale PID claims by CAS without signaling unrelated processes.
- Use bounded verified recovery only after authenticated mismatch reaches Recovering.
- Validate empty state claim, stable port availability, and control socket availability before Starting.
- Keep existing bounded restart/backoff behavior for launch failures.

**Logic design**:
- A preflight error is a hold/failure, never permission to delete arbitrary socket paths or start anyway.
- Recovery confirms bounded process exit before clearing the claim.
- State/revision races surface as conflicts and cause a later loop retry; stale writes are not replayed blindly.
- Manifest verification runs once and only reloads after manifest metadata changes.
- A generic `ReloadingCache` owns version comparison and swaps in a newly verified value only after loading succeeds; a failed reload keeps the last verified value but returns the error for the current cycle.
- One extracted Supervisor reconciliation function owns the observation-to-action execution. It may clear a stale CAS claim, recover only after the machine emits `RecoverVerifiedProcess`, or validate/start only from an empty claim.
- The production loop resolves a current active Binding token only for the authenticated identity probe; the token is never stored in logs or persisted outside Keychain.

**Test design**:
- Claimed state fails start preflight.
- Occupied stable port fails preflight.
- Live control socket fails preflight.
- Stale UID/path claim is clearable without any terminate call.
- Authenticated mismatch recovery calls termination only after the threshold and requires confirmed exit.
- Process inspection/probe errors hold without clearing state, terminating, or starting.
- Reload cache calls the verifier once for a stable manifest version and again only after the version changes.
- A failed reload does not replace the last verified cached value.
- Source/contract regression proves production monitor no longer uses PID-only `process_exists` as health.
- Expected initial failure: preflight/cache helpers and production integration do not exist.

**Acceptance**:
- `cd apps/desktop/src-tauri && cargo test --test provider_gateway_supervisor --test provider_gateway_recovery --test provider_gateway_sidecar --test provider_sidecar_production -- --test-threads=1`

**Done criteria**:
- [x] Tests listed above were written before implementation
- [x] New tests failed for the expected missing API/behavior
- [x] Production monitor uses the new identity-safe path
- [x] Unknown/mismatched processes are never signaled
- [x] Start preflight blocks occupied state/port/socket
- [x] Focused regression tests pass
- [x] Task overview row and task status are `验证成功`

### T4: Run Stage 2 regression gate and update documentation

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Stage 2 changes process lifecycle safety and must be accepted as a combined frontend/backend unit without changing request scheduling.

**What to do**:
- Run focused frontend countdown tests/build.
- Run focused Rust Supervisor, recovery, sidecar, production wiring, and server identity tests.
- Run formatting and diff checks.
- Update the main remediation Stage 2 checklist and this execution record.

**Logic design**: Record unrelated failures rather than broadening scope. Do not modify Stage 3 limits/order.

**Test design**: Combined regression commands below; no new behavior is added in this task.

**Acceptance**: All commands in Verification commands exit 0, except clearly documented unrelated existing failures.

**Done criteria**:
- [x] All task verification commands were run
- [x] Relevant frontend build and Rust formatting pass
- [x] Main Stage 2 status is accurate
- [x] Task overview row and task status are `验证成功`

## Test plan

- Live Attach/Detach plan expiry and timer cleanup.
- Process missing, reused PID, UID mismatch, executable mismatch, and inspection failure.
- Healthy, unavailable, and authenticated-mismatch Gateway identity probes.
- Healthy → Suspect → Degraded → Healthy and mismatch → Recovering transitions.
- Unknown process and transient probe failures never produce terminate/start actions.
- Empty claim, stable port, and control socket start gates.
- Verified recovery requires bounded confirmed exit.
- Installation verification cache reloads only after manifest metadata changes.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Countdown | `cd apps/desktop && npm test -- --run src/components/provider-binding-dialog.test.tsx` | exit 0 |
| Frontend build | `cd apps/desktop && npm run build` | exit 0 |
| Supervisor focused | `cd apps/desktop/src-tauri && cargo test --test provider_gateway_supervisor -- --test-threads=1` | exit 0 after implementation; compile/test failure before implementation |
| Supervisor regression | `cd apps/desktop/src-tauri && cargo test --test provider_gateway_supervisor --test provider_gateway_recovery --test provider_gateway_sidecar --test provider_sidecar_production --test provider_gateway_server -- --test-threads=1` | exit 0 |
| Rust formatting | `cd apps/desktop/src-tauri && cargo fmt -- --check` | exit 0 |
| Diff hygiene | `git diff --check` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No Stage 2 STOP condition remains unresolved

## Verification record

**Verified at**: 2026-07-17

- Rust Stage 2 regression: 49 passed, 0 failed across Supervisor, recovery,
  launcher, sidecar, production wiring, and server suites.
- Frontend countdown regression: 6 passed, 0 failed.
- Frontend production build: passed.
- `cargo fmt -- --check` and `git diff --check`: passed.
- No application packaging, installation, deployment, or launch was performed.

### Post-completion CLI readiness correction (2026-07-17)

The first Stage 2 pass missed the separate `lam codex` readiness path and did
not classify macOS zombie processes as stopped. This caused API-account wrappers
to return `GATEWAY_PROCESS_TERMINATION_TIMEOUT`. The CLI now uses the shared
identity observation/reconciliation path, and macOS process status inspection
treats zombies as exited. Focused regression and a real `codex-welfare
--version` smoke test pass. See `docs/todo-provider-stage2-cli-readiness-fix.md`.

### Post-completion stable listener handoff correction (2026-07-17)

The original start preflight bound and immediately released the stable port,
leaving a check-to-spawn race. Reconciliation now returns an owned listener
reservation. Desktop and `lam codex` launch paths retain it through `spawn`, copy
it to child FD 3 only inside `pre_exec`, and the sidecar validates and adopts the
inherited IPv4 loopback listener. A real child-FD test and the packaged
launcher/sidecar/Codex/upstream process test pass. See
`docs/todo-provider-gateway-stage2-stage4-lifecycle-handoff-fix.md`.

## STOP conditions

- Identity proof would require exposing a secret or weakening loopback authentication.
- Safe recovery would require signaling a process whose UID/executable and authenticated Gateway identity are not proven.
- Launch would need to proceed while state, stable port, or control socket ownership is ambiguous.
- Required work changes Stage 3 scheduling/capacity semantics.
- Focused verification still fails after five fix loops.
