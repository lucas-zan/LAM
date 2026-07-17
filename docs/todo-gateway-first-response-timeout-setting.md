# Todo: Gateway 首响应超时 Settings 配置

> Executor instructions: Follow TDD per task. Keep this change independent from
> Supervisor and concurrency scheduling. The setting applies to newly launched Gateway processes.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: none
- **Category**: bugfix/feature
- **Planned at**: current workspace, 2026-07-16

## Why this matters

**Background**: Gateway hardcodes upstream response-header timeout to 10 seconds. Real request logs show 502 responses at 10.027～10.031 seconds while the same Gateway remains healthy and nearby requests succeed.

**Current state**: `lam-provider-gateway` uses `first_byte_timeout=10s`; Provider `streamIdleTimeoutMs` does not control this stage; Settings has no Gateway timeout field.

**Impact**: Slow but healthy LLM upstreams are incorrectly terminated with `UPSTREAM_FIRST_BYTE_TIMEOUT`.

**What improves**: Default becomes 60 seconds; users can configure 10～600 seconds in Settings; all Gateway launch paths pass one validated value.

## Scope

**In scope**:

- Persist `gatewayFirstResponseTimeoutSeconds` in existing settings.json.
- Default 60, inclusive range 10～600.
- Tauri get/set commands and TypeScript API/store/UI.
- Launcher and packaged supervisor pass a controlled environment variable.
- Gateway validates the environment value and uses it for `first_byte_timeout`.

**Out of scope**:

- Running Gateway live reload or forced restart.
- connect/stream-idle/total timeout changes.
- retries, Supervisor identity state machine, Admission Controller.

## Design

- Backend is the source of truth; invalid persisted values fall back to 60 on read.
- Setter rejects out-of-range values and preserves unrelated settings keys.
- Environment name: `LAM_GATEWAY_FIRST_RESPONSE_TIMEOUT_SECS`.
- Gateway missing env defaults to 60; malformed/out-of-range env fails closed with `GATEWAY_TIMEOUT_CONFIG_INVALID`.
- Both `lam` launcher and desktop supervisor must set the env after `env_clear()`.
- UI uses numeric seconds and explains that changes apply to the next Gateway launch.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|---|---|---|---|
| T1 | Settings persistence and validation | Default 60, range enforced, merge preserved | 待执行 |
| T2 | Gateway launch/runtime wiring | All launch paths pass value; Gateway uses it | 待执行 |
| T3 | Settings UI | User can load/save seconds with validation | 待执行 |
| T4 | Full verification | Frontend/Rust suites and build pass | 待执行 |

### T1: Settings persistence and validation

**Status**: [x] 待执行 [ ] 待测试验证 [ ] 验证成功 [ ] 验证失败

**Test design**: default 60; valid value roundtrip; 9/601 rejected; malformed persisted value falls back; unrelated settings preserved.

**Expected initial failure**: functions/constants do not exist.

**Acceptance**: focused Rust settings tests pass.

### T2: Gateway launch/runtime wiring

**Status**: [x] 待执行 [ ] 待测试验证 [ ] 验证成功 [ ] 验证失败

**Test design**: missing env=60; valid env accepted; malformed/out-of-range rejected; production wiring tests assert both launch paths set env and hardcoded 10 is removed.

**Expected initial failure**: Gateway remains hardcoded to 10 seconds.

**Acceptance**: Gateway runtime and production wiring tests pass.

### T3: Settings UI

**Status**: [x] 待执行 [ ] 待测试验证 [ ] 验证成功 [ ] 验证失败

**Test design**: load displays 60; user changes to 120 and API receives 120; invalid input is not persisted; help text says next Gateway launch.

**Expected initial failure**: state/API/UI fields do not exist.

**Acceptance**: App store and Settings UI tests pass; build passes.

### T4: Full verification

**Status**: [x] 待执行 [ ] 待测试验证 [ ] 验证成功 [ ] 验证失败

**Acceptance**: full frontend tests/build and full Rust serial tests pass.

## Verification commands

| Purpose | Command | Expected |
|---|---|---|
| Rust focused | `cd apps/desktop/src-tauri && cargo test gateway_first_response_timeout` | exit 0 |
| Wiring | `cd apps/desktop/src-tauri && cargo test --test provider_sidecar_production` | exit 0 |
| Frontend focused | `cd apps/desktop && npm test -- --run src/stores/app.test.ts src/App.handoff.test.tsx` | exit 0 |
| Frontend full/build | `cd apps/desktop && npm test -- --run && npm run build` | exit 0 |
| Rust full | `cd apps/desktop/src-tauri && cargo test --tests -- --test-threads=1` | exit 0 |

## Done criteria

- [ ] All tasks are `验证成功`
- [ ] Default is 60 seconds
- [ ] User value persists and reaches new Gateway processes
- [ ] No unrelated timeout or scheduling behavior changed

## STOP conditions

- Applying the setting requires unsafe termination of a running Gateway.
- Existing stable external command contract cannot be migrated atomically.
- Full verification reveals an unrelated failure requiring scope expansion.
