# Todo: RPG-005 Phase Zero Gate

> Executor instructions: Add adversarial gate tests first, observe the expected
> failure, then implement the offline verifier, approval artifact and coverage
> report. Do not invoke Codex or network during verification.

## Status

- **Priority**: P0
- **Effort**: S
- **Risk**: MEDIUM
- **Depends on**: RPG-003, RPG-004
- **Category**: test/CI
- **Planned at**: `codex/202607071457-ui-optimize-20260710102915-remote-provider-gateway`

## Why this matters

**Background**: G1 currently depends on manually correlating fixtures, checksum,
sanitization, route coverage and design decisions.

**Current state**: Rust contract tests validate the checked-in Codex fixture, but
there is no single offline Phase 0 command, explicit approval artifact, generated
coverage report or adversarial verifier test.

**Impact**: A removed scenario, stale checksum, leaked marker or changed design
decision could pass review without invalidating the Phase 0 gate.

**What improves**: One deterministic command proves the exact-tested contract and
approved architecture inputs before Phase 1 begins.

## Scope

**In scope**:

- Offline Node verifier and tests.
- Versioned Phase 0 approval JSON.
- Deterministic contract coverage Markdown report.
- Package command and G1 status/evidence links.

**Out of scope**:

- Running the live Codex capture harness.
- Replacing Rust fixture behavior tests.
- Phase 1 implementation.

## Design

The verifier accepts an injectable fixture root/approval/report for tests and
fails closed. It validates schema/target/support policy, exact required and
observed routes, mandatory scenario ids/status/runs/determinism/artifacts,
manifest ownership, FNV-1a checksum/byte count, secret/path/content markers,
approved architecture decisions and exact generated report content. The normal
CLI is read-only; `--write-report` is an explicit maintainer action.

## Tasks

### Task overview

| ID  | Task                                    | Acceptance summary                                                | Status   |
| --- | --------------------------------------- | ----------------------------------------------------------------- | -------- |
| T1  | Add adversarial Phase 0 gate tests      | Corrupt manifest, secret marker and missing case fail as expected | 验证成功 |
| T2  | Implement verifier, approval and report | Offline package command passes and G1 evidence is reproducible    | 验证成功 |

### T1: Add adversarial Phase 0 gate tests

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:

The gate must prove it fails closed, not merely print a success message for the
current fixture.

**What to do**:

- Add tests against a temporary fixture copy.
- Corrupt a manifest checksum/byte count.
- Inject a synthetic secret marker into an owned artifact.
- Remove one mandatory scenario.

**Logic design**:

- Tests call an exported verifier with temporary paths and no network/process
  dependency.
- Each mutation must fail with a specific diagnostic category.

**Test design**:

- Valid fixture passes.
- `manifest checksum mismatch` mutation fails.
- `synthetic secret marker` mutation fails before checksum acceptance.
- `missing mandatory contract case` mutation fails.

**Acceptance**:

- Initial `node --test scripts/verify-remote-provider-gateway-phase0.test.mjs`
  fails because the verifier module does not exist.
- After implementation all adversarial cases pass.

**Done criteria**:

- [x] Tests written before verifier implementation
- [x] Expected red state recorded: `ERR_MODULE_NOT_FOUND` for the not-yet-created verifier module
- [x] All four behavior cases pass
- [x] T1 becomes `验证成功`

### T2: Implement verifier, approval and report

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:

Phase 1 needs one machine-readable contract gate and human-readable coverage
summary.

**What to do**:

- Implement verifier CLI/library without third-party runtime dependencies.
- Add approval JSON covering platform, routes, retries, store, credentials,
  state mode and threat decisions.
- Generate and verify the deterministic coverage report.
- Add package command and update G1/master design evidence.

**Logic design**:

- Normal verification never mutates artifacts and never invokes Codex/network.
- Approval and manifest must agree on target/routes/state mode.
- Report is derived from validated inputs and exact-content checked.

**Test design**:

- Existing T1 tests cover fail-closed behavior.
- CLI succeeds on checked-in artifacts.
- Capture helper 9/9 and Rust contract suites remain green.
- Additional approval-mismatch and stale-report cases were added after the
  shared T1-driven verifier existed; a separate red state could not be produced
  without removing the already-tested implementation, so their adversarial
  failure assertions are the documented substitute.

**Acceptance**:

- `pnpm test:gateway-phase0` exits 0.
- Focused Rust contract tests exit 0.
- Prettier and `git diff --check` pass.

**Done criteria**:

- [x] Implementation follows the design
- [x] Approval/report are verifier-owned and deterministic
- [x] Package gate passes offline
- [x] Relevant contract suites and formatting pass
- [x] G1 is marked `验证成功` with evidence
- [x] T2 becomes `验证成功`

## Test plan

- Valid fixture.
- Corrupt checksum/bytes.
- Synthetic API key/bearer/content marker.
- Missing mandatory scenario or route.
- Approval/manifest mismatch and stale report.

## Verification commands

| Purpose       | Command                                                                                                                                                                                                                                                         | Expected on success                      |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------- |
| Red/focused   | `node --test scripts/verify-remote-provider-gateway-phase0.test.mjs` from `apps/desktop`                                                                                                                                                                        | fails before implementation; then exit 0 |
| Phase 0 gate  | `pnpm test:gateway-phase0` from `apps/desktop`                                                                                                                                                                                                                  | exit 0                                   |
| Rust contract | `cargo test --test provider_legacy_contract --test provider_codex_contract` from `apps/desktop/src-tauri`                                                                                                                                                       | exit 0                                   |
| Format/diff   | `pnpm exec prettier --check scripts/verify-remote-provider-gateway-phase0.mjs scripts/verify-remote-provider-gateway-phase0.test.mjs ../../docs/remote-provider-gateway-contract-coverage.md ../../docs/todo-rpg-005-phase-zero-gate.md` and `git diff --check` | exit 0                                   |

## Done criteria

- [x] T1 and T2 are `验证成功`
- [x] All task checklists are complete
- [x] G0 and G1 are explicitly `验证成功`
- [x] No STOP condition remains

## STOP conditions

- Offline verification requires running Codex or using network.
- Existing fixture does not contain evidence for a mandatory approved decision.
- A gate mutation cannot be made to fail deterministically.
