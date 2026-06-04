# Todo List: Phase 0 + Phase 1 Implementation

| 任务 | 验收标准 | 状态 |
|------|----------|------|
| Phase 0 文档与追踪收敛 | `FINAL-DESIGN.md`、`IMPLEMENTATION-ISSUES.md`、`TODO.md` 已作为权威入口，旧原型被标记为 Phase 0 参考 | 验证成功 |
| Phase 1 核心模型与扫描服务 | 测试覆盖 Codex account 扫描、metadata 兼容、session 解析、auth 只检查存在性 | 验证成功（Rust） |
| Phase 1 受管账号与 relay 创建 | 测试覆盖 plan/execute、命名校验、wrapper、metadata、relay 不修改 runtime profile | 验证成功（Rust） |
| Phase 1 safe sync 与 manifest | 测试覆盖 dry-run 不写文件、备份目标 sessions、阻止 auth/config/sqlite/cache/log/tmp、写 manifest、无 history merge | 验证成功（Rust） |
| Phase 1 resume command 与 shell escape | 测试覆盖 cwd/session/profile 参数转义、无 cwd fallback、前端不能传任意 shell 字符串 | 验证成功（Rust） |
| Phase 1 桌面 UI scaffold | 存在 Lam shell、主要 route、Phase 1.2/1.5 能力标注为未实现或只读，并接真实 API wrapper | 验证成功（前端构建 + UI smoke） |
| Phase 1 文档与验证 | README 说明运行方式与安全边界，相关测试全部通过，TODO 状态更新为验证成功 | 部分完成（TODO-504 手工发布验收未执行） |
| **Phase 1F 实现纠偏** | Tauri 接入、command 层、GUI 接线、fixtures、前端测试；详见 `docs/TODO.md` Phase 1F（TODO-104–118） | 部分完成（TODO-104 native window 手工验收未完成） |

| **Phase 1G 原型对齐** | 见 `docs/CORRECTION-PLAN.md`（TODO-119–124） | 未开始 |
| **启动修复 TODO-125** | UTF-8 session 摘要截断；`make start` 不再 panic | 已修复 |

**说明（2026-06-02）**：纠偏顺序见 `docs/CORRECTION-PLAN.md`；Tauri vs Electron 见 `docs/DESKTOP-RUNTIME.md`；504 验收填 `docs/PHASE1-ACCEPTANCE.md`。
