# Todo: Phase 4 Account-first 最终产品交付

> Executor instructions: Follow this todo in dependency order. For every task,
> add the tests described below first, confirm the expected red state, implement,
> run focused acceptance, and update the task immediately. The product is not
> releasable until every task is `验证成功` and G5 passes.

## Status

- **Priority**: P0
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: G4 / Phase 0–3 verified
- **Category**: feature/refactor/security/release
- **Planned at**: branch `codex/202607071457-ui-optimize-20260710102915-remote-provider-gateway`, commit `19b1527`

## Why this matters

**Background**: 用户确认 LAM 的产品不变量是 `一个 Account = 一个 Profile = 一个
CODEX_HOME`。现有 Provider-first 页面要求用户先创建 Provider，再从已有 Profile 中选择
binding，暴露了内部连接资源，且模型切换被误导为需要新建 Profile。

**Current state**: Phase 0–3 已提供真实 Provider V2、credential、binding、非破坏 config
投影、Gateway、launcher 和 packaged G4；Add Account 只创建 CLI/PAT profile，Provider Center
独立执行 create/attach。删除已绑定账号会被 lifecycle guard 阻止。readiness/health 已有基础，
但 capability evidence、Account-first 原子编排、provider-aware relay 与 G5 尚未完成。

**Impact**: 普通用户无法以“命名 API 账号”的方式一次完成创建；失败可能留下孤立资源；
API account 删除、模型切换、resume/relay 和诊断不构成完整产品闭环。

**What improves**: Add Account 成为主入口；Provider 退为可复用的高级连接资源；创建、模型
切换、删除均有 plan/fingerprint/事务语义；session/relay 在写入前验证兼容性；最终 app/DMG
通过可重复的 G5 门禁。

## Scope

**In scope**:

- RPG-401～406；Account-first API/UI；capability evidence；模型切换；API account 删除。
- Provider-aware session/resume/relay analyzer 与 no-partial-write 执行边界。
- smoke、安全、迁移、文档、真实 packaged acceptance 和最终 `.app`/DMG。

**Out of scope**:

- 修改用户现有 weekly/tray 悬浮框视觉实现。
- Phase 0 未锁定的 Responses retrieve/cancel/store、hosted/MCP/computer/image/audio/file 适配。
- 绕过 Codex 0.144.1 history contract；DeepSeek thinking+tools 继续 fail closed。

## Design

- 产品实体：Account/Profile/CODEX_HOME 一一对应；Provider 是 endpoint/auth/protocol 连接资源；
  Binding 是账号当前连接和默认模型。
- 默认 Add API Account 创建账号专属 Provider；高级选项可复用已有 Provider。任何 secret 只进
  Keychain/受控 reference，不进入 DTO、plan、DOM、config、journal 或日志。
- 所有跨 account/provider/config/binding 操作使用 plan fingerprint、revision/CAS 和恢复 journal；
  失败与启动恢复不得留下半创建资源。
- 模型切换是同一 profile 的 rebind/config transaction，不新建 CODEX_HOME。
- capability 分离 declared、adapter-supported、verified evidence、current health；health 不覆写能力。
- relay analyzer 为纯函数，输出 compatible / compatible_with_loss / blocked；blocked 时执行器零写入。
- Providers 页面保留高级诊断与复用，Accounts 页面承担创建和日常模型切换。

## Task overview

| ID | Task | Acceptance summary | Status |
| --- | --- | --- | --- |
| P4-1 / RPG-401 | readiness、capability 与 evidence | 可解释 blocker/provenance/expiry，DTO 脱敏 | 验证成功 |
| P4-2 / RPG-402 | Account-first 原子生命周期与 UI | Add API Account、switch model、delete 全闭环 | 验证成功 |
| P4-3 / RPG-403 | 纯 RelayCompatibilityAnalyzer | 所有受控 item 确定分类，blocked 零写入 | 验证成功 |
| P4-4 / RPG-404 | session/resume/sync/relay 集成 | API profile 全入口走 planner/analyzer | 验证成功 |
| P4-5 / RPG-405 | release 质量与文档 | smoke/security/migration/metrics/docs 全绿 | 验证成功 |
| P4-6 / RPG-406 | G5 与最终发布验收 | 真实 `.app`/DMG 和 Account-first E2E 通过 | 验证成功 |

## P4-1 / RPG-401: readiness、capability 与 conformance evidence

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: 账号创建和 relay 必须基于可解释且不会被瞬时网络状态污染的能力结论。

**What to do**:
- 增加 capability/evidence DTO、verification key/expiry 和 provenance resolver。
- 聚合 credential、adapter、Gateway、drift、health freshness blocker，但保持 declared capability 独立。

**Logic design**:
- verification key 覆盖 provider/model/endpoint/adapter/policy/suite version。
- user override 只能收紧，不能升级 adapter 不可能能力；evidence 不含 body/reasoning/secret。

**Test design**:
- 先增加 resolver 编译/行为测试并确认缺类型失败。
- 覆盖多 blocker、health transition、key 任一字段变化失效、expiry、override fail closed、序列化脱敏。

**Acceptance**:
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_phase4_capability`

**Done criteria**:
- [x] tests-first red state 已记录：`provider_phase4_capability` 因
      `provider_capability` 模块不存在产生预期 E0432
- [x] focused tests 3/3 与 Provider API 12/12 通过
- [x] DTO/API 只暴露 endpoint hash/evidence reference；capability 与 health 分离
- [x] overview/status 更新为 `验证成功`

## P4-2 / RPG-402: Account-first 原子生命周期与 UI

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: 普通用户应创建 API Account，而不是手工拼 Provider + Profile + Binding。

**What to do**:
- 新增 plan/execute Add API Account API，默认专属 Provider，可显式复用 Provider。
- 新增同 profile 模型切换 plan/execute；API account 删除先安全 detach，再清理专属 Provider/account。
- Add Account modal 增加 API Account，成功后直接选中新账号；Provider Center 标记高级连接管理。

**Logic design**:
- 创建 journal 串联 account directory/wrapper、Provider store、Keychain version、binding/config；启动恢复幂等补偿。
- plan/execute fingerprint 与 revisions 防 stale；失败清理所有本次创建资源，不触碰既有 Provider/Profile。
- env/auth-command 不收 secret；Keychain secret 只出现在 execute 入参并立即写入 Keychain。

**Test design**:
- 先写 production service 与 UI tests，确认命令/组件缺失失败。
- 新建专属 Provider、复用 Provider、模型切换保持 CODEX_HOME、重复执行、stale、各阶段 fault rollback、删除清理、secret scan。

**Acceptance**:
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_phase4_api_account`
- `npm --prefix apps/desktop test -- --run src/components/api-account-flow.test.tsx`

**Done criteria**:
- [x] tests-first red state 已记录：`provider_phase4_api_account` 因 Account-first DTO/service
      尚不存在产生预期 E0432
- [x] service/fault/UI tests 通过：Rust 7/7，API Account UI/store focused 8/8
- [x] 一个 Account 严格对应一个 Profile/CODEX_HOME；切模保持 home，创建/删除 crash recovery 幂等
- [x] overview/status 更新为 `验证成功`

## P4-3 / RPG-403: 纯 RelayCompatibilityAnalyzer

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Provider/model 不同的 session 不能在不知道历史是否可表示时直接复制并运行。

**What to do**:
- 实现无 credential、无 IO 的 analyzer 和 typed report。
- text、完成的 verified function pair 可兼容；unsupported/partial/stateful item fail closed。

**Logic design**:
- 输出 compatible、compatible_with_loss 或 blocked，列出 item id/type、转换、warning、确认要求。
- unknown/corrupt/encrypted reasoning/unfinished tool/previous-response/unsupported media 均 blocked。

**Test design**:
- 先写 analyzer tests 确认模块缺失失败；覆盖正常 text/tool、每类 blocked、组合、loss confirmation、secret-free report。

**Acceptance**:
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_phase4_relay_analyzer`

**Done criteria**:
- [x] tests-first red state 已记录：缺 `provider_relay_compatibility` 模块产生预期 E0432
- [x] 全分类 fixture 5/5 通过
- [x] analyzer 纯函数且不读取 credential/payload
- [x] overview/status 更新为 `验证成功`

## P4-4 / RPG-404: session、resume、sync 与 relay 集成

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: API Account 必须在所有产品入口保持 launcher 恢复、目标认证隔离和 relay 零部分写入。

**What to do**:
- session view 使用 original/current Provider/model；resume 统一走 CodexLaunchPlanner。
- relay 写入前调用 analyzer；loss 必须显式确认；sync/relay 拒绝 credential/private Provider state。

**Logic design**:
- target runtime 的 Provider/model/auth/billing 来自 target binding；不复制 auth/config ownership/journal/Gateway log。
- analyzer ticket/fingerprint 绑定 source session hash 与 target capability evidence。

**Test design**:
- 先扩展 relay integration tests 确认缺 analyzer ticket 失败。
- ChatGPT↔Direct/Gateway、Gateway↔Gateway、blocked/no-write、loss confirmation、stopped Gateway resume、secret manifest scan。

**Acceptance**:
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_phase4_relay_integration`
- 既有 relay/session/launch planner suites 全绿。

**Done criteria**:
- [x] tests-first red state 已记录：integration test 因缺 confirmation/report DTO 字段产生预期 E0560/E0609
- [x] provider mismatch text 与 encrypted-reasoning blocked/no-write 集成 2/2；legacy relay/session 53/53
- [x] resume/relay 统一调用 CodexLaunchPlanner；Gateway entry 由 launcher 恢复 sidecar
- [x] overview/status 更新为 `验证成功`

## P4-5 / RPG-405: smoke、安全、迁移、可观测性与文档

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: 最终产品需要防回归证据而非局部绿色测试。

**What to do**:
- Account-first UI smoke、legacy migration、structured metrics、security/secret/path/concurrency suites。
- 更新 README、架构、命令/API、troubleshooting、privacy/security、release notes 与总 tracker。

**Logic design**:
- metrics 只含 route/status/latency/usage/retry/hash identity，不含 body/tool/reasoning/token。
- migration 幂等、冲突 fail closed；旧 Provider-first 数据继续可读。

**Test design**:
- 先扩展 static/runtime gates 使缺 Account-first wiring/evidence/relay/metrics 时失败。
- fixture checksum/secret marker、重复 migration、并发/stale、UI DOM/store secret scan。

**Acceptance**:
- Phase 0 gate、UI smoke、security/migration/observability suites、docs checker 全绿。

**Done criteria**:
- [x] release static gate/manifest 的缺失由初始 source inventory 确认；新增不完整 manifest 负向测试
- [x] smoke/security/migration/metrics/docs 通过；metrics 为 allowlist 且 G4 验证 usage/retry
- [x] weekly/tray UI 实现未改（`git diff --exit-code -- apps/desktop/src/components/tray-quota-panel.tsx`）
- [x] overview/status 更新为 `验证成功`

## P4-6 / RPG-406: G5、人工验收与发布

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: 只有真实用户路径和 packaged artifact 都通过才能声称最终可用。

**What to do**:
- 真实 Add API Account → test → launch → switch model → session/resume → delete E2E。
- 全量 Rust/frontend/lint/format/build/clippy/release/fixture/diff；最终 app/DMG mount verification。

**Logic design**:
- acceptance 使用 synthetic local upstream 和临时 HOME，不要求真实 secret/公网。
- 记录精确版本、命令、计数和已知受控限制。

**Test design**:
- 先创建 G5 verifier/manifest，确认缺场景或 checksum 时失败。
- packaged Account-first process E2E、重启恢复、失败补偿、artifact secret scan。

**Acceptance**:
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --no-fail-fast`
- `npm --prefix apps/desktop test && npm --prefix apps/desktop run lint && npm --prefix apps/desktop run format:check && npm --prefix apps/desktop run build`
- `npm --prefix apps/desktop run test:gateway-phase0 && npm --prefix apps/desktop run test:ui && npm --prefix apps/desktop run tauri:build`

**Done criteria**:
- [x] G5 verifier 先要求完整 scenario/route/source/artifact manifest；缺 scenario 负向测试通过
- [x] G5/full/package gates 全绿：Rust 351 tests（349 passed、2 ignored）、frontend 170 passed
- [x] `.app` codesign strict、DMG CRC/mount 与 SHA-256 `50848749d1813f870d41e5f1ef049a167ee680e30d92810b456f9f0a129eba5d` 通过
- [x] tracker/design/release evidence 精确，overview/status 更新为 `验证成功`

## Test plan

- Normal: 专属/复用 Provider API account、Direct/Gateway、模型切换、resume/relay、删除。
- Edge: 多 blocker、evidence expiry、同 Provider 多 account、重复命令、进程重启、idle/restart。
- Invalid: stale fingerprint/revision、unsupported history、invalid model/protocol/header/path/schema。
- Error: upstream/auth/Gateway/config/Keychain/CAS/fault injection，均零部分写入。
- Security: secret/response/reasoning 不出现在 DTO/DOM/config/journal/log/metric/artifact。

## Verification commands

| Purpose | Command | Expected |
| --- | --- | --- |
| Focused Rust | Phase 4 four focused integration suites | exit 0 |
| Frontend | API account/provider/store/component suites | exit 0 |
| Full | Rust + Vitest + lint + format + build + clippy | exit 0 |
| Contracts | Phase 0 + G5 + fixture checksum/secret scan | exit 0 |
| Package | release `.app` + mounted DMG verification | exit 0 |

## Final done criteria

- [x] Every task has exactly one checked status and it is `验证成功`.
- [x] Task overview shows P4-1～P4-6 `验证成功`.
- [x] G5 is `验证成功`; no unresolved STOP condition.
- [x] Final product follows Account-first semantics and has no known production disconnect in the verified matrix.

## STOP conditions

- Codex contract change would require response store or CoT persistence.
- A secret can reach config/DTO/DOM/journal/log/metric/artifact.
- Atomic lifecycle cannot recover without destructive action against pre-existing user resources.
- A focused task still fails after five repair loops.
- Completion would require modifying the user-owned weekly/tray visual implementation.
