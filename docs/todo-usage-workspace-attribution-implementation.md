# Todo: Usage workspace scopes and account attribution

> Executor instructions: Follow this todo step by step. Generate tests from
> the "Test design" section before implementation. Run each verification command
> and confirm the expected result before moving to the next task. If a STOP
> condition occurs, stop and report instead of improvising.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: `docs/usage-workspace-account-attribution-design.md`
- **Category**: feature/refactor
- **Planned at**: `agent/main-20260703094659-optimize-reset-credits-usage-display`

## Why this matters

**Background**: Usage currently reads only `~/.codex`, so it cannot represent multiple LAM profile workspaces. Users can mix profile and PAT workflows, so usage must be grouped by actual workspace directories first.

**Current state**: `refresh_usage_index` reads fixed `home_root/.codex/sessions`, stores aggregate local JSONL usage in `~/.codex/lam/usage/usage.sqlite3`, and the Usage page is gated behind PAT mode.

**Impact**: Multi-profile usage is incomplete, PAT/Profile mode naming leaks into usage semantics, and users cannot view `Total | main | codex-*` workspace scopes.

**What improves**: Usage becomes workspace-scoped, the dashboard exposes backend-provided scopes, and the frontend renders usage in all modes without modifying unrelated frontend surfaces.

## Scope

**In scope**:
- Usage backend workspace discovery, refresh, storage columns, scoped response, and PAT switch timeline recording.
- Usage frontend API/store/page integration only.
- Tests covering backend workspace scopes and frontend scope tabs.

**Out of scope**:
- Non-Usage frontend layout changes.
- Official billing API integration.
- Manual auth file modification handling.

## Design

Implement the first usable slice of `docs/usage-workspace-account-attribution-design.md`:

- Backend discovers actual Codex homes from account scan.
- Refresh parses every discovered workspace directory.
- Usage DB records workspace fields and basic attribution fields.
- Backend returns `UsageDashboardResponse { scopes, activeScopeId, dashboard }`.
- Frontend usage store consumes response and renders scope tabs if more than one scope exists.
- Usage is no longer hidden outside PAT mode.
- PAT switch writes a timeline file after successful auth copy verification.

## Tasks

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Backend workspace scopes | Rust tests show refresh aggregates `~/.codex` and `~/.codex-c`, scoped dashboard filters by workspace | 验证成功 |
| T2 | PAT timeline recording | Rust test shows `switch_to_pat_account` records an open timeline event after switch | 验证成功 |
| T3 | Frontend scope tabs | Usage store/page tests show scopes render as tabs and tab selection requests `scopeId` | 验证成功 |

### T1: Backend workspace scopes

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Current usage refresh only scans `~/.codex`.

**What to do**:
- Add `UsageScope` and `UsageDashboardResponse`.
- Extend usage event/source schemas with workspace fields.
- Discover and refresh all account workspaces.
- Add query scope filtering for dashboard response.

**Logic design**:
- Use account scan as workspace source.
- De-duplicate by canonical home path.
- `total` is default when there is more than one workspace.
- Per-workspace query filters by `workspace_id`.

**Test design**:
- Backend fixture creates `~/.codex` and `~/.codex-c` sessions.
- Refresh indexes both.
- Default response has `Total`, `main`, `codex-c`.
- `workspace:c` response includes only `codex-c` tokens.

**Acceptance**:
- `cargo test usage_workspace`

### T2: PAT timeline recording

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
PAT attribution requires knowing which LAM account was active in shared `~/.codex`.

**What to do**:
- Add timeline JSON file under LAM config.
- Record switch event after `switch_to_pat_account` succeeds.

**Logic design**:
- Close previous open event.
- Append new open event.
- Fail switch if timeline cannot be written.

**Test design**:
- Existing PAT switch fixture asserts `pat-usage-timeline.json` records selected account and shared workspace.

**Acceptance**:
- `cargo test pat_usage_timeline`

### T3: Frontend scope tabs

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Users need `Total | main | codex-*` usage scopes regardless of PAT mode.

**What to do**:
- Add response API wrapper and types.
- Update usage store to hold scopes and active scope.
- Update Usage page to render tabs from backend response.
- Remove PAT-only empty state.

**Logic design**:
- Frontend does not infer PAT/Profile.
- Backend response is source of truth for scopes.
- Tab click passes `scopeId` in request.

**Test design**:
- Store test verifies response state.
- App/Usage test verifies non-PAT Usage renders dashboard, tabs, and tab click calls API with selected scope.

**Acceptance**:
- `npm test -- --run src/stores/usage.test.ts src/App.handoff.test.tsx`

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Backend workspace | `cargo test usage_workspace` | exit 0 |
| Backend PAT timeline | `cargo test pat_usage_timeline` | exit 0 |
| Frontend focused | `npm test -- --run src/stores/usage.test.ts src/App.handoff.test.tsx` | exit 0 |
| Frontend build/lint | `npm run lint && npm run build` | exit 0 |

## Done criteria

- [x] Every task's own tests were written before implementation
- [x] Expected failures were observed before implementation
- [x] All task statuses are `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

- Usage event timestamps cannot support scope/timeline filtering.
- Multi-workspace refresh would require unrelated frontend changes.
- Existing usage tests reveal schema migration incompatibility that cannot be safely fixed in this pass.
