# Todo: Paginate every active-session consumer

- **Status**: 验证成功
- **Scope**: Sessions 页面按最近优先分页，Active source/托盘只读取最近一条，消除完整列表常驻内存
- **Baseline**: `agent/20260812-handoff-session-pagination` @ `641cf2523cc0ae7adbb8433975d399af9910706b`

## Why

Handoff 已分页，但 Sessions store、账号 Active source 和托盘仍调用完整列表 API。`main` 有数百个 session 时仍会解析并保留全部元数据。

## Behavior

- **Input**: 账号切换、页面首次加载、用户请求加载更早 session。
- **Output**: 首屏最近 20 条；继续加载按稳定 cursor 追加 20 条；Active source/托盘每账号只请求最近 1 条。
- **Constraints/errors**: 账号切换清空旧 cursor/列表；迟到响应不能覆盖新账号；搜索只作用于已加载记录并明确标注。

## Test first

- Normal: 首屏/下一页请求 limit 20 并去重追加。
- Edge/invalid: 无 cursor 时隐藏继续加载；切换账号重置状态。
- Error/state conflict: 旧账号迟到响应被忽略；结构化错误可读。
- Expected RED: store 仍调用 `listSessions`，没有 cursor/hasMore/loadMore 状态。

## Implementation

扩展 session store 的分页状态与动作，Sessions UI 增加“加载更早”，并把仅需 latest 的调用改为 `listSessionsPage(limit: 1)`。保留旧 API 供兼容测试，不再用于常规运行路径。

## Verification

- Focused: session store、Sessions view、account store、tray tests。
- Regression/lint/build: Handoff 回归、前端全量相关测试、lint/build、diff check。

## Evidence

- RED: session store 测试确认旧实现仍调用 `listSessions`，且缺少 cursor、`hasMore` 与 `loadMoreSessions`；Sessions view、account store、tray 的分页契约测试分别因缺少加载按钮或未调用 `listSessionsPage` 失败。
- GREEN: Sessions 首屏与后续页按 20 条加载并去重追加；账号切换使用 generation 丢弃迟到响应；Active source 与托盘按 `limit: 1` 获取最近 session。前端全量 24 files / 245 tests、ESLint、TypeScript/Vite build 均通过。
- Changed files: `src/stores/sessions.ts`、`src/routes/views.tsx`、`src/App.tsx`、`src/stores/accounts.ts`、`src/components/tray-quota-panel.tsx` 及对应测试。
- Remaining limitation: 搜索只过滤当前已加载的分页数据，输入框已明确标注 `Search loaded sessions`；如需全库搜索，应另建后端索引查询能力。

## Done

- [x] Test was written first and RED observed, or exception recorded.
- [x] Implementation stays in scope.
- [x] Focused and relevant regression checks pass.
- [x] Status is 验证成功.
