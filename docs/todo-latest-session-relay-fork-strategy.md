# Todo List

| 任务 | 验收标准 | 状态 |
|------|----------|------|
| 接力策略支持分叉设置 | 设置中可选择分叉策略；Rust 接力请求接收策略；分叉时支持 stop、prefer_source、prefer_target、timeline_merge_to_fork、summarize_fork_with_target_account 的安全版本 | 验证成功 |
| Resume Here 默认使用全局最新活跃 session | app 自动定位所有账号最新活跃 session，Overview 显示活跃账号和 session id，Resume Here 使用该 session 而不是隐式选中 session | 验证成功 |
| 托盘显示活跃源并支持接力按钮 | 右上角插件显示活跃账号/session id；每个账号行右侧有切换/接力按钮；按钮调用同一接力流程 | 验证成功 |
| quota 两分钟定时刷新 | app 和托盘插件都按 2 分钟刷新 quota，并保留手动刷新反馈 | 验证成功 |
| 更新测试和 smoke 验证 | Rust 测试覆盖分叉策略；UI smoke 覆盖设置、活跃 session、托盘接力按钮、2 分钟刷新 | 验证成功 |
