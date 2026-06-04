## 设计背景

随着 AI coding agent 的快速发展，开发者的本地开发环境正在变得越来越复杂。过去开发者可能只使用一个命令行工具或一个 AI 助手，但现在很多开发者会同时使用多个 agent，例如 **Codex、Claude Code、Reasonix、OpenCode、Aider、Cursor Agent** 等。不同 agent 往往有各自独立的账号体系、配置目录、模型 Provider、API Key、session/history 存储方式、项目上下文和 resume 机制。

这个问题最初是在 Codex 的多账号使用场景中被发现的：开发者为了隔离不同 ChatGPT/Codex 账号，会在本地创建多个 `CODEX_HOME` 目录，例如 `~/.codex-a`、`~/.codex-b`、`~/.codex-luna`。当某个账号在开发过程中额度耗尽时，开发者希望临时切换到另一个账号继续开发，但由于 session、history 和账号目录彼此隔离，新的账号无法直接理解之前的开发进度，只能重新扫描代码、阅读文档、分析上下文，造成大量重复 token 消耗和时间浪费。

进一步看，这并不是 Codex 独有的问题，而是所有本地 coding agent 都会遇到的共性问题。不同 agent 的账号、Provider、session、history、密钥和项目上下文分散在本机各个目录中，缺少一个统一、安全、可视化的管理入口。开发者很难清楚知道当前本机安装了哪些 agent、每个 agent 有哪些账号或 profile、每个账号使用哪个 Provider、哪些 session 属于哪个项目、哪些上下文可以继续、哪些密钥被存在哪里，以及切换或同步 session 时会影响哪些文件。

同时，开发者还会接入不同的模型服务和外部 Provider，例如 OpenAI、Anthropic、OpenAI-compatible gateway、OpenRouter、LiteLLM、Ollama、LM Studio、公司内部 LLM 网关等。这些 Provider 往往需要配置 `base_url`、`api_key`、`env_key`、默认模型、wire API 等信息。如果缺少统一管理，Provider 配置容易散落在不同 agent 或 profile 的配置文件中，带来密钥泄露、配置不一致、账号串用、历史污染和调试困难等风险。

因此，这个产品虽然从 Codex 多账号 session relay 的痛点出发，但它的长期目标不应局限于 Codex，而应抽象为一个通用的本地 AI Agent 管理平台：统一管理本机所有 coding agent 的账号、Provider、session、workspace、relay 和 secrets，让开发者能够在多个 agent、多个账号和多个模型服务之间安全、透明、可控地切换和接续开发。

一句话概括：

> **Codex 暴露了问题，但问题属于所有本地 coding agent：账号、Provider、session、密钥和上下文分散在不同工具里。我们希望构建一个本地优先、安全可审计的统一控制台。**

---

## 设计目标

本产品命名为 **LocalAgentManager**，简称 **Lam**。它的目标是打造一个面向开发者的本地 AI coding agent 工作区管理器，作为本地 AI coding agent 的统一管理入口。它不仅管理 Codex，也为后续支持 Claude Code、Reasonix、OpenCode、Aider、Cursor Agent 等更多 agent 预留统一抽象和扩展能力。

产品需要围绕以下核心目标展开：

### 1. 统一管理本地 AI Coding Agents

产品应能够识别和管理开发者本机安装的多个 agent，包括但不限于 Codex、Claude Code、Reasonix、OpenCode 等。每个 agent 都应具备统一的可视化入口，展示安装状态、binary 路径、版本信息、账号/profile 数量、session 数量、Provider 使用情况和支持能力。

短期先完整支持 Codex，长期通过 adapter/plugin 架构扩展到更多 agent。

---

### 2. 统一管理账号与 Profile

产品应支持扫描、创建和管理不同 agent 的本地账号目录或 profile，例如 Codex 的多个 `CODEX_HOME` 目录。用户可以通过可视化界面创建新的受管账号目录，自动生成 wrapper 命令，并引导完成登录或 Provider 配置。

账号管理需要保证命名统一、目录清晰、命令可控，避免手工创建导致路径混乱、配置遗漏和后续 session 同步失败。

---

### 3. 统一管理 Provider 与模型配置

产品应提供 Provider Center，用于统一管理不同模型服务的配置，包括：

```text
Provider ID
Provider Name
base_url
wire_api
default model
env_key
API key storage mode
optional headers
used by accounts/profiles
```

Provider 不应绑定死在某个账号上，而应作为可复用的配置资源，被多个 agent、账号或 relay workspace 引用。API Key 等敏感信息应优先存储在 macOS Keychain 等安全存储中，而不是直接写入配置文件、wrapper 脚本或日志。

---

### 4. 统一浏览和管理 Sessions

产品应提供跨账号、跨 profile、跨 agent 的 session 浏览能力，让用户可以按 agent、账号、项目目录、最近活跃时间、Provider、模型等维度查看历史会话。

对于 Codex 这样的 agent，应支持识别本地 `sessions/`，解析 session id、项目路径、摘要、修改时间、大小、原始 Provider 和当前 Provider 等信息，并生成可执行的 resume 命令。

长期目标是把不同 agent 的 conversation/session 统一抽象为通用的 `AgentSession`，让开发者可以在一个界面里理解所有 AI agent 的本地上下文资产。

---

### 5. 支持安全的 Session Relay 与上下文接力

产品的关键能力之一是解决“账号额度耗尽后，如何用另一个账号继续开发”的问题。

产品应支持创建 relay workspace，例如：

```text
~/.codex-b-relay-a
```

让 B 账号可以临时接续 A 账号的 session，而不污染 B 原来的 session/history。同步默认只同步 resume 所需的 session 数据，不复制认证信息，不默认合并历史。

核心原则是：

```text
默认同步 sessions/
不复制 auth.json
不默认合并 history.jsonl
不复制 logs、cache、state、tmp 等内部状态
同步前必须可预览
同步后必须可追踪
```

这样可以帮助开发者减少重复阅读代码和文档带来的 token 浪费，同时避免账号串号、历史污染和密钥泄露。

---

### 6. 保持账号、Provider、Session 三者边界清晰

产品必须明确区分三类对象：

```text
Account/Profile：代表本地身份、目录、登录态和配置上下文
Provider：代表模型服务、base_url、API key、模型和协议
Session：代表历史对话、项目上下文和 resume 能力
```

用户在进行 session relay 或 resume 时，产品需要明确展示：

```text
原始 agent 是什么
原始账号/profile 是什么
原始 Provider 是什么
目标账号/profile 是什么
当前 Provider 是什么
是否存在 Provider mismatch
执行 resume 会使用什么命令
会影响哪些本地文件
```

这样用户可以在切换账号、切换 Provider 或跨 agent 接力时保持可控。

---

### 7. 实时额度剩余与刷新时间可视

产品应让每个受管账号/profile 展示**当前额度使用情况**与**距离重置还有多久**，帮助用户在额度耗尽前决定继续、切换账号或创建 relay，而不是在 Codex CLI 报错后才被动发现。

每个账号至少展示（按 Agent 能力适配命名）：

```text
窗口类型（如 Session / 5h Burst、Weekly / 7d）
已用百分比与进度条
剩余可用比例（100% - used）
重置倒计时（如 Resets in 3h 53m）
数据更新时间（如 Updated just now）
计划类型（如 Plus / Pro / API）— 若可获取
```

交互与策略要求：

```text
按 CODEX_HOME / profile 隔离查询，不混用 auth
支持手动刷新与可配置自动刷新（默认 5 分钟，可关闭）
额度紧张时（如 ≥70% / ≥90%）在账号卡片、侧栏、Overview 显示警告色
未登录或无数据时显示明确状态（未登录 / 需先运行 codex / 数据过期）
不把 access_token 写入日志；额度查询走本机已登录态，不上传 session
```

与 session relay 联动：当某账号主窗口额度接近上限时，UI 应提示「可切换到账号 B」或「创建 B-relay-A」，减少重复扫仓库的 token 浪费。

长期：Claude、Cursor 等通过各 Agent Adapter 的 `fetch_usage_quota(profile_id)` 统一为 `UsageQuotaSnapshot` 模型；可选菜单栏紧凑视图（类似多 Agent 额度切换条）。

---

### 8. 本地优先、安全可审计

产品应坚持 local-first 原则。所有账号目录、Provider 配置、session、history 和密钥都应默认保存在用户本机，不做云端同步，不上传用户 session 内容，不收集项目代码和 prompt 内容。

所有敏感操作都应提供明确提示和 dry-run 预览，例如：

```text
创建账号目录
写入 wrapper 命令
修改 config.toml
同步 sessions/
备份 history
打开 Terminal 执行 resume
注入 Provider API key
```

用户需要清楚知道每个操作会读什么、写什么、改什么。

---

### 9. 为开源和长期扩展设计

产品应从一开始就按照开源工具设计，具备清晰的架构边界、adapter 机制、数据模型和安全策略。

短期实现 Codex-first MVP，但底层不应写死 Codex，而应抽象为：

```text
Agent
AgentProfile
ProviderProfile
AgentSession
UsageQuotaSnapshot
Workspace
RelayWorkspace
Secret
ResumeCommand
```

后续通过 adapter 扩展：

```text
CodexAdapter
ClaudeCodeAdapter
OpenCodeAdapter
ReasonixAdapter
AiderAdapter
CursorAdapter
```

这样产品可以逐步演进为开发者本地 AI agent 的统一控制台，而不是单一工具的辅助脚本。

---

## 阶段性目标

> **实施口径说明：** 本节描述的是长期目标的目标族，而不是单个版本必须一次性交付的范围。实际排期以 `docs/FINAL-DESIGN.md` 为准：Phase 1 先交付 Codex session relay 闭环；实时额度面板进入 Phase 1.2；Provider CRUD、Keychain secret 管理和 Provider Attach 进入 Phase 1.5。

### 第一阶段：Codex-first MVP

先完整解决 Codex 多账号、session relay 的核心闭环，并为 Provider 与额度能力保留模型和 UI 位置：

```text
扫描 ~/.codex*
创建受管 Codex 账号目录
生成 wrapper 命令
只读展示 Codex Provider / model 状态
查看 Codex sessions
创建 relay workspace
安全同步 sessions/
生成 resume command
打开 Terminal resume
从 session jsonl 展示近期 token activity / estimate（不得伪装为实时额度）
```

### 第一阶段补充：实时额度面板（Phase 1.2）

在 Codex MVP 闭环稳定后，补齐每个账号的 Session / Weekly 额度剩余、重置倒计时、自动刷新与额度紧张提示：

```text
按 CODEX_HOME / profile 隔离查询额度
优先通过 codex app-server 获取实时 UsageQuotaSnapshot
jsonl fallback 仅显示 activity estimate / stale data
额度紧张时提示切换账号或创建 relay
```

### 第一阶段补充：Provider & Secret（Phase 1.5）

在安全同步和额度面板稳定后，再开放 Provider Center 的写能力：

```text
Provider CRUD
macOS Keychain / env secret 管理
Attach Provider to Profile
Provider mismatch 检测与 resume 警告
```

---

### 第二阶段：通用 Agent 抽象

重构为统一 agent 模型：

```text
AgentAdapter 接口
统一 Account/Profile 模型
统一 Session 模型
统一 Provider 模型
统一 Secret 管理
统一 Relay 工作流
统一 UsageQuotaSnapshot 与 Adapter 额度查询
```

---

### 第三阶段：扩展更多 Agent

逐步支持：

```text
Claude Code
Reasonix
OpenCode
Aider
Cursor Agent
其他本地 coding agent
```

每个 agent 通过 adapter 实现独立探测、配置读取、session 解析和命令生成。

---

### 第四阶段：本地 Agent Workspace 控制台

最终形成一个本地统一控制台，帮助开发者管理所有 AI coding agent 的：

```text
账号
Provider
模型
API Key
session
history
workspace
relay
handoff
resume command
各账号实时额度与刷新时间
安全策略
```

让开发者在多个 agent、多个账号、多个模型 Provider 之间自由切换，同时保持上下文连续、安全隔离和操作透明。
