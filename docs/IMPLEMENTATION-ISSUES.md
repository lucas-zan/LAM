# LocalAgentManager (Lam) Implementation Issues

版本：0.1
日期：2026-06-01
状态：实施任务清单

本文把 `FINAL-DESIGN.md` 拆成可执行 issue。主规格始终以 `docs/FINAL-DESIGN.md` 为准；旧 Codex-only 文档只作历史参考。

## 里程碑

| Milestone | 目标 | 版本 |
|---|---|---|
| M0: Spec Cleanup | 消除文档冲突，冻结主规格 | docs-only |
| M1: Desktop Scaffold | 初始化 Tauri + React 桌面 app | pre-v0.1 |
| M2: Codex Read Model | 扫描账号与 sessions，真实数据只读展示 | pre-v0.1 |
| M3: Safe Relay MVP | 创建账号/relay、安全 sync、resume 闭环 | v0.1.0 |
| M4: Usage Quota | Codex 实时额度与 fallback activity estimate | v0.1.2 |
| M5: Provider & Secret | Provider CRUD、Keychain、Attach、mismatch | v0.2.0 |
| M6: Agent Abstraction | AgentAdapter 泛化，为多 Agent 做准备 | v0.3.0 |

## Issue 列表

### LAM-001: 文档权威关系与旧草稿标记

类型：docs
优先级：P0
Milestone：M0

范围：
- `FINAL-DESIGN.md` 是唯一主规格。
- `01-product-design.md`、`02-development-design.md`、`04-roadmap.md` 顶部标记为历史草稿。
- `05-tauri-command-contracts.md` 标记为基础契约草案。
- `codex-session-manager-managed/README.md` 标记为 Phase 0 Node/Bun 原型。

验收：
- 新成员能从文档顶部明确知道应以哪个文件为准。
- 不再把旧 `Codex Relay / Codex Session Manager` 命名当作正式产品名。

### LAM-002: 冻结 Phase 1 / 1.2 / 1.5 范围

类型：docs / product
优先级：P0
Milestone：M0

范围：
- Phase 1 只做 Codex safe relay MVP。
- Phase 1 不做 Provider CRUD、Keychain 写入、真实额度倒计时、history merge。
- Phase 1.2 做 UsageQuota。
- Phase 1.5 做 Provider & Secret。

验收：
- `DESIGN_GOAL.md` 与 `FINAL-DESIGN.md` 对阶段范围不再互相冲突。
- `FINAL-DESIGN.md` §9 的验收项可直接映射到 issue。

### LAM-003: 初始化 Tauri + React 项目骨架

类型：engineering
优先级：P0
Milestone：M1

范围：
- 创建 `apps/desktop`。
- Tauri v2 + React + Vite + TypeScript。
- Rust 后端模块结构：`commands/`、`services/`、`adapters/codex/`、`models.rs`、`errors.rs`。
- 前端基础布局：Sidebar、Main route、Inspector、Statusbar。

验收：
- `pnpm` 或项目约定命令可启动桌面开发环境。
- 空 app 可打开，包含 Lam shell 和主导航。
- Rust command invoke 能返回 mock health check。

### LAM-004: 设计 token 与主原型落地

类型：frontend
优先级：P1
Milestone：M1

范围：
- 从 `account-manager/codex-manager/index.html` 提取颜色、字体、spacing、radius、shadow。
- 实现 Overview / Accounts / Sessions / Relay / Providers / Sync / Settings 的 route shell。
- Provider 和 Usage 超出 Phase 1 的操作按钮先禁用或标注 Phase 1.2 / 1.5。

验收：
- 视觉与 `codex-manager` 主原型一致。
- Phase 1 不可用能力不会误导用户以为已实现。

### LAM-005: CodexAccount / AgentProfile Phase 1 模型

类型：backend
优先级：P0
Milestone：M2

范围：
- 实现 `CodexAccount`，字段按 `FINAL-DESIGN.md` Phase 1 映射。
- 扫描 `$HOME/.codex`、`$HOME/.codex-*`。
- 读取新 metadata `.managed-by-agent-workspace.json`。
- 兼容旧 metadata `.managed-by-codex-session-manager.json`，但新写入只用新名。

验收：
- 能识别 main、普通账号、relay 账号。
- 不读取 `auth.json` 内容，只检查存在性。
- 单元测试覆盖命名、路径、metadata 兼容。

### LAM-006: Codex session parser

类型：backend
优先级：P0
Milestone：M2

范围：
- 遍历 `CODEX_HOME/sessions/`。
- 保守解析 session id、cwd、summary、first_user_message、modified_at、size。
- 解析失败时 fallback 到文件名和文件 mtime。
- 不修改 session 文件。

验收：
- 对 fake fixture 可返回稳定 session 列表。
- 对未知 JSONL 格式不崩溃。
- 不读取超大文件全文，设置合理读取上限。

### LAM-007: Accounts / Sessions 真实数据 UI

类型：frontend
优先级：P0
Milestone：M2

范围：
- 通过 Tauri invoke 加载 accounts 和 sessions。
- Accounts 显示 login/config/history/session count/managed/relay。
- Sessions 支持按 account、cwd、session id、summary 搜索与排序。
- Inspector 展示选中 session 基础信息。

验收：
- UI 可展示本机真实 `~/.codex*`。
- 未识别 cwd 时显示明确状态。
- 没有真实数据时有 empty state。

### LAM-008: 创建受管 Codex account

类型：backend / frontend
优先级：P0
Milestone：M3

范围：
- `plan_create_account` / `execute_create_account`。
- 校验 suffix：`[a-zA-Z0-9][a-zA-Z0-9_-]{0,31}`。
- 创建 `~/.codex-xxx`，权限 `0700`。
- 写 `.managed-by-agent-workspace.json`。
- 生成 `~/bin/codex-xxx` wrapper。
- 可选复制安全 config 模板，但不复制 auth。

验收：
- dry-run 展示将创建/写入路径。
- wrapper 参数透传。
- 不创建、不复制 `auth.json`。

### LAM-009: 创建 Relay Workspace

类型：backend / frontend
优先级：P0
Milestone：M3

范围：
- `plan_create_relay` / `execute_create_relay`。
- 选择 runtime profile 和 source profile。
- 默认命名 `runtime-relay-source`。
- 写入 relay metadata：runtime/source/provider_policy。
- 生成 relay wrapper。

验收：
- 创建 `~/.codex-b-relay-a` 不修改 `~/.codex-b`。
- UI 明确展示 runtime 身份和 source sessions 的边界。

### LAM-010: Safe Sync Engine 双阶段实现

类型：backend
优先级：P0
Milestone：M3

范围：
- `build_sync_plan(req)` 只返回 operations、warnings、blocked_files。
- `execute_sync(req)` 必须基于已确认 plan。
- 默认只同步 `sessions/`。
- 同步前备份目标 `sessions/`。
- 跳过或阻止 `auth.json`、API key 文件、`*.sqlite*`、`cache/`、`tmp/`、`logs/`、`installation_id`。
- 写 `~/.config/agent-workspace/sync-manifests/<uuid>.json`。
- Phase 1 不实现 history merge。

验收：
- 自动化测试证明 `auth.json` 不会复制。
- dry-run 不写文件。
- execute 后存在 manifest 和 backup。
- 目标已有较新/同大小 session 时按策略 skip。

### LAM-011: Sync Sessions Safely UI

类型：frontend
优先级：P0
Milestone：M3

范围：
- from/to 选择器。
- dry-run 结果展示：copy / skip / backup / blocked。
- 二次确认后才执行。
- history 只提供 sidecar backup 选项；不提供 merge 选项。
- Provider mismatch 位暂时只读展示或 placeholder。

验收：
- 用户能在执行前看见所有将改动路径。
- UI 没有 “直接开始同步” 的危险路径。

### LAM-012: Resume command builder 与 Terminal launcher

类型：backend / frontend
优先级：P0
Milestone：M3

范围：
- 后端构造 resume command，不接受前端任意 shell 字符串。
- 所有参数 shell escape。
- 支持 Copy command。
- 支持 Terminal.app。
- Terminal 权限失败时 fallback 到复制命令提示。

验收：
- 特殊字符路径和 session id 不造成命令注入。
- 无 cwd 时使用 `resume --last --all` 或提示用户选择 cwd。

### LAM-013: Phase 1 Provider 只读解析

类型：backend / frontend
优先级：P1
Milestone：M3

范围：
- 从 Codex `config.toml` 保守解析 provider/model/auth mode。
- Provider 页面 Phase 1 显示只读状态或 Coming in 1.5。
- Add/Test/Attach 禁用或显示 Phase 1.5。

验收：
- 不写 `config.toml`。
- 不读取或展示 API key 明文。
- Session / Account badge 不伪造 provider 信息。

### LAM-014: Phase 1 Usage activity estimate

类型：backend / frontend
优先级：P2
Milestone：M3

范围：
- 从 session jsonl 最近 token_count 事件估算 activity。
- UI 标注 `activity estimate` / `stale`。
- 不展示实时剩余额度或重置倒计时。

验收：
- 离线可用。
- 不把 estimate 展示成 `% left` 或 `Resets in`。

### LAM-015: Phase 1 测试与假数据 fixtures

类型：testing
优先级：P0
Milestone：M3

范围：
- `.fake-home/.codex-*` fixtures。
- Rust 单元测试：命名、路径、wrapper、escape、sync blacklist、session parser。
- 集成测试：A -> b-relay-a sync、backup、manifest、resume command。

验收：
- CI 或本地测试命令一键通过。
- 覆盖最关键安全策略。

### LAM-016: README 与开源安全说明

类型：docs
优先级：P1
Milestone：M3

范围：
- 安装、运行、Codex CLI 前置要求。
- 安全声明：不绕过额度、不复制 auth、不上传 session。
- Relay 推荐流程。
- Phase 1 / 1.2 / 1.5 功能边界。

验收：
- 新用户 5 分钟内能跑起 app。
- README 明确 warning history merge 不支持。

### LAM-017: UsageQuotaService

类型：backend
优先级：P1
Milestone：M4

范围：
- `get_profile_quota(profile_id, force_refresh)`。
- `refresh_all_quotas(profile_ids?)`。
- 优先通过 `CODEX_HOME=... codex app-server` JSON-RPC `account/rateLimits/read`。
- fallback jsonl activity estimate。
- cache TTL 默认 5 分钟。
- 实验性 wham/usage 默认关闭。

验收：
- 两个 profile 独立刷新，不串 auth。
- 未登录返回 `source=unavailable`。
- token 不进入日志、前端、cache、manifest。

### LAM-018: Usage quota UI

类型：frontend
优先级：P1
Milestone：M4

范围：
- Accounts 卡片、侧栏 Quick accounts、Overview、Inspector 额度条。
- Session / Weekly 窗口、used%、remaining%、reset countdown。
- Refresh quotas。
- warn/critical 阈值与建议切换/relay。

验收：
- 离线或 stale 时不显示伪造 0%。
- 未登录显示需要 login。

### LAM-019: ProviderStore + SecretStore

类型：backend
优先级：P1
Milestone：M5

范围：
- `providers.json` 保存非敏感元数据。
- macOS Keychain 保存 provider secret。
- Provider CRUD。
- Test provider。
- Attach provider to profile，写 config 引用，不写明文 key。

验收：
- UI 和日志无明文 key。
- 删除 provider 前检测 used_by_profile_ids。

### LAM-020: Provider-aware sync/resume

类型：frontend / backend
优先级：P1
Milestone：M5

范围：
- Session 显示 original/current provider。
- Sync / Resume 展示 mismatch 警告。
- Inspector 展示 source/target/provider/cwd/side effects。

验收：
- mismatch 不阻止 resume，但必须二次警告。
- Resume command 不包含明文 API key。

### LAM-021: AgentAdapter 抽象

类型：architecture
优先级：P2
Milestone：M6

范围：
- `AgentAdapter` trait。
- `AgentProfile` / `AgentSession` 泛型化。
- `CodexAdapter` 迁入 adapter。
- Command API 兼容层或版本化。

验收：
- Codex 功能不回归。
- 新 Agent 不需要改 SyncEngine 核心。

## 推荐实施顺序

1. 完成 M0 文档收敛，冻结 `FINAL-DESIGN.md`。
2. 做 M1 app skeleton 和设计 token，避免 UI 后期大改。
3. 先实现 M2 只读扫描与 session parser，拿到真实数据。
4. 集中完成 M3 safe relay 闭环，这是 v0.1.0 的核心价值。
5. v0.1.0 稳定后再接 M4 UsageQuota，避免把非官方/不稳定额度来源拖进 MVP。
6. M5 Provider 写能力最后做，因为它牵涉 Keychain、config 写入和更高安全边界。
7. M6 再抽 AgentAdapter，避免在 Codex 行为未稳定前过早泛化。
