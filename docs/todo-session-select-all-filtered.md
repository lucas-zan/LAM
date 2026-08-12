# Todo: Select all filtered sessions

- **Status**: 验证成功
- **Scope**: 一次选择当前账户、时间与搜索条件下全部可删除 session
- **Baseline**: `agent/20260812-handoff-session-pagination` @ `641cf2523cc0ae7adbb8433975d399af9910706b`

## Why

分页列表只能逐条选择，无法批量清理完整过滤结果。全选不能为了取路径而把所有 session 内容长期保存在前端。

## Behavior

- **Input**: 当前账户、age filter 和搜索文本。
- **Output**: 表头三态复选框查询并选择所有匹配且可删除的 session 路径；再次点击清空。
- **Constraints/errors**: 受保留策略保护的 session 永不进入选择；查询仅在用户触发全选时执行。

## Test first

- Normal: 后端返回跨分页的匹配可删除路径，表头全选调用它并更新删除计数。
- Edge/invalid: 无可删除匹配时保持未选；部分选择显示 indeterminate。
- Expected RED: 当前没有路径查询接口和表头全选控件。

## Implementation

新增只返回路径的后端命令、API/store action 和表头复选框；复用现有时间、搜索与保护规则。

## Verification

- Focused: Rust query、API/store 与 Sessions view tests。
- Regression/lint/build: Rust/前端全量、lint、build、format 和 diff check。

## Evidence

- RED: Rust 测试因缺少选择查询接口无法编译；前端 API 和视图测试分别因函数与复选框不存在失败。
- GREEN: Rust 跨分页筛选测试 1 passed；前端 API/store/view 聚焦测试 3 passed。
- Regression: `cargo test` 全量通过；前端 24 files / 246 tests passed，lint、build、格式与 diff check 通过。
- Changed files: Rust session service/command/types/tests，以及前端 API/store/App/Sessions view/tests/styles。
- Remaining limitation: 带搜索文本的全选会在用户点击时解析全部候选 session 元数据，可能比无搜索条件的路径筛选慢，但不会把 session 内容长期加载到前端。

## Done

- [x] Test was written first and RED observed, or exception recorded.
- [x] Implementation stays in scope.
- [x] Focused and relevant regression checks pass.
- [x] Status is 验证成功.
