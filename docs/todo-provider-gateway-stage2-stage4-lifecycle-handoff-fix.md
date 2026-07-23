# Todo: Gateway upstream lifecycle and listener handoff fixes

> Executor instructions: Follow this todo step by step. Generate tests from the
> "Test design" section before implementation. Run each verification command and
> confirm the expected result before moving to the next task. If a STOP condition
> occurs, stop and report instead of improvising.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: HIGH
- **Depends on**: Stage 2 Supervisor and current Stage 4 observability worktree
- **Category**: bugfix/reliability/concurrency
- **Planned at**: `20260716-gateway-external-provider` at `bf59e4e` plus uncommitted Stage 3/4 work

## Why this matters

**Background**: The Stage 2 start preflight binds and immediately releases the
stable port before spawning the sidecar. Stage 4 tracks the downstream HTTP Body,
while the separately spawned upstream stream task retains the upstream semaphore
permit.

**Current state**: Another process can bind the stable port between preflight and
sidecar startup. When a client drops a streaming response, `active_streams` becomes
zero immediately, but the upstream task may wait until another chunk or the
60-second idle timeout before observing a closed response channel and releasing
its permit.

**Impact**: Startup has a TOCTOU failure window. Stream observability can claim
that no stream is active while upstream work and capacity remain retained.

**What improves**: The parent process owns the stable listener continuously and
hands it to the sidecar as an inherited file descriptor. A downstream Body drop
cancels the upstream task immediately, and a separate activity gauge remains
nonzero until that task actually releases its resources.

## Scope

**In scope**:
- Explicit upstream stream lifecycle attachment to Gateway responses.
- Immediate upstream cancellation when the downstream Body is dropped.
- `active_upstream_streams` tracking until background task exit.
- Stable-port reservation ownership and inherited-listener handoff for desktop
  Supervisor and `lam codex` readiness launches.
- Sidecar validation and adoption of the inherited loopback listener.
- Focused failure-first tests and regression checks.

**Out of scope**:
- Stage 2 mixed transient/mismatch failure counters (review issue 1).
- Per-request post-header stream outcome logging (review issues 3/4).
- RSS methodology or Stage 5 admission enforcement.
- Windows/Linux production support; the packaged Gateway remains macOS-only.

## Design

Streaming responses may carry an optional internal lifecycle composed of a
`GatewayCancellation` and a one-shot background-completion receiver. Axum Body
drop cancels the token. Server activity increments `active_upstream_streams` when
the response is installed and decrements it only when the completion receiver
resolves or its sender is dropped. Route background tasks own the completion
sender by RAII, so every return path signals task exit without duplicated cleanup.

`GatewayStartReservation` owns both the revision-consistent runtime snapshot and
a bound IPv4 loopback `TcpListener`. Reconciliation returns this reservation
instead of a bare snapshot. Launch code installs the listener at a fixed child FD
using a child-only `pre_exec` duplication and passes the FD number via a bounded
environment contract. The sidecar validates loopback address and stable port,
converts the inherited listener to Tokio, and starts Axum from it. Dropping a
reservation without launching releases the port. No bind/drop/rebind gap remains.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Couple downstream drop to upstream task lifecycle | Client drop cancels task and releases simulated upstream capacity promptly | 验证成功 |
| T2 | Hand off a continuously reserved stable listener | Port stays owned through child launch and sidecar uses inherited listener | 验证成功 |
| T3 | Run regression gate and update remediation records | Focused suites, format, lint and diff checks pass | 验证成功 |

### T1: Couple downstream drop to upstream task lifecycle

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Downstream Body state must not be confused with the task that owns the
upstream permit.

**What to do**:
- Add an explicit response lifecycle with cancellation and completion.
- Add `active_upstream_streams` to the immutable activity snapshot.
- Attach the lifecycle to passthrough and adapted streaming route tasks.

**Logic design**:
- Body drop cancels only explicitly attached streaming work.
- Completion sender is RAII-owned by the background task.
- Task gauge decrements on normal return, error return, cancellation, or panic
  sender drop; no permit ownership is moved into observability.
- Existing downstream `active_streams` and cancellation semantics remain.

**Test design**:
- A fake stream task holds a single semaphore permit and waits for cancellation.
- Dropping the client response must make `active_upstream_streams` return to zero
  and restore the permit within a bounded deadline.
- Normal stream completion returns both downstream and upstream gauges to zero.
- Expected initial failure: no lifecycle attachment API or upstream-task gauge exists.

**Acceptance**:
- `cargo test --test provider_gateway_server streaming_activity -- --test-threads=1`
- `cargo test --test provider_gateway_routes -- --test-threads=1`

**Done criteria**:
- [x] Tests were written before implementation
- [x] New tests failed for the missing lifecycle contract
- [x] Implementation follows the lifecycle design
- [x] Client drop promptly releases simulated upstream capacity
- [x] Focused suites pass (3 lifecycle tests and 25 route tests)
- [x] Task overview and status are `验证成功`

### T2: Hand off a continuously reserved stable listener

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: A bind check followed by `drop(listener)` is not a reservation.

**What to do**:
- Return an owned listener reservation from start reconciliation.
- Add a small Unix listener-handoff boundary used by desktop and CLI launchers.
- Make the sidecar require and validate the inherited listener for packaged starts.
- Allow `GatewayLoopbackServer` to start from a supplied Tokio listener.

**Logic design**:
- Reservation construction validates revision, empty claim, loopback port, and
  control socket before returning ownership.
- Child-only FD duplication avoids globally clearing `FD_CLOEXEC` in the parent.
- Sidecar rejects missing, malformed, non-loopback, or wrong-port inherited FDs.
- Spawn/bootstrap failure drops the parent reservation and keeps errors stable.

**Test design**:
- While a reservation is alive, rebinding the stable port fails; after drop it succeeds.
- Reconciliation returns an owned reservation rather than a bare snapshot.
- A loopback server starts from a prebound listener without rebinding.
- Invalid inherited FD/address/port inputs return stable errors.
- Production source-contract tests prove both desktop and CLI configure handoff
  and sidecar adopts the inherited listener.
- Expected initial failure: preflight releases the listener and no handoff API exists.

**Acceptance**:
- `cargo test --test provider_gateway_supervisor --test provider_gateway_server --test provider_sidecar_production -- --test-threads=1`

**Done criteria**:
- [x] Tests were written before implementation
- [x] New tests failed for the released-port/missing-handoff behavior
- [x] Desktop and CLI launch paths retain and hand off the reservation
- [x] Sidecar validates and adopts the inherited listener
- [x] Focused suites pass, including a real inherited-FD child and packaged process test
- [x] Task overview and status are `验证成功`

### T3: Run regression gate and update remediation records

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: These changes cross process startup and streaming cancellation boundaries.

**What to do**:
- Run Supervisor/recovery/sidecar/server/routes/upstream/observability suites.
- Run format, changed-library Clippy, and diff hygiene.
- Update Stage 2/4 records only for these two corrected findings.

**Logic design**: Preserve admission limits, stable errors, Supervisor mismatch
policy, Provider behavior, and direct `/responses` routing.

**Test design**: Combined regression commands below.

**Acceptance**: Every verification command exits zero.

**Done criteria**:
- [x] Relevant regression suites pass (90 focused tests plus packaged process test)
- [x] Format, changed-library Clippy, and diff checks pass
- [x] Documentation accurately records the correction
- [x] Task overview and status are `验证成功`

## Test plan

- Normal: stream completes and inherited listener serves requests.
- Edge: client drops before upstream emits another chunk.
- Invalid input: inherited listener FD/address/port is rejected.
- Error: child spawn or completion sender drop releases owned resources.
- State/conflict: revision/claim/occupied port still block reservation.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Stream lifecycle | `cargo test --test provider_gateway_server streaming_activity -- --test-threads=1` | exit 0 after implementation; compile failure before |
| Listener handoff | `cargo test --test provider_gateway_supervisor --test provider_gateway_server --test provider_sidecar_production -- --test-threads=1` | exit 0 after implementation; new reservation test fails before |
| Gateway regression | `cargo test --test provider_gateway_routes --test provider_gateway_upstream --test provider_gateway_observability -- --test-threads=1` | exit 0 |
| Supervisor regression | `cargo test --test provider_gateway_recovery --test provider_gateway_sidecar -- --test-threads=1` | exit 0 |
| Quality | `cargo fmt --all -- --check`; changed-library Clippy; `git diff --check` | exit 0 |

## Done criteria

- [x] Every task's Done criteria is checked
- [x] Every task has exactly one checked status and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

- Safe listener inheritance requires globally disabling `FD_CLOEXEC` in the parent.
- Client cancellation cannot reach the upstream task without exposing request data.
- The sidecar cannot validate ownership/address/port of the inherited listener.
- Fixing either issue requires changing admission capacity or Stage 5 semantics.
- A task still fails after five focused fix loops.
