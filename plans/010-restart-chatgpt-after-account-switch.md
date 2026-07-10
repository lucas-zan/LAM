# Plan 010: Restart ChatGPT after account switching

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report; do not improvise. When done, update the status row for this plan in
> `plans/README.md` unless a reviewer says they maintain the index.
>
> **Drift check (run first)**:
> `git diff --stat 6e1acf3..HEAD -- apps/desktop/src-tauri/src/commands/mod.rs apps/desktop/src-tauri/src/main.rs apps/desktop/src/lib/api.ts apps/desktop/src/App.tsx apps/desktop/src/App.handoff.test.tsx apps/desktop/src/components/tray-quota-panel.tsx apps/desktop/src/components/tray-quota-panel.test.tsx`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding. A mismatch
> is a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: MED
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `6e1acf3`, 2026-07-10
- **Execution**: BLOCKED on 2026-07-10; implementation and focused gates pass,
  but repository-wide baseline gates and live PID acceptance remain incomplete

### Execution result (2026-07-10)

The executor completed all seven in-scope source changes. Reviewer verification
passed the 42 focused frontend tests, frontend production build, full Rust test
suite, Rust format check, stale-name search, scope audit, and diff check.

Plan 010 remains blocked because its hard gates are not all green:

- Full Vitest has one pre-existing failure in unchanged
  `src/routes/handoff.test.tsx:243`.
- UI smoke has two pre-existing failures against unchanged tray sizing and CSS
  sources: `tray popover remains scrollable with footer visible` and
  `tray popover height grows up to four accounts`.
- Clippy is blocked by pre-existing warnings in unchanged
  `src/services/antigravity.rs:236` and `src/services/quota.rs:921`.
- `make check` therefore stops at UI smoke.
- The live ChatGPT PID acceptance check was not run because restarting ChatGPT
  would interrupt the desktop environment hosting the execution session.

Do not change Plan 010 source scope to repair these unrelated baseline issues.
After the repository baseline is green, rerun `make check` and the live PID
acceptance check, then mark this plan DONE.

## Why this matters

PAT switching still stops and opens `/Applications/Codex.app`, but that bundle
no longer exists on the target Mac. The installed Dock item launches
`file:///Applications/ChatGPT.app/`; its main process is `ChatGPT`, while its
bundle identifier remains `com.openai.codex`. As a result, LAM currently does
not stop the running replacement app and reports success after spawning an
`open` command for a missing path. The account switch must restart the same
ChatGPT bundle that the Dock launches so the newly copied `auth.json` is loaded.

## Current state

The relevant files are:

- `apps/desktop/src-tauri/src/commands/mod.rs` - owns the macOS process matcher,
  graceful quit, forced cleanup, window restoration, and restart command.
- `apps/desktop/src-tauri/src/main.rs` - registers the Tauri command.
- `apps/desktop/src/lib/api.ts` - exposes the command to React.
- `apps/desktop/src/App.tsx` - calls the restart after main-window account
  switching.
- `apps/desktop/src/components/tray-quota-panel.tsx` - calls the restart after
  tray PAT switching.
- `apps/desktop/src/App.handoff.test.tsx` and
  `apps/desktop/src/components/tray-quota-panel.test.tsx` - contain the switch
  regression tests and API mocks.

At `apps/desktop/src-tauri/src/commands/mod.rs:74-79`, the process scope is
still tied to the removed bundle:

```rust
const CODEX_APP_PATH: &str = "/Applications/Codex.app";
const CODEX_BUNDLE_PATH_PREFIX: &str = "/Applications/Codex.app/Contents/";
const CODEX_BUNDLE_PROCESS_PATTERN: &str = "/Applications/Codex[.]app/Contents/";
```

At `apps/desktop/src-tauri/src/commands/mod.rs:109-216`, AppleScript looks for
the process and application name `Codex`, and all wait/kill helpers use the old
bundle prefix. At `apps/desktop/src-tauri/src/commands/mod.rs:790-819`,
`restart_codex` calls `spawn()` for the old path, so it only detects failure to
start `/usr/bin/open`; it does not observe a non-zero exit from `open` when the
bundle path is missing.

At `apps/desktop/src/App.tsx:578-598` and
`apps/desktop/src/components/tray-quota-panel.tsx:1249-1258`, both switch paths
call `restartCodex()` and present `Restarting Codex` status text.

Live macOS evidence collected on 2026-07-10:

```text
Dock file URL: file:///Applications/ChatGPT.app/
Dock label: ChatGPT
Bundle identifier: com.openai.codex
Bundle executable: ChatGPT
Main process: /Applications/ChatGPT.app/Contents/MacOS/ChatGPT
```

The bundle identifier is historical metadata. Match the Dock's application
path and the live process name; do not infer the launch target from the bundle
identifier. Internal helpers still contain names such as `Codex Framework`,
but every helper executable is under `/Applications/ChatGPT.app/Contents/`, so
the bundle-path process matcher remains the correct ownership boundary.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Frontend focused tests | `cd apps/desktop && npm test -- App.handoff.test.tsx src/components/tray-quota-panel.test.tsx` | exit 0; both suites pass |
| Frontend build | `cd apps/desktop && npm run build` | exit 0; TypeScript and Vite build pass |
| Rust command tests | `cd apps/desktop/src-tauri && cargo test commands::tests` | exit 0; command tests pass |
| Rust format | `cd apps/desktop/src-tauri && cargo fmt -- --check` | exit 0; no formatting diff |
| Rust lint | `cd apps/desktop/src-tauri && cargo clippy -- -D warnings` | exit 0; no warnings |
| Full gate | `make check` | exit 0 |

## Scope

**In scope** (the only source files to modify):

- `apps/desktop/src-tauri/src/commands/mod.rs`
- `apps/desktop/src-tauri/src/main.rs`
- `apps/desktop/src/lib/api.ts`
- `apps/desktop/src/App.tsx`
- `apps/desktop/src/App.handoff.test.tsx`
- `apps/desktop/src/components/tray-quota-panel.tsx`
- `apps/desktop/src/components/tray-quota-panel.test.tsx`
- `plans/README.md` for status only

**Out of scope**:

- PAT credential copying and `switch_to_pat_account`; its auth mutation order
  already precedes the restart and must remain unchanged.
- Quota refresh behavior, reset credits, usage parsing, and quota cache files.
- Modifying the user's macOS Dock preferences or adding/removing Dock items.
- Dynamic application discovery, bundle-id lookup, fallback to `Codex.app`, or
  support for non-standard install paths.
- Renaming Codex CLI, Codex app-server, profile, session, provider, or quota
  terminology. This plan changes only the desktop application being restarted.
- `quit_app`; it exits LAM and is not part of account switching.

## Git workflow

- Branch: use the operator's current branch unless explicitly told to create a
  new one.
- Commit message: use the repo's current conventional style, for example
  `fix: restart ChatGPT after account switch`.
- Do not push or open a PR unless the operator explicitly requests it.

## Steps

### Step 1: Replace the macOS restart target and process boundary

In `apps/desktop/src-tauri/src/commands/mod.rs`, rename the app-specific
constants and helpers from `codex_*` to `chatgpt_*`. Use exactly these paths:

```rust
const CHATGPT_APP_PATH: &str = "/Applications/ChatGPT.app";
const CHATGPT_BUNDLE_PATH_PREFIX: &str = "/Applications/ChatGPT.app/Contents/";
const CHATGPT_BUNDLE_PROCESS_PATTERN: &str = "/Applications/ChatGPT[.]app/Contents/";
```

Update window capture and restoration AppleScript to look for process
`ChatGPT`. Update graceful quit to send `quit` to application `ChatGPT`.
Retain the existing sequence and timeouts:

1. Capture window bounds when available.
2. Ask ChatGPT to quit.
3. Wait up to two seconds.
4. Send `TERM` only to processes under the ChatGPT bundle path and wait again.
5. Send `KILL` only to the same bundle path if required.
6. Open `/Applications/ChatGPT.app`.
7. Restore the prior window bounds when available.

Rename error codes and messages to `STOP_CHATGPT_FAILED`,
`RESTART_CHATGPT_FAILED`, and `RESTART_CHATGPT_UNSUPPORTED`. Execute `open`
with `.status()` inside the existing blocking task and return
`RESTART_CHATGPT_FAILED` when the command exits non-zero; do not retain the
fire-and-forget `.spawn()` behavior.

Rename the Tauri command from `restart_codex` to `restart_chatgpt`. Do not keep
an alias for the removed command.

Update the macOS unit test to assert the new regex-safe process pattern and
bundle prefix. Its positive examples must cover:

- `/Applications/ChatGPT.app/Contents/MacOS/ChatGPT`
- a helper under `/Applications/ChatGPT.app/Contents/Frameworks/Codex Framework.framework/`
- `/Applications/ChatGPT.app/Contents/Resources/native/bare-modifier-monitor`

Its negative examples must cover ChatGPT Atlas and a similarly named bundle
such as `/Applications/ChatGPTX.app/...`.

**Verify**:
`cd apps/desktop/src-tauri && cargo test commands::tests` -> all command tests
pass and the matcher only accepts processes inside the ChatGPT bundle.

### Step 2: Register and expose the renamed Tauri command

In `apps/desktop/src-tauri/src/main.rs`, replace
`commands::restart_codex` with `commands::restart_chatgpt`.

In `apps/desktop/src/lib/api.ts`, replace `restartCodex()` with
`restartChatgpt()` and invoke `restart_chatgpt`. Preserve the existing
non-Tauri no-op behavior.

**Verify**:
`rg -n "restart_codex|restartCodex" apps/desktop/src apps/desktop/src-tauri/src`
-> no matches.

### Step 3: Switch both UI account flows to ChatGPT

In `apps/desktop/src/App.tsx`, call `api.restartChatgpt()` after the existing
PAT switch, refresh, and `refreshAccountQuota('main')` operations. Change the
status to `Restarting ChatGPT...` using the file's existing punctuation style.
Use the same ChatGPT restart function in the existing non-PAT best-effort
branch because the old Codex desktop bundle no longer exists. Do not change
`login(account)` or error handling.

In `apps/desktop/src/components/tray-quota-panel.tsx`, import and call
`restartChatgpt()` after the existing PAT switch, account reload, and main
quota refresh. Change the tray status to `Restarting ChatGPT...`.

**Verify**:
`rg -n "Restarting Codex|restartCodex" apps/desktop/src` -> no matches.

### Step 4: Lock the switch-to-restart sequence with frontend tests

In `apps/desktop/src/App.handoff.test.tsx`, add `restartChatgpt: vi.fn()` to
the API mock and make it resolve in test setup. Extend the existing PAT switch
test to assert that `restartChatgpt()` is called after
`switchToPatAccount('codex-c')`. Keep the existing quota refresh assertion.

In `apps/desktop/src/components/tray-quota-panel.test.tsx`, rename the mock and
setup from `restartCodex` to `restartChatgpt`. Extend `shows PAT switch actions
in PAT mode` to assert `restartChatgpt()` is called after the switch. Update
the unrelated Stats assertion to confirm it does not call `restartChatgpt()`.

Do not mock or bypass `switchToPatAccount`; the tests must preserve the current
observable switch sequence.

**Verify**:
`cd apps/desktop && npm test -- App.handoff.test.tsx src/components/tray-quota-panel.test.tsx`
-> both suites pass, including main-window and tray restart assertions.

### Step 5: Run the complete verification gate

Run the frontend build, Rust format check, Rust lint, and full project gate.
Do not repair unrelated failures outside the Scope section.

**Verify**:

```bash
cd apps/desktop && npm run build
cd src-tauri && cargo fmt -- --check
cargo clippy -- -D warnings
cd ../../.. && make check
```

Expected: every command exits 0.

### Step 6: Perform the macOS acceptance check

Only after the automated gates pass, run the installed LAM build with ChatGPT
already open and perform one PAT account switch from either the main window or
tray. This acceptance check intentionally restarts ChatGPT; preserve any work
that should not be interrupted before running it.

Before switching, record the main ChatGPT PID:

```bash
pgrep -f '^/Applications/ChatGPT[.]app/Contents/MacOS/ChatGPT$'
```

After switching, run the same command again and inspect the process boundary:

```bash
ps -axo pid=,command= | rg '/Applications/ChatGPT[.]app/Contents/' | head -c 12000
```

Expected: ChatGPT closes and reopens, the main PID changes, all matched app
processes are under `/Applications/ChatGPT.app/Contents/`, and LAM reports no
restart error. The selected PAT account remains active after the restart.

## Test plan

- Rust command test verifies the exact ChatGPT bundle prefix, regex-safe
  process pattern, nested helper coverage, and exclusion of similarly named
  applications.
- Main-window React test verifies PAT switch invokes the ChatGPT restart while
  retaining the main quota refresh.
- Tray React test verifies PAT switch invokes the ChatGPT restart.
- Existing tray Stats test verifies an unrelated action does not restart
  ChatGPT.
- The installed-app acceptance check verifies the OS boundary that unit tests
  cannot exercise safely: actual quit, process cleanup, Dock-equivalent launch,
  and relaunch with a new PID.

## Done criteria

- [ ] `restart_chatgpt` opens `/Applications/ChatGPT.app` and checks the
  `/usr/bin/open` exit status.
- [ ] Graceful quit, wait, TERM, and KILL target only the ChatGPT process or
  processes under `/Applications/ChatGPT.app/Contents/`.
- [ ] Window bounds are captured from and restored to process `ChatGPT`.
- [ ] Main-window and tray PAT switching both invoke `restartChatgpt()` after
  the auth switch.
- [ ] `rg -n "restart_codex|restartCodex|Restarting Codex|/Applications/Codex[.]app" apps/desktop/src apps/desktop/src-tauri/src`
  returns no matches.
- [ ] Focused frontend tests, Rust command tests, frontend build, Rust format,
  Rust lint, and `make check` all exit 0.
- [ ] The macOS acceptance check shows a changed ChatGPT main PID after one PAT
  switch and no process launched from `/Applications/Codex.app`.
- [ ] No source files outside the Scope list are modified.
- [ ] `plans/README.md` marks Plan 010 `DONE` only after all gates pass.

## STOP conditions

Stop and report back instead of improvising if:

- `/Applications/ChatGPT.app` does not exist, its executable is not
  `Contents/MacOS/ChatGPT`, or the Dock no longer points to that path.
- The current switch flow no longer calls `switchToPatAccount` before the
  desktop restart.
- ChatGPT uses processes outside `/Applications/ChatGPT.app/Contents/` that
  must be terminated for account changes to take effect. Do not broaden the
  kill pattern without review.
- The change requires modifying PAT credential files, quota behavior,
  macOS Dock preferences, or `quit_app`.
- An in-scope excerpt has drifted or a verification command fails twice after
  a reasonable in-scope correction.

## Maintenance notes

- The ChatGPT bundle currently retains internal `Codex` names and bundle id
  `com.openai.codex`; these do not justify targeting `/Applications/Codex.app`.
- Reviewers should scrutinize the process matcher. It must include all helpers
  nested in the ChatGPT bundle and exclude ChatGPT Atlas and similarly named
  applications.
- Fixed-path launch intentionally mirrors the current Dock item. Dynamic app
  discovery and alternate install locations are deferred because they are not
  required by the current macOS contract.
