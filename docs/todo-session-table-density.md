# Todo: Fix session row density and All pagination clarity

- **Status**: 验证成功
- **Scope**: 修复五列表格列宽导致的 Actions 换行撑高，并明确 All 是不限时间的分页查询
- **Baseline**: `agent/20260812-handoff-session-pagination` @ `641cf2523cc0ae7adbb8433975d399af9910706b`

## Why

加入 Size 列后旧四列宽度仍占满 100%，Actions 列被压缩并纵向换行，导致每行异常高。All 文案也容易被理解为已一次性显示全部记录。

## Behavior

- **Input**: Sessions 表格、All 时间筛选、分页结果。
- **Output**: 五列使用显式稳定列宽，Actions 保持紧凑；All 标为 All time，并显示当前已加载数量/总数与分页提示。
- **Constraints/errors**: 继续保持每页 20 条，不能为文案语义重新引入全量加载内存问题。

## Test first

- Normal: 表格存在五个具名 col，Actions 不换行；All time 与 Showing N of total 文案可见。
- Edge/invalid: 无更多页时显示完整数量，不显示加载按钮。
- Expected RED: 当前没有 colgroup，All 文案不含时间含义，未展示已加载/总数。

## Implementation

增加 colgroup 与具名 CSS 宽度，禁止 row actions 换行；补充分页计数文案。不改变后端查询契约。

## Verification

- Focused: Sessions view test。
- Regression/lint/build: 前端全量、lint/build/format/diff。

## Evidence

- RED: 聚焦测试在实现前因缺少 `All time`、加载数量文案和五列 `colgroup` 失败。
- GREEN: `npx vitest run src/routes/handoff.test.tsx -t "filters, size-sorts"`，1 passed。
- Regression: `npm test`，24 files / 245 tests passed；`npm run lint`、`npm run build`、Prettier 与 `git diff --check` 均通过。
- Changed files: `apps/desktop/src/routes/views.tsx`、`apps/desktop/src/styles.css`、`apps/desktop/src/routes/handoff.test.tsx`。
- Remaining limitation: All time 仍按每页 20 条懒加载；筛选后的总匹配数目前不单独计数，只有不限时间视图展示全账户总数。

## Done

- [x] Test was written first and RED observed, or exception recorded.
- [x] Implementation stays in scope.
- [x] Focused and relevant regression checks pass.
- [x] Status is 验证成功.
