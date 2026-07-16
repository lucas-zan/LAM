# Todo: 修复 make start 启动时 Tauri invoke 未初始化

> Executor instructions: Add the regression test first, confirm the current implementation fails,
> implement only the runtime-boundary fix, then run focused frontend and startup/build gates.

## Status

- **Priority**: P0
- **Effort**: S
- **Risk**: MEDIUM
- **Depends on**: Phase 4 / G5
- **Category**: bugfix
- **Planned at**: current dirty Phase 4 worktree; no commit requested

## Why this matters

**Background**: `make start` 后 UI 报 `Cannot read properties of undefined (reading 'invoke')`。

**Current state**: `inTauri()` 只判断 `__TAURI_INTERNALS__` 属性是否存在，不验证其值和
`invoke` 函数；Account 启动 refresh 即使不在有效 Tauri runtime 中也无条件触发 Provider V2
refresh，而 Provider API 直接调用 `@tauri-apps/api/core.invoke`。

**Impact**: Tauri bridge 尚未可用或 Vite renderer 被浏览器打开时，启动 refresh 会进入原始
Tauri SDK 并产生不可理解的 JavaScript TypeError。

**What improves**: 仅当 bridge 对象和 `invoke` 函数真实存在时调用 native command；preview/
初始化边界返回空 Provider state，不出现 SDK TypeError。真实 Tauri 行为保持不变。

## Scope

**In scope**:
- 加固 runtime detection。
- Provider 启动 refresh 的非 Tauri 边界。
- 前端回归测试、build/UI smoke。

**Out of scope**:
- Provider/API Account 产品行为调整。
- weekly/tray 悬浮框 UI 修改。
- Rust command 或 Gateway 修改。

## Design

- `inTauri()` 必须验证 `window.__TAURI_INTERNALS__` 是非空对象且 `invoke` 是函数。
- Provider store `refresh()` 在 bridge 不可用时原子地清空 provider/binding/loading state并返回。
- mutation API 保持 native-only；UI 正常只会在 Tauri 中提供这些操作。
- 不吞掉真实 native command 错误。

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Runtime boundary regression | undefined/malformed internals 不调用 SDK invoke；真实 bridge 仍刷新 | 验证成功 |

### T1: Runtime boundary regression

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: 启动流程必须区分有效 native bridge 与仅存在同名属性/浏览器 renderer。

**What to do**:
- 扩展 `api` 和 Provider store 测试。
- 修正 `inTauri` 与 Provider refresh guard。

**Logic design**:
- 使用结构检查，不依赖属性存在性。
- preview guard 不调用 `invoke`，并形成确定的空状态。

**Test design**:
- `__TAURI_INTERNALS__ = undefined` 时 `inTauri()` 为 false。
- `{}` 或 `{ invoke: non-function }` 时为 false。
- Provider refresh 在无 bridge 时不调用 provider APIs 且 loading 结束。
- 有 bridge/mock runtime 时保留现有 refresh 行为。

**Acceptance**:
- `npm --prefix apps/desktop test -- --run src/lib/api-runtime.test.ts src/stores/providers-v2.test.ts`
- `npm --prefix apps/desktop run lint && npm --prefix apps/desktop run format:check && npm --prefix apps/desktop run build && npm --prefix apps/desktop run test:ui`

**Done criteria**:
- [x] tests-first red state 已确认：5 个 malformed bridge 断言和 1 个 Provider refresh guard 断言按预期失败
- [x] 实现符合 runtime boundary 设计
- [x] focused tests 21/21 通过
- [x] full frontend 177/177、lint/format/build/UI smoke 通过
- [x] weekly/tray UI 无 diff
- [x] 状态更新为 `验证成功`

## Test plan

- Normal: mock valid bridge 保持 native refresh。
- Edge: internals undefined、null、空对象、invoke 非函数。
- Error: 非 native startup 不进入 SDK invoke；native command error 仍传播。

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused | `npm --prefix apps/desktop test -- --run src/lib/api-runtime.test.ts src/stores/providers-v2.test.ts` | exit 0 |
| Quality | `npm --prefix apps/desktop run lint && npm --prefix apps/desktop run format:check && npm --prefix apps/desktop run build && npm --prefix apps/desktop run test:ui` | exit 0 |

## Done criteria

- [x] T1 为 `验证成功`
- [x] 启动边界无 raw SDK TypeError
- [x] 无 unresolved STOP condition

## STOP conditions

- 错误来自 Rust/Tauri bridge 注入失败而非 frontend boundary，且需要升级 framework。
- 修复需要改动用户的 weekly/tray 悬浮框 UI。
