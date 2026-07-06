# Todo: Usage Scope Backfill And Headline Filtering

> Executor instructions: Follow this todo step by step. Generate tests from the "Test design" section before implementation. Run each verification command and confirm the expected result before moving to the next task.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: current Usage workspace attribution
- **Category**: bugfix
- **Planned at**: current dirty workspace

## Why this matters

**Background**: Usage scope tabs should make Total and per-workspace numbers internally consistent.

**Current state**: Old rows can have `workspace_id IS NULL`, so they appear only in Total. `query_local_headline_stats` also misses workspace/account filters for peak daily tokens and longest task.

**Impact**: Total does not match visible workspace tabs, and per-workspace headline cards can show global peak/longest values.

**What improves**: Refresh can migrate old indexed rows into workspace scopes, and all headline values use the same scope filter as overview totals.

## Scope

**In scope**:
- `apps/desktop/src-tauri/src/services/usage.rs`

**Out of scope**:
- Frontend visual changes.
- Account billing attribution semantics beyond existing workspace/account fields.

## Design

- Add workspace metadata backfill during refresh after logs are discovered and before parsing decisions are applied.
- Backfill by exact `source_file` path from `SourceLog`, not fuzzy string matching.
- Update both `source_files` and `usage_events` for rows whose workspace metadata is missing or stale.
- Keep existing incremental parsing behavior: unchanged files still skip parsing.
- Add workspace/account predicates to every query inside `query_local_headline_stats`.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Scope-filter headline stats | Scoped peak/longest values no longer use global rows | 验证成功 |
| T2 | Backfill old workspace metadata | Refresh assigns old NULL workspace rows to their workspace without reparse | 验证成功 |

### T1: Scope-filter headline stats

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Per-workspace headline cards currently reuse global daily peak and longest task calculations.

**What to do**:
- Add a Rust test with two workspaces where global peak/longest differ from one workspace.
- Add `workspace_id` and `attributed_account_id` filters to `peak_daily_tokens` and `longest_running_turn_sec`.

**Logic design**:
- Reuse `SummaryFilter` values.
- Keep query parameter order explicit.
- Leave account snapshot override behavior unchanged for total scope only.

**Test design**:
- Create `main` with small one-event day.
- Create `codex-c` with two events in the same turn spanning 120 seconds and larger tokens.
- Request `workspace:main` and assert peak/longest reflect only main rows.

**Acceptance**:
- `cd apps/desktop/src-tauri && cargo test usage::tests::scoped_headline_stats_apply_workspace_filter`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T2: Backfill old workspace metadata

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Rows indexed before workspace attribution remain `workspace_id IS NULL` and only appear in Total.

**What to do**:
- Add a Rust test that simulates an already parsed unchanged source with NULL workspace columns.
- Refresh again and assert the row appears under its workspace scope without reparsing.
- Implement exact-source backfill for `usage_events` and `source_files`.

**Logic design**:
- Use discovered `SourceLog` entries as authoritative workspace metadata.
- Run backfill before `source_logs_requiring_parse`, so unchanged sources can still be fixed.
- Count backfill as metadata maintenance, not parsed files.
- Do not overwrite unrelated source paths.

**Test design**:
- Parse a main source once.
- Manually null out `usage_events.workspace_id` and matching `source_files.workspace_id`.
- Refresh without changing the file.
- Assert `parsed_files == 0`, Total still has one row, and `workspace:main` also has one row.

**Acceptance**:
- `cd apps/desktop/src-tauri && cargo test usage::tests::refresh_backfills_workspace_metadata_for_unchanged_sources`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Test plan

- Rust focused tests for scoped headline filtering and workspace backfill.
- Existing `usage_workspace_response_aggregates_and_filters_workspaces`.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| T1 focused | `cd apps/desktop/src-tauri && cargo test usage::tests::scoped_headline_stats_apply_workspace_filter` | exit 0 |
| T2 focused | `cd apps/desktop/src-tauri && cargo test usage::tests::refresh_backfills_workspace_metadata_for_unchanged_sources` | exit 0 |
| Relevant suite | `cd apps/desktop/src-tauri && cargo test usage::tests::usage_workspace_response_aggregates_and_filters_workspaces usage::tests::scoped_headline_stats_apply_workspace_filter usage::tests::refresh_backfills_workspace_metadata_for_unchanged_sources` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:
- Backfill requires deleting or rebuilding the user DB.
- Exact source-file matching is insufficient for existing indexed rows.
