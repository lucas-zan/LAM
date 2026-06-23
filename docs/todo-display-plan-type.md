# Todo: Display Plan Type Beside Account

> Executor instructions: Write tests before implementation, confirm expected failure, then implement the narrow UI change.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Category**: feature

## Why this matters

**Background**: Codex app-server already returns `planType` in quota snapshots. Users want to see account type such as `team`, `plus`, or `pro` near the account name.

**Current state**: `planType` is parsed and cached but not shown in account rows/cards.

**Impact**: Users must inspect raw quota cache or infer plan from quota behavior.

**What improves**: Account rows show the plan next to the account name wherever quota data is available.

## Scope

**In scope**:
- Show plan type badge next to account name in the tray quota row.
- Show plan type badge next to account name in Overview account cards.
- Keep display hidden when `planType` is missing or blank.

**Out of scope**:
- Inferring Pro 5x/20x from quota data.
- Changing backend quota parsing.

## Design

Use `quota.planType` from the account's matching `UsageQuotaSnapshot`. Normalize only for display: trim and uppercase the raw value. Do not translate `team` to Business automatically.

## Tasks

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Render plan type badges | Tray and Overview show `TEAM` beside account name when quota has `planType` | 验证成功 |

### T1: Render plan type badges

**Status**:
- [x] 待执行
- [x] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Test design**:
- Extend tray quota panel test to assert `TEAM` appears beside the account name.
- Add a small helper test for plan label normalization and blank hiding.
- Expected initial failure: no `TEAM` text is rendered and helper does not exist.

**Acceptance**:
- `cd apps/desktop && npm test -- --run src/lib/quota.test.ts src/components/tray-quota-panel.test.tsx`
- `cd apps/desktop && npm run test:ui`
- `cd apps/desktop && npm run build`

**Done criteria**:
- [x] Tests written before implementation
- [x] Expected failure observed before implementation
- [x] Implementation is scoped to plan type display
- [x] Verification commands pass
- [x] Status is `验证成功`
