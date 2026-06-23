# Todo: Monthly Tray Layout

> Executor instructions: Follow this todo step by step. Write the test before implementation, confirm the expected failure, then implement the narrow layout fix.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: monthly quota window support
- **Category**: bugfix

## Why this matters

**Background**: Monthly-only quota rows currently reuse the left-side 5h layout. The circular progress ring overlaps the longer `monthly` label and reset text.

**Current state**: The tray row always uses the same `.trayAccountRowContentLeft` spacing for the first window, regardless of whether a second window exists.

**Impact**: Monthly quota rows are readable only partially; 5h/weekly rows must keep the existing layout.

**What improves**: Monthly-only rows get extra horizontal separation, while dual-window rows remain unchanged.

## Scope

**In scope**:
- Add a monthly-only class or equivalent marker to Codex tray account rows.
- Add CSS that moves only the monthly-only ring to the right.
- Add focused component/static coverage.

**Out of scope**:
- Changing 5h/weekly layout.
- Redesigning the tray row or overview cards.

## Design

Monthly-only means the derived quota window list has one item and that item has `variant === 'monthly'`. In that case, add a row/content class and use CSS spacing/padding for the left content. The normal two-window row must not receive this class.

## Tasks

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Monthly-only tray spacing | Tests prove monthly-only gets a dedicated class and standard dual-window rows do not | 验证成功 |

### T1: Monthly-only tray spacing

**Status**:
- [x] 待执行
- [x] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Test design**:
- Extend the monthly-only tray test to assert the monthly-only row/content class exists.
- Add/keep coverage that normal cached quota still shows 5h/weekly without monthly-only marker.
- Expected initial failure: class is missing.

**Acceptance**:
- `cd apps/desktop && npm test -- --run src/components/tray-quota-panel.test.tsx`
- `cd apps/desktop && npm run test:ui`
- `cd apps/desktop && npm run build`

**Done criteria**:
- [x] Test written before implementation
- [x] Expected failure observed before implementation
- [x] Implementation is scoped to monthly-only layout
- [x] Verification commands pass
- [x] Status is `验证成功`
