# Todo List: Resume ID Fix

| 任务 | 验收标准 | 状态 |
|------|----------|------|
| 修复 resume ID 解析 | `list_sessions` 展示/传给 `codex resume` 的 ID 使用 Codex 可恢复的真实 session id，覆盖 rollout 文件名场景 | 验证成功 |
| 验证收敛 | 相关 Rust 测试与 `make check` 通过 | 验证成功 |
