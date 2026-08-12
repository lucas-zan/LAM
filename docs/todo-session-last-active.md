# Todo: Show session last active time

- **Status**: 验证成功
- **Scope**: 在 Sessions 列表中展示每条 session 的最后活跃时间
- **Baseline**: `agent/20260812-handoff-session-pagination` @ `641cf2523cc0ae7adbb8433975d399af9910706b`

## Why

用户需要在清理和筛选 session 时直观看到最后活跃时间，而不只依赖当前排序。

## Behavior

- **Input**: session 的 Unix 秒级 `modifiedAt`。
- **Output**: 表格新增 Last active 列，以本地日期时间显示并保留机器可读时间。
- **Constraints/errors**: 六列仍需保持紧凑，Actions 不得换行撑高。

## Test first

- Normal: 列标题和每行格式化时间可见，`time` 元素包含 ISO `dateTime`。
- Edge/invalid: 时间列加入后 colgroup 必须恰好包含六列。
- Expected RED: 当前只有五列，且没有 Last active 字段。

## Implementation

在 Sessions 视图增加本地时间格式化和第六列，重新分配列宽；不改变后端数据与分页协议。

## Verification

- Focused: Sessions view test。
- Regression/lint/build: 前端全量测试、lint、build、format 和 diff check。

## Evidence

- RED: 聚焦测试失败，无法找到名为 `Last active` 的 columnheader；随后将不受当前 Chai 配置支持的 matcher 改为原生属性断言。
- GREEN: `npx vitest run src/routes/handoff.test.tsx -t "filters, size-sorts"`，1 passed。
- Regression: `npm test`，24 files / 245 tests passed；lint、build、Prettier 与 `git diff --check` 均通过。
- Changed files: `apps/desktop/src/routes/views.tsx`、`apps/desktop/src/styles.css`、`apps/desktop/src/routes/handoff.test.tsx`。
- Remaining limitation: 时间按运行 LAM 的系统本地时区显示到分钟，不提供独立时区切换。

## Done

- [x] Test was written first and RED observed, or exception recorded.
- [x] Implementation stays in scope.
- [x] Focused and relevant regression checks pass.
- [x] Status is 验证成功.
