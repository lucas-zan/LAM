# RPG-307: Unified Codex launch planner

- **Status**: 验证成功
- **Depends on**: RPG-306
- **Scope**: normal/resume/exec-resume/relay/wrapper/copy/terminal 的统一 typed launch plan；login 保持独立。

## Logic design

- `CodexLaunchPlanner` 接收 profile binding、entry kind、cwd 与 typed args，产出 direct 或 `lam codex --profile` command spec。
- Gateway profile 总经 launcher；Direct profile 也经同一 decision path 后保留现有行为。
- command renderer 做平台明确的 argv/shell escaping，前端只能选择 typed entry，不能注入任意 shell。
- 删除支持路径中各自拼接的 `CODEX_HOME=... codex`，login 不复制 Provider auth。

## Test and acceptance

- [x] red test：首次运行因 `gateway::launch_planner` 不存在而按预期失败（E0432）。
- [x] typed normal/resume/resume-last/exec-resume/relay-handoff/terminal/copy-command matrix；Gateway 全走 `lam codex --profile`，Direct 同一 planner 保持旧 shape。
- [x] relay/resume builders 通过 persisted profile route resolver；managed wrapper 保留兼容 `CODEX_HOME` export，但 exec 只走 launcher，不再 `CODEX_BIN` bypass。
- [x] cwd/session/prompt/args 使用 allowlisted static token + strict dynamic shell quoting；login 路径保持独立。
- [x] `rg` 仅剩说明文字与 unit-test synthetic raw command；planner 4 passed、Phase1 core 53 passed，targeted clippy/fmt/diff 通过。

## STOP conditions

- 某支持入口缺少 profile binding 上下文且不能安全解析。
