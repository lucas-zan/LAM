# Implementation Plans

**Base commit:** `6e4471e`  
**Generated:** 2026-06-24  
**Last updated:** 2026-06-28 (Plan 008 completed)

## Plan Index

| # | Title | Status | Effort | Risk | Tests |
|---|-------|--------|--------|------|-------|
| 001 | Personal Access Token Authentication Mode (Base) | TODO | L | M | 9 unit + 2 integration |
| 001-addendum | Account Switching with Backup | TODO | M | M | +3 tests (integrated) |
| 001-patches | Critical 5% Patches | TODO | S | L | +1 test (integrated) |
| 005 | Split PAT runtime auth from quota refresh auth | DONE | M | M | 5 focused regressions |
| 006 | Prevent Codex restart and quit from leaving orphan helper processes | DONE | S | L | 1 focused command test |
| 007 | PAT-mode Codex usage statistics from JSONL into local SQLite | DONE | L | H | `make check` + real-home smoke + deep audit |
| 008 | Usage statistics dashboard reference parity and full-page route | DONE | L | H | dashboard parity + tray smoke + installed-app verification |

## Dependencies

- **001-patches** must be applied to **001** during execution
- **001-addendum** steps (3A-3C) inserted after base Step 3
- All three documents form one complete implementation
- **005** is an independent delta plan against the implemented PAT account flow
  at commit `9d9f9ae`; it does not require re-executing Plans 001-004.
- **006** is an independent bugfix for the Codex restart and Quit commands; it
  can execute after the current worktree is clean enough for the executor to
  avoid unrelated changes.
- **007** is an independent feature plan for PAT-mode usage statistics. It adds
  a LAM-native Rust JSONL ingestion service, local `usage.sqlite3`, incremental
  refresh on the quota cadence, and a PAT-only statistics modal. It does not
  require re-executing Plans 001-006.
- **008** is a follow-up to the Plan 007 implementation. It moves usage
  statistics from a modal into a full bottom-nav page, adds the tray footer
  Stats entry, and expands the dashboard to reference-project parity for all
  aggregate data items except `Usage observed`.

## Plan 008 execution note

Read `plans/008-usage-statistics-dashboard-parity.md` as a self-contained
follow-up plan. It expects a source tree that already contains Plan 007's usage
ingestion and the Quit fix from commit `98ef3f0` or an equivalent merge. The
plan intentionally supersedes Plan 007's first-pass modal and LAM-only dashboard
styling: reuse/adapt the reference `codex-usage-tracker` dashboard frontend
assets directly, keep SQLite under `~/.codex/lam/usage/`, omit only `Usage
observed`, and preserve the rule that Quit quits only LAM while Switch is the
only path that stops/restarts Codex.

**Execution completed 2026-06-28**: executor continued in the existing Plan 007
worktree, preserving commits `319823e`, `43ac142`, and `98ef3f0`, and delivered
Plan 008 on branch `codex/008-usage-dashboard-parity` at commits `d976196` and
`556c116`. The app version was bumped to `0.1.1` in npm, Cargo, lockfile, and
Tauri config metadata. Reviewer reran focused dashboard/tray tests, frontend
build, `npm run test:ui`, `cargo test usage`, `cargo test --lib`, `cargo fmt
-- --check`, `cargo clippy -- -D warnings`, `make check`, ignored
`real_home_usage_smoke`, and `env -u CI npx tauri build`; all passed. The
packaged artifacts were produced at
`apps/desktop/src-tauri/target/release/bundle/macos/LAM.app` and
`apps/desktop/src-tauri/target/release/bundle/dmg/LAM_0.1.1_aarch64.dmg`.

## Plan 007 execution note

Read `plans/007-pat-usage-statistics-jsonl-sqlite.md` as a self-contained
plan. It intentionally borrows semantics and UI shape from
`/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker`, but
keeps the implementation inside LAM's current Tauri/Rust/React framework:
aggregate-only SQLite ingestion under `~/.codex/lam/usage/`, reusable
dashboard helper logic adapted into TypeScript, LAM-owned styling, no raw
transcript persistence, local pricing/aggregate diagnostics, and no MCP/CLI
or standalone dashboard shell in this pass.

**Execution completed 2026-06-28**: executor implemented the in-scope Rust
usage ingestion service, all-history archived incremental refresh,
tray/background scheduler, guarded SQLite refresh, PAT-only stats modal,
Settings reset, local pricing, aggregate diagnostics, TypeScript dashboard
helpers, and usage store on branch `codex/007-pat-usage-statistics` at commit
`43ac142`. Reviewer fixed deep-review findings for pricing fields, mixed-model
thread cost, context-window diagnostics, local-date windows, known non-token
event diagnostics, and date-stable tests. Reviewer reran `npm run build`,
focused Vitest tests, `npm run test:ui`, `cargo test usage`, `cargo test`,
`cargo fmt -- --check`, `cargo clippy -- -D warnings`, `make check`, and the
ignored real-home usage smoke. The real SQLite database was created at
`~/.codex/lam/usage/usage.sqlite3` with integrity check `ok`; no tracker DB was
created directly under `.codex`, `.codex/sessions`, `.codex/logs`, or
`.codex/cache`.

## Plan 006 execution note

Read `plans/006-prevent-codex-restart-orphans.md` as a self-contained plan. It
keeps frontend switch/Quit behavior unchanged and narrows the fix to backend
commands: make `restart_codex` and `quit_app` share one stop-Codex routine,
wait for all `/Applications/Codex.app/Contents/` processes to exit, escalate
only if needed, then either reopen Codex or exit LAM.

**Execution completed 2026-06-27**: executor implemented the in-scope backend
change. The quota test failure was fixed by making the fake Codex script match
the production pretty-printed staged auth JSON, and `.fake-home` fixtures were
restored. Focused quota tests, focused command tests, `cargo fmt -- --check`,
`cargo clippy --all-targets -- -D warnings`, and `make check` all pass.

## Plan 005 execution note

Read `plans/005-split-pat-auth-and-quota-source.md` as a self-contained plan.
It preserves Switch behavior, splits uploaded credentials into `auth-f.json`
and a minimal PAT `auth.json`, and makes realtime quota refresh prefer
`auth-f.json` through an isolated temporary `CODEX_HOME`.

**Execution completed 2026-06-26**: initial isolated execution blocked at Step
5 because `cargo fmt -- --check` needed formatting-only changes in
`apps/desktop/src-tauri/src/services/types.rs` and
`apps/desktop/src-tauri/src/commands/mod.rs`, outside Plan 005 scope. Scope was
then explicitly expanded for those formatting-only changes. Focused PAT tests,
focused quota tests, `cargo fmt -- --check`, `cargo clippy --all-targets -- -D
warnings`, and `make check` all pass.

## Execution Notes

**Executor must read all three files**:
1. `plans/001-personal-access-token-auth.md` — Base plan (Steps 1-8)
2. `plans/001-addendum-account-switching.md` — Insert Steps 3A-3C after Step 3
3. `plans/001-patches-critical-5-percent.md` — Apply patches to fix critical gaps

**Critical patches address**:
- ✅ Patch 1: **Actually write auth.json files** (was missing!) 
- ✅ Patch 2: Fix test count inconsistencies (9 tests, not 5 or 8)
- ✅ Patch 3: Clarify Bearer token mechanism (Codex auto-handles it)
- ✅ Patch 4: Add atomic rename for switch safety
- ✅ Patch 5: Backup cleanup guidance

## Implementation Order

```
1. Read all three plan documents
2. Execute base Steps 1-3 (structures, metadata, credential processing)
3. Apply Patch 1: Add write_account_auth_json() function
4. Execute addendum Steps 3A-3C (account switching)
5. Apply Patch 4: Enhance switch_account with atomic rename
6. Continue base Steps 4-8 (detection, commands, tests, docs)
7. Apply Patch 2: Update test counts in verification commands
8. Verify: 9 unit tests + 2 integration tests all pass
```

## Feature Summary

**Complete implementation delivers**:

| Feature | Plan Section | Critical? |
|---------|--------------|-----------|
| PAT credential upload | Base Step 3 | ✅ Critical |
| **Write auth.json file** | **Patch 1** | ✅ **Critical** (was missing) |
| Token expiration tracking | Base Step 3 | ✅ Critical |
| Auth mode detection | Base Step 4 | ✅ Critical |
| **Account switching** | **Addendum 3A-3C** | ✅ **Critical** |
| Timestamped backup | Addendum 3A | ✅ Critical |
| **Atomic switch safety** | **Patch 4** | ⚠️ Important |
| Metadata persistence | Base Step 2-3 | ✅ Critical |
| Tauri commands | Base Step 5 | ✅ Critical |
| Bearer token clarity | Patch 3 | 📘 Documentation |
| Backup cleanup guide | Patch 5 | 🔸 Nice-to-have |

## Test Coverage (Final)

**Unit tests (9 total)**:
1. Metadata record/read cycle
2. Valid credential processing
3. Invalid expiration handling  
4. Expiration status (not expired)
5. Expiration status (expired)
6. Account switching with backup
7. Account switching without existing auth
8. Account switching with invalid source
9. **Write auth.json from credentials** ← Patch 1

**Integration tests (2 total)**:
1. PAT end-to-end workflow
2. Account switching integration

**All tests verify**:
- Backward compatibility (OAuth accounts unaffected)
- File permissions (0600 for auth files)
- Error handling (invalid inputs rejected)
- Atomic operations (switch can't corrupt state)

## Architecture Summary

```
User uploads PAT credentials via Lam UI
  ↓
Backend: upload_pat_credentials(profile_id, credentials)
  ↓
1. Validate credentials (ISO 8601 date, non-empty token)
2. Record metadata: ~/.config/agent-workspace/auth-metadata/a.json
3. Write auth.json: ~/.codex-a/auth.json (0600 permissions) ← Patch 1
  ↓
User clicks "Switch to this account"
  ↓
Backend: switch_to_account(profile_id)
  ↓
1. Validate source exists and has auth.json
2. Backup target: ~/.codex/.auth-backups/auth.json.YYYYMMDD-HHMMSS.bak
3. Atomic copy: temp → rename → target ← Patch 4
4. Set 0600 permissions
  ↓
Codex CLI uses new account automatically
  ↓
Codex reads ~/.codex/auth.json
  ↓
Uses personal_access_token as Bearer token ← Patch 3 clarification
```

## Safety Guarantees

✅ **No data loss**: Automatic backup before every switch  
✅ **Atomic operations**: Switch either succeeds completely or fails safely  
✅ **Isolated metadata**: Lam config separate from Codex files  
✅ **Permission security**: 0600 on all auth files  
✅ **Backward compatibility**: OAuth accounts continue working  
✅ **Error recovery**: Clear instructions for manual recovery  

## Known Limitations (By Design)

1. **Frontend UI deferred** — Backend complete, UI in follow-up plan
2. **No automatic backup cleanup** — Manual cleanup documented (Patch 5)
3. **No token refresh** — Users must re-upload when expired
4. **Main account target only** — Switches always copy to `~/.codex/`
5. **Manual Codex version check** — Assumes Codex v1.x+ supports PAT

## Quality Assessment

**Plan completeness**: **100%** ✅

**Before patches**: 95% (missing auth.json write)  
**After patches**: 100% (all critical gaps filled)

**Ready for execution**: ✅ Yes

All requirements from original request satisfied:
1. ✅ 新的切换账号模式
2. ✅ 复制 auth.json 到 ~/.codex
3. ✅ personal_access_token 支持
4. ✅ 失效时间追踪
5. ✅ JSON 格式转换
6. ✅ 日期备份

## Considered and Rejected

None — first planning session.

---

**Executor**: Start with base plan, apply patches inline as indicated, then integrate addendum steps. All verification commands updated in patches document.
