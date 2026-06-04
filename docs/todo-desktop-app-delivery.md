# Todo List: Desktop App Delivery

| 任务 | 验收标准 | 状态 |
|------|----------|------|
| 核对剩余计划与交付范围 | 对 `docs/TODO.md`、`docs/FINAL-DESIGN.md`、原型和当前代码做一致性审查，明确本轮必须完成项与后续阶段项 | 验证成功 |
| Makefile 本地启动入口 | 根目录 `Makefile` 提供 `make start` / `make app` / `make stop` / `make check` 等入口，`make start` 直接启动 Tauri 桌面 app | 验证成功 |
| Phase 1.2 Usage quota | 后端提供 quota snapshot command，前端展示真实来源不可用/估算状态，不伪造 reset 或剩余额度 | 验证成功 |
| Phase 1.5 Provider/Secret | 后端提供 Provider CRUD、secret 非明文返回、Attach Provider plan/execute；前端可管理 provider 并绑定 profile | 验证成功 |
| 桌面 app 可启动验证 | 通过项目命令验证前端构建、Tauri 配置识别、Rust command 编译和测试均通过 | 验证成功 |
| TODO 与 README 状态收敛 | 文档只保留真实未完成项，启动说明与 Makefile 一致，不再把 Xcode bundle 前置条件误写成 dev 启动阻塞 | 验证成功 |
| 最终交付检查 | 本 todo 全部为 `验证成功`，并给出可直接运行的命令与剩余后续阶段说明 | 验证成功 |
