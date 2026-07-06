# Todo: Profile Account Delete

> Executor instructions: Follow this todo step by step. Generate tests from the
> "Test design" section before implementation. Run each verification command and
> confirm the expected result before moving to the next task. If a STOP condition
> occurs, stop and report instead of improvising.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: existing account scan, wrappers, notes, and account cache
- **Category**: feature
- **Planned at**: current dirty workspace

## Why this matters

**Background**: Users can create and rename Codex profiles, but stale or broken profiles must be removed manually from the filesystem. Deleted directories can also remain visible briefly through cached account state.

**Current state**: LAM scans `~/.codex*` directories and caches results. Account cards expose Rename/Login/Switch but no delete action. Backend has rename but no delete command.

**Impact**: Broken imported profiles are hard to clean up safely. Manual deletion can leave wrappers, notes, and cache entries behind.

**What improves**: Profile mode account cards can delete a non-main profile and remove the managed profile directory, wrapper, notes, and refreshed cache in one explicit action.

## Scope

**In scope**:
- Backend delete request/result and command.
- Delete non-main profile directory and wrapper.
- Remove related account notes and refresh accounts cache.
- UI account-card Delete action in profile/Auth mode.
- Focused Rust and React tests.

**Out of scope**:
- PAT active auth slot deletion behavior.
- Trash/recovery workflow.
- Deleting main `~/.codex`.
- Bulk delete.

## Design

The backend owns destructive filesystem behavior. It validates an existing profile id, blocks `main`, removes the profile directory recursively, removes the wrapper if it exists, removes note metadata, then refreshes `accounts-cache.json` through `list_accounts`. The UI shows Delete only in profile/Auth mode, confirms with the user, calls the command, refreshes state, and selects a remaining account.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Backend delete account command | Rust integration test verifies directory, wrapper, note, cache behavior | 验证成功 |
| T2 | Profile card delete action | React test verifies delete button calls API only after confirm | 验证成功 |

### T1: Backend delete account command

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Deleting account files is destructive and must be centralized behind validation.

**What to do**:
- Add `DeleteAccountRequest` and `DeleteAccountResult`.
- Add `delete_account` service and Tauri command.
- Block `main` deletion.
- Delete profile dir, wrapper, account note, and refresh cache.

**Logic design**:
- Use existing `find_account`, `wrapper_path`, note helpers, and cache writer.
- Work with non-managed profiles too if they are discoverable accounts.
- If wrapper is missing, continue.
- If profile does not exist, return `ACCOUNT_NOT_FOUND`.

**Test design**:
- Normal: deleting a managed non-main profile removes directory and wrapper and cache no longer lists it.
- State cleanup: account note for deleted profile is removed.
- Invalid: deleting `main` is rejected.

**Acceptance**:
- `cd apps/desktop/src-tauri && cargo test test_delete_account --test integration_pat_accounts`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T2: Profile card delete action

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The user needs account cleanup available where they manage profiles, without changing PAT-mode behavior.

**What to do**:
- Add API wrapper/type.
- Add Delete button to account card only for profile/Auth mode.
- Confirm before calling delete.
- Refresh account list and select a remaining account.

**Logic design**:
- Disable or hide delete for `main`.
- Do not show the action in PAT mode.
- Use `window.confirm` with the target display name.
- Keep UI side effects explicit: delete, refresh, load sessions for selected fallback.

**Test design**:
- Normal: profile mode Delete confirms and calls `deleteAccount({ profileId })`.
- Cancel: no API call.
- State: Delete is not shown in PAT mode.

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

- Rust integration tests for destructive delete boundaries.
- React tests for account card delete visibility and confirmation behavior.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Backend focused tests | `cd apps/desktop/src-tauri && cargo test test_delete_account --test integration_pat_accounts` | exit 0 after implementation; expected failure before implementation |
| Frontend focused tests | `cd apps/desktop && npm test -- App.handoff.test.tsx` | exit 0 after implementation; expected failure before implementation |
| Relevant backend suite | `cd apps/desktop/src-tauri && cargo test --test integration_pat_accounts` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:

- The account scan/cache behavior differs materially from the Current state description.
- Deleting accounts requires touching unrelated quota/usage storage.
- Verification still fails after focused fixes.
