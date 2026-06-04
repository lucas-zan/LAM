# Phase 1 纠偏与原型对齐计划

版本：0.1  
日期：2026-06-02  
状态：执行中

本文记录 `apps/desktop` 相对 `docs/FINAL-DESIGN.md`、`account-manager/codex-manager` 原型及 `docs/TODO.md` 的缺口、修正顺序与验收方式。与 `docs/TODO.md` 中的 Phase 1F / Phase 1G 任务编号对应。

## 1. 审查结论摘要

| 维度 | 结论 |
|------|------|
| Rust 核心（扫描、sync、resume、Provider 1.5） | `cargo test` 已通过；安全策略有集成测试 |
| Tauri 桌面 | 工程已接入；`make start` 可启动（需本机确认窗口） |
| Phase 1 §9.1 八条验收 | 部分满足；**504 手工矩阵**未闭环 |
| 原型 UI（额度条、时间线、跨账号 Sessions 等） | **未完全对齐** |
| Phase 1.2 真实额度 | `activity_estimate` 已有；**app-server rateLimits 稳定解析**仍为 `[~]` |

**在 App 里能直接看到的差距**：无 Overview 活动时间线、Sessions 仅当前选中账号、Sync dry-run 为 JSON 而非分组列表、无 Session/Weekly 进度条、Settings 仅 health。

**不能单靠 App 确认的**：`auth.json` 不复制、manifest 格式、Keychain 真机写入等——需测试 + 看磁盘。

## 2. 我能修正 vs 需本机配合

| 类型 | 仓库内可改 | 需你本机 |
|------|------------|----------|
| UTF-8 session 摘要截断 panic | ✅ | — |
| 跨账号 Sessions、Sync 分组 UI、Overview/Settings/Inspector | ✅ | — |
| app-server 额度 JSON-RPC（TODO-601） | ✅（受 Codex CLI 约束） | 已登录 Codex + 可选环境变量 |
| TODO-504 发布验收 | 清单 + 文档模板 | 亲手点 App、检查 `~/.codex-*` |
| TODO-104 原生窗口 | 脚本/配置 | `make start` 目视 |
| Keychain 成功写入 | 代码已有 | 系统权限对话框 |
| Phase 2 AgentAdapter（TODO-801） | 单独里程碑 | — |

## 3. 待修正点

### A. Phase 1 产品缺口（FINAL-DESIGN §9.1）

| ID | 待修正点 | 现状 |
|----|----------|------|
| A1 | Sessions **跨 profile** | 仅 `listSessions(当前账号)` |
| A2 | Sessions **筛选**（账号、cwd、Relay） | 仅全文搜索 |
| A3 | Sync dry-run **分组展示** | `JSON.stringify` |
| A4 | Overview **活动时间线** | 无 |
| A5 | Overview **最近账号**节奏 | 部分 metric |
| A6 | **Settings**（binary、wrapper、sync 默认策略） | 仅 health |
| A7 | Inspector **side effects** 列表 | 部分字段 |
| A8 | `App.tsx` **死代码**清理 | 底部重复组件 |

### B. 原型额度 UI（PROTOTYPE-CHANGES.md v1.1）

| ID | 待修正点 | 依赖 |
|----|----------|------|
| B1 | Account 卡片 Session/Weekly 进度条 | C1 |
| B2 | 侧栏 Quick Accounts 额度迷你条 | B1 |
| B3 | Inspector Usage limits 面板 | B1 |
| B4 | Settings 配额刷新/阈值 | B1、A6 |
| B5 | Toolbar Refresh quotas 布局对齐 | 部分已有 |

### C. 后端 / 契约

| ID | 待修正点 |
|----|----------|
| C1 | 稳定 `codex app-server` → `account/rateLimits/read` |
| C2 | 未登录 → `source=unavailable`，不显示假 0% |
| C3 | TODO-602 标记与实现一致 |

### D. 架构与发布

| ID | 待修正点 |
|----|----------|
| D1 | `adapters/codex/` 迁出或文档说明 |
| D2 | TODO-801 AgentAdapter（Phase 2） |
| D3 | TODO-504 手工验收记录 |
| D4 | TODO-104 关闭（本机 `make start` 确认） |

## 4. 修正顺序（波次）

```text
波次 0  工程卫生（A8、文档、smoke）
   ↓
波次 1  Phase 1 数据与核心 UX（A1→A3）← 阻塞 §9.1 第 5 条在 App 内的观感
   ↓
波次 2  FINAL-DESIGN / 原型壳层（A4→A7）
   ↓
波次 3  真实额度后端（C1→C2）TODO-601
   ↓
波次 4  原型额度 UI（B1→B5）TODO-602 对齐
   ↓
波次 5  发布验收（D3、D4）TODO-504
   ↓
波次 6  Phase 2（D2）TODO-801
```

### 波次 0 — 工程卫生（优先）

1. **A8** 删除 `App.tsx` 未使用重复组件。  
2. **启动修复**：`short_text` 按 **字符** 截断 UTF-8（修复 `make start` 在含中文 session 时 panic）。见 `docs/DESKTOP-RUNTIME.md`。  
3. 更新 `ui-smoke` / `TODO.md` 状态说明。

### 波次 1 — Phase 1 闭环数据

4. **A1** `list_all_sessions` 或前端合并多 profile。  
5. **A2** 账号 / cwd 筛选。  
6. **A3** `PlanView` 分组：Will backup / copy / skip / blocked / warnings。  
7. 补测试。

### 波次 2 — 原型壳层

8. **A4** Overview 活动时间线（基于 `modified_at`）。  
9. **A5** 最近账号与快捷操作。  
10. **A6** Settings 扩展。  
11. **A7** Inspector side effects。

### 波次 3–6

见上表 C、D；详细条目见 `docs/TODO.md` Phase 1G（TODO-119–124，待写入）。

## 5. TODO 编号映射（Phase 1G 建议）

| 编号 | 内容 | 波次 |
|------|------|------|
| TODO-119 | 跨 profile Sessions + 筛选 | 1 |
| TODO-120 | Sync Plan 分组 UI | 1 |
| TODO-121 | Overview 时间线 + 最近账号 | 2 |
| TODO-122 | Settings / Inspector 补齐 | 2 |
| TODO-123 | 原型额度 UI（依赖 601） | 4 |
| TODO-124 | Phase 1 手工验收记录模板 | 5 |

## 6. 设计参考索引

| 用途 | 文件 |
|------|------|
| 产品/架构真相 | `docs/FINAL-DESIGN.md` |
| §9.1 验收八条 | `docs/FINAL-DESIGN.md` §9.1 |
| Issue 粒度 | `docs/IMPLEMENTATION-ISSUES.md` |
| 执行清单 | `docs/TODO.md` |
| UI 主原型 | `account-manager/codex-manager/index.html` |
| 视觉/handoff | `account-manager/codex-manager/DESIGN-HANDOFF.md` |
| 原型变更说明 | `account-manager/codex-manager/PROTOTYPE-CHANGES.md` |
| macOS 仪表盘节奏 | `account-manager/anotherdesign/screens/overview.html` |
| Command 草案 | `docs/05-tauri-command-contracts.md` |
| 安全策略 | `docs/03-security-and-data-safety.md` |
| 桌面运行时选型 | `docs/DESKTOP-RUNTIME.md` |
| 手工验收模板 | `docs/PHASE1-ACCEPTANCE.md`（待填） |

## 7. 变更记录

| 日期 | 说明 |
|------|------|
| 2026-06-02 | 初版：审查结论、波次顺序、TODO 映射 |
| 2026-06-02 | 波次 0：`short_text` UTF-8 截断修复（`core.rs`） |
| 2026-06-02 | 波次 3：子进程实时额度 v1（`codex app-server --stdio` + `account/rateLimits/read` 解析 primary/secondary） |

## 8. 代理模型（Routing Mode）后续规划

目标：在不破坏默认“本地安全模式”的前提下，提供可选的代理路由能力（借鉴 cc-switch 路由模式），用于实时 provider 切换、请求观察和失效转移。

### 8.1 设计原则

1. 默认关闭：**不开代理仍可完整使用 Lam**（扫描 / relay / sync / resume / quota）。
2. 隔离开关：Routing Mode 单独启停，不隐式改用户配置。
3. 可回滚：启用前备份 `~/.codex/config.toml`，一键恢复。
4. 最小持久化：不落地 prompt/response 明文；仅结构化计数与状态。
5. 安全优先：继续禁止复制 `auth.json`，代理层不输出 token。

### 8.2 分阶段实施

| 阶段 | 内容 | 产出 |
|------|------|------|
| RM-0 | 设计冻结 | API、配置写回策略、日志字段、故障回滚策略 |
| RM-1 | 本地路由守护进程（仅 Codex） | `127.0.0.1:<port>/v1` 转发、健康检查、手工启停 |
| RM-2 | 配置接管与恢复 | 启用时重写 `base_url`，禁用时恢复备份 |
| RM-3 | 观测面板 | 请求计数、错误率、当前 provider、failover 事件 |
| RM-4 | Failover 策略 | provider 不可用时按策略切换并记录 |
| RM-5 | 多 Agent 扩展 | Claude/Gemini 接入（与 TODO-801 对齐） |

### 8.3 与当前子进程额度方案关系

- 当前（已实现）：按 profile 启动短生命周期 `codex app-server --stdio` 查询 `account/rateLimits/read`。
- 代理模式（规划）：常驻路由进程负责请求转发与观测；额度仍可沿用 app-server 查询或路由侧聚合，二者可并存。
- 原则：**先把子进程实时额度方案稳定，再引入代理模式**，避免同时引入两类复杂度。
