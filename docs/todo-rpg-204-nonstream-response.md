# RPG-204: Non-stream response conversion

- **Status**: 验证成功
- **Depends on**: RPG-203

## Logic design

- 单 choice Chat response 转为完整 typed `ResponsesResponse`，不伪装成 event collection。
- 使用注入 factory 生成稳定 response/item/call ID 与时间。
- 明确处理 text、empty、null、length、content_filter、tool-only；拒绝 zero/multiple choice 与未知 finish reason。

## Test and acceptance

- [x] 先写 non-stream 红测；首次运行因 `adapters::nonstream` 不存在而按预期失败（E0432）。
- [x] 覆盖成功与全部 terminal/empty/tool-only 分支。
- [x] malformed/choice/model/finish reason 错误稳定。
- [x] 固定 factory 重复转换得到同一 semantic JSON；focused test 4 passed。

## Done criteria

红测和 focused regression 均通过后为 `验证成功`。
