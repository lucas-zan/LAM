# RPG-206: Function tool-call round trips

- **Status**: 验证成功
- **Depends on**: RPG-204, RPG-205

## Logic design

- Responses function definition/choice/parallel → Chat tools。
- non-stream/stream Chat tool call → Responses function-call item，保持 call_id。
- `function_call_output` → `role=tool` + matching `tool_call_id`；不执行工具。
- 校验 tool name、schema/arguments size、JSON arguments、call/result linkage 与重复 ID。

## Test and acceptance

- [x] 先写 tool loop 红测；首次运行因 `ChatToolChoice` 不存在而按预期失败（E0433）。
- [x] 单/多调用、tool-only、任意 arguments fragments、interleaved indexes 均通过。
- [x] missing/duplicate/unknown/result-before-call/malformed/unsupported type 均前置失败，tool schema/arguments 使用 1 MiB 边界。
- [x] non-stream 与 stream 完整 synthetic round trip 通过；focused test 3 passed。

## Done criteria

红测和完整工具循环回归通过后为 `验证成功`。
