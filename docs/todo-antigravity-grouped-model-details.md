# Todo: Antigravity Grouped Model Details

> Executor instructions: Follow this todo step by step. Generate tests from the "Test design" section before implementation. Run each verification command and confirm the expected result before moving to the next task.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: Antigravity quota summary response fields
- **Category**: feature/bugfix
- **Planned at**: current dirty workspace

## Why this matters

**Background**: Antigravity quota summary currently renders only group-level weekly and 5-hour windows. The official app also makes it clear which models are in each quota group.

**Current state**: `RetrieveUserQuotaSummary` provides groups and buckets, while `GetUserStatus` provides a flat `models` list. Overview uses groups when present and ignores flat model rows. The tray popover still renders only flat model rows.

**Impact**: Users see only two aggregate cards and cannot inspect the model membership under each group. The tray popover can also disagree with the main Overview.

**What improves**: Overview and tray both show group quota totals first, then model rows grouped below the relevant quota group.

## Scope

**In scope**:
- Shared Antigravity model grouping helper.
- Overview Antigravity tab rendering.
- Tray quota popover Antigravity rendering and summary counters.
- Focused frontend tests.

**Out of scope**:
- Changing the Antigravity language server calls.
- Redesigning unrelated Codex quota views.

## Design

- Keep backend response as `groups + models`.
- Add a framework-independent helper that groups flat model rows under quota groups.
- Match models to groups by normalized group description terms and known group name keywords:
  - Gemini group matches model labels containing `gemini`.
  - Claude and GPT group matches labels containing `claude`, `gpt`, `gpt-oss`, or `gpt oss`.
- Preserve unmatched model rows in an `Other models` group so data is never hidden.
- Overview renders group bucket windows, then a compact model list under each group.
- Tray popover uses the same helper so hover content follows the main page.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Shared grouped quota model | Helper test groups Gemini, Claude, and GPT models under summary groups | 验证成功 |
| T2 | Overview grouped model details | Overview test sees group buckets and model rows under those groups | 验证成功 |
| T3 | Tray grouped model details | Tray test sees grouped Antigravity quota and model rows in the popover | 验证成功 |

### T1: Shared grouped quota model

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Overview and tray need identical grouping logic.

**What to do**:
- Add `apps/desktop/src/lib/antigravity.ts`.
- Export `groupAntigravityModels`.
- Keep matching deterministic and independent from React.

**Logic design**:
- If no groups exist, return an empty grouped summary and leave fallback rendering to callers.
- For each model, find the first group whose extracted terms match the model label.
- Include unmatched models in a synthetic `Other models` group.

**Test design**:
- Add `apps/desktop/src/lib/antigravity.test.ts`.
- Test Gemini/Claude/GPT labels are grouped correctly.
- Test unmatched model rows remain visible under `Other models`.

**Acceptance**:
- `cd apps/desktop && npm test -- src/lib/antigravity.test.ts`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T2: Overview grouped model details

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The main page currently only shows group totals.

**What to do**:
- Update `AntigravityModels` in `apps/desktop/src/routes/views.tsx`.
- Render model rows under each quota group after bucket windows.
- Keep current group bucket display and model-only fallback.

**Logic design**:
- Use shared grouping helper when `quota.groups` exists.
- Render group description without one-line truncation.
- Render each model label with remaining percentage and reset time when available.

**Test design**:
- Update route test to include Gemini, Claude, and GPT model rows.
- Assert model names appear after switching to Antigravity.

**Acceptance**:
- `cd apps/desktop && npm test -- src/routes/handoff.test.tsx`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T3: Tray grouped model details

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The tray popover should not show a different Antigravity quota model from Overview.

**What to do**:
- Update `TrayAntigravityModelList` in `apps/desktop/src/components/tray-quota-panel.tsx`.
- Update Antigravity counters to count groups when group summary exists.
- Add focused tray test for grouped Antigravity display.

**Logic design**:
- Prefer grouped summary cards in the tray when groups exist.
- Display bucket percentages and grouped model names.
- Preserve old flat model rendering when groups are absent.

**Test design**:
- Mock `getAntigravityQuota` with groups and models.
- Switch to Antigravity tab and assert group and model rows are present.

**Acceptance**:
- `cd apps/desktop && npm test -- src/components/tray-quota-panel.test.tsx`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Test plan

- Unit test pure grouping helper.
- React route test for Overview.
- React component test for tray popover.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Helper | `cd apps/desktop && npm test -- src/lib/antigravity.test.ts` | exit 0 after implementation |
| Overview | `cd apps/desktop && npm test -- src/routes/handoff.test.tsx` | exit 0 after implementation |
| Tray | `cd apps/desktop && npm test -- src/components/tray-quota-panel.test.tsx` | exit 0 after implementation |
| Related suite | `cd apps/desktop && npm test -- src/lib/antigravity.test.ts src/routes/handoff.test.tsx src/components/tray-quota-panel.test.tsx` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:
- The flat model data cannot be reliably matched to any group and hiding unmatched models would be required.
- The tray test cannot isolate the Antigravity tab.
