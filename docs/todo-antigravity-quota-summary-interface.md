# Todo: Antigravity Quota Summary Interface

> Executor instructions: Follow this todo step by step. Generate tests from the "Test design" section before implementation. Run each verification command and confirm the expected result before moving to the next task.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: existing Antigravity language server discovery and quota command
- **Category**: feature/bugfix
- **Planned at**: current dirty workspace

## Why this matters

**Background**: Antigravity official app exposes model quota as group-level weekly and 5-hour windows through `RetrieveUserQuotaSummary`. LAM currently calls `GetUserStatus` and only displays one per-model quota window.

**Current state**: Backend `AntigravityModelQuota` has only `label`, `remainingFraction`, and `resetTime`. Frontend `AntigravityModels` renders one `Remaining Quota` card per model.

**Impact**: LAM cannot show official weekly and 5-hour limits, which makes its Antigravity quota view materially less useful.

**What improves**: LAM uses the official quota summary endpoint and can display `Gemini Models` / `Claude and GPT models` with `Weekly Limit` and `Five Hour Limit` buckets.

## Scope

**In scope**:
- `apps/desktop/src-tauri/src/services/antigravity.rs`
- `apps/desktop/src/lib/types.ts`
- `apps/desktop/src/routes/views.tsx`
- related Rust and frontend tests

**Out of scope**:
- Redesigning tray popover layout beyond preserving compatibility.
- Removing model-level quota data.

## Design

Backend behavior:
- Continue discovering Antigravity language server processes and ports as today.
- Query `RetrieveUserQuotaSummary` to parse `response.description`, `response.groups[*]`, and `response.groups[*].buckets[*]`.
- Keep `GetUserStatus` model parsing for backward compatibility.
- Return `AntigravityQuotaResponse { ok, models, groups, description, error }`.
- A port is successful when either quota summary groups or model configs are available.

Frontend behavior:
- Extend `AntigravityQuotaResponse` with `description` and `groups`.
- Antigravity overview prefers `groups` when present.
- Each bucket maps `remainingFraction` to a `QuotaWindow` used percent of `100 - remaining%`.
- `weekly` buckets use weekly variant; `5h` buckets use session variant.
- Existing model display remains fallback.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Parse and return quota summary groups | Rust test parses weekly and 5h buckets from `RetrieveUserQuotaSummary` JSON | 验证成功 |
| T2 | Render group quota buckets in Overview | Frontend test shows Gemini Models with Weekly Limit and Five Hour Limit | 验证成功 |

### T1: Parse and return quota summary groups

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The existing backend response cannot represent two quota windows.

**What to do**:
- Add `AntigravityQuotaBucket` and `AntigravityQuotaGroup` structs.
- Add `description` and `groups` fields to `AntigravityQuotaResponse`.
- Add parser helper for `RetrieveUserQuotaSummary` response.
- Query the summary endpoint alongside model status.

**Logic design**:
- Keep parsing tolerant: missing descriptions are `None`; missing bucket display names skip only invalid bucket rows.
- Preserve current model parsing and error fallback.
- Prefer HTTPS then HTTP as current implementation does.

**Test design**:
- Unit test parses sample response with Gemini weekly and 5h buckets.
- Unit test ensures missing response/groups returns a useful parser error.

**Acceptance**:
- `cd apps/desktop/src-tauri && cargo test antigravity --lib`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Task status is updated to `验证成功`

### T2: Render group quota buckets in Overview

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The frontend must use the new group data instead of only model rows.

**What to do**:
- Extend TypeScript quota types.
- Render official quota groups first in `AntigravityModels`.
- Keep model fallback when no groups are returned.

**Logic design**:
- Prefer `quota.groups?.length` over `quota.models.length` for empty-state detection.
- Use existing `QuotaWindow` component for bucket rows.
- Keep labels and descriptions from backend response.

**Test design**:
- Render Overview, switch to Antigravity, and assert description/group/bucket labels appear.
- Assert model fallback behavior still works when groups are absent.

**Acceptance**:
- `cd apps/desktop && npm test -- src/routes/handoff.test.tsx src/App.handoff.test.tsx`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Task status is updated to `验证成功`

## Test plan

- Rust parser unit tests for quota summary.
- React route tests for group rendering and fallback compatibility.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Rust focused | `cd apps/desktop/src-tauri && cargo test antigravity --lib` | New parser tests fail before implementation, pass after |
| Frontend focused | `cd apps/desktop && npm test -- src/routes/handoff.test.tsx src/App.handoff.test.tsx` | New render tests fail before implementation, pass after |
| Related suite | `cd apps/desktop && npm test -- src/routes/handoff.test.tsx src/App.handoff.test.tsx src/components/tray-quota-panel.test.tsx` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:
- `RetrieveUserQuotaSummary` cannot be represented without breaking the existing Tauri command response.
- The frontend tests cannot isolate Antigravity tab rendering.
