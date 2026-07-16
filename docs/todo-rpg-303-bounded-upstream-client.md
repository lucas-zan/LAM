# RPG-303: Bounded upstream client

- **Status**: 验证成功
- **Depends on**: RPG-103, RPG-302
- **Scope**: 唯一 HTTP transport、controlled URL join、credential injection、DNS/redirect policy、deadline/cancel/retry/concurrency。

## Logic design

- 只由 approved immutable Provider snapshot 构造 scheme/authority/prefix/path；body/model/header 不能改变目的地。
- request 发送前即时解析 env/Keychain credential，支持 bearer/named header/none；secret 不进入 debug/error/log。
- 禁止 proxy env 与 redirect；只允许 HTTPS public hostname，并拒绝 IP literal、userinfo、fragment、private/special DNS address。
- connect/first-byte/idle/total deadline 和全局/per-binding semaphore；只有可证明未 delivery 且未收 byte 的 connect failure 最多一次底层 retry，429/5xx 不 replay。

## Test and acceptance

- [x] red test：首次运行因 `gateway::upstream` 不存在而按预期失败（E0432/E0433）。
- [x] bearer/named-header/none、prefix-preserving join、path injection、redirect disabled、production HTTPS/public-target validation。
- [x] DNS policy 返回的 address 被 pin 到 reqwest；proxy env 禁用；connect/first-byte/idle/total/response/concurrency bounds 与 cancellation。
- [x] 429/5xx 不重放；只有 `is_connect` 且未取得 response 的路径恰好重试一次，测试证明 attempts=2。
- [x] `provider_gateway_upstream` 6 passed，Phase3 focused 累计 18 passed；new-code clippy、Rust fmt、diff check 通过。

## STOP conditions

- 无法固定 validated DNS address/TLS hostname，或需要应用层重放 generation request。
