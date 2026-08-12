# Todo: Directly delete sessions without ghosts

- **Status**: 验证成功
- **Scope**: 以手动直接删除替代 LAM 归档，并同步清除 Usage SQLite 来源、事件与聚合
- **Baseline**: `agent/20260812-handoff-session-pagination` @ `641cf2523cc0ae7adbb8433975d399af9910706b`

## Why

归档不降低磁盘总占用，不符合目标。直接删除必须确保 Sessions 与 Usage 均不残留幽灵记录，并在数据库失败时恢复原文件。

## Behavior

- **Input**: 当前账号中用户明确勾选并确认的 eligible active session。
- **Output**: 删除 JSONL，清除对应 `source_files`/`usage_events` 并重建受影响 thread/model 聚合，刷新 Sessions 与存储统计。
- **Constraints/errors**: 最近 20 条或 7 天内修改的 session 仍受保护；路径必须位于账号 sessions 根；空选、重复、逃逸拒绝；无自动删除；数据库失败时文件回滚。

## Test first

- Normal: 已索引旧 session 删除后文件、source、events、thread summary 均消失。
- Edge/invalid: 未建立 SQLite 也能删除；空选择、重复、保护项和路径逃逸不产生变更。
- Error/state conflict: SQLite 事务失败时 staged 文件恢复；批量预检失败不删除任何文件。
- Expected RED: 当前只有 archive/restore DTO、命令和 UI，且归档不更新 Usage SQLite。

## Implementation

删除 LAM archive/restore 产品入口，以安全 staging + SQLite 事务 + 最终清除实现直接删除；加入明确不可恢复确认。`session_index.jsonl` 仅作为名称元数据读取，不作为 Sessions 数据源，本任务不改写 Codex 所有的索引文件。

## Verification

- Focused: Rust 删除/回滚/无幽灵集成测试、API/store/view tests。
- Regression/lint/build: Usage 聚合、Sessions/Handoff、前端全量、Rust full、lint/build/format/diff。

## Evidence

- RED: `delete_sessions` 与删除 DTO 不存在；旧实现只移动文件且不更新 SQLite。首次 GREEN 还复现了 macOS `/var`/`/private/var` 路径差异导致 Usage ghost，测试失败后修正为分别保留 canonical 安全路径与数据库原始路径。
- GREEN: 删除会先 staging 文件，再清理 LAM Usage `source_files`/`usage_events` 和受影响聚合，同时清理 Codex `state_5.sqlite` threads/关联表与 `session_index.jsonl`；数据库失败恢复文件，Usage 已提交时重新索引恢复。归档/恢复 UI、命令与公开类型已移除。前端 24 files / 245 tests、Rust 全量、lint/build/format 通过。
- Changed files: session/usage services、commands/main、前端 API/types/store/view/styles 与 Rust/TS/UI tests。
- Remaining limitation: 删除不可恢复；因此保留最近 20 条和最近 7 天保护，并要求明确确认。没有自动清理。

## Done

- [x] Test was written first and RED observed, or exception recorded.
- [x] Implementation stays in scope.
- [x] Focused and relevant regression checks pass.
- [x] Status is 验证成功.
