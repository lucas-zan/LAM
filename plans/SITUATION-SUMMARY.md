# PAT Feature Implementation - Final Summary

**Date:** 2026-06-24  
**Status:** 🔴 **CORE FEATURE MISSING** - Plan 004 needed

---

## 问题发现

你提出的正确需求和我实现的 Plans 001-003 **不匹配**！

### 你的实际需求（正确）

**核心功能：PAT 账号切换**
- 用户切换账号时选择 "Use PAT" 模式
- 上传凭证 JSON → Lam 写入 `~/.codex/auth.json`
- 格式：`{ "personal_access_token": "at-xxx" }`
- **不需要手动 `codex login`** - 直接可用

**JSON 转换：**
```
用户上传: headers.authorization = "Bearer at-xxx"
       ↓
生成 auth.json: personal_access_token = "at-xxx"
       ↓
写入: ~/.codex/auth.json
```

### Plans 001-003 实际实现了什么（错误方向）

**实现的是：元数据追踪系统**
- ✅ 在 `~/.config/agent-workspace/auth-metadata/` 存储 PAT 信息
- ✅ **只读**检测 Codex 的 auth.json
- ✅ 显示 auth mode 徽章
- ✅ 显示过期警告
- ❌ **但从不写入 `~/.codex/auth.json`** ← 核心功能缺失

**问题：**
- 只是"显示"现有 PAT 状态
- 不能"创建"PAT 切换
- 用户仍需手动 `codex login`

---

## 架构对比

### Plans 001-003（辅助功能 - 已完成）

```
用户上传 JSON
    ↓
存储到 ~/.config/agent-workspace/auth-metadata/{profile}.json  (Lam 元数据)
    ↓
读取 ~/.codex/auth.json (只读检测)
    ↓
显示 badge + 过期警告
```

**特点：**
- 独立的元数据系统
- 从不修改 Codex 文件
- 只显示，不操作

### Plan 004（核心功能 - 待实现）

```
用户选择 "Switch with PAT"
    ↓
上传 JSON → 提取 headers.authorization
    ↓
生成 auth.json: { "personal_access_token": "at-xxx" }
    ↓
写入 ~/.codex/auth.json  (直接写入！)
    ↓
复制 config.toml
    ↓
同时记录到 Lam 元数据 (复用 Plan 001 的函数)
    ↓
完成切换，无需 codex login
```

**特点：**
- **主动写入** `~/.codex/auth.json`
- 实现账号切换自动化
- 用户体验完整

---

## Plans 001-003 是否需要废弃？

**答案：不需要！** 它们可以保留为辅助功能。

**Plan 001-003 提供的基础设施（复用）：**
1. ✅ `UploadedCredentials` 类型定义
2. ✅ `AuthMetadata` 类型定义
3. ✅ `record_pat_metadata()` - 记录过期时间
4. ✅ `check_token_expiration()` - 检查过期
5. ✅ 前端 badge 组件
6. ✅ 前端 upload modal UI

**Plan 004 将复用这些，并添加核心逻辑：**
- `switch_account_with_pat()` 函数
- 写入 `~/.codex/auth.json`
- 切换模式选择对话框

---

## Plan 004 创建完成

**文件：** `plans/004-pat-account-switching.md`

**核心改动：**

### 后端 (Rust)
```rust
// 新函数
pub fn switch_account_with_pat(
    home_root: &Path,
    req: &SwitchAccountWithPatRequest,
) -> Result<()> {
    // 1. 提取 token from headers.authorization
    let token = extract_bearer_token(&req.credentials)?;
    
    // 2. 生成 auth.json
    let auth_json = generate_pat_auth_json(&token);
    
    // 3. 写入 ~/.codex/auth.json  ← 关键！
    let auth_path = home_root.join(".codex/auth.json");
    write_file_private(&auth_path, &auth_json)?;
    
    // 4. 复制 config.toml
    copy_config_toml(&profile, &main_codex)?;
    
    // 5. 记录元数据（复用 Plan 001）
    record_pat_metadata(home_root, &req.profile_id, &metadata)?;
    
    Ok(())
}

fn generate_pat_auth_json(token: &str) -> String {
    r#"{
  "OPENAI_API_KEY": null,
  "personal_access_token": "{token}"
}"#
}
```

### 前端 (TypeScript/React)
```typescript
// 切换按钮点击 → 显示模式选择
<button onClick={() => openSwitchModeDialog(account.id)}>
  Switch
</button>

// 模式选择对话框
<Modal title="Choose Switch Method">
  <button onClick={useOAuthLogin}>OAuth Login</button>
  <button onClick={usePatUpload}>Use PAT</button>
</Modal>

// PAT 模式 → 上传 JSON → 调用新命令
async function handleSwitchWithPat(profileId, credentials) {
  await api.switchAccountWithPat({ profileId, credentials });
  // auth.json 已写入，可直接使用 codex
}
```

**用户流程：**
1. 点击 "Switch to account A"
2. 选择 "Use PAT"
3. 粘贴 JSON (包含 `headers.authorization`)
4. 点击 Upload
5. ✅ 切换完成 - 直接使用，无需 `codex login`

---

## 下一步行动

### 选项 1：执行 Plan 004（推荐）

```bash
# Plan 004 实现核心 PAT 切换功能
# 预估：中等规模，1-2 小时
```

**理由：**
- 这才是你真正需要的功能
- Plans 001-003 可以保留（提供基础设施）
- 完成后用户体验完整

### 选项 2：先手动验证 Plans 001-003

虽然方向不对，但可以验证：
- Badge 显示是否正常
- UI 组件是否工作
- 为 Plan 004 提供测试基础

### 选项 3：从头重构

废弃 Plans 001-003，只实现 Plan 004。

**不推荐：** Plans 001-003 的代码（类型、UI）可以复用。

---

## 建议

**立即执行 Plan 004：**

1. ✅ Plan 004 已创建：`plans/004-pat-account-switching.md`
2. ⏳ 关闭当前运行的 Tauri app
3. ⏳ 执行 Plan 004
4. ⏳ 验证完整的 PAT 切换流程

**执行命令：**
```bash
# 停止当前 app
pkill -f localagentmanager

# 执行 Plan 004
# (使用和 Plans 001-003 相同的 executor 模式)
```

---

## 总结

| 内容 | Plans 001-003 | Plan 004 |
|------|---------------|----------|
| **性质** | 辅助（元数据追踪） | 核心（账号切换） |
| **写入 auth.json** | ❌ 只读 | ✅ 主动写入 |
| **用户价值** | 显示状态 | 自动化切换 |
| **状态** | ✅ 完成 | ⏳ 待执行 |
| **测试** | 36/36 通过 | 待实现 |
| **是否保留** | ✅ 保留（复用） | - |

**结论：Plans 001-003 走错了方向，但可以保留。Plan 004 实现真正的需求。**

---

**你想要我现在执行 Plan 004 吗？**
