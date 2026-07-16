# RPG-202: Adapter registry and exchange

- **Status**: 验证成功
- **Depends on**: RPG-201

## Logic design

- `ProtocolAdapter: Send + Sync` 为 registry 持有的不可变 factory；`AdapterExchange: Send` 为单请求状态。
- registry 校验 adapter ID、source/target protocol、semantic version 与 compatibility policy。
- ID/clock 由 factory 注入；exchange 隔离 fragment、terminal 与 lifecycle。
- adapter 不持有 credential、HTTP client、retry loop、logger body 或 network cancel handle。

## Test and acceptance

- [x] 先写 registry/exchange 红测；首次运行因 `adapters::registry` 不存在而按预期失败（E0432）。
- [x] 覆盖注册、重复、缺失、protocol/policy mismatch。
- [x] Tokio 并发 exchange ID/fragment/state 不串扰；finish/cancel/post-terminal 行为稳定。
- [x] 不依赖 reqwest/Gateway；focused test 3 passed。

## Done criteria

红测记录、并发与生命周期通过后状态才可为 `验证成功`。
