# Plan 008: Move Usage Statistics to a Reference-Parity Full Page

> **Executor instructions**: Follow this plan step by step. Run each
> verification command before moving to the next step. If a STOP condition
> occurs, stop and report instead of improvising. When done, update the Plan 008
> row in `plans/README.md` and append exact verification results to this file's
> Status Log.
>
> **Drift check (run first)**:
>
> ```bash
> git diff --stat 98ef3f0..HEAD -- apps/desktop/src apps/desktop/src-tauri plans/008-usage-statistics-dashboard-parity.md plans/README.md
> ```
>
> If any in-scope file changed since `98ef3f0`, compare the Current State
> excerpts below with live code before proceeding. If the usage service or modal
> implementation is missing, treat that as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: Plan 007 implementation plus Quit fix at commit `98ef3f0`
- **Category**: product / tech-debt / tests
- **Planned at**: commit `98ef3f0`, 2026-06-28
- **Status**: DONE

## Why This Matters

Plan 007 proved the local JSONL to SQLite pipeline, but the UI shipped as a
small modal with a narrow data surface. The user needs the LAM statistics
experience to match the reference `codex-usage-tracker` dashboard much more
closely, reuse the reference dashboard code/style where possible, and expose it
as a real page. The only reference data item that may be omitted is the
`Usage observed` card/reconciliation UI; all other aggregate dashboard data,
pricing, diagnostics, filters, and table fields are in scope.

This plan also fixes the entry points: no Stats button beside the PAT toggle,
Usage appears beside Overview in the bottom navigation, and the tray footer has
`Quit | Stats | Open`, where `Stats` opens the Usage page without touching
Codex. This pass must also bump the desktop app version from `0.1.0` to
`0.1.1`.

## Current State

Use the implementation worktree or a merged tree containing these commits:

```text
98ef3f0 fix: keep Codex running when quitting LAM
43ac142 feat: expand PAT usage statistics
319823e feat: add PAT usage statistics
600ecc7 feat: prevent Codex process orphans on restart and account switch
```

The main repo at the time this plan was reviewed is still `600ecc7`; Plan 008
must execute only after Plan 007 and `98ef3f0` are present.

Relevant current excerpts from the Plan 007 tree:

`apps/desktop/src/routes/types.ts`

```ts
export type Route = 'overview' | 'sessions' | 'providers' | 'sync' | 'settings';

export const routes: Array<{ id: Route; label: string; icon: NavIconName }> = [
  { id: 'overview', label: 'Overview', icon: 'overview' },
  { id: 'sessions', label: 'Sessions', icon: 'sessions' },
  { id: 'providers', label: 'Providers', icon: 'providers' },
  { id: 'sync', label: 'Sync', icon: 'sync' },
  { id: 'settings', label: 'Settings', icon: 'settings' },
];
```

`apps/desktop/src/App.tsx`

```tsx
{authMode === 'pat' ? (
  <UIButton size="sm" className="toolbarBtn" onClick={() => openModal('usageStats')}>
    Stats
  </UIButton>
) : null}
```

```tsx
{modal === 'usageStats' ? (
  <Shell.Modal title="Codex Usage" wide close={closeModal}>
    ...
  </Shell.Modal>
) : null}
```

`apps/desktop/src/components/tray-quota-panel.tsx`

```tsx
<footer className="trayPopoverFoot">
  <div className="trayPopoverActions">
    <UIButton ...>
      <IconClose size={13} />
      Quit
    </UIButton>
    <span />
    <UIButton ...>
      <IconExternalLink size={13} />
      Open
    </UIButton>
  </div>
</footer>
```

`apps/desktop/src-tauri/src/commands/mod.rs`

```rust
#[tauri::command]
pub fn show_main_window(app: tauri::AppHandle) -> Result<(), AppError> {
    crate::tray::show_main_window(&app);
    Ok(())
}
```

```rust
#[tauri::command]
pub async fn quit_app(app_handle: tauri::AppHandle) -> Result<(), AppError> {
    app_handle.exit(0);
    Ok(())
}
```

`apps/desktop/src-tauri/src/services/usage.rs`

```rust
pub fn usage_db_path(home_root: &Path) -> PathBuf {
    home_root.join(".codex/lam/usage/usage.sqlite3")
}
```

Current tests to extend:

- `apps/desktop/src/App.handoff.test.tsx`
- `apps/desktop/src/components/tray-quota-panel.test.tsx`
- `apps/desktop/src/lib/usage-dashboard.test.ts`
- Rust tests already live in `apps/desktop/src-tauri/src/services/usage.rs`

Repo command conventions:

- Frontend commands run from `apps/desktop`.
- Rust commands run from `apps/desktop/src-tauri`.
- Full repo check runs from repo root with `make check`.
- There is no root `package.json`.

## Reference Dashboard Parity Target

Reference files:

- `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker/src/codex_usage_tracker/plugin_data/dashboard/dashboard_template.html`
- `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker/src/codex_usage_tracker/plugin_data/dashboard/dashboard*.css`
- `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker/src/codex_usage_tracker/plugin_data/dashboard/dashboard*.js`
- `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker/src/codex_usage_tracker/schema.py`
- `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker/src/codex_usage_tracker/store_schema.py`
- `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker/src/codex_usage_tracker/parser.py`
- `/Users/micro/Documents/Code/Rust/GitHub_Project/codex-usage-tracker/src/codex_usage_tracker/pricing*.py`

Port these dashboard items:

- Header controls: Refresh, live status, load limit, active/all-history scope.
- Status chips: pricing source, privacy mode, parser diagnostics.
- Filters: search, model, reasoning effort, pricing confidence, time preset,
  custom start/end, sort.
- Summary cards: Visible Calls, Total Tokens, Cached Input, Uncached Input,
  Reasoning Output, Estimated Cost, Codex Credits.
- Views: Insights, Calls, Threads, Diagnostics.
- Insights: needs-attention cards and investigation presets.
- Calls table columns: Time, Thread, Duration, Prev gap, Initiated, Model,
  Effort, Tokens, Cached, Uncached, Output, Reasoning Output, Cost, Cache.
- Details panel: aggregate call fields only, no raw transcript text.
- Pager/load more.
- Diagnostics panels: parser diagnostics, pricing confidence, unknown models,
  credit coverage, source file refresh state, aggregate diagnostic facts.

Omit only:

- The `Usage observed` card and allowance reconciliation UI. If reference
  `rate_limit_*` columns are needed for diagnostics, keep them as aggregate
  fields, but do not show the `Usage observed` card.

Required aggregate `usage_events` fields to support parity:

```text
record_id, session_id, thread_name, session_updated_at, event_timestamp,
source_file, line_number, turn_id, turn_timestamp, cwd, model, effort,
current_date, timezone, call_initiator, call_initiator_reason,
call_initiator_confidence, is_archived, thread_key, thread_call_index,
previous_record_id, next_record_id, thread_source, subagent_type, agent_role,
agent_nickname, parent_session_id, parent_thread_name,
parent_session_updated_at, model_context_window, input_tokens,
cached_input_tokens, output_tokens, reasoning_output_tokens, total_tokens,
cumulative_input_tokens, cumulative_cached_input_tokens,
cumulative_output_tokens, cumulative_reasoning_output_tokens,
cumulative_total_tokens, rate_limit_plan_type, rate_limit_limit_id,
rate_limit_primary_used_percent, rate_limit_primary_window_minutes,
rate_limit_primary_resets_at, rate_limit_secondary_used_percent,
rate_limit_secondary_window_minutes, rate_limit_secondary_resets_at,
uncached_input_tokens, cache_ratio, reasoning_output_ratio,
context_window_percent
```

Required `thread_summaries` fields:

```text
thread_key, is_archived_scope, thread_label, first_event_timestamp,
latest_event_timestamp, call_count, session_count, input_tokens,
cached_input_tokens, uncached_input_tokens, output_tokens,
reasoning_output_tokens, total_tokens, estimated_cost_usd, usage_credits,
avg_cache_ratio, max_context_window_percent, max_recommendation_score,
primary_recommendation, call_initiator_summary, archived_call_count, updated_at
```

Required aggregate diagnostic fact fields:

```text
record_id, fact_type, fact_name, fact_category, event_count, confidence,
first_event_timestamp, last_event_timestamp, first_source_line,
last_source_line, evidence_scope, raw_content_included
```

## Commands You Will Need

| Purpose | Command | Expected on success |
|---|---|---|
| Frontend build | `cd apps/desktop && npm run build` | exit 0 |
| Frontend unit tests | `cd apps/desktop && npm test -- --run src/App.handoff.test.tsx src/components/tray-quota-panel.test.tsx src/lib/usage-dashboard.test.ts` | exit 0, selected tests pass |
| UI smoke | `cd apps/desktop && npm run test:ui` | exit 0 |
| Rust usage tests | `cd apps/desktop/src-tauri && cargo test usage` | exit 0 |
| Rust lib tests | `cd apps/desktop/src-tauri && cargo test --lib` | exit 0 |
| Rust fmt | `cd apps/desktop/src-tauri && cargo fmt -- --check` | exit 0 |
| Rust clippy | `cd apps/desktop/src-tauri && cargo clippy -- -D warnings` | exit 0 |
| Full check | `make check` | exit 0 |
| Installed app build | `cd apps/desktop && env -u CI npx tauri build 2>&1 | tee /tmp/tauri-build.log` | exit 0 and no build error in log |

## Scope

In scope:

- `apps/desktop/src/routes/types.ts`
- `apps/desktop/src/components/icons.tsx`
- `apps/desktop/src/components/shell.tsx` only if the nav component needs a
  small type/render update.
- `apps/desktop/src/App.tsx`
- `apps/desktop/src/routes/*` only for a new Usage page/component if that keeps
  `App.tsx` smaller.
- `apps/desktop/src/lib/api.ts`
- `apps/desktop/src/lib/types.ts`
- `apps/desktop/src/lib/usage-*.ts`
- `apps/desktop/src/stores/usage.ts`
- `apps/desktop/src/components/tray-quota-panel.tsx`
- `apps/desktop/src/styles.css` or a new imported usage dashboard CSS file.
- `apps/desktop/package.json`
- `apps/desktop/package-lock.json`
- `apps/desktop/src-tauri/src/services/usage.rs`
- `apps/desktop/src-tauri/src/commands/mod.rs`
- `apps/desktop/src-tauri/src/main.rs`
- `apps/desktop/src-tauri/Cargo.toml`
- `apps/desktop/src-tauri/Cargo.lock`
- `apps/desktop/src-tauri/tauri.conf.json`
- Focused tests listed in Current State.
- `plans/README.md`
- This file's Status Log.

Out of scope:

- Auth/PAT credential upload behavior.
- Account switch semantics, except tests proving Switch still restarts Codex.
- Any change that makes Quit stop or restart Codex.
- MCP, CLI, standalone reference dashboard hosting, or reference Python runtime.
- Moving the SQLite DB outside `~/.codex/lam/usage/`.
- Persisting raw transcript text in SQLite.
- Adding new npm or Rust dependencies unless an already-installed dependency
  cannot cover the exact need.

## Git Workflow

- Branch: `codex/008-usage-dashboard-parity`.
- Commit style: conventional commits, for example
  `feat: move usage statistics to full page`.
- Do not push or open a PR unless the operator asks.
- The executor may edit this plan only to append Status Log verification
  results, and may edit `plans/README.md` only to update Plan 008 status.

## Steps

### Step 1: Confirm Baseline

Run:

```bash
git rev-parse --short HEAD
git log --oneline -5
test -f apps/desktop/src-tauri/src/services/usage.rs
test -f apps/desktop/src/stores/usage.ts
test -f apps/desktop/src/components/tray-quota-panel.tsx
```

Expected:

- `98ef3f0` is present in `git log --oneline -5`, or the current branch is an
  equivalent merge containing Plan 007 and the Quit fix.
- All `test -f` commands exit 0.

**Verify**:

```bash
git diff --stat 98ef3f0..HEAD -- apps/desktop/src apps/desktop/src-tauri
```

Expected: empty output on the original Plan 007 worktree, or only reviewed
drift that still matches the Current State excerpts.

### Step 2: Expand the SQLite Aggregate Model

Update `apps/desktop/src-tauri/src/services/usage.rs` only.

Do the smallest additive migration that supports the parity fields listed
above:

- Add missing `usage_events` columns with `ALTER TABLE ... ADD COLUMN` guards.
- Add or extend `source_files` metadata for source hash, mtime, size, parser
  cursor, parser state, parser diagnostics JSON, and archive scope.
- Add `thread_summaries`.
- Add aggregate diagnostic facts and diagnostic snapshot metadata if needed by
  the diagnostics view.
- Preserve `usage_db_path(home_root) == home_root.join(".codex/lam/usage/usage.sqlite3")`.
- Preserve incremental refresh; do not rebuild all sessions on every refresh
  unless reset was requested.
- Do not store raw transcript text.

Parser behavior to port from the reference:

- call initiator, initiator reason, and confidence
- session/thread metadata
- current date and timezone
- model and reasoning effort
- model context window and context percentage
- subagent/parent thread metadata
- previous/next call linkage inside a thread
- pricing cost and Codex credit inputs needed by the dashboard

Add focused Rust tests in `usage.rs`:

- An old Plan 007 DB migrates to the new schema.
- `PRAGMA table_info(usage_events)` contains the required parity columns.
- `thread_summaries` and diagnostic fact tables exist.
- Incremental refresh updates only changed/new source files.
- No raw prompt/content column exists and no raw transcript text is stored.
- `usage_db_path` remains under `.codex/lam/usage/usage.sqlite3`.

**Verify**:

```bash
cd apps/desktop/src-tauri && cargo test usage
```

Expected: exit 0; new migration/parser tests pass.

### Step 3: Expand the Tauri Usage API and Frontend Types

Update:

- `apps/desktop/src/lib/types.ts`
- `apps/desktop/src/lib/api.ts`
- `apps/desktop/src/stores/usage.ts`
- any existing `apps/desktop/src/lib/usage-*.ts` helper files
- `apps/desktop/src-tauri/src/commands/mod.rs`
- `apps/desktop/src-tauri/src/main.rs`

Prefer one typed dashboard request/response command such as
`get_usage_dashboard` if it keeps the contract clearer than extending
`get_usage_summary`. Keep `refresh_usage_index`, `reset_usage_index`, and
`compact_usage_db` behavior intact.

The response must support:

- summary metrics
- available model/effort/pricing-confidence options
- source/status chips
- search, date range, sort, load limit, and history scope
- calls rows with all parity columns
- thread summaries
- insights and investigation presets
- diagnostics and pricing/credit coverage
- selected-call aggregate details

Do not add a generic untyped `Record<string, unknown>` dashboard payload when a
small explicit TypeScript/Rust type is feasible.

**Verify**:

```bash
cd apps/desktop && npm test -- --run src/lib/usage-dashboard.test.ts
cd apps/desktop/src-tauri && cargo test usage
```

Expected: both commands exit 0; helper tests cover pricing, sort, diagnostics,
credits, and no raw content.

### Step 4: Move Usage from Modal to Bottom-Nav Page

Update:

- `apps/desktop/src/routes/types.ts`
- `apps/desktop/src/components/icons.tsx`
- `apps/desktop/src/App.tsx`
- optionally a new route/component file under `apps/desktop/src/routes/`
- copied/adapted usage CSS under `apps/desktop/src/`

Required behavior:

- Add route `usage` immediately after `overview`.
- Bottom nav label must be `Usage`.
- Use an existing icon if enough; otherwise add a minimal `IconUsage` and add
  `usage` to `NavIconName`.
- Remove the top titlebar `Stats` button beside the PAT toggle.
- Remove the `usageStats` modal path for the main dashboard.
- Render the dashboard as a full page when `route === 'usage'`.
- In PAT mode, render the dashboard.
- Outside PAT mode, render a small LAM-style empty state.
- Copy/adapt reference dashboard CSS instead of inventing a new plain style.
  Namespace copied selectors under a usage page root such as
  `.usageDashboardPage` to avoid leaking styles.
- Reuse pure reference JS logic for formatting, analysis, filtering,
  diagnostics, insights, table state, and tooltips where practical; convert
  DOM-mutating code to React only where LAM requires it.

The page must render these visible items:

- summary cards except `Usage observed`
- filters and controls listed in Reference Dashboard Parity Target
- Insights / Calls / Threads / Diagnostics view switcher
- calls table with all parity columns
- detail panel for aggregate call fields
- load-more control

**Verify**:

```bash
cd apps/desktop && npm test -- --run src/App.handoff.test.tsx src/lib/usage-dashboard.test.ts
cd apps/desktop && npm run build
```

Expected: exit 0; tests assert Usage is beside Overview, no top Stats button
exists, the dashboard is not inside `Shell.Modal`, and all required visible
labels except `Usage observed` are present.

### Step 5: Add Tray Footer Stats Button

Update:

- `apps/desktop/src/components/tray-quota-panel.tsx`
- `apps/desktop/src/lib/api.ts`
- `apps/desktop/src-tauri/src/commands/mod.rs`
- `apps/desktop/src-tauri/src/main.rs`
- `apps/desktop/src/App.tsx` for the navigation event listener

Required behavior:

- Footer order is exactly `Quit`, `Stats`, `Open`.
- `Stats` shows/focuses the main LAM window and navigates to route `usage`.
- `Stats` must not invoke `quit_app`.
- `Stats` must not invoke `restart_codex`.
- `Open` keeps the existing main-window behavior.
- `Quit` keeps `invoke('quit_app')`.

Preferred implementation:

- Add command `show_usage_stats`.
- In Rust, reuse `crate::tray::show_main_window(&app)` and emit a route event
  to the main window, for example event name `lam:navigate` with payload
  `usage`.
- Register `commands::show_usage_stats` in `tauri::generate_handler!`.
- Add `showUsageStats()` in `apps/desktop/src/lib/api.ts`.
- In `App.tsx`, listen for the route event and call `setRoute('usage')`.
- In `tray-quota-panel.tsx`, call `showUsageStats()` from the middle button.

If the exact Tauri event API differs, use the existing installed
`@tauri-apps/api/event` and Tauri v2 Rust API equivalents. Do not introduce a
new dependency.

**Verify**:

```bash
cd apps/desktop && npm test -- --run src/components/tray-quota-panel.test.tsx src/App.handoff.test.tsx
cd apps/desktop/src-tauri && cargo test --lib
```

Expected: tests pass; tray test proves footer order and that Stats calls the
usage-page path without calling `restartCodex` or `quit_app`.

### Step 6: Full Verification

Before running full gates, bump the app version to `0.1.1` in:

- `apps/desktop/package.json`
- `apps/desktop/package-lock.json`
- `apps/desktop/src-tauri/Cargo.toml`
- `apps/desktop/src-tauri/Cargo.lock`
- `apps/desktop/src-tauri/tauri.conf.json`

Do not change dependency versions for this version bump.

Run all project gates:

```bash
cd apps/desktop && npm run build
cd apps/desktop && npm run test:ui
cd apps/desktop/src-tauri && cargo fmt -- --check
cd apps/desktop/src-tauri && cargo test usage
cd apps/desktop/src-tauri && cargo test --lib
cd apps/desktop/src-tauri && cargo clippy -- -D warnings
make check
```

Expected: every command exits 0.

Run a real-home usage smoke if the existing ignored smoke is still present:

```bash
cd apps/desktop/src-tauri && cargo test real_home_usage_smoke -- --ignored
```

Expected: exit 0; `~/.codex/lam/usage/usage.sqlite3` exists; SQLite integrity is
`ok`; no usage DB is created directly under `~/.codex`, `~/.codex/sessions`,
`~/.codex/logs`, or `~/.codex/cache`.

### Step 7: Installed-App Verification

Build the app:

```bash
cd apps/desktop && env -u CI npx tauri build 2>&1 | tee /tmp/tauri-build.log
```

Expected: exit 0; `/tmp/tauri-build.log` contains no build failure.

Install/open the produced macOS app, then verify manually and record results in
the Status Log:

- Usage entry is beside Overview in the bottom navigation.
- Usage renders as a full page, not a modal.
- Top PAT titlebar area has no Stats button.
- Tray footer shows `Quit | Stats | Open`.
- Tray `Stats` opens/focuses LAM and shows the Usage page.
- App Quit quits only LAM and does not quit Codex.
- Account Switch still restarts Codex as expected.

Capture at least one screenshot or exact observation note for the Usage page
and one for the tray footer.

## Test Plan

Frontend:

- Extend `apps/desktop/src/App.handoff.test.tsx` for route placement, full-page
  rendering, no modal, no top Stats button, and event-driven navigation.
- Extend `apps/desktop/src/components/tray-quota-panel.test.tsx` for
  `Quit | Stats | Open` order and API call behavior.
- Extend `apps/desktop/src/lib/usage-dashboard.test.ts` for reference parity
  helpers: sorting, filters, pricing confidence, credits, diagnostics, and no
  raw content.

Rust:

- Add focused tests inside `apps/desktop/src-tauri/src/services/usage.rs` for
  migrations, schema columns, parser fields, incremental refresh, diagnostics,
  credits, and DB location.

Smoke:

- Keep `npm run test:ui`.
- Run ignored real-home usage smoke and installed-app smoke before DONE.

## Done Criteria

All must hold:

- [x] `usage` route exists immediately after `overview`; label is `Usage`.
- [x] Usage dashboard is a full page, not a `Shell.Modal`.
- [x] Top PAT titlebar area has no Stats button.
- [x] Tray footer order is `Quit | Stats | Open`.
- [x] Tray `Stats` opens/focuses LAM on the Usage page.
- [x] Tray `Stats` does not invoke `quit_app` or `restart_codex`.
- [x] Quit only quits LAM.
- [x] Switch remains the only path that stops/restarts Codex.
- [x] App version is `0.1.1` in npm, Cargo, and Tauri config metadata.
- [x] All reference summary cards except `Usage observed` are visible.
- [x] Calls table includes all required parity columns.
- [x] Filters, load limit, history scope, view switcher, detail panel, pager,
  insights, diagnostics, pricing, credits, and investigation presets exist.
- [x] SQLite stays at `~/.codex/lam/usage/usage.sqlite3`.
- [x] Existing Plan 007 DBs migrate additively.
- [x] Incremental refresh remains incremental.
- [x] No raw transcript text is stored in SQLite.
- [x] All commands in Step 6 pass.
- [x] Installed-app verification in Step 7 is recorded.
- [x] Plan 008 row in `plans/README.md` is updated.

## STOP Conditions

Stop and report if:

- Plan 007 code or commit `98ef3f0` is absent from the execution tree.
- The Current State excerpts no longer match and the difference changes the
  route/modal/tray/usage-service design.
- Any reference path listed above is missing.
- Reference parity appears to require storing raw transcript text in SQLite.
- A step requires touching an out-of-scope file.
- A new dependency seems necessary.
- A verification command fails twice after a reasonable fix attempt.
- Installed-app verification cannot be performed on the target machine.

## Maintenance Notes

- Future usage-dashboard work should compare against the reference dashboard
  files first; do not grow a second independent dashboard design.
- Review schema migrations carefully. Broken migrations can strand existing
  `~/.codex/lam/usage/usage.sqlite3` databases.
- Review route/event changes carefully. Tray `Stats` is allowed to show/focus
  LAM; it is not allowed to touch Codex.

## Status Log

- 2026-06-28: Plan created from post-Plan-007 user feedback.
- 2026-06-28: Review tightened plan for template compliance: added drift
  check, current excerpts, parity field inventory, scope, per-step
  verification, STOP conditions, and done criteria. Not implemented.
- 2026-06-28: Executed in existing worktree
  `/Users/micro/Documents/Code/Rust/.codex-worktrees/LAM-007-pat-usage-statistics`
  without dropping Plan 007 commits. Completed on branch
  `codex/008-usage-dashboard-parity` at commits `d976196` and `556c116`.
  Reviewer verified version `0.1.1`, in-scope diff, focused dashboard/tray
  tests, `npm run build`, `npm run test:ui`, `cargo test usage`, `cargo test
  --lib`, `cargo fmt -- --check`, `cargo clippy -- -D warnings`, `make check`,
  ignored `real_home_usage_smoke`, and `env -u CI npx tauri build`. All passed.
  Tauri produced `LAM.app` and `LAM_0.1.1_aarch64.dmg`. Reviewer also checked
  the bundle Info.plist version, launched the built `LAM.app`, then cleaned up
  that temporary app process and confirmed Codex processes remained running.
