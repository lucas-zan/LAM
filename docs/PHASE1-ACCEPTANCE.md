# Phase 1 MVP 手工验收矩阵（TODO-504）

版本：0.1  
日期：2026-06-04（与 README / 托盘优先启动对齐）  
依据：`docs/FINAL-DESIGN.md` §9.1

在跑矩阵前：

```bash
make check          # Rust + 前端构建 + ui-smoke
make start          # 或 LAM_HOME="$(pwd)/.fake-home" make start
```

macOS：`make start` 后应出现**菜单栏托盘**；主窗口默认隐藏，用托盘浮层 **Open** 或右键「Open LocalAgentManager」打开。

状态：`[ ]` 未测 `[x]` 通过 `[!]` 失败

## §9.1 八条

| # | 验收项 | App 内可观察 | 磁盘/其它验证 | 结果 | 备注 |
|---|--------|--------------|---------------|------|------|
| 1 | 1 分钟内创建受管账号 + wrapper | New Account → Dry Run → Create | `~/.codex-{name}`、`~/bin/codex-{name}` | [ ] | |
| 2 | A → `b-relay-a` sync 后可 resume，不覆盖 B history | Sync modal dry-run → execute；Sessions resume | B 的 `history.jsonl` 未被动 | [ ] | |
| 3 | 从不复制 `auth.json` | Sync dry-run blocked 列表 | 目标目录无新 auth；`cargo test` | [ ] | |
| 4 | 任意 sync 可先 dry-run，见将改动文件 | 无 plan 时 Execute 禁用 | | [ ] | |
| 5 | Overview 见所有 `~/.codex*` 与 session 计数 | Accounts + Overview metrics | | [ ] | 跨账号 Sessions 见 TODO-119 |
| 6 | Provider Phase 1 只读（规格） / 当前实现含 1.5 CRUD | Providers 页行为 | 不写明文 key | [ ] | |
| 7 | 额度来自 Codex app-server（5h/weekly）；不可用显示 N/A | Overview + 托盘浮层 | 无假剩余额度/伪造重置倒计时；`cargo test` quota 路径 | [ ] | 非 session token 估算 |
| 8 | README 5 分钟内跑起 App | `make start` → 托盘可用；可打开主窗口 | | [ ] | 主窗口非必须首屏 |

## 安全抽查

| 项 | 结果 | 备注 |
|----|------|------|
| relay 未复制 source auth | [ ] | |
| runtime profile sessions 未被 sync 改动 | [ ] | |
| execute sync 后有 backup + manifest | [ ] | |
| resume 命令无 API key 明文 | [ ] | Inspector |
| 托盘 Close / 点浮层外不收起误开主窗口 | [ ] | 仅 **Open** 或菜单打开主窗口 |
| `relay_resume_session` 不覆盖未确认的目标 session | [ ] | diverged 策略与 backup |

## 测试人 / 日期

- 环境：macOS 版本 ______ / Codex CLI ______  
- 执行人：______  
- 日期：______  

详细纠偏项见 `docs/CORRECTION-PLAN.md`。
