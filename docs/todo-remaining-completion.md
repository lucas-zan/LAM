# Todo List: Remaining Completion

| 任务 | 验收标准 | 状态 |
|------|----------|------|
| Provider 删除使用中阻止 | 删除 provider 前扫描 profile config，已绑定时返回结构化错误；测试覆盖阻止与未使用删除 | 验证成功 |
| Provider mismatch session 模型 | `CodexSession` 返回 original/current provider/model/mismatch；UI 和 resume preview 展示 mismatch 警告 | 验证成功 |
| Codex app-server quota adapter | quota service 可受控尝试 app-server rate limit；失败时明确 fallback，不挂起、不伪造真实额度 | 验证成功 |
| Keychain secret 边界验证 | Keychain 写入失败返回可恢复错误，provider store/UI 不泄露 secret；测试覆盖失败路径 | 验证成功 |
| 文档与验证收敛 | `make check` 通过，TODO/README 只保留真实后续阶段项 | 验证成功 |
