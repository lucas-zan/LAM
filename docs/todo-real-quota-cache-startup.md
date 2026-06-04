# Todo List: Real Quota Cache And Startup

| 任务 | 验收标准 | 状态 |
|------|----------|------|
| Real Quota 缓存只保存真实来源 | app-server 成功返回的 `app_server_rate_limits` 写入缓存，activity estimate 不作为 real quota 缓存 | 验证成功 |
| 快速读取缓存 | 新增轻量缓存读取能力，启动时不启动 Codex、不扫 session 即可显示最近真实 quota | 验证成功 |
| 启动加载更快 | App 首屏先加载账号/session/provider，再异步加载缓存和后台刷新真实 quota | 验证成功 |
| 测试与 smoke 覆盖 | Rust 测试覆盖 real cache，UI smoke 覆盖缓存启动路径 | 验证成功 |
| 验证收敛 | `make check` 通过 | 验证成功 |
