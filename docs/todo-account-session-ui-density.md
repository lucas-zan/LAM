# Todo List: Account And Session UI Density

| 任务 | 验收标准 | 状态 |
|------|----------|------|
| 修复 Sessions 表格越界 | Actions 列按钮在表格容器内完整显示，不挤出右侧边界 | 验证成功 |
| 优化账号卡片密度 | Overview 账号卡片桌面端每行两个、尺寸更紧凑、有轻微背景区分 | 验证成功 |
| 提高登录状态对比度 | logged in / login needed badge 在亮色主题下清晰可读 | 验证成功 |
| 最近活跃排序 | 账号卡片按 `latestSessionModifiedAt` 降序展示，无数据时使用 session 数兜底 | 验证成功 |
| 验证收敛 | `make check` 通过 | 验证成功 |
