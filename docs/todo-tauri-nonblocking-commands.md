# Todo List: Tauri Nonblocking Commands

| 任务 | 验收标准 | 状态 |
|------|----------|------|
| 账号/session 扫描后台化 | `list_accounts` / `list_sessions` 不在 Tauri 主线程同步执行 | 验证成功 |
| quota 后台化 | `get_profile_quota` / `refresh_all_quotas` / `list_cached_quotas` 不阻塞 UI 操作 | 验证成功 |
| provider 读取后台化 | refresh 依赖的 `list_providers` 不阻塞启动和 Refresh 按钮 | 验证成功 |
| smoke 覆盖 | 检查关键 command 使用 `spawn_blocking` | 验证成功 |
| 验证收敛 | `make check` 通过 | 验证成功 |
