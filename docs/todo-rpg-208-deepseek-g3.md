# RPG-208: DeepSeek thinking compatibility and G3

- **Status**: 验证成功
- **Depends on**: RPG-206, RPG-207

## Logic design

- 以 typed preset 表达 DeepSeek `/chat/completions`、`thinking.type`、effort mapping 与限制；generic adapter 不含 vendor 判断。
- `reasoning_content` 与 `content` 分离；不把 raw chain-of-thought 输出给 Codex Responses。
- assistant tool-call turn 的完整 reasoning 必须在后续 history 回放；非 tool turn 可按 policy 省略。
- reasoning buffer 有界，超限为 `ADAPTER_REASONING_LIMIT_EXCEEDED`；不制造 signature/encrypted reasoning。

## Test and gate acceptance

- [x] 先写官方文档语义对应的脱敏 fixture 与红测；首次运行因 `adapters::deepseek` 与 reasoning capture API 不存在而按预期失败（E0432/E0599）。
- [x] thinking on/off、high/max 及 low/medium/xhigh 兼容映射、stream/non-stream reasoning 分离通过。
- [x] tool→result→tool history 的完整 reasoning replay 通过；缺失时 preflight 失败，1 MiB 超限为 `ADAPTER_REASONING_LIMIT_EXCEEDED`。
- [x] generic tests 证明无 DeepSeek policy 泄漏；raw reasoning 不进入 Responses JSON/event。
- [x] Phase 2 focused 35 tests 与 G3 manifest/checksum gate 通过；全 Rust 271 passed、2 ignored，前端 164 passed，format/lint/build/Phase 0 fixture gate/diff 检查通过。

## Verification notes

- `cargo clippy --all-targets -- -D warnings` 仍会命中 Phase 2 外既有的 `antigravity.rs`、`quota.rs`、`relay.rs`、Phase 1 test 与 `commands/mod.rs` 共五类 lint；保持这些无关文件不变。
- 在显式 allow 上述既有 lint 后，`cargo clippy --all-targets -- -D warnings ...` 通过，新增 adapter 无 warning。
- 未创建 commit，未修改 weekly quota 悬浮框 UI。

## Done criteria

只有完整离线 gate 与仓库相关回归全绿后才能为 `验证成功`，禁止占位 adapter 或 skipped gate。
