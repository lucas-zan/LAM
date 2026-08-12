# Todo: Harden session deletion consistency

- **Status**: 验证成功
- **Scope**: 让 session 索引清理失败成为可见失败并补偿恢复文件和派生统计
- **Baseline**: `agent/20260812-handoff-session-pagination` @ `641cf2523cc0ae7adbb8433975d399af9910706b`

## Why

当前 `session_index.jsonl` 清理错误被忽略，可能让删除显示成功但留下索引记录。文件系统与两个 SQLite 无法共享原子事务，因此需要明确顺序和补偿恢复。

## Behavior

- **Input**: 一组通过保留策略校验的 session 路径。
- **Output**: 文件、Codex SQLite、LAM usage SQLite 和 session index 全部更新后才返回成功。
- **Constraints/errors**: session index 或 SQLite 清理失败必须返回错误并尽力恢复 session 文件、索引及 usage 派生数据；不得静默成功。

## Test first

- Normal: 成功删除后四类数据均无幽灵记录。
- Error/state conflict: session index 不可更新时返回错误且 session 文件保留。
- Expected RED: 当前索引清理结果被 `_ =` 忽略。

## Implementation

删除前保存 index 原文；暂存文件后先更新 index，再清理 usage 和 Codex SQLite；任一步失败均执行带错误报告的补偿恢复。

## Verification

- Focused: Rust session deletion integration tests。
- Regression/lint/build: Rust 全量测试与前端受影响回归。

## Evidence

- RED: 代码审查确认 `remove_session_index_entries` 返回值被显式丢弃，索引失败仍会返回删除成功。
- GREEN: `cargo test --test phase1_core delete`，4 passed；Codex DB 故障测试新增 index 与 usage 恢复断言。
- Regression: `cargo test` 全量通过；前端 246 tests、lint、build、Rustfmt、Prettier 与 diff check 通过。
- Changed files: `src-tauri/src/services/session.rs`、`src-tauri/tests/phase1_core.rs`、删除确认文案、Usage 缓存失效逻辑与相关测试。
- Remaining limitation: 文件系统与两个 SQLite 无法组成单一 ACID 事务；当前覆盖可捕获错误的补偿恢复，进程在步骤之间被强制终止仍需未来增加持久化恢复日志才能做到 crash-safe。

## Done

- [x] Test was written first and RED observed, or exception recorded.
- [x] Implementation stays in scope.
- [x] Focused and relevant regression checks pass.
- [x] Status is 验证成功.
