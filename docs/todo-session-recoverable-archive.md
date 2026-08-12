# Todo: Recoverable session archive management

- **Status**: 验证成功（已被直接删除方案替代）
- **Scope**: 展示 Active/Archived 占用，手动归档安全候选，并可无覆盖恢复
- **Baseline**: `agent/20260812-handoff-session-pagination` @ `641cf2523cc0ae7adbb8433975d399af9910706b`

## Why

旧 session 长期留在 active sessions 目录，会持续参与 Codex/LAM 扫描。第一阶段需要可审查、可恢复、无永久删除的生命周期管理。

> 后续产品决策取消归档：归档不降低磁盘总占用。该实现的 UI、命令、公开类型和测试已移除，现行行为见 `todo-session-direct-delete.md`。

## Behavior

- **Input**: 当前账号、用户勾选的 eligible active session，或 LAM archived session。
- **Output**: Active/Archived 数量和字节统计；归档移动到 `<codex_home>/archived_sessions/lam/<原相对路径>`；恢复到原路径。
- **Constraints/errors**: 最近 20 条或 7 天内修改的 session 不可归档；路径必须经 canonical/根目录校验；批量操作先完整验证，禁止覆盖；不触碰 auth/config/API secrets；不永久删除。

## Test first

- Normal: 旧且非最近保护集的 session 可归档并恢复，内容和相对路径不变。
- Edge/invalid: 空选择、重复路径、根目录逃逸、目标冲突被拒绝。
- Error/state conflict: 受保护 session 拒绝归档；批量预检失败时不移动任何文件。
- Expected RED: lifecycle DTO/API/命令/UI 尚不存在。

## Implementation

在共享 session service 增加 storage summary、LAM archived 分页、archive/restore 命令；前端 Sessions 页增加 Active/Archived 切换、统计、eligible 多选归档与逐条恢复。永久删除和自动策略明确留待第二阶段。

## Verification

- Focused: Rust lifecycle 集成测试、API/store/view tests。
- Regression/lint/build: `phase1_core`、Handoff/Sessions 回归、Rustfmt、ESLint、build、diff check。

## Evidence

- RED: Rust lifecycle 集成测试最初因 DTO/service 不存在而无法编译；API 测试因缺少命令封装失败；store/view 测试因没有 management actions、统计与归档控件失败。实现后的首次路径测试还暴露了 macOS `/var` 与 `/private/var` canonical 路径差异，并据此修正。
- GREEN: 验证了 eligible session 可归档并按原内容、原相对路径恢复；空选择、重复路径、根目录逃逸、近期/最近保护、目标冲突均被拒绝；批量预检失败不移动文件。前端全量 24 files / 245 tests、Rust `cargo test --no-default-features`（含 `phase1_core` 71/71）、Rustfmt、ESLint、build 全部通过。
- Changed files: Rust session service/DTO/commands/registration 与 `phase1_core.rs`；前端 types/API/session store/Sessions view/styles 及对应测试。
- Remaining limitation: 归档是同磁盘可恢复移动，会减少 active 扫描与常驻列表压力，但不会降低磁盘总占用；自动归档、压缩和永久删除留待明确保留策略的第二阶段。

## Done

- [x] Test was written first and RED observed, or exception recorded.
- [x] Implementation stays in scope.
- [x] Focused and relevant regression checks pass.
- [x] Status is 验证成功.
