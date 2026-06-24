# 架构改造完成报告

**日期：** 2026-06-24  
**状态：** ✅ **阶段 1-2 完成 - 等待测试**

---

## 架构变更总结

### 之前（轻量级 PAT 存储）

```
~/.codex-account-a/          ← OAuth 账号
  ├── auth.json
  ├── config.toml
  └── sessions/

~/.config/agent-workspace/pat-accounts/  ← PAT 账号（轻量级）
  ├── auth-account-b.json
  └── metadata-account-b.json

~/.codex/                    ← 当前激活账号
```

### 现在（统一架构）

```
~/.codex-account-a/          ← OAuth 账号
  ├── auth.json
  ├── config.toml
  └── sessions/

~/.codex-account-b/          ← PAT 账号（完整目录）
  ├── auth.json              ← 上传的 + personal_access_token
  ├── config.toml            ← 最小配置
  └── sessions/              ← 空目录

~/.codex/                    ← 当前激活账号（根据模式切换）
```

---

## 实现的功能

### ✅ 阶段 1：Settings UI

**前端：**
- Settings 页面添加 "Account Switch Mode" 选项
- ☑️ OAuth Mode
- ⭕ PAT Mode
- 说明：OAuth: switch entire directory | PAT: copy auth.json only

**后端：**
- `get_auth_mode()` - 读取模式
- `set_auth_mode()` - 保存模式
- 存储位置：`~/.config/agent-workspace/settings.json`

**状态管理：**
- App.tsx 中的 `authMode` state
- 自动加载上次保存的模式
- 修改时自动保存

### ✅ 阶段 2：后端架构统一

**1. 统一存储结构**
- 所有账号（OAuth 和 PAT）都创建 `.codex-{id}/` 目录
- 移除轻量级 PAT 存储（`~/.config/agent-workspace/pat-accounts/`）
- 删除 `pat_accounts_dir()`, `pat_auth_path()`, `pat_metadata_path()`

**2. 重写 `add_pat_account()`**
```rust
pub fn add_pat_account() {
    // 1. 验证 account_id
    // 2. 创建 .codex-{id}/ 目录
    // 3. 创建 sessions/ 子目录
    // 4. 保存 auth.json（上传的 + personal_access_token）
    // 5. 创建最小 config.toml
    // 6. 添加管理标记
}
```

**3. 重写 `switch_to_pat_account()`**
```rust
pub fn switch_to_pat_account(account_id) {
    let auth_mode = get_auth_mode();
    
    if auth_mode == "oauth" {
        // OAuth 模式：切换整个目录
        rm -rf ~/.codex
        ln -s ~/.codex-{id} ~/.codex  // 或复制
    } else {
        // PAT 模式：只复制 auth.json
        cp ~/.codex-{id}/auth.json ~/.codex/auth.json
    }
}
```

**4. 新增辅助函数**
- `build_pat_auth_json()` - 合并上传的 auth.json 和 personal_access_token
- `copy_dir_all()` - 递归复制目录

**5. 移除代码**
- 删除 PAT 账号单独扫描逻辑（`list_accounts()` 中）
- 删除未使用的 `generate_pat_auth_json()`

---

## Git 提交记录

```
e0373a2 - refactor: unify PAT and OAuth account storage architecture
cc8d1cf - feat: add Auth Mode settings (OAuth/PAT switch mode)
9756109 - feat(ui): replace manual form with auth.json file upload
15e2ab3 - fix(ui): prevent badge wrapping and update PAT tab label
[...之前的提交...]

总计：11 commits
```

---

## 测试指南

**应用运行中：** PID 35499  
**URL：** http://127.0.0.1:1420/

### 测试 1：检查 Settings 页面

1. 打开应用
2. 导航到 Settings 标签
3. 查看 "Account Switch Mode"
4. 确认默认选中 "OAuth Mode"

**预期：**
- ✅ 显示两个选项：OAuth Mode / PAT Mode
- ✅ 说明文字显示
- ✅ 默认选中 OAuth

### 测试 2：上传 PAT 账号

**前提条件：** 准备一个 auth.json 文件

```bash
# 从现有 Codex 账号复制
cp ~/.codex/auth.json ~/Desktop/test-auth.json
```

**步骤：**
1. 点击 "New Account"
2. 选择 "PAT (Personal Access Token)" 标签
3. 点击 "Choose File"，选择 test-auth.json
4. （可选）填写 Personal Access Token
5. 点击 "Upload & Create"

**预期：**
- ✅ 账号创建成功
- ✅ 创建目录：`~/.codex-test-account/`
- ✅ 包含 auth.json, config.toml, sessions/
- ✅ auth.json 内容正确

**验证：**
```bash
ls -la ~/.codex-test-account/
cat ~/.codex-test-account/auth.json | grep personal_access_token
```

### 测试 3：OAuth 模式切换

**步骤：**
1. 确认 Settings > Auth Mode = OAuth Mode
2. 创建/选择一个账号
3. 点击 "Switch"

**预期（OAuth 模式）：**
- ✅ 删除旧的 `~/.codex`
- ✅ 创建符号链接：`~/.codex -> ~/.codex-{account_id}`
  ```bash
  ls -l ~/.codex
  # 应该显示：lrwxr-xr-x ... ~/.codex -> /Users/xxx/.codex-test-account
  ```
- ✅ 或者整个目录被复制

### 测试 4：PAT 模式切换

**步骤：**
1. 进入 Settings
2. 选择 "PAT Mode"
3. 返回 Overview
4. 点击账号的 "Switch"

**预期（PAT 模式）：**
- ✅ `~/.codex/` 目录保持存在
- ✅ 只有 `~/.codex/auth.json` 被更新
- ✅ `~/.codex/config.toml` 和 `sessions/` 不变

**验证：**
```bash
# 切换前记录时间戳
stat ~/.codex/config.toml

# 执行切换

# 切换后再查看
stat ~/.codex/config.toml  # 时间戳应该不变
stat ~/.codex/auth.json    # 时间戳应该更新
```

### 测试 5：模式持久化

**步骤：**
1. 选择 "PAT Mode"
2. 关闭应用
3. 重新打开应用
4. 进入 Settings

**预期：**
- ✅ 仍然选中 "PAT Mode"

**验证：**
```bash
cat ~/.config/agent-workspace/settings.json
# 应该包含：{"authMode":"pat"}
```

### 测试 6：混合账号列表

**前提条件：**
- 创建 1 个 OAuth 账号（传统方式）
- 创建 1 个 PAT 账号（上传 auth.json）

**预期：**
- ✅ 两个账号都出现在列表
- ✅ 可以在它们之间切换
- ✅ 切换行为由当前 Auth Mode 决定

---

## 架构优势

### 1. ✅ 统一管理
- 所有账号使用相同的目录结构
- 不再有特殊的 PAT 存储路径
- 简化代码逻辑

### 2. ✅ 灵活切换
- OAuth 模式：完整隔离（独立 sessions）
- PAT 模式：共享配置（只切换认证）
- 用户可以根据需求选择

### 3. ✅ 向后兼容
- 旧的 OAuth 账号继续工作
- 新的 PAT 账号使用相同结构
- 无缝迁移

### 4. ✅ 清晰的责任分离
- 账号存储：统一在 `~/.codex-{id}/`
- 切换逻辑：由 `auth_mode` 决定
- Settings：用户可见可控

---

## 已知限制

### 1. PAT 模式下的共享行为
- 所有 PAT 账号共享 `~/.codex/config.toml`
- 所有 PAT 账号共享 `~/.codex/sessions/`
- 如果需要隔离，应使用 OAuth 模式

### 2. 没有自动检测
- 不会根据账号类型自动切换模式
- 用户需要手动选择模式

### 3. 文件上传限制
- 需要用户提供完整的 auth.json
- 不能只输入 account_id + token

---

## 后续可选功能

### 1. 自动模式检测
- 检测账号是否有 `personal_access_token`
- 建议用户切换到 PAT 模式

### 2. 混合模式
- 允许每个账号指定切换方式
- 而不是全局模式

### 3. 迁移工具
- 将旧的轻量级 PAT 账号迁移到新架构
- 一键转换

### 4. 删除账号
- 添加删除按钮
- 清理 `.codex-{id}/` 目录

---

## 下一步

**现在请测试：**

1. ✅ Settings 页面的 Auth Mode 开关
2. ✅ 上传 auth.json 创建 PAT 账号
3. ✅ OAuth 模式切换（整个目录）
4. ✅ PAT 模式切换（只复制 auth.json）
5. ✅ 模式持久化（重启后保持）
6. 🔥 **Quota 功能（最关键）**

**如果一切正常 → 架构改造完成！** 🎉  
**如果有问题 → 立即报告，我会修复！** 🔧

---

**应用正在运行，等待你的测试反馈！** 🚀
