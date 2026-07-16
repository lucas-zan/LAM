# RPG-205: Bounded SSE conversion state machine

- **Status**: 验证成功
- **Depends on**: RPG-203

## Logic design

- SSE framing 与 chunk conversion 分层；状态为 created → item/part → delta → done → terminal。
- response/item/call ID 全程稳定；UTF-8、role-only、usage-only、`[DONE]` 显式处理。
- frame 256 KiB、tool arguments 1 MiB、wire response 32 MiB、event channel 64/约 2 MiB。
- malformed/overflow/drop/missing DONE/duplicate terminal/cancel 均最多产生一个终态，终态后拒绝事件。

## Test and acceptance

- [x] 先写 framer/state-machine 红测；首次运行因 `adapters::sse` 不存在而按预期失败（E0432）。
- [x] text fixture event order；任意 byte partition 与 UTF-8 分割语义一致。
- [x] 覆盖 malformed JSON/field、limit、drop、missing DONE、backpressure、所有合法取消状态。
- [x] named constants 受 limit tests 约束，无无界 accumulator；focused test 4 passed。

## Done criteria

红测、property-style partition、故障路径和 focused regression 通过后为 `验证成功`。
