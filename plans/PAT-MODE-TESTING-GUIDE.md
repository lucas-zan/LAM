# 🎯 PAT Mode 完整测试指南

**日期：** 2026-06-24  
**状态：** ✅ 所有功能已实现，等待测试

---

## 📊 实现总结

### Git 历史（最新 17 commits）

```
2f7368a - feat: disable buttons for active account in PAT mode
8c25a1b - refactor: rename 'Upload PAT' button to 'Switch' on account cards
5c45d0a - feat: add Account Name input and auto-switch for PAT accounts
0b0617b - refactor: move Auth Mode toggle to header
e0373a2 - refactor: unify PAT and OAuth account storage architecture
cc8d1cf - feat: add Auth Mode settings (OAuth/PAT switch mode)
9756109 - feat(ui): replace manual form with auth.json file upload
```

### 核心功能

1. ✅ **Header Toggle Switch** - PAT/OAuth 模式切换
2. ✅ **PAT 创建表单** - Account Name + Upload + Switch
3. ✅ **Switch 按钮** - 账号卡片上的切换按钮
4. ✅ **按钮禁用逻辑** - PAT Mode 下的智能禁用
5. ✅ **统一架构** - 所有账号使用相同目录结构

---

## 🚀 测试步骤

### 前提条件

**应用状态：**
- 运行中：PID 31784
- URL: http://127.0.0.1:1420/
- 日志：/tmp/tauri-pat-disabled.log

**准备测试文件：**
```bash
# 备份当前 auth.json（如果有）
cp ~/.codex/auth.json ~/Desktop/backup-auth.json 2>/dev/null || echo "No existing auth.json"
```

---

## 测试 1：创建第一个 PAT 账号

### 目标
验证 PAT 账号创建流程和 Account Name 功能。

### 步骤

1. **打开应用**
   - 访问 http://127.0.0.1:1420/
   - 确认当前显示 "No Codex profiles found"

2. **点击 "New Account"**
   - 点击右上角的 "+ New Account" 按钮

3. **选择 PAT 标签**
   - 点击 "PAT (Personal Access Token)" 标签

4. **填写表单**
   ```
   Account name: test-main
   Upload file: ~/Desktop/backup-auth.json
   Personal Access Token: (留空)
   ```

5. **点击 "🔄 Switch"**

### 预期结果

- ✅ 创建 `~/.codex-test-main/` 目录
- ✅ 包含以下文件：
  - `auth.json` - 从上传文件复制
  - `config.toml` - 最小配置
  - `sessions/` - 空目录
  - `.managed-by-agent-workspace.json` - 标记文件
- ✅ 模态框自动关闭
- ✅ 账号出现在 Overview 列表

### 验证命令

```bash
# 1. 检查目录创建
ls -la ~/.codex-test-main/

# 预期输出：
# drwx------  auth.json
# drwx------  config.toml
# drwxr-xr-x  sessions/
# -rw-r--r--  .managed-by-agent-workspace.json

# 2. 检查 auth.json 内容
cat ~/.codex-test-main/auth.json | jq .

# 预期：包含完整的认证信息

# 3. 检查 config.toml
cat ~/.codex-test-main/config.toml

# 预期：
# # PAT account
```

---

## 测试 2：验证 PAT Mode Toggle

### 目标
确认 Toggle 开关工作正常并且状态持久化。

### 步骤

1. **当前状态检查**
   ```bash
   cat ~/.config/agent-workspace/settings.json
   ```
   - 预期：`{"authMode":"oauth"}` 或 `{"authMode":"pat"}`

2. **在应用中**
   - 找到 Header 中间的 "PAT Mode" toggle
   - 当前状态：OFF (灰色) = OAuth Mode

3. **打开 PAT Mode**
   - 点击 Toggle → 切换到 ON (蓝色)

4. **验证保存**
   ```bash
   cat ~/.config/agent-workspace/settings.json
   ```
   - 预期：`{"authMode":"pat"}`

5. **重启应用**
   ```bash
   # 关闭并重新打开应用窗口
   ```

6. **验证状态保持**
   - Toggle 应该仍然是 ON (蓝色)

---

## 测试 3：创建第二个 PAT 账号

### 目标
验证多账号管理和 Switch 按钮。

### 步骤

1. **确认 Toggle ON**
   - PAT Mode 应该是 ON (蓝色)

2. **创建第二个账号**
   - New Account → PAT tab
   - Account name: `test-backup`
   - Upload: `~/Desktop/backup-auth.json`
   - PAT: (留空或填写测试 token)
   - Click "Switch"

3. **观察账号列表**
   - 应该看到两个账号：
     - test-main
     - test-backup (新创建的，应该是 Active)

### 预期结果

- ✅ 创建 `~/.codex-test-backup/` 目录
- ✅ 两个账号都显示在列表
- ✅ test-backup 有 "Active" 标记
- ✅ test-main 没有 "Active" 标记

---

## 测试 4：PAT Mode 下的按钮状态

### 🔥 关键测试！

### 目标
验证 PAT Mode 下按钮禁用逻辑正确。

### 前提
- Toggle ON (PAT Mode)
- 有两个账号：test-main, test-backup
- test-backup 是当前激活账号 (Active)

### 步骤

1. **检查激活账号 (test-backup) 的按钮**

   **预期：所有按钮都是灰色（禁用）**
   
   | 按钮 | 状态 | Tooltip |
   |------|------|---------|
   | Relay Latest | ❌ 灰色 | "Not available for active account in PAT mode" |
   | Handoff | ❌ 灰色 | "Not available for active account in PAT mode" |
   | Sync Sessions | ❌ 灰色 | "Not available for active account in PAT mode" |
   | Rename | ❌ 灰色 | "Not available for active account in PAT mode" |
   | Login | ❌ 灰色 | "Not available for active account in PAT mode" |
   | Switch | ❌ 灰色 | "Already active in PAT mode" |

2. **检查非激活账号 (test-main) 的按钮**

   **预期：只有 Switch 可用**
   
   | 按钮 | 状态 | Tooltip |
   |------|------|---------|
   | Relay Latest | ❌ 灰色 | "Not available for active account in PAT mode" |
   | Handoff | ❌ 灰色 | "Not available for active account in PAT mode" |
   | Sync Sessions | ❌ 灰色 | "Not available for active account in PAT mode" |
   | Rename | ❌ 灰色 | "Not available for active account in PAT mode" |
   | Login | ❌ 灰色 | "Not available for active account in PAT mode" |
   | **Switch** | ✅ **蓝色** | "Switch to this account" |

3. **鼠标悬停验证**
   - 将鼠标悬停在每个按钮上
   - 确认 tooltip 文字正确显示

### 验证截图

**期望看到：**
```
┌─────────────────────────────────────┐
│ test-backup        [Active]         │
│ ~/.codex-test-backup/               │
│                                     │
│ [灰] Relay Latest                   │
│ [灰] Handoff                        │
│ [灰] Sync Sessions                  │
│ [灰] Rename                         │
│ [灰] Login                          │
│ [灰] Switch                         │
└─────────────────────────────────────┘

┌─────────────────────────────────────┐
│ test-main          [Logged in]      │
│ ~/.codex-test-main/                 │
│                                     │
│ [灰] Relay Latest                   │
│ [灰] Handoff                        │
│ [灰] Sync Sessions                  │
│ [灰] Rename                         │
│ [灰] Login                          │
│ [蓝] 🔄 Switch  ← 只有这个可用！     │
└─────────────────────────────────────┘
```

---

## 测试 5：PAT Mode 下切换账号

### 目标
验证 PAT Mode 下只复制 auth.json 的逻辑。

### 前提
- Toggle ON (PAT Mode)
- test-backup 当前激活

### 步骤

1. **记录当前状态**
   ```bash
   # 记录时间戳
   stat ~/.codex/auth.json | grep Modify
   stat ~/.codex/config.toml | grep Modify
   stat ~/.codex/sessions/ | grep Modify
   
   # 记录文件内容
   ls -la ~/.codex/sessions/
   ```

2. **点击 test-main 的 Switch 按钮**
   - 找到 test-main 账号卡片
   - 点击蓝色的 "🔄 Switch" 按钮
   - 应该打开 PAT 上传模态框

3. **上传 auth.json**
   - Upload: 选择 test-main 的 auth.json
     ```bash
     # 或者使用已有的备份
     # ~/Desktop/backup-auth.json
     ```
   - 点击 "Switch"

4. **验证切换结果**
   ```bash
   # 1. 检查 auth.json 已更新
   stat ~/.codex/auth.json | grep Modify
   # 预期：时间戳更新（最新）
   
   # 2. 检查 config.toml 未变
   stat ~/.codex/config.toml | grep Modify
   # 预期：时间戳不变（旧的）
   
   # 3. 检查 sessions/ 未变
   stat ~/.codex/sessions/ | grep Modify
   ls -la ~/.codex/sessions/
   # 预期：内容不变
   ```

### 预期结果

**只有 auth.json 更新：**
- ✅ `~/.codex/auth.json` - 时间戳最新
- ❌ `~/.codex/config.toml` - 时间戳不变
- ❌ `~/.codex/sessions/` - 内容不变

**这就是 PAT Mode 的核心行为！**

---

## 测试 6：OAuth Mode 下的完整切换

### 目标
验证 OAuth Mode 切换整个目录。

### 步骤

1. **关闭 PAT Mode**
   - 点击 Toggle → 切换到 OFF (灰色)
   - 确认：`cat ~/.config/agent-workspace/settings.json`
   - 应该显示：`{"authMode":"oauth"}`

2. **检查按钮状态变化**
   - **预期：所有按钮都变成可用（蓝色）**
   - 激活账号的按钮不再禁用
   - 所有账号的所有按钮都可用

3. **切换账号**
   - 点击 test-main 的 "Switch" 按钮

4. **验证完整切换**
   ```bash
   # 检查是否是 symlink 或完整复制
   ls -l ~/.codex
   
   # 如果是 symlink：
   # lrwxr-xr-x ... ~/.codex -> /Users/xxx/.codex-test-main
   
   # 验证所有文件都切换了
   stat ~/.codex/auth.json
   stat ~/.codex/config.toml
   ls -la ~/.codex/sessions/
   ```

### 预期结果

**OAuth Mode（完整切换）：**
- ✅ `~/.codex/` → symlink 指向 `~/.codex-test-main/`
  - 或者完整复制整个目录
- ✅ 所有文件都是新的（auth.json, config.toml, sessions/）
- ✅ 完全隔离的环境

---

## 测试 7：按钮状态对比表

### OAuth Mode vs PAT Mode 对比

| 场景 | OAuth Mode | PAT Mode |
|------|-----------|----------|
| **激活账号** | | |
| - Relay Latest | ✅ 可用 | ❌ 禁用 |
| - Handoff | ✅ 可用 | ❌ 禁用 |
| - Sync Sessions | ✅ 可用 | ❌ 禁用 |
| - Rename | ✅ 可用 | ❌ 禁用 |
| - Login | ✅ 可用 | ❌ 禁用 |
| - Switch | ✅ 可用 | ❌ 禁用 |
| **非激活账号** | | |
| - Relay Latest | ✅ 可用 | ❌ 禁用 |
| - Handoff | ✅ 可用 | ❌ 禁用 |
| - Sync Sessions | ✅ 可用 | ❌ 禁用 |
| - Rename | ✅ 可用 | ❌ 禁用 |
| - Login | ✅ 可用 | ❌ 禁用 |
| - **Switch** | ✅ 可用 | ✅ **可用** |

### 切换行为对比

| 模式 | 切换行为 |
|------|---------|
| **OAuth Mode** | 切换整个 `~/.codex/` 目录<br>→ Symlink 或完整复制<br>→ 完全隔离环境 |
| **PAT Mode** | 只复制 `auth.json`<br>→ config.toml 不变<br>→ sessions/ 共享 |

---

## 🎯 成功标准

### 必须通过的测试

1. ✅ **创建 PAT 账号** - 目录结构正确
2. ✅ **Toggle 持久化** - 重启后状态保持
3. ✅ **PAT Mode 按钮逻辑** - 激活账号所有按钮禁用，其他账号只有 Switch 可用
4. ✅ **PAT Mode 切换** - 只更新 auth.json
5. ✅ **OAuth Mode 切换** - 更新整个目录
6. ✅ **模式切换** - Toggle 改变按钮状态

### 额外测试（如果可能）

7. 🔥 **Quota 刷新** - 切换后 Refresh 按钮工作
8. 🔥 **Session 功能** - PAT Mode 下 sessions 共享

---

## 🐛 已知问题和限制

### 当前限制

1. **PAT Mode 下功能受限**
   - 不能 Relay/Handoff（因为 sessions 共享）
   - 不能独立 Rename（会混淆目录）
   - 只能通过 Switch 改变认证

2. **主要用例**
   - PAT Mode 适合：单用户多 token 轮换
   - OAuth Mode 适合：多租户完全隔离

---

## 📝 测试报告模板

**测试完成后，请填写：**

```markdown
# PAT Mode 测试报告

**测试日期：** 2026-06-24  
**测试人员：** [你的名字]  
**应用版本：** Commit 2f7368a

## 测试结果

### 测试 1：创建 PAT 账号
- [ ] 通过 / [ ] 失败
- 问题：

### 测试 2：Toggle 持久化
- [ ] 通过 / [ ] 失败
- 问题：

### 测试 3：多账号管理
- [ ] 通过 / [ ] 失败
- 问题：

### 测试 4：按钮禁用逻辑
- [ ] 通过 / [ ] 失败
- 问题：

### 测试 5：PAT Mode 切换
- [ ] 通过 / [ ] 失败
- 问题：

### 测试 6：OAuth Mode 切换
- [ ] 通过 / [ ] 失败
- 问题：

## 额外发现

[记录任何意外行为、UI 问题或改进建议]

## 总体评价

- [ ] ✅ 所有功能正常，可以发布
- [ ] ⚠️ 部分功能有问题，需要修复
- [ ] ❌ 严重问题，需要重新设计

## 下一步

[需要修复的问题或改进建议]
```

---

## 🚀 开始测试！

**应用状态：**
- ✅ 运行中（PID 31784）
- ✅ URL: http://127.0.0.1:1420/
- ✅ 所有代码已提交（17 commits）

**准备好了吗？开始测试吧！** 🎉

如果遇到任何问题，立即告诉我！
