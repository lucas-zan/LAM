# 桌面运行时：Tauri 与 Electron

版本：0.1  
日期：2026-06-02

LocalAgentManager (Lam) Phase 1 桌面端采用 **Tauri v2 + React + Rust**，而非 Electron。选型依据见 `docs/FINAL-DESIGN.md` §6。

## 1. 为什么不用 Electron？

| 考量 | Tauri（当前） | Electron（未选） |
|------|----------------|------------------|
| **本地能力边界** | 文件系统、sync、Keychain、Terminal 等在 **Rust** 中实现；前端只 `invoke` | 通常在 **Node 主进程** 或 preload 中实现；攻击面常含完整 Node |
| **包体与内存** | 使用系统 **WebView**（macOS WKWebView）；安装包更小 | 捆绑 **Chromium + Node**；体积常 100MB+ |
| **安全模型** | 命令白名单、Rust 类型与路径校验；符合「危险操作不进前端」设计 | 可行但需自行约束 `nodeIntegration` / preload；团队更易误开权限 |
| **与规格一致** | `FINAL-DESIGN`、`IMPLEMENTATION-ISSUES`、目录 `apps/desktop/src-tauri` 均按 Tauri 规划 | 引入需重写 command 层与打包管线，无 Phase 1 收益 |
| **Codex 工具定位** | 本地 first、偏系统工具；Rust 测试锁安全策略 | 更适合强依赖 Node 生态或必须固定 Chromium 版本的场景 |

**何时 Electron 更合理**：团队只会 TypeScript、强依赖仅存在于 Node 的原生模块、或必须锁定某一 Chromium 版本做复杂 WebGL/扩展。Lam 的核心是 **本地 FS + shell 安全**，Tauri + Rust 更贴设计。

## 2. 当前架构（简图）

```text
┌─────────────────────────────────────┐
│  React UI (Vite, port 1420 dev)      │
├─────────────────────────────────────┤
│  @tauri-apps/api invoke              │
├─────────────────────────────────────┤
│  Tauri commands (Rust, 校验/序列化)   │
├─────────────────────────────────────┤
│  services/core.rs — 扫描/sync/resume  │
│  Provider / Quota / Keychain 等       │
├─────────────────────────────────────┤
│  ~/.codex* 、Keychain、Terminal.app   │
└─────────────────────────────────────┘
```

开发时：`make start` → `tauri dev` 启动原生窗口，内嵌 Vite 渲染进程；**不是**让用户单独用浏览器当正式产品。

## 3. 启动方式

```bash
# 正式开发入口（原生窗口）
make start

# 使用仓库假数据，避免动真实 ~/.codex*
LAM_HOME="$(pwd)/.fake-home" make start

# 仅前端（无 Tauri，状态栏会显示 Browser preview）
cd apps/desktop && npm run dev
```

## 4. 已知启动问题与修复

### 4.1 UTF-8 session 摘要 panic（已修复）

**现象**：`make start` 崩溃：

```text
thread 'main' panicked at src/services/core.rs:1156:34:
byte index N is not a char boundary; it is inside '存' ...
```

**原因**：扫描 `~/.codex*/sessions` 时，对 session 内中文（或其它多字节 UTF-8）摘要做 **按字节** 截断（`&s[..max]`），在字符中间切开导致 panic。

**修复**：`short_text()` 改为按 **Unicode 标量（chars）** 计数截断（`services/core.rs`）。

**验证**：

```bash
cd apps/desktop/src-tauri && cargo test parses_session_summary_with_multibyte_utf8_without_panicking
make start   # 本机应能打开窗口并加载含中文 session 的账号
```

### 4.2 其它

| 问题 | 处理 |
|------|------|
| `Couldn't recognize ... Tauri project` | 确认在 `apps/desktop` 存在 `src-tauri/tauri.conf.json` |
| 完整 Xcode 仅打包签名可能需要 | 开发 `tauri dev` 通常不需要完整 Xcode |
| Terminal 权限 | 系统设置 → 隐私 → 自动化；失败时 UI 应 fallback 为复制命令 |

## 5. 与 Electron 体验差异（用户可见）

| 体验 | Tauri | Electron |
|------|-------|----------|
| 窗口/菜单 | 系统原生 WebView 壳 | Chromium 窗口，可完全一致跨平台 |
| 外观与 Web 一致性 | 依赖系统 WebView 版本 | Chromium 版本固定，像素级一致 |
| 安装包 | 较小 `.app` / `.dmg` | 较大 |
| 本地命令执行 | 经 Rust 后端 | 经 Node（需自行加固） |

对用户：功能一致取决于 **产品逻辑**，不取决于 Electron/Tauri；Lam 的安全叙事依赖 **Rust 后端**，这是选 Tauri 的主要原因。

## 6. 相关文档

- `docs/FINAL-DESIGN.md` §6 技术架构  
- `docs/CORRECTION-PLAN.md` 纠偏顺序  
- `README.md` — `make start` / `make check`  
