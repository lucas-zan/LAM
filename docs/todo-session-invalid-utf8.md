# Todo: Session listing tolerates non-UTF-8 artifacts

- **Status**: 验证成功
- **Scope**: Sessions/Handoff 扫描不因 sessions 目录中的二进制杂项或损坏 UTF-8 首行整体失败
- **Baseline**: `agent/20260812-handoff-session-pagination` @ `641cf2523cc0ae7adbb8433975d399af9910706b`

## Why

实际 `main` 目录中存在 3 个 `.DS_Store`，扫描器把它们作为 session 读取，严格 UTF-8 首行解码返回 `stream did not contain valid UTF-8`，导致整个 Sessions 页面为空。

## Behavior

- **Input**: sessions 目录含合法 `.jsonl`/兼容 `.json`、非 session 二进制文件，以及首行含异常字节的 `.jsonl`。
- **Output**: 非 JSON/JSONL 文件不进入 session 列表；异常 UTF-8 JSONL 使用有损解码和文件名 fallback，不中断其他 session。
- **Constraints/errors**: 保留既有空/不完整 JSONL 的兼容行为；不修改用户 session 文件。

## Test first

- Normal: 合法 JSONL 继续被解析。
- Edge/invalid: `.DS_Store` 被忽略；异常 UTF-8 JSONL 不导致列表调用失败。
- Error/state conflict: 单个异常文件不能阻断整个账号的 Sessions/Handoff 加载。
- Expected RED: 当前扫描所有文件，`read_line` 对异常字节返回 UTF-8 错误。

## Implementation

在共享 session 文件枚举层只接受 `.jsonl` 和既有兼容 `.json`，并将首行读取改为字节读取后 `from_utf8_lossy`。分页与完整列表自动共享修复。

## Verification

- Focused: Rust session 集成测试。
- Regression/lint/build: `phase1_core`、Rustfmt、桌面 build、diff check。

## Evidence

- RED: 两个聚焦测试均以 `AppError { code: "IO_ERROR", message: "stream did not contain valid UTF-8" }` 失败，复现页面错误。
- GREEN: 聚焦测试 2/2；`phase1_core` 67/67；Rustfmt、桌面 build、`git diff --check` 通过。实际 main 诊断确认 `session_index.jsonl` 合法、554 个 JSONL 首行合法，错误源为 sessions 目录中的 `.DS_Store`。
- Changed files: session 文件枚举只接受 `.jsonl`/兼容 `.json`；首行按字节读取并使用 UTF-8 lossy decode；新增二进制杂项和损坏 UTF-8 回归测试。
- Remaining limitation: 损坏 JSONL 会以文件名作为 fallback session ID 展示，但不会阻断其余列表；文件内容不会被修改。

## Done

- [x] Test was written first and RED observed, or exception recorded.
- [x] Implementation stays in scope.
- [x] Focused and relevant regression checks pass.
- [x] Status is 验证成功.
