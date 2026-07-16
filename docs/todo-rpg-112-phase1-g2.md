# Todo: RPG-112 Phase 1 G2 vertical-slice gate

> Start after RPG-111. Add the end-to-end acceptance test before supporting
> implementation, confirm its expected failure, then run every gate.

## Status

- **Priority**: P0
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: RPG-107, RPG-110, RPG-111
- **Category**: acceptance/security
- **Planned at**: current uncommitted Phase 0/1 worktree

## Why this matters

**Background**: Phase 1 promises a usable Responses Provider direct slice.

**Current state**: Coverage exists through RPG-110, but no single gate composes
all credential paths, migration, complex config, rebind/stale/drift/detach and
the pinned Codex contract without Gateway.

**Impact**: Phase 2 must not start on an unproven direct-provider baseline.

**What improves**: A repeatable offline acceptance harness and full repository
gates provide auditable G2 evidence.

## Scope

**In scope**:
- Rust vertical-slice acceptance using local fakes/offline fixtures.
- Env, fake Keychain and approved auth-command credentials.
- Create, route validation, preview, attach, config/contract assertion, rebind,
  replay/stale/drift, migration and detach.
- Source-to-test mapping and all Rust/frontend/Phase 0/quality gates.

**Out of scope**:
- Real Provider/Keychain automation, Gateway startup and Phase 2 adapters.

## Design

- Compose public Phase 1 services over a temporary HOME, fake Keychain, fake
  auth-command executor and approved Codex fixtures.
- Any local upstream record contains only method/path/header names/status, never
  authorization values or bodies.
- Assert direct route and a fake Gateway lifecycle with zero starts.
- Audit each Phase 1 source module has focused test mapping.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
| --- | --- | --- | --- |
| T1 | Vertical-slice acceptance harness | Credential/migration/conflict/direct-route scenarios pass | 验证成功 |
| T2 | Repository G2 gate | Full suites/audits pass and G2 status updated | 验证成功 |

### T1: Vertical-slice acceptance harness

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Distributed unit tests do not prove feature composition.

**What to do**: Add provider_phase1_g2.rs with local fakes and fixtures,
covering all three credentials, legacy migration, complex config,
attach/rebind/stale/drift/detach and direct/no-Gateway behavior.

**Logic design**: Reuse public Phase 1 APIs. Secrets stay inside resolver/request
boundaries; scan persisted artifacts/records for marker absence; use pinned
Codex fixtures as the offline route oracle.

**Test design**: Env full lifecycle; Keychain/auth-command variations; legacy
migration; stale/config drift/replay/safe detach; zero Gateway starts and no
secret/body logs.

**Red-state evidence**: The first focused run passed env and legacy scenarios
but failed the Keychain/auth-command scenario because the direct planner still
emitted UnsupportedCredentialRoute for Keychain.

**Acceptance**: cargo test --test provider_phase1_g2

**Done criteria**:
- [x] Tests written before supporting implementation
- [x] Expected red failure recorded
- [x] Focused acceptance passes
- [x] Secret scans pass
- [x] Status is 验证成功

### T2: Repository G2 gate

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: G2 is a merge gate across Phase 0/1.

**What to do**: Run source/test mapping; run full Rust/frontend/Phase 0/lint/
build/format/diff gates; update RPG-112, G2, evidence and design status.

**Logic design**: Any failure keeps G2 closed. Evidence records exact counts and
states that direct routing did not start Gateway.

**Test design**: Mechanical mapping plus full regression and deterministic
contract verification.

**Red-state exception**: The mapping audit is a gate over already-created
Phase 1 source/test pairs. Manufacturing a red state would require deleting or
renaming verified user-worktree files, so it is run directly as a non-mutating
audit. The vertical-slice behavior itself had a recorded red state in T1.
The final product audit also produced and resolved red tests for atomic new
Keychain Provider creation and lossless Codex-option view round-tripping.

**Acceptance**: cargo test --all-targets; pnpm test; pnpm
test:gateway-phase0; pnpm lint; pnpm build; pnpm format:check; cargo fmt --all
-- --check; git diff --check

**Done criteria**:
- [x] Mapping passes
- [x] Full suites pass
- [x] Quality checks pass
- [x] RPG-112 and G2 are 验证成功
- [x] Status is 验证成功

## Test plan

- Offline lifecycle for three credential paths.
- Migration, complex TOML, stale/replay/drift/security.
- Full regression and contract checksum/sanitization.

## Verification commands

| Purpose | Command | Expected |
| --- | --- | --- |
| Focused | cargo test --test provider_phase1_g2 | exit 0 |
| Rust | cargo test --all-targets | exit 0 |
| Frontend | pnpm test | exit 0 |
| Phase 0 | pnpm test:gateway-phase0 | exit 0 |
| Quality | lint/build/format/diff commands above | exit 0 |

## Done criteria

- [x] T1 and T2 are 验证成功
- [x] Master G2 is 验证成功
- [x] No STOP condition remains

## STOP conditions

- Any credential cannot be isolated from real secret storage/network.
- Direct Responses unexpectedly requires Gateway.
- A full gate remains red after five fix loops.
