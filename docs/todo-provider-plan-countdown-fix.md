# Todo: Provider Attach/Detach 计划倒计时修复

> Executor instructions: Write the fake-timer expiry test first, confirm it fails
> because the dialog clock is frozen, then implement only the internal ticking clock.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: Provider revision P0 completed
- **Category**: bugfix
- **Planned at**: current workspace, 2026-07-16

## Why this matters

**Background**: `ProviderBindingDialog` stores `Date.now()` once as `openedAt`. Without an injected `nowMs`, the countdown never rerenders or expires.

**Current state**: UI can continue to show an expired Attach/Detach plan as executable until the backend rejects it.

**Impact**: No data corruption, but misleading countdown and avoidable failed execution UX.

**What improves**: Countdown updates once per second, reaches zero, displays expiry, and disables Execute. Tests retain deterministic `nowMs` injection.

## Scope

**In scope**: `provider-binding-dialog.tsx` and its focused tests.

**Out of scope**: backend expiry rules, plan TTL, Provider Center state, Gateway concurrency.

## Design

- Use internal `tickNow` state initialized from `Date.now()`.
- If `nowMs` is provided, use it directly and create no interval.
- If no plan exists, create no interval.
- If plan exists and is not expired, tick once per second.
- On expiry, stop scheduling further ticks.
- Always clear the interval on dependency change/unmount.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|---|---|---|---|
| T1 | Implement live plan clock | Fake time advance expires plan and cleans timer | 验证成功 |

### T1: Implement live plan clock

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Frozen `openedAt` never causes a rerender.

**What to do**: Add `useEffect`/`tickNow`, preserve `nowMs`, stop and clean interval.

**Logic design**: Derive `effectiveNow = nowMs ?? tickNow`; interval exists only for a live plan using internal time.

**Test design**:

- Render without `nowMs`, advance fake time beyond `expiresAtMs`, assert countdown zero, expiry warning, and disabled Execute.
- Verify injected `nowMs` behavior remains deterministic.
- Unmount and assert interval cleanup.

**Expected initial failure**: advancing fake timers does not rerender; Execute remains enabled and expiry warning is absent.

**Acceptance**:

- `npm test -- --run src/components/provider-binding-dialog.test.tsx`
- `npm run build`

**Done criteria**:

- [x] Test was written before implementation and failed for frozen time
- [x] Internal interval is bounded and cleaned up
- [x] Focused tests pass
- [x] Frontend build passes
- [x] Status is `验证成功`

## Test plan

- Attach plan live → expires automatically.
- Detach shares the same clock derivation.
- Injected `nowMs` remains stable.
- No plan/unmount leaves no interval.

## Verification commands

| Purpose | Command | Expected |
|---|---|---|
| Focused | `cd apps/desktop && npm test -- --run src/components/provider-binding-dialog.test.tsx` | exit 0 |
| Build | `cd apps/desktop && npm run build` | exit 0 |

## Done criteria

- [x] T1 is `验证成功`
- [x] No STOP condition remains

## STOP conditions

- Component lifecycle requires parent-owned time for a reason not represented in current tests.
- Fix requires changing backend plan TTL or execution contract.
