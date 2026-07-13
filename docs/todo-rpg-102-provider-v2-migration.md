# Todo: RPG-102 Provider V2 and Legacy Migration

> Executor instructions: Add Provider V2 domain/repository/migration tests first,
> observe the missing-API failure, then implement without changing legacy Tauri DTOs.

## Status

- **Priority**: P0
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: RPG-101
- **Category**: provider/domain/persistence

## Why this matters

**Background**: Legacy Provider metadata is a flat unversioned array whose wire,
secret, health and runtime concepts are conflated.

**Current state**: RPG-101 supplies safe storage, while `provider.rs` still owns
the compatibility API and raw V0 format.

**Impact**: Binding/config/auth work cannot depend on a typed Provider contract or
revision semantics until V2 exists.

**What improves**: A domain-only V2 repository and pure fixture migration become
available without prematurely breaking legacy DTO compatibility.

## Scope

**In scope**:

- Typed protocol/model/adapter/capability/Codex/credential-reference metadata.
- Validation, canonical URL and safe upstream path join.
- Pure legacy array migration with warnings and no write-back.
- V2 repository CRUD on RPG-101 storage with revision CAS and timestamps.

**Out of scope**:

- Plaintext secret resolution and transport mapping (RPG-103).
- Binding/config projection and current Tauri command cutover.
- Provider frontend.

## Design

Add `provider_v2.rs` alongside legacy `provider.rs`. V2 envelopes flatten a
`providers` collection beside schema/revision. Provider IDs keep the frozen
syntax, including dots as opaque IDs; config quoting is RPG-105's responsibility.
Remote HTTP is rejected; HTTPS preserves path prefixes. Adapter paths are
absolute-path-shaped but authority/query/fragment/traversal-free and are joined by
path segments without `Url::join` prefix replacement. Migrations receive an
explicit timestamp and return warnings, never filesystem writes.

## Tasks

### Task overview

| ID  | Task                                | Acceptance summary                                                       | Status   |
| --- | ----------------------------------- | ------------------------------------------------------------------------ | -------- |
| T1  | Add Provider V2 and migration tests | Domain, invalid inputs, migration, join, no-secret tests fail before API | 验证成功 |
| T2  | Implement V2 domain and repository  | Focused and full Rust suites pass; legacy remains green                  | 验证成功 |

### T1: Add Provider V2 and migration tests

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: The schema and invalid-state boundaries must be fixed before code.

**What to do**:

- Test Responses and Chat Completions metadata.
- Test IDs, URL, model, adapter/path and compatibility rejection.
- Test both legacy fixtures, deterministic timestamp/warnings and no writes.
- Test repository revision/timestamp semantics and secret-free Debug/JSON.

**Logic design**:

- Use synthetic reference values only; no plaintext secret field exists.
- Repository input accepts an explicit clock value for deterministic tests.

**Test design**:

- Normal create/update/load and CAS conflict.
- Validation table for every required invalid class.
- Legacy `openai` and `responses` map to protocol Responses, model catalog and
  bearer env reference; `openai` adds warning.
- URL join preserves base prefix and rejects injection.

**Acceptance**: focused test fails on missing `provider_v2` API, then passes.

**Done criteria**:

- [x] Tests precede implementation
- [x] Red state recorded: unresolved `localagentmanager_core::provider_v2` import
- [x] All behavior classes represented
- [x] T1 is `验证成功`

### T2: Implement V2 domain and repository

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Later Phase 1 issues require one validated Provider authority.

**What to do**:

- Implement types, validation, migration, URL join and repository.
- Add direct `url` dependency.
- Export V2 API while retaining legacy API until RPG-110.

**Logic design**:

- Closed enums and BTree collections keep serialization deterministic.
- Create/update validate before CAS; update preserves `created_at`.
- Health/readiness/used-by/conformance never enter V2 persistence.

**Test design**: T1 tests, legacy contract tests, full Rust suite, rustfmt/diff.

**Acceptance**: focused/full suites green; V2 writes have no legacy `wireApi`.

**Done criteria**:

- [x] Domain/persistence/API separation preserved
- [x] V2 invariants and pure migration implemented
- [x] Focused and legacy suites pass
- [x] Full Rust/format/diff pass
- [x] T2 is `验证成功`

## Test plan

- Valid Responses/Chat metadata.
- Invalid/reserved/dotted IDs, URL/userinfo/HTTP, model catalog, adapter path/id,
  compatibility profile.
- Legacy fixtures and deterministic no-write migration.
- Prefix-preserving endpoint join.
- Revision CRUD and timestamps.
- JSON/Debug synthetic secret absence.

## Verification commands

| Purpose     | Command                                                       | Expected        |
| ----------- | ------------------------------------------------------------- | --------------- |
| Focused     | `cargo test --test provider_v2` from `apps/desktop/src-tauri` | red then exit 0 |
| Legacy      | `cargo test --test provider_legacy_contract`                  | exit 0          |
| Full/format | `cargo test`, `cargo fmt --check`, `git diff --check`         | exit 0          |

## Done criteria

- [x] T1/T2 `验证成功`
- [x] Master RPG-102 evidence updated
- [x] No STOP condition remains

## STOP conditions

- Legacy fixture requires inferring Chat Completions from `wireApi = openai`.
- V2 needs plaintext secret storage.
- Repository cutover would break frozen compatibility DTOs before RPG-110.
