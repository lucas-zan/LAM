# Todo: Smaller Plan Type Badge

> Executor instructions: Add static coverage before implementation, confirm expected failure, then make the scoped style change.

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Category**: bugfix

## Why this matters

**Background**: The plan type badge is useful, but it is visually too large in the compact tray popover.

**Current state**: `.planTypeBadge` uses `font-size: 10px` and `padding: 3px 6px`.

**Impact**: The badge competes with the account name and takes more vertical/horizontal space than needed.

**What improves**: The badge remains readable but becomes more compact.

## Scope

**In scope**:
- Reduce `.planTypeBadge` size.
- Add smoke coverage for the compact badge dimensions.

**Out of scope**:
- Moving the badge again.
- Changing account name, quota, or reset layouts.

## Tasks

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Shrink plan badge | Smoke and build pass with compact badge dimensions | 验证成功 |

### T1: Shrink plan badge

**Status**:
- [x] 待执行
- [x] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Test design**:
- Update smoke to require `font-size: 9px` and `padding: 2px 5px` for plan badge.
- Expected initial failure: current CSS still uses larger values.

**Acceptance**:
- `cd apps/desktop && npm run test:ui`
- `cd apps/desktop && npm run build`
- `cd apps/desktop && npx prettier --check src/styles.css scripts/ui-smoke.mjs`

**Done criteria**:
- [x] Static test written before implementation
- [x] Expected failure observed before implementation
- [x] Style change is limited to `.planTypeBadge`
- [x] Verification commands pass
- [x] Status is `验证成功`
