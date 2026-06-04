# Todo List: Quota Scheduler

| 任务 | 验收标准 | 状态 |
|------|----------|------|
| 启动不立即抢 usage | 启动只扫描账号/session/provider 并读取 cached quota，不立刻启动真实 quota 请求 | 验证成功 |
| 定时延迟刷新真实 usage | 启动后延迟触发真实 quota，之后按定时器刷新 | 验证成功 |
| Refresh 不触发 usage 风暴 | Refresh 按钮只刷新账号/session/provider/cache，并重新调度 usage，不直接等待或立即并发 app-server | 验证成功 |
| 防止 quota 刷新重叠 | 同一时间只允许一个真实 quota 批次运行 | 验证成功 |
| 验证收敛 | `make check` 通过 | 验证成功 |
