# Todo: Usage Activity Daily Weekly Tooltips

> Executor instructions: Follow this todo step by step. Generate tests from the "Test design" section before implementation. Run each verification command and confirm the expected result before moving to the next task.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: current Usage heatmap
- **Category**: bugfix
- **Planned at**: current dirty workspace

## Why this matters

**Background**: The activity heatmap has Daily and Weekly controls, but the hover text still reads like a daily data point in both modes.

**Current state**: Weekly mode computes a weekly value but labels every cell with the cell date.

**Impact**: Users cannot tell whether a hovered cell is showing daily or weekly consumption.

**What improves**: Daily cells show daily consumption and weekly cells show the corresponding week total, matching the official Codex profile interaction.

## Scope

**In scope**:
- `apps/desktop/src/routes/usage.tsx`
- `apps/desktop/src/routes/usage.test.tsx`

**Out of scope**:
- CSS/style changes.
- Backend aggregation changes.

## Design

- Daily mode tooltip: `<value> <metric> on <date>`.
- Weekly mode tooltip: `<value> <metric> on week of <week start date>`, using the Sunday-based week shown by the activity grid.
- Weekly mode keeps each cell colored by its week total.
- Token/call label follows the selected metric.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Align daily and weekly activity tooltips | Tests prove daily tooltip uses date and weekly tooltip uses week total | 验证成功 |

### T1: Align daily and weekly activity tooltips

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The UI currently makes Daily and Weekly look equivalent.

**What to do**:
- Add UsagePage tests for daily token tooltip and weekly token tooltip.
- Update heatmap cell title generation.

**Logic design**:
- Use the activity grid's Sunday-based week start for weekly values and labels.
- Format dates with a readable month/day form.
- Do not change visual styles.

**Test design**:
- Render token daily mode and assert May 17 cell tooltip says daily tokens on May 17.
- Switch to Weekly and assert May 17 cell tooltip says weekly total on week of May 17, 2026.

**Acceptance**:
- `cd apps/desktop && npm test -- src/routes/usage.test.tsx`
- `cd apps/desktop && npx tsc --noEmit`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Test plan

- Focused UsagePage test for Daily vs Weekly token tooltip.

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
