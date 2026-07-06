# Todo: Usage Activity Insights

> Executor instructions: Follow this todo step by step. Generate tests from the "Test design" section before implementation. Run each verification command and confirm the expected result before moving to the next task.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: current Usage page
- **Category**: bugfix
- **Planned at**: current dirty workspace

## Why this matters

**Background**: The official Codex app shows Activity insights as compact behavioral metrics. LAM currently uses the Insights tab for diagnostic and investigation content.

**Current state**: `UsagePage` renders `Needs attention` and a raw `Diagnostics brief` inside the Insights tab. Diagnostics are summarized by joining parser counters and unknown model names into one paragraph.

**Impact**: Users cannot compare LAM's insights with the official Activity insights, and diagnostics look noisy and unstructured.

**What improves**: Insights becomes a concise activity summary. Diagnostics stay available in the Diagnostics tab with clearer structure.

## Scope

**In scope**:
- `apps/desktop/src/routes/usage.tsx`
- `apps/desktop/src/routes/usage.test.tsx`

**Out of scope**:
- CSS/style changes.
- Backend schema changes.
- New backend aggregates for skill events.

## Design

- Keep the existing tabs and styles.
- Replace the Insights tab content with an `Activity insights` panel.
- Compute insights from the currently loaded `recentCalls` and `topThreads`.
- `Fast Mode`: percentage of calls whose reasoning effort is fast-like (`low`, `minimal`, `none`, `fast`).
- `Most used reasoning`: most frequent non-empty `effort`, with percentage.
- `Skills explored`: distinct observed skill labels from `subagentType`, `agentRole`, or `agentNickname`.
- `Total skills used`: count of calls with at least one observed skill label.
- `Total threads`: count of current `topThreads`.
- Move diagnostics content to Diagnostics tab only and render it as structured rows.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Render activity insights | Tests prove Insights shows activity metrics and no diagnostics brief | 验证成功 |

### T1: Render activity insights

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The current Insights tab is a diagnostics view, not an activity insights view.

**What to do**:
- Add a test that renders UsagePage with mixed efforts and skill fields.
- Assert Insights displays Fast Mode, Most used reasoning, Skills explored, Total skills used, and Total threads.
- Assert `Diagnostics brief` is not shown in Insights.
- Update the Diagnostics tab to show diagnostics as labeled rows instead of one raw paragraph.

**Logic design**:
- Add small pure helper functions in `usage.tsx` for counting reasoning/skill metrics.
- Use deterministic tie-breaking by count, then label.
- Avoid changing CSS classes or visual styles.
- Keep diagnostics data source unchanged.

**Test design**:
- Render calls with 1 fast-like call, 3 medium calls, and two distinct skill labels across three skill-bearing calls.
- Expect `Fast Mode` to show `25%`.
- Expect `Most used reasoning` to show `Medium · 75%`.
- Expect `Skills explored` to show `2`, `Total skills used` to show `3`, and `Total threads` to match loaded threads.
- Expect no `Diagnostics brief` text while Insights tab is active.

**Acceptance**:
- `cd apps/desktop && npm test -- src/routes/usage.test.tsx`
- `cd apps/desktop && npx tsc --noEmit`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Test plan

- Focused UsagePage render test for Activity insights.
- Existing UsagePage tests for tabs, activity heatmap, and row caps.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Usage route | `cd apps/desktop && npm test -- src/routes/usage.test.tsx` | exit 0 |
| Typecheck | `cd apps/desktop && npx tsc --noEmit` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:
- Matching the behavior requires CSS changes.
