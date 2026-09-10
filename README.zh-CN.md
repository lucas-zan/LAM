# LAM — 专为 macOS 设计的 Codex 多账号运行时与智能网关

> 让 Codex 突破单账号额度限制与模型边界：支持**零泄露会话接力**、**第三方/自建 API 智能接入（Responses 直连 + Chat Completions 本地网关适配）**、**Session 账号一键导入（类似 sub2api）** 与 **全景配额监控**。

[![Release](https://img.shields.io/badge/release-v0.4.0-blue.svg)](https://github.com/lucas-zan/LAM/releases/tag/v0.4.0)
[![Platform](https://img.shields.io/badge/platform-macOS%20(Apple%20Silicon)-lightgrey.svg)]()
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

**English:** [`README.md`](README.md)

---

## 为什么选择 LAM？

传统的 Codex CLI 用户在面对多账号和第三方模型时，往往陷入繁琐甚至危险的手动折腾：要么整盘拷贝 `~/.codex` 导致串号与密钥泄漏，要么因为 Codex 只支持官方 Responses 协议而无法使用市面上主流的 `/chat/completions` 模型。

**LAM（Local Agent Manager）彻底改变了这一点：**

| 你的痛点 / 需求 | 传统手动做法 | LAM 优雅解决方案 |
| :--- | :--- | :--- |
| **官方账号额度耗尽** | 复制整套 `~/.codex`，极易串号、凭据与历史数据库污染 | **安全会话接力（Handoff）**：只迁移上下文 JSONL，**绝不复制 `auth.json`**，一键换号无感继续（`codex resume`） |
| **想用第三方模型（DeepSeek / Claude / 自建模型）** | Codex 原生只支持 Responses 协议，普通 OpenAI 兼容接口无法直接使用 | **内置 Local Gateway**：自动将 `/chat/completions` 转换适配为 Responses 协议，任何兼容 API 即插即用 |
| **有官方 Responses 协议上游** | 手动修改 `config.toml`，参数繁琐且容易配错 | **原生直连转发**：图形化创建 API 账号，一键 Fetch Models，自动映射受管配置 |
| **手头有 ChatGPT Session Token** | 每次在终端跳转浏览器 OAuth 登录，繁琐耗时 | **Session 账号快速导入（类似 sub2api）**：直接粘贴 Session JSON 自动生成独立 Profile / PAT，无需打开浏览器 |
| **多个账号需要并行开发** | 环境变量冲突，多终端同时跑容易覆盖配置 | **Profile 隔离模式**：每个账号独立 `CODEX_HOME` 与 wrapper，支持多窗口真正并发运行 |
| **随时掌握账号配额进度** | 只能触发请求碰壁或登录网页查询 | **macOS 菜单栏常驻**：实时掌握 5h 与每周配额进度条、重置倒计时，**同时支持 Codex 与 Google Antigravity** |

![LAM 账号、额度、使用量与 Session 总览](docs/assets/lam-overview.png)

> **注意**：LAM **不内嵌** Codex 命令行。会话启动、接力及 API Profile 均在系统外部终端（默认 Terminal.app 或 Ghostty）中以官方 CLI 契约执行，保证稳定性与兼容性。

---

## 一览表：核心功能与特性

```
┌────────────────────────────────────────────────────────────────────────┐
│                              LAM 运行时架构                             │
├───────────────────────────────────┬────────────────────────────────────┤
│           官方账号体系            │           第三方 API 体系          │
│  • Profile 模式（多环境完全物理隔离）  │  • 原生 Responses 协议直连转发     │
│  • PAT 模式（单目录凭据快速热切换）   │  • Chat Completions 本地网关智能适配│
│  • Session JSON 一键导入 (免 OAuth)│  • 上游模型一键 Fetch 与 Allowlist │
├───────────────────────────────────┴────────────────────────────────────┤
│                              核心引擎与功能                             │
│  • 零凭据泄漏的会话安全接力 (Safe Session Handoff)                       │
│  • macOS 菜单栏常驻配额监控 (Codex 5h/周配额 + Antigravity 多模型分组)   │
│  • 本地多维使用量看板 (Token 统计、调用热力图、成本预估)                │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 核心亮点深度解析

### 1. 第三方 API 账号：Responses 直连转发 + Chat Completions 本地网关适配

Codex 自定义 Provider 的官方稳定边界是 **`wire_api = "responses"`**。但市面上绝大多数中转平台、自建反代、开源大模型只提供标准 OpenAI `/chat/completions` 接口。

LAM 原生提供**双路径智能适配**，让你彻底告别手写配置：

| 上游接口协议 | LAM 怎么接 | Codex 看到什么 | 适用场景 |
| :--- | :--- | :--- | :--- |
| **OpenAI Responses**<br>(`/v1/responses`) | **直连转发 (Direct Projection)**：将 Base URL、认证、模型配置安全投影进该 Profile 的 `config.toml` | 官方 Responses Provider | 官方兼容的 Responses 上游端点 |
| **Chat Completions**<br>(`/chat/completions`) | **本地 Gateway 智能适配**：内置轻量高效的 Rust 本地网关（`lam-provider-gateway`），实时将 Chat Completions 转换为 Responses 格式及双向流式协议 | 本机 Loopback 端点<br>(`127.0.0.1`) | DeepSeek、Claude、各大第三方模型中转平台、自建 vLLM / Ollama 端点 |

* **API Account 一键式接入**：输入 Base URL、API Key 与协议类型即可完成接入。
* **模型一键发现 (Fetch Models)**：支持自动拉取上游支持的模型列表，一键勾选 Model Allowlist，快速切换默认模型。
* **凭据安全引用**：API Key 仅通过受管引用存储，不暴露在明文配置与前端状态中。
* **安全附加与解绑**：Attach / Rebind / Detach 全流程可视化预览，在工具或状态历史不兼容时严格拦截（Fail-Closed）。

![创建 External API Account 并配置 Model](docs/assets/lam-external-api.png)

---

### 2. 官方账号双模式：Profile 模式 vs PAT 模式

针对官方账号的不同使用习惯，LAM 提供了两种完全不同架构的运行模式：

| 维度 | Profile 模式（推荐） | PAT 模式（单目录模式） |
| :--- | :--- | :--- |
| **底层实现** | 每个账号独立一套 `~/.codex-<name>` 目录与 wrapper 脚本 | 所有账号共用默认的 `~/.codex` 目录，单点替换 `auth.json` |
| **运行隔离性** | **完全物理隔离**：配置、会话记录、缓存、认证互不影响 | **单点覆盖**：当前激活账号的凭据会写入默认目录 |
| **并发支持** | **支持**：不同终端窗口可同时运行不同账号，互不干扰 | **不支持**：同一时刻只能激活一个账号 |
| **启动方式** | 通过 LAM 启动或使用生成的专属 wrapper 命令 | 直接在终端使用系统原生的 `codex` 命令 |
| **适用人群** | 多账号并行开发、工作/个人环境严格隔离的高阶用户 | 习惯单目录操作、仅在额度耗尽时快速换号的用户 |

在 LAM 设置（Settings）中可根据需要随时切换主展示模式。

![Profile/PAT 显示模式与 Handoff 终端设置](docs/assets/lam-settings.png)

---

### 3. Session 账号快速导入（类似 sub2api）

无需繁琐地每次都打开浏览器进行 OAuth 网页跳转授权，LAM 提供了极速导入通道：

* **支持粘贴 Session JSON**：直接复制粘贴外部导出的 ChatGPT Session JSON（包含 `accessToken`、`idToken` 等字段）或 `auth.json`。
* **自动生成 Profile / PAT**：系统自动解析 Token 有效期、Plan 类型（Plus / Team / Enterprise 等），一键转换并初始化为可独立运行的 Codex Profile。
* **支持 CPA 导出 (CPA Export)**：支持将已有账号导出为标准的 CPA 认证文件，便于在多台设备间快速分发与备份。

![将可信的 ChatGPT Session JSON 导入独立 Codex Profile](docs/assets/lam-import-session.png)

---

### 4. 零凭据泄露的安全会话接力 (Safe Session Handoff)

当账号 A 的 5 小时会话窗口或每周额度耗尽时，无需重开话题：

1. **选择会话与目标**：在 LAM 界面选择当前未完成的会话及有剩余额度的目标账号。
2. **安全上下文迁移**：LAM **仅复制选中会话的单个 JSONL 记录**。
   * **绝对不拷贝**：`auth.json`、`config.toml`、历史记录索引、SQLite 数据库、缓存、Token、系统 ID。
   * **彻底避免串号**：目标账号的认证凭据保持 100% 独立纯净。
3. **分叉与兼容预检**：支持模型兼容性检查；若目标端已存在同名会话，提供备份、优先源、Fork、时间线合并等多种安全策略。
4. **一键恢复执行**：直接调用目标环境执行 `codex resume <session-id>`，在外部终端无缝接续工作。

---

### 5. macOS 菜单栏全景配额监控 + 本地多维用量分析

* **macOS 菜单栏常驻面板**：
  * 随时点击菜单栏图标展开浮层，查看各账号的 **5h 滑动窗口**与 **每周额度** 剩余百分比及重置时间。
  * **支持 Codex 与 Google Antigravity 双平台**：不仅能监控 Codex 各账号，还能分组实时追踪 Google Antigravity 的模型配额（Gemini 模型组、Claude & GPT 模型组）。
  * 浮层内直接提供一键刷新与快捷 Resume 操作。
* **本地用量看板 (Usage Dashboard)**：
  * **Token 消耗统计**：精确追踪输入、输出及总 Token 数量。
  * **请求与调用分析**：按天/周展示调用次数、Thread 活跃趋势与交互热力图。
  * **成本预估**：根据使用模型和 Token 消耗进行本地成本分析与预估。

![带账号 Relay 操作的 LAM 菜单栏额度浮层](docs/assets/lam-menu-bar.png)
![本地 Codex Token、调用、Thread 与估算成本](docs/assets/lam-usage.png)

---

## 隐私、数据安全与边界承诺

LAM 坚持 **Local-First（本地优先）** 架构原则：

- **无云端服务**：没有 LAM 集中式账号系统，不收集、不上传任何会话记录、Prompt 内容或私有密钥。
- **透明出网**：除配额查询、API 账号请求、Gateway 向上游转发以及本地语言服务探测外，所有数据均保存在本地 `~/.lam`。
- **安全失败原则 (Fail-Closed)**：跨账号接力时若检测到环境冲突或协议断裂，严格终止操作并提示风险，绝不进行不可逆的静默覆盖。

---

## 快速安装与使用

### 下载体验版 (macOS Apple Silicon)

前往 [Releases 页面](https://github.com/lucas-zan/LAM/releases) 下载最新安装包：

* **下载地址**：[v0.4.0 预编译 DMG](https://github.com/lucas-zan/LAM/releases/tag/v0.4.0) (`LAM_0.4.0_aarch64.dmg`)
* 双击打开 DMG，将 LAM 拖拽入 `Applications`（应用程序）文件夹即可。
* *系统要求：macOS 12+，需预先安装官方 Codex CLI。*

### 从源码编译运行

```bash
# 1. 克隆代码仓库
git clone https://github.com/lucas-zan/LAM.git
cd LAM

# 2. 安装前端与 Rust 依赖
make install

# 3. 启动开发模式
make start

# 4. 打包构建 DMG
make dmg
```

---

## 常用命令速查

| 命令 | 用途 |
| :--- | :--- |
| `make install` | 安装前端与 Rust 依赖 |
| `make start` | 启动 Tauri 开发模式（实时热重载） |
| `make check` | 运行前端测试、TypeScript 检查、Clippy 及 Rust 单元测试 |
| `make build` | 编译打包生产版本的 `.app` |
| `make dmg` | 生成可分发的 macOS DMG 安装镜像 |

---

## 参与贡献与开发

欢迎提交 Issue 和 Pull Request！开发与架构设计详情可参考文档目录：

* [产品架构设计文档](docs/01-product-design.md)
* [安全与数据保护设计](docs/03-security-and-data-safety.md)
* [桌面端运行时与交互设计](docs/DESKTOP-RUNTIME.md)
* [Provider Gateway 协议覆盖文档](docs/remote-provider-gateway-contract-coverage.md)

---

## 开源协议

本项目采用 [MIT License](LICENSE) 开源协议。
