# Todo: 修复 Codex 退出后 Gateway 身份不匹配

> Executor instructions: Execute T1, T2, then T3. Write and fail each task's tests before implementation. Do not deliver until every task is 验证成功 and the real parent/child lifecycle passes.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: HIGH
- **Depends on**: macOS Keychain install identity, verified Gateway installation, runtime state PID
- **Category**: security bugfix
- **Planned at**: current working tree

## Why this matters

**Background**: Codex clients intentionally exit while the shared Gateway sidecar can remain alive. A later `codex-idragon3` launch must verify and reuse that sidecar.

**Current state**: `load_or_create_install_identity` treats every Keychain read error as a missing item and can overwrite the identity with random bytes. If verification then fails while the state PID is alive, `ensure_ready` returns `GATEWAY_IDENTITY_MISMATCH` without a bounded recovery path.

**Impact**: A normal exit/re-entry can permanently block the External API account until the sidecar is manually killed. A transient Keychain authorization error can also rotate a security identity unexpectedly.

**What improves**: Keychain failures never rotate identity, a stale sidecar can be terminated only after its UID and executable path are verified, and the Gateway lifetime is owned by the launcher that started it.

## Scope

**In scope**:
- Abstract install identity persistence behind a testable contract.
- Create identity only for an explicit Keychain item-not-found result.
- Add verified, bounded sidecar process termination and launcher recovery.
- Keep the owned Gateway alive while Codex runs and stop it when the launcher exits.
- Rebuild/re-sign components and verify the real parent/child lifecycle.

**Out of scope**:
- Image input support.
- Rotating or deleting existing provider credentials.
- Terminating an unverified PID.

## Design

- Core identity logic depends on an `InstallIdentityStore` contract returning Found, Missing, or Error; it never interprets a generic error as Missing.
- The macOS adapter maps only `errSecItemNotFound` to Missing; authorization, interaction, lock, and other failures become `KEYCHAIN_UNAVAILABLE`.
- Both launcher and packaged supervisor reuse the same identity loader.
- Recovery inspects the state PID through an abstract process-control boundary. It requires the effective UID and canonical executable path to match the verified Gateway component before sending SIGTERM.
- Termination waits for exit within a bounded deadline. Identity/path/UID mismatch remains a hard error.
- A new launcher does not adopt a stale Gateway from a previous launcher. After verified termination, it starts and retains ownership of a sidecar with the current persisted identity.
- Launcher completion always invokes the readiness shutdown contract, including non-zero Codex exits and launch failures after readiness.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Preserve install identity | Non-missing Keychain errors never write or rotate identity | 验证成功 |
| T2 | Recover verified sidecar | Re-entry replaces only a verified mismatched Gateway process | 验证成功 |
| T3 | Own Gateway lifetime | Gateway lives with one launcher and exits with that launcher | 验证成功 |

### T1: Preserve install identity

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: A transient Keychain read failure must not become a destructive identity rotation.

**What to do**:
- Add a focused identity module and a macOS Keychain adapter.
- Replace duplicated launcher/supervisor identity reads with the module.

**Logic design**:
- Found 32-byte identity returns unchanged.
- Missing generates and persists exactly once.
- Read/write errors propagate with no write after a read error.
- Invalid identity length is rejected without mutation.

**Test design**:
- Fake store Found, Missing, read-error, write-error, and invalid-length cases.
- Assert write counts and stable error codes.

**Acceptance**:
- `cargo test --test provider_gateway_identity install_identity`

**Done criteria**:
- [x] Tests were written before implementation
- [x] Expected red failure was confirmed
- [x] Store contract and macOS adapter follow the design
- [x] Focused and relevant tests pass
- [x] Formatting/build checks pass
- [x] Task status and overview are `验证成功`

### T2: Recover verified sidecar

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: A live PID currently turns any readiness failure into an unrecoverable launcher error.

**What to do**:
- Add a process identity/control boundary and verified termination policy.
- Use it in `PackagedReadiness::ensure_ready` before bounded restart.

**Logic design**:
- Missing/dead PID proceeds to normal start.
- Matching UID and canonical Gateway path may be terminated once.
- UID/path mismatch, inspection failure, and timeout never proceed to restart.
- No arbitrary process is signaled.

**Test design**:
- Fake process control covers verified termination, wrong UID, wrong path, missing PID, termination error, and timeout.
- Production wiring test asserts launcher calls verified recovery rather than returning immediately on a live PID.

**Acceptance**:
- `cargo test --test provider_gateway_recovery verified_gateway`
- `cargo test --test provider_sidecar_production production_binaries_wire_bounded_restart_idle_and_dual_stream_budgets`

**Done criteria**:
- [x] Tests were written before implementation
- [x] Expected red failure was confirmed
- [x] Recovery signals only verified processes
- [x] Focused and relevant tests pass
- [x] A stale verified Gateway is replaced and launch succeeds
- [x] Task status and overview are `验证成功`

### T3: Own Gateway lifetime

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Dropping the `Child` handle detaches the Gateway, so it can outlive `lam` and force the next launcher into recovery.

**What to do**:
- Extend the readiness contract with explicit shutdown.
- Retain the successfully started Gateway child in `PackagedReadiness`.
- Stop and reap that exact child when Codex/launcher completes.

**Logic design**:
- One launcher owns at most one Gateway child.
- The child remains alive for the full Codex invocation.
- Normal, non-zero, and post-readiness launch failure paths invoke shutdown.
- A later launcher starts a new Gateway rather than adopting an orphan.

**Test design**:
- Launcher contract tests assert shutdown after normal and non-zero Codex completion and after spawn failure.
- Production wiring test asserts a successful child is retained and shutdown is implemented.
- Real test observes one stable PID while Codex is running, no PID after launcher exit, and a different PID on the next launch.

**Acceptance**:
- `cargo test --test provider_gateway_launcher`
- `cargo test --test provider_sidecar_production`

**Done criteria**:
- [x] Tests were written before implementation
- [x] Expected red failure was confirmed
- [x] Launcher invokes shutdown on every post-readiness exit path
- [x] Packaged readiness retains, stops, and reaps its child
- [x] Focused and relevant tests pass
- [x] Real parent/child PID lifecycle passes
- [x] Task status and overview are `验证成功`

## Test plan

- Install identity storage normal, missing, invalid, and error behavior.
- Process identity match/mismatch and bounded termination behavior.
- Existing launcher, sidecar, packaging, and Gateway suites.
- Real launch, stable child PID while Codex runs, child exit with launcher, different child PID on next launch.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Identity focused | `cargo test --test provider_gateway_identity` | exit 0 after expected red failure |
| Recovery focused | `cargo test --test provider_gateway_recovery --test provider_sidecar_production` | exit 0 after expected red failure |
| Relevant regression | `cargo test --test provider_gateway_launcher --test provider_gateway_sidecar --test provider_sidecar_production` | exit 0 |
| Quality | `cargo fmt --all -- --check && cargo check --release --bins` | exit 0 |
| Real lifecycle | Keep one `codex-idragon3` invocation open, observe one stable Gateway PID, exit it, then launch again | first PID exits; second launch uses a new PID |

## Done criteria

- [x] Every task Done criteria is checked
- [x] Every task has exactly one checked status and it is `验证成功`
- [x] Overview shows all tasks as `验证成功`
- [x] No STOP condition remains

## STOP conditions

- Recovery cannot verify both UID and executable path.
- Fix would delete or rotate an existing Keychain identity.
- Fix would kill an unverified process.
- Five repair cycles fail for a task.
