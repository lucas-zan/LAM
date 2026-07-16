# Todo: 将顶层 New Provider 合并到 External API 创建流程

> Executor instructions: Change the top-level Provider Center creation CTA only after its tests fail.
> Existing Provider edit/diagnostic/reuse behavior must remain available. Do not modify weekly/tray UI.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: Phase 4 Account-first lifecycle
- **Category**: feature/refactor
- **Planned at**: current dirty worktree; no commit requested

## Why this matters

**Background**: 用户确认普通操作应是 `Add Account → External API`，而不是先创建孤立 Provider。

**Current state**: Providers 页面顶层 `Add Provider` 打开 ProviderEditor，只写 Provider store；Add
Account 中的 API Account 才会原子创建 Account/Profile/CODEX_HOME/Provider/Binding。两个入口看似
完成同一目标，且顶层入口会留下不能直接运行的孤立连接。

**Impact**: 用户可能创建 Provider 后仍找不到可使用账号，并误解 Provider 是账号。

**What improves**: 顶层创建动作统一进入完整 External API Account 流程；Provider 页面只管理、
编辑、测试和复用已有连接。

## Scope

**In scope**:
- Provider Center 顶层 CTA 改为 `Add External API`。
- CTA 打开 Add Account modal 并预选 External API/API Account tab。
- 移除顶层新建孤立 Provider 的可达路径；保留已有 Provider 编辑。
- 对应组件/App wiring 测试和 UI smoke。

**Out of scope**:
- 删除 Provider domain/service API（兼容历史数据和高级复用仍需要）。
- 修改 API Account 的字段或事务语义。
- 修改 weekly/tray 悬浮框 UI。

## Design

- `ProviderCenter` 接收 `onAddExternalApi` callback；顶层 CTA 只调用 callback。
- App callback 重置 Add Account 临时状态、设置 `createMode = 'api'`，再打开 account modal。
- Provider editor 仅由现有卡片 `Edit` 打开；不再传入 `null` 创建态。
- 现有 Provider 仍可 Test/Edit/Attach/Detach，并可在 External API 的 Advanced 选项复用。

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Provider Center CTA | 不再打开 Add Provider；调用 External API callback | 验证成功 |
| T2 | App modal wiring | 从 Providers 入口打开 Add Account 且 API tab active | 验证成功 |
| T3 | Global header CTA | Overview 顶栏不再显示 New Provider；直接打开 External API | 验证成功 |
| T4 | Dedicated External API modal | External API 使用专用弹窗，不显示 New Account 类型 Tabs | 验证成功 |
| T5 | Remove duplicate API Account tab | New Account 仅保留 Profile/PAT；External API 为唯一 API 入口 | 验证成功 |

### T1: Provider Center CTA

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: 防止创建无 Account/CODEX_HOME 的孤立 Provider。

**What to do**: 修改 ProviderCenter props、按钮和 editor 可达状态。

**Logic design**: CTA callback 与 Provider 管理职责解耦；卡片 edit 保持原逻辑。

**Test design**:
- 点击 `Add External API` 调用 callback。
- 页面不存在 `Add Provider` 顶层按钮/创建 modal。
- 点击已有 Provider 的 Edit 仍打开 `Edit Provider`。

**Acceptance**: `provider-center-flow.test.tsx` 通过。

**Done criteria**:
- [x] tests-first red state 已确认：2 个测试因不存在 `Add External API` CTA 按预期失败
- [x] CTA 不创建孤立 Provider
- [x] existing edit/diagnostic focused tests 14/14 通过
- [x] 状态为 `验证成功`

### T2: App modal wiring

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: callback 必须落到完整 Account-first transaction UI，而非只改文案。

**What to do**: App 向 ProviderCenter 注入打开 External API modal 的 callback；扩展 static UI gate。

**Logic design**: 复用 account modal 和 ApiAccountFlow；只强制本次入口的初始 tab 为 API。

**Test design**:
- static/UI smoke 要求 ProviderCenter callback、`setCreateMode('api')`、`openModal('account')` 同时存在。
- Add Account 其他入口仍使用默认 auth mode。

**Acceptance**: focused frontend tests、全量 test/lint/format/build/UI smoke。

**Done criteria**:
- [x] tests-first red state 已确认：UI smoke 因缺 `openExternalApiModal` App wiring 按预期失败
- [x] App wiring 指向完整 ApiAccountFlow
- [x] full frontend gates 通过：22 files / 178 tests，lint、Prettier、build、UI smoke、G5
- [x] weekly/tray UI 无 diff
- [x] focused frontend tests 49/49 与 UI smoke 通过
- [x] 状态为 `验证成功`

### T3: Global header CTA

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: 截图中的真正顶层入口位于全局 Header。当前按钮仍显示 `New Provider`，并且只导航到
Provider Center，没有直接进入完整账号创建事务。

**What to do**: 将 Header CTA 改为 `External API`，点击时直接调用
`openExternalApiModal`。不改变通用 `New Account` 入口。

**Logic design**: 全局按钮和 Provider Center CTA 复用同一个 App callback，保证两处入口都预选
API Account 并打开同一个 Add Account modal，避免两套创建逻辑。

**Test design**:
- 从 Overview 点击全局 `External API` 后，出现 Add Account modal。
- API Account tab 为 active，且不会先跳转到 Provider Center。
- 页面不再存在 `New Provider` 按钮。

**Acceptance**: `App.handoff.test.tsx` focused test、UI smoke、全量前端门禁通过。

**Done criteria**:
- [x] tests-first red state 已确认：测试观察到 Header 仍渲染 `New Provider`
- [x] Header CTA 指向完整 ApiAccountFlow
- [x] focused 36/36、full 179/179、lint、Prettier、build、UI smoke、G5 通过
- [x] weekly/tray UI 无 diff
- [x] 状态为 `验证成功`

### T4: Dedicated External API modal

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: 当前 External API 与 New Account 共用 `Add Account` 外壳，只是默认选中 API Account，
造成两个顶层动作视觉和语义完全一致，不符合独立入口预期。

**What to do**: 新增独立 `externalApi` modal 状态。External API 入口显示标题
`Add External API` 并直接渲染 `ApiAccountFlow`，不显示 Profile/PAT/API 类型 Tabs；New Account
保持现状。

**Logic design**: 两个 modal 只区分导航和外壳，共享同一个 `ApiAccountFlow` 及同一个创建成功
handler，确保底层 Account-first 原子事务没有分叉。

**Test design**:
- Header 与 Provider Center 的 External API 入口都打开 `Add External API`。
- 专用弹窗没有 Profile Account、PAT Account、API Account Tabs。
- New Account 仍打开 `Add Account` 并显示可用类型 Tabs。

**Acceptance**: App focused tests、store modal test、UI smoke、全量前端门禁通过。

**Done criteria**:
- [x] tests-first red state 已确认：两个入口仍出现 `Add Account` 标题和账号类型 Tabs
- [x] External API 使用独立 modal 外壳
- [x] New Account 行为无回归
- [x] focused 3/3 + store 5/5、full 181/181、lint、Prettier、build、UI smoke、G5 通过
- [x] weekly/tray UI 无 diff
- [x] 状态为 `验证成功`

### T5: Remove duplicate API Account tab

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: `New Account → API Account` 与顶层 `External API` 使用相同流程和产物，重复入口会让用户
误以为它们能力不同。

**What to do**: 从通用 Add Account modal 移除 API Account Tab 和 ApiAccountFlow 分支；将
`createMode` 状态收紧为 `AuthMode`。保留专用 External API modal。

**Logic design**: New Account 仅负责 Profile/PAT 身份账号创建；External API 是外部端点账号的
唯一 UI 入口。底层 API Account service/domain 不删除。

**Test design**:
- New Account 显示 Profile/PAT（按 mode availability）且不存在 API Account Tab/API 表单。
- External API 专用弹窗继续渲染 API 表单。
- 源码静态门禁阻止 API Account Tab 回流到通用 modal。

**Acceptance**: App focused tests、UI smoke、全量前端门禁通过。

**Done criteria**:
- [x] tests-first red state 已确认：New Account 中仍能查询到 `API Account` 按钮
- [x] New Account 不再包含 API Account
- [x] External API 专用流程无回归
- [x] focused 3/3、full 181/181、lint、Prettier、build、UI smoke、G5 通过
- [x] weekly/tray UI 无 diff
- [x] 状态为 `验证成功`

## Test plan

- Normal: Provider Center → Add External API → Add Account/API Account。
- Compatibility: edit/test/attach existing Provider；Advanced reuse 保持。
- Negative: 顶层没有创建孤立 Provider 的按钮或 null editor。

## Verification commands

| Purpose | Command | Expected |
|---------|---------|----------|
| Focused | `npm --prefix apps/desktop test -- --run src/components/provider-center-flow.test.tsx` | exit 0 after expected red |
| Full | `npm --prefix apps/desktop test` | exit 0 |
| Quality | `npm --prefix apps/desktop run lint && npm --prefix apps/desktop run format:check && npm --prefix apps/desktop run build && npm --prefix apps/desktop run test:ui` | exit 0 |

## Done criteria

- [x] T1/T2/T3/T4/T5 都为 `验证成功`
- [x] 顶层 Provider create 已合并到 External API
- [x] 无 unresolved STOP condition

## STOP conditions

- 合并会破坏历史 Provider 编辑或 Advanced reuse。
- 完成需要修改 weekly/tray 悬浮框 UI。
