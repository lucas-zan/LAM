# Todo: RPG-104 Profile Binding Lifecycle

## Status

- **Priority**: P0
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: RPG-101, RPG-102
- **Category**: binding/persistence

## Why this matters

Legacy used-by inference scans account/config state. Phase 1 requires one
revisioned authority for provider/model/config ownership and explicit drift.

## Scope

**In scope**: binding model/store, managed/adopted projection metadata,
reconciliation states, adoption, used-by, rename/delete/detach lifecycle,
provider-route staleness and CAS conflicts. **Out of scope**: TOML mutation,
journal/Gateway token, UI.

## Design

One binding per stable profile ID is stored through RPG-101. Binding revision and
store revision are both checked. Adoption receives a parsed config observation,
never scans config after binding creation. Reconciliation compares managed values
and provider fingerprint deterministically; used-by queries only the store.

## Tasks

| ID  | Task                        | Test design                                                                           | Status   |
| --- | --------------------------- | ------------------------------------------------------------------------------------- | -------- |
| T1  | Add lifecycle tests         | adopt/idempotence/errors, managed/drift/stale, used-by, rename/delete/detach/conflict | 验证成功 |
| T2  | Implement binding authority | focused/full suites and format pass                                                   | 验证成功 |

### T1

**Status**: [ ] 待执行 [ ] 待测试验证 [x] 验证成功 [ ] 验证失败

**Why/What/Logic**: Lock state transitions before persistence implementation.
Adoption inputs explicitly describe ambiguity/auth/manageability.

**Acceptance**: `cargo test --test provider_binding` fails on missing API, then passes.

**Done criteria**: [x] tests first [x] red recorded [x] transitions covered [x] status success

### T2

**Status**: [ ] 待执行 [ ] 待测试验证 [x] 验证成功 [ ] 验证失败

**Why/What/Logic**: Implement domain-neutral binding repository on versioned CAS;
no config/Gateway reverse reconstruction.

**Acceptance**: focused, Provider/storage and full Rust suites pass.

**Done criteria**: [x] implementation scoped [x] conflict zero-side-effect [x] suites pass [x] status success

## Test plan

Adopt custom/legacy observations and repeat; unknown/malformed/unsupported/ambiguous;
managed/unmanaged drift; provider changes; rename/delete/detach; revision conflict;
used-by source of truth.

## Verification commands

| Purpose    | Command                                                                       | Expected        |
| ---------- | ----------------------------------------------------------------------------- | --------------- |
| Focused    | `cargo test --test provider_binding`                                          | red then exit 0 |
| Regression | `cargo test --test provider_v2 --test provider_versioned_store && cargo test` | exit 0          |
| Format     | `cargo fmt --check && git diff --check`                                       | exit 0          |

## Done criteria

- [x] T1/T2 `验证成功`
- [x] Master updated
- [x] No STOP condition

## STOP conditions

Binding authority requires parsing/mutating TOML inside the store or silently
overwriting drift.
