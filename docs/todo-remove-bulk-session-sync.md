# Todo: 移除整帐号 Sessions 同步功能

> Executor instructions: Follow this todo step by step. Generate tests from the
> "Test design" section before implementation. Run each verification command and
> confirm the expected result before moving to the next task. If a STOP condition
> occurs, stop and report instead of improvising.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MEDIUM
- **Depends on**: none
- **Category**: refactor
- **Planned at**: current workspace

## Why this matters

**Background**: LAM currently exposes a `Sync` first-level page and a `Sync Sessions...`
account action. Both copy an account's complete sessions directory to another account.
The actual workflow is usually the continuation of one current task, already covered by
`Relay Latest` and `Handoff`; bulk copying has no clear product use case.

**Current state**: the feature crosses route definitions, account cards, App modal state,
frontend API/types, Tauri commands and a Rust sync service. Merely hiding buttons would
leave an unsupported and callable backend surface.

**Impact**: keeping the feature adds visual noise, makes session ownership ambiguous and
increases the chance of copying stale or unrelated conversations between accounts.

**What improves**: account cards and primary navigation become focused on single-session
continuation, while dead frontend and native code is removed rather than left dormant.

## Scope

**In scope**:
- Remove the bottom `Sync` route/page and account-card `Sync Sessions...` actions.
- Remove the sync modal, its state, request/result contracts and frontend API calls.
- Remove Tauri sync commands, command registration and the Rust bulk-sync service.
- Remove or update tests and current documentation that advertise the feature.
- Preserve `Relay Latest`, per-session `Handoff`, unrelated adapter `Send + Sync`
  semantics and shared session-file helpers still used elsewhere.

**Out of scope**:
- Redesigning Relay/Handoff behavior.
- Deleting historical release records merely because they mention past implementation.
- Installing or replacing the packaged `/Applications/LAM.app`.

## Design

The supported continuation boundary is one session. No compatibility shim or hidden
bulk-sync command remains. UI route declarations are the source of truth for navigation;
account cards expose only supported actions. Transport contracts must not export commands
whose implementation was removed. Shared lower-level session utilities remain when another
feature depends on them. Historical documents can retain dated facts, while current user
and command-contract documentation must describe only supported behavior.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Remove visible product entry points | No Sync route/menu/button/modal; Relay and Handoff remain | 验证成功 |
| T2 | Remove frontend and native implementation | No sync API, command, type or Rust service remains | 验证成功 |
| T3 | Clean documentation and run regression | Current docs match behavior; full checks pass | 验证成功 |

### T1: Remove visible product entry points

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Users should not see or enter an unsupported whole-account workflow.

**What to do**:
- Remove `sync` from route types/navigation and delete `SyncHome`.
- Remove account-card `openSync` props and both compact/non-compact actions.
- Remove App's sync modal rendering/opening state and the dedicated modal component.
- Remove `sync` from modal state contracts and uniquely-owned styling/smoke assertions.

**Logic design**:
- Navigation derives solely from the route list, so removing the route removes the dock item.
- Cards retain Relay/Handoff actions unchanged in supported modes.
- No fallback redirect is required because persisted route state already validates against
  the route list; an invalid old value resolves to the normal default.

**Test design**:
- Before implementation, assert route ids/labels exclude Sync and confirm failure now.
- Assert account menus contain no `Sync Sessions...` action in normal mode.
- Assert every account still exposes Handoff and Relay Latest.

**Acceptance**:
- `npm test -- --run src/routes/handoff.test.tsx src/App.handoff.test.tsx`
- `rg -n "Sync Sessions|SyncHome|openSync|route === 'sync'|modal === 'sync'" apps/desktop/src`
  returns no product implementation matches.

**Done criteria**:
- [x] Tests listed above were written before implementation.
- [x] New tests failed for the expected existing `sync` route (`handoff.test.tsx`, 2026-07-17).
- [x] Implementation matches the design and scope.
- [x] Focused tests pass (60/60).
- [x] Source scan has no visible Sync feature match.
- [x] Status is `验证成功`.

### T2: Remove frontend and native implementation

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Hidden native commands and copy logic would remain an unsupported callable API.

**What to do**:
- Remove sync request/plan/result frontend contracts and API wrappers.
- Simplify plan rendering back to operation plans only.
- Remove Tauri commands and registrations, Rust sync service/types imports and sync tests.
- Add a persistent source-contract test proving the commands/service are absent.

**Logic design**:
- Remove the feature vertically at each boundary; do not retain no-op commands.
- Let TypeScript and Rust compilation expose any remaining consumer.
- Keep shared file scanners/types when referenced by Relay, backup or inventory code.

**Test design**:
- Add a Rust integration contract test that initially fails because main/commands/services
  still expose bulk sync.
- Compile/test frontend and Rust after removal to catch stale imports and registrations.
- Verify source scans exclude `build_sync_plan`, `execute_sync`, `SyncRequest`, `SyncPlan`
  and `SyncResult` from live source.

**Acceptance**:
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test removed_bulk_sync`
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test phase1_core`
- `npm test -- --run`

**Done criteria**:
- [x] Contract test was written before implementation.
- [x] Contract test failed on registered `commands::build_sync_plan` (2026-07-17).
- [x] Frontend/native implementation is removed.
- [x] Focused and relevant suites pass (frontend 224/224; Rust 57/57 across two suites).
- [x] Status is `验证成功`.

### T3: Clean documentation and run regression

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Current documentation must not instruct users to use a deleted feature.

**What to do**:
- Remove bulk Sync from current READMEs, route/command contracts and active design docs.
- Retain dated historical records where rewriting history would be misleading, marking the
  feature retired where a current-status statement is needed.
- Run formatting, lint, build, Rust formatting/tests and diff checks.

**Logic design**:
- Treat README and command contracts as authoritative current behavior.
- Distinguish whole-account session Sync from unrelated adapter `Send + Sync` terminology.

**Test design**:
- Capture a pre-change documentation scan showing current claims.
- After edits, scan authoritative docs for unsupported UI/command terms.
- Run full static and build checks as regression coverage.

**Acceptance**:
- `npm run lint && npm run build` from `apps/desktop`.
- `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --check`.
- `git diff --check`.

**Done criteria**:
- [x] Pre-change documentation scan was recorded (README, command contracts and active design docs still advertised bulk Sync).
- [x] Current documentation matches supported behavior; historical designs carry an explicit retirement amendment.
- [x] Format, lint, build, native binary check and relevant tests pass.
- [x] Status is `验证成功`.

## Test plan

- Normal: Sync route and account actions are absent; Relay/Handoff remain usable.
- Edge: PAT/profile/API card variants do not regain a Sync action through conditional UI.
- Invalid input: stale `sync` route values are not accepted by the current route contract.
- Error: removed native commands cannot be invoked because they are not registered.
- State/conflict: no sync modal/request/result state can survive route or account changes.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| UI focused tests | `npm test -- --run src/routes/handoff.test.tsx src/App.handoff.test.tsx` | exit 0 after implementation |
| Native removal contract | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test removed_bulk_sync` | exit 0 after implementation |
| Frontend suite | `npm test -- --run` | exit 0 |
| Native relevant suite | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test phase1_core` | exit 0 |
| Static/build | `npm run lint && npm run build` | exit 0 |
| Format/diff | `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --check && git diff --check` | exit 0 |

## Done criteria

- [x] Every task is `验证成功`.
- [x] Every task's own Done criteria are satisfied.
- [x] There are no live UI, frontend API, Tauri command or Rust service entry points.
- [x] Relay Latest and Handoff regressions remain green.
- [x] No STOP condition remains unresolved.

## STOP conditions

- The identified Sync controls invoke behavior other than whole-account session copying.
- Removing shared code would break Relay/Handoff and cannot be cleanly separated.
- Existing unrelated workspace changes conflict with the required edits.
- Relevant tests still fail after the scoped fix loop.
