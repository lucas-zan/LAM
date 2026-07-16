# RPG-203: Responses request translation

- **Status**: 验证成功
- **Depends on**: RPG-202

## Logic design

- 将 string input、system/developer/user、assistant history 转为 Chat messages，保持顺序。
- model 只能来自已验证 binding；body model 不得切换 provider/host/model。
- 支持 stream、usage stream option、max output、text format、function tool seam；`previous_response_id` 只接受 null。
- image/audio/file/hosted/MCP/computer/未知 input 在产生 upstream request 前失败。
- 所有 provider 差异由 typed compatibility policy 控制，generic 路径不比较 vendor 名称。

## Test and acceptance

- [x] 先写 request mapping 红测；首次运行因 `adapters::request` 不存在而按预期失败（E0432）。
- [x] 正常 string/multi-message/history 与 authoritative model 映射通过。
- [x] model mismatch、role/order、previous response、unsupported item/tool/parameter 返回稳定错误。
- [x] 每个 schema 字段有映射或显式拒绝；focused test 4 passed。

## Done criteria

红测、focused test 与无 vendor branch 审计通过后为 `验证成功`。
