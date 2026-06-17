# Todo List: v0.2.0 Remote API Provider

状态规则：

- `待执行`
- `待测试验证`
- `验证成功`
- `验证失败`

| 任务 | 验收标准 | 状态 |
|------|----------|------|
| Provider model 扩展 | `ProviderProfile` 支持 `kind: responses/chat_completions`、capabilities、adapter metadata；旧 providers.json 可迁移或兼容读取 | 待执行 |
| Provider 多模型支持 | Provider 可保存 `models[]` 与 `default_model`；profile 绑定时选择 active model；切换模型会备份并更新 config | 待执行 |
| Codex config writer 收敛 | Attach/Add Account 写入官方 `model_providers.<id>` TOML；Responses Provider 直连；Chat Completions Provider 写入本地 adapter endpoint；写前备份 | 待执行 |
| SecretStore 能力补齐 | Keychain/env 两种模式均可供 adapter 使用；secret 不进前端、不进 config、不进日志；失败返回结构化错误 | 待执行 |
| Add Account 远程 API 流程 | Add Account 支持 ChatGPT Login / Remote API 分支；Remote API 最小只需 base URL、API key/env key、model；可选择已有 Provider 或 inline 创建；显示 config preview | 待执行 |
| Provider Center v0.2 UI | Provider 卡片显示 kind、base URL、model、secret mode、health、used profiles、adapter status；支持 Test/Edit/Attach/Delete | 待执行 |
| Provider live test | Responses Provider smoke `/v1/responses`；Chat Completions Provider smoke `/chat/completions`；健康状态区分 available/degraded/unavailable | 待执行 |
| Adapter manager | LAM 内置 loopback-only adapter；稳定端口策略；启动/停止/status API；端口占用返回明确错误 | 待执行 |
| Responses -> Chat Completions 非流式转换 | 文本 input、基础 roles、model、temperature/top_p/max tokens、普通 text output 可转换；单元测试覆盖正常/非法输入/上游错误 | 待执行 |
| Chat Completions -> Responses 非流式转换 | 上游 message content 转 Responses-style output；usage/error/status 规范化；测试覆盖空内容、多 choice、错误响应 | 待执行 |
| 流式转换 MVP | Chat Completions SSE delta 可转换为 Codex 可消费的 Responses-style stream；断流/上游错误有结构化错误 | 待执行 |
| Tool/function 能力策略 | 支持基础 function tools 转换；不支持的 Responses tool 类型明确 degraded，不静默丢弃 | 待执行 |
| Adapter 安全与日志 | adapter 不记录 prompt/body/secret；只记录低敏指标；仅绑定 127.0.0.1；测试覆盖日志脱敏 | 待执行 |
| Account/Provider 状态联动 | 绑定 Chat Completions Provider 的 profile 在 adapter 未运行时显示 unavailable；adapter running 后刷新为 available/degraded | 待执行 |
| Session provider mismatch 更新 | Session 列继续展示 original/current provider；Chat Completions adapter Provider 也能参与 mismatch 判断 | 待执行 |
| API profile 参与账号切换 | 从 ChatGPT profile 切到 API profile 时，resume 使用目标 profile Provider；UI 展示 original/current provider/model 和 capability warning | 待执行 |
| API profile 参与 Relay runtime | API profile 可作为 relay runtime；默认 provider policy 为 `inherit_runtime`；relay profile 使用 runtime Provider secret/config | 待执行 |
| API profile 参与 Relay source | API profile 可作为 relay source；sync/relay 不复制 secret；target Provider 与 source 不一致时展示 mismatch warning | 待执行 |
| Adapter 状态联动 Relay/Resume | Chat Completions Provider 的 adapter 未运行时，Relay/Resume 禁用或提示启动 adapter；running 后恢复可用 | 待执行 |
| API profile wrapper 命令 | 添加 Remote API 账号后生成/更新 `codex-<name>` wrapper；设置 `CODEX_HOME`；参数原样透传；Chat Completions Provider 在 adapter 未运行时给出明确提示 | 待执行 |
| 启动方式设置 | UI 支持 Terminal / Copy command / LAM Runner 三种启动策略；至少保留 Copy command fallback | 待执行 |
| LAM Runner MVP | LAM 可在内部管理 Codex 子进程，展示 profile/provider/model/session/cwd 与输出；失败时可复制命令 | 待执行 |
| Terminal 体验优化 | Terminal 模式优先新 tab 或可配置 window/tab；避免每次 Relay/Resume 无提示地产生大量窗口 | 待执行 |
| 手工验收：Responses Provider | 用真实 Responses-compatible Provider 创建 profile，启动 Codex，完成一轮普通 prompt | 待执行 |
| 手工验收：Chat Completions Provider | 用真实 `/chat/completions` Provider 创建 profile，经 LAM adapter 启动 Codex，完成一轮普通 prompt | 待执行 |
| 手工验收：API profile 接力 | 使用 ChatGPT profile 的 session 接力到 Responses API profile，确认 resume 成功且 mismatch 信息准确 | 待执行 |
| 手工验收：Chat Completions API 接力 | 使用 ChatGPT profile 的 session 接力到 Chat Completions API profile，经 adapter resume 成功；adapter 停止时 UI 正确阻止 | 待执行 |
| 手工验收：wrapper 启动 | `codex-<api-profile>` 可从终端启动；`codex-<api-profile> resume <session-id>` 可用；Chat Completions adapter 未运行时提示明确 | 待执行 |
| 文档更新 | README、FINAL-DESIGN 或 roadmap 链接到 `docs/06-v0.2-remote-api-provider-design.md`；明确 v0.2.0 范围和限制 | 待执行 |

## 依赖关系

1. 先做 Provider model 扩展与 config writer。
2. 再做 Add Account Remote API 基础流程。
3. Responses Provider 直连先完成，作为最小可交付。
4. wrapper 命令兼容 API profile，保证用户可从终端启动。
5. Adapter manager 完成后再接 Chat Completions Provider。
6. Tool/function 与流式转换在文本非流式闭环后推进。
7. LAM Runner 与 Terminal 体验优化可以在 API profile 跑通后独立推进。

## 风险清单

| 风险 | 处理策略 | 状态 |
|------|----------|------|
| Codex 只支持 `responses` wire_api，无法直接使用 `/chat/completions` | LAM 提供本地 Responses-compatible adapter | 待执行 |
| Chat Completions Provider 能力不完整 | capability check + degraded 状态，不伪装支持 | 待执行 |
| Secret 泄露 | Keychain/env 引用；日志脱敏；测试覆盖 | 待执行 |
| adapter 端口冲突 | 固定端口 + 明确错误；后续可加端口设置 | 待执行 |
| 转换语义不完整导致 Codex 行为异常 | MVP 先限定文本/基础 tool；复杂能力明确 unsupported | 待执行 |
| API profile 与 ChatGPT profile 跨 Provider 接力导致行为变化 | 保留 original/current provider 信息，resume 前展示 mismatch/capability warning | 待执行 |
| 多模型选择导致配置混乱 | Provider 保存 models[]，profile 只写 active model；每次切换都备份 config | 待执行 |
| LAM Runner TTY 兼容性不足 | 保留 Terminal 和 Copy command fallback；Runner 先做 MVP，再做完整 PTY | 待执行 |
