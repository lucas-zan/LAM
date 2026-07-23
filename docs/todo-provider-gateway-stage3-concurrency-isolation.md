# Todo: Provider Gateway Stage 3 concurrency isolation

> Executor instructions: Follow this todo step by step. Generate tests from the
> "Test design" section before implementation. Run each verification command and
> confirm the expected result before moving to the next task. If a STOP condition
> occurs, stop and report instead of improvising.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: HIGH
- **Depends on**: Stage 2 verified; current 32/4/64/8/16 capacity contract
- **Category**: bugfix/reliability/concurrency
- **Planned at**: `20260716-gateway-external-provider` at `bf59e4e`

## Why this matters

**Background**: Stage 3 is the minimal concurrency isolation correction from
`todo-provider-gateway-review-remediation.md`. Gateway admission currently
acquires the global semaphore before the per-Binding semaphore in both the
immediate and queued paths.

**Current state**: In `gateway/server.rs`, a queued request can acquire a global
permit and then wait for its busy Binding. A different Binding is forced to queue
even when its own per-Binding capacity is free. Queue bounds are global=64 and
Binding=8; running bounds are global=32 and Binding=4; the upstream client has an
independent fail-fast bound of 16.

**Impact**: A busy Binding can reserve global processing capacity while it is not
running, causing unrelated Bindings to wait or time out. Incorrect cancellation
handling could also leak the first permit while the second is pending.

**What improves**: Admission always reserves Binding capacity before global
capacity. Waiting requests do not consume global capacity, and one RAII owner
releases both permits on normal completion, handler error, timeout, or future
cancellation.

## Scope

**In scope**:
- Freeze the production 32/4/64/8/16 capacity and stable error-code baseline.
- Unify immediate and queued admission as Binding-first, then global.
- Add cross-Binding isolation and RAII release tests.
- Update the parent Stage 3 checklist and verification record.

**Out of scope**:
- Changing any capacity, queue bound, timeout, or error code.
- Changing upstream `try_acquire_owned()` behavior.
- Admission Controller, CapacityKey, fairness, borrowing, or metrics.
- Provider API decomposition.

## Design

`GatewayAdmissionPermits` is a focused RAII value containing one Binding permit
and one global permit. Its immediate and asynchronous constructors both accept
the Binding semaphore first and acquire it first. If global acquisition fails,
times out, or the future is cancelled, the local Binding permit is dropped. The
dispatch function owns this value until handler completion and then drops it
before response mapping. Existing queue guards, total deadline calculation,
status codes, and upstream admission remain unchanged.

The behavioral proof uses two authenticated Binding snapshots. Binding A holds
one running request and queues a second. With global capacity two and Binding
capacity one, Binding B must start and finish while A's first request remains
blocked. This fails under global-first queued acquisition because A's waiter
reserves the second global permit.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Freeze Stage 3 baseline | Production limits and stable errors remain explicit | 验证成功 |
| T2 | Implement Binding-first RAII admission | Cross-Binding request proceeds; permits release on all exits | 验证成功 |
| T3 | Run regression gate and update plan | Focused/full suites, formatting, lint and docs pass | 验证成功 |

### T1: Freeze Stage 3 baseline

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Stage 3 must change acquisition order without silently changing
capacity or rejection behavior.

**What to do**:
- Add a production source-contract test for 32/4/64/8 and upstream 16.
- Preserve existing 429 `GATEWAY_CONCURRENCY_LIMIT` and 504
  `GATEWAY_REQUEST_TIMEOUT` behavior tests.

**Logic design**: Treat production capacity literals and existing error responses
as a frozen compatibility contract for this stage.

**Test design**:
- Assert the production sidecar config contains exactly the planned limits.
- Run the existing queue-overflow and timeout tests unchanged.
- This is a characterization test and is expected to pass before implementation.

**Acceptance**:
- `cargo test --test provider_gateway_server production_concurrency_baseline -- --exact`

**Done criteria**:
- [x] Tests listed above were written before implementation
- [x] Passing characterization baseline was recorded before behavior changes
- [x] Capacity and error contracts remain unchanged
- [x] Focused verification passes (1 passed, 0 failed)
- [x] Task overview row and task status are `验证成功`

### T2: Implement Binding-first RAII admission

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Waiting on a busy Binding must not consume scarce global capacity.

**What to do**:
- Add the RAII admission owner in `gateway/server.rs`.
- Use it in immediate and queued dispatch paths.
- Add behavioral cross-Binding and private permit-release tests.

**Logic design**:
- Immediate: try Binding, then global; failure drops any first permit.
- Queued: await Binding, then global inside the existing total timeout.
- Cancellation or acquisition error relies on Rust drop semantics.
- Handler completion/error/timeout drops the combined owner exactly once.

**Test design**:
- Two Binding integration test proves B completes while A remains blocked.
- Global-unavailable immediate acquisition releases Binding immediately.
- Cancellation while waiting for global releases Binding.
- Dropping successful admission restores both semaphore counts.
- Expected initial failure: the cross-Binding request times out under the current
  global-first queued path and the RAII type does not exist.
- Handler-error release was already correct before Stage 3; its added regression
  assertion cannot be made red without intentionally breaking established behavior.

**Acceptance**:
- `cargo test --test provider_gateway_server -- --test-threads=1`
- `cargo test gateway_admission --lib -- --test-threads=1`

**Done criteria**:
- [x] Tests listed above were written before implementation
- [x] New tests failed for the expected old acquisition order/missing RAII API
- [x] Implementation follows the Binding-first design
- [x] Cross-Binding and RAII tests pass (3 unit tests and the isolation test)
- [x] Existing server tests pass without changed expectations (10/10)
- [x] Task overview row and task status are `验证成功`

### T3: Run regression gate and update plan

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Concurrency changes require regression evidence beyond one scheduling
scenario.

**What to do**:
- Run Gateway server, routes, upstream, Binding, and sidecar suites.
- Run Rust formatting/clippy and repository diff checks.
- Update the parent Stage 3 checklist without marking later stages complete.

**Logic design**: Record unrelated failures instead of broadening scope. Do not
change scheduling capacity, upstream behavior, or errors to make tests pass.

**Test design**: Combined regression commands below; no new behavior is added in
this task.

**Acceptance**:
- All commands in Verification commands exit zero.

**Done criteria**:
- [x] All relevant regression commands pass
- [x] Formatting, changed-library clippy, and diff hygiene pass
- [x] Parent plan and this execution record are accurate
- [x] Task overview row and task status are `验证成功`

## Test plan

- Normal: free Binding and global capacity admits immediately.
- Isolation: Binding A waiter holds no global capacity needed by Binding B.
- Edge: queue bounds remain global=64 and Binding=8.
- Error: overflow remains 429; total deadline remains 504.
- State/cancellation: failure or cancellation during second acquisition releases
  the first permit; successful owner drop releases both.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Server behavior | `cargo test --test provider_gateway_server -- --test-threads=1` | exit 0 after implementation; new isolation test fails before implementation |
| Admission unit tests | `cargo test gateway_admission --lib -- --test-threads=1` | exit 0 after implementation; compile failure before implementation |
| Gateway regression | `cargo test --test provider_gateway_routes --test provider_gateway_upstream --test provider_gateway_binding --test provider_gateway_sidecar -- --test-threads=1` | exit 0 |
| Rust quality | `cargo fmt -- --check`; changed-library Clippy with recorded baseline exceptions | exit 0 |
| Diff hygiene | `git diff --check` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## Verification record

**Verified at**: 2026-07-17

- Failure-first isolation proof: old global-first code blocked Binding B beyond
  150ms while Binding A had one running request and one waiter.
- Gateway server suite: 10 passed, 0 failed.
- Admission RAII unit tests: 3 passed, 0 failed.
- Routes/upstream/Binding/sidecar regression: 50 passed, 0 failed.
- `cargo fmt -- --check` and `git diff --check`: passed.
- Changed-library Clippy passed with explicit allowances for three pre-existing
  lint categories outside `gateway/server.rs`. Full-target strict Clippy remains
  blocked by six pre-existing findings in `account.rs`, `gateway/supervisor.rs`,
  and `bin/lam.rs`; none is introduced or touched by Stage 3.
- No capacity, queue bound, timeout, error code, or upstream admission behavior
  changed.

## STOP conditions

- Correct isolation requires changing 32/4/64/8/16 capacities or error codes.
- The existing upstream fail-fast semaphore must be replaced to complete Stage 3.
- Cross-Binding behavior cannot be isolated with deterministic local tests.
- The change requires Stage 4 metrics or Stage 5 Admission Controller semantics.
- Focused verification still fails after five fix loops.
