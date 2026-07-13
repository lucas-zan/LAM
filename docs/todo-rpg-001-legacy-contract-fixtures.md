# Todo: RPG-001 Freeze legacy Provider, config, account, and DTO fixtures

> Executor instructions: This issue captures the current contract only. Write
> the Rust and frontend fixture-consumer tests before adding the fixture corpus.
> Do not implement Provider V2 migration, config mutation, or new DTOs here.

## Status

- **Status**: 验证成功
- **Priority**: P0
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: RPG-006 / G0
- **Category**: test/contract/migration
- **Parent tracker**: [`todo-remote-provider-gateway.md#rpg-001-freeze-legacy-provider-config-account-and-dto-fixtures`](./todo-remote-provider-gateway.md#rpg-001-freeze-legacy-provider-config-account-and-dto-fixtures)
- **Planned at**: 2026-07-10 workspace state

## Why this matters

Phase 1 replaces the current unversioned Provider array, flat Provider model,
destructive config writer, and shared persistence/API structs. Without immutable
legacy inputs, migration work can silently drop fields, rewrite files during a
read, change Tauri casing, or expose a credential.

This issue creates an executable boundary around what the current release must
continue to read. It deliberately does not decide or implement the V2 storage or
domain model.

## Scope

**In scope**:

- A central, versioned fixture manifest under
  `apps/desktop/src-tauri/tests/fixtures/legacy-provider-contract/`.
- Legacy Provider store arrays for `wireApi=openai` and
  `wireApi=responses`, plus empty, missing, malformed, and future-version cases.
- Existing Codex config variants: top-level only, official Provider table,
  legacy flat keys, quoted/dotted IDs, comments, unknown tables, CRLF/non-ASCII,
  and conflicting auth/placeholder secret-bearing fields.
- Existing account cache and managed-account metadata shapes.
- Current camelCase Provider, create request, attach request, attach result, and
  operation-plan DTO JSON.
- Rust tests for manifest integrity, current deserialization/error behavior,
  no-write reads, mtime preservation, and DTO round trips.
- Frontend tests that type-check and snapshot the shared DTO JSON shapes.
- Fixture sanitization checks for token prefixes, bearer values, personal home
  paths, email addresses, and known personal identifiers.

**Out of scope**:

- Provider V2 types, migration functions, or write-back.
- `toml_edit` config mutation behavior.
- Codex HTTP request/response capture; RPG-002 owns that corpus.
- Changing current Provider/config/account behavior to make a fixture pass.
- Real credentials, real user paths, or production account data.

## Contract design

### Manifest

The manifest has `schemaVersion=1`. Every physical fixture records:

- stable fixture ID and relative path;
- kind, origin, and input schema;
- expected future migration outcome;
- explicit `writeBackAllowed=false`;
- byte length and FNV-1a 64-bit checksum.

The missing-store case is represented by a manifest entry with no path or
checksum. Tests own the allowed fixture-ID inventory so adding or removing a
fixture requires a reviewed contract change.

FNV is used only as a dependency-free accidental-change detector; it is not a
security primitive. Separate content scanning enforces sanitization.

### Provider read contract

- A missing store and `[]` both return an empty list without creating a file.
- Current legacy arrays preserve all current fields and both accepted wire API
  strings exactly as stored.
- Reads do not write and do not change file modification time.
- Malformed JSON and a future envelope are rejected as
  `PROVIDER_STORE_INVALID` by the current reader.
- The fixtures describe the future migration expectation, but this issue does
  not perform it.

### Config contract

- TOML fixtures remain raw user-owned input; comments, line endings, quoting,
  unknown keys/tables, and Unicode are part of the fixture.
- Valid fixtures must parse as TOML, while tests assert the important raw
  preservation markers separately.
- CRLF is stored as an escaped JSON string so Git/editor newline normalization
  cannot silently convert the test case.
- Placeholder secret-bearing values use unmistakably synthetic text and must
  never resemble a real token.

### DTO contract

- Rust deserializes each current DTO fixture and serializes it back to exactly
  the same JSON value, locking camelCase names and optional/null behavior.
- Frontend imports the same JSON files, assigns them to the public TypeScript
  types, and asserts the exact public shape.
- Provider output contains only secret metadata/reference fields; create input
  uses an env reference and contains no plaintext secret.

**Observed compatibility gap**: the frontend type and submitted JSON use
`secret.envKey`, while the current Rust `SecretInput::Env` serde shape accepts
and emits `secret.env_key`. RPG-001 freezes both shapes and asserts the mismatch;
the API-boundary compatibility work must normalize it before the legacy window
can be considered complete. No production behavior is changed in this issue.

## Tasks

| ID | Task | Acceptance summary | Status |
| --- | --- | --- | --- |
| T1 | Add contract-consumer tests first | Rust and frontend tests fail because the manifest/corpus is absent | 验证成功 |
| T2 | Add sanitized immutable fixture corpus | Focused Rust/frontend contract tests pass without production changes | 验证成功 |
| T3 | Run repository quality gates | Rust/frontend suites and frontend build/lint/format pass | 验证成功 |

### T1: Add contract-consumer tests first

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Test design**:

- Rust manifest test fails with a missing `manifest.json`.
- Rust Provider test drives the public `list_providers` API against copied
  fixtures and checks normal, empty/missing, malformed, future-version, and
  no-write/mtime behavior.
- Rust DTO test locks deserialize/serialize camelCase JSON values.
- Rust config test validates parseability and raw preservation markers.
- Frontend contract test imports the shared DTO fixtures and checks exact typed
  values.

**Done criteria**:

- [x] Tests are added before any fixture file
- [x] Focused Rust test fails for the expected missing corpus
- [x] Focused frontend test fails for the expected missing DTO fixtures
- [x] Red evidence is recorded before T2

**Red evidence on 2026-07-10**:

- Rust: 5/5 contract tests failed only because `manifest.json` and referenced
  fixture files did not exist; exit 101.
- Frontend: the contract suite failed import resolution for the absent shared
  account/DTO fixtures before collecting tests; exit 1.

### T2: Add sanitized immutable fixture corpus

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Implementation**:

- Add only manifest and data fixtures; do not edit production source.
- Populate byte lengths/checksums only after fixture contents are final.
- Keep all endpoints under `example.test`, paths under `/fixture`, IDs generic,
  and secret values as explicit placeholders.
- Make expected migration text precise enough for RPG-102/RPG-105 tests to
  consume later.

**Done criteria**:

- [x] Inventory is complete and checksummed
- [x] All fixtures pass sanitization
- [x] Provider reads preserve content and mtime
- [x] Negative fixtures produce the frozen error contract
- [x] Rust and frontend focused tests pass (6 Rust, 3 frontend)
- [x] No production source file changed for RPG-001

**Validation evidence on 2026-07-10**:

- Rust contract suite: 6/6 passed.
- Frontend shared DTO contract suite: 3/3 passed.
- Manifest locks 19 logical cases, 18 physical sanitized fixture files, byte
  lengths, and FNV-1a checksums.

### T3: Run repository quality gates

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Acceptance**:

- `cd apps/desktop/src-tauri && cargo fmt --check` exits 0.
- `cd apps/desktop/src-tauri && cargo test --test provider_legacy_contract`
  exits 0.
- `cd apps/desktop && npm test -- src/lib/provider-contract.test.ts` exits 0.
- Full Rust and frontend suites exit 0.
- `cd apps/desktop && npm run lint` exits 0.
- `cd apps/desktop && npm run format:check` exits 0.
- `cd apps/desktop && npm run build` exits 0.
- `git diff --check` exits 0.

**Done criteria**:

- [x] Focused verification passes
- [x] Full Rust/frontend verification passes
- [x] Frontend lint, format, and build pass
- [x] Diff contains no generated cache mutation or secret
- [x] T3, RPG-001, and the master overview are `验证成功`

**Validation evidence on 2026-07-10**:

- Full Rust suite: 156 passed, 2 ignored, 0 failed.
- Full frontend suite: 15 files, 132 tests passed.
- Rust format, frontend lint/format, frontend production build, and
  `git diff --check`: passed.
- The Rust suite's generated `.fake-home` cache timestamp was restored; no
  generated cache diff remains.

## Verification commands

| Purpose | Command | Expected |
| --- | --- | --- |
| Rust red/focused | `cd apps/desktop/src-tauri && cargo test --test provider_legacy_contract` | missing corpus before T2; exit 0 after T2 |
| Frontend red/focused | `cd apps/desktop && npm test -- src/lib/provider-contract.test.ts` | missing DTO fixtures before T2; exit 0 after T2 |
| Rust full | `cd apps/desktop/src-tauri && cargo test` | exit 0 |
| Rust format | `cd apps/desktop/src-tauri && cargo fmt --check` | exit 0 |
| Frontend full | `cd apps/desktop && npm test` | exit 0 |
| Frontend lint | `cd apps/desktop && npm run lint` | exit 0 |
| Frontend format | `cd apps/desktop && npm run format:check` | exit 0 |
| Frontend build | `cd apps/desktop && npm run build` | exit 0 |
| Diff | `git diff --check` | exit 0 |

## STOP conditions

- A fixture contains or appears derived from a real credential/account/path.
- Current code cannot read an existing shape without a behavior change; record
  the gap for the owning migration issue instead of fixing it here.
- A V2 schema or config-writing decision is required to make the corpus pass.
