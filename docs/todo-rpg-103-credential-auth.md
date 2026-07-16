# Todo: RPG-103 Credential Source and Upstream Authentication

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: HIGH
- **Depends on**: RPG-101
- **Category**: security/domain

## Why this matters

Provider V2 currently stores a credential reference beside a coarse bearer/none
flag. Header authentication, safe env resolution and route-specific Codex mapping
are not representable without leaking transport concerns into storage kinds.

## Scope

**In scope**: closed `CredentialSource`/`UpstreamAuth`, header/env validation,
secret wrapper and resolver boundary, readiness errors, direct Codex auth mapping,
static query/header credential rejection, V2 migration.

**Out of scope**: Keychain I/O and auth-command execution; UI and weekly files.

## Design

`UpstreamAuth` owns its `CredentialSource` and is `bearer`, `header`, or `none`.
Resolved values live in a non-serializable/redacted `SecretValue` exposed only to a
closure. Environment access is injected. Direct Codex supports env bearer/header
references; other sources return a structured unsupported-route error.

## Tasks

| ID  | Task                          | Test design                                                                                     | Status   |
| --- | ----------------------------- | ----------------------------------------------------------------------------------------------- | -------- |
| T1  | Add credential/auth tests     | bearer/header/none, missing/empty/invalid/conflict, route mapping, leak/static-option rejection | 验证成功 |
| T2  | Implement credential boundary | focused, Provider V2, full Rust and format pass                                                 | 验证成功 |

### T1

**Status**: [ ] 待执行 [ ] 待测试验证 [x] 验证成功 [ ] 验证失败

**Why/What/Logic**: Fix public shapes and fail-closed behavior before replacing
Provider V2 fields. Tests use an injected environment and synthetic marker.

**Acceptance**: `cargo test --test provider_credentials` first fails on missing
API, then passes.

**Done criteria**: [x] tests first [x] red recorded [x] all cases represented [x] status success

### T2

**Status**: [ ] 待执行 [ ] 待测试验证 [x] 验证成功 [ ] 验证失败

**Why/What/Logic**: Implement a provider-neutral auth model and narrow secret
boundary; update V2/migration without plaintext values or vendor branches.

**Acceptance**: focused + Provider V2 + full Rust + format/diff pass.

**Done criteria**: [x] implementation scoped [x] no leak surface [x] suites pass [x] status success

## Test plan

Normal bearer/header/none; invalid names and source/auth combinations; missing and
empty env; secret Debug/JSON/errors; direct mapping; sensitive query/static-header
attempts.

## Verification commands

| Purpose    | Command                                       | Expected        |
| ---------- | --------------------------------------------- | --------------- |
| Focused    | `cargo test --test provider_credentials`      | red then exit 0 |
| Regression | `cargo test --test provider_v2 && cargo test` | exit 0          |
| Format     | `cargo fmt --check && git diff --check`       | exit 0          |

## Done criteria

- [x] T1/T2 are `验证成功`
- [x] Master status/evidence updated
- [x] No STOP condition remains

## STOP conditions

Plaintext must enter a DTO/store, or direct mapping requires executing Keychain or
auth commands before RPG-109.
