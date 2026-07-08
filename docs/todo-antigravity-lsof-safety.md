# Todo: Antigravity lsof safety

> Executor instructions: Follow this todo step by step. Generate tests from
> the "Test design" section before implementation. Run each verification command
> and confirm the expected result before moving to the next task. If a STOP
> condition occurs, stop and report instead of improvising.

## Status

- **Priority**: P0
- **Effort**: S
- **Risk**: MEDIUM
- **Depends on**: none
- **Category**: bugfix
- **Planned at**: current workspace state

## Why this matters

**Background**: Antigravity quota refresh discovers the local language server port by running `lsof -Pan -p <pid> -i`.

**Current state**: `find_listening_ports` waits on `Command::output()` without a timeout. The app and tray refresh Antigravity every two minutes and do not skip overlapping refreshes.

**Impact**: A hung or busy-looping `lsof` can remain alive indefinitely. Later refreshes can start more `lsof` processes, causing CPU saturation and UI/system instability.

**What improves**: The app keeps Antigravity quota support while bounding subprocess lifetime and preventing avoidable overlapping refreshes.

## Scope

**In scope**:
- `apps/desktop/src-tauri/src/services/antigravity.rs`
- `apps/desktop/src/App.tsx`
- `apps/desktop/src/components/tray-quota-panel.tsx`
- Focused Rust and frontend tests for the changed behavior.

**Out of scope**:
- Replacing Antigravity integration with a different official API.
- Changing Codex quota refresh behavior.
- Broad UI redesign.

## Design

Expected behavior:
- Port discovery must prefer explicit command-line ports when available.
- `lsof` remains only as a bounded fallback and must be killed on timeout.
- Antigravity refresh calls from React must skip when a previous call from the same component is still in flight.

Inputs and constraints:
- Antigravity processes may expose `--https_server_port <port>`, `--https_server_port=<port>`, or only `--https_server_port 0`.
- PID-based `lsof` discovery may fail, hang, or return no ports.
- Frontend refresh should keep existing visible state behavior.

Boundaries:
- Rust service owns process and port discovery.
- React components own local in-flight guards.
- No vendor-specific SDK is introduced.

Error handling:
- A timed-out `lsof` returns `PORT_SCAN_TIMEOUT`.
- Invalid or zero explicit ports are ignored.
- Existing fallback response shape remains unchanged.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Bound Antigravity lsof | Rust tests prove timed-out subprocesses are killed and parser ignores invalid ports | 验证成功 |
| T2 | Prefer explicit ports before lsof | Rust tests prove explicit process ports bypass fallback parsing risk | 验证成功 |
| T3 | Prevent frontend refresh reentry | Vitest proves interval ticks do not call Antigravity API while prior call is pending | 验证成功 |
| T4 | Cache successful Antigravity ports | Rust tests prove cached ports are matched only to the same process identity | 验证成功 |
| T5 | Add backend single-flight fallback | Rust tests prove in-flight requests can return the last cached response without launching discovery | 验证成功 |
| T6 | Skip recently failed explicit ports | Rust tests prove failed explicit ports are suppressed during their cooldown and retried after expiry | 验证成功 |

### T1: Bound Antigravity lsof

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Unbounded `lsof` subprocesses are the failure mode that can accumulate and consume CPU.

**What to do**:
- Add a small command runner for child processes with a hard timeout.
- Use it from `find_listening_ports`.
- Keep stdout/stderr collection for successful or failed exits.

**Logic design**:
- Spawn child with piped stdout/stderr.
- Poll `try_wait()` until completion or deadline.
- On timeout, call `kill()` and `wait()`, then return a timeout error.
- Use short sleep intervals to avoid busy waiting.

**Test design**:
- Add a test that runs a long `sh -c sleep ...` command with a short timeout and expects `PORT_SCAN_TIMEOUT`.
- Add a test that runs a fast command and verifies stdout is captured.
- Expected initial failure mode: helper function does not exist or timeout behavior is absent.

**Acceptance**:
- `cd apps/desktop/src-tauri && cargo test antigravity_command` exits 0 after implementation.

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T2: Prefer explicit ports before lsof

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
If Antigravity exposes a usable port in process arguments, the service should not invoke `lsof`.

**What to do**:
- Extend process discovery to collect explicit port arguments.
- Make `get_live_antigravity_quota` try explicit ports before `lsof`.
- Ignore missing, zero, and invalid port values.

**Logic design**:
- Keep process parsing independent from command execution.
- Use a focused parser for `--https_server_port`, `--http_server_port`, and `--extension_server_port`.
- Preserve standalone-first process sorting.

**Test design**:
- Add parser tests for space-separated and equals-separated port args.
- Add parser tests for zero and invalid ports.
- Expected initial failure mode: parser does not exist or process struct has no ports.

**Acceptance**:
- `cd apps/desktop/src-tauri && cargo test antigravity_explicit_port` exits 0 after implementation.

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T3: Prevent frontend refresh reentry

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Even with backend timeouts, overlapping UI refreshes create avoidable child processes and network calls.

**What to do**:
- Add `useRef` in-flight guards to main app and tray Antigravity loaders.
- Skip new loads while the current load is pending.

**Logic design**:
- Guard before setting refresh state.
- Reset guard in `finally`.
- Preserve existing error logging and state updates.

**Test design**:
- Add App test with fake timers and unresolved `getAntigravityQuota`; interval tick should not call API again.
- Add tray test with fake timers and unresolved `getAntigravityQuota`; interval tick should not call API again.
- Expected initial failure mode: API call count increases on interval tick while first promise is pending.

**Acceptance**:
- `cd apps/desktop && npm test -- --run App.handoff.test.tsx tray-quota-panel.test.tsx` exits 0 after implementation.

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T4: Cache successful Antigravity ports

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Without a successful port cache, standalone Antigravity with `--https_server_port 0` can run bounded `lsof` on every refresh.

**What to do**:
- Store the successful `{ pid, csrfToken, port, response }`.
- Before explicit-port and `lsof` discovery, try the cached port when the process identity still matches.
- Refresh the cached response after a successful cached-port query.

**Logic design**:
- Cache matching must require both PID and CSRF token so a reused PID or restarted language server does not reuse a stale port.
- Cache should be process-local and guarded by a mutex.
- A failed cached-port query should clear the cache and continue to normal discovery.

**Test design**:
- Add tests for cache identity matching.
- Add tests that mismatched PID or token rejects cache use.
- Expected initial failure mode: cache helper does not exist.

**Acceptance**:
- `cd apps/desktop/src-tauri && cargo test antigravity_cache` exits 0 after implementation.

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T5: Add backend single-flight fallback

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Component-level guards do not prevent main window and tray from invoking the backend at the same time.

**What to do**:
- Add a backend refresh lock.
- If a refresh is already in progress, return the last cached response when available.
- If no cached response exists, return a non-fatal `ok:false` response explaining refresh is in progress.

**Logic design**:
- Use `try_lock` so the second caller does not queue another scan.
- Keep the response shape compatible with existing frontend code.
- Do not hold unrelated cache locks while performing process discovery.

**Test design**:
- Add tests for the in-flight fallback response with and without cached response.
- Expected initial failure mode: fallback helper does not exist.

**Acceptance**:
- `cd apps/desktop/src-tauri && cargo test antigravity_single_flight` exits 0 after implementation.

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T6: Skip recently failed explicit ports

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Wrong explicit ports can add repeated curl delays before the service falls back to `lsof`.

**What to do**:
- Track failed explicit ports for a short cooldown.
- Filter explicit ports through that cooldown before trying them.
- Let expired failures be retried.

**Logic design**:
- Store failures by PID, CSRF token, and port.
- Keep cooldown local to process memory.
- Only apply cooldown to explicit ports; `lsof` discovered ports remain authoritative for the current scan.

**Test design**:
- Add tests for active cooldown filtering.
- Add tests for expired cooldown retry.
- Expected initial failure mode: cooldown helper does not exist.

**Acceptance**:
- `cd apps/desktop/src-tauri && cargo test antigravity_negative_port` exits 0 after implementation.

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Test plan

- Normal behavior: fast subprocess output is captured; explicit valid port is parsed.
- Edge cases: zero and invalid explicit ports are ignored.
- Error behavior: timed-out subprocess returns `PORT_SCAN_TIMEOUT`.
- State/conflict behavior: React interval refresh does not reenter while a request is pending.
- Cache behavior: successful ports are reused only for matching process identity.
- Backend conflict behavior: concurrent backend calls return cached/in-progress responses instead of launching duplicate discovery.
- Cooldown behavior: recently failed explicit ports are skipped until expiry.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Rust timeout tests | `cd apps/desktop/src-tauri && cargo test antigravity_command` | exit 0 after implementation |
| Rust parser tests | `cd apps/desktop/src-tauri && cargo test antigravity_explicit_port` | exit 0 after implementation |
| Frontend focused tests | `cd apps/desktop && npm test -- --run App.handoff.test.tsx tray-quota-panel.test.tsx` | exit 0 after implementation |
| Rust relevant suite | `cd apps/desktop/src-tauri && cargo test antigravity` | exit 0 |
| Rust cache tests | `cd apps/desktop/src-tauri && cargo test antigravity_cache` | exit 0 |
| Rust single-flight tests | `cd apps/desktop/src-tauri && cargo test antigravity_single_flight` | exit 0 |
| Rust negative-port tests | `cd apps/desktop/src-tauri && cargo test antigravity_negative_port` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:

- The current code differs materially from the Current state description.
- Required test infrastructure is missing and cannot be added safely.
- A dependency or external service cannot be isolated by mock/stub.
- Verification still fails after the configured fix loop.
- Completing the task would require out-of-scope changes.
