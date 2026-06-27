# Plan 006: Prevent Codex restart and quit from leaving orphan helper processes

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` unless a reviewer says they maintain the index.
>
> **Drift check (run first)**:
> `git diff --stat 17ef5d1..HEAD -- apps/desktop/src-tauri/src/commands/mod.rs apps/desktop/src/App.tsx apps/desktop/src/components/tray-quota-panel.tsx plans/README.md`
>
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding. A source
> behavior mismatch is a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `17ef5d1`, 2026-06-27
- **Execution status**: DONE on 2026-06-27. The implementation, quota-test
  correction, `.fake-home` fixture restoration, and full verification gates
  passed.

## Why this matters

LAM restarts Codex after account switching so the newly copied `auth.json`
takes effect, and the tray popover also has a Quit action that exits LAM. The
current restart command kills only the main Codex executable path, sleeps for
500 ms, then reopens the app. The current Quit command exits LAM without
stopping Codex at all. Codex also launches helper processes under the app
bundle, such as crashpad handlers and native monitors; those helpers can
survive as PPID=1 orphan processes after repeated restarts or quit cycles.

This plan makes both restart and Quit use the same local stop-Codex routine.
Restart stops Codex, waits for the whole app-bundle process set to exit, then
reopens Codex. Quit stops Codex with the same routine, then exits LAM. It keeps
the change small: one backend command file plus a small regression test for the
process matching rule.

## Current state

Relevant files:

- `apps/desktop/src-tauri/src/commands/mod.rs` — implements the Tauri
  `restart_codex` and `quit_app` commands plus existing macOS window-bounds
  helpers.
- `apps/desktop/src/App.tsx` — calls `api.restartCodex()` after main-window
  account switching.
- `apps/desktop/src/components/tray-quota-panel.tsx` — calls `restartCodex()`
  after tray account switching and invokes `quit_app` from the footer Quit
  button.

Current backend restart implementation:

```rust
// apps/desktop/src-tauri/src/commands/mod.rs:410-437
#[tauri::command]
pub async fn restart_codex() -> Result<(), AppError> {
    run_blocking(|| {
        #[cfg(target_os = "macos")]
        let bounds = codex_window_bounds();

        // Force kill any running Codex process
        let _ = std::process::Command::new("pkill")
            .args(["-f", "/Applications/Codex.app/Contents/MacOS/Codex"])
            .output();

        // Wait briefly for the process to fully terminate
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Reopen Codex
        std::process::Command::new("open")
            .arg("/Applications/Codex.app")
            .spawn()
            .map_err(|e| AppError::new("RESTART_CODEX_FAILED", e.to_string()))?;

        #[cfg(target_os = "macos")]
        if let Some(bounds) = bounds {
            restore_codex_window_bounds(bounds);
        }

        Ok(())
    })
    .await
}
```

Current backend Quit implementation:

```rust
// apps/desktop/src-tauri/src/commands/mod.rs:439-442
#[tauri::command]
pub fn quit_app(app_handle: tauri::AppHandle) {
    app_handle.exit(0);
}
```

Main-window switch caller:

```tsx
// apps/desktop/src/App.tsx:359-368
async function handleSwitchAccount(account: CodexAccount) {
  if (authMode === 'pat') {
    try {
      await api.switchToPatAccount(account.id);
      await refresh();
      refreshAccountQuota('main');
      useAppStore
        .getState()
        .setStatus(`Switched auth.json to '${account.id}'. Restarting Codex…`);
      await api.restartCodex();
```

Tray switch caller:

```tsx
// apps/desktop/src/components/tray-quota-panel.tsx:1058-1068
async function switchTo(account: CodexAccount) {
  setRelayingAccountId(account.id);
  setStatus(`Switching to ${account.displayName}...`);
  try {
    await switchToPatAccount(account.id);
    await load(false);
    await refreshAccountQuota(accounts.find((a) => a.id === 'main') ?? account);
    setStatus(`Switched to ${account.displayName}. Restarting Codex...`);
    await restartCodex();
```

Tray Quit caller:

```tsx
// apps/desktop/src/components/tray-quota-panel.tsx:388-399
function TrayPopoverFooter({ onClose: _onClose, onOpen }: TrayPopoverFooterProps) {
  return (
    <footer className="trayPopoverFoot">
      <div className="trayPopoverActions">
        <UIButton
          size="sm"
          variant="ghost"
          onClick={(e) => {
            e.preventDefault();
            e.stopPropagation();
            void invoke('quit_app');
```

Observed process evidence from read-only inspection on 2026-06-27:

```text
/Applications/Codex.app/Contents/Frameworks/.../browser_crashpad_handler ... PPID=1
/Applications/Codex.app/Contents/Resources/native/bare-modifier-monitor ... PPID=1
```

The current `pkill` pattern only targets:

```text
/Applications/Codex.app/Contents/MacOS/Codex
```

It does not match helper paths under `Contents/Frameworks` or
`Contents/Resources`. The current `quit_app` command does not attempt to stop
Codex processes before exiting LAM.

Repo conventions to preserve:

- Tauri commands in `commands/mod.rs` return `Result<T, AppError>`.
- Blocking OS work is wrapped in `run_blocking`.
- macOS-only process/window helpers are guarded with `#[cfg(target_os = "macos")]`.
- Existing commit style is Conventional Commits, for example
  `feat: restart Codex app after account switch` and
  `fix: enable Login button for all accounts in PAT mode`.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Focused Rust test | `cd apps/desktop/src-tauri && cargo test codex_process -- --test-threads=1` | exit 0; the new Codex process matching test passes |
| Rust format check | `cd apps/desktop/src-tauri && cargo fmt -- --check` | exit 0; no diff |
| Rust lint | `cd apps/desktop/src-tauri && cargo clippy --all-targets -- -D warnings` | exit 0; no warnings |
| Full project gate | `make check` | exit 0; frontend build/UI smoke/Rust checks/tests pass |

## Scope

**In scope**:

- `apps/desktop/src-tauri/src/commands/mod.rs`
- `plans/README.md` — status update only after execution

**Out of scope**:

- Frontend switch flows in `apps/desktop/src/App.tsx` and
  `apps/desktop/src/components/tray-quota-panel.tsx`; they already call
  `restartCodex()` or invoke `quit_app`.
- Account switching, auth-file copying, PAT storage, and quota refresh behavior.
- Adding a process supervisor, persistent PID file, background daemon, or new
  dependency.
- Changing `/Applications/Codex.app` discovery. Keep the current fixed app path
  unless live code has already changed during drift check.
- Killing unrelated `Codex Computer Use.app` processes. Match only the
  `/Applications/Codex.app/Contents/` bundle path.

## Git workflow

- Branch: `codex/006-prevent-codex-process-orphans`
- One logical commit is sufficient.
- Commit message: `fix: stop Codex helpers on restart and quit`
- Do not push or open a PR unless explicitly instructed.

## Steps

### Step 1: Add a bundle-wide Codex process matcher

In `apps/desktop/src-tauri/src/commands/mod.rs`, add a small macOS-only helper
near the existing window helpers:

- `const CODEX_APP_PATH: &str = "/Applications/Codex.app";`
- `const CODEX_BUNDLE_PATH_PREFIX: &str = "/Applications/Codex.app/Contents/";`
- `const CODEX_BUNDLE_PROCESS_PATTERN: &str = "/Applications/Codex[.]app/Contents/";`

Use `CODEX_APP_PATH` for the existing `open` call later. Use the regex-safe
`CODEX_BUNDLE_PROCESS_PATTERN` for `pgrep -f` and `pkill -f`; do not pass the
literal app path with `Codex.app` to `pgrep -f`, because `.` is regex syntax
there. Use `CODEX_BUNDLE_PATH_PREFIX` only for pure Rust string tests. Do not
match the looser string `Codex`, because that can catch unrelated tools.

Add a macOS-only unit test in the existing `#[cfg(all(test, target_os = "macos"))]`
test module proving the pattern covers these sample paths:

- `/Applications/Codex.app/Contents/MacOS/Codex`
- `/Applications/Codex.app/Contents/Frameworks/Codex Framework.framework/Versions/149.0.7827.197/Helpers/browser_crashpad_handler`
- `/Applications/Codex.app/Contents/Resources/native/bare-modifier-monitor`

Also assert it does **not** match:

- `./Codex Computer Use.app/Contents/SharedSupport/SkyComputerUseClient.app/Contents/MacOS/SkyComputerUseClient`
- `/Applications/CodexXapp/Contents/MacOS/Codex`

The test should explicitly assert:

- `CODEX_BUNDLE_PROCESS_PATTERN == "/Applications/Codex[.]app/Contents/"`
- `CODEX_BUNDLE_PROCESS_PATTERN` contains `Codex[.]app`, not `Codex.app`

Do not add a `regex` crate just for this test.

**Verify**:

`cd apps/desktop/src-tauri && cargo test codex_process -- --test-threads=1`

Expected: exit 0 and the new matcher test passes. If the exact test filter does
not discover the test because of its name, rename the test to include
`codex_process`.

### Step 2: Extract one local stop-Codex routine

Still in `apps/desktop/src-tauri/src/commands/mod.rs`, add a small macOS-only
helper such as `stop_codex_app_processes() -> Result<(), AppError>` near the
restart command. Both `restart_codex` and `quit_app` must call this same helper.
Keep it local to this file; do not create a process-management module.

The helper behavior:

1. Ask Codex to quit politely on macOS without launching it if it is not
   already running. Use `System Events` to check `process "Codex"` first, then
   quit by bundle id or app name only inside that branch. For example:

   ```applescript
   tell application "System Events"
     if exists process "Codex" then
       tell application "Codex" to quit
     end if
   end tell
   ```

   Ignore failure, because Codex may not be running or automation permission
   may be unavailable.
2. Wait up to about 2 seconds for no processes matching
   `CODEX_BUNDLE_PROCESS_PATTERN`.
3. If processes still exist, run `pkill -TERM -f CODEX_BUNDLE_PROCESS_PATTERN`.
4. Wait up to about 2 more seconds.
5. If processes still exist, run `pkill -KILL -f CODEX_BUNDLE_PROCESS_PATTERN`.
6. Wait up to 500 ms more, polling every 100 ms. If processes still exist,
   return `Err(AppError::new("STOP_CODEX_FAILED", "..."))`. Restart must not
   open a second Codex instance in this case; Quit must not exit LAM. The error
   message should say Codex processes did not exit.

Keep this boring and local. A small helper such as
`codex_bundle_processes_running()` plus `wait_for_codex_exit(timeout)` is enough.
Do not create a general process-management abstraction.

Implementation details:

- Use `std::process::Command`, `std::thread::sleep`, and `std::time::{Duration, Instant}`.
- `codex_bundle_processes_running()` should run:
  `pgrep -f CODEX_BUNDLE_PROCESS_PATTERN`
  and return `true` only when the command exits successfully.
- `wait_for_codex_exit(timeout)` should poll every 100 ms until
  `codex_bundle_processes_running()` is false or timeout expires.
- Keep all new helpers behind `#[cfg(target_os = "macos")]`.
- Split `restart_codex` internally by platform:
  - `#[cfg(target_os = "macos")]`: capture window bounds, call
    `stop_codex_app_processes()`, open `CODEX_APP_PATH`, then restore bounds.
  - `#[cfg(not(target_os = "macos"))]`: return
    `Err(AppError::new("RESTART_CODEX_UNSUPPORTED", "Codex restart is only supported on macOS"))`.
    Do not call `open`, `pkill`, or `pgrep` on non-macOS.
- Split `quit_app` internally by platform:
  - `#[cfg(target_os = "macos")]`: call `stop_codex_app_processes()` before
    `app_handle.exit(0)`. If stopping Codex returns an error, do not exit LAM;
    return that error so the caller can surface it.
  - `#[cfg(not(target_os = "macos"))]`: keep the existing behavior and call
    `app_handle.exit(0)`.
- Change `quit_app` from `pub fn quit_app(...)` to
  `pub fn quit_app(...) -> Result<(), AppError>` if needed. The existing
  frontend `invoke('quit_app')` does not need to change.

**Verify**:

`cd apps/desktop/src-tauri && cargo test codex_process -- --test-threads=1`

Expected: exit 0; the new test and existing command-module tests pass.

### Step 3: Run full verification and update the plan index

Run:

```bash
(cd apps/desktop/src-tauri && cargo fmt -- --check)
(cd apps/desktop/src-tauri && cargo clippy --all-targets -- -D warnings)
make check
```

Expected:

- `cargo fmt -- --check` exits 0.
- `cargo clippy --all-targets -- -D warnings` exits 0.
- `make check` exits 0.

Then update the Plan 006 row in `plans/README.md` from `TODO` to `DONE`. If a
verification gate fails and cannot be fixed within the in-scope file, do not
edit `plans/README.md`; stop and report the failing command and error instead.

## Test plan

Add one macOS-only unit test in `apps/desktop/src-tauri/src/commands/mod.rs`.
Use the existing `parses_osascript_window_bounds` test module as the structural
pattern.

Cases:

- Matcher includes the main Codex executable path.
- Matcher includes Codex helper paths under `Contents/Frameworks`.
- Matcher includes Codex helper paths under `Contents/Resources`.
- Matcher excludes `Codex Computer Use.app`.
- Matcher excludes a regex false-positive such as
  `/Applications/CodexXapp/Contents/MacOS/Codex`.

No live process-killing test is required. It would be flaky and dangerous in a
unit test. The runtime behavior is covered by code review plus the command
gates above.

## Done criteria

- [ ] `restart_codex` no longer relies on only
      `/Applications/Codex.app/Contents/MacOS/Codex`.
- [ ] `restart_codex` waits for app-bundle processes matching the regex-safe
      `/Applications/Codex[.]app/Contents/` pattern to exit before reopening
      Codex.
- [ ] `quit_app` uses the same stop-Codex routine before exiting LAM on macOS.
- [ ] If polite quit does not clear processes, the shared stop-Codex routine
      escalates to `pkill -TERM`, then `pkill -KILL`.
- [ ] If processes remain after `pkill -KILL` and the final bounded wait,
      restart returns `STOP_CODEX_FAILED` and does not reopen Codex; Quit
      returns the same error and does not exit LAM.
- [ ] The matcher test proves helper process paths are covered and
      `Codex Computer Use.app` plus regex false-positives are excluded.
- [ ] `cd apps/desktop/src-tauri && cargo test codex_process -- --test-threads=1`
      exits 0.
- [ ] `cd apps/desktop/src-tauri && cargo fmt -- --check` exits 0.
- [ ] `cd apps/desktop/src-tauri && cargo clippy --all-targets -- -D warnings`
      exits 0.
- [ ] `make check` exits 0.
- [ ] No files outside the in-scope list are modified, except unavoidable
      formatter-only changes reported explicitly.
- [ ] `plans/README.md` Plan 006 status is updated.

## STOP conditions

Stop and report back instead of improvising if:

- The live `restart_codex` implementation no longer matches the excerpt in
  "Current state".
- Codex is no longer installed at `/Applications/Codex.app` and the code has
  already moved to dynamic app discovery.
- Fixing the issue appears to require frontend flow changes beyond the existing
  `restartCodex()` and `invoke('quit_app')` calls.
- The process matcher would also kill `Codex Computer Use.app` or another
  non-Codex-app tool.
- A verification command fails twice after a reasonable in-scope fix attempt.

## Maintenance notes

- Reviewers should scrutinize the process pattern and the shared stop helper.
  The pattern should be broad enough
  for helpers under `/Applications/Codex.app/Contents/`, but not broad enough
  to kill unrelated Codex-named tools.
- The wait time is intentionally short. If real machines still show residual
  helpers after this lands, increase the bounded wait before adding any larger
  process-management mechanism.
- If Codex later changes install location, handle that in a separate plan; do
  not combine app discovery with this orphan-process fix.
