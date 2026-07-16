# Todo: RPG-108 Versioned Keychain Credential Lifecycle

> Executor instructions: Execute each task with tests first. Automated tests
> use a fake backend only; never invoke or require the user's real Keychain.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: HIGH
- **Depends on**: RPG-103, RPG-107
- **Category**: feature/security
- **Planned at**: current uncommitted Phase 0/1 workspace

## Why this matters

**Background**: Credential references can describe Keychain items, and the auth
helper boundary exists, but Provider V2 cannot yet create, rotate, read-use, or
revoke a versioned Keychain credential safely.

**Current state**: Legacy Provider code invokes `/usr/bin/security` with the
plaintext secret in argv and uses a provider-derived account name. Provider V2
has only reference validation; no backend trait or metadata-CAS compensation.

**Impact**: A metadata conflict could delete the active old credential, static
account names cannot distinguish versions, and argv exposes secret material.

**What improves**: Keychain access becomes a fakeable secret boundary; every new
version has an opaque identity, metadata points to it only after a successful
write, conflicts delete only the new version, and views/config contain references
only.

## Scope

**In scope**:

- `KeychainBackend` trait, stable service and versioned opaque account identity.
- Secret write/read-use/delete with structured redacted errors.
- Provider V2 credential-reference CAS integration for bearer/header auth.
- Create/rotate compensation and post-commit old-version cleanup result.
- Direct Codex auth-helper projection for a versioned Keychain reference.
- Fake tests for unavailable/denied/missing backend and leak resistance.

**Out of scope**:

- Keychain UI/Tauri wiring and signed-helper packaging; the production macOS
  backend itself uses Security.framework behind the tested trait.
- Gateway binding token/hash/generation lifecycle (RPG-301).
- UI and Tauri commands.
- Changing the legacy `/usr/bin/security` compatibility path in this issue.

## Design

`KeychainCredentialReference` fixes service to `lam.remote-provider`, stores an
opaque credential ID, a versioned account, and a positive version. The backend
accepts/returns `SecretValue`; Debug, serialization, errors, outcomes and Provider
metadata never contain its value.

Lifecycle order is fixed:

```text
validate provider revision/current reference + non-empty secret
  -> generate new opaque credential ID/account/version
  -> backend.write(new)
  -> ProviderRepository CAS current reference -> new reference
  -> on CAS failure backend.delete(new), old remains authoritative
  -> on success backend.delete(old); cleanup failure is reported as pending
```

Rotation never overwrites an existing Keychain account. Read-use exposes the
secret only through the existing closure boundary. Revoke deletes the exact
versioned reference and is idempotent for an already-missing item. Direct Codex
configuration calls the approved bundled helper with reference metadata only.

## Tasks

### Task overview

| ID  | Task                            | Acceptance summary                                      | Status   |
| --- | ------------------------------- | ------------------------------------------------------- | -------- |
| T1  | Backend/reference boundary      | Fake-backed write/read-use/revoke and redacted failures | 验证成功 |
| T2  | Provider metadata lifecycle     | Create/rotate CAS ordering and exact compensation       | 验证成功 |
| T3  | Codex projection and regression | Reference-only helper projection and all gates green    | 验证成功 |

### T1: Backend and reference boundary

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Tests and callers need a narrow boundary that cannot silently fall back
to plaintext storage or shell argv.

**What to do**: Define reference/backend/lifecycle types, validate service/account/
version, implement read-use and idempotent exact-version revoke.

**Logic design**: Backends return stable error codes for unavailable, denied and
missing items. `SecretValue` remains non-serializable and Debug-redacted. Backend
methods receive structured references, never arbitrary labels assembled by UI.

**Test design**: Fake normal write/read-use/revoke; empty secret; missing item;
unavailable/denied errors; Debug/JSON/error marker scans; no real Keychain call.

**Acceptance**: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_keychain credential_boundary_`

**Done criteria**:

- [x] Tests written first and confirmed red (missing Keychain module).
- [x] Secret values cross only the closure/backend boundary.
- [x] Focused tests pass (3/3) and task status is `验证成功`.

### T2: Provider metadata create/rotate lifecycle

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Keychain and Provider metadata must never disagree in a way that destroys
the credential used by the active revision.

**What to do**: Add ProviderRepository reference replacement CAS and lifecycle
create/rotate orchestration with exact new-version compensation.

**Logic design**: Validate revision/current source before secret write. Rotation
uses version + 1 with a new account. CAS failure deletes only the generated new
account; successful CAS makes the new reference authoritative before deleting
the old. Old cleanup failure is returned as `cleanup_pending` without rolling
metadata back.

**Test design**: Create and rotate; stale revision before write; injected CAS
conflict after write; write failure; cleanup failure; bearer and named-header
source replacement; concurrent old reference mismatch.

**Acceptance**: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_keychain metadata_lifecycle_`

**Done criteria**:

- [x] Tests written first and confirmed red (missing metadata lifecycle contracts).
- [x] Conflict preserves old and removes only new exact version.
- [x] Focused tests pass (4/4) and task status is `验证成功`.

### T3: Direct Codex projection and regression

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: A stored reference is useful for direct Responses only when official
Codex auth config can invoke the trusted helper without exposing the secret.

**What to do**: Map a Keychain reference to structured helper command/args and
run all project gates; update master evidence.

**Logic design**: Helper argv contains service/account/version and opaque approval
metadata only. Config auth remains mutually exclusive with env/static auth.

**Test design**: Official auth table, no secret marker in reference/Debug/JSON/
TOML, invalid helper path/reference rejection, full regression.

**Acceptance**: focused tests plus full Rust/frontend/Phase 0/lint/build/format.

**Done criteria**:

- [x] Projection test written first and confirmed red (missing projection function).
- [x] Full quality gates pass.
- [x] Master/this TODO evidence and task status are `验证成功`.

## Test plan

- Normal: create, read-use, rotate, cleanup, revoke, direct helper projection.
- Edge: repeated revoke, positive monotonic version, opaque account uniqueness.
- Invalid: empty secret, invalid/mismatched reference, stale revision/source.
- Error: unavailable/denied/missing/write/delete/CAS conflict.
- Security: synthetic marker absent from metadata, serialization, Debug, errors,
  config, and command arguments.

## Verification commands

| Purpose          | Command                                                                                 | Expected                     |
| ---------------- | --------------------------------------------------------------------------------------- | ---------------------------- |
| Focused          | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_keychain` | pass                         |
| Rust             | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`                          | pass except approved ignores |
| Frontend/Phase 0 | existing pnpm test/lint/build/gateway gate                                              | pass                         |
| Format           | rustfmt, Prettier, diff check                                                           | pass                         |

## Done criteria

- [x] All tasks are `验证成功` with one checked status each.
- [x] Full regression evidence is recorded.
- [x] No STOP condition remains.

## Validation evidence

- T1 red state: tests failed to compile before `provider_keychain` existed;
  credential boundary tests pass 3/3.
- T2 red state: tests failed for missing metadata lifecycle contracts; lifecycle
  tests pass 4/4.
- T3 red state: projection test failed for missing `codex_auth_for_keychain`; the
  complete RPG-108 focused suite passes 8/8.
- The production macOS backend calls Security.framework directly; automated
  tests use only the fake backend and never access the user's Keychain.
- Full Rust suite passes 220 tests with 2 ignored; frontend passes 132/132;
  Phase 0 gate passes 15/15; lint and production build pass.

## STOP conditions

- Automated tests require the real Keychain or a real secret.
- A secret must enter argv, environment, metadata, DTO, Debug, error or config.
- Compensation cannot identify only the newly created version.
- Work requires RPG-110/UI or changes to the weekly quota popover.
