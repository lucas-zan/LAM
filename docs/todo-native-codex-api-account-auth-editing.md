# Todo: Native Codex API account authentication and editing

> Executor instructions: Follow this todo step by step. Generate tests from
> each task's Test design before implementation, confirm the expected failure,
> then implement and verify. Stop instead of bypassing a failed safety contract.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: HIGH
- **Depends on**: `docs/todo-native-responses-direct-routing.md`
- **Category**: feature/security/migration
- **Planned at**: current working tree

## Why this matters

**Background**: Responses API accounts now route directly to their remote
`/responses` endpoint, but their bearer key is still obtained through LAM's
`lam-auth-helper` and macOS Keychain. That causes authorization prompts and is
not the same credential path as a native Codex API-key login.

**Current state**: API account creation accepts `keychainSecret`, converts the
Provider credential to a Keychain reference, and projects a command-backed auth
table into the account's `config.toml`. Provider Center can edit Provider
metadata and rotate Keychain credentials, but it does not update an attached API
account's config projection as one account-level operation.

**Impact**: Direct Responses accounts can still prompt after debug rebuilds,
URL edits can leave an attached profile stale, and users cannot manage URL and
key through one clear API-account workflow.

**What improves**: Responses API accounts use Codex's documented file login
format (`auth.json` with mode `apikey`) and `requires_openai_auth`, so their
wrapper launches Codex without LAM/Keychain runtime access. Account URL and key
become viewable-as-metadata/write-only-as-secret and safely editable.

## Scope

**In scope**:

- Native Codex API-key file writer/reader boundary with private permissions.
- A typed Provider credential reference for a profile-owned Codex auth file.
- Responses API account creation and legacy Keychain migration.
- Account-level detail and update services for URL and optional replacement key.
- Tauri, TypeScript API/store, and Provider Center editing integration.
- Tests, design documentation, and real welfare migration/verification.

**Out of scope**:

- Showing an existing API key value.
- Removing Gateway/Keychain support for Chat Completions Providers.
- Building the future remote billing backend. The resulting per-account bearer
  key and direct remote base URL remain compatible with such a backend.
- Sharing one profile-owned credential across multiple API accounts.

## Design

- Responses API accounts own their credential under their independent
  `CODEX_HOME/auth.json`; secret values never enter Provider views, logs, plans,
  command arguments, or frontend state after submission.
- The stored file matches Codex API-key login semantics: `auth_mode = "apikey"`
  and `OPENAI_API_KEY = <secret>`, written atomically with directory mode 0700
  and file mode 0600.
- Provider metadata stores only a typed `codex_profile` reference naming the
  owning profile. Direct planning maps this reference to Codex native auth and
  projects `requires_openai_auth = true`; it never emits an auth command.
- API-account detail returns profile/provider/model/protocol/base URL and a
  boolean indicating whether a key is configured. It never returns the key.
- Update accepts the expected Provider revision, normalized HTTPS base URL, and
  an optional non-empty replacement key. It validates exclusive ownership and
  direct Responses protocol, updates Provider metadata, reprojects the bound
  Codex config, and rotates the auth file with rollback on failure.
- Legacy direct Responses API accounts backed by LAM Keychain migrate once:
  read the old secret, stage native auth, revoke the old reference, then replace
  Provider metadata and rebind. Every later failure restores the old reference
  and removes staged auth. After success, startup is idempotent and performs no
  Keychain read.
- Chat Completions behavior remains unchanged.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|---|---|---|---|
| T1 | Native Codex credential contract | New Responses accounts and legacy accounts use private `auth.json` with no auth helper | 验证成功 |
| T2 | API account detail/update service | URL and optional key update reproject the account safely and redact secrets | 验证成功 |
| T3 | Provider Center editing workflow | UI displays account configuration and edits URL/key through the account API | 验证成功 |
| T4 | Regression and real migration | Relevant suites and real direct launch pass without Gateway or auth helper | 验证成功 |

### T1: Native Codex credential contract

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Direct Responses traffic should not depend on a LAM executable or
macOS Keychain after account provisioning.

**What to do**:
- Add the profile-owned credential/domain and config projection contracts.
- Add an atomic private Codex API-key auth-file boundary.
- Use it for new Responses API accounts.
- Migrate existing exclusive Responses API accounts from Keychain idempotently.

**Logic design**:
- Reject empty keys, invalid profile identifiers, malformed auth files, and
  credential/profile mismatches with stable error codes.
- Config projection owns and clears mutually exclusive auth fields.
- Migration writes native auth before revoking the old credential and restores
  the old credential if Provider replacement or direct rebind fails.

**Test design**:
- Creation writes a 0600 `auth.json`, sets `requires_openai_auth = true`, and
  emits no `auth` command/Keychain reference.
- Credential/config unit tests cover normal, empty, malformed, conflicting, and
  detach/restore behavior.
- Legacy Keychain migration is idempotent and does not resolve Keychain again
  after the first successful migration.
- Chat Completions continues to use Gateway credentials.

**Acceptance**:
- `cargo test --test provider_credentials --test provider_config_editor --test provider_phase4_api_account -- --test-threads=1`

**Failure-first record**: The focused suite failed before implementation with
missing `CodexProfile`, `NativeApiKey`, and `codex_api_key_auth` contracts, which
is the expected boundary-level failure.

**Done criteria**:
- [x] Tests listed above were written first
- [x] Expected failure was recorded
- [x] Implementation matches this task design
- [x] Focused tests pass
- [x] Formatting passes
- [x] Status is `验证成功`

### T2: API account detail/update service

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Generic Provider edits do not guarantee that an attached API account's
Provider, config projection, and credential stay consistent.

**What to do**:
- Add redacted detail and revision-aware update DTOs/services.
- Update URL and optionally replace the API key.
- Register Tauri commands and TypeScript bindings.

**Logic design**:
- Resolve account → binding → exclusive Provider explicitly.
- Reject shared, Gateway, non-Responses, stale revision, invalid URL, empty key,
  and managed-config conflicts before irreversible changes.
- Preserve the existing key when the optional replacement is absent.
- Restore Provider/config/auth snapshots if a later commit fails.

**Test design**:
- Detail exposes URL/key-present metadata but never secret content.
- URL-only update changes Provider and profile config while preserving auth.
- Key-only and combined updates replace `auth.json` privately.
- Invalid/stale/shared/conflicting updates fail without partial changes.
- DTO serialization and command registration remain stable.

**Acceptance**:
- `cargo test --test provider_phase4_api_account --test provider_api_v2 -- --test-threads=1`

**Failure-first record**: Focused tests failed with the expected missing
redacted detail DTO and revision-aware update service imports.

**Done criteria**:
- [x] Tests listed above were written first
- [x] Expected failure was recorded
- [x] Implementation matches this task design
- [x] Focused tests pass
- [x] Formatting passes
- [x] Status is `验证成功`

### T3: Provider Center editing workflow

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Users need one discoverable screen to inspect an API account's remote
URL and replace its key without exposing the current secret.

**What to do**:
- Add API/store methods for detail and update.
- Route exclusive API-account cards to an account editor.
- Show profile, protocol, URL, model, and configured-key state; make a new key
  optional and write-only.

**Logic design**:
- Existing secret fields always render empty.
- Save sends the key only when the user entered a replacement.
- Conflicts refresh state and keep a clear recoverable error.
- Generic reusable Provider editing remains available for non-account Providers.

**Test design**:
- Component test opens an API-account editor with redacted details.
- URL-only save omits `apiKey`; key replacement sends it once and clears input.
- Invalid URL/empty-whitespace key and API errors stay in the modal.
- Store/API tests verify exact Tauri command payloads and conflict refresh.

**Acceptance**:
- `npm test -- --run src/components/provider-center.test.tsx src/stores/providers-v2.test.ts src/lib/api.test.ts`

**Failure-first record**: The targeted Vitest run failed with the expected
missing component export, API functions, and store actions; all pre-existing
tests in the selected files remained green.

**Done criteria**:
- [x] Tests listed above were written first
- [x] Expected failure was recorded
- [x] Implementation matches this task design
- [x] Focused tests pass
- [x] TypeScript build and changed-file lint pass
- [x] Status is `验证成功`

Global `npm run lint` remains blocked by the pre-existing, out-of-scope
`react-hooks/set-state-in-effect` finding at `src/routes/views.tsx:1444`; the
targeted ESLint run for every changed frontend file exits 0.

### T4: Regression and real migration

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Authentication migration is complete only when existing profiles work
and no Gateway/Keychain helper remains on the Responses runtime path.

**What to do**:
- Run all relevant Provider/Gateway/API-account suites.
- Migrate `welfare` and verify its generated files without exposing the key.
- Run a minimal real Responses request and confirm no Gateway process exists.

**Logic design**:
- One legacy Keychain access may be required to transfer the existing key; after
  success, repeated launch/migration must not access it.
- Verification inspects only file shape, permissions, redacted metadata, and
  process state.

**Test design**:
- Automated source assertions prevent Responses wrappers/configs from
  reintroducing `lam-auth-helper`.
- Real request returns `OK`; config uses remote URL/native auth and Gateway state
  remains unclaimed.

**Acceptance**:
- Relevant Rust integration suite, targeted Vitest suite, `cargo fmt --check`,
  `npm run build`, changed-file ESLint, and `git diff --check` all exit 0. The
  documented pre-existing global lint baseline remains outside this change.

**Done criteria**:
- [x] Relevant suites pass
- [x] Existing welfare profile is migrated
- [x] Real request passes without Gateway/auth helper
- [x] Documentation matches final behavior
- [x] Status is `验证成功`

**Verification record**: 147 focused Rust tests and 56 targeted frontend tests
passed. `welfare` and `idragon3` were migrated to private mode-0600 native auth
files and direct bindings; their configs contain no helper command. A direct
`codex-idragon3 exec` returned `OK` with no Gateway process. The current
`welfare` request reached its remote `/v1/responses` endpoint directly but that
service returned two 503 responses; this is an upstream availability failure,
not a local auth or Gateway failure.

## Test plan

- Normal: create, inspect, URL-only update, key-only update, combined update,
  legacy migration, direct launch.
- Edge: optional key omitted, Codex-added unmanaged config fields, repeated
  migration/update, no active Chat binding.
- Invalid: empty key, invalid URL/profile/reference/auth JSON.
- Error: revision/config conflicts and staged-write failure leave prior state.
- Security: DTO/Debug/log/config never expose secret; auth file is private.

## Verification commands

| Purpose | Command | Expected on success |
|---|---|---|
| Native auth | `cargo test --test provider_credentials --test provider_config_editor --test provider_phase4_api_account -- --test-threads=1` | exit 0 |
| Backend API | `cargo test --test provider_phase4_api_account --test provider_api_v2 -- --test-threads=1` | exit 0 |
| Frontend | `npm test -- --run src/components/provider-center.test.tsx src/stores/providers-v2.test.ts src/lib/api.test.ts` | exit 0 |
| Build/style | `cargo fmt --all -- --check`, `npm run build`, changed-file ESLint, `git diff --check` | exit 0 |

## Done criteria

- [x] Every task has exactly one checked status and it is `验证成功`
- [x] Every task-specific Done criteria checklist is complete
- [x] No STOP condition remains unresolved

## STOP conditions

- Current Codex rejects the documented `auth.json` plus
  `requires_openai_auth` contract against a custom Responses base URL.
- Existing secrets cannot be migrated without exposing them to logs/arguments.
- Safe rollback would require overwriting user-managed config fields.
- Five implementation/verification repair loops fail for one task.
