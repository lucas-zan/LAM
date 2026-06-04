# Todo List: Relay Resume

| 任务 | 验收标准 | 状态 |
|------|----------|------|
| 后端单 session 接力 | 新增接力请求/结果和测试，目标无 session 时复制，目标为源前缀时补齐，目标已更新时跳过，分叉时备份并拒绝 | 验证成功 |
| Tauri/API 接口 | 前端 API 能调用 `relay_resume_session`，类型覆盖请求和结果 | 验证成功 |
| 前端入口与样式 | 账号卡片提供统一风格的 `Resume Here` 操作，使用当前选中 session 接力并打开目标账号 resume | 验证成功 |
| 验证收敛 | 相关 Rust 测试与 UI smoke/check 通过 | 验证成功 |
