# Todo List

| 任务 | 验收标准 | 状态 |
|------|----------|------|
| 修复 DMG 启动 quota 实时读取 | 打包安装启动时 quota 服务能使用与 `make start` 等价的 Codex 运行时路径，并有测试覆盖 dev/prod 路径解析 | 验证成功 |
| 暴露 quota 实时失败根因 | 账号实时 quota 失败时返回包含 app-server 真实错误的 warning，避免只显示 cached quota | 验证成功 |
| 应用显示名统一为 LAM | Tauri 产品名、窗口标题、菜单文案不再使用 LocalAgentManager 全称 | 验证成功 |
| 清理 UI 旧 quota 错误 | 安装版 app 后端刷新 fresh 后，主页面不再保留旧的 realtime quota unavailable 错误 | 验证成功 |
| 修复手动刷新 quota 超时回缓存 | Codex app-server 延迟返回时手动 Refresh 等到 fresh 结果，不再立即显示 using cached quota | 验证成功 |
