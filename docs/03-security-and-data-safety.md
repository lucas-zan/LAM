# 安全与数据保护设计

版本：0.1 draft

---

## 1. 安全目标

Codex Relay 的核心安全目标是：

1. 保持不同 Codex 账号登录态隔离。
2. 只同步对 session relay 必要的数据。
3. 防止复制、覆盖或泄漏认证文件。
4. 防止污染目标账号原有历史。
5. 所有操作本地执行，不上传数据。
6. 对写操作提供 dry-run、备份、恢复路径。

---

## 2. 敏感文件分类

### 2.1 绝对禁止复制

```text
auth.json
```

原因：包含认证相关状态。复制会导致账号串号或 token 泄漏。

### 2.2 默认禁止复制

```text
config.toml
logs_2.sqlite
logs_2.sqlite-shm
logs_2.sqlite-wal
state_*.sqlite
installation_id
cache/
tmp/
log/
```

原因：

- 可能包含账号级配置。
- 可能包含内部状态。
- 可能体积大、易冲突。
- 不属于 resume 所需最小数据集。

### 2.3 可同步

```text
sessions/
```

原因：用于 `codex resume` 的本地 session 数据。

### 2.4 可旁路备份，不默认参与目标历史

```text
history.jsonl
```

推荐处理：

```text
history.from-a.20260526-143000.jsonl
```

不推荐处理：

```text
append into target history.jsonl
```

---

## 3. 同步安全策略

### 3.1 默认策略

```text
只同步 sessions/
同步前备份目标 sessions/
不复制 auth.json
不复制 history.jsonl
不复制 SQLite / cache / logs
```

### 3.2 Relay 策略

推荐将接力开发放到 relay account：

```bash
~/.codex-b         # B 原始账号，不动
~/.codex-b-relay-a # B 临时接力 A
```

这样 A 的项目 session 不会污染 B 原来的项目 session。

### 3.3 Dry-run

任何写操作必须先能生成 plan：

```text
will_create_dirs
will_create_files
will_copy_files
will_backup_dirs
will_skip_files
warnings
```

用户确认后才执行。

---

## 4. 路径安全

### 4.1 限制操作范围

默认只允许操作：

```text
$HOME/.codex
$HOME/.codex-*
$HOME/bin/codex-*
```

### 4.2 禁止路径穿越

用户输入 account name 后，只能生成预期路径。禁止：

```text
../x
~/x
/x
x/y
x y
```

### 4.3 canonicalize

执行前必须对路径做 canonicalize，并确认仍在允许范围内。

---

## 5. Shell 命令安全

### 5.1 不让前端直接执行任意命令

前端只能请求：

```text
open terminal for this validated account + validated session
```

不能直接传入任意 shell 字符串给后端执行。

### 5.2 Shell escape

如果需要构造命令：

```bash
cd '...'; CODEX_HOME='...' codex resume '...'
```

每个参数必须单独 escape。

### 5.3 Terminal 权限

如果 macOS 阻止 AppleScript 控制 Terminal，App 应 fallback 到复制命令，并提示用户手动粘贴执行。

---

## 6. 隐私策略

### 6.1 默认不上网

App 不需要联网。后续如果检查更新，应明确提示并可关闭。

### 6.2 不读取 token 内容

只判断文件存在：

```text
has_auth = auth.json exists
```

不读取其内容。

### 6.3 不上传 session

Session 内容可能包含代码、路径、业务上下文，必须留在本机。

---

## 7. 风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| 用户误把 A 同步到 B 原账号 | 污染 B sessions | 默认推荐 relay account，执行前显示目标路径 |
| session 格式变化 | 解析失败 | 保守解析，解析失败仍可文件级同步 |
| 复制 auth.json | 账号串号 | 硬编码黑名单，测试覆盖 |
| history 合并污染 | 历史混乱 | 默认禁用，仅 sidecar backup |
| Terminal 命令注入 | 执行恶意命令 | 严格命名校验、参数 escape、后端构造命令 |
| 目标 session 覆盖 | 数据丢失 | 默认备份目标 sessions |

---

## 8. 安全测试清单

- [ ] 输入 `../evil` 被拒绝。
- [ ] 输入 `a/b` 被拒绝。
- [ ] 输入 `a; rm -rf ~` 被拒绝。
- [ ] 同步时 `auth.json` 不会被复制。
- [ ] 同步时 `history.jsonl` 默认不会被复制。
- [ ] 同步前自动备份目标 sessions。
- [ ] wrapper 里不包含绝对用户名路径以外的敏感信息。
- [ ] Terminal command 中所有路径和 session id 都被 escape。
- [ ] app 断网可用。

---

## 9. 开源安全说明建议

README 中必须明确写：

```text
This app does not bypass Codex limits. It only manages local CODEX_HOME directories and session files.
This app never copies auth.json.
Use relay accounts if you want to continue another account's session without polluting your original account history.
```
