# Todo: Query and sort sessions

- **Status**: 验证成功
- **Scope**: Sessions 页面可按时间范围查询，并按文件大小升序或降序进行服务端全量稳定分页排序
- **Baseline**: `agent/20260812-handoff-session-pagination` @ `641cf2523cc0ae7adbb8433975d399af9910706b`

## Why

用户需要按时间范围定位 session，并优先识别占用空间最大的记录。筛选和排序必须发生在后端完整数据集上，不能只处理当前已加载页面。

## Behavior

- **Input**: 全部、近 7 天、近 30 天、30 天前三种时间范围，newest、largest、smallest 三种排序及分页 cursor。
- **Output**: 先按时间范围筛选，再按指定顺序稳定返回；大小相同以路径确定顺序；切换筛选或排序重置列表和 cursor。
- **Constraints/errors**: cursor 必须携带对应排序键；已有默认调用保持 newest 行为。

## Test first

- Normal: 四个时间 Tab 边界正确；largest/smallest 跨页顺序正确；UI 切换会用新查询重新请求第一页。
- Edge/invalid: 近 30 天包含近 7 天，恰好 7/30 天采用一致边界；相同大小结果稳定。
- Error/state conflict: 旧筛选/排序迟到响应不能覆盖新查询。
- Expected RED: page request/cursor/store/UI 均没有 filter/sort 契约。

## Implementation

扩展共享分页 DTO、后端时间筛选、排序和 cursor 比较，增加 Sessions 时间 Tab、排序控件与 store generation 隔离。不改变 Handoff 默认最近优先行为。

## Verification

- Focused: Rust session pagination、API/store/view tests。
- Regression/lint/build: Handoff、前端全量、Rust phase1/full、lint/build/format/diff。

## Evidence

- RED: `phase1_core` 无法导入 `query_sessions_page`、`SessionQueryRequest`、`SessionSort` 与 `SessionAgeFilter`；store/view/API 也没有查询契约。
- GREEN: 全部、近 7 天、近 30 天、30 天前均由后端完整数据集筛选；newest/largest/smallest 使用稳定 cursor 跨页；切换条件重置 generation 与第一页。前端 24 files / 245 tests、Rust 全量、lint/build/format 通过。
- Changed files: session service/DTO/commands，前端 API/types/session store/Sessions view 及对应测试。
- Remaining limitation: 文本搜索仍只作用于已加载分页；时间和大小查询是服务端全量。

## Done

- [x] Test was written first and RED observed, or exception recorded.
- [x] Implementation stays in scope.
- [x] Focused and relevant regression checks pass.
- [x] Status is 验证成功.
