# RPG-301: Gateway binding credentials and token lifecycle

- **Status**: 验证成功
- **Depends on**: G3, RPG-104, RPG-108（均验证成功）
- **Scope**: Gateway binding 持久化、profile 独立 bearer、Keychain reference、轮换/撤销和 request-start snapshot；不启动 HTTP server。

## Logic design

- 新增 versioned `GatewayBindingStore`；只保存 binding/profile/provider/model、provider snapshot、token SHA-256、credential reference、generation、created/revoked/revocation-pending metadata 和 schema revision。
- token 使用 OS CSPRNG 生成，明文只进入 credential backend；验证先解析严格 bearer 格式，再对固定长度 digest 做 constant-time compare。
- prepare 与 RPG-107 transaction 对接；rotation 先写新 credential/binding，提交后撤销旧 generation；detach 后新请求立即失败，已认证请求持有 immutable snapshot。
- auth helper 只通过 binding reference 读取 token，空/未知/revoked/cross-profile 均返回稳定脱敏错误。

## Test and acceptance

- [x] 先新增 focused tests；首次运行因 `localagentmanager_core::gateway` 不存在而按预期失败（E0433）。
- [x] create/authenticate/rotate/revoke/detach、wrong/empty/malformed/cross-profile token。
- [x] transaction candidate 以 operation id 幂等；新旧 token 仅在 commit 前并存，request snapshot 在轮换后保持 immutable。
- [x] store/debug/error 不出现 plaintext token；credential 删除失败先撤销 hash，再持久化 `revocation_pending`。
- [x] `provider_gateway_binding` 7 passed；`provider_attach_transaction` 19 passed；lib clippy（仅 allow 已记录的无关 baseline）、Rust fmt 与 diff check 通过。

## STOP conditions

- 需要 plaintext file fallback，或无法保证 detach 后旧 token 立即失效。
