# RPG-201: Controlled protocol schemas

- **Status**: 验证成功
- **Depends on**: G2, RPG-002（均验证成功）
- **Scope**: 仅在纯 adapter 边界定义本项目验证过的 Responses 与 Chat Completions 子集；不引入 HTTP、credential、Gateway 或 UI。

## Logic design

- 分离 `responses` 与 `chat_completions` 类型，不构造巨型 vendor-neutral DTO。
- 对 request/message/item/tool/usage/error/stream chunk 建立强类型；未知字段按明确矩阵拒绝。
- 任意 JSON Schema 只停留在协议边界并受序列化大小限制，不流入 Provider/domain 服务。
- 所有边界校验返回稳定 code 与 field path；限制 request 32 MiB、SSE frame 256 KiB、tool arguments 1 MiB。

## Test and acceptance

- [x] 先新增 schema contract 测试；首次运行因 `localagentmanager_core::adapters` 不存在而按预期失败（E0433）。
- [x] RPG-002 text/tool/follow-up/resume 请求可解析，`previous_response_id = null` 保持可区分。
- [x] 合成 Chat response/chunk/tool/error 可往返；错误类型、未知字段、超限值给出稳定路径。
- [x] `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_adapter_schema`：4 passed。

## Done criteria

只有红测已记录、真实类型与校验完成、focused test 通过时才能改为 `验证成功`。
