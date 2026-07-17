# Plan 011: Configure the Antigravity port in Settings and remove automatic port discovery

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan in
> `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**:
> `git diff --stat bf59e4e..HEAD -- apps/desktop/src-tauri/src/services/antigravity.rs apps/desktop/src-tauri/src/services/types.rs apps/desktop/src-tauri/src/services/mod.rs apps/desktop/src-tauri/src/commands/mod.rs apps/desktop/src-tauri/src/main.rs apps/desktop/src-tauri/tests/phase1_core.rs apps/desktop/src/lib/api.ts apps/desktop/src/lib/api.test.ts apps/desktop/src/stores/app.ts apps/desktop/src/App.tsx apps/desktop/src/routes/views.tsx apps/desktop/src/App.handoff.test.tsx apps/desktop/src/styles.css plans/011-configure-antigravity-port-in-settings.md plans/README.md`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: none
- **Category**: dx
- **Planned at**: commit `bf59e4e`, 2026-07-17
- **Implementation**: commit `94f5550`, integrated as `f26e0b9` on
  `20260716-gateway-external-provider`
- **Result**: BLOCKED (pre-existing clippy baseline; Plan 011 scoped gates pass)

## Why this matters

LAM currently runs `lsof` during Antigravity quota refresh to discover localhost
listeners. This is slow, depends on an external command at runtime, and can try
unrelated listeners because one Antigravity language-server process may expose
multiple ports. The required behavior is explicit configuration: the operator
finds the port with the documented `lsof` command, saves it in Settings, and all
future quota refreshes query only that port.

The CSRF token is still dynamic and required by the local Antigravity service.
Keep the existing `ps` process scan solely to locate the Antigravity language
server and read its `--csrf_token`; do not turn this plan into static token
storage.

## Current state

- `apps/desktop/src-tauri/src/services/antigravity.rs` owns process discovery,
  port discovery, local quota requests, and the in-memory successful-response
  cache.
- `apps/desktop/src-tauri/src/services/types.rs` incrementally stores backend
  settings in `<LAM_HOME>/.config/agent-workspace/settings.json` through
  `settings_file_path(...)` and `write_file_private(...)`.
- `apps/desktop/src/stores/app.ts` loads backend settings in parallel and exposes
  them through the Zustand app store.
- `apps/desktop/src/routes/views.tsx` renders Settings with existing
  `settingsRowLayout`, `settingsRowInfo`, and `settingsRowControl` components and
  styles. Reuse them; add only one narrowly scoped command-guide style and no
  component abstraction.
- `apps/desktop/src/App.handoff.test.tsx` is the existing integration-style
  Settings regression suite and already covers the Gateway numeric setting.
- `apps/desktop/src-tauri/tests/phase1_core.rs:55-83` is the existing persisted
  numeric-setting test pattern. It already verifies defaulting, range errors,
  and preservation of another `settings.json` key; extend this file instead of
  creating a second settings-test module.
- `apps/desktop/src/lib/api.test.ts` mocks `@tauri-apps/api/core` and is the
  existing contract-test location for exact frontend invoke names and payloads.
- `apps/desktop/src/styles.css:4565-4604` gives Settings rows `min-width: 0` on
  the information column, but provides no wrapping/monospace style for a long
  command guide. Add only the one class needed for this command.

Baseline recorded at commit `bf59e4e` on 2026-07-17:

- `npm run build`, `npm run test:ui`, and `cargo fmt -- --check` pass.
- `make check` then stops at `cargo clippy -- -D warnings`; `cargo test` is not
  reached by that Make target.
- The four existing clippy failures are outside Plan 011 scope:
  `services/account.rs:1938` (`collapsible_else_if`) and
  `services/gateway/supervisor.rs:646,845,849` (`redundant_closure` and two
  `needless_borrows_for_generic_args`).
- Do not repair these files under Plan 011. Step 0 records whether this baseline
  still exists when execution starts, and Step 5 compares final output against
  that snapshot.
- A separate review-time `cargo test` attempt did not reach compilation because
  `static.crates.io` DNS/download failed for an uncached dev dependency. Treat
  dependency availability as an environment prerequisite, not as a repository
  test failure.

Current automatic discovery in
`apps/desktop/src-tauri/src/services/antigravity.rs:394`:

```rust
fn find_listening_ports(pid: u32) -> Result<Vec<u16>, AppError> {
    let output = run_command_with_timeout(
        "lsof",
        &["-Pan", "-p", &pid.to_string(), "-i"],
        LSOF_PORT_SCAN_TIMEOUT,
    )?;
    // Parses every 127.0.0.1:<port> LISTEN entry.
}
```

Current fallback sequence in
`apps/desktop/src-tauri/src/services/antigravity.rs:722`:

```rust
let processes = match find_antigravity_processes() { /* ... */ };
// Try cached port.
// Try ports parsed from --https_server_port/--http_server_port/etc.
// Run lsof and try every discovered listener.
```

Current backend setting convention in
`apps/desktop/src-tauri/src/services/types.rs:250`:

```rust
pub fn gateway_first_response_timeout_seconds(home_root: &Path) -> u64 { /* ... */ }

pub fn set_gateway_first_response_timeout_seconds(home_root: &Path, seconds: u64) -> Result<()> {
    // Validate, merge one key into settings.json, then write_file_private(...).
}
```

Current frontend setting convention in `apps/desktop/src/stores/app.ts:101` and
`apps/desktop/src/routes/views.tsx:1781`:

```typescript
setGatewayFirstResponseTimeoutSeconds: async (seconds) => {
  await api.setGatewayFirstResponseTimeoutSeconds(seconds);
  set({ gatewayFirstResponseTimeoutSeconds: seconds });
}
```

```tsx
<input
  id="gatewayFirstResponseTimeout"
  type="number"
  value={gatewayTimeoutDraft}
  onBlur={commitGatewayTimeout}
/>
```

## Required behavior

1. Persist the optional setting as `antigravityPort` in LAM's existing
   `settings.json`; never write to Antigravity or Codex-owned files. Clearing
   the input removes this key and returns the app to the unconfigured state.
2. Valid configured values are integers `1..=65535`. Missing, malformed, zero,
   and out-of-range values read as unconfigured. The setter rejects invalid
   values with `ANTIGRAVITY_PORT_CONFIG_INVALID`.
3. Add the setting to **Settings > System & Desktop**, under a new
   **Antigravity Integration** section. Render a native numeric input. A blank
   committed value clears the setting; malformed, zero, fractional, or
   out-of-range edits revert to the last persisted value without calling the
   backend.
4. Show this command as the discovery guide in the same setting row:

   ```bash
   lsof -Pan -p "$(pgrep -f '/Antigravity.app/.*/language_server.*--standalone' | head -n 1)" -iTCP -sTCP:LISTEN
   ```

   Tell the user to start Antigravity before running the command. Explain
   concisely that the user should enter the port from the first
   `127.0.0.1:<port> (LISTEN)` row. If Antigravity exposes more than one row and
   refresh remains offline, the user should try the next listed port. Do not run
   this command from LAM.
5. A quota refresh with no configured port returns the existing
   `AntigravityQuotaResponse` shape with `ok: false`, empty data, and an error
   directing the user to **Settings > System & Desktop > Antigravity
   Integration**. It must not run `ps`, `lsof`, or `curl` first.
6. With a configured port, retain `ps` process discovery only for PID,
   standalone preference, and `--csrf_token`. Keep the current HTTPS-first then
   HTTP-on-the-same-port request behavior. Query exactly the configured port,
   at most once per discovered process/token pair during one refresh.
7. Remove all automatic port sources: `lsof`, command timeout code used only by
   `lsof`, process-argument port parsing, failed-explicit-port cooldown state,
   and their now-obsolete tests.
8. Keep the existing single-flight behavior and successful-response cache, but
   make the ordering explicit: read the configured port, discover the current
   process/token list, then attempt the refresh lock. On lock contention, return
   a cached response only when its PID/token pair occurs in that current process
   list and its port equals the current configured port. Otherwise return the
   existing `Antigravity refresh already in progress` response. This ordering is
   required to make the three-part cache identity check possible without stale
   reuse after a process restart or setting change.
9. Main-window and tray refreshes continue calling the unchanged
   `get_antigravity_quota` frontend API. Do not add a second quota path.
10. Saving a valid port affects the next quota refresh immediately; no LAM or
    Antigravity restart is required. Do not trigger a quota refresh from the
    Settings setter itself.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Deterministic install | `cd apps/desktop && npm ci` | exit 0; `package-lock.json` unchanged |
| Frontend regression | `cd apps/desktop && npm run test -- App.handoff.test.tsx` | exit 0; all tests in the file pass |
| Frontend API contract | `cd apps/desktop && npm run test -- src/lib/api.test.ts` | exit 0; exact get/set invoke names and payload pass |
| Frontend build | `cd apps/desktop && npm run build` | exit 0; TypeScript and Vite build pass |
| Rust focused tests | `cd apps/desktop/src-tauri && cargo test antigravity` | exit 0; all Antigravity tests pass |
| Rust formatting | `cd apps/desktop/src-tauri && cargo fmt -- --check` | exit 0; no diff |
| Rust tests | `cd apps/desktop/src-tauri && cargo test` | exit 0; all tests pass |
| Rust lint | `cd apps/desktop/src-tauri && cargo clippy --all-targets -- -D warnings` | exit 0 if baseline was fixed before execution; otherwise only the four recorded out-of-scope failures remain and no in-scope warning appears |
| Repository gate | `make check` | exit 0 if baseline was fixed before execution; otherwise reaches the same clippy-only baseline and introduces no new failure |
| Production bundle | `make build` | exit 0; finalized `apps/desktop/src-tauri/target/release/bundle/macos/LAM.app` exists |

## Scope

**In scope** (the only source/test files to modify):

- `apps/desktop/src-tauri/src/services/antigravity.rs`
- `apps/desktop/src-tauri/src/services/types.rs`
- `apps/desktop/src-tauri/src/services/mod.rs`
- `apps/desktop/src-tauri/src/commands/mod.rs`
- `apps/desktop/src-tauri/src/main.rs`
- `apps/desktop/src-tauri/tests/phase1_core.rs`
- `apps/desktop/src/lib/api.ts`
- `apps/desktop/src/lib/api.test.ts`
- `apps/desktop/src/stores/app.ts`
- `apps/desktop/src/App.tsx`
- `apps/desktop/src/routes/views.tsx`
- `apps/desktop/src/App.handoff.test.tsx`
- `apps/desktop/src/styles.css`
- `plans/011-configure-antigravity-port-in-settings.md` for executor status and
  verification notes only
- `plans/README.md` for the final status update only

**Out of scope**:

- Antigravity quota response parsing, grouping, pricing, or display redesign.
- Replacing `curl` or the CSRF-token `ps` scan.
- Automatically rerunning `lsof`, `pgrep`, or the displayed guide command.
- Persisting the CSRF token or a PID.
- Supporting remote hosts, non-local addresses, or multiple configured ports.
- A new Settings component, a generic settings repository, or a new dependency.
- Installing/replacing `/Applications/LAM.app`; launch the built bundle in place
  for acceptance unless the operator separately authorizes installation.

## Git workflow

- Branch: `codex/011-antigravity-port-setting`
- Use one logical commit with message:
  `feat: configure Antigravity port in settings`
- Do not push or open a PR unless the operator explicitly requests it.

## Steps

### Step 0: Record drift, dependencies, and verification baseline

Run the plan's drift check from the repository root. Then install frontend
dependencies deterministically:

```bash
(cd apps/desktop && npm ci)
git diff --exit-code -- apps/desktop/package-lock.json
```

Expected: both commands exit 0. If `npm ci` cannot use the committed lockfile,
STOP; do not replace it with `npm install` or modify the lockfile under this
plan.

Run and save the pre-implementation baseline:

```bash
(cd apps/desktop && npm run build)
(cd apps/desktop && npm run test:ui)
(cd apps/desktop/src-tauri && cargo fmt -- --check)
(cd apps/desktop/src-tauri && cargo test)
(cd apps/desktop/src-tauri && cargo clippy --all-targets -- -D warnings) > /tmp/lam-plan011-clippy-before.log 2>&1
make check > /tmp/lam-plan011-make-check-before.log 2>&1
```

Expected at `bf59e4e` once dependencies are available: the first four commands
exit 0. The last two may exit nonzero only for the four clippy findings recorded
in **Current state**. Inspect
both logs with:

```bash
rg -n 'error:|warning:|FAILED|failed|UI smoke failed' /tmp/lam-plan011-clippy-before.log /tmp/lam-plan011-make-check-before.log
```

If either full gate is clean, record that and require it to remain clean in Step
5. If it contains any other failure, apply the matching STOP condition before
editing source.

### Step 1: Add the persisted Antigravity port setting

In `apps/desktop/src-tauri/src/services/types.rs`, add exactly these core APIs
(the setter may use an equivalent integer type only if the error contract stays
identical):

```rust
pub fn antigravity_port(home_root: &Path) -> Option<u16>
pub fn set_antigravity_port(home_root: &Path, port: Option<u64>) -> Result<()>
```

Use key `antigravityPort`, reuse
`settings_file_path`, `config_root`, and `write_file_private`, and merge the key
without replacing other settings. `Some(port)` validates and inserts the key;
`None` removes only that key. If existing JSON is malformed or is a valid
non-object value (array/scalar/null), normalize it to an empty object before the
insert/remove operation, matching the existing setter fallback instead of
silently returning success without persisting the requested state.

Extend `apps/desktop/src-tauri/tests/phase1_core.rs`, following
`gateway_first_response_timeout_setting_defaults_validates_and_preserves_settings`,
with focused coverage for:

- missing settings file returns `None`;
- a valid port round-trips and preserves an unrelated existing setting;
- clearing removes only `antigravityPort` and preserves an unrelated setting;
- zero and `65536` are rejected with `ANTIGRAVITY_PORT_CONFIG_INVALID`;
- malformed string, fractional number, zero, and out-of-range JSON values read
  as `None`;
- a valid non-object settings document does not produce a silent successful
  no-op when a port is saved.

Also add a regression that calls `get_live_antigravity_quota` with a temporary
home containing no setting and asserts the exact actionable offline response.
This test must use the existing `env_lock()` pattern, temporarily make `ps`
unavailable through `PATH`, restore `PATH` before asserting, and remain
independent of a running Antigravity process. It must still receive the exact
missing-configuration response, proving the setting check precedes `ps`; because
`curl` is only reachable after a successful process scan, this also proves no
network command runs.

Re-export the getter and setter from `apps/desktop/src-tauri/src/services/mod.rs`
using the existing settings export list.

**Verify**:
`cd apps/desktop/src-tauri && cargo test antigravity_port`
→ exit 0; all new setting and missing-configuration tests pass.

### Step 2: Expose the setting through Tauri and the frontend store

Add `get_antigravity_port` and `set_antigravity_port` commands in
`apps/desktop/src-tauri/src/commands/mod.rs`, then register both in
`apps/desktop/src-tauri/src/main.rs`.

Add matching wrappers in `apps/desktop/src/lib/api.ts`:

- browser fallback for the getter: `null`;
- browser fallback for the setter: no-op;
- Tauri command names remain snake_case.

Extend `apps/desktop/src/lib/api.test.ts` with an `Antigravity settings API`
case. Install a usable `window.__TAURI_INTERNALS__.invoke` bridge for this test,
then assert exact calls:

```typescript
invoke('get_antigravity_port')
invoke('set_antigravity_port', { port: 62891 })
invoke('set_antigravity_port', { port: null })
```

Restore the global bridge after the test so runtime-detection tests cannot leak
state into one another. Add the complementary no-bridge assertion: the getter
resolves `null`, the setter resolves without invoking Tauri, and neither throws.

Extend `apps/desktop/src/stores/app.ts` with `antigravityPort: number | null`, an
async setter accepting `number | null`, and loading through the existing
`Promise.all`. Follow the Gateway timeout error-handling pattern, with error text
`Failed to save Antigravity port`.

Thread the state and setter through `apps/desktop/src/App.tsx` into
`Views.Settings`; do not create context or another store.

**Verify**:
`cd apps/desktop && npm run test -- src/lib/api.test.ts && npm run build`
→ exit 0; invoke contract passes and there are no TypeScript errors.

### Step 3: Add the Settings control and `lsof` guide

In `apps/desktop/src/routes/views.tsx`, add `antigravityPort` and
`setAntigravityPort` to `Settings` props. Under **System & Desktop**, add an
**Antigravity Integration** section using the existing Settings row classes.

Use a local string draft so the input can be blank while editing. On blur or
Enter: trimmed blank calls `setAntigravityPort(null)`; an integer in `1..=65535`
calls `setAntigravityPort(value)`; every other value restores the last saved
value without a backend call. The displayed saved value is blank when the getter
returned `null`. Implement Enter by blurring the input and let the single
`onBlur` path perform the commit; do not call the commit function from both
handlers.

Use a native `<input type="number" min={1} max={65535} step={1}>`. Show the exact
command from **Required behavior** in a selectable `<code>` block. The copy must
state that LAM no longer discovers the port automatically, that Antigravity must
be running before the command is used, that the new value applies on the next
refresh, and that the setting may need updating after Antigravity restarts.

In `apps/desktop/src/styles.css`, add one narrowly named command-guide class
beside the existing Settings styles. It must use the existing mono font token,
stay within the information column, preserve spaces, wrap long content
(`white-space: pre-wrap` plus `overflow-wrap: anywhere`), and allow text
selection. Do not change global `<code>`, all Settings rows, or unrelated
responsive rules.

Do not add a copy button, shell execution button, tooltip framework, or general
command component.

Extend `apps/desktop/src/App.handoff.test.tsx` mocks and add one regression test
that proves:

- the backend value loads into the input;
- changing and blurring calls `setAntigravityPort` with the numeric value;
- clearing and blurring calls `setAntigravityPort(null)`;
- the `lsof` guide is visible;
- the full rendered command text exactly matches **Required behavior**;
- invalid input does not call the setter;
- pressing Enter and then blurring produces only one setter call.

Add `getAntigravityPort`/`setAntigravityPort` default mocks and reset
`antigravityPort: null` in the suite's shared `beforeEach`. Without that reset,
Zustand state from the new Settings test can leak into later tests.

**Verify**:
`cd apps/desktop && npm run test -- App.handoff.test.tsx`
→ exit 0; the new Settings regression and all existing tests pass.

### Step 4: Replace automatic discovery with the configured port

Change `get_live_antigravity_quota` in
`apps/desktop/src-tauri/src/services/antigravity.rs` to receive `&Path` (or the
minimum equivalent needed to read LAM settings). Update
`commands::get_antigravity_quota` to resolve `home_root()` before entering
`run_blocking` and move the owned path into the closure.

Read `antigravityPort` before acquiring the refresh lock or scanning processes.
When absent, clear any old in-memory Antigravity cache and return an offline
`AntigravityQuotaResponse` with the Settings-path instruction from **Required
behavior**.

For a configured port:

- call `find_antigravity_processes` once before `try_lock`, preserving standalone
  process ordering;
- on `WouldBlock`, return the cached response only if one current process matches
  cache PID and CSRF token and cache port equals `configured_port`; otherwise
  return the existing in-progress response;
- after acquiring the lock, iterate the already-discovered processes once and
  call `query_ports_for_quota(&[configured_port], ...)` once per process;
- store the successful response with that process PID/token/configured-port key;
- clear the old cache if all current process/token attempts fail;
- return an offline error that names the configured port and tells the user to
  rerun the Settings `lsof` guide; do not say `Failed to query all ports` because
  only one port is configured.

Do not first query a cached candidate and then query the same configured port
again in the process loop. The configured port gets one attempt per process in
one refresh.

Delete the complete automatic discovery surface and dead imports:

- `LSOF_PORT_SCAN_TIMEOUT`;
- `FAILED_EXPLICIT_PORT_COOLDOWN`, `FAILED_EXPLICIT_PORTS`, and
  `FailedExplicitPort`;
- the `ports` field on `AntigravityProcess`;
- `extract_antigravity_ports`, its collection helper if now unused, and tests;
- `run_command_with_timeout` and its tests;
- `find_listening_ports`;
- explicit-port failure filtering/marking helpers and tests;
- all production invocations of `lsof`.

Update retained cache tests to cover all cache-key dimensions: same
PID/token/port returns the cached response; wrong PID, wrong token, or wrong
configured port returns the in-progress response. Do not add a helper whose only
purpose is wrapping one port in a slice, and do not make a real network request
in unit tests.

Do not remove `query_ports_for_quota`; it remains useful for the one-element
configured-port slice and avoids rewriting quota response composition.

**Verify**:

```bash
cd apps/desktop/src-tauri
cargo test antigravity
rg -n 'Command::new\("lsof"\)|run_command_with_timeout|find_listening_ports|extract_antigravity_ports|collect_arg_ports|FAILED_EXPLICIT_PORT|PORT_SCAN_|NO_PORTS_FOUND|Failed to query all ports|https_server_port|http_server_port|extension_server_port' src
```

Expected: tests exit 0; `rg` exits 1 with no matches.

### Step 5: Run final gates and inspect scope

Run every command from the repository root. Do not paste these into a shell that
retains a changed working directory; each command uses a subshell so the block
is executable line by line or as a whole. Run the clippy and `make check` lines
as separate commands because the recorded baseline may make either return
nonzero; continue with the remaining independent gates after saving their logs.

```bash
(cd apps/desktop/src-tauri && cargo fmt)
(cd apps/desktop/src-tauri && cargo fmt -- --check)
(cd apps/desktop && npm run test -- App.handoff.test.tsx src/lib/api.test.ts)
(cd apps/desktop && npm run build)
(cd apps/desktop && npm run test:ui)
(cd apps/desktop/src-tauri && cargo test)
(cd apps/desktop/src-tauri && cargo clippy --all-targets -- -D warnings) > /tmp/lam-plan011-clippy-after.log 2>&1
make check > /tmp/lam-plan011-make-check-after.log 2>&1
make build
/usr/bin/codesign --verify --deep --strict apps/desktop/src-tauri/target/release/bundle/macos/LAM.app
git diff --check
git status --short
```

Compare baseline-sensitive logs:

```bash
rg -n 'error:|warning:|FAILED|failed|UI smoke failed' /tmp/lam-plan011-clippy-before.log /tmp/lam-plan011-make-check-before.log /tmp/lam-plan011-clippy-after.log /tmp/lam-plan011-make-check-after.log
```

Expected: all focused commands, frontend build/UI smoke, full Rust tests,
production build, codesign, and diff checks exit 0. If the preflight full gates
were clean, both final full gates must exit 0. If the preflight contained only
the recorded out-of-scope clippy failures, final output may contain only those
same failures at the same paths; there must be no new warning/error
in an in-scope file. The finalized `.app` must exist and pass strict codesign
verification. `git status --short` lists only the in-scope files; ignored build
artifacts do not appear.

After manual acceptance, mark Plan 011 `DONE` only if both full gates are clean.
If the baseline clippy failures remain, record the completed scoped
verification in this plan but set the index status to
`BLOCKED (pre-existing clippy baseline; Plan 011 scoped gates pass)`.

### Execution record (2026-07-17)

- Implemented as `94f5550` in the isolated worktree, then integrated into the
  primary worktree as `f26e0b9` on `20260716-gateway-external-provider`.
- Reviewer reran the full Rust suite, the 49 focused frontend tests, frontend
  production build, Rust formatting, static discovery-removal scan,
  `git diff --check`, `make build`, and strict codesign verification; all passed.
- `cargo clippy --all-targets -- -D warnings` and therefore `make check` remain
  blocked only by five pre-existing out-of-scope Rust 1.97 lints in
  `services/gateway/server.rs:577`, `services/gateway/supervisor.rs:646,845,849`,
  and `services/session.rs:109`. No in-scope file produced a warning.
- The Settings view was visually checked at `1280x820` and `1100x700` in light
  and dark appearance. A live Antigravity process/quota smoke was not performed;
  keep the plan blocked until that check and the repository clippy baseline are
  resolved or explicitly accepted.

## Test plan

- Rust settings tests in `tests/phase1_core.rs`: missing, valid round-trip,
  unrelated-key preservation, clear, malformed/non-object document, malformed
  value, fractional value, zero, and out-of-range values.
- Rust missing-configuration test: returns the actionable Settings error without
  depending on a running Antigravity process.
- Existing Antigravity tests in `services/antigravity.rs`: retain response
  parsing and single-flight coverage; update cache fixtures to assert PID,
  token, and configured-port identity.
- Frontend API contract in `lib/api.test.ts`: exact get/set Tauri command names
  and `{ port }` payload.
- Frontend integration test in `App.handoff.test.tsx`: load, render guide, save a
  valid value, clear to `null`, reject an invalid value, avoid Enter/blur double
  save, and reset Zustand state between tests.
- Static regression gate: no executable `lsof` path or former discovery helper
  remains under `apps/desktop/src-tauri/src`.
- Manual smoke after automated gates:
  1. Start Antigravity.
  2. Use the existing app's normal **Quit** action if another LAM instance is
     running; do not use `pkill` and do not stop Antigravity.
  3. Launch the built bundle in place with
     `open -n apps/desktop/src-tauri/target/release/bundle/macos/LAM.app`; do not
     overwrite `/Applications/LAM.app` as part of this plan.
  4. At the default `1280x820` window and minimum supported `1100x700` window,
     inspect **Settings > System & Desktop** in light and dark appearance. The
     input and complete command must remain visible, selectable, wrapped inside
     the row, and free of clipping/overlap.
  5. Run the displayed `lsof` command in Terminal.
  6. Enter the first localhost listener port in Settings, navigate away and back,
     and confirm the persisted value reloads without restarting LAM.
  7. Refresh Antigravity quota from Overview and the tray; both should show the
     same live data.
  8. Enter a wrong but valid port and refresh; both surfaces should show offline,
     name the configured port, and direct the user back to the guide without LAM
     changing the configured value.
  9. Restore the working port and confirm both surfaces recover on the next
     refresh.

## Done criteria

- [x] `antigravityPort` persists in LAM's existing private `settings.json` and
      preserves other keys.
- [x] Clearing the field removes only `antigravityPort` and returns quota refresh
      to the actionable unconfigured state.
- [x] Settings loads and saves a valid `1..=65535` port and displays the exact
      `lsof` guide.
- [x] Empty edits clear the setting; invalid edits revert to the persisted value;
      valid edits take effect on the next refresh without restarting either app.
- [x] Missing configuration produces an actionable offline response before any
      process or network command runs.
- [x] Antigravity quota refresh queries exactly the configured port.
- [x] `ps` remains only for process/CSRF-token discovery; CSRF tokens are never
      persisted or shown.
- [x] No production `lsof`, command-line port parser, port-scan timeout, or
      failed-discovered-port state remains.
- [x] Cache reuse is keyed by PID, CSRF token, and configured port.
- [x] One refresh attempts the configured port no more than once per discovered
      process/token pair and clears stale cache after total failure.
- [x] Focused Rust tests, frontend API/UI tests, frontend build/UI smoke, full
      Rust tests, format check, `make build`, strict codesign, and
      `git diff --check` all pass.
- [x] Clippy and `make check` either pass or reproduce only the exact
      pre-implementation out-of-scope clippy baseline, with no Plan 011 warning
      or regression.
- [x] The command guide is visually verified at `1280x820` and `1100x700` in
      light and dark appearance with no clipping, overflow, or overlap.
- [x] No source files outside the in-scope list are modified.
- [ ] `plans/README.md` follows Step 5's status rule after manual smoke: `DONE`
      only when full gates are clean, otherwise the exact documented `BLOCKED`
      status when only the pre-existing clippy baseline remains.

## STOP conditions

Stop and report back instead of improvising if:

- Antigravity no longer exposes `--csrf_token` in the language-server process
  command line; this plan intentionally does not redesign authentication.
- The configured port requires a host other than `127.0.0.1` or more than one
  port must be queried for one successful refresh.
- The first `lsof` listener is not the usable service port on repeated real
  launches; revise the user guidance with measured evidence before shipping.
- The configured port works only when more than one port is queried in sequence,
  or HTTPS/HTTP behavior differs from the current same-port fallback; the single
  setting contract would then be insufficient.
- Adding the setting requires changing the settings-file location or replacing
  the existing incremental JSON merge contract.
- Preflight `npm ci` changes `package-lock.json` or fails against it; do not use
  `npm install` as a workaround.
- Required npm/crates dependencies cannot be fetched and are not already cached;
  report the environment blocker rather than treating it as a code regression.
- Current clippy or `make check` fails before implementation anywhere other than
  the recorded out-of-scope clippy locations; record the exact baseline
  failure and revise this plan before editing source.
- The final clippy or `make check` output adds any failure beyond the recorded
  preflight baseline. Do not expand scope to repair unrelated files; fix an
  in-scope regression or stop and report an out-of-scope blocker.
- Any in-scope excerpt has materially drifted since commit `bf59e4e`.
- A verification command fails twice after one reasonable in-scope correction.
- Completion appears to require an out-of-scope file or a new dependency.

## Maintenance notes

- Antigravity currently starts with an ephemeral port, so the operator may need
  to update this setting after Antigravity restarts. This is an accepted product
  tradeoff of removing automatic discovery, not a reason to reintroduce `lsof`.
- Reviewers should verify that the guide is informational only and no frontend or
  backend path executes it, and that the long command does not alter global code
  or Settings-row styling.
- If a future Antigravity release exposes a stable documented settings/API port,
  replace the guide and manual value then; do not add speculative discovery now.
