# RPG-305: Sidecar state, stable port and supervisor

- **Status**: 验证成功
- **Depends on**: RPG-302, RPG-304
- **Scope**: versioned state、stable port、single instance、authenticated private control、bounded supervisor lifecycle。

## Logic design

- state 保存 install/instance/process/component/protocol/schema/port；installation `flock` 持有 start/mutation 临界区，端口不静默变化。
- Unix control socket 校验 same uid，并用 install identity key 对 challenge/nonce/canonical message 做 HMAC；64 KiB 上限与 monotonic nonce。
- readiness 同时验证 package identity、control proof/state fields 和 bearer `/healthz`。
- supervisor 使用 bounded exponential backoff+jitter；bindings 或 inflight 存在时 UI close 不停止；仅无 bindings/inflight 且 grace 到期 idle exit。

## Test and acceptance

- [x] red test：首次运行因 `gateway::sidecar` 不存在而按预期失败（E0432）。
- [x] versioned install/instance/port/process/component/protocol/schema state；CAS + installation flock 证明 concurrent claim 仅一个成功，stale recovery 必须显式确认。
- [x] 真实 Unix control socket 为 0600，校验双向 peer uid、HMAC、version、64 KiB、length-prefix 与 monotonic nonce/replay。
- [x] bounded exponential restart+jitter 与 binding/inflight-aware idle policy；UI close 不构成 shutdown 条件。
- [x] stable port 首次固定且不静默改变；显式 Tauri plan/execute API 对全部 Gateway profile
      预检 config hash，按 fingerprint/revision 提交，第二个配置前故障会反向恢复已写配置且
      binding/state 保持原值。
- [x] control socket basename 使用 install-id SHA-256 前 16 hex，避免 Darwin per-user temp path
      与完整 UUID 组合超过 AF_UNIX 路径上限。
- [x] `provider_sidecar_production` 4/4、`provider_gateway_sidecar` 7/7；全量 Rust、
      clippy/fmt/diff 通过。

## STOP conditions

- control 使用 unauthenticated TCP/HTTP，或依赖 PID metadata 强制解锁。
