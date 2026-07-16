# Provider / Gateway 代码评审统一修正方案

> 本文汇总当前 Remote Provider Gateway / External API Account 实现中已经确认的缺陷、原评审文档自身存在的问题，以及面向后续多账户并发场景的最佳实现方案。
>
> 本文是实施 TODO，不代表相关代码已经完成。所有任务必须先补测试，再实施，再按本文验收。

## 1. 目标与结论

当前实现的整体安全方向正确：Gateway 仅监听 loopback、使用 Binding Token、凭据进入 Keychain、上游 URL 受 SSRF 策略限制，并且 Provider/Binding 写入采用 revision/CAS 和事务恢复机制。

但目前仍有四个已经确认的问题：

| 编号 | 问题 | 实际影响 | 优先级 |
|---|---|---|---|
| P0-1 | 空 Provider 列表无法携带真实 store revision，导致直接创建持续冲突 | 普通 UI 路径可触发，跨刷新和重启持续存在 | P0，发布阻塞 |
| P1-1 | Attach/Detach 计划倒计时冻结 | 后端安全，但 UI 会错误显示计划仍可执行 | P1 |
| P1-2 | Gateway 分层 Semaphore 获取会造成跨 Binding 资源占用和不公平 | 多账户并发时可能出现无关 Binding 超时或限流 | P1-A 最小修复；P2 完整调度 |
| P1-3 | Supervisor 仅凭 PID 判活 | PID 复用时可能永久不再拉起 Gateway | P1 |

另外还有两项架构工作：

- `provider_api_v2.rs` 已超过 3200 行，应在功能修正后按领域拆分。
- Gateway 当前的 32/4/16 并发限制是固定硬限制组合，缺少真正的 Binding 公平调度、空闲容量借用、Provider 故障隔离和可观测排队反馈。

禁止把上述改动作为一次“大爆炸式”重构交付。建议实施顺序：

1. Stage 0：建立测试基线和当前 32/4/16 行为基线，不修改调度语义。
2. Stage 1：只修复 Provider 集合快照和 revision 契约。
3. Stage 2：修复计划时钟和 Supervisor 身份判活状态机。
4. Stage 3：只做 Gateway 最小并发隔离修复，保持容量、队列和错误行为不变。
5. Stage 4：先增加并发、排队、流生命周期指标和压力测试。
6. Stage 5：引入 Admission Controller，但保持 Binding burst 等于 4，不扩大容量。
7. Stage 6：启用两级公平队列，验证跨 CapacityKey 和跨 Binding 公平性。
8. Stage 7：单独启用有界借用，从 5 或 6 起步，指标证明安全后才允许到 8。
9. Stage 8：真实指标充分后再评估按 Provider/CapacityKey 的自适应并发。
10. Stage 9：拆分 `provider_api_v2.rs`，不改变对外行为。

### 1.1 全局实施原则

- 每个 Stage 必须可以独立合入、独立验证和独立回滚。
- 同一个 Stage 不同时修改调度算法、默认容量和错误语义。
- 新调度器启用初期必须支持内部配置开关回退到旧调度器；该开关不得暴露为不受控的公共安全旁路。
- 所有 permit、queue waiter、后台 stream task 和 Supervisor 状态都必须有单一所有者和明确 Drop/退出路径。
- 等待中的请求不得持有执行额度、明文凭据或已解析的 Authorization header。
- 无法证明旧 Gateway 已退出时，不得启动第二个 Gateway。
- 不以单次单元测试通过代替压力测试、取消测试、超时测试和进程恢复测试。
- 自适应并发默认关闭；在固定容量公平调度稳定前不得实施。

### 1.2 明确不在同一阶段完成的事项

- P0 Provider revision 修复不得顺带重构整个 `provider_api_v2.rs`。
- P1-A Semaphore 顺序修复不得同时把单 Binding 上限从 4 提高到 8。
- Admission Controller 首次启用不得同时开启 burst 或自适应并发。
- Supervisor 身份校验不得在一次 control timeout 后直接终止进程或启动替代实例。
- Provider 列表契约变更不得通过静默兼容猜测处理，必须有明确的命令/API 迁移策略。

### 1.3 新方案风险登记

| 风险 | 可能后果 | 强制缓解措施 |
|---|---|---|
| 一次分配本地与上游全部额度 | 上游席位被解析/DNS/I/O 空占 | 两阶段 permit |
| 新旧上游计数并存 | 双重限流、指标漂移 | 唯一容量真值 |
| 长流按普通请求计费 | 启动公平但资源时间不公平 | active stream 分类与硬上限 |
| 只按 Provider ID 隔离 | 重复 Provider 绕过容量 | 非秘密 UpstreamCapacityKey |
| 只做 Binding 内公平 | 一个 CapacityKey 占满全局 16 | CapacityKey + Binding 两级公平 |
| Binding 数量可无限增加 | 多 Binding 获取过多轮询份额 | 容量域硬上限、状态数量限制、明确公平主体 |
| 排队只限制请求数 | 大 Body 导致内存峰值 | queued body bytes 硬限制 |
| Notify/取消竞态 | 丢失唤醒、幽灵 waiter | generation、循环检查、单一状态转换 |
| 流后台 task 与 Body 生命周期脱节 | permit 泄漏、shutdown hang | task 持有 permit、断连链路测试 |
| refresh 乱序 | 前端 revision 倒退 | refresh generation |
| Supervisor 健康瞬断 | 双实例/split-brain | 显式状态机、旧实例退出证明 |
| 调度与 burst 同时上线 | 无法定位性能/429 回归 | 分 Stage 启用，burst 初始仍为 4 |
| 自适应使用错误延迟信号 | 冷启动导致剧烈震荡 | 默认关闭、离线回放、只用审定信号 |

---

## 2. P0：Provider 空列表丢失 revision

### 2.1 已确认的根因

前端 Provider 写操作当前从列表第一项推导 store revision：

```ts
const expectedRevision = get().providers[0]?.storeRevision ?? 0;
```

相关位置包括：

- `apps/desktop/src/stores/providers.ts` 的 `saveProvider`。
- `createKeychainProvider`。
- `rotateKeychainCredential`。

后端 `list_providers_v2` 只返回 `Vec<ProviderProfileView>`。当 Provider 集合为空时，数组无法表达集合本身的 revision，前端只能错误回退为 `0`。

真实可达的普通操作路径：

1. 创建 External API Account，并生成独占 Provider `account-{profileId}`。
2. 从账户页面删除该 API Account。
3. `delete_api_account_service_v2` 在解绑、删除账户后调用 `delete_exclusive_provider`。
4. 最后一个 Provider 被删除，Provider store revision 递增，但集合变为空。
5. Provider Center 刷新只得到 `[]`，丢失真实 revision。
6. 随后直接创建 Provider 或 Keychain Provider 时提交 `expectedRevision = 0`，持续收到 `STORE_REVISION_CONFLICT`。

API Account 创建失败后的补偿回滚和崩溃恢复删除也可以形成同一状态。

该状态会跨刷新和应用重启保留。完整 API Account 创建计划可能通过后端重新读取 revision 而继续工作，但 Provider Center 的直接创建路径没有可靠自愈方式。

### 2.2 最佳数据契约

Provider 列表必须是一个显式带版本号的集合快照：

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderListViewV2 {
    pub revision: u64,
    pub providers: Vec<ProviderProfileView>,
}
```

前端对应类型：

```ts
export type ProviderListViewV2 = {
  revision: number;
  providers: ProviderProfileViewV2[];
};
```

即使集合为空，也必须能够表达：

```json
{
  "revision": 7,
  "providers": []
}
```

### 2.3 Provider 内容与 revision 必须使用同一快照

不能先读取 Provider store 获得 revision，再调用会重新读取 store 的 `list_provider_views_service_v2`。两次读取之间可能发生并发写入，形成 `revision=N`、`providers=N+1` 的不一致响应。

应抽取一个不执行 I/O 的视图构建函数：

```rust
fn build_provider_views(
    home_root: &Path,
    providers: &VersionedSnapshot<ProviderCollectionV2>,
    bindings: &VersionedSnapshot<ProfileBindingCollection>,
    health: &VersionedSnapshot<ProviderHealthCollectionV2>,
) -> Result<Vec<ProviderProfileView>> {
    // 仅使用传入快照组装 readiness、usedBy 和 health 视图。
}
```

新的列表 service 一次读取所需快照：

```rust
pub fn list_provider_hub_view_v2(home_root: &Path) -> Result<ProviderListViewV2> {
    let stores = provider_hub_stores(home_root)?;
    let providers = stores.providers.load_or_default()?;
    let bindings = stores.bindings.load_or_default()?;
    let health = stores.health.load_or_default()?;

    let views = build_provider_views(home_root, &providers, &bindings, &health)?;

    Ok(ProviderListViewV2 {
        revision: providers.revision,
        providers: views,
    })
}
```

现有内部调用者如果仍只需要数组，可以通过同一个构建函数或新列表结果取得 `providers`，不能复制视图组装逻辑，也不能为同一个响应再次读取 Provider store。

这里必须准确区分两种一致性：

- Provider 内容与 `revision` 必须来自同一次 Provider store 读取，这是 CAS 正确性的硬要求。
- Bindings、health 与 Provider store 当前分别调用 `load_or_default()`，每次会单独获取 shared installation lock；连续读取不等于跨 Store 原子快照。`usedBy`、health 和 readiness 可以接受短暂最终一致，但文档和代码注释不得把它描述成完整 Hub 原子快照。

如果产品未来要求 Provider、Binding 和 health 完全原子一致，必须一次取得 `InstallationLockGuard` 后使用各 Store 的 `load_locked()` 读取。持锁期间禁止调用会再次自行获取同一 installation lock 的 service/helper，避免重入锁或死锁。

### 2.3.1 命令/API 迁移策略

修改 `list_providers_v2` 的返回形状属于契约变化。桌面前后端通常同包发布，但测试脚本、smoke 工具或其他内部调用者仍可能依赖旧数组响应。

实施前必须通过 `rg` 枚举全部调用者并选择一种明确策略：

1. 如果 V2 契约尚未对外承诺：原命令原子升级，前后端、测试和脚本在同一 Stage 一起修改。
2. 如果已有外部/独立调用者：新增 `list_provider_hub_v2` 返回集合快照，保留旧 `list_providers_v2` 一段迁移期并标记 deprecated。

禁止通过运行时判断“返回的是数组还是对象”来长期维持双形状兼容，这会隐藏版本不匹配。发生不兼容时应返回明确的 protocol/contract error。

### 2.4 前端 revision 必须独立保存

Zustand 状态新增：

```ts
providerStoreRevision: number | null;
```

语义：

- `null`：尚未成功加载集合快照，禁止写操作。
- `0`：后端明确返回 store revision 为 0。
- 正整数：后端返回的真实 revision。

刷新逻辑：

```ts
const [providerList, bindings] = await Promise.all([
  api.listProvidersV2(),
  api.listProfileProviderBindingsV2(),
]);

set({
  providers: providerList.providers,
  providerStoreRevision: providerList.revision,
  bindings,
});
```

所有 Provider 写操作只允许读取 `providerStoreRevision`：

```ts
const expectedRevision = get().providerStoreRevision;
if (expectedRevision == null) {
  throw new Error('Provider state has not been loaded');
}
```

禁止继续使用：

```ts
get().providers[0]?.storeRevision ?? 0
```

### 2.4.1 防止 refresh 响应乱序覆盖新状态

新增独立 revision 后，仍需处理并发 refresh 的返回乱序：

```text
refresh A 开始
refresh B 开始
refresh B 先返回 revision 8
refresh A 后返回 revision 7
```

如果 A 无条件 `set()`，前端会退回旧状态。Store 应维护 refresh generation/request token，只允许当前最新 refresh 提交结果；同时将 Provider 和 Binding 的显示状态作为同一 refresh generation 提交。

仅比较 `providerList.revision` 仍不足以保护 bindings 响应，因为 bindings 有独立 revision。建议模式：

```ts
const generation = get().nextProviderRefreshGeneration();
const result = await loadProviderHubViews();
if (generation !== get().activeProviderRefreshGeneration) return;
set(result);
```

如果继续使用两个并行 Tauri 命令，应在同一个 generation 内等待两者完成后一次提交；任一失败不得提交半份新状态。更完整的后续方案是返回带 provider/binding revisions 的 Hub read model，但不应扩大 P0 修复范围。

### 2.5 CAS 冲突不得普遍自动重试

收到 `STORE_REVISION_CONFLICT` 后，默认行为应为：

1. 重新读取完整 `ProviderListViewV2`。
2. 原子更新 `providers` 和 `providerStoreRevision`。
3. 清除依赖旧快照的 Attach/Detach plan。
4. 提示状态已经变化，要求用户重新确认或重新提交。

不能简单读取 `actual` revision 后自动重放原来的 update/rotate 请求。这样会绕过乐观锁，可能用基于旧快照的编辑覆盖其他并发修改。

允许自动重试的操作必须逐项证明安全：

- Create：重新加载后确认 ID 仍不存在，才可以选择自动重试一次；第一版可直接要求用户重新提交。
- Update：不得盲目重试，必须让用户基于新快照重新确认。
- Credential rotation：必须重新确认当前 credential reference 仍等于 `expectedCredential`。
- Attach/Detach：必须重新生成 plan，不能重放旧 ticket。

当前 `StructuredErrorView` 不包含 `details`，因此不得在方案中假设前端能直接读取 `details.actual`。如未来确需暴露冲突元数据，应设计经过白名单和脱敏的专用字段，而不是透传全部内部错误 details。

### 2.6 P0 测试

后端测试：

- 空集合仍返回真实 revision。
- `ProviderListViewV2.revision` 与 `providers` 来自同一 Provider snapshot。
- 删除最后一个独占 Provider 后，列表为 `[]` 且 revision 大于删除前。
- API Account 创建回滚删除 Provider 后，列表仍携带正确 revision。
- 使用空列表响应中的 revision 可以成功创建下一项 Provider。
- 使用旧 revision 创建时仍返回 `STORE_REVISION_CONFLICT`。

前端测试：

- `refresh()` 同时保存 `providers` 和 `providerStoreRevision`。
- 空列表不会把 revision 重置为 0。
- revision 为 `null` 时拒绝写操作。
- `saveProvider`、Keychain 创建和 credential rotation 使用独立 revision。
- CAS 冲突后刷新并提示，不自动覆盖新状态。
- 删除最后一个 External API Account 后仍能从 Provider Center 创建 Provider。
- 跨应用重新加载空列表后仍能创建 Provider。
- 两次并发 refresh 乱序返回时，旧响应不能覆盖新响应。
- Provider 与 Binding 并行加载任一失败时，不提交半份快照。
- 契约升级后旧响应形状会明确失败，不静默猜测。

### 2.7 P0 验收

- Provider 集合 revision 不再依赖数组元素存在。
- 空 Provider 集合可以继续创建普通和 Keychain Provider。
- 读接口不存在同一响应内的双重 Provider store 读取。
- 并发修改不会被前端自动重试静默覆盖。
- 并发 refresh 不会让前端 revision 倒退。
- API/命令迁移策略已明确，所有调用者在同一 Stage 完成更新。

---

## 3. P1：Attach/Detach 计划倒计时冻结

### 3.1 根因

`provider-binding-dialog.tsx` 把时间固定为弹窗打开时刻：

```ts
const [openedAt] = useState(() => Date.now());
const effectiveNow = nowMs ?? openedAt;
```

父组件没有传入持续变化的 `nowMs`，因此倒计时不减少，`expired` 不会自动翻转。后端仍会拒绝过期计划，所以不存在配置安全问题，但 UI 会错误允许用户点击执行。

### 3.2 最佳修复

Dialog 内置每秒更新的时钟，测试仍可通过 `nowMs` 覆盖：

```tsx
const [tickNow, setTickNow] = useState(() => Date.now());

useEffect(() => {
  if (nowMs != null) return;
  const timer = window.setInterval(() => setTickNow(Date.now()), 1_000);
  return () => window.clearInterval(timer);
}, [nowMs]);

const effectiveNow = nowMs ?? tickNow;
```

当没有 plan 时可不启动定时器，减少无意义渲染。到期后必须自动：

- 将剩余时间显示为 0。
- 显示计划已过期。
- 禁用 Execute。
- 保留 Preview 按钮以生成新计划。

### 3.3 测试与验收

- fake timer 推进后倒计时递减。
- 到期时 Execute 自动禁用。
- `nowMs` 注入时不创建真实 interval。
- 关闭/卸载 Dialog 后 interval 被清理。

---

## 4. P1-A：Gateway 当前并发隔离的最小安全修复

### 4.1 当前限制

当前默认限制为：

| 层级 | 当前值 |
|---|---:|
| Gateway 全局 inflight | 32 |
| 单 Binding inflight | 4 |
| Gateway 全局队列 | 64 |
| 单 Binding 队列 | 8 |
| 上游 HTTP inflight | 16 |

这些限制说明 Gateway 已支持异步并发。问题不是“没有并发”，而是多个限制通过嵌套 Semaphore 获取，等待第二个许可时可能持有第一个许可。

### 4.2 已确认的问题

当前排队路径先获取 global permit，再等待 binding permit：

```rust
let global = state.inflight.clone().acquire_owned().await;
let binding = binding_limits.inflight.clone().acquire_owned().await;
```

当 Binding A 已达到 4 个并发时，A 的排队请求可能占住 global permit 等待 A 的 binding permit。多个繁忙 Binding 可以消耗全部 global permit，使仍有自身额度的 Binding B 无法执行。

上游层另有 16 个 inflight 限制，并使用立即失败的 `try_acquire_owned()`。因此即使 Gateway 层允许 32 个请求进入处理，超过上游 16 的请求仍可能直接返回 `GATEWAY_UPSTREAM_CONCURRENCY_LIMIT`，两层调度行为不一致。

### 4.3 发布前最小修复

如果完整 Admission Controller 无法在当前发布周期完成，先统一改成：

```text
先取得 Binding permit
再取得 global permit
```

繁忙 Binding 的后续请求会等待自己的 Binding semaphore，不再提前占用 global permit。

即时路径同样必须先尝试 Binding，再尝试 global；第二个许可失败时必须通过 RAII 立即释放第一个许可。

这个改动只能解决“等待中的繁忙 Binding 占住全局许可”，不能宣称实现了严格的跨 Binding 公平性。Tokio semaphore 的 FIFO 是请求级 FIFO，不是按 Binding 的公平轮询，也不支持空闲额度借用。

该最小修复会让最多 4 个请求持有某个 Binding permit 等待 global permit。这是有界的，并且不会占用其他 Binding 的 permit，但仍不是最终的原子 admission。实现时必须确认：

- 等待 global 时 Future 被取消会释放 Binding permit。
- timeout 同时触发与 global permit 同时到达时只完成一个状态转换。
- 即时路径获取 Binding 成功但 global 失败时立即 Drop Binding permit。
- P1-A 不修改 `SecureUpstreamClient` 的 16 并发行为，不引入第二套上游计数。
- P1-A 合入前后使用相同负载脚本对比吞吐、拒绝码和 permit 峰值，避免修复隔离时造成明显性能退化。

### 4.4 P1-A 测试

- A 达到 4 个 inflight 后，A5 排队但不占用 global permit。
- A 拥堵时，B 在自身额度和 global 额度可用时可以进入执行。
- 即时路径第二个 permit 获取失败时不泄漏第一个 permit。
- 排队超时、Future 取消和 handler 错误都释放全部 permit 和队列计数。
- 单 Binding 队列仍最多 8，全局队列仍最多 64。
- 上游并发达到 16 时返回稳定、可识别的结构化错误。

### 4.5 P1-A 验收

- 繁忙 Binding 的等待请求不占 global permit。
- 不存在持有 global permit 无限等待 binding permit 的路径。
- 所有退出路径均有 permit/queue 计数测试。

---

## 5. P2：统一 Admission Controller 与弹性公平调度

### 5.1 为什么不能长期停留在“颠倒 Semaphore 顺序”

颠倒顺序是正确的最小补丁，但仍有以下限制：

- 不能保证不同 Binding 轮流取得执行机会。
- 单 Binding 的 4 是硬上限，其他 Binding 空闲时无法利用剩余容量。
- 一个 Provider 的 429、延迟和故障可能影响其他 Provider 的全局容量。
- Gateway 32 和上游 16 是两个独立 admission 点，行为不一致。
- 当前请求总 timeout 同时承担排队 timeout，无法给用户快速反馈。
- 流式请求的上游连接生命周期与 Gateway handler 生命周期不同，容易低估真实占用。

最佳目标不是“一次性给请求分配所有资源”，而是按生命周期建立两个明确 admission 阶段，并让每个阶段内部原子决定对应资源：

```text
阶段 A：Gateway 本地处理 admission
  鉴权、Body 限制、解析、协议转换准备

阶段 B：上游 admission
  CapacityKey 公平队列、Binding 公平队列、真实上游连接
```

请求不能在 DNS、Keychain 读取、请求解析或本地转换期间提前占用稀缺上游席位。阶段 A 和阶段 B 使用不同 permit 类型和不同生命周期。

### 5.2 容量模型

保留现有默认硬限制，同时把单 Binding 的 4 从绝对硬上限演进为“竞争时目标额度”：

| 参数 | 初始值 | 语义 |
|---|---:|---|
| Gateway 处理硬上限 | 32 | 鉴权、转换、路由建立等本地工作 |
| 上游连接硬上限 | 16 | 真实活跃的上游请求/流 |
| Binding nominal target | 4 | 正常竞争时的目标额度 |
| Binding minimum progress | 1 | 活跃 Binding 至少持续获得进展 |
| Binding burst limit | 8 | 无竞争时允许借用后的上限 |
| Binding queue limit | 8 | 单 Binding 最大等待请求 |
| Global queue limit | 64 | Gateway 最大等待请求 |
| Global queued body bytes | 待基准测试确定 | 所有等待请求 Body 总字节硬上限 |
| Binding queued body bytes | 待基准测试确定 | 单 Binding 等待 Body 字节硬上限 |

当活跃 Binding 数量过多、总容量不足以保证每个 Binding 都有 4 个请求时，4 不能被描述为绝对保证。调度器应按活跃 Binding 和权重计算公平份额，同时保证每个有资格的 Binding 至少持续取得一个执行机会。

第一次启用 Admission Controller 时，必须保持：

```text
nominal = 4
burst = 4
```

也就是只替换调度机制，不扩大单 Binding 并发。burst 必须在单独 Stage 中启用，先从 5 或 6 开始；只有压力测试和真实指标证明安全后才允许配置到 8。

### 5.3 调度层级

```text
Gateway 本地处理池（32）
  └── 上游全局池（16）
        └── UpstreamCapacityKey 公平轮询
              └── Binding 公平轮询
                    └── 请求（首次请求优先于重试，但带 aging）
```

只在 Binding 内公平不足以防止 Provider A 占满全局 16 并饿死 Provider B，因此需要两级公平：

1. CapacityKey 之间公平。
2. 同一 CapacityKey 内 Binding 之间公平。

### 5.3.1 UpstreamCapacityKey

不能直接只按 Provider ID 隔离容量。用户可以创建多个指向同一上游、同一凭据的 Provider，从而无意或有意绕过限制。也不能只按 hostname 合并，因为同一 hostname 下不同 API Key/账户可能有独立额度。

定义不可包含秘密的稳定容量键，至少考虑：

```text
规范化 scheme + host + port
+ credential source identity
+ protocol/route class
```

credential source identity 使用非秘密引用，例如：

- `env:COMPANY_API_KEY`。
- `keychain:<service>:<account>:<version>` 的不可逆稳定摘要。
- `auth-command:<approval-id>`。
- `none`。

禁止把 API Key、Authorization header、Keychain secret 或完整敏感 query 加入 key、日志或指标 label。

在无法可靠生成 CapacityKey 的兼容路径上，回退到全局上游池，不得错误地为每个 Provider 创建独立 16 个席位。

Provider 更新、credential rotation、Binding rebind/revoke 会改变 CapacityKey。状态迁移规则：

- 已经运行的请求继续使用取得 permit 时的旧 CapacityKey，直到自然结束。
- 新请求使用更新后的 CapacityKey。
- 旧 CapacityKey 无运行、无等待后回收。
- rotation/revoke 不迁移或复用包含 secret 的内存对象。
- CapacityKey 变化不得让同一个 waiter 同时存在于新旧两个队列。

### 5.4 公平队列

使用 Deficit Round Robin 或等价的按 Binding 公平轮询，不使用单个全局 FIFO：

```text
A: A1 A2 A3 A4 ...
B: B1 B2 ...
C: C1 ...

调度：A1 → B1 → C1 → A2 → B2 → A3 ...
```

第一版权重统一为 1。只有出现明确的产品等级或后台任务类型后，才引入权重差异。

重试请求的初始优先级低于首次请求，避免故障期间重试风暴挤压新请求；但必须使用 aging/deadline，避免持续新流量让重试永久饥饿。健康检查和控制通道不进入普通推理队列，但必须有独立、很小且有硬上限的额度，不能无限制绕过保护。

公平调度的单位不能只看“启动了几个请求”。LLM 长流可能运行数分钟，而非流式请求可能数秒结束。第一版至少分别跟踪：

```text
active_streams
active_nonstream
```

并保留 `Binding active stream limit = 4` 的初始硬限制。否则单 Binding 借到 8 个长流后，其他 Binding 只能等待自然结束，无法保证 minimum progress。

Binding 公平不是安全租户隔离。一个用户可以创建多个 Profile/Binding，从而在同一 CapacityKey 内取得更多轮询份额。第一版必须至少保证：

- 无论 Binding 数量多少，CapacityKey 总运行数不能突破其硬上限。
- 每个 CapacityKey 和全局 admission state 的 Binding/queue 数量有硬限制。
- 文档明确公平主体是“活跃 Binding”，不是“自然人/组织租户”。
- 如果未来出现多用户或团队共享 Gateway，必须引入 owner/tenant 层级，不能继续把 Binding 当作租户身份。

### 5.5 有界借用

借用规则：

1. 当前 Binding 已达到 nominal target 4。
2. Gateway 和对应 Provider/上游容量仍有空闲。
3. 没有其他活跃 Binding 低于其公平份额并正在等待。
4. 当前 Binding 未达到 burst limit 8。
5. 调度器在一次锁内原子更新 global、provider 和 binding 计数。

归还规则：

- 已经运行的借用请求不抢占、不强制中断。
- 请求自然结束、取消或失败后通过 guard 自动归还全部计数。
- 一旦其他 Binding 开始等待基础份额，停止向借用者发放新的突发额度。
- 新释放的额度优先分配给低于公平份额的 Binding。

### 5.6 Admission Controller 接口建议

```rust
struct GatewayProcessingController {
    permits: Semaphore,
}

struct UpstreamAdmissionController {
    state: Mutex<AdmissionState>,
    wake: Notify,
}

struct AdmissionState {
    upstream_running: usize,
    capacity_keys: BTreeMap<UpstreamCapacityKey, CapacityAdmissionState>,
    fair_queue: HierarchicalFairQueue,
    queued_requests: usize,
    queued_body_bytes: usize,
}

struct AdmissionPermit {
    controller: Arc<UpstreamAdmissionController>,
    capacity_key: UpstreamCapacityKey,
    binding_id: String,
    class: PermitClass,
    request_kind: RequestKind,
}
```

`AdmissionPermit::drop` 必须归还所有计数并唤醒调度器。请求在等待队列中时不能持有 global、provider 或 binding 执行额度。

上游计数判断和扣减必须在一次临界区内完成，避免再次出现“持有一个额度等待另一个额度”。不得在持有 admission state mutex 时执行 await、DNS、Keychain、文件 I/O、HTTP 或日志落盘。

### 5.6.1 唯一容量真值

当前 `SecureUpstreamClient` 使用 `inflight: Semaphore`，`UpstreamStreamResponse` 持有 `_permit`。引入新 Admission Controller 时，必须确定唯一容量真值：

- 推荐由 `UpstreamAdmissionController` 统一发放上游 permit，并移除旧的独立 `try_acquire_owned()` 限流；或者
- 让公平调度器最终取得同一个共享 Semaphore permit，并把它包装进 `AdmissionPermit`。

禁止同时保留两套互不知情的“上游运行数”，否则会出现新调度器允许、旧 Semaphore 拒绝的双重限流和指标漂移。

### 5.6.2 Notify、取消与状态机

基于 `Mutex + Notify` 的等待必须循环检查状态，防止丢失唤醒：

```rust
loop {
    let notified = controller.wake.notified();
    {
        let mut state = controller.state.lock().await;
        if let Some(permit) = state.try_admit(waiter_id) {
            return Ok(permit);
        }
    }
    notified.await;
}
```

每个 waiter 必须有唯一 ID、generation、deadline 和 cancellation token。以下竞态必须只有一个赢家：

- admission 与 queue timeout 同时发生。
- admission 与客户端取消同时发生。
- shutdown 与 admission 同时发生。
- Binding revoke/delete 与 admission 同时发生。

等待 Future Drop 时必须从队列和所有索引中移除 waiter，并扣减 queued request/body bytes。Provider/Binding/CapacityKey 无运行、无等待后应延迟回收，避免状态 map 随 Binding rotation 无限增长。

### 5.7 排队超时与错误体验

排队 timeout 必须与请求执行 timeout 分离：

- `queue_timeout`：允许请求等待 admission 的时间。
- `first_byte_timeout`：上游必须开始响应的时间。
- `stream_idle_timeout`：流式响应无新数据的最大时间。
- `total_timeout`：整个上游请求的最大生命周期。

第一版建议把 queue timeout 做成配置项，并以较短默认值起步；具体数值在真实 UI/CLI 行为测试后确定，不直接复用 15 分钟总请求 timeout。

队列除请求数量外必须限制 Body 总字节。当前请求 Body 上限为 4 MiB，64 个完整 Body 理论上可保留约 256 MiB，尚未计算解析和转换缓冲。优先方案是在读取完整 Body 前完成轻量 admission；如果架构暂时无法做到，则必须增加 global/per-Binding queued body byte limits，并保证取消时立即释放内存。

排队不能持有 SecretValue、已展开凭据、Authorization header 或 DNS resolved addresses。凭据解析尽量在真正获得上游 admission 后、发请求前完成，并确保不会把 secret 存入 waiter/debug/metrics。

错误至少区分：

- `GATEWAY_GLOBAL_BUSY`。
- `PROVIDER_CONCURRENCY_LIMIT`。
- `BINDING_QUEUE_FULL`。
- `GATEWAY_QUEUE_TIMEOUT`。
- `UPSTREAM_THROTTLED`。
- `UPSTREAM_UNAVAILABLE`。

结构化错误可包含经过脱敏的 `retryAfterMs`、`providerId` 和是否建议重试。不要暴露 Binding Token、凭据、完整上游 URL 或内部路径。

### 5.8 流式请求

必须分别理解两类资源：

- Gateway 本地处理席位：鉴权、协议转换、建立上游请求。
- 上游连接席位：持续到流结束、取消或超时。

流建立后可以释放不再需要的本地处理席位，但不能提前释放上游连接席位。

当前实现把 `UpstreamStreamResponse` 移入 `tokio::spawn` 后台任务，后台任务读取 chunk 并通过 channel 发送给 HTTP Body。因此新 admission permit 必须与真实持有上游响应的后台任务同生命周期，而不能只假设由 Body 对象直接持有。

必须保证以下退出链：

```text
客户端断开
  → Body receiver drop
  → sender 发送失败
  → cancellation.cancel()
  → 后台 stream task 退出
  → UpstreamStreamResponse/AdmissionPermit drop
  → 上游席位归还
```

第一版让流式与非流式请求共享全局上游硬上限，但启用单 Binding 活跃流上限 4。是否进一步为短请求保留席位必须由指标决定。

### 5.9 P2 测试

- Stage 5：Admission Controller 启用且 burst=4 时，单 Binding 不能突破 4。
- Stage 7：启用借用后，单 Binding 无竞争时只能借到当前配置的 burst limit；最终配置为 8 时最多到 8。
- B 开始等待后，A 不再获得新的 burst permit。
- A 已运行的借用请求不会被中断，结束后额度优先给 B。
- 一个 Binding 连续提交大量请求时，其他 Binding 仍持续取得进展。
- 不同 Provider 的队列和错误状态互不阻塞。
- 全局上游运行数永不超过 16，本地处理数永不超过 32。
- 排队请求不持有任何执行计数。
- Future 取消、客户端断连、stream drop、上游超时和 handler panic 路径均归还额度。
- 队列满和排队超时返回正确结构化错误。
- 首次请求在同等条件下优先于重试请求。
- CapacityKey A 占满时，CapacityKey B 仍持续取得全局上游席位。
- 同一 endpoint + 同一 credential identity 的多个 Provider 不能通过不同 ID 绕过容量域。
- 同一 endpoint + 不同 credential identity 不会被错误合并为同一个账户限额，除非配置明确要求共享。
- 排队总请求数和总 Body 字节分别受限。
- timeout、取消、Binding revoke 和 shutdown 竞态不会产生幽灵 waiter。
- 空 CapacityKey/Binding admission state 会被安全回收。
- 客户端首字节前断开、流中断、Body 从未 poll、channel 满和 Gateway shutdown 都会归还上游 permit。
- 新 Admission Controller 启用但 burst=4 时，与旧系统的容量和主要错误行为一致。

### 5.10 P2 验收

- 当前 32/4/16 默认行为在正常竞争场景保持兼容。
- 空闲容量可以被单 Binding 有界借用到 8。
- 多 Binding 竞争时不存在长期饥饿。
- 一个 Provider 的拥堵不会无故阻塞其他 Provider。
- Gateway 层与上游层不再出现两个互相矛盾的 admission 结果。
- 指标能够解释一次请求为何排队、借用、被拒绝或超时。
- 只有一个上游容量真值，不存在新旧双重限流。
- 等待队列不会因 64 个大 Body 造成无界内存增长。
- 长流不会让单 Binding 无限制占用所有上游席位。

---

## 6. P3：按 CapacityKey 的保守自适应并发

自适应并发不作为第一阶段发布阻塞项。必须先有稳定的公平调度和真实指标，再决定是否启用。

每个 UpstreamCapacityKey 独立观察：

- 首字节时间（TTFT）的百分位数和长期基准。
- 429 比例和 `Retry-After`。
- 连接超时、502/503/504。
- 流式响应中断比例。
- 当前 queue wait 和运行中请求数。

初始策略应保守：

- CapacityKey limit 有明确最小值和最大值。
- 成功且延迟稳定时每个窗口最多增加 1。
- 429 或持续延迟恶化时乘法降低，例如乘以 0.5～0.75。
- 进入 bounded cooldown，恢复后缓慢增加。
- 单次慢请求不得触发剧烈降级。
- 自适应状态按 CapacityKey 隔离，不允许一个上游账户/容量域的节流降低所有 Provider 的容量。

重试必须使用有界预算、指数退避和 jitter。POST 请求只有在可以证明请求未发送，或具有可靠幂等标识时才能自动重试，不能对所有断连和 5xx 盲目重放。

---

## 7. P1：Supervisor 进程身份判活

### 7.1 根因

Supervisor 和 Gateway 启动 claim 当前只通过：

```rust
libc::kill(pid, 0) == 0
```

判断 PID 是否存在。旧 Gateway 退出后，如果 PID 被其他进程复用，Supervisor 会误认为 Gateway 仍在运行，不再拉起真实 Gateway。

### 7.2 最佳修复

判活分两层：

1. 进程身份快速检查：PID、当前用户 UID、经过安装清单验证的 executable path。
2. Gateway 身份确认：通过受认证 control channel 或 health proof 验证 `install_id`、`instance_id` 和协议版本。

只验证 executable 仍不足以识别“同一路径启动的新实例”。运行状态应记录可用于区分进程实例的启动标识，优先使用受认证的 `instance_id`/control handshake；如果平台 API 稳定可用，可额外记录 process start time。

Supervisor 必须使用显式状态机，不能把一次 control/health 失败等同于进程死亡：

```text
Healthy
  → Suspect：一次身份或健康检查失败
  → Degraded：连续失败，但进程身份仍匹配
  → Recovering：已确认需要恢复，并验证目标确实是受管 Gateway
  → Stopped：旧实例已确认退出
  → Starting：只有此时才允许启动新实例
```

Supervisor 判定规则：

- PID 不存在：启动 Gateway。
- PID 存在但 UID 或 executable 不匹配：视为 PID 复用，不得终止无关进程，清理陈旧 state 后启动 Gateway。
- 进程身份匹配但 control/health 暂时失败：进入 Suspect/Degraded 并有界重试，不直接启动第二实例。
- 进程身份匹配但经过多次验证后 instance identity 明确不匹配：视为错误实例，进入受验证 recovery 流程。
- 身份和健康证明均匹配：认为 Gateway 正常运行。
- 检查暂时失败：进入有界重试，不立即对未知进程执行 kill。

核心安全规则：

> 无法证明旧 Gateway 已退出时，不得启动新 Gateway；无法证明某 PID 是受管 Gateway 时，不得终止它。

启动新实例前必须同时满足以下条件之一：

- state 中的 PID 已确认不存在。
- PID 已被复用且当前进程明确不是 Gateway；仅清理陈旧 state，不终止无关进程。
- 旧进程已通过 UID、executable、install/instance identity 验证为受管 Gateway，并已完成有界终止且确认退出。

随后还要验证 stable port、control socket 和 state claim 的一致性。端口仍被占用时不得通过清 state 强行启动形成竞争实例。

不要在每个 5 秒轮询中重复进行不必要的 manifest/codesign 全量校验。启动 Supervisor 时先验证安装清单并缓存预期组件身份，轮询时执行轻量身份检查；组件更新后重新加载验证结果。

### 7.3 测试与验收

- PID 不存在时启动。
- PID 存在但 UID 不匹配时不终止该进程。
- PID 存在但 executable 不匹配时判为陈旧 state。
- executable 相同但 instance identity 不匹配时不误判为健康 Gateway。
- 临时检查失败进入 bounded retry。
- 正常 Gateway 不会被 Supervisor 重复拉起。
- 更新后的 Gateway 组件身份能够被重新加载。
- 一次或短暂 control timeout 不会启动第二个 Gateway。
- 旧实例仍占用 stable port/control socket 时不会形成 split-brain。
- 只有完成受验证终止后状态机才能进入 Starting。

---

## 8. P3：拆分 `provider_api_v2.rs`

功能修复完成并有回归测试保护后，再进行纯结构重构。建议拆分：

```text
provider_api_v2/
  mod.rs
  dto.rs
  errors.rs
  provider_crud.rs
  provider_views.rs
  account_plan.rs
  account_transaction.rs
  binding_plan.rs
  binding_execution.rs
  runtime_test.rs
  recovery.rs
```

约束：

- 保持现有 Tauri command 名称和序列化契约不变，明确计划修改的 Provider 列表响应除外。
- 先迁移代码，不在同一提交中改变业务逻辑。
- 每次拆分后运行 Provider V2、Gateway、API Account、前端 store 和组件测试。
- 不复制 repository、snapshot 或错误转换逻辑。

---

## 9. 可观测性要求

Gateway 并发改造至少记录以下脱敏指标：

- 当前 global/provider/binding running。
- global/provider/binding queued。
- queue wait milliseconds。
- permit class：nominal 或 borrowed。
- 拒绝原因和 timeout 阶段。
- TTFT、总时长、status、retry count。
- 上游 429/5xx 和流中断计数。
- queued body bytes、活跃 stream 数和 admission state map 大小。
- Supervisor 当前状态、连续失败次数和最近一次已脱敏状态转换原因。

日志不得记录：

- Binding Token 或其可逆形式。
- Keychain secret、环境变量值或 Authorization header。
- 原始 prompt、tool 参数或完整响应正文。
- 未经脱敏的完整上游 URL/query。

`binding_id`、`provider_id` 和 CapacityKey 如进入请求日志，应继续使用稳定短 hash 或经过审查的非敏感标识。

指标 label 必须限制基数。不得把 request ID、完整 Binding ID、原始 hostname/query、模型自由文本或错误 message 直接作为长期 metrics label。高基数诊断信息放入有界日志，不进入聚合指标。

在修改调度算法前先记录旧实现基线：

- 单 Binding 1/4/5/8 并发时的吞吐和错误码。
- 2、4、8 个 Binding 竞争时的成功率、queue wait 和最大 inflight。
- 16 个长流、长流与短请求混合时的 permit 生命周期。
- 客户端断开、timeout 和 Gateway shutdown 后的残留 task/permit 数。
- RSS、queued body bytes 和文件描述符峰值。

后续每个 Stage 必须与该基线比较，不能只验证“测试通过”。

---

## 10. 分阶段任务编排

### Stage 0：基线、夹具与回滚准备

目标：在任何行为修改前固定当前契约和资源行为。

任务：

- [ ] 记录当前 32/4/64/8/16 配置来源和所有错误码。
- [ ] 建立单 Binding、多 Binding、多 Provider、长流/短请求混合压力夹具。
- [ ] 增加 active tasks、permit、queue count、RSS 和 FD 基线采集。
- [ ] 枚举 `list_providers_v2` 的全部 Rust、TS、脚本和测试调用者。
- [ ] 确认新调度器内部开关和旧调度器回退路径，但不在生产 UI 暴露开关。
- [ ] 为 ProcessInspector、GatewayIdentityProbe 和 Clock 建立可注入测试接口。

进入下一 Stage 的门槛：

- 当前行为可以稳定复现。
- 压力夹具能够检测 permit 泄漏、饥饿和流任务残留。
- 回退路径在测试环境验证可用。

### Stage 1：Provider revision 契约修复

目标：消除空 Provider 集合导致的直接创建持续冲突，不修改 Gateway 并发。

任务：

- [ ] 新增 `ProviderListViewV2`。
- [ ] 抽取不执行 I/O 的 `build_provider_views`。
- [ ] 保证 Provider 内容与 revision 来自同一次 Store 读取。
- [ ] 明确 bindings/health 为最终一致，或显式使用共同 lock guard；不得含糊描述。
- [ ] 按调用者情况选择原命令升级或新增版本化命令。
- [ ] 前端新增 `providerStoreRevision: number | null`。
- [ ] 增加 refresh generation，防止旧响应覆盖新状态。
- [ ] 删除所有从 `providers[0]` 推导 revision 的代码。
- [ ] CAS 冲突改为刷新并重新确认，不做通用自动重试。
- [ ] 补“删除最后一个独占 Provider 后再次创建”的后端和 UI 回归测试。

禁止事项：

- 不修改 Gateway 调度代码。
- 不拆分整个 `provider_api_v2.rs`。
- 不透传全部 `AppError.details`。

回滚点：

- 如果采用新命令，前端可回退到旧命令，但旧命令不得继续承载写操作的 revision 真值。
- 如果原命令原子升级，前后端必须同包回滚，不支持混合版本静默运行。

Stage 验收：见 2.7，并要求所有调用者和契约测试完成更新。

### Stage 2：计划时钟与 Supervisor 状态机

目标：修复独立 UX 问题和恢复可靠性，不修改请求调度。

任务：

- [ ] Dialog 使用可注入、可清理的走动时钟。
- [ ] 抽象 ProcessInspector 和 GatewayIdentityProbe。
- [ ] Supervisor 实现 Healthy/Suspect/Degraded/Recovering/Stopped/Starting 状态机。
- [ ] 加入 UID、executable、install/instance identity 验证。
- [ ] 明确连续失败阈值、bounded retry 和状态转换日志。
- [ ] 启动前验证旧实例退出、stable port、control socket 和 state claim。
- [ ] 补 PID 复用、瞬时 control timeout、未知进程不得误杀和 split-brain 测试。

禁止事项：

- 一次 health/control 失败后不得直接启动新实例。
- 身份不明确时不得 kill。
- 不在每个轮询周期重复完整 codesign 校验。

回滚点：

- 保留旧 supervisor policy 的配置级回退，但 PID 身份不匹配时“不得误杀”是不可回退的安全约束。

### Stage 3：Gateway 最小并发隔离

目标：只消除等待中的繁忙 Binding 占用 global permit，不改变容量。

任务：

- [ ] 即时和排队路径统一为先 Binding、后 global。
- [ ] 覆盖第二 permit 失败、timeout、取消和 handler error 的 RAII 释放。
- [ ] 保持 global=32、binding=4、global queue=64、binding queue=8、upstream=16。
- [ ] 保持现有上游 `try_acquire_owned()` 和错误码，避免在同一 Stage 引入双重行为变化。
- [ ] 与 Stage 0 基线对比吞吐和错误分布。

回滚条件：

- 吞吐出现无法解释的显著下降。
- 出现 permit 泄漏、queue count 负向/漂移或新的 timeout 回归。

### Stage 4：观测与负载验证

目标：在替换调度器前获得决定 CapacityKey、burst、queue timeout 和 stream 限制所需的数据。

任务：

- [ ] 增加脱敏 running/queued/queue bytes/active streams 指标。
- [ ] 记录 TTFT、429、5xx、timeout 阶段和 client cancellation。
- [ ] 对同一 endpoint 的不同 credential identity 验证容量隔离假设。
- [ ] 测试 4 MiB Body × 队列上限时的 RSS。
- [ ] 确定 global/per-Binding queued body byte limits。
- [ ] 确定初始 queue timeout，但不得复用 15 分钟 total timeout。
- [ ] 验证指标 label 基数和日志脱敏。

进入 Stage 5 的门槛：

- 能从指标解释当前拒绝发生在 Gateway、Binding 还是 upstream 层。
- 能可靠检测 client disconnect 后的残留流任务和 permit。
- CapacityKey 设计已通过同凭据合并、不同凭据隔离测试。

### Stage 5：Admission Controller 固定容量模式

目标：替换嵌套 admission，但保持外部容量语义不变。

任务：

- [ ] 实现 GatewayProcessingPermit 和 UpstreamAdmissionPermit 两阶段模型。
- [ ] 定义稳定、非秘密的 UpstreamCapacityKey。
- [ ] 选择唯一上游容量真值，删除或统一旧 upstream Semaphore 计数。
- [ ] waiter 包含 ID、generation、deadline、cancellation 和 body byte cost。
- [ ] 实现 Notify 循环检查、取消删除、shutdown drain 和空 state 回收。
- [ ] burst 固定为 4，暂不允许借用。
- [ ] 实现 request count 和 queued body bytes 双重限制。
- [ ] stream 后台任务持有真实上游 permit 到退出。
- [ ] 内部开关默认先在测试/受控环境启用，保留旧调度器回退。
- [ ] 调度器切换只影响新请求；切换前已取得旧 permit 的请求由旧所有者自然完成，不能跨实现转移 permit。

禁止事项：

- 不启用 burst>4。
- 不启用自适应并发。
- 不同时保留两套独立 upstream running 真值。
- 不在 admission mutex 内执行 await 或 I/O。

回滚条件：

- running 指标与真实活跃上游请求不一致。
- 出现丢失唤醒、幽灵 waiter、shutdown hang 或 stream permit 残留。
- 固定容量模式下错误率或延迟明显劣于 Stage 3 基线。

### Stage 6：两级公平队列

目标：在不扩大并发的情况下实现 CapacityKey 和 Binding 公平。

任务：

- [ ] 实现 CapacityKey 级 DRR/公平轮询。
- [ ] 实现 CapacityKey 内 Binding 级 DRR/公平轮询。
- [ ] 首次请求优先于重试，但加入 aging/deadline 防止重试永久饥饿。
- [ ] 增加 active_streams/active_nonstream 分类和 Binding active stream limit=4。
- [ ] 验证单 Provider、大量 Provider ID 和 Binding rotation 下 state 有界。
- [ ] 使用确定性 fake clock/调度器测试公平性，不依赖真实 sleep。

进入 Stage 7 的门槛：

- 在 burst=4 时，多 CapacityKey、多 Binding 均持续取得进展。
- 长流不会让一个 Binding 占满全部 16 个上游席位。
- 调度顺序、取消和回收测试可确定性重复。

### Stage 7：有界借用渐进启用

目标：只在公平调度稳定后提高空闲资源利用率。

任务：

- [ ] 实现 nominal、borrowed 和 burst limit 明确计数。
- [ ] 初始 burst 从 5 或 6 开始，不直接默认 8。
- [ ] 其他 Binding/CapacityKey 低于公平份额时停止发放新借用额度。
- [ ] 已运行借用请求不抢占，结束后优先归还给等待基础份额者。
- [ ] 分别测量非流式 burst 和流式 burst；活跃流仍受 4 限制。
- [ ] 观察 429、TTFT、queue wait、RSS 和连接数后再决定是否允许 burst=8。

回滚方式：

- 将 burst 配置恢复为 4 即可关闭借用，不回滚公平调度器。

### Stage 8：自适应并发评估

目标：只评估，不默认开启。

任务：

- [ ] 使用真实数据离线回放固定策略与候选自适应策略。
- [ ] 只使用 TTFT、429、明确超时和服务端错误，不使用总生成时长作为唯一负载信号。
- [ ] 自适应状态按 CapacityKey 隔离。
- [ ] 定义最小/最大 limit、增长步长、乘法下降、cooldown 和恢复规则。
- [ ] 证明自适应不会因单次模型冷启动剧烈降级。
- [ ] 通过显式配置灰度启用，默认保持关闭。

### Stage 9：领域拆分

目标：在行为稳定后降低维护成本。

- [ ] 按第 8 节模块边界迁移代码。
- [ ] 每次迁移保持行为和序列化契约不变。
- [ ] 不与调度、自适应或 Provider DTO 变更混合提交。

---

## 11. 发布验收

### 11.1 Stage 1～3 发布门禁

以下条件全部满足后，P0/P1 修正才可视为完成：

- 删除最后一个 External API Account 的独占 Provider 后，可以直接创建新 Provider。
- Provider 空列表跨重启仍携带正确 revision。
- Attach/Detach 到期后 UI 自动禁用执行。
- 一个打满的 Binding 不会提前占用 global permit 阻塞其他 Binding。
- 所有取消、超时和错误路径不泄漏 permit 或队列计数。
- PID 复用不会阻止 Gateway 恢复，也不会误杀无关进程。
- 前端测试、Rust 单元/集成测试、Gateway 契约测试全部通过。
- 受限环境无法执行的 macOS 进程测试已在具备权限的 runner 通过，或明确记录为发布阻塞。
- 没有把 P2 burst、自适应或 DTO 大规模重构混入 P0/P1 发布。

### 11.2 Stage 5 固定容量 Admission 发布门禁

- 新旧调度器在 burst=4 下通过同一套契约和压力夹具。
- 单 Binding、多 Binding、多 CapacityKey 的最大运行数与旧硬限制一致。
- 只有一个上游容量真值。
- 取消、timeout、disconnect、shutdown 后 running/queued 均回到 0。
- 无幽灵 waiter、丢失唤醒、后台 stream task 残留和 state map 无界增长。
- queue count 与 queued body bytes 均受硬限制。
- 新调度器回退开关经过演练，回退不会损坏持久化 Provider/Binding 状态。

### 11.3 Stage 6 公平调度发布门禁

- CapacityKey A 持续高压时，CapacityKey B 仍在有界时间内取得席位。
- 同一 CapacityKey 下，单个 Binding 大队列不会让其他 Binding 长期饥饿。
- 重试低优先级但不会因持续新请求永久饥饿。
- 16 个长流和长短混合压力下，active stream limit 生效。
- 公平性测试使用 fake clock/确定性调度，重复运行结果稳定。

### 11.4 Stage 7 借用发布门禁

- 无竞争时单 Binding 只能借到配置的 burst limit。
- 多 Binding 竞争时每个 Binding 都能持续取得进展。
- 一个 CapacityKey 的拥堵不会扩散到其他 CapacityKey。
- 实际上游并发始终不超过硬上限。
- 用户可以从结构化错误和指标判断请求是在排队、限流、上游节流还是超时。
- burst 从 4 提升后的 429、TTFT、错误率、RSS 和 FD 峰值不超过预先定义的回退阈值。
- 将 burst 动态恢复为 4 后，新借用立即停止，已运行请求自然完成且不泄漏。

### 11.5 Stage 8 自适应启用门禁

- 固定公平策略已经稳定运行并积累足够指标。
- 候选算法通过离线回放、故障注入和冷启动场景。
- 自适应默认关闭，只能按明确配置灰度启用。
- 任一 CapacityKey 的降级不会改变其他 CapacityKey 的 limit。
- 可以一键恢复固定 limit，且恢复不需要重启或修改持久化 Provider 数据。

### 11.6 回滚触发条件

任一 Stage 出现以下情况必须停止扩大灰度并回滚本 Stage：

- running/queued 指标无法与真实请求生命周期对应。
- 出现新的 permit 泄漏、幽灵 waiter、shutdown hang 或 split-brain。
- P95/P99 queue wait、错误率、RSS 或 FD 峰值超过 Stage 0/上一 Stage 预设阈值。
- 同一请求可能被重复发送上游。
- 凭据、Binding Token、原始 prompt 或完整响应进入日志/指标。
- 旧客户端/脚本因契约变化静默产生错误数据，而不是明确失败。

回滚不得通过删除用户 Provider、Binding、Keychain credential 或配置文件完成。调度器回滚只能切换运行时实现/配置，数据契约回滚必须使用兼容迁移或同包版本回滚。

## 12. 测试矩阵

### 12.1 功能与契约

| 场景 | 单元 | 集成 | UI/契约 |
|---|---|---|---|
| 空 Provider 集合携带非零 revision | 必须 | 必须 | 必须 |
| 删除最后一个独占 Provider 后再创建 | 必须 | 必须 | 必须 |
| refresh 乱序 | 必须 | 可选 | 必须 |
| Attach/Detach 时钟到期 | 必须 | 不需要 | 必须 |
| 旧/新列表契约不匹配 | 必须 | 必须 | 必须 |

### 12.2 并发与公平

| 场景 | 断言 |
|---|---|
| A=4，A5 等待，B1 到达 | B1 不被 A5 提前占用的 global permit 阻塞 |
| 单 Binding burst=4 | 永不超过 4 |
| 单 Binding burst=6 | 无竞争最多 6，有竞争停止新借用 |
| CapacityKey A 高压、B 低压 | B 在有界时间取得席位 |
| Binding A 大队列、B 小队列 | B 持续取得进展 |
| 16 个长流 | upstream 永不超过 16，单 Binding stream 不超过 4 |
| 长流 + 短请求 | 短请求不因启动顺序公平而永久饥饿 |
| 多 Provider 同 endpoint/同 credential identity | 合并到同 CapacityKey |
| 同 endpoint/不同 credential identity | 按设计隔离，不泄露 secret |

### 12.3 取消、超时与资源释放

- 等待 admission 时客户端取消。
- admission 与 timeout 同时触发。
- admission 与 Binding revoke 同时触发。
- 首字节前客户端断开。
- 流式响应中途断开。
- Body 从未 poll。
- channel 满时 receiver drop。
- 上游 idle timeout 和 total timeout。
- Gateway graceful shutdown 和强制进程退出恢复。
- handler/adapter task panic 或提前返回。

每项都必须断言：

```text
running count 恢复
queued count/bytes 恢复
waiter 从索引移除
后台 task 退出
Secret 不进入错误或日志
```

### 12.4 Supervisor 故障注入

- PID 不存在。
- PID 被无关进程复用。
- UID 不匹配。
- executable 不匹配。
- executable 相同但 instance identity 不匹配。
- 单次 control timeout。
- 连续 control timeout 后旧 Gateway 仍占端口。
- 受验证 Gateway 拒绝优雅终止。
- 组件升级导致预期 executable identity 变化。
- state 写入失败或 revision conflict。

### 12.5 性能与容量

- 1/4/5/8/16/32 并发阶梯。
- 1、2、4、8、32 个 Binding。
- 1、2、4、8 个 CapacityKey。
- 小 Body 与接近 4 MiB Body。
- 非流式、流式、工具调用、多轮 follow-up。
- 正常上游、慢首字节、429、503、连接失败、流中断。

记录吞吐、P50/P95/P99 queue wait、TTFT、总时长、RSS、FD、task 数、拒绝码和 retry count。

## 13. 测试环境说明

现有两项账号删除 Rust 集成测试在受限 macOS 沙箱中可能因进程扫描返回 `Operation not permitted` 而失败。该现象不能直接判定为业务失败，但说明进程扫描依赖真实系统权限。

后续应把进程检查抽象为可注入接口：

- 单元测试使用 fake process inspector。
- 平台集成测试在具备必要权限的 macOS runner 执行。
- 受限环境仍应运行其余 Provider/Gateway 测试，不能因为平台测试受限而跳过整个测试集。
