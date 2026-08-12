# Todo: Relay session labels put the session ID first

- **Status**: 验证成功
- **Scope**: Handoff 等接力选择界面统一以 session ID 作为标签首项
- **Baseline**: `agent/20260812-handoff-session-pagination` @ `641cf2523cc0ae7adbb8433975d399af9910706b`

## Why

接力选择器当前先显示 thread 文本，长文本会挤占可见空间，使真正用于识别和接力的 session ID 落在末尾。预期 ID 始终在最前，辅助文本随后显示。

## Behavior

- **Input**: 包含 ID、thread name/summary 和 cwd 的 session。
- **Output**: 接力标签为 `sessionId · 文本 · cwd`；文本等于 ID 时去重。
- **Constraints/errors**: 只改变接力相关标签，不改变 Session 详情页的主标题规则。

## Test first

- Normal: 有 thread name 的 session 以 ID 开头，再显示 thread name 和 cwd。
- Edge/invalid: 没有独立文本或 cwd 时不重复 ID，并显示 `unknown cwd`。
- Expected RED: 新标签格式化函数尚不存在，Handoff option 仍以文本开头。

## Implementation

增加接力专用标签格式化函数，并用于 Handoff 下拉项和已选 session 预览。

## Verification

- Focused: format 单元测试、Handoff 回归测试。
- Regression/lint/build: ESLint、TypeScript/Vite build、diff check。

## Evidence

- RED: `format.test.ts` 2 个测试因 `relaySessionLabel is not a function` 失败；Handoff 行为测试收到旧顺序 `main-5 thread name · /repo/main · main-5`，与期望 ID-first 不符。
- GREEN: `format.test.ts`（2 passed）；`App.handoff.test.tsx`（42 passed）；`npm run build`、`npm run lint`、受影响文件 Prettier 检查及 `git diff --check` 通过。
- Changed files: 接力专用标签函数、Handoff option/预览展示、对应单元与行为测试。
- Remaining limitation: 普通 Session 列表和详情页继续以可读标题为主，符合本次非目标约束。

## Done

- [x] Test was written first and RED observed, or exception recorded.
- [x] Implementation stays in scope.
- [x] Focused and relevant regression checks pass.
- [x] Status is 验证成功.
