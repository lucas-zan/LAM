# 产品设计文档：Codex Relay / Codex Session Manager

> **历史草稿 / 已被替代：** 本文保留 Codex relay 的早期产品细节作为参考。当前唯一主规格是 `docs/FINAL-DESIGN.md`；如本文与 `FINAL-DESIGN.md` 冲突，以 `FINAL-DESIGN.md` 为准。产品命名、阶段范围、Provider、额度与多 Agent 抽象均已在最终设计中收敛为 **LocalAgentManager (Lam)**。

版本：0.1 draft  
目标平台：macOS first  
推荐技术栈：Tauri v2 + Rust + TypeScript/React  
目标用户：重度使用 OpenAI Codex CLI 的开发者、团队成员、独立开发者、需要多账号隔离和会话接力的人

---

## 1. 背景

开发者在本地使用 Codex CLI 时，常见做法是通过不同的 `CODEX_HOME` 目录隔离多个账号，例如：

```bash
~/.codex
~/.codex-a
~/.codex-b
~/.codex-luna
~/.codex-b-relay-a
```

这种方式可以很好地隔离登录态、配置、session 和历史记录。但当某个账号在开发过程中达到使用额度，需要临时切换到另一个账号继续工作时，会遇到明显问题：

1. 不同账号的 session 隔离，另一个账号不知道当前开发进度。
2. 新账号重新分析代码、文档和 git diff，会消耗大量 token。
3. 如果直接复制整个 `CODEX_HOME`，可能覆盖或泄漏 `auth.json`，造成账号串号和安全风险。
4. 如果直接合并 `history.jsonl`，可能污染目标账号原本的开发历史。

因此需要一个本地桌面 App，帮助用户安全、可控、可视化地管理多个 Codex 账号目录，并支持 session relay。

---

## 2. 产品定位

Codex Relay 是一个本地优先的 Codex 多账号与 session 接力管理工具。它帮助用户：

- 发现本机所有 Codex 账号目录。
- 查看各账号下的 sessions。
- 创建新的受管控 Codex 账号目录和 wrapper 命令。
- 创建 relay 账号目录，例如 `~/.codex-b-relay-a`。
- 将某个账号的 session 安全同步到另一个账号或 relay 账号。
- 生成或执行 `codex resume` 命令，帮助用户无缝继续开发。

它不是多账号绕过工具，也不是 token 共享工具。它只管理用户本地已有 Codex 状态目录，并避免不必要的上下文重复消耗。

---

## 3. 核心价值

### 3.1 减少重复上下文消耗

用户不需要在切换账号后重新让 Codex 扫描整个项目、重新读取文档、重新解释业务背景，而是通过 `sessions/` 里的本地会话状态继续已有开发任务。

### 3.2 账号状态安全隔离

每个账号仍然拥有独立的：

```text
auth.json
config.toml
history.jsonl
sessions/
cache/
logs
```

App 默认不复制认证文件、不合并历史文件，确保账号边界清晰。

### 3.3 规范化多账号管理

新增账号时，App 自动完成：

```text
创建 ~/.codex-xxx
创建 ~/bin/codex-xxx wrapper
设置目录权限
写入 managed metadata
可选启动 codex-xxx login
```

避免用户手工创建目录、命令、路径时出错。

---

## 4. 目标用户

### 4.1 独立开发者

- 同时有个人账号、工作账号、测试账号。
- 项目经常需要长时间 agent 开发。
- 希望一个账号额度不足时，能临时切到另一个账号继续。

### 4.2 AI coding power users

- 高频使用 Codex CLI、Claude Code、Cursor、OpenAI API 等工具。
- 关注 token 成本、session 连续性、CLI 工作流效率。

### 4.3 团队内高级用户

- 可能维护多个项目、多种身份。
- 希望统一管理本机 Codex 状态目录。

---

## 5. 非目标

本产品不做以下事情：

1. 不破解、不绕过 OpenAI 额度或使用规则。
2. 不共享或同步 `auth.json`。
3. 不把不同人的账号 token 合并或托管。
4. 不上传用户 session、代码路径、历史记录到云端。
5. 不默认合并 `history.jsonl`。
6. 不试图解析或修改 Codex 内部 session 格式，只做保守读取和文件级同步。

---

## 6. 核心概念

### 6.1 Account

一个 Codex Account 指一个本地 `CODEX_HOME` 目录，例如：

```bash
~/.codex-a
```

它不一定代表真实 OpenAI 账号，但通常和一个登录态绑定。

### 6.2 Managed Account

由本 App 创建和管理的账号目录。目录内包含：

```bash
.managed-by-codex-session-manager.json
```

### 6.3 Relay Account

为了临时接力某个账号 session 而创建的隔离账号目录，例如：

```bash
~/.codex-b-relay-a
```

语义：使用 B 的登录态，临时接力 A 的开发 session，但不污染 `~/.codex-b` 原有状态。

### 6.4 Session

`CODEX_HOME/sessions/` 下保存的 Codex 本地 session transcript / thread 状态。Codex CLI 支持 `codex resume` 恢复本地 session。

### 6.5 History

`history.jsonl` 更接近输入历史 / prompt history。它不是 session relay 的主要依赖。默认不合并、不覆盖。

---

## 7. 核心用户故事

### 7.1 查看本机所有 Codex 账号

作为用户，我希望打开 App 后看到所有 `~/.codex*` 目录，以及每个目录是否登录、session 数量、最近活跃时间，方便我知道当前有哪些账号。

验收标准：

- 能发现 `~/.codex`。
- 能发现 `~/.codex-a`、`~/.codex-b`、`~/.codex-luna` 等目录。
- 显示是否存在 `auth.json`。
- 显示 session 数量。
- 显示是否为 managed account。

### 7.2 新增账号目录

作为用户，我希望输入一个账号标识，例如 `luna`，App 自动创建：

```bash
~/.codex-luna
~/bin/codex-luna
```

并能一键执行：

```bash
codex-luna login
```

验收标准：

- 用户只能输入安全命名，例如 `[a-zA-Z0-9][a-zA-Z0-9_-]{0,31}`。
- 自动创建目录并设置权限为 `0700`。
- 自动生成 wrapper。
- wrapper 支持参数透传：`codex-luna resume --all`。
- 不创建或复制 `auth.json`。

### 7.3 创建 relay 账号

作为用户，我希望在 A 账号没额度时，用 B 账号临时接力 A 项目，但不影响 B 原来的 session 和 history。

示例：

```bash
~/.codex-a
~/.codex-b
~/.codex-b-relay-a
```

验收标准：

- App 支持一键创建 `b-relay-a`。
- 生成 `~/bin/codex-b-relay-a`。
- 用户登录 B 账号到 relay 目录。
- 同步 A 的 sessions 到 relay 目录。
- 不修改 `~/.codex-b`。

### 7.4 同步 sessions

作为用户，我希望把 A 的 sessions 同步到 B relay 目录，之后用 B relay 继续 A 的开发。

验收标准：

- 默认只同步 `sessions/`。
- 默认不复制 `history.jsonl`。
- 默认不复制 `auth.json`。
- 支持 dry-run 预览。
- 支持同步前自动备份目标 `sessions/`。
- 有同步报告。

### 7.5 恢复 session

作为用户，我希望选择某个 session 后，App 能生成命令：

```bash
cd '/project/path' && CODEX_HOME='/Users/me/.codex-b-relay-a' codex resume '<SESSION_ID>'
```

验收标准：

- 可复制命令。
- 可打开 Terminal 执行。
- 如果无法识别 cwd，提示用户手动选择项目目录。
- 支持 `resume --all` fallback。

---

## 8. 页面设计

### 8.1 主页面：Accounts Overview

布局：

```text
左侧：账号列表
右侧：所选账号详情 + session 列表
顶部：搜索 / 刷新 / 添加账号 / 创建 relay / 设置
```

账号卡片字段：

```text
名称：codex-a
路径：/Users/me/.codex-a
状态：已登录 / 未登录
sessions：23
managed：yes/no
最近活动：2026-05-26 14:30
```

### 8.2 Session 列表

字段：

```text
Session ID
Project cwd
Summary / first prompt preview
Modified time
Size
Actions: Copy Resume / Open Terminal / Reveal in Finder
```

筛选：

- 按项目路径筛选。
- 按最近修改排序。
- 按账号筛选。
- 按 session id 搜索。

### 8.3 添加账号弹窗

字段：

```text
Account suffix: luna
CODEX_HOME preview: ~/.codex-luna
Wrapper command preview: codex-luna
Copy config from: none / codex-a / codex-b
After create: open Terminal and run codex-luna login
```

风险提示：

```text
The app will create a local directory and a wrapper command. It will not create or copy auth.json. Login is performed by Codex CLI.
```

### 8.4 同步弹窗

字段：

```text
From account
To account
Sync sessions: checked
Backup target sessions first: checked
Backup source history as sidecar: optional unchecked
Merge history into target: hidden behind advanced danger option
Dry run button
Sync button
```

默认安全配置：

```text
Sync sessions = true
Backup target sessions = true
Sidecar history backup = false
Merge history = false
```

### 8.5 设置页面

设置项：

```text
Codex binary path
Managed wrapper directory, default ~/bin
Default terminal app: Terminal.app / iTerm2 / Warp / Ghostty / Copy only
Show advanced options
Enable history sidecar backup
Enable dangerous history merge
```

---

## 9. 权限与隐私

产品必须本地优先：

- 不联网，不上传任何 session 内容。
- 不读取用户项目源码内容，除非为了显示 cwd 或 session metadata 必须从 session 文件提取。
- 不读取或展示 token。
- 不复制 `auth.json`。
- 不自动修改 shell rc 文件，优先生成 `~/bin/codex-xxx` wrapper。

---

## 10. 成功指标

MVP 成功标准：

1. 用户能在 1 分钟内创建新 Codex 账号目录。
2. 用户能从 A 同步 session 到 B relay 并 resume。
3. 不覆盖任何目标账号 history。
4. 不复制 auth。
5. 支持 dry-run，用户清楚知道会改哪些文件。
6. 开源用户能通过 README 在 5 分钟内跑起来。

长期指标：

- GitHub stars / issues 活跃度。
- 用户反馈中 session relay 成功率。
- 因误同步造成的数据事故为 0。
- 对 Codex 内部格式变化有兼容策略。

---

## 11. 开源定位

建议项目名称：

- `codex-relay`
- `codex-session-manager`
- `codex-account-switcher`

推荐名称：`codex-relay`

一句话描述：

```text
A local-first desktop app for managing multiple Codex CLI accounts and safely relaying sessions across isolated CODEX_HOME directories.
```

---

## 12. 参考资料

- OpenAI Codex CLI docs: https://developers.openai.com/codex/cli
- OpenAI Codex CLI features / resume: https://developers.openai.com/codex/cli/features
- OpenAI Codex CLI reference: https://developers.openai.com/codex/cli/reference
- Tauri calling Rust from frontend: https://v2.tauri.app/develop/calling-rust/
- Tauri security: https://v2.tauri.app/security/
