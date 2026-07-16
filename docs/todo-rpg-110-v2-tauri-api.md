# Todo: RPG-110 V2 Tauri DTOs and Compatibility Commands

> Executor instructions: Define and golden-test DTOs before command wiring. Keep
> commands thin and blocking work off the UI thread. Do not change RPG-111 UI.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: HIGH
- **Depends on**: RPG-102–RPG-109
- **Category**: feature/API compatibility

## Why this matters

**Background**: Phase 1 domain/store/planner/transaction/credential modules are
implemented, while Tauri still exposes only the legacy flat Provider commands.

**Current state**: TypeScript imports legacy `ProviderProfile` shapes and calls
unversioned commands. V2 domain structs are public Rust types but are not a
stable frontend contract, and plan execution has no Tauri state/cache boundary.

**Impact**: UI work cannot safely consume revisions, blockers, fingerprints or
redacted credential references; frontend/backend casing can drift again.

**What improves**: Explicit camelCase DTOs isolate persistence/domain evolution,
legacy `envKey` is accepted only at compatibility conversion, and thin commands
expose revision-aware create/list/plan/execute/detach flows.

## Scope

**In scope**:

- Closed V2 request/view/error DTOs and domain conversions.
- Legacy create compatibility conversion with warning and `envKey` alias.
- Provider V2 list/create/update, attach dry-run/execute, rebind/detach commands.
- Process-local dry-run cache and versioned Provider Hub path construction.
- TypeScript DTOs and wrappers matching Rust golden fixtures.

**Out of scope**:

- Provider UI components/modal/card changes (RPG-111).
- Chat Completions attach presentation before Phase 3.
- Removal of legacy commands/types.

## Design

DTO enums are independent closed API enums; conversion into domain types is
explicit. Views include reference metadata/readiness/revisions but never secret
values, helper output, previous sensitive config, token hashes, or private paths
not required for the operation. Structured errors preserve stable code,
recoverability and recovery actions.

Commands use a single path factory and `spawn_blocking`. Attach planning resolves
the profile config path, builds the shared planner plan, issues a process-local
ticket, and returns a redacted view. Execute carries ticket/fingerprint and
expected versions; the coordinator revalidates everything under the installation
lock. Legacy commands remain registered unchanged during migration.

## Tasks

| ID  | Task                           | Acceptance                                                      | Status   |
| --- | ------------------------------ | --------------------------------------------------------------- | -------- |
| T1  | DTO/golden compatibility       | Rust camelCase goldens, unknown values rejected, no leak        | 验证成功 |
| T2  | Thin Tauri commands            | Registered V2 create/list/credential-rotate/plan/execute/detach | 验证成功 |
| T3  | TypeScript wrappers/regression | TS contract matches Rust and all gates pass                     | 验证成功 |

### T1: DTO and compatibility contract

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Persistence/domain types must not become accidental frontend ABI.

**What to do**: Add independent V2 DTOs, conversions and legacy request adapter.

**Logic design**: Closed tagged enums reject unknown values. Views recursively
redact credential values and expose stable camelCase. Legacy `wireApi=openai`
maps to Responses with a warning; `secret.envKey` is accepted, `env_key` is not
emitted by any V2 view.

**Test design**: Golden request/view/plan/error JSON; legacy envKey/openai;
unknown protocol/adapter/auth; synthetic marker scan of every view.

**Acceptance**: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_api_v2 dto_`

**Done criteria**:

- [x] Tests first and confirmed red (missing V2 API module).
- [x] Goldens and leak checks pass (4/4).
- [x] Status is `验证成功`.

### T2: Thin commands and state assembly

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: The shared plans/transactions must be reachable without duplicating
defaults in UI or command handlers.

**What to do**: Add state/path factory, service functions, Tauri handlers and
registration for V2 operations; keep blocking work off async UI threads.

**Logic design**: Service functions are testable without Tauri. Commands only
resolve home, clone request, and call them through `run_blocking`. Stable
conflicts retain domain error codes plus recovery actions.

**Test design**: Temporary-home create/list/update; plan view/ticket; stale
execute; attach/detach; legacy coexistence; command registration source audit.

**Acceptance**: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_api_v2 service_`

**Done criteria**:

- [x] Tests first and confirmed red (service/state APIs, then missing Keychain rotation API).
- [x] Service/registration tests, including Keychain rotation command, pass (5/5 service/registration; 9/9 total).
- [x] Status is `验证成功`.

### T3: TypeScript wrappers and regression

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Rust and TypeScript must share one explicit contract before RPG-111.

**What to do**: Add V2 TS types/wrappers and contract tests without modifying UI.

**Logic design**: Wrappers pass request objects unchanged and return only V2
views. Existing legacy wrappers remain available.

**Test design**: Mock invoke names/payloads and fixture equality; full gates.

**Acceptance**: frontend tests/lint/build, full Rust, Phase 0, formats.

**Done criteria**:

- [x] Tests first and confirmed red (missing TypeScript wrappers).
- [x] TS focused 5/5 and full regression pass.
- [x] Status/master evidence are `验证成功`.

## Verification commands

Use focused Rust `provider_api_v2`, focused frontend contract/API tests, then the
full Rust/frontend/Phase 0/lint/build/rustfmt/Prettier/diff gates.

## Done criteria

- [x] T1–T3 are all `验证成功` with one checked status each.
- [x] No secret marker or persistence shape reaches a V2 view.
- [x] No STOP condition remains.

## Validation evidence

- T1 red state: Rust DTO tests failed before `provider_api_v2` existed; DTO
  goldens and compatibility tests pass 4/4.
- T2 red states: service/command tests failed before state/execute APIs and again
  before the dedicated Keychain rotation API; Rust V2 suite passes 9/9.
- T3 red state: frontend tests failed before V2 wrappers existed; focused
  TypeScript wrapper tests pass 5/5.
- Full Rust passes 229 tests with 2 ignored. Frontend passes 137/137. Phase 0
  passes 15/15; lint, production build and formats pass.

## STOP conditions

- A command requires exposing plaintext secret or helper output.
- Stable plan execution cannot share the existing planner/coordinator.
- Completion requires RPG-111 UI or weekly popover edits.
