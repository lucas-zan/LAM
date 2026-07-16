# Todo: External API 帐号跳过额度刷新并明确标识

> Executor instructions: Execute T1 then T2. For each task, write and run the tests before implementation, confirm the expected failure, implement only that task, verify it, and update its status immediately.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: profile-provider bindings, quota refresh store, account cards
- **Category**: bugfix
- **Planned at**: current working tree

## Why this matters

**Background**: External API accounts provide model access but do not expose ChatGPT subscription quota. The current shared refresh flow treats them like logged-in OpenAI accounts.

**Current state**: Bulk, scheduled, and manual quota refreshes include every profile. An External API profile therefore produces `realtime quota unavailable` warnings. Its card says `API Account`, and an active External API account can lose even that label because the active badge branch takes precedence.

**Impact**: Expected lack of quota looks like a runtime error, unnecessary refresh work is performed, and the account type is unclear.

**What improves**: Provider-bound External API profiles are excluded from quota refresh at both backend and frontend boundaries. Their cards always show `External API` and expose no quota refresh control.

## Scope

**In scope**:
- Treat profiles present in the Provider Hub binding collection as External API profiles for quota eligibility.
- Exclude those profiles from bulk, immediate, scheduled, and interval quota refresh targets.
- Hide per-card quota refresh UI and show a persistent `External API` badge.

**Out of scope**:
- Adding provider-specific billing or usage APIs.
- Changing OpenAI OAuth/PAT quota collection.
- Inferring quota support from model or provider names.

## Design

- Use profile-provider bindings as the authoritative External API classification; do not depend on naming conventions.
- Add one pure frontend selector that subtracts bound profile IDs from account IDs, and reuse it for all refresh scheduling.
- Keep the backend bulk refresh defensive by excluding bound profile IDs before calling quota collection.
- Render account activity/auth state and External API type as independent badges so active accounts remain clearly classified.
- Do not render quota refresh or quota windows for External API cards.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Skip External API quota refresh | Bound profiles never reach quota collection or warning generation | 验证成功 |
| T2 | Label External API cards | Badge is always visible and quota controls are absent | 验证成功 |
| T3 | Guard direct quota endpoint | Direct refresh returns unsupported without launching a collector | 验证成功 |

### T1: Skip External API quota refresh

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: External API bindings do not have a supported subscription quota source.

**What to do**:
- Add a pure frontend quota-target selector based on binding profile IDs.
- Use it for immediate, delayed, and interval refreshes.
- Filter Provider Hub bindings in Rust `refresh_all_quotas` as a backend safeguard.

**Logic design**:
- Inputs are the current accounts plus bound profile IDs.
- Output preserves account order and contains only non-bound profile IDs.
- Empty/missing bindings keep existing quota behavior.
- Explicit requests containing only External API IDs return no snapshots and no warnings.
- Binding read failures remain explicit rather than silently refreshing an account of unknown type.

**Test design**:
- Pure selector: mixed normal/external inputs return only normal IDs; empty and stale binding IDs are harmless.
- Account refresh: a manual refresh calls quota only for the normal account after bindings load.
- Rust bulk refresh: a persisted External API binding is omitted from snapshots and warnings.

**Acceptance**:
- `npm test -- --run src/lib/quota.test.ts src/stores/accounts.test.ts`
- `cargo test --test phase1_core refresh_all_quotas_skips_external_api_bindings`

**Done criteria**:
- [x] Tests listed above were written before implementation
- [x] Expected red failures were confirmed
- [x] Implementation follows the binding-based design
- [x] Focused frontend and backend tests pass
- [x] Relevant suite/build/lint passes
- [x] Overview row and task status are `验证成功`

### T2: Label External API cards

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: `API Account` is ambiguous and disappears when the same account is active.

**What to do**:
- Render `External API` independently of Active/Logged in state.
- Remove the per-account quota refresh button and quota/reset presentation for External API cards.

**Logic design**:
- `apiAccountIds` remains the view contract.
- External API classification affects only type labeling and quota controls, not model switching or handoff.
- Normal account cards retain current quota behavior.

**Test design**:
- An active External API card shows both `Active` and `External API`.
- The External API card has no quota refresh button and clicking other actions does not invoke quota refresh.
- A normal account still exposes its refresh button.

**Acceptance**:
- `npm test -- --run src/routes/handoff.test.tsx`
- Visual inspection of the overview card. If the Browser runtime cannot initialize, record the runtime error and use component DOM rendering plus `npm run test:ui` as the fallback.

**Done criteria**:
- [x] Tests listed above were written before implementation
- [x] Expected red failures were confirmed
- [x] Implementation follows the independent badge design
- [x] Focused UI test passes
- [x] Relevant suite/build/lint passes
- [x] Overview row and task status are `验证成功`

### T3: Guard direct quota endpoint

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: The tray can call `get_profile_quota` directly even though bulk and main-card refresh targets are filtered.

**What to do**:
- Detect an External API binding before starting the app-server quota collector.
- Return a stable unsupported snapshot without alerts or network/process work.

**Logic design**:
- Reuse the same binding lookup as bulk refresh.
- Validate that the account exists first, then return `external_api_quota_unsupported` for a forced External API refresh.
- Preserve existing behavior for normal, unknown, and non-forced quota requests.

**Test design**:
- Configure a fake Codex collector that writes an invocation marker.
- Force-refresh a bound External API profile and assert the marker is absent.
- Assert the result has the unsupported source and no alerts.

**Acceptance**:
- `cargo test --test phase1_core direct_quota_refresh_skips_external_api_collector`

**Done criteria**:
- [x] Test was written before implementation
- [x] Expected red failure was confirmed
- [x] Implementation reuses binding classification
- [x] Focused and full Rust tests pass
- [x] Rust formatting passes
- [x] Overview row and task status are `验证成功`

## Test plan

- Mixed and empty quota eligibility selection.
- Backend binding-based exclusion with no warning.
- Direct endpoint collector bypass for External API profiles.
- Manual and automatic refresh target filtering.
- Active External API and normal account card comparison.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Frontend focused | `npm test -- --run src/lib/quota.test.ts src/stores/accounts.test.ts src/routes/handoff.test.tsx` | exit 0 after expected red failures |
| Backend focused | `cargo test --test phase1_core refresh_all_quotas_skips_external_api_bindings direct_quota_refresh_skips_external_api_collector` | exit 0 after expected red failures |
| Frontend quality | `npm run build && npm run lint` | exit 0 |
| Rust quality | `cargo fmt --all -- --check` | exit 0 |

## Done criteria

- [x] Every task's Done criteria is checked
- [x] Every task has exactly one checked status and it is `验证成功`
- [x] Task overview shows all tasks as `验证成功`
- [x] No STOP condition remains unresolved

## Visual verification note

- The Browser runtime was attempted twice but could not initialize because its process bridge reported `Cannot redefine property: process`.
- Fallback verification passed through the rendered DOM component test and `npm run test:ui`; the assertions cover simultaneous `Active` + `External API`, absence of the External API quota refresh button, and preservation of the normal-account refresh button.

## STOP conditions

- External API accounts cannot be identified from profile-provider bindings.
- Quota filtering would require provider-name heuristics.
- Existing test infrastructure cannot isolate quota refresh calls.
- Five repair cycles fail for either task.
