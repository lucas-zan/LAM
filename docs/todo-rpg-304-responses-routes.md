# RPG-304: Compose Responses routes

- **Status**: 验证成功
- **Depends on**: RPG-208, RPG-303
- **Scope**: `/v1/models`、`/v1/responses` 的 non-stream/stream composition；不增加 RPG-002 未证明的 state/cancel route。

## Logic design

- Provider/model 只来自 authenticated immutable binding；先做 schema/size/capability validation，再解析 upstream credential。
- Responses binding 在显式 `route_via_gateway` 时受控 passthrough；Chat Completions binding 通过 Phase2 registry/adapter exchange 转换。
- non-stream 返回 typed Responses JSON；stream 保持严格 SSE framing/order/terminal semantics，disconnect 取消 upstream/adapter。
- unsupported field/tool/`previous_response_id` 在无 upstream traffic 时返回稳定 Responses error。

## Test and acceptance

- [x] red test：首次运行因 `gateway::routes` 不存在而按预期失败（E0432）。
- [x] `/v1/models` 返回 authenticated Provider allowlist 的全部模型，并生成严格 Codex catalog；non-stream text 与 function tool/follow-up 经真实 wiremock transport 转换。
- [x] streaming 通过 reqwest chunk + Phase2 bounded SSE state machine + bounded channel 实时输出 Responses events；receiver drop 会取消 upstream。
- [x] model switch、response store、malformed SSE 在 upstream 前或单一 failed terminal event 失败；immutable binding snapshot 控制 Provider/model/auth。
- [x] Responses JSON/SSE content type、event order、no-store、sanitized errors；Phase3 focused 24 passed，new-code clippy/fmt/diff 通过。

## STOP conditions

- 需要 response store，或 Codex 契约以外的 route 才能通过测试。
