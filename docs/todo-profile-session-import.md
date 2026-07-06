# Todo: Profile Session Import

> Executor instructions: Follow this todo step by step. Generate tests from the
> "Test design" section before implementation. Run each verification command and
> confirm the expected result before moving to the next task. If a STOP condition
> occurs, stop and report instead of improvising.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: existing account creation and PAT session JSON parsing paths
- **Category**: feature
- **Planned at**: current dirty workspace

## Why this matters

**Background**: PAT imports already store account material under separate `.codex-{id}` directories, but PAT switching still uses the shared main `.codex` auth slot. Users want the same pasted ChatGPT session JSON to create a complete independent profile that behaves like `codex-c` or `codex-luna002`.

**Current state**: Profile mode only creates an empty profile and wrapper, then expects `codex login`. Session JSON paste is only shown in PAT mode and either stores raw uploaded auth or a PAT runtime auth file.

**Impact**: Users who prefer isolated `CODEX_HOME` profiles cannot import an existing session token directly and must manually shape `auth.json` or use PAT shared-space switching.

**What improves**: A pasted ChatGPT session can become a normal LAM-managed Codex profile with its own `auth.json`, `config.toml`, `sessions/`, and wrapper.

## Scope

**In scope**:
- Add a backend request and command for profile session import.
- Convert common ChatGPT session JSON key variants into Codex `auth.json`.
- Create a normal managed profile directory, sessions directory, config, marker, and wrapper.
- Add UI entry in non-PAT New Account flow.
- Add focused Rust and React tests.

**Out of scope**:
- Changing existing PAT mode behavior.
- Migrating sessions from main to the imported profile.
- Validating tokens against external services.
- Supporting non-ChatGPT credential formats beyond existing key aliases.

## Design

Expected input is a profile name plus pasted session JSON object. The backend validates the name, rejects existing profiles, requires at least one usable access/id/refresh token, converts aliases such as `accessToken`, `idToken`, `refreshToken`, `accountId`, `expires`, and nested `user.email`, then writes a Codex-style `auth.json` under the new profile. The profile is otherwise created like a normal LAM-managed account: private directory, `sessions/`, minimal `config.toml`, marker, and executable wrapper. The UI keeps ordinary empty profile creation as the default and exposes a second session import path only outside PAT mode.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Backend session profile import | Rust tests pass for conversion, structure, duplicate, and invalid input | 验证成功 |
| T2 | Frontend profile import UI | React test passes for non-PAT session import submission | 验证成功 |

### T1: Backend session profile import

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The core behavior must be framework-independent and safe before the UI can call it.

**What to do**:
- Add request/result type for session profile import.
- Add service function that creates a normal profile from pasted session JSON.
- Add Tauri command and command registration.
- Preserve existing PAT import and switch behavior.

**Logic design**:
- Validate profile name with existing account validators.
- Reject existing target directory.
- Convert aliases into a nested `tokens` object in `auth.json`.
- Include stable optional fields such as email, expired, last_refresh, type, and websockets.
- Reuse existing private file/directory helpers and wrapper creation.

**Test design**:
- Normal: ChatGPT session JSON creates `.codex-{id}`, `sessions/`, `config.toml`, marker, wrapper, and converted `auth.json`.
- Edge: snake_case keys are accepted.
- Invalid: empty JSON or no token fields is rejected.
- Conflict: existing profile id is rejected.

**Acceptance**:
- `cd apps/desktop/src-tauri && cargo test test_add_session_profile_account --test integration_pat_accounts`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T2: Frontend profile import UI

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Users need a profile-mode path distinct from PAT mode to create the imported account.

**What to do**:
- Add API wrapper and TypeScript type.
- Add a non-PAT New Account session import toggle/form.
- Submit parsed JSON to the new backend command and refresh accounts/sessions.

**Logic design**:
- Keep empty profile creation as the default.
- Only show session import in profile/Auth mode.
- Validate account name and JSON parse errors in the UI.
- Do not call PAT commands for this flow.

**Test design**:
- Normal: in OAuth mode, New Account -> Import Session submits `addSessionProfileAccount` with parsed JSON.
- State: existing normal profile creation still uses `executeCreateAccount`.

**Acceptance**:
- `cd apps/desktop && npm test -- App.handoff.test.tsx`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Test plan

- Rust integration tests for conversion, structure, duplicate profile, and invalid session JSON.
- React interaction test for OAuth/Profile-mode session import.
- Existing PAT tests remain unchanged.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Backend focused tests | `cd apps/desktop/src-tauri && cargo test test_add_session_profile_account --test integration_pat_accounts` | exit 0 after implementation; expected failure before implementation |
| Frontend focused tests | `cd apps/desktop && npm test -- App.handoff.test.tsx` | exit 0 after implementation; expected failure before implementation |
| Relevant backend suite | `cd apps/desktop/src-tauri && cargo test --test integration_pat_accounts` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:

- Current account creation code differs materially from the Current state description.
- Required test infrastructure cannot run locally.
- Codex auth schema cannot be represented without external runtime verification.
- Verification still fails after focused fixes.
