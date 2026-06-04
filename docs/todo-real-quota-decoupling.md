# Todo List: Real Quota Decoupling

| 任务 | 验收标准 | 状态 |
|------|----------|------|
| usage 不扫描 session | `get_profile_quota` 不再通过 session 文件计算 activity estimate | 验证成功 |
| usage 不依赖 list_accounts | quota 刷新只解析目标 profile home，不调用完整账号扫描 | 验证成功 |
| cached real fallback | 真实 quota 获取失败时优先返回 cached real quota，无缓存才返回 unavailable | 验证成功 |
| 测试更新 | Rust 测试覆盖 unavailable/cached real 行为 | 验证成功 |
| 验证收敛 | `make check` 通过 | 验证成功 |
