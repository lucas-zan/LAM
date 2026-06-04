# Todo List: Startup Quota Performance

| 任务 | 验收标准 | 状态 |
|------|----------|------|
| 首屏加载解耦 quota | `refresh()` 先展示 health/accounts/sessions/providers，不等待 quota 完成 | 验证成功 |
| quota 并发刷新 | 多账号 quota 刷新不再通过单个 `refreshAllQuotas` 串行等待，每个账号独立完成/失败 | 验证成功 |
| 保留手动刷新体验 | 单账号刷新按钮仍显示 loading 状态，并能更新对应 quota | 验证成功 |
| smoke 覆盖 | UI smoke 覆盖后台刷新和并发 quota 逻辑 | 验证成功 |
| 验证收敛 | `make check` 通过 | 验证成功 |
