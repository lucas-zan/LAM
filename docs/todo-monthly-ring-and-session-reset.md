# Todo: Monthly Ring Spacing And Session Reset Text

> Executor instructions: Write tests before implementation, confirm expected failure, then keep changes scoped to monthly-only tray layout and session reset formatting.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Category**: bugfix

## Why this matters

**Background**: Monthly-only rows now show the correct quota type, but the progress ring still sits too close to the text. Session/5h reset text also shows a full date in cases where only time is needed.

**Current state**: Monthly-only layout has a dedicated class, but the spacing is not enough. `formatResetAt` can include date text for session windows when the reset date is not the current local day.

**Impact**: Monthly rows look cramped, and 5h rows show unnecessarily long reset labels.

**What improves**: Monthly-only rows move the ring farther right while 5h/weekly rows stay unchanged. Session resets always display hour/minute AM/PM only.

## Scope

**In scope**:
- Adjust `.trayAccountRow--monthlyOnly` CSS only.
- Add/reset formatting tests for session time-only behavior.
- Add smoke guard for monthly-only spacing CSS.

**Out of scope**:
- Any layout change to normal 5h + weekly rows.
- Changing weekly/monthly date formatting.

## Tasks

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Monthly ring spacing and 5h reset format | Monthly-only spacing is guarded, session reset is time-only | 验证成功 |

### T1: Monthly ring spacing and 5h reset format

**Status**:
- [x] 待执行
- [x] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Test design**:
- Add a reset helper test proving session reset formatting excludes year/month/day for a future reset.
- Add smoke/static coverage that monthly-only layout has wider text spacing and does not alter base `.trayAccountRowContentLeft`.
- Expected initial failure: session helper includes date; smoke lacks the new spacing guard.

**Acceptance**:
- `cd apps/desktop && npm test -- --run src/lib/reset.test.ts src/components/tray-quota-panel.test.tsx`
- `cd apps/desktop && npm run test:ui`
- `cd apps/desktop && npm run build`

**Done criteria**:
- [x] Tests written before implementation
- [x] Expected failure observed before implementation
- [x] Implementation stays scoped to monthly-only/session formatting
- [x] Verification commands pass
- [x] Status is `验证成功`
