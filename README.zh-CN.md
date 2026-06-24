# LAM (LocalAgentManager)

**LAM** 是一款 **macOS 菜单栏应用**，面向使用 **多个 [Codex CLI](https://github.com/openai/codex) 账号**（`~/.codex`、`~/.codex-a` 等）的用户：一眼看额度、按账号浏览 session，并**安全地把对话迁到另一个账号**继续 — 不上传代码与聊天记录到云端。

**English:** [`README.md`](README.md)

---

## 为什么需要 LAM？

多账号用 Codex 时，经常会遇到：

- 每个 profile 是**独立目录**，各有 `auth.json`、`config.toml`、`sessions/`。
- `codex resume` 只在**当前** `CODEX_HOME` 下找会话。
- A 账号额度用尽换 B 时，**对话不会自动跟过去**；整目录乱拷又可能弄坏登录或污染 B 的环境。

LAM 全部在**本机**完成：扫描 profile、读取真实额度（5h / weekly），只复制**允许复制的 session 文件**，再帮你在终端里拼好 `codex resume` 命令。

---

## LAM 能做什么

| 能力 | 说明 |
|------|------|
| **菜单栏托盘** | 左键看额度；有最近 session 时可快捷 Relay / Resume |
| **Overview** | 所有 Codex 账号、额度条、续费备注、账号操作按钮 |
| **Sessions** | 浏览、搜索、复制 resume 命令、开终端、接力单条 session |
| **Sync** | 在两个 profile 之间**批量**同步 `sessions/`（须先 dry-run） |
| **Providers** | 管理 API 端点元数据并挂到账号（密钥在 Keychain / 环境变量） |
| **Settings** | 主题、Home 根目录、两边都改过同一会话时的处理策略 |

LAM **不会**在应用内直接跑 Codex，只准备命令并打开 **Terminal.app**。

---

## 安装

### 使用 Release DMG（推荐）

1. 在 [GitHub Releases](https://github.com/lucas-zan/LAM/releases) 下载 **`LAM_<版本>_*.dmg`**。
2. 打开 DMG，将 **LAM** 拖入 **应用程序**。
3. 启动 LAM，菜单栏会出现图标，主窗口为普通应用窗口。

**未签名包：** 首次打开可能被拦截，请 **右键 → 打开**，或在 **系统设置 → 隐私与安全性** 中允许。

**还需要：**

- macOS（DMG 架构需与机器一致，如 Apple Silicon / Intel）。
- 已安装 **Codex CLI**，且至少有一个 profile 已登录，才能看到真实额度与会话。

### 从源码运行（开发者）

需要：**Node.js**、**npm**、**Rust**、**Cargo**、**macOS**。

```bash
git clone https://github.com/lucas-zan/LAM.git
cd LAM
make install
make start
```

不碰真实 `~/.codex*` 的测试数据：

```bash
LAM_HOME="$(pwd)/examples/fake-home" LAM_ALLOW_FAKE_HOME=1 make start
```

本地打 DMG：`make dmg`  
发版：推送标签如 `v0.1.0`（见 [`.github/workflows/release.yml`](.github/workflows/release.yml)）。

---

## 日常使用

### 菜单栏托盘

- **左键** — 各账号额度浮层（5h / weekly 进度条）。
- 账号行 **Relay / Resume** — 等同该账号上的 **Relay Latest**（见下文）。
- **右键** — 刷新额度或打开主窗口。
- 点击浮层外或 **Close** 收起。

### 主窗口

底部导航：**Overview · Sessions · Providers · Sync · Settings**

顶栏：**Refresh**、**New Account**、**New Provider**

在 **Overview → Codex** 中，每个账号卡上有额度、备注和四个核心操作按钮，含义见下一节。

---

## 账号上的四个按钮：Relay Latest、Handoff、Sync Sessions、Rename

这四个按钮解决**不同**问题，请按需求选择。

### Relay Latest —「用**最近那条**会话，在这个账号上继续」

**做什么**

1. 在所有 Codex profile 里找到**修改时间最新的一条 session**（「最新活跃会话」）。
2. 若该 session 文件还不在你点击的**目标账号**下，LAM 会把**这一条** session 文件复制到目标的 `sessions/`（**不复制** `auth.json`）。
3. 打开**终端**，执行 `CODEX_HOME=<目标> codex resume <session-id>`，即可在目标账号下继续聊。

**什么时候用**

- 你刚在账号 A 上开发，想在账号 B 上**接着同一条对话**继续（例如 B 还有额度）。
- 不用自己挑 session — 「我最近在做啥就续啥」。

**什么时候不用**

- 要续的是**更早的某一条** session → 用 **Handoff**。
- 要一次性搬**很多** session → 用 **Sync Sessions**。
- 找不到任何 session 时按钮会禁用。

---

### Handoff —「把**指定**的一条会话，接到另一个账号」

**做什么**

1. 弹出对话框：**源账号**、**会话**（从源账号列表选）、**目标账号**。
2. 按需把**该条** session 的 transcript 文件复制到目标（规则与 Relay Latest 相同）。
3. 在目标账号下打开终端执行 `codex resume`。

**怎么打开**

- 账号卡 **Handoff** — 目标已设为该卡账号，你选源账号和 session。
- **Sessions** 页某行 **Relay To…** — session 已选好，你再选目标（可改源）。

**什么时候用**

- 明确知道要迁**哪一条** session，不一定是「最新一条」。
- 复制前想先看清楚源、目标、会话再确认。

**若两边都已改过同一条 session**，按 **Settings → Diverged session strategy** 处理（备份、分叉或中止），不会无声覆盖。

---

### Sync Sessions —「把 A 的**全部** session 文件批量拷到 B」

**做什么**

1. 打开 **Sync** 对话框，选源 profile 与目标 profile。
2. 必须先 **Dry Run** — 列出 `sessions/` 下将要复制、跳过或阻止的每个文件。
3. 确认后**整棵 `sessions/` 目录**按文件复制到目标。
4. 覆盖前会备份目标原有的 `sessions/`。
5. 生成变更 manifest。

**绝不会复制**

- `auth.json`、`config.toml`、sqlite、`cache/`、`tmp/`、`logs/` 等。

**什么时候用**

- 给 **relay 工作区**或第二个账号**一次性**搬很多（或全部）会话，再在其中 resume。
- **一次性迁移** session 资产。

**什么时候不用**

- 只要**一条**对话并立刻开终端 → 用 **Relay Latest** 或 **Handoff**。
- Sync **不会**替你执行 `codex resume`；同步完成后到 Sessions 或 Relay Latest 再续聊。

**说明：** 同步到**主 profile**（非 relay 类目录）可以做，但 LAM 会警告 — 更推荐用独立 relay 目录，避免混用运行时状态。

---

### Rename —「给受管账号改文件夹名和 wrapper」

**做什么**

1. 将 `~/.codex-{旧名}` **整体重命名**为 `~/.codex-{新名}`。
2. 生成新 wrapper `~/bin/codex-{新名}`，删除旧 wrapper。
3. **`auth.json` 随目录一起移动**，不会单独删除或外拷。

**什么时候用**

- 想把 `~/.codex-b` 改成更好记的名字如 `~/.codex-work`。
- 让 LAM 与 shell 里的 `codex-xxx` 命令与目录名一致。

**限制**

- **`main`（`~/.codex`）不能重命名。**
- 新名不能与已有 profile 冲突。
- 须先 **Dry Run**；重命名前关闭正在使用该 profile 的 Codex 进程。

Rename **不会**把 session 迁到别的账号，只是**同一个**账号改目录名。

---

### 对照表

| 按钮 | 复制范围 | 复制什么 | 是否开终端 | 是否自选 session |
|------|----------|----------|------------|------------------|
| **Relay Latest** | 单文件 | 全库最新一条 session | 是（`codex resume`） | 否（自动） |
| **Handoff** | 单文件 | 你指定的一条 session | 是（`codex resume`） | 是 |
| **Sync Sessions** | 多文件 | 整个 `sessions/` 树 | 否 | 不适用（全部 session） |
| **Rename** | — | 重命名一个 profile 目录 | 否 | 不适用 |

---

## 其他常用操作

**New Account** — 创建 `~/.codex-{name}` 与 `~/bin/codex-{name}`（dry-run → 创建）；可选从已有 profile 复制 `config.toml`。

**Login** — 在该 profile 的 `CODEX_HOME` 下打开 `codex login`。

**Edit note** — 在 LAM 本地记录续费日期与备注（不写进 Codex auth）。

**Sessions** — 按账号筛选；**Copy** / **Terminal** / **Details** / **Relay To…**

**Providers** — 登记代理或自定义 API；Attach 到账号；密钥在环境变量或 Keychain。

---

## 实现方式（简要）

```text
React 界面  →  Tauri (macOS)  →  Rust 核心  →  ~/.codex*  +  终端
```

- **界面：** `apps/desktop/src` — 托盘浮层 + 主窗口。
- **核心：** `apps/desktop/src-tauri` — 扫描、同步、app-server 额度、命令生成。
- **LAM 元数据：** `~/.config/agent-workspace/` — 账号缓存、备注、Provider 登记。
- **Codex 数据：** 仍在 `~/.codex*`；LAM 不上传云端。

详见 [`docs/DESKTOP-RUNTIME.md`](docs/DESKTOP-RUNTIME.md)

---

## 安全原则

- 同步与接力**永不复制 `auth.json`**。
- 破坏性操作在适用处采用 **dry-run → 确认 → 执行**。
- Provider 密钥**不会**在界面明文显示，也不会写入 `config.toml` 明文。

---

## 开发与测试

| 命令 | 作用 |
|------|------|
| `make start` | 开发模式运行 |
| `make check` | 构建、UI smoke、Rust 测试 |
| `make dmg` | 打可安装 DMG |
| `make clean` | 删除 `target/`、`dist/`（开发时 `target/` 可能占数 GB，可安全删除后重编） |

---

## 许可证

[MIT License](LICENSE) — 可自由使用、修改与分发。再分发时请保留版权声明。
