# LocalAgentManager (Lam)

**本地优先的 AI Coding Agent 工作区管理器。** 首期以 Codex 为主：在本机管理多个 `CODEX_HOME` 配置目录、Relay 工作区、安全同步 `sessions/`，并生成可审计的 `codex resume` 命令。长期为多 Agent 预留架构，实现路径保持 **Codex-first**。

**English:** [`README.md`](README.md)

---

## 背景与动机

日常用 Codex CLI 时，常见痛点包括：

- **多账号并存**：`~/.codex`、`~/.codex-a` 等目录各自维护 `auth.json`、`config.toml` 和 `sessions/`，切换成本高，容易误用错误 wrapper 或 `CODEX_HOME`。
- **切换账号后 Session 不会继承**：对话状态绑定在某一组 `CODEX_HOME` 下。若在 A 账号额度用尽或主动改用 B，却**没有把对应 session 资产迁到 B 的目录**，新账号无法延续原线程，往往需要重新打开仓库、重复说明需求、**让模型重新读代码，造成 token 浪费**。
- **Session 拷贝不安全**：手工把 A 的目录拷到 B，容易连带 `auth.json`、sqlite、缓存，污染目标环境。
- **Provider 与运行时脱节**：旧 transcript 仍能 `resume`，但模型/代理/费用/工具行为可能已变，需要显式提示。
- **缺少统一桌面入口**：命令行强，但缺少「账号、会话、额度、同步计划」一览。

Lam 的目标：**不上传 session/代码/prompt**，把账号边界、Provider、Session 资产、Relay、同步收敛到本地 Rust 核心 + 桌面 UI，且写操作必须先 **dry-run**。

主规格：[`docs/FINAL-DESIGN.md`](docs/FINAL-DESIGN.md)；任务与偏差：[`docs/TODO.md`](docs/TODO.md)、[`docs/IMPLEMENTATION-ISSUES.md`](docs/IMPLEMENTATION-ISSUES.md)。

---

## 目标

| 维度 | 说明 |
|------|------|
| **本地优先** | 数据留在本机；不依赖云端同步 session 内容。 |
| **边界清晰** | Account（`CODEX_HOME`）· Provider（元数据 + 密钥引用）· Session（`sessions/`）分层。 |
| **安全默认** | 默认只同步 `sessions/`；**永不复制 `auth.json`**；不默认合并 `history.jsonl`。 |
| **可审计写操作** | 建号、Relay、Sync、Attach Provider：plan → 确认 → execute。 |
| **Codex-first** | Phase 1 扫描 / Resume / Safe Sync；Provider Center（1.5 基础能力已落地）；预留 `AgentAdapter`。 |
| **桌面原生** | **macOS**：**Tauri v2 + Rust + React**（非 Electron；其它系统暂无计划）。见 [`docs/DESKTOP-RUNTIME.md`](docs/DESKTOP-RUNTIME.md)。 |

---

## 跨账号 Session 接力 — 当前做到哪一步

这是「换账号 → 上下文断了 → 重复读代码费 token」要解决的核心场景。**初版还没有「点一下就在 B 账号里无缝接着聊」的自动切换**；Phase 1 提供的是与 Codex 存储方式一致的 **手动、安全流水线**。

### 为什么 Codex 换账号不会自动继承 Session

每个 profile 是独立 home：

```text
~/.codex-a/sessions/...   ← 仅当 CODEX_HOME=~/.codex-a 时可见
~/.codex-b/sessions/...   ← 仅当 CODEX_HOME=~/.codex-b 时可见
```

`codex resume <session-id>` 只在**当前** `CODEX_HOME` 下的 `sessions/` 里找会话。只换 wrapper/账号、不迁移 session 文件，对新 home 来说就是新上下文。

### 当前应用怎么解决（Phase 1 已实现）

| 步骤 | 应用内入口 | 作用 |
|------|------------|------|
| 1 | **Relay** / 「+ New Relay」 | 创建独立 relay 目录（如 `~/.codex-b-relay-a`），用 **B 的登录态**，与源账号 A 分离。 |
| 2 | 账号卡 **「↑ Sync To…」** 或 **Sync** | **Safe Sync**：仅复制 **`sessions/`**（须先 dry-run）；阻止 `auth.json`、sqlite、cache 等。 |
| 3 | **「→ Login」** | 在 relay 的 `CODEX_HOME` 下执行 `codex login`，不把 A 的 auth 拷过去。 |
| 4 | **Sessions** → Copy / Terminal / Details | 生成 `CODEX_HOME=<profile> codex resume <id>`，打开终端或复制命令。 |
| 5 | **Relay / Continue**（Overview 账号卡、托盘浮层） | 在已选定活跃 session 时，按需把该 session 资产复制到目标 profile（`relay_resume_session`），再打开终端执行 `codex resume`（分叉策略见 Settings）。 |

推荐端到端流程（与设计文档 §4.3–4.5 一致）：

```text
账号 A（额度将尽）  --[Safe Sync 仅 sessions/]-->  Relay 目录（B 的 auth）
                                                    |
                                                    v
                                          在 relay 下 codex resume <session-id>
```

### 尚未实现（所以会感觉「还没做到跨 session 切换」）

- **没有一键「用 B 继续当前会话」** — 仍需自行：建 Relay → Sync → Login → Resume。
- **Sessions 页按单账号筛选** — 下拉框只列一个 `CODEX_HOME` 的会话，没有跨账号统一列表 + 内置接力向导（见 `docs/CORRECTION-PLAN.md` A1/A2）。
- **不内嵌 Codex 进程** — 只拼命令并打开 **Terminal.app**，不在应用内直接续聊。
- **切换筛选不会自动搬文件** — 换下拉选项只是换「看哪个目录」，不会同步 session。
- **`history.jsonl` 合并** — Phase 1 故意不做。

规划中的改进：额度不足时的引导式 Relay、跨 profile 会话浏览、Overview 一键接力（见下文 **未来规划**）。

---

## 当前能力（初版 / Phase 1）

**Rust 核心**

- 扫描 `~/.codex` / `~/.codex-*` 与 session 元数据。
- 托管账号 / Relay 的 plan & execute。
- Safe Sync（仅 `sessions/`、目标备份、manifest）。
- Resume 命令构建与 Terminal 拉起。
- **`relay_resume_session`**：单 session 复制/合并到目标 profile（含分叉策略）并返回 resume 命令。
- Provider CRUD、密钥引用、Attach、mismatch 检测。
- 账号/额度**本地缓存**（`accounts-cache.json`、quota 缓存）以加快启动。

**桌面 UI**

- Overview（账号 + 额度）、Sessions、Relay、Providers、Sync、Settings。
- 额度：app-server 5h/weekly；失败显示 **N/A**。
- **macOS 菜单栏托盘：** **左键**弹出紧凑**额度浮层**（5h / weekly 进度条；有最近 session 时可在账号行 **Relay / Resume**）。**右键**刷新额度或打开主窗口。点击浮层外或 **Close** 会收起（不会自动弹出主窗口，除非点 **Open**）。后台每 5 分钟刷新；启动先读缓存账号/额度，再按账号并行拉取真实额度。
- Sync 须先 dry-run。

**不在 Phase 1**

- `history.jsonl` 合并、云端同步、多 Agent 适配器、伪造额度。

详见 [`docs/PHASE1-ACCEPTANCE.md`](docs/PHASE1-ACCEPTANCE.md)、[`docs/CORRECTION-PLAN.md`](docs/CORRECTION-PLAN.md)。

---

## 安全原则

- Sync **永不**复制 `auth.json`。
- 默认阻止 `config.toml`、sqlite、`cache/`、`tmp/`、`logs/`、`installation_id`。
- 优先用 relay 目录，避免污染源账号。
- UI 写操作强制 **先 dry-run**。

---

## 路线图

### 已交付（Phase 1 核心 + 1.2 额度 + 1.5 Provider 基础）

| 领域 | 内容 |
|------|------|
| **账号与 Relay** | 扫描 `~/.codex*`；创建受管账号；**Relay 工作区**；Safe Sync（仅 `sessions/`，须 dry-run）。 |
| **额度 Quota** | Overview 展示 app-server **5h / weekly**；**菜单栏托盘浮层**；本地缓存；**按账号并行刷新**；不可用显示 N/A。 |
| **接力** | `codex resume` 命令；**`relay_resume_session`**（复制/合并 session 到目标 profile），入口：Overview **Relay/Continue**、托盘 **Relay/Resume**；分叉策略见 Settings。 |
| **Provider** | Provider Center CRUD、密钥引用（env/Keychain）、Attach、mismatch 提示。 |
| **桌面** | Tauri v2；托盘优先启动（主窗口经 **Open** 打开）；点击浮层外收起。 |

### 下一步（Phase 1 收尾 — 仅 macOS）

**平台范围：** 当前**只做 macOS**，暂无 Linux/Windows 移植计划；优先打磨托盘、浮层、Terminal 集成与 Phase 1 验收。

| 主题 | 方向 |
|------|------|
| **产品与 UI** | 活动时间线；**跨 profile 统一 Sessions 列表**；Settings 完善；[`docs/PHASE1-ACCEPTANCE.md`](docs/PHASE1-ACCEPTANCE.md) 验收签字。 |
| **接力体验** | 额度不足时的**引导式向导**（一条流走完 relay → sync → login → resume），而不只是分散按钮。 |
| **额度与 Relay 打磨** | 边界场景、陈旧数据提示、Provider/额度相关测试与文档对齐。 |
| **macOS 打磨** | 菜单栏托盘体验、浮层焦点与收起、app-server 额度稳定性、签名打包准备。 |

### 更长期

| 阶段 | 方向 |
|------|------|
| **Phase 2** | `AgentAdapter`；Claude Code / OpenCode 等，不削弱 Codex 安全默认。 |

详见 [`docs/FINAL-DESIGN.md`](docs/FINAL-DESIGN.md)、[`docs/TODO.md`](docs/TODO.md)。

---

## 环境要求

- Node.js + npm、Rust + Cargo
- **macOS** — 当前支持的目标平台（托盘、Terminal 集成）；暂无 Linux/Windows 计划。签名打包才需完整 Xcode
- 可选：已登录 Codex CLI；测试可用 `.fake-home`

---

## 快速开始

```bash
make install
make start
```

假数据 home：

```bash
LAM_HOME="$(pwd)/.fake-home" make start
```

- `make start` 启动 **Tauri 应用**（macOS 上为菜单栏托盘）。**主窗口默认隐藏**，需从托盘打开（浮层 **Open** 或右键「Open LocalAgentManager」）。终端里的 Vite 地址只是内嵌开发服务器。
- 默认扫描真实 home；仅显式设置 `LAM_HOME` 时用 fixture。

---

## 常用命令

| 命令 | 作用 |
|------|------|
| `make start` | 启动 Tauri 开发模式 |
| `make stop` | 停止本仓库拉起的进程 |
| `make check` | 前端 build + UI smoke + Rust 测试 |
| `make build` | 本地构建 bundle（`.dmg` 在 `src-tauri/target/release/bundle/dmg/`） |
| Git 标签 `v*` | 推送如 `v0.1.0` → [`.github/workflows/release.yml`](.github/workflows/release.yml) 在 GitHub Releases 构建 macOS `.dmg` |
| `make status` | 环境信息 |
| `make accounts` | CLI 扫描（`lam-core`） |
| `make help` | 所有 Make 目标 |

---

## 测试

```bash
cd apps/desktop/src-tauri && cargo test
LAM_HOME="$(pwd)/../../.fake-home" cargo test

cd apps/desktop && npm run build && npm run test:ui
```

---

## 仓库结构

```text
apps/desktop/              # React 前端
apps/desktop/src-tauri/    # Tauri + Rust 核心
.fake-home/                # 测试 fixture
account-manager/           # HTML 原型（参考）
docs/                      # 规格与验收
Makefile
```

---

## 文档索引

| 文档 | 用途 |
|------|------|
| [`docs/FINAL-DESIGN.md`](docs/FINAL-DESIGN.md) | 主规格 |
| [`docs/TODO.md`](docs/TODO.md) | 实施清单 |
| [`docs/IMPLEMENTATION-ISSUES.md`](docs/IMPLEMENTATION-ISSUES.md) | 问题级跟踪 |
| [`docs/DESKTOP-RUNTIME.md`](docs/DESKTOP-RUNTIME.md) | Tauri 与启动说明 |
| [`docs/CORRECTION-PLAN.md`](docs/CORRECTION-PLAN.md) | UI/规格纠偏 |
| [`docs/PHASE1-ACCEPTANCE.md`](docs/PHASE1-ACCEPTANCE.md) | Phase 1 验收 |

---

## 许可证

本项目采用 **[MIT License](LICENSE)**，可自由使用、修改、合并、发布、分发、再授权和出售副本。

再分发时请附带 [`LICENSE`](LICENSE) 中的版权声明与许可全文。在 README 或关于页**注明来源于 LocalAgentManager** 非 MIT 强制要求，但**欢迎**这样做，便于项目传播。
