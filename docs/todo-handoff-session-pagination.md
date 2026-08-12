# Todo: Handoff session pagination and error feedback

- **Status**: 验证成功
- **Scope**: Handoff 弹窗只按最近优先分页加载 session，并显示可读的后端错误
- **Baseline**: `agent/20260812-handoff-session-pagination` @ `641cf2523cc0ae7adbb8433975d399af9910706b`

## Why

`main` 账号的 session 数量和 transcript 体积很大。当前 Handoff 一次加载全部 session，且每个文件都完整读入内存，导致加载失败时列表为空；前端还把结构化 Tauri 错误渲染为 `[object Object]`。

## Behavior

- **Input**: Handoff source account，默认请求最近一页；用户可继续加载更早的 session。
- **Output**: 首页最多 5 条，按修改时间倒序；后续页追加更早的 5 条；Handoff 错误显示错误对象中的 message。
- **Constraints/errors**: 后端必须使用 bounded tail read；分页 cursor 必须稳定，不能依赖 offset；从已有旧 session 入口打开时不能丢失该 session。

## Test first

- Normal: 后端首/后续页返回稳定的 5 条；前端显示首页并可加载更早 session。
- Edge/invalid: 没有下一页时隐藏/禁用继续加载；文件小于 tail limit 时完整返回；cursor 无效时返回结构化错误。
- Error/state conflict: Handoff session 请求失败时保留弹窗并显示可读错误，不显示 `[object Object]`。
- Expected RED: 新增测试在分页 API、bounded tail read 和前端分页/错误处理实现前失败。

## Implementation

增加 `list_sessions` 的分页请求/响应契约，按修改时间和路径生成稳定 cursor；session 扫描只读取目标页并用文件尾部 bounded read 提取元数据。Handoff 弹窗维护 cursor 和已加载列表，使用“加载更早的 5 条”入口，并复用 `formatError`。非 Handoff session 浏览行为保持兼容。

## Verification

- Focused: Rust session tests；`apps/desktop` Handoff/API tests。
- Regression/lint/build: 相关前端测试、Rust 测试、TypeScript build/lint、`git diff --check`。

## Evidence

- RED: 分页核心函数、`SessionPageRequest`、`listSessionsPage` 和 Handoff 新测试在实现前均按预期失败；另发现全局测试夹具中后一行 `mockResolvedValue([])` 覆盖了分页 fallback，已修正。
- GREEN: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test phase1_core --no-default-features`（65 passed）；`npm test -- --run src/App.handoff.test.tsx`（42 passed）；`npm test -- --run src/lib/api.test.ts`（10 passed）；`npm run build`、`npm run lint`、`cargo fmt -- --check`、`git diff --check` 通过。
- Changed files: Rust 分页服务/命令注册、bounded tail reader；TypeScript API/types；Handoff 弹窗分页与 `formatError`；Rust/前端回归测试。
- Remaining limitation: 非 Handoff 的 Sessions 浏览仍使用原有完整列表 API；本次只改变 Handoff 读取路径，避免扩大行为范围。

## Done

- [x] Test was written first and RED observed, or exception recorded.
- [x] Implementation stays in scope.
- [x] Focused and relevant regression checks pass.
- [x] Status is 验证成功.
