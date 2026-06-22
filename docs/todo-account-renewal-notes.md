# Todo: Account Renewal Notes

> Executor instructions: Follow this todo step by step. Generate tests from
> the "Test design" section before implementation. Run each verification command
> and confirm the expected result before moving to the next task. If a STOP
> condition occurs, stop and report instead of improvising.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: existing account scan and overview card UI
- **Category**: feature
- **Planned at**: current workspace

## Why this matters

**Background**: Users operate many Codex accounts and need lightweight reminders such as renewal dates and notes.

**Current state**: Accounts are discovered from `~/.codex*` homes and returned by `list_accounts`. The app caches accounts under `~/.config/agent-workspace`, but account-specific user metadata does not exist.

**Impact**: Renewal reminders live outside the app and are easy to forget when scanning accounts.

**What improves**: Each account can carry local, app-owned metadata without introducing a database or writing into Codex auth/config/sqlite files.

## Scope

**In scope**:
- Add local JSON persistence for account renewal date and note.
- Include metadata in `CodexAccount` from scanned and cached account lists.
- Add Tauri/API/store wiring to update metadata.
- Show and edit metadata from overview account cards.

**Out of scope**:
- Notifications, calendar integration, cloud sync, recurring billing rules, and database adoption.
- Editing provider/auth configuration.

## Design

Metadata is stored in an app-owned JSON file at `config_root(home_root)/account-notes.json`, keyed by stable profile id. `list_accounts` reads this file and overlays values onto scanned accounts before cache write. `update_account_note` validates that the account exists, trims and size-limits strings, writes the JSON, refreshes account cache, and returns the updated account. Empty values become `null`.

Inputs are `profileId`, optional `renewalDate` in `YYYY-MM-DD`, and optional `note`. Outputs are the updated `CodexAccount`. Invalid profile ids, malformed dates, and oversized notes return explicit errors.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Persist account notes in core | Rust tests cover read/write, validation, cache overlay | 验证成功 |
| T2 | Wire API/store and UI editor | Frontend tests cover display and edit call | 验证成功 |

### T1: Persist account notes in core

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
User-owned notes must be durable and separate from Codex profile internals.

**What to do**:
- Extend `CodexAccount` with `renewalDate` and `note`.
- Add `AccountNoteUpdate` and `update_account_note`.
- Store metadata in app config JSON.
- Keep cache reads consistent after metadata updates.

**Logic design**:
- Read metadata with missing file as empty map.
- Validate date with `chrono::NaiveDate`.
- Limit notes to 500 chars after trimming.
- Verify profile exists through normal account discovery.

**Test design**:
- Core test saves metadata and sees it in `list_accounts`.
- Core test rejects unknown account, invalid date, and too-long note.
- Core test confirms cached accounts include metadata after update.

**Acceptance**:
- Focused command: `cargo test accounts_include_renewal_notes update_account_note_validates_input --manifest-path apps/desktop/src-tauri/Cargo.toml`
- Relevant suite: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T2: Wire API/store and UI editor

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Metadata is useful only if users can see and change it from the account workflow.

**What to do**:
- Add TypeScript fields and API wrapper.
- Add store action to save note and update local account state.
- Add account card metadata display and edit controls.

**Logic design**:
- Keep UI local to account cards and use existing `UIButton` style.
- Stop card click propagation from form controls.
- Save through the store and surface API errors through app error state.

**Test design**:
- Store test verifies `saveAccountNote` calls API and updates the matching account.
- Route test verifies renewal date/note render and edit save callback is invoked.

**Acceptance**:
- Focused command: `npm test -- --run src/stores/accounts.test.ts src/routes/handoff.test.tsx`
- Relevant suite: `npm test`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Test plan

- Normal behavior: save date and note, return/display updated account.
- Edge case: empty strings clear stored metadata.
- Invalid input: malformed date and excessive note length fail.
- Error behavior: unknown profile id fails.
- State/conflict: updating one account does not modify others and cache reflects saved metadata.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Rust focused tests | `cargo test accounts_include_renewal_notes update_account_note_validates_input --manifest-path apps/desktop/src-tauri/Cargo.toml` | exit 0 after implementation; expected failure before implementation |
| Rust suite | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` | exit 0 |
| Frontend focused tests | `npm test -- --run src/stores/accounts.test.ts src/routes/handoff.test.tsx` from `apps/desktop` | exit 0 after implementation; expected failure before implementation |
| Frontend suite | `npm test` from `apps/desktop` | exit 0 |

## Done criteria

- [x] All tasks are `验证成功`.
- [x] The app does not introduce SQLite or another database.
- [x] Account notes are persisted in app-owned configuration.
