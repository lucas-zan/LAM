# Todo: Web Session Profile Import

> Executor instructions: Follow this todo step by step. Generate tests from the
> "Test design" section before implementation. Keep changes scoped to profile
> session import conversion.

## Status

- **Priority**: P0
- **Effort**: S
- **Risk**: MEDIUM
- **Depends on**: none
- **Category**: feature
- **Planned at**: current worktree

## Why this matters

**Background**: Users paste raw ChatGPT Web session JSON into the Profile import
flow. The current backend can read top-level `accessToken`, but it does not
normalize nested Web session fields into a complete Codex-compatible `auth.json`.

**Current state**: `add_session_profile_account` calls
`build_session_profile_auth_json`, which shallowly maps token aliases from the
input JSON. It does not synthesize a compatibility `id_token`, does not extract
`account.id` or `user.id`, and does not map `sessionToken` as refresh/session
token data.

**Impact**: Raw Web session imports may create incomplete profiles that Codex
cannot identify reliably or that miss useful account metadata.

**What improves**: A raw ChatGPT Web session can be imported directly as an
isolated profile and written as a normal Codex `auth.json` without requiring a
separate converter.

## Scope

**In scope**:
- `apps/desktop/src-tauri/src/services/account.rs`
- `apps/desktop/src-tauri/tests/integration_pat_accounts.rs`

**Out of scope**:
- UI redesign or changing modal copy.
- Network token refresh or remote validation.
- PAT mode import changes.

## Design

Inputs are JSON objects from the existing Profile Import Session modal. The
converter should accept:
- Existing Codex token JSON with `id_token/access_token/refresh_token`.
- Raw ChatGPT Web session JSON with `accessToken`, optional `sessionToken`,
  `user`, `account`, and `expires`.

For raw Web session JSON, decode the JWT payload locally without verification to
extract account, user, email, plan, project, organization, and expiration claims.
If `idToken` is absent, generate a local compatibility JWT with a stable header,
payload claims Codex expects, and a local compatibility signature. This token is
for local client compatibility only; authentication still depends on the
`access_token` and any available refresh/session token.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Add Web session normalization | Raw Web session profile import writes complete `auth.json` | 验证成功 |

### T1: Add Web session normalization

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Profile import should support the same raw Web session format users already have
from ChatGPT, without requiring an external conversion website.

**What to do**:
- Add tests for raw Web session import with no `idToken`.
- Generate `auth.json.tokens.id_token` with required claims.
- Preserve explicit `idToken` when present.
- Map `sessionToken` to refresh token when no explicit refresh token exists.

**Logic design**:
- Use small pure helpers for JSON string lookup, JWT payload decode, and
  compatibility JWT creation.
- Keep existing snake_case/camelCase token behavior.
- Return existing `INVALID_SESSION_JSON` errors when no usable token exists.

**Test design**:
- New integration test imports a raw Web session with only `accessToken` and
  `sessionToken`.
- Assert `access_token`, `refresh_token`, `account_id`, `email`, `expired`,
  `chatgpt_plan_type`, and decoded compatibility `id_token` claims.
- Expected pre-fix failure: `id_token` is missing and `account_id` is not read
  from nested `account.id`.

**Acceptance**:
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml test_add_session_profile_account_accepts_raw_web_session_json`
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml test_add_session_profile_account_creates_complete_profile_space`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Existing profile import test passes
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused backend | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml test_add_session_profile_account_accepts_raw_web_session_json` | exit 0 after implementation |
| Regression backend | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml test_add_session_profile_account_creates_complete_profile_space` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:
- Web session conversion requires contacting an external service.
- Existing profile import API does not receive the raw session object.
- Supporting the feature would require broad UI or auth workflow redesign.
