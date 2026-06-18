# Todo List: v0.2.0 Remote API Provider

状态规则：

- `待执行`
- `待测试验证`
- `验证成功`
- `验证失败`

本 todo 已按 2026-06-18 调研结果重排：v0.2.0 采用 Responses-first，同时纳入 Chat Completions text-only experimental adapter MVP；流式转换、tool/function 转换、LAM Runner 不作为 GA 阻塞项。

## v0.2.0 必做

| 任务 | 验收标准 | 状态 |
|------|----------|------|
| Provider schema 迁移 | `ProviderProfile` 支持 `kind: responses/chat_completions`、`models[]`、`default_model`、`codex` tuning、`capabilities`、`adapter`、`used_by_profile_ids`；旧 `wireApi/defaultModel` 可兼容读取或迁移 | 待执行 |
| Provider kind 状态 | `responses` 标记 stable；`chat_completions` 标记 experimental；不能把 experimental 伪装成 stable available | 待执行 |
| Codex TOML writer/reader | Attach/Add Account 写入官方 `model`、`model_provider`、`[model_providers.<id>]`、`base_url`、`env_key`、`wire_api = "responses"`；支持 query params 和 retry/timeout；写前备份，写后重新解析 | 待执行 |
| Provider ID 校验 | 禁用内置保留 ID：`openai`、`ollama`、`lmstudio`、`amazon-bedrock`；非法 ID 返回结构化错误 | 待执行 |
| OpenAI base URL 快捷路径 | 对 OpenAI-compatible proxy/router/data residency 支持 `openai_base_url` mode；不强制创建自定义 provider；写前备份 | 待执行 |
| SecretStore 补齐 | Keychain/env/auth command 三种模式可用；secret 不进前端、不进 config、不进 wrapper、不进日志；失败返回结构化错误 | 待执行 |
| Codex auth helper | Keychain 模式可生成 `[model_providers.<id>.auth]` command-backed auth；helper 只向 stdout 输出 token，不打印额外日志 | 待执行 |
| Add Account Remote API | Add Account 支持 ChatGPT Login / Remote API 分支；Remote API 最小只需 account suffix、base URL、secret、model；可选择已有 Provider 或 inline 创建；显示 config preview | 待执行 |
| Provider 多模型支持 | Provider 可保存 `models[]` 与 `default_model`；profile 绑定时选择 active model；切换模型会备份并更新 config | 待执行 |
| Provider Center v0.2 UI | Provider 卡片显示 kind、base URL、model、secret mode、health、used profiles、support status；支持 Test/Edit/Attach/Delete | 待执行 |
| Provider metadata/config test | 校验 base URL、model、secret reference、reserved ID、TOML 可生成并可重新解析；错误结构化 | 待执行 |
| Provider live test | Responses Provider smoke `/v1/responses`；Chat Completions Provider smoke `/chat/completions`；健康状态区分 available/experimental/degraded/unavailable/untested；不记录 prompt/body/secret | 待执行 |
| Adapter feasibility 子集 | 明确 v0.2.0 Responses text-only request/response 子集、Chat Completions 映射矩阵、不可转换能力列表 | 待执行 |
| Adapter manager | loopback-only adapter；启动/停止/status API；端口占用返回明确错误；默认只在 experimental Provider 需要时启动 | 待执行 |
| Responses -> Chat Completions 非流式转换 | 文本 input、基础 roles、model、temperature/top_p/max tokens、普通 text output 可转换；单元测试覆盖正常/非法输入/上游错误 | 待执行 |
| Chat Completions -> Responses 非流式转换 | 上游 message content 转 Responses-style output；usage/error/status 规范化；测试覆盖空内容、多 choice、错误响应 | 待执行 |
| Adapter 安全与日志 | adapter 不记录 prompt/body/secret；只记录低敏指标；仅绑定 127.0.0.1；测试覆盖日志脱敏 | 待执行 |
| Adapter 状态联动 | Chat Completions Provider 的 adapter 未运行时，profile/Relay/Resume 显示 unavailable 或提示启动 adapter；running 后刷新为 experimental/degraded | 待执行 |
| Account/Provider 状态联动 | Attach 后刷新 account/provider/session；used-by profiles 正确；health 和 unsupported 状态显示准确 | 待执行 |
| Session provider mismatch 更新 | Session 继续展示 original/current provider/model；API profile 也参与 mismatch 判断 | 待执行 |
| API profile 参与账号切换 | 从 ChatGPT profile 切到 Responses API profile 时，resume 使用目标 profile Provider；UI 展示 original/current provider/model 和 capability warning | 待执行 |
| API profile 参与 Relay runtime | API profile 可作为 relay runtime；默认 provider policy 为 `inherit_runtime`；relay profile 使用 runtime Provider secret/config | 待执行 |
| API profile 参与 Relay source | API profile 可作为 relay source；sync/relay 不复制 secret；target Provider 与 source 不一致时展示 mismatch warning | 待执行 |
| API profile wrapper 命令 | 添加 Remote API 账号后生成/更新 `codex-<name>` wrapper；设置 `CODEX_HOME`；参数原样透传；至少保留 Copy command fallback | 待执行 |
| 手工验收：Responses Provider | 用真实 Responses-compatible Provider 创建 profile，启动 Codex，完成一轮普通 prompt | 待执行 |
| 手工验收：Chat Completions Provider | 用真实 `/chat/completions` Provider 经 experimental adapter 启动 Codex，完成一轮普通文本 prompt | 待执行 |
| 手工验收：API profile 接力 | 使用 ChatGPT profile 的 session 接力到 Responses API profile，确认 resume 成功且 mismatch 信息准确 | 待执行 |
| 手工验收：wrapper 启动 | `codex-<api-profile>` 可从终端启动；`codex-<api-profile> resume <session-id>` 可用 | 待执行 |
| 文档更新 | README、FINAL-DESIGN 或 roadmap 链接到 `docs/06-v0.2-remote-api-provider-design.md`；明确 v0.2.0 Responses stable + Chat Completions experimental 范围 | 待执行 |

## v0.2.x 实验项，不阻塞 v0.2.0

| 任务 | 验收标准 | 状态 |
|------|----------|------|
| 流式转换 spike | 验证 Chat Completions SSE delta 是否能稳定转换为 Codex 可消费的 Responses-style stream；断流/上游错误有结构化错误 | 待执行 |
| Tool/function 能力策略 | 支持基础 function tools 转换；不支持的 Responses tool 类型明确 degraded，不静默丢弃 | 待执行 |
| Terminal 体验优化 | Terminal 模式优先新 tab 或可配置 window/tab；避免每次 Relay/Resume 无提示地产生大量窗口 | 待执行 |
| LAM Runner MVP | LAM 可在内部管理 Codex 子进程，展示 profile/provider/model/session/cwd 与输出；失败时可复制命令；完整 PTY 可后续推进 | 待执行 |
| 手工验收：Chat Completions API 接力 | 使用 ChatGPT profile 的 session 接力到 Chat Completions API profile；adapter 停止时 UI 正确阻止 | 待执行 |

## 依赖关系

1. 先做 Provider schema 迁移和旧 store 兼容。
2. 再做 Codex TOML writer/reader 与 config 备份恢复。
3. SecretStore 和 auth helper 与 writer 并行推进，但必须在 Add Account 前完成。
4. Provider Center 和 Add Account Remote API 依赖 schema、writer、secret。
5. Responses Provider live test 完成后，先跑通 stable Remote API profile。
6. Adapter feasibility 子集确认后，再实现 Chat Completions text-only experimental adapter。
7. wrapper 命令兼容 API profile，保证用户可从终端启动。
8. account/session/relay 状态联动依赖 stable Provider 和 adapter status。
9. 手工验收真实 Responses Provider 和真实 Chat Completions Provider 后，v0.2.0 才算闭环。
10. 流式转换、tools、Terminal 优化、LAM Runner 进入 v0.2.x，不阻塞 v0.2.0。

## 风险清单

| 风险 | 处理策略 | 状态 |
|------|----------|------|
| 用户填入 Chat Completions-only Provider | 标记 experimental；只允许 text-only adapter MVP | 待执行 |
| Secret 泄露 | Keychain/env/auth helper；日志脱敏；测试覆盖 | 待执行 |
| config 写坏导致 Codex 无法启动 | 写前备份、写后重新解析、失败回滚或提供恢复入口 | 待执行 |
| Provider ID 与 Codex 内置 ID 冲突 | reserved ID 校验 | 待执行 |
| OpenAI proxy 场景过度建模 | 提供 `openai_base_url` mode | 待执行 |
| capability check 不完整 | unknown/degraded 状态，不伪装支持 | 待执行 |
| Chat Completions 转换语义不完整导致 Codex 行为异常 | v0.2.0 限定 text-only 非流式；stream/tools/reasoning 进入 v0.2.x | 待执行 |
| LAM Runner TTY 兼容性不足 | 不作为 v0.2.0 依赖；保留 Terminal 和 Copy command fallback | 待执行 |
