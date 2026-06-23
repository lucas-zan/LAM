# Todo: Tray Plan Type Below Account Name

> Executor instructions: Write tests before implementation, confirm expected failure, then keep the fix scoped to the tray popover account row.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Category**: bugfix

## Why this matters

**Background**: The tray popover currently renders `planType` beside the account name. Long account names can be truncated earlier because the plan badge shares the same row.

**Current state**: `.trayAccountNameWrap` is a horizontal flex row containing the account name, plan badge, and active badge.

**Impact**: In the compact popover, the account name loses space.

**What improves**: The tray popover shows the account name on the first line and the plan badge below it, matching the main app's stacked account metadata style while keeping Overview unchanged.

## Scope

**In scope**:
- Change only tray account row markup/CSS.
- Keep `planType` hidden when missing.
- Keep Overview plan badge placement unchanged.

**Out of scope**:
- Changing quota parsing or plan type labels.
- Redesigning account card layout outside the tray popover.

## Tasks

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Move tray plan badge below name | Tray test and smoke confirm dedicated below-name wrapper | 验证成功 |

### T1: Move tray plan badge below name

**Status**:
- [x] 待执行
- [x] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Test design**:
- Add a tray test asserting `TEAM` is inside a dedicated `.trayAccountPlanLine`.
- Add smoke/static coverage that tray uses `.trayAccountPlanLine`.
- Expected initial failure: wrapper is absent.

**Acceptance**:
- `cd apps/desktop && npm test -- --run src/components/tray-quota-panel.test.tsx`
- `cd apps/desktop && npm run test:ui`
- `cd apps/desktop && npm run build`

**Done criteria**:
- [x] Test written before implementation
- [x] Expected failure observed before implementation
- [x] Implementation is scoped to tray popover
- [x] Verification commands pass
- [x] Status is `验证成功`
