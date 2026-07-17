# Todo: Provider Stage 1 completion

> Executor instructions: Follow this todo step by step. Generate tests from the
> "Test design" section before implementation. Run each verification command and
> confirm the expected result before moving to the next task. If a STOP condition
> occurs, stop and report instead of improvising.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: existing `ProviderListViewV2` revision snapshot implementation
- **Category**: bugfix/contract/refactor
- **Planned at**: current working tree on 2026-07-16

## Why this matters

**Background**: Stage 1 already fixes the core empty-provider-list revision bug, but review of the real implementation found three remaining contract gaps: Keychain create/credential rotation do not recover from CAS conflicts like normal Provider writes; the TypeScript boundary trusts the new object response without runtime validation; and the Rust helper marked as an I/O-free view builder still performs credential/config I/O and accepts Provider value/revision separately.

**Current state**: `saveProvider` refreshes and clears plans after `STORE_REVISION_CONFLICT`, while `createKeychainProvider` and `rotateKeychainCredential` only surface the error. `listProvidersV2` uses a compile-time generic without validating the runtime payload. `build_provider_views` calls readiness logic that reads config files and credentials.

**Impact**: Conflict recovery differs by write type, mixed frontend/backend contracts can corrupt in-memory state instead of failing explicitly, and the Stage 1 implementation does not fully match its documented snapshot/purity contract.

**What improves**: All Provider writes use one safe conflict recovery path without replay, old response shapes fail explicitly, and Provider DTO construction becomes pure and tied to one versioned Provider snapshot while readiness I/O is isolated before construction.

## Scope

**In scope**:
- `apps/desktop/src/stores/providers.ts` and its tests.
- `apps/desktop/src/lib/api.ts` and its tests.
- `apps/desktop/src-tauri/src/services/provider_api_v2.rs` and Provider API tests.
- Stage 1 status/acceptance documentation.

**Out of scope**:
- Gateway timeout settings and related files.
- Attach/Detach countdown work.
- Gateway semaphore ordering, Admission Controller, Supervisor, or file-domain split.
- Changes to public Provider write semantics other than conflict recovery and explicit contract failure.

## Design

- Introduce one Provider-store conflict recovery helper used by normal create/update, Keychain create, and credential rotation. It clears stale attach/detach plans, records a re-preview message, refreshes the combined Provider/Binding snapshot, and always rethrows the original write error. It never retries the write.
- Validate `list_providers_v2` at the TypeScript invoke boundary. Accept only a non-negative safe integer revision and an array-valued `providers` field. Reject arrays and malformed objects with a stable `PROVIDER_LIST_CONTRACT_MISMATCH` error.
- In Rust, collect readiness outcomes (credential/config/health checks) in an explicit I/O phase. Make `build_provider_views` consume a `StoreSnapshot<ProviderCollection>` plus precomputed readiness data and perform no I/O. Passing the complete snapshot prevents value/revision mismatch.
- Preserve existing final-consistency behavior between Provider, Binding, health, credentials, and projected config state.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Unify Provider CAS conflict recovery | Every Provider write refreshes/clears plans without replay | 验证成功 |
| T2 | Enforce Provider list runtime contract | Legacy arrays and malformed snapshots fail explicitly | 验证成功 |
| T3 | Isolate readiness I/O and use typed Provider snapshot | Pure DTO builder consumes one versioned Provider snapshot | 验证成功 |
| T4 | Run Stage 1 regression gate and update status | Focused suites/build pass and main Stage 1 status is accurate | 验证成功 |

### T1: Unify Provider CAS conflict recovery

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Keychain create and credential rotation currently leave the store on a stale revision after a conflict, unlike normal create/update.

**What to do**:
- Add a shared conflict recovery helper in `stores/providers.ts`.
- Use it from `saveProvider`, `createKeychainProvider`, and `rotateKeychainCredential`.
- Preserve the original error and never replay a stale write.

**Logic design**:
- Recognize existing conflict codes through `structuredError`/`conflictCodes`.
- On conflict, clear both plans, set the recovery message, attempt one refresh, ignore refresh failure only so the original conflict remains the surfaced error.
- Non-conflict errors do not trigger refresh.

**Test design**:
- Keychain create conflict refreshes once, clears plans, updates revision, and invokes the write once.
- Credential rotation conflict does the same and is not replayed.
- Non-conflict Keychain failure does not refresh.
- Expected initial failure: current implementation leaves revision/plans unchanged and performs no refresh.

**Acceptance**:
- `npm test -- --run src/stores/providers-v2.test.ts`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T2: Enforce Provider list runtime contract

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
TypeScript generics do not validate the Tauri payload. An old array response can currently produce undefined store fields instead of a clear protocol failure.

**What to do**:
- Validate the result returned by `list_providers_v2` in `lib/api.ts`.
- Return a stable structured-like error code for incompatible response shapes.
- Do not support array/object dual-shape compatibility.

**Logic design**:
- Invoke as `unknown`.
- Require a non-array object, a non-negative safe integer `revision`, and an array `providers` field.
- Throw an `Error` carrying `code = PROVIDER_LIST_CONTRACT_MISMATCH` when invalid.

**Test design**:
- Valid empty snapshot is accepted.
- Legacy array response is rejected with the stable code.
- Missing/invalid revision and non-array providers are rejected.
- Expected initial failure: legacy arrays resolve successfully because the generic is unchecked.

**Acceptance**:
- `npm test -- --run src/lib/provider-api-v2.test.ts`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T3: Isolate readiness I/O and use typed Provider snapshot

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Stage 1 documents an I/O-free view builder and same-snapshot revision contract, but the current helper performs readiness I/O and takes value/revision as separate parameters.

**What to do**:
- Add an explicit precomputed readiness outcome type.
- Move credential/config/health checks into a collection phase.
- Make `build_provider_views` consume `&StoreSnapshot<ProviderCollection>` and precomputed outcomes only.
- Preserve serialized output and final-consistency behavior.

**Logic design**:
- The I/O phase reads Provider, Binding, and health snapshots already loaded by the service, then performs existing credential/config checks and produces blockers, binding count, and last-health data keyed by Provider ID.
- The pure phase constructs `ProviderProfileView` using `providers.value` and `providers.revision` from the same object and applies only precomputed values.
- Internal single-provider view behavior remains unchanged.

**Test design**:
- Add/refine a service test proving list revision and each view's `storeRevision` stay tied to one snapshot across empty/populated/deleted states.
- Preserve readiness blocker coverage through existing Provider tests.
- Expected initial failure: a compile-time-oriented source assertion/test will require the builder to accept `StoreSnapshot` and avoid direct readiness resolver/file reads; current helper does not.

**Acceptance**:
- `cargo test --test provider_api_v2 --test provider_phase4_api_account -- --test-threads=1`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T4: Run Stage 1 regression gate and update status

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Stage 1 should only be marked complete when all documented conflict, contract, snapshot, and regression behavior is verified together.

**What to do**:
- Run relevant frontend tests, production build, and Rust Provider suites.
- Update `docs/todo-provider-gateway-review-remediation.md` and this todo with accurate final status.

**Logic design**:
- Do not modify Gateway timeout or later-stage code.
- Record any unrelated existing failure instead of broadening scope.

**Test design**:
- Run the combined regression commands below.
- No new implementation-specific failure is expected after T1-T3 pass.

**Acceptance**:
- Frontend Provider tests pass.
- Frontend production build passes.
- Rust Provider API and API Account tests pass serially.

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Test plan

- Provider normal create/update, Keychain create, and credential rotation use explicit collection revision.
- All Provider write conflicts refresh and clear stale plans without replay.
- Non-conflict write failures do not refresh.
- Valid `{ revision, providers }` payloads pass; legacy arrays and malformed objects fail explicitly.
- Empty/non-empty/deleted Provider snapshots retain the exact collection revision in both list and views.
- Readiness behavior remains compatible after isolating I/O.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| T1 store tests | `cd apps/desktop && npm test -- --run src/stores/providers-v2.test.ts` | exit 0 after implementation; new tests fail before implementation |
| T2 API tests | `cd apps/desktop && npm test -- --run src/lib/provider-api-v2.test.ts` | exit 0 after implementation; new tests fail before implementation |
| T3 Rust tests | `cd apps/desktop/src-tauri && cargo test --test provider_api_v2 --test provider_phase4_api_account -- --test-threads=1` | exit 0 |
| Frontend regression | `cd apps/desktop && npm test -- --run src/stores/providers-v2.test.ts src/lib/provider-api-v2.test.ts src/components/provider-center-flow.test.tsx` | exit 0 |
| Frontend build | `cd apps/desktop && npm run build` | exit 0 |
| Rust regression | `cd apps/desktop/src-tauri && cargo test --test provider_api_v2 --test provider_phase4_api_account -- --test-threads=1` | exit 0 |

## Verification record

- T1 failure-first: new Keychain create and credential rotation conflict tests failed because neither path refreshed; after implementation, `providers-v2.test.ts` passed 18/18.
- T2 failure-first: legacy arrays and malformed payloads resolved successfully before validation; after implementation, `provider-api-v2.test.ts` passed 15/15.
- T3 failure-first: the source contract test failed because `build_provider_views` did not accept `StoreSnapshot`; after implementation, Provider API 17/17 and API Account 11/11 passed. Two socket-based tests required running outside the filesystem/network sandbox after the sandbox returned `Operation not permitted`.
- Frontend Stage 1 regression: 3 files / 36 tests passed.
- Frontend production build: `tsc && vite build` passed.
- Rust formatting: `cargo fmt -- --check` passed.
- Full frontend suite: 21 files passed and 1 unrelated Settings test failed because the current Gateway first-response-timeout control was not found. This test belongs to the user's separate timeout-setting work and no timeout files were modified in this Stage 1 task.

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No Stage 1 STOP condition remains unresolved

## STOP conditions

- Required behavior would need changes to Gateway timeout configuration or Stage 2+ code.
- Runtime payload validation would require accepting both old and new response shapes.
- Readiness refactor changes serialized Provider output or credential disclosure behavior.
- Focused verification still fails after five fix loops.
