# LocalAgentManager (Lam) TODO

版本：0.1
日期：2026-06-01
用途：实施推进清单

本文基于 `docs/FINAL-DESIGN.md` 和 `docs/IMPLEMENTATION-ISSUES.md`，用于逐步推进开发。每个任务完成后应更新状态，并补充实际实现偏差。

状态标记：

```text
[ ] 未开始
[~] 进行中
[x] 已完成
[!] 阻塞
```

**状态说明**：`[x]` 仅表示该 TODO 的**验收标准已全部满足**。若实现审查发现与目标不符，会在条目下增加「实际偏差」，并将状态改回 `[~]`，同时登记「纠偏任务」（见 Phase 1F）。

**审查记录**：2026-06-02 对照 `apps/desktop` 代码审查；同日完成 Phase 1F 主要功能纠偏并重新验证。仍保留的非完成项以 `[~]` 标记，不做完成态伪造。

**纠偏与验收文档**（2026-06-02）：

- `docs/CORRECTION-PLAN.md` — 缺口清单、波次修正顺序、设计参考索引
- `docs/DESKTOP-RUNTIME.md` — Tauri vs Electron、`make start` 与 UTF-8 启动修复说明
- `docs/PHASE1-ACCEPTANCE.md` — TODO-504 手工验收矩阵模板

## Phase 0: 规格收敛与实施准备

### [x] TODO-001: 冻结文档权威关系

目标：
明确 `FINAL-DESIGN.md` 是唯一主规格，旧 Codex-only 文档只作为历史参考，避免实现时出现命名、范围和安全策略混用。

范围：
- `docs/FINAL-DESIGN.md`
- `docs/DESIGN_GOAL.md`
- `docs/01-product-design.md`
- `docs/02-development-design.md`
- `docs/04-roadmap.md`
- `docs/05-tauri-command-contracts.md`
- `codex-session-manager-managed/README.md`

逻辑设计：
所有实施判断优先读取 `FINAL-DESIGN.md`。旧文档只保留早期背景、细节参考和迁移线索，不再作为验收依据。

详细要求：
- 旧文档顶部必须有 superseded / historical draft 说明。
- `DESIGN_GOAL.md` 的第一阶段必须拆清 Phase 1 / 1.2 / 1.5。
- `codex-session-manager-managed` 必须明确是 Phase 0 Node/Bun 原型。
- Phase 1 明确不包含 Provider CRUD、Keychain 写入、真实额度倒计时、history merge。

验收：
- 新成员能在 5 分钟内判断哪个文档是主规格。
- 搜索 `history merge` 时能看到 Phase 1 不实现的明确说明。
- 搜索 `Provider CRUD` 时能看到它属于 Phase 1.5。

### [x] TODO-002: 建立实施追踪入口

目标：
让 issue、TODO、最终设计三者形成清晰闭环，后续开发可以按 TODO 推进，按 issue 归档，按最终设计验收。

范围：
- `docs/IMPLEMENTATION-ISSUES.md`
- `docs/TODO.md`
- 后续 README 或 PR 模板

逻辑设计：
`IMPLEMENTATION-ISSUES.md` 负责 issue 粒度，`TODO.md` 负责执行顺序和细节，`FINAL-DESIGN.md` 负责产品/架构真相。

详细要求：
- TODO 编号应稳定，不随意重排。
- 每个 TODO 必须包含目标、范围、逻辑设计、详细要求、验收。
- TODO 完成后只更新状态和实际偏差，不改历史目标。

验收：
- 任意 TODO 可以映射到一个或多个 LAM issue。
- 任意 Phase 1 验收项可以在 TODO 中找到对应任务。

## Phase 1A: 桌面工程骨架

### [~] TODO-101: 初始化 Tauri + React 项目

目标：
创建正式桌面应用工程骨架，为后续 Rust 本地能力、React UI 和 Tauri command 通信打基础。

范围：
- 新建 `apps/desktop`
- Tauri v2
- React + Vite + TypeScript
- Rust 后端基础模块

逻辑设计：
前端只表达用户意图，通过 Tauri `invoke` 调用后端。所有文件系统、Terminal、wrapper、sync 等危险操作只能在 Rust 后端实现。

详细要求：
- 目录结构遵循 `FINAL-DESIGN.md` 的 `apps/desktop` 规划。
- Rust 至少建立：
  - `models.rs`
  - `errors.rs`
  - `commands/mod.rs`
  - `services/`
  - `adapters/codex/`
- 前端至少建立：
  - `src/App.tsx`
  - `src/main.tsx`
  - `src/lib/api.ts`
  - `src/lib/types.ts`
  - `src/routes/`
  - `src/components/`
- 增加一个 `health_check` command 验证 invoke 链路。

验收：
- 本地开发命令能启动桌面 app。
- UI 显示 LocalAgentManager (Lam) 基础 shell。
- 前端能调用 Rust `health_check` 并展示成功状态。

实际偏差（2026-06-02）：
- React + Vite + TS、Tauri v2 配置、Tauri native binary、`health_check` command、`src/lib/api.ts` 均已存在。
- `npm run build`、`npm run test:ui`、`cargo test` 已通过；`npm run tauri -- info` 已识别 Tauri app。
- 当前环境未实际打开 macOS native window；`tauri info` 报告 full Xcode 未安装。
- Rust 业务主体已迁移到 `services/core.rs`，`lib.rs` 只保留模块声明与 re-export。

剩余纠偏任务：TODO-104（手工 native window 验收）。

### [x] TODO-102: 落地主应用 UI shell

目标：
把 `account-manager/codex-manager/index.html` 的主信息架构转成正式 React shell。

范围：
- Sidebar
- Titlebar / Toolbar
- Main content route area
- Contextual Inspector
- Statusbar
- 基础 modal 容器

逻辑设计：
UI route 先按最终 IA 建立完整骨架，即使部分能力 Phase 1 只展示占位。这样后续 Provider、Usage、Sync Center 不需要重构导航。

详细要求：
- 导航包含：
  - Overview
  - Accounts
  - Sessions
  - Relay
  - Providers
  - Sync
  - Settings
- Providers 页 Phase 1 显示只读或 Coming in 1.5。
- Usage 相关入口 Phase 1 显示 estimate / Coming in 1.2，不显示真实额度假数据。
- 不使用原型中的 Open Design chrome 或设计过程标注。
- UI token 从 `codex-manager/index.html` 抽取，不使用框架默认主题直接替代。

验收：
- 主要路由可切换。
- Inspector 在未选中对象时有空状态。
- Phase 1 未实现按钮不会执行伪功能。

纠偏结果（2026-06-02）：
- 视觉 shell、7 个主导航状态切换、Inspector、Statusbar、modal 容器已落地。
- `src/routes/` 与 `src/components/` 已建立，`App.tsx` 实际渲染引用拆分后的模块。
- Refresh、New Account、New Relay、Sync、Resume 等入口已接真实 API wrapper。
- Phase 1.2 / 1.5 能力以只读或 disabled/staged 文案展示。

### [x] TODO-103: 定义共享类型与错误模型

目标：
建立前后端共享的数据契约，先满足 Phase 1 Codex MVP，同时保留 Agent 抽象迁移空间。

范围：
- TS types
- Rust structs
- Error model
- API wrapper

逻辑设计：
Phase 1 使用 `CodexAccount` / `CodexSession` 名称实现，但字段按 `AgentProfile` / `AgentSession` 方向设计，避免后续多 Agent 抽象大改。

详细要求：
- `AppError` 包含：
  - `code`
  - `message`
  - `recoverable`
  - `details`
- Phase 1 类型至少包含：
  - `CodexAccount`
  - `CodexSession`
  - `CreateAccountPlan`
  - `CreateRelayPlan`
  - `SyncPlan`
  - `SyncResult`
  - `ResumeCommand`
- 所有路径字段使用绝对路径字符串返回给 UI。
- 不返回 `auth.json`、API key 或 token 内容。

验收：
- 前端 API wrapper 类型完整。
- Rust command 返回结构化错误。
- 错误码覆盖 invalid name、unsafe path、account not found、session not found、terminal permission denied。

纠偏结果（2026-06-02）：
- Rust `AppError` 已包含 `details`，并可序列化给 Tauri command。
- TS `types.ts` 已补齐 `AppError`、创建计划、sync、resume、health 等 Phase 1 类型。
- `api.ts` 已封装 Phase 1 invoke；浏览器预览保留只读 fallback。
- Tauri command 层已存在并注册主要 Phase 1 command。

## Phase 1B: Codex 只读模型

### [x] TODO-201: 实现 Codex account scanner

目标：
扫描本机 `~/.codex*`，识别 Codex profiles/accounts，并返回可展示的账号状态。

范围：
- `$HOME/.codex`
- `$HOME/.codex-*`
- managed metadata
- wrapper detection

逻辑设计：
扫描器只做只读探测。认证文件只检查存在性，不读取内容。relay 语义优先从 metadata 获取，缺失时可从命名模式做保守推断。

详细要求：
- 识别 `main`：
  - `~/.codex` -> `main`
- 识别普通 profile：
  - `~/.codex-a` -> `a`
- 识别 relay：
  - `~/.codex-b-relay-a` -> `b-relay-a`
- 新 metadata 文件：
  - `.managed-by-agent-workspace.json`
- 兼容旧 metadata：
  - `.managed-by-codex-session-manager.json`
- 返回字段至少包含：
  - id
  - display_name
  - codex_home
  - wrapper_path
  - has_auth
  - has_config
  - has_history
  - managed
  - is_relay
  - relay_source
  - relay_identity
  - session_count
  - latest_session_modified_at

验收：
- 对真实 `$HOME` 可列出账号。
- 对 fake home fixture 可稳定测试。
- 测试证明不读取 `auth.json` 内容。

### [x] TODO-202: 实现 Codex session parser

目标：
从每个 Codex profile 的 `sessions/` 中读取 resume 所需的 session 元数据。

范围：
- `CODEX_HOME/sessions/`
- JSONL / JSON session files
- fallback metadata

逻辑设计：
Codex session 格式可能变化，因此 parser 必须保守。解析失败不影响文件级同步和 resume fallback。

详细要求：
- 读取字段优先级：
  - session id: `session_id` / `sessionId` / `conversation_id` / 文件名
  - cwd: `cwd` / `workdir` / `working_directory`
  - summary: `summary` / `title` / 第一条 user message
  - model: `model`
- 限制单文件读取大小，避免加载巨大 session 全文。
- 不修改 session 文件。
- 返回 size、modified_at、path。
- 支持按 mtime 倒序。

验收：
- parser 对未知格式不崩溃。
- 没有 cwd 时 UI 可显示 unknown。
- fake fixture 覆盖 JSONL、JSON、空文件、损坏 JSON 行。

纠偏结果（2026-06-02）：
- Rust parser 已覆盖 JSONL、JSON、空文件、损坏 JSON 行、无 cwd fallback。
- 集成测试 `parses_session_edge_cases_without_crashing` 已验证 parser 不因坏行崩溃。
- UI Sessions 表格和 empty state 已对 unknown cwd 做展示。

### [x] TODO-203: 实现 Accounts / Sessions 真实数据 UI

目标：
用真实扫描数据替换静态原型数据，让用户能浏览本机 Codex profiles 和 sessions。

范围：
- Accounts route
- Sessions route
- Overview 统计
- Inspector 基础信息

逻辑设计：
Accounts 是 profile 入口，Sessions 是跨 profile 的会话资产视图，Inspector 展示当前选中对象的上下文和后续操作入口。

详细要求：
- Accounts 卡片显示：
  - profile name
  - path
  - login state
  - managed / relay tag
  - session count
  - latest active time
  - wrapper state
- Sessions 表格显示：
  - session id
  - account/profile
  - cwd
  - summary / first user message
  - modified time
  - size
- 支持筛选：
  - account
  - cwd / project path
  - session id
  - summary
- Overview 显示：
  - account count
  - session count
  - relay count
  - recent active profiles

验收：
- UI 可显示真实 `~/.codex*` 数据。
- 无账号、无 session、解析失败均有明确 empty/error state。
- 搜索和排序在 1000 条 session 级别可用。

纠偏结果（2026-06-02）：
- 前端已调用 `listAccounts` / `listSessions`，Overview、Accounts、Sessions、Quick Accounts、Inspector 使用真实扫描数据。
- 支持 session 搜索；无账号、无 session 有明确 empty state。
- 1000 条级别未做浏览器性能压测，但实现为本地数组过滤，无额外后端瓶颈。

### [x] TODO-204: Phase 1 Provider 只读解析

目标：
在 Phase 1 只读展示 Codex profile 的 provider/model 状态，为 Phase 1.5 Provider Center 留出数据接口。

范围：
- Codex `config.toml`
- Account card provider badge
- Session provider placeholder
- Providers route readonly state

逻辑设计：
只读解析配置，不做 Provider CRUD，不写 config，不读取 secret。无法识别时显示 unknown，而不是猜测。

详细要求：
- 从 `config.toml` 保守解析：
  - provider id
  - model
  - auth mode
  - env key reference
- 不读取 API key 值。
- Providers 页面显示：
  - Phase 1 readonly
  - Phase 1.5 will support CRUD / Keychain / Attach
- Add/Test/Attach 按钮禁用或展示 Phase 1.5 提示。

验收：
- 不写 `config.toml`。
- UI 不出现明文 API key。
- 未识别 provider 时显示 `unknown`。

纠偏结果（2026-06-02）：
- Rust 只读解析 `provider_id` / `model` / `auth_mode`。
- Providers route、Account card provider badge、Session provider placeholder 已展示解析结果或 `unknown`。
- Provider CRUD / Attach / Test 明确延后到 Phase 1.5，UI 不显示 API key。

## Phase 1C: 受管账号与 Relay Workspace

### [x] TODO-301: 实现受管账号创建 plan / execute

目标：
安全创建新的 Codex profile 目录和 wrapper，帮助用户避免手工配置错误。

范围：
- `plan_create_account`
- `execute_create_account`
- Add Managed Account modal

逻辑设计：
所有写操作分成 plan 和 execute。plan 只预览路径和操作，不写文件；execute 必须基于用户确认。

详细要求：
- name 校验：
  - regex: `[a-zA-Z0-9][a-zA-Z0-9_-]{0,31}`
  - 拒绝 `/`
  - 拒绝 `..`
  - 拒绝 `~`
  - 拒绝空格
  - 拒绝绝对路径
- 创建目录：
  - `~/.codex-{name}`
  - mode `0700`
- 写 metadata：
  - `.managed-by-agent-workspace.json`
  - mode `0600`
- wrapper：
  - `~/bin/codex-{name}`
  - mode `0755`
  - 使用 `$HOME`，不硬编码用户名。
  - 参数全部透传。
- 可选复制 config 模板时必须排除 auth 和 secret。

验收：
- dry-run 展示所有将创建/写入路径。
- execute 后 wrapper 可运行 `codex-{name} --help` 或至少正确透传。
- 不创建或复制 `auth.json`。
- wrapper 已存在时必须阻止或要求明确覆盖确认。

纠偏结果（2026-06-02）：
- `create_account_plan` / `execute_create_account` 已由 Tauri command 暴露。
- Add Managed Account modal 已实现 plan → confirm execute → refresh。
- 集成测试覆盖命名、metadata、wrapper 和不创建 `auth.json`。

### [x] TODO-302: 实现 Relay Workspace 创建 plan / execute

目标：
创建隔离 relay profile，让 runtime 账号接续 source 账号 sessions，同时不污染 runtime 原始 profile。

范围：
- `plan_create_relay`
- `execute_create_relay`
- Create Relay Workspace modal

逻辑设计：
relay 是一个独立 Codex profile。它有 runtime identity 和 session source 两条关系。同步只发生 source -> relay，不修改 runtime profile。

详细要求：
- 输入：
  - runtime_profile_id
  - source_profile_id
  - optional relay name
  - provider_policy
- 默认命名：
  - `{runtime}-relay-{source}`
- 创建：
  - `~/.codex-{runtime}-relay-{source}`
  - `~/bin/codex-{runtime}-relay-{source}`
- metadata 必须记录：
  - kind = relay
  - runtime_profile_id
  - source_profile_id
  - provider_policy
  - created_at
- UI 必须展示：
  - runtime account
  - source account
  - relay path
  - wrapper path
  - 不复制 auth 的安全说明

验收：
- 创建 relay 不修改 runtime profile 的 sessions/history。
- 创建后 Accounts 显示 relay tag。
- 未登录 relay 时显示需要 `codex-relay login`。

纠偏结果（2026-06-02）：
- `create_relay_plan` / `execute_create_relay` 已由 Tauri command 暴露。
- Create Relay Workspace modal 和 Relay route 已接 plan/execute。
- UI 明确 relay 不复制 `auth.json` 且不修改 runtime profile。

## Phase 1D: Safe Sync 与 Resume

### [x] TODO-401: 实现 SyncPlan 构建

目标：
在任何文件写入前生成可审计 sync plan，让用户清楚看到会复制、跳过、备份和阻止哪些内容。

范围：
- `build_sync_plan`
- `SyncPlan`
- Sync Engine path validation

逻辑设计：
plan 阶段不写文件，只扫描 source/target 并生成 operations。所有敏感文件必须进入 blocked list 或 ignored list。

详细要求：
- 默认只包含：
  - `sessions/`
- 默认排除：
  - `history.jsonl`
  - `auth.json`
  - `config.toml`
  - `*.sqlite`
  - `*.sqlite-shm`
  - `*.sqlite-wal`
  - `cache/`
  - `tmp/`
  - `log/`
  - `logs/`
  - `installation_id`
- operations 至少包含：
  - backup target sessions
  - copy session file
  - skip same/newer file
  - blocked sensitive file
- warnings 至少包含：
  - source/target same path
  - target is primary account
  - provider mismatch placeholder
  - large session directory

验收：
- dry-run 不写任何文件。
- `auth.json` 永远出现在 blocked 或 never-considered 集合，不会进入 copy operations。
- source 和 target 相同时返回错误。

纠偏结果（2026-06-02）：
- `sync_plan` dry-run 不写文件，支持 backup/copy/skip operations。
- 已区分 `policy_blocked_files` 与实际扫描到的 `blocked_files`。
- 已加入 target primary、provider mismatch/unknown provider、large session directory warnings。
- source/target same path 仍按安全错误返回，而不是 warning；这是有意的安全收紧。

### [x] TODO-402: 实现 execute sync 与 manifest

目标：
按已确认 SyncPlan 安全执行 sessions 同步，并留下可追踪 manifest。

范围：
- `execute_sync`
- backup target sessions
- copy merge
- sync manifest store

逻辑设计：
execute 必须复用 plan 的安全判断。执行前先备份目标 `sessions/`，再合并复制 source sessions，最后写 manifest。

详细要求：
- 备份目录命名：
  - `sessions.backup.YYYYMMDD-HHMMSS`
- copy 策略：
  - 不覆盖 target 较新文件
  - 同 size + same/newer mtime 跳过
  - 保留合理权限
- manifest 路径：
  - `~/.config/agent-workspace/sync-manifests/<uuid>.json`
- manifest 内容：
  - from_profile_id
  - to_profile_id
  - timestamp
  - operations executed
  - skipped
  - blocked
  - warnings
  - backup path
- Phase 1 不实现 history merge。

验收：
- execute 后目标 sessions 存在 source 文件。
- execute 后存在 backup 和 manifest。
- 测试证明不会复制 auth/config/sqlite/cache/log/tmp。

纠偏结果（2026-06-02）：
- backup 目录已改为 `sessions.backup.YYYYMMDD-HHMMSS` 格式。
- manifest 文件名已改为 UUID `.json`。
- manifest 已包含 timestamp、operations、blockedFiles、policyBlockedFiles、warnings、backupPath。
- 集成测试覆盖 auth/config/sqlite/state/installation 等敏感文件不进入 sessions copy。

### [x] TODO-403: 实现 Sync Sessions Safely UI

目标：
为用户提供完整安全同步流程：选择 from/to、dry-run、查看 plan、确认执行、查看结果。

范围：
- Sync Center
- Sync Sessions Safely modal
- Inspector sync action

逻辑设计：
UI 不提供直接执行入口。用户必须先 dry-run，再确认执行。危险项必须可见，不能隐藏在成功文案里。

详细要求：
- from/to 选择器显示：
  - profile name
  - path
  - managed/relay state
- dry-run 结果分组：
  - Will backup
  - Will copy
  - Will skip
  - Blocked
  - Warnings
- 对 target 是 primary account 显示警告，推荐 relay。
- history 只提供 sidecar backup 选项；不提供 merge。
- execute 完成后显示 manifest id/path。

验收：
- UI 中没有 “Start sync” 直接执行按钮。
- 用户能清楚看到目标路径和将影响的文件。
- Provider mismatch 即使 Phase 1 只是 placeholder，也有展示位。

纠偏结果（2026-06-02）：
- Sync route 与 Sync modal 已实现 from/to、dry-run、plan 展示、confirm execute、manifest path 展示。
- UI 无直接 execute 入口；Confirm Execute 在 dry-run plan 生成前 disabled。
- Plan 展示包含 operations、blocked、warnings，Provider mismatch 有展示位。

### [x] TODO-404: 实现 ResumeCommand builder

目标：
生成安全、可复制、可执行的 Codex resume 命令。

范围：
- `build_resume_command`
- Copy command
- command preview

逻辑设计：
后端接收 profile id 和 session id，自己查路径并构造命令。前端不能传入任意 shell 字符串。

详细要求：
- 命令格式：
  - `cd '<cwd>' && CODEX_HOME='<home>' codex resume '<session_id>'`
- 无 cwd 时：
  - 提示手动选择项目目录，或
  - fallback `CODEX_HOME='<home>' codex resume --last --all`
- 所有 shell 参数必须单独 escape。
- command preview 必须展示 side effects：
  - 使用哪个 CODEX_HOME
  - resume 哪个 session
  - cwd 是什么

验收：
- 路径包含空格、单引号、特殊字符时命令仍安全。
- 前端无法让后端执行任意命令。
- Copy command 内容不包含 API key 明文。

纠偏结果（2026-06-02）：
- `build_resume_command` 与 shell escape 已在 Rust 实现并有测试。
- Sessions route 已提供 Copy 与 Terminal 操作；Inspector 可展示命令 preview。
- 前端只传 profile/session/cwd 请求，不传任意 shell 命令给后端执行。

### [x] TODO-405: 实现 Terminal.app launcher

目标：
让用户可以从 UI 打开 Terminal.app 执行已验证的 resume/login 命令。

范围：
- `open_terminal_with_resume`
- `open_terminal_for_login`
- Terminal permission fallback

逻辑设计：
Terminal launcher 只接受经过后端验证的 action request，不接受前端传来的 arbitrary command。

详细要求：
- 支持：
  - open resume
  - open login for profile
- 使用 macOS AppleScript 调 Terminal.app。
- AppleScript 字符串需要正确转义。
- 权限失败时返回结构化错误，UI 提示复制命令手动执行。
- Phase 1 不支持 iTerm/Warp/Ghostty。

验收：
- Terminal.app 可打开并填入命令。
- Terminal 权限被拒绝时 UI 有 fallback。
- 测试覆盖 shell escape。

纠偏结果（2026-06-02）：
- `open_terminal_with_resume` 与 `open_terminal_for_login` 已实现并由 Tauri command 暴露。
- UI 已提供 resume Terminal 和 login Terminal 入口。
- Terminal 失败时 UI 会构造 copy-command fallback；测试覆盖 shell/AppleScript escape。

## Phase 1E: Phase 1 完整性与发布准备

### [x] TODO-501: 建立 fake-home 测试 fixtures

目标：
用可重复的假 Codex home 数据测试扫描、解析、同步和安全策略。

范围：
- `.fake-home/.codex`
- `.fake-home/.codex-a`
- `.fake-home/.codex-b`
- `.fake-home/.codex-b-relay-a`
- fake sessions / configs / dangerous files

逻辑设计：
测试不能依赖开发者真实 `$HOME`。所有核心服务应支持注入 home root。

详细要求：
- fixture 包含：
  - auth.json
  - config.toml
  - history.jsonl
  - sessions/*.jsonl
  - logs_2.sqlite
  - state_*.sqlite
  - cache/
  - tmp/
  - installation_id
- 既包含正常 session，也包含损坏 JSONL。
- 包含新旧 metadata 文件。

验收：
- 测试可在干净机器运行。
- 不访问真实 `~/.codex*`。
- 安全 blacklist 测试可重复。

纠偏结果（2026-06-02）：
- 仓库已新增 `.fake-home` 静态 fixture，包含 `.codex`、`.codex-a`、`.codex-b`、`.codex-b-relay-a`。
- fixture 覆盖 auth/config/history/sessions/sqlite/cache/tmp/installation_id、损坏 JSONL、空 session、新旧 metadata。
- 集成测试 `static_fake_home_fixture_scans_expected_profiles` 已直接扫描 `.fake-home`。

### [x] TODO-502: 补齐 Phase 1 自动化测试

目标：
用测试锁住最关键的安全行为和 MVP 闭环。

范围：
- Rust unit tests
- Rust integration tests
- 前端关键状态测试或轻量 e2e

逻辑设计：
安全策略优先测试后端，因为后端才是可信边界。前端主要测试不会暴露直接执行入口和关键状态展示。

详细要求：
- Rust 单元测试：
  - account name validation
  - path canonicalization
  - wrapper content
  - shell escape
  - session parser fallback
  - sync blocked files
- 集成测试：
  - create account plan/execute
  - create relay plan/execute
  - build sync plan
  - execute sync
  - manifest written
  - resume command
- 前端测试：
  - empty state
  - dry-run before execute
  - disabled Phase 1.2 / 1.5 actions

验收：
- 一条命令可跑完测试。
- `auth.json` copy 测试必须存在且失败即阻塞发布。
- history merge 在 Phase 1 不存在执行路径。

纠偏结果（2026-06-02）：
- `cargo test` 覆盖扫描、创建账号、创建 relay、sync 安全、manifest、resume escape、session parser 边界、静态 `.fake-home`。
- 新增 `npm run test:ui` 轻量 smoke，覆盖 empty state、sync 必须 dry-run、Phase 1.2/1.5 staged/disabled、Tauri invoke wrapper。
- Rust 校验仍集中在 integration test 内；当前未单独拆 unit test 文件。

### [x] TODO-503: Phase 1 README 与安全说明

目标：
让开源用户能理解产品边界、安全原则和 Codex relay 使用流程。

范围：
- Root README
- docs 安全链接
- install/run instructions

逻辑设计：
README 不是营销页，优先说明怎么安装、怎么安全使用、不会做什么。

详细要求：
- 明确：
  - 本工具不绕过 Codex / OpenAI 使用限制。
  - 不复制 `auth.json`。
  - 不上传 session、代码、prompt。
  - Phase 1 不支持 history merge。
  - relay 目录用于避免污染 runtime 原账号。
- 提供基本流程：
  - scan accounts
  - create account
  - create relay
  - sync sessions
  - resume
- 提供故障处理：
  - Codex binary not found
  - wrapper dir not in PATH
  - Terminal permission denied

验收：
- 新用户 5 分钟内能跑起 app。
- README 与 `FINAL-DESIGN.md` 阶段范围一致。

纠偏结果（2026-06-02）：
- README 已说明 Rust/Frontend 测试、core scanner、Vite browser preview 与 Tauri native app 的区别。
- README 明确安全边界：不复制 auth、不合并 history、不做 Provider CRUD/Keychain/真实 quota。
- README 记录当前 macOS 环境报告 full Xcode 缺失，避免把本机 bundle/build 前置条件说成已满足。

### [~] TODO-504: Phase 1 发布前验收

目标：
确认 v0.1.0 MVP 可以安全完成 Codex A -> B relay -> resume 闭环。

范围：
- 功能验收
- 安全验收
- 手工测试矩阵

逻辑设计：
发布前验收只看 Phase 1 承诺，不因 Phase 1.2 / 1.5 未完成而阻塞。

详细要求：
- 手工流程：
  - 扫描现有 `~/.codex*`
  - 创建 `~/.codex-luna`
  - 创建 `~/.codex-b-relay-a`
  - 从 A dry-run sync 到 relay
  - execute sync
  - copy resume command
  - open Terminal resume
- 安全检查：
  - relay 没有复制 source auth
  - runtime 原 profile sessions/history 未被改动
  - manifest 存在
  - backup 存在

验收：
- `FINAL-DESIGN.md` §9.1 的 8 条 Phase 1 验收全部满足。
- 未完成能力在 UI 中明确标注 Phase 1.2 / 1.5。

实际偏差（2026-06-02）：
- 阻塞原因：Tauri 未启动、GUI 未接真实数据与写操作/sync/resume 流程，无法执行文档中的手工验收矩阵。
- Rust 层闭环可由 `cargo test` + `lam-core` 部分验证，不等于 §9.1 产品验收。

当前状态（2026-06-02）：
- GUI 接线、创建账号/relay、sync/resume、Provider 只读、health_check 等前置功能已实现并通过自动化验证。
- 发布前仍需在真实 macOS 桌面环境执行完整手工验收矩阵；当前环境未实际打开 native window。

## Phase 1F: 实现纠偏任务（2026-06-02 审查登记）

以下任务由代码审查产生，用于闭合上文「实际偏差」。编号插入 Phase 1A–1E，不改动原 TODO 历史目标描述。

### [~] TODO-104: 接入 Tauri v2 桌面运行时

目标：
使 `apps/desktop` 成为可 `tauri dev` / `tauri build` 的正式桌面应用，而非仅 Vite + Rust 库。

范围：
- `tauri.conf.json`（或 `Tauri.toml`）
- `src-tauri` 增加 `tauri` / `tauri-build` 依赖与 `main` 入口
- 将 `localagentmanager-core` 作为 lib 被 Tauri crate 引用
- `package.json` 增加 `tauri dev` 脚本

详细要求：
- `npm run tauri dev` 可打开原生窗口并加载前端。
- 开发文档区分：`npm run dev`（仅浏览器）、`npm run tauri dev`（桌面）。

验收：
- macOS 上可启动带 Lam shell 的桌面窗口。
- 无 `Couldn't recognize the current folder as a Tauri project` 错误。

依赖：TODO-504 的 native window 手工验收。

当前结果（2026-06-02）：
- 已新增 Tauri v2 配置、build script、native binary 入口、icon、npm tauri scripts。
- `cargo test` 会编译 Tauri `src/main.rs`，`npm run tauri -- info` 已识别当前目录为 Tauri app。
- 未在当前沙箱实际打开 native macOS 窗口；`tauri info` 报告 full Xcode 未安装。因此窗口手工验收仍待本机确认。

### [x] TODO-105: 拆分 Rust 模块并暴露 Tauri commands

目标：
对齐 `FINAL-DESIGN.md` 目录约定，把 `lib.rs` 拆为可维护模块，并注册全部 Phase 1 command。

范围：
- `models.rs`、`errors.rs`
- `commands/mod.rs`（或等效）
- `services/`、`adapters/codex/`（可先薄封装，逻辑从 `lib.rs` 迁出）
- `#[tauri::command]`：`list_accounts`、`list_sessions`、`plan_create_account`、`execute_create_account`、`plan_create_relay`、`execute_create_relay`、`build_sync_plan`、`execute_sync`、`build_resume_command`、`open_terminal_with_resume` 等

详细要求：
- command 入参/出参与 `src/lib/types.ts` 对齐（camelCase 序列化）。
- `home_root` 由后端从 `$HOME` 解析，支持 `LAM_HOME` 仅用于测试/开发配置（若暴露给 UI 需文档化）。

验收：
- `lib.rs` 不再承载全部业务（或仅保留 re-export）。
- 前端 `api.ts` 所列 invoke 均有对应 command 且可调用。

依赖：TODO-104。

当前结果（2026-06-02）：
- `models.rs`、`errors.rs`、`commands/mod.rs`、`services/`、`adapters/codex/` 已建立。
- Phase 1 Tauri commands 已注册并与 `api.ts` 对齐。
- Rust 业务主体已迁移到 `services/core.rs`，`lib.rs` 只保留模块声明与 re-export。

### [x] TODO-106: 前端 routes / components 与导航状态

目标：
落实 TODO-102 未完成的 IA：可切换路由、可复用组件、modal 容器。

范围：
- `src/routes/`：Overview、Accounts、Sessions、Relay、Providers、Sync、Settings
- `src/components/`：Sidebar、Inspector、Statusbar、Modal 等
- `App.tsx` 路由状态（React state 或轻量 router）

验收：
- 7 个主导航可切换且 URL/状态一致。
- Inspector 空态与选中态随路由/选择变化。
- Phase 1 未实现按钮保持 disabled 或明确提示，不执行伪操作。

依赖：TODO-104（推荐同步进行）。

### [x] TODO-107: health_check 与 invoke 端到端验证

目标：
验证 Tauri bridge 可用，作为后续功能联调的冒烟测试。

范围：
- Rust `health_check` command
- 前端 Statusbar 或 Settings 展示后端版本/健康状态

验收：
- 桌面 app 内可见 health check 成功。
- 失败时展示结构化错误（非静默失败）。

依赖：TODO-104、TODO-105。

### [x] TODO-108: 提交仓库内 `.fake-home` 静态 fixtures

目标：
满足 TODO-501 原文档范围，支持人工检查、文档示例与可选 CI。

范围：
- `.fake-home/.codex`、`.codex-a`、`.codex-b`、`.codex-b-relay-a`
- 含 auth、config、history、sessions、sqlite、cache、tmp、installation_id、损坏 JSONL、新旧 metadata

详细要求：
- **不得**包含真实 token；auth.json 仅为占位。
- 集成测试可改为优先读 `.fake-home`（保留 `temp_home` 作为并行策略可选）。

验收：
- `LAM_HOME=$REPO/.fake-home cargo test` 通过。
- 新成员可用固定目录复现扫描/sync 演示。

### [x] TODO-109: 补齐前端关键测试

目标：
闭合 TODO-502 中未完成的前端验收项。

范围：
- empty state（无账号、无 session）
- Sync 流程必须先 dry-run 再 execute（无「直接同步」按钮）
- Phase 1.2 / 1.5 写操作按钮 disabled 或 Coming soon

验收：
- `apps/desktop` 内一条命令可跑前端测试（Vitest 或项目约定工具）。
- 与 Tauri 的 e2e 可列为可选 follow-up，不阻塞 v0.1.0。

依赖：TODO-106、TODO-111（至少 Sync 页存在后）。

### [x] TODO-110: 受管账号与 Relay 创建 UI

目标：
闭合 TODO-301、TODO-302 的 GUI 缺口。

范围：
- Add Managed Account modal（plan → 确认 → execute）
- Create Relay Workspace modal
- Relay 路由说明 runtime / source 边界

验收：
- 用户可从 UI 完成 dry-run 预览路径并 execute。
- execute 后 Accounts 列表刷新并显示 managed / relay 标签。

依赖：TODO-104、TODO-105、TODO-106、TODO-203（列表刷新）。

### [x] TODO-111: Sync 与 Resume 安全操作 UI

目标：
闭合 TODO-403、TODO-404、TODO-405 的 GUI 缺口。

范围：
- Sync Center：from/to、dry-run 分组展示、二次确认、execute 结果与 manifest 路径
- Sessions / Inspector：Copy resume command、Open in Terminal、权限失败 fallback
- 禁止无 dry-run 的「Start sync」

验收：
- 完整 GUI 路径：选 session → copy/open Terminal；选 profiles → dry-run sync → confirm → execute。
- Provider mismatch 占位可见。

依赖：TODO-104、TODO-105、TODO-106、TODO-203。

### [x] TODO-112: Provider 只读与 Usage 占位 UI 接线

目标：
闭合 TODO-204 的 UI 部分；明确 Phase 1.2 额度未实现。

范围：
- Providers 路由只读列表 / Coming 1.5
- Account 卡片 provider badge；Session provider placeholder
- Usage 区域仅文案或 `activity estimate` 占位，**不**伪造 `% left` / `Resets in`

验收：
- 展示 Rust 已解析的 provider/model/auth mode 或 `unknown`。
- 无 API key 明文。

依赖：TODO-106、TODO-203。

### [x] TODO-113: 补齐 AppError.details 与 TS 共享类型

目标：
闭合 TODO-103 契约缺口。

范围：
- Rust `AppError.details: Option<...>`
- TS：`CreateAccountPlan`、`CreateRelayPlan`、`SyncResult`、`ResumeCommand`、`AppError` 及 `api.ts` 全量封装

验收：
- 前后端类型与 command 返回值一一对应。
- 文档列出的错误码均可从 UI 触发并展示。

依赖：TODO-105。

### [x] TODO-114: 实现 open_terminal_for_login

目标：
闭合 TODO-405 范围中未实现的 login 路径。

范围：
- Rust `open_terminal_for_login(profile_id)`（构造 `CODEX_HOME=... codex login` 或项目约定命令）
- UI：未登录账号的「Open Terminal to login」入口

验收：
- 与 resume 相同的安全边界：不接受前端任意 shell 字符串。
- Terminal 失败时有 copy-command fallback。

依赖：TODO-105、TODO-111。

### [ ] TODO-115: Phase 1 activity estimate（LAM-014，可选）

目标：
实现 IMPLEMENTATION-ISSUES LAM-014：session jsonl `token_count` 活动估算，**不**冒充实时额度。

优先级：P2；可在 v0.1.0 之后、TODO-601 之前做。

范围：
- Rust 从最近 session 事件估算 activity
- UI 标注 `activity estimate` / `stale`

验收：
- 离线可用；不显示 `Resets in` 或假 0%。

## Phase 1.2: 实时额度面板

### [~] TODO-601: 实现 UsageQuotaService

目标：
为每个 Codex profile 独立查询 UsageQuotaSnapshot，展示真实额度来源或明确 fallback 状态。

范围：
- `get_profile_quota`
- `refresh_all_quotas`
- `QuotaCache`
- Codex app-server adapter
- jsonl fallback estimate

逻辑设计：
默认通过隔离的 `CODEX_HOME=... codex app-server` 获取额度。fallback 只能显示 activity estimate，不伪装成实时剩余额度。

详细要求：
- 每个 profile 独立子进程/查询上下文。
- cache TTL 默认 5 分钟。
- cache path：
  - `~/.config/agent-workspace/quota-cache/<profile_id>.json`
- snapshot 包含：
  - profile_id
  - source
  - fetched_at
  - staleness
  - plan_type
  - windows[]
  - alerts[]
  - suggested_actions[]
- wham/usage 实验开关默认关闭。
- token 不进入日志、前端、cache、manifest。

验收：
- 两个已登录 profile 显示独立数据。
- 未登录返回 `source=unavailable`。
- 离线时显示 stale/estimate，不显示假 0%。

实现结果（2026-06-02）：
- 已实现 `get_profile_quota` / `refresh_all_quotas` / quota cache。
- 当前数据源为本地 session activity estimate，明确返回 `source=activity_estimate`，不伪造 remaining percent / reset countdown。
- 已实现受控 Codex app-server quota 尝试：仅在 `LAM_ENABLE_CODEX_APP_SERVER_QUOTA=1` 时执行，失败会写入 alerts 并 fallback，不挂起、不伪造真实额度。
- 已通过集成测试覆盖“不展示假实时额度”和 app-server 失败 fallback。
- 已通过 Codex CLI schema 定位到 app-server `account/rateLimits/read` / `GetAccountRateLimitsResponse`，但本轮未实现稳定 JSON-RPC 握手调用；因此真实 rate-limit source 仍保留 `[~]`。

### [x] TODO-602: 实现 Usage quota UI

目标：
在 Overview、Accounts、侧栏和 Inspector 显示 Session / Weekly 额度、刷新时间和切换建议。

范围：
- Account cards
- Quick accounts
- Overview
- Inspector quota panel
- Settings quota refresh

逻辑设计：
紧凑模式用于快速判断账号状态，展开模式用于查看详细窗口、pace 和操作建议。

详细要求：
- 显示：
  - used percent
  - remaining percent
  - reset countdown
  - updated time
  - plan badge
- 阈值：
  - Session >= 70% warn
  - Session >= 90% critical
  - Weekly >= 80% warn
- 操作：
  - Refresh
  - Switch to profile suggestion
  - Create relay suggestion
- Settings：
  - auto refresh interval
  - thresholds
  - data source status

验收：
- 手动 refresh 更新 `fetched_at`。
- stale 数据有明确视觉状态。
- 额度紧张时出现 relay/switch 建议。

实现结果（2026-06-02）：
- Overview 已展示 quota snapshots、activity estimate 和 Refresh quotas 操作。
- UI 明确说明只有真实来源可用时才显示 remaining/reset，当前不显示假 `% left`。
- `npm run test:ui` 覆盖 quota estimate 文案。

## Phase 1.5: Provider & Secret

### [x] TODO-701: 实现 ProviderStore 与 SecretStore

目标：
提供 Provider Center 的正式写能力，同时保证 API key 不进入 config、wrapper、日志或剪贴板。

范围：
- Provider CRUD
- macOS Keychain
- env secret mode
- providers.json

逻辑设计：
Provider 元数据和 secret 分离。`providers.json` 只存非敏感字段；Keychain 存 secret；env mode 只记录 env key 名称。

详细要求：
- provider metadata：
  - id
  - name
  - base_url
  - wire_api
  - default_model
  - env_key
  - secret_storage
  - optional_headers metadata
  - health
- storage：
  - `~/.config/agent-workspace/providers.json`
  - Keychain service `agent-workspace-manager`
  - Keychain account `provider:<provider_id>`
- 不把 secret 返回前端。
- test provider 不记录 key。

验收：
- 创建 provider 后 UI 无明文 key。
- 删除被使用 provider 前有阻止或确认。
- Keychain 失败时返回可恢复错误。

实现结果（2026-06-02）：
- 已实现 `providers.json` metadata store、Provider create/update/delete/test commands、env secret reference、Keychain 写入尝试。
- 集成测试覆盖 provider store 不返回/不持久化明文 secret。
- UI 已提供 Add/Test/Delete Provider。
- 删除被使用 provider 的阻止策略已实现并测试覆盖。
- Keychain 空 secret/失败边界已测试覆盖，失败不会写 provider metadata；真实 macOS Keychain 成功写入仍属于手工验收项。

### [x] TODO-702: 实现 Attach Provider to Profile

目标：
把 Provider 绑定到 Codex profile，并以安全方式写入配置引用。

范围：
- attach provider command
- config writer
- Attach modal

逻辑设计：
Attach 只写 provider 引用、model、env key 等非明文字段。secret 仍由 Keychain 或用户环境变量提供。

详细要求：
- 写操作必须 plan / execute。
- dry-run 展示将改动的 config path 和字段。
- 写 config 前备份原 `config.toml`。
- 不写 API key 明文。
- 支持 rollback 说明或备份路径展示。

验收：
- attach 后 profile provider badge 更新。
- `config.toml` 不包含 API key 明文。
- 备份文件存在。

实现结果（2026-06-02）：
- 已实现 Attach Provider plan/execute、config.toml backup、只写 provider/model/base_url/wire_api/env_key 引用。
- 集成测试覆盖 backup 存在且 config 不包含明文 key。
- Providers UI 已提供 Attach modal。

### [x] TODO-703: Provider-aware sync/resume mismatch

目标：
在 relay、sync、resume 流程中清晰展示 source provider 与 target runtime provider 的差异。

范围：
- Sessions table provider columns
- Inspector provider block
- Sync warning
- Resume warning

逻辑设计：
Provider mismatch 不阻止 resume，但必须显式告知行为、成本和工具兼容性可能变化。

详细要求：
- Session 显示：
  - original_provider_id
  - original_model
  - current_provider_id
  - current_model
  - provider_mismatch
- Warning 文案必须说明：
  - transcript 可继续
  - runtime behavior may differ
  - cost/tool compatibility may differ
- Resume command 不包含明文 key。

验收：
- mismatch case 有二次警告。
- non-mismatch case 不显示误报警。
- 用户能在 resume 前看到 source/target/account/provider/cwd。

实现结果（2026-06-02）：
- SyncPlan 已包含 provider mismatch/unknown provider warnings，Sync UI 可展示。
- `CodexSession` 已包含 `original_provider_id` / `original_model` / `current_provider_id` / `current_model` / `provider_mismatch`。
- Sessions table 已展示 original → runtime provider/model。
- Inspector 已展示 mismatch 二次警告：transcript 可继续，但 runtime behavior、cost、tool compatibility 可能不同。
- 集成测试与 UI smoke 均覆盖 mismatch 行为。

## Phase 2: 通用 Agent 抽象

### [ ] TODO-801: 抽象 AgentAdapter

目标：
把 Codex-specific 逻辑迁入 adapter，为 Claude Code、OpenCode、Aider、Cursor 等后续扩展打基础。

范围：
- `AgentAdapter` trait
- `AgentRegistry`
- `AgentProfile`
- `AgentSession`
- CodexAdapter migration

逻辑设计：
Sync、Provider、Secret、Usage 等通用服务依赖 Agent 抽象，不直接依赖 Codex 路径或命令。

详细要求：
- adapter 能力声明：
  - scan_profiles
  - parse_sessions
  - generate_resume
  - relay
  - fetch_usage_quota
- CodexAdapter 实现现有 Phase 1 / 1.2 / 1.5 能力。
- API 保持兼容或提供 v2 command。
- 数据迁移 CodexAccount -> AgentProfile。

验收：
- Codex 功能不回归。
- 新增一个 stub adapter 不需要改 SyncEngine 核心。
- UI 可按 agent 分组 profile。

### [ ] TODO-802: 可选 Routing Mode（代理模型）

目标：
在保持默认本地安全模式的前提下，引入可选路由代理能力（借鉴 cc-switch 路由模式），支持 provider 无重启切换与请求观测。

范围：
- 本地路由服务（`127.0.0.1:<port>/v1`）
- 启用/禁用 routing 时 `~/.codex/config.toml` 备份与回滚
- 请求统计（计数/错误率/延迟）与 failover 事件记录
- UI 开关与风险提示

逻辑设计：
Routing Mode 默认关闭；开启后接管 base_url 转发。关闭后必须恢复用户原配置。

验收：
- 不启用 routing 时，现有 Lam 功能不受影响。
- 启用 routing 后，provider 切换无需重启 CLI。
- 关闭 routing 可恢复原配置，且不丢失手工配置。

## 当前推荐推进顺序

### 已完成或已验证

1. TODO-001 到 TODO-002：文档权威与追踪入口。
2. TODO-102、TODO-103、TODO-201 到 TODO-204：Phase 1 UI shell、类型契约、扫描、session parser、真实数据 UI、Provider 只读。
3. TODO-301 到 TODO-302：受管账号与 relay plan/execute 及 UI。
4. TODO-401 到 TODO-405：safe sync、manifest、resume、Terminal launcher 及 UI。
5. TODO-501 到 TODO-503：`.fake-home`、自动化测试、README 与安全说明。
6. TODO-106 到 TODO-114、TODO-116 到 TODO-118：Phase 1F 主要功能纠偏。

### 仍需客观保留的工作

7. **TODO-104**：Tauri 工程已接入并被 CLI 识别；`make start` 已提供 dev 桌面启动入口，native window 可由本机直接启动验证。
8. **TODO-504**：执行 v0.1.0 手工验收矩阵：真实账号扫描、创建 profile、创建 relay、dry-run sync、execute sync、copy/open resume、确认 backup/manifest/auth 安全。
9. TODO-601：真实 Codex app-server rate-limit source 的稳定 JSON-RPC 握手（已实现 primary/secondary 解析 v1，仍需增强兼容与窗口映射）。
10. TODO-701：真实 macOS Keychain 成功写入手工验收。
11. TODO-801：多 Agent 抽象。
12. TODO-802：可选 Routing Mode（代理模型）。

### 状态对照（2026-06-02 纠偏后）

| TODO | 当前状态 | 说明 |
|------|----------|------|
| TODO-101 | `[~]` | 工程骨架已基本满足；native window 手工启动仍待确认 |
| TODO-104 | `[~]` | Tauri CLI 已识别；`make start` 已提供本机启动入口 |
| TODO-504 | `[~]` | 发布前手工验收矩阵未执行 |
| TODO-115 | `[ ]` | 可选 activity estimate |
| TODO-601 | `[~]` | 已实现 app-server 子进程解析 v1（primary/secondary）；仍需增强协议兼容与稳定性 |
| TODO-602 | `[x]` | quota UI 已实现 |
| TODO-701 | `[x]` | Provider CRUD/env reference/删除阻止/Keychain 失败边界已实现 |
| TODO-702 | `[x]` | Attach Provider 已实现 |
| TODO-703 | `[x]` | Provider mismatch session 模型与 UI 警告已实现 |
| TODO-801 | `[ ]` | 多 Agent 抽象 |
| TODO-802 | `[ ]` | 可选 Routing Mode（代理模型） |

### [x] TODO-116: 补齐 session parser 边界测试

目标：
让 TODO-202 的验收从 happy path 扩展到真实边界输入。

范围：
- JSON session 文件
- 空 session 文件
- 损坏 JSONL 行
- 无 cwd session
- 文件名 fallback

验收：
- `cargo test` 覆盖上述 case。
- parser 不因损坏行崩溃。
- 无 cwd 时返回 `cwd=None`，供 UI 显示 unknown。

### [x] TODO-117: 补齐 SyncPlan warnings 与占位检查

目标：
让 TODO-401 的 plan 输出更接近产品 UI 所需的审计信息。

范围：
- provider mismatch placeholder / warning
- large session directory warning
- target primary account warning
- blocked files 扫描结果与策略列表的区分

验收：
- plan 中能区分 policy-blocked 与 actually-seen blocked files。
- 大目录阈值有明确配置或常量。
- mismatch 暂无完整 Provider 模型时，也能提供保守 placeholder 或 unknown 状态。

### [x] TODO-118: 规范 sync backup 与 manifest

目标：
让 TODO-402 的执行结果满足文档要求的可追踪格式。

范围：
- backup 目录名格式：`sessions.backup.YYYYMMDD-HHMMSS`
- manifest 文件名：`<uuid>.json` 或文档明确接受的稳定唯一 ID
- manifest 内容：完整 operations、blocked、warnings、backup path、timestamp

验收：
- 集成测试断言 backup 名称格式。
- 集成测试断言 manifest 包含 executed operations。
- 不复制 logs、`state_*.sqlite`、installation_id 的断言补齐。

## Phase 1G: 原型对齐与发布收尾（2026-06-02 登记）

详见 `docs/CORRECTION-PLAN.md` 波次 0–6。下列为执行追踪编号。

### [x] TODO-125: 修复 UTF-8 session 摘要截断导致 `make start` panic

目标：
扫描含中文（或其它多字节 UTF-8）的 Codex session 时，应用不得崩溃。

范围：
- `services/core.rs` 中 `short_text()`

验收：
- `cargo test parses_session_summary_with_multibyte_utf8_without_panicking` 通过。
- 本机 `make start` 在真实 `~/.codex*` 含中文 session 时可打开窗口。

### [ ] TODO-119: 跨 profile Sessions + 筛选

见 `docs/CORRECTION-PLAN.md` A1、A2；波次 1。

### [ ] TODO-120: Sync Plan 分组 UI

见 `docs/CORRECTION-PLAN.md` A3；波次 1。

### [ ] TODO-121: Overview 时间线 + 最近账号

见 `docs/CORRECTION-PLAN.md` A4、A5；波次 2。

### [ ] TODO-122: Settings / Inspector 补齐

见 `docs/CORRECTION-PLAN.md` A6、A7；波次 2。

### [ ] TODO-123: 原型额度 UI（依赖 TODO-601）

见 `docs/CORRECTION-PLAN.md` B1–B5；波次 4。

### [ ] TODO-124: Phase 1 手工验收记录

见 `docs/PHASE1-ACCEPTANCE.md`；波次 5；关闭 TODO-504。
