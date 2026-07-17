# Todo: Provider 空集合 revision 修复

> Executor instructions: Follow this todo step by step. Generate tests from
> the "Test design" section before implementation. Run each verification command
> and confirm the expected result before moving to the next task. If a STOP
> condition occurs, stop and report instead of improvising.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: `docs/todo-provider-gateway-review-remediation.md` Stage 1
- **Category**: bugfix
- **Planned at**: current workspace, 2026-07-16

## Why this matters

**Background**: Provider V2 写操作从 `providers[0].storeRevision` 推导 CAS revision。删除最后一个 API Account 独占 Provider 或创建失败补偿回滚后，Provider store revision 已递增但列表为空，前端只能错误回退为 0。

**Current state**:

- 后端 `list_providers_v2` 返回 `Vec<ProviderProfileView>`，空数组不能携带集合 revision。
- 前端 `saveProvider`、Keychain Provider 创建和 credential rotation 从第一项读取 revision。
- 前端并发 refresh 没有 generation 保护，较旧响应可能覆盖较新状态。
- `StructuredErrorView` 不暴露 CAS details，冲突处理只能刷新并要求重新提交，不能盲目自动重试。

**Impact**: 普通“创建 External API Account → 删除该账户 → 从 Provider Center 创建 Provider”流程可让直接创建路径跨刷新、跨重启持续返回 `STORE_REVISION_CONFLICT`。

**What improves**: Provider 集合即使为空也携带真实 revision；所有写操作使用独立集合 revision；并发 refresh 不会让 revision 倒退；冲突不会静默覆盖并发修改。

## Scope

**In scope**:

- Provider V2 列表 service/command 返回显式集合快照。
- Provider view 构建复用同一次 Provider store snapshot。
- TypeScript API 和类型契约同步升级。
- Zustand 保存 `providerStoreRevision: number | null` 并保护 refresh 乱序。
- Provider 写操作使用独立 revision。
- 后端、前端 store/API 和关键 UI mock 回归测试。

**Out of scope**:

- Gateway Semaphore/Admission Controller 修改。
- Attach/Detach 倒计时和 Supervisor 修改。
- 通用自动重试 CAS 冲突。
- 跨 Provider/Binding/health 的完整原子 Hub snapshot。
- `provider_api_v2.rs` 领域拆分。

## Design

### Backend contract

新增：

```rust
pub struct ProviderListViewV2 {
    pub revision: u64,
    pub providers: Vec<ProviderProfileView>,
}
```

`list_providers_v2` 返回该结构。Provider 内容与 revision 必须来自同一次 Provider store load。Bindings 和 health 允许最终一致，但视图构建函数不得重新加载 Provider store。

现有内部只需要 `Vec<ProviderProfileView>` 的调用者继续通过兼容 service 获取数组；内部实现复用同一个 snapshot-to-view helper，不复制逻辑。

### Frontend state

```ts
providerStoreRevision: number | null;
providerRefreshGeneration: number;
```

- `null` 表示尚未成功加载，不能写入。
- `0` 只表示后端明确返回 revision 0。
- refresh 为每次请求分配 generation，只有最后一次发起的 refresh 可以提交结果。
- Provider 和 Binding 两个请求都成功后一次提交，任一失败不提交半份状态。

### Conflict behavior

`STORE_REVISION_CONFLICT` 后刷新并显示重新提交提示。不得读取一个新的 revision 后自动重放 update/rotate/attach 请求。

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|---|---|---|---|
| T1 | 后端 Provider 集合快照 | 空列表返回非零 revision，并可用它再次创建 | 验证成功 |
| T2 | 前端 API 和类型契约 | `listProvidersV2` 返回 `{ revision, providers }` | 验证成功 |
| T3 | Store revision 与 refresh 竞态 | 写操作使用独立 revision，旧 refresh 不能覆盖新状态 | 验证成功 |
| T4 | 集成回归与完整验收 | 相关 Rust/TS 测试和生产构建通过 | 验证成功 |

### T1: 后端 Provider 集合快照

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: 空数组无法表达 Provider store revision，是持续冲突的根因。

**What to do**:

- 在 `provider_api_v2.rs` 新增 `ProviderListViewV2`。
- 抽取接收已加载 Provider/Binding/health snapshot 的视图构建 helper。
- 新增/调整列表 service，使 Provider 内容和 revision 来自同一次 Provider snapshot。
- 修改 Tauri `list_providers_v2` command 返回集合快照。
- 保持内部数组调用者行为兼容。

**Logic design**:

- Provider store 只读取一次。
- 视图 helper 不执行 Provider store I/O。
- 删除最后一个 Provider 后返回空 `providers` 和递增后的 revision。
- 使用返回 revision 进行下一次 create 时 CAS 成功。

**Test design**:

- 先新增后端测试：空初始 store 返回 revision 0 和空列表。
- 创建再删除最后一个独占 Provider 后，列表为空且 revision 非零。
- 使用该非零 revision 再创建 Provider 成功。
- 旧 revision 仍返回 `STORE_REVISION_CONFLICT`。
- 命令/序列化契约断言 camelCase `{ revision, providers }`。

**Expected initial failure**: 当前没有 `ProviderListViewV2`/集合 service；现有列表只返回数组，新增测试应编译失败或契约断言失败。

**Acceptance**:

- `cargo test --test provider_api_v2`
- `cargo test --test provider_phase4_api_account`

**Verification record (2026-07-16)**:

- 新增测试先因缺少 `ProviderListViewV2` / `list_provider_hub_view_v2` 按预期编译失败。
- DTO 空列表 revision 测试通过。
- service 创建 → 删除最后一项 → 空列表 revision=2 → 使用 revision=2 再创建测试通过。
- API Account 删除集成测试在当前受限沙箱因无法分配 Gateway loopback port 报 `GATEWAY_PORT_UNAVAILABLE`；相同 revision 回归已由不依赖端口的 repository/service 测试覆盖，完整 runner 在 T4 再验证。

**Done criteria**:

- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T2: 前端 API 和类型契约

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: 后端返回形状变更必须由 TypeScript 类型、API wrapper 和调用者显式接收，不能兼容猜测数组/对象双形状。

**What to do**:

- 在 `lib/types.ts` 新增 `ProviderListViewV2`。
- 修改 `lib/api.ts` 的 `listProvidersV2()` 返回类型。
- 更新 API 契约测试和相关 mock。

**Logic design**:

- API 只接受新对象形状。
- 旧数组形状应在类型/契约测试中明确失败，而不是运行时静默转换。

**Test design**:

- API wrapper 断言调用 `list_providers_v2` 并返回集合快照。
- 相关组件/store mock 改为 `{ revision, providers }`。

**Expected initial failure**: 当前 API 返回类型为数组，新测试的对象访问/类型断言失败。

**Acceptance**:

- `npm test -- --run src/lib/provider-api-v2.test.ts`
- `npm run build`

**Verification record (2026-07-16)**:

- TypeScript 构建先因缺少 `ProviderListViewV2` 导出按预期失败。
- 新增显式集合类型并更新 `listProvidersV2()` 泛型返回值。
- API wrapper 测试和隔离 TypeScript 检查通过。
- 全项目 build 会继续暴露 T3 Store 尚未适配新返回形状的问题，属于下一任务的预期失败态。

**Done criteria**:

- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T3: Store revision 与 refresh 竞态

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: 独立 revision 修复空列表问题；generation 防止旧 refresh 响应覆盖新 revision。

**What to do**:

- Zustand 新增 `providerStoreRevision` 和内部 refresh generation。
- refresh 等 Provider/Binding 都成功后一次提交。
- 写操作在 revision 为 `null` 时拒绝执行。
- `saveProvider`、Keychain 创建和 credential rotation 使用独立 revision。
- 冲突后刷新并要求重新提交，不自动重放。

**Logic design**:

- 后发起的 refresh 是唯一可以提交的 generation。
- 非最新 refresh 成功或失败都不能覆盖最新状态/loading。
- 非 Tauri 路径清空列表并把 revision 恢复为 `null`。

**Test design**:

- 空列表 revision=7 时 create 提交 expectedRevision=7。
- revision=null 时 write 拒绝且不调用 API。
- 两个 refresh 乱序返回时保留较新 generation。
- Provider 请求成功但 Binding 请求失败时不提交半份状态。
- 三类写操作都使用独立 revision。
- CAS conflict 刷新但不自动重放原写请求。

**Expected initial failure**: 当前 store 无独立 revision/generation，空列表仍提交 0，旧 refresh 会覆盖新状态。

**Acceptance**:

- `npm test -- --run src/stores/providers-v2.test.ts`
- `npm test -- --run src/components/provider-center-flow.test.tsx src/App.handoff.test.tsx`

**Verification record (2026-07-16)**:

- 新返回形状先让 Store 测试按预期失败：Store 把集合对象写进 `providers`，空列表写操作仍提交 revision 0。
- 新增空列表 revision=7、revision=null、refresh 乱序、partial failure、Keychain 创建、credential rotation 和冲突不重放测试。
- Store 15 项聚焦测试通过；Provider Center/App/accounts 相关 47 项测试通过；生产 TypeScript/Vite build 通过。
- Keychain/rotation 显式断言在核心实现后补充，但测试设计已预先写入本 todo；该顺序偏差记录于此，不作为后续任务跳过失败态的先例。

**Done criteria**:

- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T4: 集成回归与完整验收

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: 契约变化涉及 Rust service、Tauri command、TypeScript API、Store 和多处 mock，需要完整相关套件证明无回归。

**What to do**:

- 更新全部 `listProvidersV2` mock/caller。
- 运行格式、前端全套测试、构建和相关 Rust 测试。
- 记录受限环境测试例外，不把权限失败误判为业务失败。

**Logic design**:

- 不修改 Gateway 并发、Supervisor 或其他范围外代码。
- 失败测试必须定位并修复，最多 5 轮。

**Test design**:

- 由 T1～T3 测试组成；额外运行全相关套件发现契约遗漏。

**Expected initial failure**: T1～T3 实现前不执行此任务。

**Acceptance**:

- `npm test -- --run`
- `npm run build`
- `cargo test --test provider_api_v2 --test provider_phase4_api_account`
- `cargo test --tests`（受限 macOS process scan 失败需单独记录）

**Done criteria**:

- [ ] Tests listed in this task's Test design were written before implementation
- [ ] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [ ] Implementation follows this task's Logic design and stays inside this task's What to do
- [ ] Focused verification command passes
- [ ] Relevant suite/build/lint command passes when applicable
- [ ] Task overview row status matches this task status
- [ ] This task status is updated to `验证成功`

**Verification record (2026-07-16)**:

- 前端完整测试：22 files / 204 tests passed。
- 前端生产构建：`tsc && vite build` passed。
- Rust `provider_api_v2`：16 passed。
- Rust `provider_phase4_api_account` 串行：7 passed；并行运行存在既有 stable Gateway port 争用，因此最终使用串行结果验收该套件。
- 完整 Rust `cargo test --tests -- --test-threads=1` 运行到 `provider_auth_runtime::unresolved_or_unsafe_auth_runtime_fails_closed` 时失败：实际错误码 `PROVIDER_AUTH_HELPER_INVALID`，测试期望 `AUTH_HELPER_EXECUTABLE_INVALID`。
- 本轮没有修改 `provider_runtime` / auth helper 错误映射；该失败属于范围外既有工作区状态。根据 STOP condition，不在 Provider revision 修复中顺手改变认证错误契约。
- 用户要求继续后，该错误码问题被拆分到独立 `docs/todo-auth-helper-error-contract.md`，按现有失败测试修复。
- 修复后完整 Rust 串行套件通过。

**Post-implementation design review (2026-07-16)**:

- 全仓调用者审计确认 `list_providers_v2` 只由同包桌面前端和测试使用，因此选择 V2 命令原子升级；没有保留数组/对象双形状兼容。
- Provider 内容与 revision 来自同一次 Provider store load；Binding/health 仍按设计保持最终一致。
- `providerStoreRevision: null` 明确表示尚未加载，`0` 只来自后端真实 revision。
- refresh generation 只允许最后发起的 refresh 提交 Provider/Binding 组合结果。
- CAS 冲突刷新但不重放写请求。
- 未发现需要修改实现的设计偏差。

**Done criteria**:

- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Test plan

- Backend normal: empty store revision 0, non-empty store correct revision.
- Backend regression: delete last exclusive Provider, empty list keeps nonzero revision, next create succeeds.
- Backend conflict: stale revision still rejected.
- Contract: serialized list is `{ revision, providers }` in camelCase.
- Frontend normal: refresh stores revision and providers together.
- Frontend empty: empty providers does not imply revision 0.
- Frontend edge: revision null blocks writes.
- Frontend race: stale refresh cannot overwrite latest generation.
- Frontend error: partial Promise failure does not commit half state.
- Conflict: refresh and prompt retry, no blind write replay.

## Verification commands

| Purpose | Command | Expected on success |
|---|---|---|
| Rust focused | `cd apps/desktop/src-tauri && cargo test --test provider_api_v2 --test provider_phase4_api_account` | exit 0 |
| Store focused | `cd apps/desktop && npm test -- --run src/stores/providers-v2.test.ts` | exit 0 |
| API focused | `cd apps/desktop && npm test -- --run src/lib/provider-api-v2.test.ts` | exit 0 |
| Frontend suite | `cd apps/desktop && npm test -- --run` | exit 0 |
| Frontend build | `cd apps/desktop && npm run build` | exit 0 |
| Rust relevant/full | `cd apps/desktop/src-tauri && cargo test --tests` | exit 0 outside documented sandbox exception |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

- `list_providers_v2` 已有未发现的外部稳定调用者，无法在同一版本升级契约。
- Provider、Binding、health 必须被产品要求为完整原子 snapshot，导致需要扩大锁设计。
- 测试证明修改需要触及 Gateway 调度或其他范围外子系统。
- 必需测试基础设施缺失且不能安全补充。
- 单任务修复循环达到 5 次仍无法通过。
