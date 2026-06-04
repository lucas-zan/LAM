# 开发路线图

> **历史草稿 / 已被替代：** 当前实施路线图以 `docs/FINAL-DESIGN.md` §8 和 `docs/IMPLEMENTATION-ISSUES.md` 为准。本文只保留早期 Codex-only 规划作为参考，不再作为排期或验收依据。

版本：0.1 draft

---

## Phase 0：验证原型

目标：确认现有 Node/Bun 原型的业务流程可用。

功能：

- 扫描 `~/.codex*`。
- 列出 accounts。
- 列出 sessions。
- 创建新 account 目录。
- 创建 wrapper。
- 同步 sessions。
- 生成 resume command。

产出：

- 作为 Tauri 迁移参考。
- 收集真实用户反馈。

---

## Phase 1：Tauri MVP

目标：完成可安装 macOS App 的核心功能。

功能：

- React UI 框架。
- Rust `list_accounts`。
- Rust `list_sessions`。
- 创建账号目录和 wrapper。
- 复制 resume command。
- 打开 Terminal resume。
- 同步 sessions dry-run。
- 执行 sessions sync。

验收：

- 不复制 auth。
- 不默认复制 history。
- 能从 A session relay 到 B-relay-A。

---

## Phase 2：安全与稳定性

目标：减少误操作风险。

功能：

- Dry-run 预览 UI。
- 同步前备份目标 sessions。
- 同步报告。
- 恢复备份。
- 更严格路径校验。
- 错误码标准化。
- 单元测试和集成测试。

---

## Phase 3：开源发布

目标：让普通用户可安装使用。

功能：

- GitHub README。
- 安全说明。
- `.dmg` 打包。
- GitHub Actions build。
- Release notes。
- issue template。

---

## Phase 4：高级能力

目标：提高 power user 使用体验。

功能：

- Session 搜索。
- 按项目 cwd 聚合 sessions。
- 多 terminal 支持：iTerm2 / Warp / Ghostty。
- codex binary 自动检测。
- wrapper 目录 PATH 检查。
- relay account 生命周期管理。
- Session 归档。

---

## Phase 5：跨平台

目标：探索 Linux / Windows 支持。

功能：

- Linux terminal launcher。
- Windows PowerShell / Windows Terminal launcher。
- 跨平台路径策略。
- 跨平台 CI。

---

## 推荐首个公开版本范围

v0.1.0 应该只包含：

```text
账号扫描
session 查看
新增账号
新增 relay 账号
sessions 单向同步
resume command 复制
Terminal.app 打开
```

不要急于加入：

```text
history merge
cloud sync
multi-user sharing
auto quota switching
session 内容编辑
```
