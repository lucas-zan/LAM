# LAM — macOS Codex 账号、额度与 Session 管理器

在 macOS 上管理多个 Codex CLI 账号、额度、使用量与 Session，并用另一个账号继续已有 Session。

**Early Preview** · **仅支持 macOS** · **仅支持 Codex** · **Local-first，但并非完全离线**

LAM 是一款面向多 Codex CLI Profile 用户的 macOS 菜单栏工具。它扫描本机的 `CODEX_HOME`，查看额度和本地 Token 使用量，浏览和恢复 Session，并将已有 Session 安全接力到另一个账号继续。

![LAM 账号、额度、使用量与 Session 总览](docs/assets/lam-overview.png)

LAM 不会内嵌运行 Codex。Resume 或 Handoff 会使用所选 Profile 的 `CODEX_HOME`，在外部终端中启动 Codex；默认终端是 Terminal.app。

**English:** [`README.md`](README.md)

## 为什么需要 LAM

Codex Session 属于创建它的 `CODEX_HOME`。当一个账号达到使用上限时，切换到另一个账号并不会自动让之前的 Session 出现在新账号下。LAM 可以准备该 Session，并通过另一个 Profile 恢复它。

如果没有专用工具，用户通常需要手动找到正确的 Session JSONL，在不泄露账号状态的前提下复制它，选择正确的目标 Profile，再使用正确的环境执行 `codex resume`。直接复制整个 `~/.codex*` 目录并不安全，因为认证、配置、数据库、缓存和其他账号级状态也可能被一并复制。

从实际用途看，LAM 同时是 Codex CLI Account Manager、Codex Session Manager、Codex Quota 面板、本地 Codex Usage Tracker、Codex Token Usage 分析工具、估算 Codex Cost Tracker，以及安全的 Codex Session Handoff 工具。

LAM 将以下能力集中在一个本地界面中：

- 多个 Codex 账号及其 `CODEX_HOME` Profile；
- Codex 额度和 Reset Credits；
- 本地 Codex Token、调用、Thread 和估算成本；
- Session 浏览、Resume、冲突处理与跨账号接力；
- 自定义 OpenAI-compatible Provider Profile；以及
- macOS 菜单栏额度快捷入口。

## LAM 适合你吗？

LAM 适合：

- macOS 用户；
- Codex CLI 重度用户；
- 本机维护 `~/.codex` 和多个 `~/.codex-*` Profile 的开发者；
- 经常切换多个 Codex 账号的人；
- 一个账号额度不足后，需要跨账号继续已有 Session 的人；
- 希望查看本地 Codex Quota、Token、调用量、Thread 和成本估算的人；以及
- 使用自定义 OpenAI-compatible Responses 或 Chat Completions Provider 的高级用户。

LAM 可能不适合：

- Windows 或 Linux 用户；
- 只使用一个 Codex 账号且不需要本地使用量分析的人；
- 当前需要 Claude Code 或 OpenCode 支持的人；
- 需要云同步或多设备访问的人；
- 希望直接在 LAM 窗口内运行 Codex 的人；或
- 不熟悉 PAT、Session JSON 和认证文件风险的用户。

## 核心流程

1. **发现 Profile。** LAM 扫描 `~/.codex` 和 `~/.codex-*`，展示账号、Session 数量、额度和最近的本地 Session。
2. **选择 Session。** 从账号卡或 Session 行打开 **Handoff Session**，在 UI 中选择源账号、源 Session 和目标账号。
3. **安全准备目标。** LAM 只将选中的 Session JSONL 复制到目标 `CODEX_HOME` 下的对应路径。跨 Provider 写入前会检查兼容性；目标中已存在的 Session 会经过内容比较和显式冲突策略处理。
4. **继续使用 Codex。** LAM 生成目标命令，并在所选外部终端中启动 `CODEX_HOME=<target> codex resume <session-id>`。

## 功能

### Account 与 Profile 管理

- 扫描主 Profile `~/.codex` 以及同级的 `~/.codex-*`。
- 展示 Profile 路径、认证状态、Session 数量、活动认证状态、Provider/Model、续费日期和备注。
- 创建相互隔离的受管 Profile 和 shell wrapper。
- 在冲突检查后重命名或删除非 `main` 的受管 Profile。
- 使用所选 `CODEX_HOME` 打开 `codex login`。
- 保持账号隔离：一个 Profile 对应一个 `CODEX_HOME`。

### Quota

- 展示 Codex 或 ChatGPT 返回的额度窗口，通常包括 5 小时和每周窗口，以及上游提供的重置时间。
- 实时刷新失败时展示缓存的真实额度；完全没有真实数据时显示 `N/A`。
- 上游返回相关信息时，展示 Reset Credit 数量和过期详情。
- 分账号刷新额度，并在主窗口和菜单栏浮层中展示。
- 不为自定义 External API Account 伪造额度百分比。

Quota 与本地 Usage Analytics 是不同的数据：Quota 是上游账号状态，Usage 是从本机 Codex 事件数据中统计得到的。

### Sessions、Resume 与 Handoff

- 按账号浏览 Session，并读取 ID、工作目录、名称、摘要、Model、时间和 Provider mismatch 信息。
- 复制或打开经过 shell 转义的 Resume 命令。
- 默认在 Terminal.app 中启动 `codex resume`；Settings 中也提供 Ghostty 和 cmux 目标。
- 在一个 Handoff 对话框中选择源账号、源 Session 和目标账号。
- 复制目标缺失的 Session；当目标是源内容的旧前缀时扩展目标；否则识别内容分叉。
- 支持保留备份、优先源版本、保留目标并创建源分叉、将时间线合并到分叉，以及为目标账号生成总结接力材料等策略。
- 不兼容的跨 Provider 历史会在目标写入前被阻止；仅存在表示层损失时需要用户确认。

### Safe Session Sync

Safe Session Sync 是 Session Handoff 内部的安全复制步骤，范围被刻意限制：

- 只复制 `sessions/` 下选中的一个 Session JSONL。
- 在目标 `CODEX_HOME` 中保留相对 Session 路径。
- 目标 Session 分叉时，会在执行配置的冲突策略前保留备份。
- 当前不暴露批量同步整个 `sessions/` 目录的命令。

由于 LAM 只复制选中的 Session 文件，而不是复制整个 Profile，所以 Handoff 不会复制：

- `auth.json` 或 `auth-f.json`；
- `config.toml`；
- `history.jsonl`；
- `logs_2.sqlite`、`state_*.sqlite`、LAM Usage 数据库等 SQLite 文件；
- `cache/`；
- `tmp/`；
- `log/` 或 `logs/`；以及
- `installation_id`。

Session Handoff 不是 Dry Run 工作流。Provider 兼容性会在目标写入前分析；本地 Session 分叉则通过备份和显式策略处理。

### 本地 Usage Analytics

**Usage** 页面将本地 Codex JSONL 事件索引到 LAM 自己的 `.codex/lam/usage/` SQLite 数据库中。它支持账号/Workspace Scope、活动或归档历史、时间范围、搜索、Model、Reasoning Effort、价格可信度和分页过滤。

当前 UI 展示：

- Total、Input、Cached Input、Uncached Input、Output 和 Reasoning Output Token；
- 调用量、Thread 数量、活动热力图、连续使用天数和最长 Turn；
- 每次调用的 Model、Effort、时长、Context、Cache Ratio 和估算成本；
- 每个 Thread 的调用、Session、Token、Cache Ratio、建议和估算成本；
- 价格覆盖率、未知 Model、解析器诊断和跳过的事件；以及
- 从原始本地日志按需读取的 Request、Assistant 和 Tool Output 详情。

成本来自内置 Rate Card 的本地估算，不是 OpenAI Invoice 或真实账单余额。当前 “Codex Credits” 卡片复用了估算成本，并不表示真实 Credit 余额。

![本地 Codex Token、调用、Thread 与估算成本](docs/assets/lam-usage.png)

### Provider Profile

- 创建、编辑、测试和删除 Provider Profile。
- 自动发现或手动配置 Model Allowlist 和默认 Model。
- 支持直接调用 OpenAI Responses Endpoint，以及为已验证 Chat Completions Adapter 提供本地认证 Gateway。
- 通过环境变量、macOS Keychain、已批准的 Auth Command 或 Codex Profile Login 引用凭据。
- 执行 Provider Attach、Rebind 和 Detach 前预览操作。
- 将 Provider/Model Attach 到 Profile，并把受管配置投影写入该 Profile 的 `config.toml`。
- 在 Session 上展示 Provider/Model mismatch。
- 提供以账号为入口的 **External API Account** 流程，并可在同一 `CODEX_HOME` 内切换 Model。

自定义 Provider 面向高级用户。兼容性取决于协议、Model、Tool、Streaming 行为和具体上游实现。

![创建 External API Account 并配置 Model](docs/assets/lam-external-api.png)

### macOS 菜单栏

- 提供紧凑的账号额度浮层。
- 支持刷新、打开主窗口、Resume 和 Handoff 快捷操作。
- 可隐藏 Dock 图标并作为菜单栏应用运行。
- LAM 主窗口与实际运行 Codex 的终端进程相互独立。

![带账号 Relay 操作的 LAM 菜单栏额度浮层](docs/assets/lam-menu-bar.png)

当前应用路由为 **Overview**、**Usage**、**Sessions**、**Providers** 和 **Settings**。

## 认证模式

LAM 提供 OAuth/Profile 和 PAT 两类账号工作流。两种模式对认证文件的写入行为不同。

Settings 可以控制界面显示 Profile、PAT 或两种模式，也可以选择 Handoff、Resume 和 Login 使用的终端。

![Profile/PAT 显示模式与 Handoff 终端设置](docs/assets/lam-settings.png)

### OAuth / Profile 模式

- 使用相互隔离的 Codex Profile 及其已有认证状态。
- **Login** 打开 `CODEX_HOME=<profile> codex login`；认证流程由 Codex 负责，并由 Codex 在对应 Profile 中写入认证状态。
- Session Handoff 只复制选中的 Session，不会从源账号复制认证信息。
- 普通 Profile 也可以通过粘贴 ChatGPT Session JSON 创建；导入过程会把提供的凭据转换为新 Profile 的 `auth.json`。

![将可信的 ChatGPT Session JSON 导入独立 Codex Profile](docs/assets/lam-import-session.png)

### PAT 模式

PAT 管理是高级功能，并且**会修改认证文件**：

- 添加 PAT Account 会创建新的 `~/.codex-<name>` Profile、`auth.json`、最小 `config.toml` 和元数据。
- 单独提供 Personal Access Token 时，LAM 将 PAT Runtime 形式写入 `auth.json`，并将上传的 Session Credential 保存在 `auth-f.json`。
- Credential Upload Command 可以替换 Profile 的 `auth.json`。
- 更新 Session Authentication 会替换该 PAT Profile 的 `auth-f.json`。
- 切换 PAT Account 时，LAM 会原子地把所选 Profile 的 `auth.json` 写入活动的 `~/.codex/auth.json`，同步复制或删除 `auth-f.json`，验证结果，然后由 UI 尝试重启 ChatGPT App。

除非你理解哪个 Profile 是活动认证槽，并已在必要时准备安全备份，否则不要使用 PAT 模式。

## 隐私与网络行为

LAM 是 local-first 工具，但并非完全离线。

- Session、源代码和 Prompt 不会上传到 LAM 自己运营的服务器。
- 项目没有 LAM 云端账号、云端 Session Store 或云同步服务。
- 账号扫描、Session 浏览、Handoff 和本地 Usage 索引都在 Mac 上完成。
- Usage SQLite 数据库不会持久化原始 Prompt/Response 内容，但 UI 可以按需从原始本地 JSONL 读取调用详情。
- Quota 刷新可能启动 `codex app-server`，并使用所选 Profile 的认证访问 Codex 或 ChatGPT 服务。
- Usage 和 Reset Credit 使用的部分 ChatGPT Web Backend 路径属于上游内部接口，可能随时变化。
- Provider Test 和通过 Provider 运行的 Codex Session 会访问用户配置的上游 Provider。
- Antigravity Quota 会检查本机进程，并访问手动配置的本地 Antigravity Language Server Endpoint。
- 正常安装和开发可能访问 npm、Cargo Registry、GitHub 和用户配置的 API Provider。

## 安全

LAM 会处理敏感的本地状态。使用高级功能前，请理解以下边界：

- **Handoff 边界：** 只复制选中的 Session JSONL。认证、配置、History、数据库、缓存、临时文件、日志和 Installation ID 都不在复制路径内。
- **冲突边界：** 不兼容的 Provider History 会在目标写入前失败。分叉的本地 Session 不会被静默覆盖；配置的策略会保留备份或分叉。
- **Provider Credential 边界：** Provider Secret 是 write-only 输入，并通过 Keychain、环境变量等引用保存或解析。Provider DTO 和 Plan 会保持脱敏。
- **PAT 边界：** PAT 导入、更新和切换会创建或替换认证文件。这与 Session Handoff 不复制认证的保证是两个不同边界。
- **导出边界：** ChatGPT Session JSON Import 和 CPA Export 可能包含 Access Token、Refresh Token、ID Token、Session Token、Account ID 或 Authorization Header。

**请像对待密码一样对待导出的 Credential 文件。**

不要提交导出的 Credential，不要粘贴到 Issue，也不要通过不可信渠道发送。仓库 Fixture 使用合成数据，切勿用真实认证文件替换。

现有安全说明见 [Security and data safety](docs/03-security-and-data-safety.md)。部分旧设计文档包含已被替代的计划，因此当前源码和测试是功能行为的最终依据。

## 高级与实验功能

以下功能已经存在，但不属于风险最低的 Profile → Session → Handoff 核心路径：

- **PAT Account Import 与切换** — 创建和替换认证文件，并可能重启 ChatGPT App。
- **ChatGPT Session JSON Import** — 将粘贴的 Access/Refresh/ID/Session Token 转换为新的 Codex Profile。
- **CPA Credential Export** — 为兼容工具导出认证信息。
- **Reset Credits** — 存在可用 Reset Credit 时执行消耗操作；需要用户确认，并会改变上游账号状态。
- **Antigravity Quota** — 通过手动配置的端口查询本机运行的 Antigravity Language Server。
- **ChatGPT Usage Path 与 Fallback** — 存在可用 Credential 时访问 ChatGPT 内部 Web Backend，并使用 Codex app-server 和缓存数据作为回退路径。
- **External Provider Gateway** — 将支持的 Chat Completions Provider 适配到 Responses Contract；不支持或包含有状态历史时会 fail closed。

上游内部接口和第三方 Provider 行为可能独立于 LAM 发生变化。

## 当前限制

- **Early Preview：** 仍可能存在体验问题、上游兼容性变化和未完成的人工验证。
- **仅支持 macOS：** 不支持 Windows 和 Linux。
- **仅支持 Codex：** Claude Code 和 OpenCode Adapter 尚未实现。
- **下载包仅支持 Apple Silicon：** 当前 v0.3.0 Release 只提供 `aarch64` DMG，没有 Intel 或 Universal DMG。
- **签名状态未确认：** 仓库和 v0.3.0 Release Metadata 无法证明已完成 Developer ID 签名或 Apple Notarization。
- **必须使用外部终端：** LAM 不内嵌 Codex。默认使用 Terminal.app，也可选择 Ghostty 或 cmux。
- **Sessions 页面按账号过滤：** 页面一次显示一个 Profile；Handoff 对话框提供跨 Profile 的源账号、Session 和目标账号流程。
- **没有批量 Safe Sync：** 批量同步整个 `sessions/` 目录的功能已移除；Handoff 只复制一个选中的 Session JSONL。
- **不合并 `history.jsonl`：** Handoff 不合并命令历史。
- **Provider 兼容性有限：** 不支持的 Tool 或有状态 History 可能阻止跨 Provider Handoff。
- **Quota 是 best effort：** 依赖 Codex/ChatGPT 认证和上游行为；External API Account 不会显示伪造额度。
- **Usage 是本地近似统计：** 缺失或归档的日志会影响总数；成本是估算值，不是账单成本。
- **没有云同步：** 除非用户自行移动，否则 Profile 和 Session 只保留在当前 Mac。
- **人工验收未完成：** `docs/PHASE1-ACCEPTANCE.md` 中的项目目前全部未勾选。
- **仓库完整检查当前不是绿色：** Frontend Build、UI Smoke、Vitest 和 `cargo test` 通过，但 `make check` 会因 4 个现有 Clippy Warning 被视为 Error 而停止。

## 安装

### 下载 Preview Release

最新发布版本是适用于 Apple Silicon 的 [v0.3.0](https://github.com/lucas-zan/LAM/releases/tag/v0.3.0)：

- `LAM_0.3.0_aarch64.dmg`
- `LAM_0.3.0_aarch64.dmg.sha256`

请将其视为 Preview Build。仓库元数据无法确认签名与公证状态。

要使用 Session Resume 和实时 Codex 功能，系统中必须安装可用的 Codex CLI。

### 从源码安装

要求：

- macOS；
- Node.js 和 npm；
- Rust 和 Cargo；以及
- 用于真实账号、Quota、Session 和 Resume 的 Codex CLI。

```bash
git clone https://github.com/lucas-zan/LAM.git
cd LAM
make install
make start
```

`make start` 启动原生 Tauri 开发应用。Vite 只是嵌入式 Renderer 的开发服务器。

如果不想扫描真实的 `~/.codex*`，可以使用仓库中的合成 Fixture：

```bash
LAM_HOME="$(pwd)/.fake-home" make start
```

Fixture 包含用于扫描测试的合成认证结构，不是真实可用的 Credential。

## 开发

在仓库根目录执行：

| 命令 | 用途 |
| --- | --- |
| `make install` | `node_modules` 不存在时安装前端依赖 |
| `make start` | 打包开发 Sidecar 并运行 `tauri dev` |
| `LAM_HOME="$(pwd)/.fake-home" make start` | 使用仓库中的合成 Fixture Home 运行 |
| `make accounts` | 使用 `lam-core` CLI 扫描账号 |
| `make check` | Frontend Build、UI Smoke、Rust Format、Clippy 和 Rust Tests |
| `make build` | 构建 macOS `.app` Bundle |
| `make dmg` | 构建 `.app` 和带版本号的 DMG |
| `make status` | 显示 Node、npm、Rust 和 Tauri 环境信息 |

单独运行测试：

```bash
cd apps/desktop
npm test
npm run test:ui

cd src-tauri
cargo test
```

对提交 `e2e41bd` 在 2026-07-27 的审计结果：

- `npm run build`：通过；
- `npm run test:ui`：通过；
- `npm test`：22 个文件、228 个测试通过；
- `cargo test`：通过，部分环境/负载 Probe 被明确标记为 ignored；
- `make check`：在运行 `cargo test` 前，因为 4 个现有 Clippy Warning 被提升为 Error 而失败；以及
- 人工验收：没有完成记录。

单元和集成测试通过不代表 Release 已签名、公证，或已完成真实账号端到端验证。

## 仓库结构

```text
apps/desktop/                 React、TypeScript、Vite、Zustand 与 UI Tests
apps/desktop/src-tauri/       Tauri Commands、Rust Services、Binaries 与 Rust Tests
.fake-home/                   本地开发与测试使用的合成 Fixture Home
examples/fake-home/           较小的示例 Profile Fixture
docs/                         产品、安全、Runtime、Contract、设计与 TODO 文档
docs/assets/                  README 使用的产品截图
plans/                        历史实施计划和报告
Makefile                      仓库根目录开发命令
LICENSE                       MIT License
```

用户界面路由位于 `apps/desktop/src/routes/`；Tauri Command 注册位于 `apps/desktop/src-tauri/src/main.rs`；Command Adapter 位于 `apps/desktop/src-tauri/src/commands/`；核心行为位于 `apps/desktop/src-tauri/src/services/`。

现有相关文档：

- [Desktop runtime](docs/DESKTOP-RUNTIME.md)
- [Security and data safety](docs/03-security-and-data-safety.md)
- [Tauri command contracts](docs/05-tauri-command-contracts.md)
- [Phase 1 manual acceptance](docs/PHASE1-ACCEPTANCE.md)
- [Remote Provider Gateway contract coverage](docs/remote-provider-gateway-contract-coverage.md)

部分旧设计文档仍包含已被替代的 Phase Plan 或已移除的 Bulk Sync。后续文档整理建议将当前实现提取到：

- `docs/ARCHITECTURE.md`
- `docs/SECURITY.md`
- `docs/PAT-MODE.md`
- `docs/SESSION-HANDOFF.md`
- `docs/PROVIDERS.md`
- `docs/DEVELOPMENT.md`
- `docs/TROUBLESHOOTING.md`

这些聚焦文档目前尚未创建，因此没有作为已完成文档链接。

## Roadmap

- Claude Code Adapter。
- OpenCode Adapter。
- 如果维护者决定扩展当前 macOS 范围，则支持 Windows 和 Linux。
- 提供签名、公证的 macOS 安装包，以及 Intel 或 Universal Build。
- 完成并记录真实账号人工验收矩阵。
- 上游出现稳定公开接口后，替换 ChatGPT 内部 Web Backend 依赖。
- 完成上面列出的聚焦文档拆分。

Roadmap 内容不是当前功能。

## License

[MIT](LICENSE) © 2026 LocalAgentManager contributors。
