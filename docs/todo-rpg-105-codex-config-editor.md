# Todo: RPG-105 Non-destructive Codex Config Editor

## Status

- **Priority**: P0
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: RPG-102, RPG-103
- **Category**: config/security

## Why this matters

The legacy attach path replaces the whole config with a formatted string. V2 must
preserve user TOML/comments and own only explicit values with drift-safe detach.

## Scope

**In scope**: `toml_edit` snapshot/apply/detach, managed allowlist and projection,
auth conflict/security validation, hashes, private collision-free backup and
atomic write fault tests. **Out of scope**: journal/executor, API/UI, weekly files.

## Design

The editor hashes original bytes, checks expected hash, mutates only top-level
model/provider and the selected provider allowlist, validates the result, writes a
private UUID backup and same-directory atomic replacement. Projection stores
non-secret previous/applied TOML values. Detach compares ownership before restoring;
repeat apply with a matching projection is a no-write idempotent success.

## Tasks

| ID  | Task                        | Test design                                                                   | Status   |
| --- | --------------------------- | ----------------------------------------------------------------------------- | -------- |
| T1  | Add editor tests            | empty/complex/direct/gateway/dotted/auth/conflict/fault/repeat/detach/backups | 验证成功 |
| T2  | Implement editor/projection | focused/full suites and format pass                                           | 验证成功 |

### T1

**Status**: [ ] 待执行 [ ] 待测试验证 [x] 验证成功 [ ] 验证失败

**Why/What/Logic**: Behavior tests assert preserved fragments instead of whole-file
replacement and inject pre-rename failure.

**Acceptance**: `cargo test --test provider_config_editor` fails on missing API, then passes.

**Done criteria**: [x] tests first [x] red recorded [x] safety/failure paths covered [x] status success

### T2

**Status**: [ ] 待执行 [ ] 待测试验证 [x] 验证成功 [ ] 验证失败

**Why/What/Logic**: Implement a narrow editor; legacy string attach remains only
for frozen compatibility until RPG-110.

**Acceptance**: focused/full Rust, Phase 0, format/diff pass.

**Done criteria**: [x] no whole-file V2 replacement [x] ownership-safe detach [x] suites pass [x] status success

## Test plan

Empty and complex comments; direct/Gateway responses config; dotted IDs; auth
mutual exclusion; sensitive inputs; hash/parse/temp failures; repeat; managed drift;
unmanaged additions; unique backups.

## Verification commands

| Purpose    | Command                                    | Expected        |
| ---------- | ------------------------------------------ | --------------- |
| Focused    | `cargo test --test provider_config_editor` | red then exit 0 |
| Regression | `cargo test && pnpm test:gateway-phase0`   | exit 0          |
| Format     | `cargo fmt --check && git diff --check`    | exit 0          |

## Done criteria

- [x] T1/T2 `验证成功`
- [x] Master updated
- [x] No STOP condition

## STOP conditions

Preservation needs whole-file replacement, secret-bearing prior values would enter
projection, or safe mutation conflicts with user-owned keys.
