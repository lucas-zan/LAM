# Todo List: Quota Card Responsive

| 任务 | 验收标准 | 状态 |
|------|----------|------|
| quota 降级显示收敛 | 无真实 usage 时只显示 Session/Weekly 为 `N/A`，不展示 estimate token 和长错误说明 | 验证成功 |
| 卡片自适应修复 | 缩窄窗口时 quota 区块不撑出账号卡片，长 provider/source 文本可换行或截断 | 验证成功 |
| smoke 覆盖更新 | UI smoke 覆盖 N/A 降级与移除 estimate 文案 | 验证成功 |
| 验证收敛 | `make check` 通过 | 验证成功 |
