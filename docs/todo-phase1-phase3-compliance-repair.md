# Phase 1–3 生产链路合规修复 Todo

## 背景

Phase 1–3 现有单元与集成测试通过，但复核发现若干测试只验证了模块或字符串，未覆盖
真实生产调用链。主要断点包括：生成的 Codex auth command 与 `lam-auth-helper` CLI 不一致、
launcher 清空环境后无法解析相对 helper 路径、Provider test 未发出 HTTP、binding 生命周期与
事务恢复未接入启动/账户操作、Gateway routes 未通过 adapter registry 和 compatibility profile、
DeepSeek thinking tool history 未在真实路由保留、sidecar supervisor/资源上限只存在于测试对象，
以及 G4 未执行真实 `lam -> sidecar -> fake Codex -> fake upstream` 进程链。

本修复暂停 Phase 4。只有下面所有任务均为 `验证成功`，Phase 1–3 才可重新标记为完成。
用户的 weekly 悬浮框 UI 不在本次改动范围；若修复生产流程需要调整其他 Provider UI/API，允许
做最小必要修改。

## 目标与完成定义

- Codex 0.144.1 锁定契约继续通过，Gateway profile 严格实现 `/v1/models`、
  `/v1/responses`、完整历史、bearer 验证以及 Codex 两类 retry 均为 0。
- Direct 与 Gateway profile 都从真实安装上下文生成可执行配置；任何 auth 模式不得依赖占位参数。
- Phase 1 的 readiness、HTTP health、binding/profile 生命周期和 crash recovery 接入生产入口。
- Phase 2 adapter registry、compatibility policy、DeepSeek tool reasoning history 被 Phase 3 routes 使用。
- sidecar 的稳定端口、单实例、有限重启、idle 退出、资源限制和流式失败语义由生产代码执行。
- G4 启动真实二进制进程并模拟 Codex 行为，不以进程内 handler 调用代替。
- 所有 fixture 脱敏且 checksum 有效；Rust、前端、lint、format、build 和 packaging gate 全绿。

## 里程碑与依赖

```text
M1 运行时/认证闭环
  -> M2 Phase 1 生产服务与生命周期
  -> M3 adapter/DeepSeek 生产接线
  -> M4 sidecar 生命周期与资源边界
  -> M5 真实进程 G4
  -> M6 全量回归与文档复核
```

## M1 — 运行时路径与认证闭环

### FIX-101：统一安装上下文、state root 与 auth helper 契约

**Why**

当前 config 生成相对 `lam-auth-helper` 和 placeholder binding，实际 helper 又要求不同参数；
launcher `env_clear()` 后无 PATH，导致配置无法运行。state root 还存在设计路径与 `~/.config`
双写风险。

**What to do**

- 引入单一的运行时路径/安装清单解析，生成绝对且已校验的 `lam`、sidecar、auth helper 路径。
- Gateway auth config 只能在真实 binding/profile/state root 已知后物化，不持久化 placeholder。
- helper 实现 Gateway、Keychain、approved auth-command 三种生产模式；stdout 仅 token，stderr 脱敏。
- 统一设计规定的 Provider Hub root；对已有 V2 legacy root 做显式、幂等、冲突安全的兼容迁移。
- 保留 Codex auth table 与 `env_key`/其他认证字段互斥，并将 Gateway retry 显式写为 0。

**Logic design**

- `ProviderHubPaths` 从 app data/install manifest 构造所有绝对路径；业务层不得拼接默认 HOME 路径。
- attach planner 输出不含 secret 的 auth intent；transaction executor 在 binding 已准备后用
  binding id/profile id/state root/helper path 生成最终 patch，并把该 patch 纳入 journal/rollback。
- helper 以判别命令解析参数；Gateway token 从 binding store 读取，Keychain 从版本引用读取，
  approved command 从只含结构化命令/参数/timeout/hash 的 approval store 读取并受限执行。

**Test design**

- 正常：三种 helper 模式均从真实临时 store 输出 token，stdout 无额外字符。
- 边界：绝对路径含空格；环境被清空；legacy root 只有一份数据；重复迁移幂等。
- 非法：缺参数、相对/未签名组件、过期 approval、auth 冲突、未来 schema 均零写入且无 secret 泄漏。
- 集成：执行生成的 auth table 命令，而非只比较字符串；Gateway config 含两个 retry = 0。

**Acceptance**

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_auth_runtime
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_phase1_g2
```

**状态：验证成功**

- [x] 测试已先失败（缺少 `provider_runtime` 与未物化 auth intent，编译失败）
- [x] 实现完成
- [x] 验收命令通过（Rust 50/50；前端 focused 25/25；frontend build 通过）
- [x] 无占位 binding/相对 helper/secret 落盘

## M2 — Phase 1 生产服务与生命周期

### FIX-102：真实 readiness、上游 HTTP、binding 生命周期与启动恢复

**Why**

当前 test-provider 只做 URL/auth 校验后返回成功，attach readiness 硬编码；事务恢复、adopt、
rename/delete guard、drift/reconcile 没有接入 Tauri/launcher/account 的生产路径。

**What to do**

- test-provider 使用受控 HTTP client 发起真实探测并记录 health observation，不改 capability declaration。
- readiness 聚合 missing secret、adapter required、Gateway unavailable、binding/drift 等所有 blocker。
- Tauri 启动与 launcher 入口执行幂等 journal recovery。
- account/profile rename/delete 和 provider update 使用 binding lifecycle；冲突时零写入。
- provider update 对已绑定投影执行明确 stale/rebind 规则，不静默留下旧配置。

**Logic design**

- 将网络 client/clock/DNS 作为可注入依赖；生产使用严格 client，测试使用 loopback fake upstream。
- readiness 只解析 secret 是否可消费，不把 token 暴露到 DTO/log。
- mutation service 在 CAS 前执行 binding guard；跨 store/config 变更继续使用 journal coordinator。

**Test design**

- 正常：fake upstream 实际收到 `/models`/健康请求；env/keychain/auth-command readiness 正确。
- 边界：多 blocker 同时返回；rename display name 不改变 profile id；重复 recovery 幂等。
- 错误：timeout/401/5xx、缺 secret、drift、attached delete、CAS 冲突均有稳定错误码且零部分写入。
- 启动：调用真实 app bootstrap/launcher service，证明 recovery 函数不是孤立测试代码。

**Acceptance**

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_phase1_production
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_api_v2 --test provider_binding --test provider_attach_transaction
```

**状态：验证成功**

- [x] 测试已先失败（production resolver/HTTP/attach readiness API 尚不存在，编译失败）
- [x] 实现完成
- [x] 验收命令通过（production 3/3；API/binding/transaction 35/35；G2 5/5）
- [x] 生产入口覆盖证据存在（Tauri setup 与 `lam` 均调用同一 recovery service）

## M3 — Adapter 与 DeepSeek 生产接线

### FIX-201：Gateway routes 使用 registry、compatibility profile 与完整历史

**Why**

当前 routes 直接调用转换函数并用 model 名称包含 `reasoner` 判断，`AdapterRegistry` 与
`ReasoningHistory` 只在单元测试使用。DeepSeek thinking + tool follow-up 因而不满足设计。

**What to do**

- 提供正式 `responses_to_chat_completions` ProtocolAdapter，并由 Gateway composer/route 从 registry 获取。
- compatibility policy 来自绑定的 Provider profile/version，不依据 model 字符串猜测。
- 对 Codex 完整 input history 做确定性 reasoning continuity 映射；不引入 response store。
- `/v1/models` 输出继续严格匹配 Codex 0.144.1 捕获契约。

**Logic design**

- 每请求创建独立 exchange；route 只负责认证、限流、HTTP、取消和错误边界。
- adapter 从完整 Responses input 中恢复 tool-call history 所需的受控 metadata；原始 CoT 不作为
  普通 content/summary 暴露。若入站历史缺少 Provider 强制要求的数据，明确失败而非伪造。
- 若 Codex 合同无法携带所需 reasoning，则在测试先证明该限制，并采用设计允许的、无 response
  store 且 schema 合法的受控 item/metadata 映射；不得按 binding 建隐式对话缓存。

**Test design**

- 正常：text、stream、tool initial/follow-up、resume 均经 registry exchange。
- DeepSeek：thinking + tool call 的下一请求包含正确 reasoning/tool_call_id，Codex-facing 不泄漏 CoT。
- 错误：未知 adapter/profile、缺 reasoning、previous_response_id、非法 tool、超限均结构化失败。
- 断言生产 routes 不再通过 model substring 选择策略。

**Acceptance**

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_gateway_adapter_integration
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_phase2_g3 --test provider_gateway_routes
```

**状态：验证成功**

- [x] 测试已先失败（旧 route 未经 registry 且按 model substring 选策略）
- [x] 实现完成
- [x] 验收命令通过（adapter focused 37/37；route 8/8；G3 2/2）
- [x] 无 model substring/隐式 response store；Codex 0.144.1 不携带 reasoning 的
      thinking+tool 组合在上游调用前明确返回 `ADAPTER_REASONING_HISTORY_UNREPRESENTABLE`

## M4 — Sidecar 生命周期与有界资源

### FIX-301：把 supervisor、稳定端口迁移和 stream failure 语义接入生产

**Why**

当前 supervisor、byte queue、port migration 主要是可单测对象，sidecar 主程序没有有限重启/
idle 退出闭环；生产 stream transport failure 也未保证发出 `response.failed`。

**What to do**

- launcher/Tauri supervisor 实际执行单实例、有限指数退避、readiness 与退出状态传播。
- 无 binding 且无 inflight 经过 idle grace 后 sidecar 退出；UI 退出不终止仍需运行的 sidecar。
- stable-port 迁移以事务重投影全部 Gateway profile，任一失败完整回滚。
- 生产 SSE channel 同时受 event count 与 byte budget 限制；transport/malformed/overflow 发出一次 failed terminal event。
- `/v1/models` 与 `/v1/responses` 在读取 body 前强制 bearer，所有 guard cancellation-safe 释放。

**Test design**

- 正常：崩溃后按预算重启并恢复；最后 binding detach 后 idle 退出。
- 边界：最大 restart、最大 queue、inflight 阻止 idle、两个 launcher 竞争单实例。
- 错误：foreign port、迁移中 config/CAS 失败、SSE 中断/畸形/溢出均不部分提交且 terminal 唯一。
- 安全：未认证大 body 在 body read 前拒绝；旧 token 撤销立即生效。

**Acceptance**

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_sidecar_production
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_gateway_sidecar --test provider_gateway_routes --test provider_gateway_server
```

**状态：验证成功**

- [x] 测试已先失败（生产 sidecar/launcher 未调用 policy，stream disconnect 无 failed event）
- [x] 实现完成（supervisor、idle、双重 queue budget、stream terminal 与 port 全量重投影事务）
- [x] 验收通过（sidecar production 4/4；sidecar 7/7；routes 9/9；server 6/6）
- [x] supervisor/queue/port migration 均有生产调用方；port 迁移通过 Tauri plan/execute API
      强制 fingerprint/revision 校验并在任一配置写入失败时反向回滚
- [x] Darwin control socket basename 使用 install-id 哈希限制长度，避免正常 per-user temp root
      与 UUID 组合超过 AF_UNIX 路径上限

## M5 — 真实进程 G4

### FIX-302：用 packaged-layout 二进制链重建 G4

**Why**

现有 G4 在同一测试进程中启动 server 并直接 reqwest，无法证明安装 manifest、绝对 helper、
`lam` launcher、sidecar 子进程与 Codex config 能协同工作。

**What to do**

- 建立 fake Codex 可执行程序：读取 `CODEX_HOME/config.toml`，执行 auth command，调用
  `/v1/models` 和 `/v1/responses`，覆盖 text/stream/tool follow-up/resume。
- G4 构造与 app bundle 一致的临时安装布局和签名/hash manifest，执行真实 `lam codex`。
- sidecar 和 fake upstream 均为独立进程；测试关闭管理端后 Codex 仍可调用，detach 后旧 token 立即失败。
- 捕获所有子进程退出码并可靠清理，不以固定 sleep 判定 ready。

**Test design**

- 正常：完整进程链 text/stream/tool/resume 通过，fake upstream 观察到准确请求历史。
- 生命周期：冷启动、复用已运行 sidecar、manager close、sidecar restart、detach/revoke。
- 错误：helper 不可执行、manifest hash 错、sidecar 不 ready、upstream 断流均传播稳定错误。
- secret scan：argv/env/config/log/fixture/journal 无 token/provider secret。

**Acceptance**

```bash
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_phase3_g4 -- --nocapture
pnpm --dir apps/desktop gateway:package:verify
```

**状态：验证成功**

- [x] 测试已先失败（首次真实进程测试暴露 Keychain ACL 与 Unix socket 路径边界）
- [x] 实现完成
- [x] 验收命令通过（真实 process G4 1/1；组件 G4 2/2；release 三组件签名/checksum verify 通过）
- [x] `provider_phase3_process_g4` 未直接调用 in-process route/server；实际执行签名后的
      `lam`/sidecar/helper、读取 config 的 fake Codex 与独立 fake upstream

## M6 — 全量门禁与文档复核

### FIX-401：重新验收 Phase 1–3 并校正状态

**Why**

旧文档把局部测试通过误记为生产完成。必须在真实链通过后重新生成证据并防止回归。

**What to do**

- 增加静态 wiring gate，阻止关键生产类型再次变成 test-only/dead code。
- 更新 G2/G3/G4 manifest/checksum/coverage，保持 synthetic fixture 脱敏。
- 运行 Rust/前端/lint/format/build/packaging/contract/full diff gates。
- 逐项对照设计，将 Phase 1–3 状态只在证据通过后恢复为 `验证成功`。

**Test design**

- gate 故意移除 helper/registry/supervisor/bootstrap wiring 时应失败。
- fixture checksum 被篡改、出现 secret pattern、旧 placeholder/path 时应失败。
- 完整测试从干净临时 HOME/state root 运行两次，结果确定。

**Acceptance**

```bash
pnpm test:gateway-phase0
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
pnpm --dir apps/desktop test
pnpm --dir apps/desktop lint
pnpm --dir apps/desktop format:check
pnpm --dir apps/desktop build
git diff --check
```

**状态：验证成功**

- [x] 所有前置任务为验证成功
- [x] 全量门禁通过（Rust 332 passed/2 ignored；前端 167 passed；clippy、lint、
      Rust/Prettier format、frontend/release build、UI smoke、Phase 0 contract、diff 全绿）
- [x] release 三组件签名/checksum/manifest 校验通过；最终 `.app` 完整性 finalize 与
      `LAM_0.2.1_aarch64.dmg` 创建/只读挂载校验通过
- [x] coverage/todo/manifest 与实现一致
- [x] 无已知 Phase 1–3 生产断链

## STOP 条件

- 单任务修复/验证连续 5 轮仍失败，标记 `验证失败` 并停止该任务。
- 发现必须改变已锁定 Codex 0.144.1 contract、扩大网络信任边界或引入 response store 时，
  先更新设计和威胁模型，不得静默实现。
- 任何未解决的 secret 泄漏、配置破坏、旧 token 可用、无限重试或未认证访问均阻止交付。
