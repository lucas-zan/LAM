# RPG-302: Authenticated loopback server

- **Status**: 验证成功
- **Depends on**: RPG-301
- **Scope**: IPv4 loopback composition root、bearer middleware、limits、sanitized request id/error 和可验证 health；业务 route 暂用 fake handler。

## Logic design

- listener 只接受 `127.0.0.1`；CORS 不启用，unknown route/method 返回稳定 JSON error。
- `/healthz` 返回 protocol/component/schema/installation/instance/readiness，验证方必须同时校验私有 control proof，不能只信占用端口的 HTTP 响应。
- `/v1/models` 与 `/v1/responses` 在读取 body 前完成严格 bearer/binding lookup；auth、limit、request id、handler 分层。
- 限制 header/body/concurrency/queue/timeout，日志只记录 allowlist metadata。

## Test and acceptance

- [x] red test：首次运行因 `gateway::server` 不存在而按预期失败（E0432）。
- [x] 真实 TCP listener 仅允许 `127.0.0.1`；bearer 在 body read 前执行，body/concurrency/deadline 有稳定错误。
- [x] `/healthz` 要求 bearer 与合法 nonce；256-bit identity key HMAC proof 可校验并拒绝 tampered/foreign instance。
- [x] fake binding/handler 全链路运行；request id allowlist、无 CORS、404/405 JSON 契约、metadata-only observer。
- [x] `provider_gateway_server` 5 passed、RPG-301 7 passed；new-code clippy、Rust fmt、diff check 通过。

## STOP conditions

- readiness 只能依赖 `/healthz`，或 server 在鉴权前读取 request body。
