# Todo: Configure Codex launch permissions

- **Status**: 验证成功
- **Scope**: 保存用户选择的 Codex 权限预设，并在 LAM 启动或接力 Codex agent session 时统一应用
- **Baseline**: `agent/20260812-handoff-session-pagination` @ `641cf2523cc0ae7adbb8433975d399af9910706b`

## Why

LAM 启动或接力 CLI 时未携带用户偏好的权限预设，Codex 会再次要求设置权限。用户需要在系统设置中选择一次，并让普通 Resume 与 Relay/Handoff 入口统一生效。

## Behavior

- **Input**: `askForApproval`、`approveForMe`、`fullAccess` 三种预设。
- **Output**: 私有 settings.json 持久化选择；直接 Codex 命令与 LAM launcher 分别注入 CLI 0.147.0 对应参数。
- **Constraints/errors**: 未配置或非法值回退 Ask for approval；Full Access 必须显示绕过审批与沙箱的风险；登录认证命令不注入 agent 执行权限。

## Test first

- Normal: 三种设置读写；Resume 与 Relay/Handoff 命令包含对应参数；Settings 可切换预设。
- Edge/invalid: 缺失或未知设置回退 Ask for approval，参数不能重复注入。
- Expected RED: 当前没有设置 API、UI 或启动参数注入。

## Implementation

增加强类型权限预设与 settings API；planner 对直接 Codex 命令注入参数，`lam` launcher 对所有经 launcher 的命令注入参数；Settings 增加带风险说明的下拉选项。

## Verification

- Focused: Rust settings/launch tests、frontend API/store/view tests。
- Regression/lint/build: Rust 与前端全量、lint、build、format、diff check。

## Evidence

- RED: Rust setting/launch tests initially failed on missing permission preset APIs and planner fields; frontend tests failed on missing API and Settings control.
- GREEN: Settings persistence, direct and gateway launch injection, managed wrappers, Resume, Relay/Handoff, API/store/UI, and the packaged launcher process test all pass.
- Changed files: Rust settings/commands/planner/launcher/wrapper/relay plus frontend types/API/store/Settings UI and their tests.
- Remaining limitation: The mapping is verified against locally installed `codex-cli 0.147.0`; only Codex sessions launched through LAM are affected, not an unrelated shell invocation of `codex`.

## Done

- [x] Test was written first and RED observed, or exception recorded.
- [x] Implementation stays in scope.
- [x] Focused and relevant regression checks pass.
- [x] Status is 验证成功.
