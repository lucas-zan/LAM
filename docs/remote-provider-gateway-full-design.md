# LAM Remote Provider Gateway Full Design

状态：已完成架构评审，待按本文纵向切片实施与验收
目标：通过可独立验收的纵向切片，完整交付外部 API Provider 接入、协议适配、本地统一网关、Codex profile 绑定、自动化测试和验收闭环。

## 1. 目标

LAM 要从“Codex 账号/session 管理工具”扩展为本地 Provider Hub：

- 管理外部模型 Provider、模型列表、密钥引用、健康状态和能力矩阵。
- 支持原生 `/v1/responses` Provider 直连 Codex。
- 支持 `/chat/completions` Provider 通过用户显式启用的本地适配器接入 Codex。
- 只有启用本地适配器的 Chat Completions Provider 才走代理；Responses Provider 直接使用上游 endpoint。
- API profile 与 ChatGPT auth/session profile 同等参与 session 浏览、sync、relay、resume。
- 本地网关可作为统一外部接口管理层，后续可开放给其他本机客户端复用。

这不是 DeepSeek 专用补丁。DeepSeek 只是一个 Chat Completions Provider 示例。设计必须让 Mistral、OpenAI-compatible router、Azure、Ollama、LM Studio、内部代理等 Provider 以配置和小型 adapter 扩展接入。

### 1.1 已确定的架构决策

- Codex 边界只输出 `wire_api = "responses"`；旧 `wireApi = "openai"` 只是迁移输入，不得写回 Codex 配置。
- Responses Provider 默认直连；Chat Completions Provider 只能通过显式启用的本地 adapter 接入 Codex。
- Provider metadata、secret reference、运行时状态和测试观测结果分开存储，不用一个字段同时表示配置、健康和能力。
- `config.toml` 采用非破坏性编辑，只更改 LAM 管理的 key，不覆盖用户其他配置。
- Gateway 是可被 LAM 和 profile launcher 拉起的独立 sidecar，不与前端窗口生命周期绑定。
- 首版只声称支持经过 contract test 验证的 Responses 子集，不声称完整实现 OpenAI Responses API。

## 2. 文档依据

本设计需要同时兼容 Codex、OpenAI API 和 DeepSeek API 的公开行为。实现前后都要以这些文档和样例作为协议依据：

- OpenAI Responses migration guide: <https://developers.openai.com/api/docs/guides/migrate-to-responses>
- Codex custom model providers: <https://developers.openai.com/codex/config-advanced>
- Codex configuration reference: <https://developers.openai.com/codex/config-reference>
- DeepSeek thinking mode guide: <https://api-docs.deepseek.com/guides/thinking_mode>
- DeepSeek Chat Completions reference: <https://api-docs.deepseek.com/api/create-chat-completion>

结论：

- Codex 侧自定义 Provider 的稳定接入口是 `wire_api = "responses"`，LAM 写给 Codex 的 endpoint 必须是 Responses-compatible。
- OpenAI 官方推荐从 Chat Completions 迁移到 Responses；Responses 的工具调用、状态、输出 item、事件流语义与 Chat Completions 不同，不能只做字段改名。
- DeepSeek 的 Chat Completions 兼容接口包含 thinking mode 和 tool call 的特殊约束。特别是 thinking mode 下的 reasoning 内容、assistant tool call 消息、tool result 消息、后续请求历史，都必须按 DeepSeek 样例处理。
- Provider 协议判断只使用官方文档和经受控捕获的真实响应 fixture；第三方样例不作为规范依据。

## 3. 非目标

- 不把外部 API key 写入 `config.toml`、session、日志、wrapper 或前端状态。
- 不复制 `auth.json` 或 API secret 到 relay/sync 目标。
- 不在项目级 `.codex/config.toml` 写 provider/auth 配置；必须写 user-level `$CODEX_HOME/config.toml`。
- 不用大量 `if provider == "deepseek"` 或 `if protocol == ...` 分散在 UI、配置写入、网关和测试里。

## 4. 核心原则

### 4.1 抽象

Provider 接入围绕清晰契约组织：

- `ProviderStore`：保存 provider 元数据和兼容旧 store。
- `SecretStore`：按 env/keychain/auth command 解析 secret，不暴露 secret 值。
- `ProviderProtocol`：描述上游协议能力。
- `ProviderGateway`：对外提供 Codex 可消费的 Responses endpoint。
- `ProtocolAdapter`：在协议之间转换请求、事件、工具调用、错误和 usage。
- `CodexConfigEditor`：只负责非破坏性编辑、校验和原子写入 `$CODEX_HOME/config.toml`。

业务流程依赖这些契约，而不是依赖具体 DeepSeek/OpenAI/Azure 实现细节。

### 4.2 简洁

每个模块只有一个职责：

- Provider metadata 只处理数据和校验。
- Secret 只处理密钥读取和 helper 配置。
- Config editor 只修改 LAM 管理的 TOML key，并负责备份/回滚。
- Gateway 只处理 HTTP 路由、鉴权、provider 选择和日志脱敏。
- Adapter 只处理协议转换。
- Relay 只处理 session/context 迁移，不处理密钥迁移。

### 4.3 易扩展

新增协议或供应商时，优先新增一个 adapter 或 provider preset，而不是修改一串分散的条件分支。

推荐模式：

```text
ProtocolAdapterRegistry
  responses -> passthrough adapter
  chat_completions -> chat-completions-to-responses adapter
  anthropic_messages -> anthropic-to-responses adapter later
```

业务层只问：

```text
ProviderRuntimePlan:
  codex_base_url
  codex_wire_api
  upstream_base_url
  adapter_required
  adapter_id
  capability_matrix
```

而不是到处判断 provider 类型。

## 5. 用户模型

### 5.1 Provider 类型

LAM UI 中 Provider 创建表单提供两个核心字段：

```text
Protocol:
- Responses API (/v1/responses)
- Chat Completions API (/chat/completions)

Use local adapter proxy:
- disabled for Responses API
- optional for Chat Completions API
```

规则：

- `protocol = responses`：直连；不走本地代理。
- `protocol = chat_completions` 且 `adapter = none`：可保存元数据和测试上游，但不能 attach 成可运行 Codex profile。
- `protocol = chat_completions` 且 `adapter = local { ... }`：可 attach；Codex 访问本地 gateway；gateway 转换到上游 `/chat/completions`。

### 5.2 Profile 类型

所有 profile 都是独立 `CODEX_HOME`：

```text
ChatGPT profile:
  CODEX_HOME=~/.codex-a
  auth.json / keychain
  sessions/
  config.toml

Remote API profile:
  CODEX_HOME=~/.codex-deepseek
  config.toml
  sessions/
  no copied auth.json
  provider references only
```

两者都可以参与 relay/sync/resume。接力迁移的是 session/context，不迁移身份或 secret。

## 6. 数据模型

### 6.1 ProviderProfile

替换当前扁平 `ProviderProfile`，保留兼容读取旧字段：

```text
ProviderProfile
  id: string
  name: string
  protocol: responses | chat_completions
  base_url: string
  default_model: string
  models: ProviderModel[]
  secret: SecretReference
  capabilities: CapabilityDeclaration
  adapter: AdapterConfig
  compatibility_profile: string | null
  codex: CodexProviderOptions
  created_at: string
  updated_at: string
```

`used_by_profile_ids` 是 profile store 的派生视图，不持久化到 provider 条目，避免两份索引不一致。Readiness 也根据 metadata、secret availability、profile binding 和 Gateway state 动态计算。健康观测和 adapter 验证结果不嵌入 `ProviderProfile`，分别写入 runtime state store 和 conformance cache。

Provider store 使用显式版本封装：

```text
ProviderStoreEnvelope
  schema_version: integer
  revision: integer
  providers: ProviderProfile[]
```

迁移必须是可测试的纯函数，例如 `migrate_v1_to_v2(old) -> Result<V2>`。读取阶段不得隐式写回；迁移完成后通过临时文件、flush/fsync 和原子 rename 单独提交。`revision` 用于检测多实例之间的乐观并发冲突。遇到高于当前支持版本的 store 时必须拒绝写入，不得降级覆盖。

兼容迁移：

- 旧 `wireApi = "responses"` 或 `"openai"` 默认映射为 `protocol = responses`，除非显式迁移配置标记为 chat completions。
- 旧 `defaultModel` 补成 `models = [{ id: defaultModel, label: defaultModel }]`。
- 旧 `envKey` 转成 `secret.kind = env`。

`compatibility_profile` 只描述供应商协议细节，例如 DeepSeek 的 `reasoning_content` 和 thinking tool-call 历史策略。它必须是 registry 中已注册的受控 ID，不是可任意拼写的策略字符串。普通 OpenAI-compatible provider 可以为空或使用 `openai_chat_completions`。

### 6.2 SecretReference

```text
SecretReference
  kind: env | keychain | auth_command | none
  env_key?: string
  keychain_account?: string
  auth_command?: AuthCommand
```

约束：

- 前端只能看到引用信息，不能看到 secret。
- Keychain 写入失败时不得持久化 provider metadata。
- auth command stdout 只能输出 bearer token。

Secret 到运行时认证的映射必须是单一、可预测的：

| SecretReference | Responses 直连 | Gateway 访问上游 | Codex 访问 Gateway |
| --- | --- | --- | --- |
| `env` | 写 Codex `env_key` | sidecar 从明确允许的环境变量读取 | 不用于共享 Gateway token |
| `keychain` | 写 LAM auth helper | sidecar 通过 `SecretStore` 读取 | 写 profile-specific gateway auth helper |
| `auth_command` | 写 Codex provider `auth` table | sidecar 通过受限 command runner 调用 | 不复用上游 helper |
| `none` | 仅允许无认证上游 | 仅允许无认证上游 | 禁止；Gateway 必须鉴权 |

`env_key`、provider `auth`、`experimental_bearer_token` 和 `requires_openai_auth` 是互斥认证模式，Runtime Planner 必须在写配置前阻止冲突。Auth command 只允许结构化参数，禁止 shell 展开；必须定义 timeout、最大 stdout 大小、退出码、token 缓存/刷新和 stderr 脱敏规则。前端不展示 auth command 的运行结果。

### 6.3 AdapterConfig

```text
AdapterConfig
  none
  local:
    adapter_id: chat_completions_to_responses
    upstream_path: /chat/completions | validated_custom_path
```

Adapter 使用判别联合而不是 `enabled + mode` 两套状态，避免 `enabled = false, mode = local` 等非法组合。`local_base_url` 是 Gateway runtime state，不属于 Provider metadata。

Responses Provider 固定：

```text
adapter = none
```

Chat Completions Provider 可选：

```text
adapter.local.adapter_id = chat_completions_to_responses
adapter.local.upstream_path = /chat/completions
```

### 6.4 Capability、Readiness 与 Health

能力要分上游能力和适配能力，避免误导用户。

```text
CapabilityDeclaration
  declared:
    streaming: supported | partial | unsupported | unknown
    tool_calls: supported | partial | unsupported | unknown
    structured_outputs: supported | partial | unsupported | unknown
    reasoning: supported | partial | unsupported | unknown
  user_overrides:
    streaming: supported | partial | unsupported | not_required | unknown
    tool_calls: supported | partial | unsupported | not_required | unknown
    structured_outputs: supported | partial | unsupported | not_required | unknown
    reasoning: supported | partial | unsupported | not_required | unknown
```

Adapter 测试结果单独表示：

```text
CapabilityVerification
  adapter_id
  adapter_version
  provider_compatibility_profile
  tested_at
  results: CapabilitySet
  evidence_ids[]
```

Provider 当前可否运行与网络健康分开：

```text
ProviderReadiness = metadata_only | missing_secret | attachable | attached
ProviderHealthObservation
  status: unknown | healthy | degraded | unavailable
  checked_at
  latency_ms
  error_code?
```

短暂的网络失败不得把 `tool_calls = supported` 改成 `unsupported`；健康状态只描述本次观测。UI 展示的 effective capability 由 declaration、user override 和与当前 adapter 版本匹配的 verification 汇总得出。

DeepSeek 示例：

```text
protocol = chat_completions
upstream.streaming = supported
upstream.tool_calls = supported
adapter.streaming = supported only after automated stream tests pass
adapter.tool_calls = supported only after tool-call roundtrip tests pass
adapter.reasoning = partial until reasoning metadata mapping is verified
```

能力数据来源：

```text
1. Provider preset 提供初始能力声明（如 DeepSeek preset 声明 upstream.streaming = supported）。
2. 用户可在 Provider Edit Modal 中 override 能力值。
3. 自动化测试写入带 adapter 版本、时间和 evidence 的 conformance cache，不改写 provider 声明。
4. Health check 只更新健康观测，不改变能力定义。
5. UI 必须区分 declared、verified、user override 和当前 health。
```

UI warning 应表达适配层兼容性，而不是说上游不支持：

```text
This provider uses a local protocol adapter. DeepSeek supports streaming and tool calls, but runtime behavior depends on LAM's Responses-to-Chat-Completions compatibility for this provider.
```

## 7. 运行时规划

核心服务新增 `ProviderRuntimePlanner`：

```text
fn build_runtime_plan(provider, selected_model) -> ProviderRuntimePlan
```

输出：

```text
ProviderRuntimePlan
  provider_id
  selected_model
  codex_base_url
  codex_wire_api = responses
  codex_auth: env_key | auth_command | none
  upstream_base_url
  upstream_protocol
  adapter_required
  adapter_id
  gateway_binding_id?
  effective_capabilities
  warnings[]
```

规则：

- Responses Provider：
  - `codex_base_url = provider.base_url`
  - `adapter_required = false`
  - Codex config 直接写上游。
- Chat Completions + adapter：
  - `codex_base_url = local_gateway_url`
  - `adapter_required = true`
  - 上游地址只保存在 LAM provider store。
- Chat Completions 未启用 adapter：
  - runtime plan 返回 `PROVIDER_ADAPTER_REQUIRED`
  - 可保存但不可 attach/run。

这样 attach、test、UI、relay 都依赖同一个 runtime plan，避免重复条件判断。Runtime plan 是纯计算结果，不启动 Gateway、不读取 secret 值、不写文件。具体副作用由 attach executor 按 plan 执行，便于 dry-run 和幂等验证。

Runtime Planner 必须验证：

- `selected_model` 存在于 `models`，或被明确标记为用户自定义模型。
- Provider base URL 是合法 `http/https` URL，不包含 userinfo；非本地上游默认要求 `https`。
- `upstream_path` 使用 URL path join，不允许 scheme、host、`..` 或查询参数注入。
- adapter id 已注册，且 source/target protocol 与 Provider/Codex 匹配。
- 认证方式互斥，且当前运行路径可以消费该 SecretReference。

## 8. Codex 配置写入

新增 `CodexConfigEditor`，替代当前字符串拼接和整文件覆盖写法。

### 8.1 Responses 直连

```toml
model = "<model>"
model_provider = "<provider_id>"

[model_providers.<provider_id>]
name = "<display_name>"
base_url = "<provider_base_url>"
env_key = "<ENV_KEY>"
wire_api = "responses"
```

Codex 还支持以下可选字段：

```toml
[model_providers.<provider_id>]
http_headers = { "User-Agent" = "LAM" }
query_params = { version = "2024" }
requires_openai_auth = false
```

`CodexConfigEditor` 应支持写入这些可选字段。`http_headers` 用于非敏感自定义请求头，`query_params` 用于 URL 查询参数（如 Azure 的 `api-version`），`requires_openai_auth` 只在选择 OpenAI 认证模式时写入。

静态 `http_headers` 不得包含 `Authorization`、`Proxy-Authorization`、`X-API-Key` 等敏感值。需要从环境注入 header 时使用 Codex 支持的 `env_http_headers`，store 只保存 header 名与环境变量名。

### 8.2 Chat Completions 适配

Codex 仍只看到 Responses endpoint：

```toml
model = "<model>"
model_provider = "<provider_id>"

[model_providers.<provider_id>]
name = "<display_name>"
base_url = "http://127.0.0.1:<gateway_port>/v1"
wire_api = "responses"

[model_providers.<provider_id>.auth]
command = "<lam-auth-helper>"
args = ["gateway-token", "--binding", "<binding-id>"]
```

真实上游保存在 LAM store：

```text
provider.base_url = https://api.deepseek.com
provider.protocol = chat_completions
provider.adapter = local(chat_completions_to_responses)
provider.secret.env_key = DEEPSEEK_API_KEY
```

### 8.3 写入安全

- 读取现有 `config.toml`，用 `toml_edit` 只修改 `model`、`model_provider` 和目标 `[model_providers.<id>]`。
- 保留现有 MCP、sandbox、approval、reasoning、未知字段和注释。
- 写前备份 `config.toml`，并记录原文件 hash 以检测并发修改。
- 编辑后用 TOML parser 重新解析，写入同目录临时文件，flush/fsync 后原子 rename。
- 校验、并发检测或写入失败时原文件保持不变。
- attach 重复执行必须幂等；已存在的 provider table 含非 LAM 管理内容时必须保留或报冲突。
- 禁止 provider id 使用保留 ID：`openai`、`ollama`、`lmstudio`。
- `env_key` 只写变量名。
- 写给 Codex 的 `wire_api` 始终是 `responses`，不透传 legacy `openai` 值。

### 8.4 现有格式迁移

当前代码使用扁平 TOML key 写入 config.toml：

```toml
# 旧格式（已废弃）
model = "gpt-5.4"
model_provider = "company-proxy"
provider_base_url = "https://proxy.example.test/v1"
provider_wire_api = "responses"
env_key = "COMPANY_PROXY_API_KEY"
```

Codex 官方要求 `[model_providers.<id>]` section 格式：

```toml
# 新格式（Codex 官方）
model = "gpt-5.4"
model_provider = "company-proxy"

[model_providers.company-proxy]
name = "Company Proxy"
base_url = "https://proxy.example.test/v1"
env_key = "COMPANY_PROXY_API_KEY"
wire_api = "responses"
```

迁移策略：

- 新写入的 config.toml 一律使用新格式。
- 读取时：`parse_codex_config` 立即改用 TOML parser，通过顶级 key（`model`、`model_provider`）获取绑定信息，并能正确处理 `[model_providers]` table。
- 移除 `parse_toml_like_string` 对 provider 配置的依赖；旧扁平 key 只作为可识别迁移输入，不再写回。
- Attach 操作时会备份旧 config.toml，然后对现有 TOML 做最小化结构编辑，不整体覆盖。

### 8.5 Attach 事务边界

Attach executor 按事务日志执行，不把“已写 secret、未写 metadata”或“已写 config、未创建 binding”留为正常状态。

```text
1. build + validate runtime plan
2. acquire provider/config transaction locks
3. prepare secret reference or gateway binding
4. prepare edited config in temporary file
5. validate all prepared artifacts
6. commit metadata/binding/config in a journaled order with compensating rollback
7. on failure, revoke newly created binding, remove newly created secret when safe, and restore config
```

dry-run 执行全部纯验证和内存中的 artifact 生成，但不写 secret、binding、journal 或文件；返回预计操作、冲突、备份路径和脱敏后的配置摘要。恢复时以 transaction journal 为准，不根据当前文件状态猜测。

## 9. 本地 Gateway

### 9.1 端口与监听

- 只监听 `127.0.0.1`。
- 首次初始化时从保留范围选择一个安装级稳定端口，持久化到 gateway state；日常重启不自动变更。
- 端口被非 LAM 进程占用时返回 `GATEWAY_PORT_IN_USE`，不静默切换端口和批量改写 profile。
- 更换端口是显式管理操作，必须先 dry-run，然后以事务清单更新所有受影响 config，任一写入失败即回滚。
- Gateway 可按需启动：只有存在 attached Chat Completions adapter profile 时必须运行。

### 9.2 本地鉴权

Codex config 通过 profile-specific auth helper 取得 Gateway token。Gateway 校验 bearer token：

```text
Authorization: Bearer <token>
```

token 存在 Keychain 或 LAM 私有文件，文件权限 `0600`。每个 attached profile 使用独立 token/binding，store 只持久化 token hash 和密钥引用。token 必须可轮换和撤销，detach 后立即失效。鉴权比较使用 constant-time compare，失败日志不输出 token 或请求 body。

### 9.3 路由

第一组必须实现：

```text
GET  /healthz
GET  /v1/models
POST /v1/responses
POST /v1/responses/{id}/cancel   optional if Codex needs it
```

后续可给本地其他客户端提供：

```text
POST /v1/chat/completions
```

但 Codex profile 接入的最小必需入口是 `/v1/responses`。

### 9.4 Provider 选择

避免在请求 body 里放 secret 或 provider 明文策略。推荐由 token 或 path 解析 provider binding：

```text
GatewayBinding
  token_hash
  binding_id
  profile_id
  provider_id
  model
  created_at
  revoked_at?
```

Codex profile 调用 gateway 时，gateway 根据 token binding 找到 provider 和上游 secret 引用。

### 9.5 生命周期管理

Gateway 作为独立 sidecar 运行，不与 Tauri 窗口或前端页面生命周期绑定。原因是 attached Codex profile 在 LAM UI 关闭后仍应可用。Tauri 应用是管理者和 supervisor，Gateway 不直接共享内存 `AppState`，而是通过最小化的私有 state store 和窄 IPC/control endpoint 获取配置。

生命周期规则：

```text
启动：
- LAM 启动、首次 attach 或 profile launcher 发现 Gateway 不可用时，通过单实例锁幂等地拉起 sidecar。
- sidecar 只有在获得单实例锁后才能绑定端口；其他实例复用已运行 Gateway。
- 无 attached adapter profile 时不自动启动。

端口管理：
- 使用 9.1 定义的安装级稳定端口。
- 多 provider 共享同一 Gateway 进程，通过 token binding 区分。

监控：
- LAM 打开时监控 sidecar 进程和 `/healthz`；profile launcher 在启动 Codex 前做一次 readiness check。
- 重启使用有上限的指数退避和 jitter；连续失败后报告 `GATEWAY_HEALTH_FAILED`，不无限重启。

关闭：
- 所有 adapter profile detach 后，Gateway 在无 in-flight request 且经过固定 idle grace period 后退出。
- LAM UI 退出不终止仍有 attached binding 的 Gateway。

崩溃恢复：
- LAM supervisor 和 profile launcher 都可以幂等拉起 sidecar，保留稳定端口。
- UI 显示 Gateway 状态变更（available -> restarting -> available）。
```

### 9.6 多轮对话状态模型

首版 Gateway 优先保持无状态，但这是经 contract test 验证后的能力限制，不是对 Codex 行为的未验证假设。

```text
- 在锁定支持范围前，用当前支持的 Codex 版本捕获普通文本、streaming、tool call 和 resume 请求 fixture。
- 若所有支持路径都传完整 input items，首版不持久化 response history。
- 若收到 `previous_response_id`，返回结构化 `ADAPTER_UNSUPPORTED_FIELD`，错误中声明当前 adapter 版本和可恢复建议；不静默忽略。
- 若 Codex contract fixture 表明关键路径依赖 `previous_response_id`，则在发布前增加有界 response store、TTL、容量限制和清理策略。
```

### 9.7 资源、网络与日志边界

- 对 request body、单个 SSE frame、tool argument 累计大小、并发请求数、上游连接数和 response 时长设置有界限制。
- 上游 URL 只能来自已校验 ProviderProfile；请求 body、model 或 header 不得改变 upstream host。
- 重定向默认关闭；若开启，每一跳都必须重新执行 scheme/host 策略和最大跳数检查。
- Gateway 不开启 CORS，除非后续有明确的本地浏览器客户端需求和 origin allowlist。
- 默认日志只包含 request id、binding/provider id、status、latency、脱敏错误码和 token usage；不记录 prompt、tool arguments、response text、Authorization、secret 或 auth helper stdout/stderr。
- provider id、model 和 URL 作为日志字段前移除控制字符，防止 log injection。

## 10. Adapter 设计

### 10.1 Trait

Rust 侧定义协议适配契约：

```rust
trait ProtocolAdapter {
    fn id(&self) -> &'static str;
    fn source_protocol(&self) -> ProviderProtocol;
    fn target_protocol(&self) -> ProviderProtocol;
    fn capabilities(&self) -> AdapterCapabilities;
    fn translate_request(&self, request: ResponsesRequest) -> Result<UpstreamRequest>;
    fn translate_response(&self, response: UpstreamResponse) -> Result<ResponsesResponse>;
    fn translate_stream_event(&self, event: UpstreamStreamEvent) -> Result<Vec<ResponsesStreamEvent>>;
}
```

业务层通过 registry 获取 adapter：

```rust
adapter_registry.get(plan.adapter_id)
```

不在业务流程里写一堆协议分支。

Adapter 只做纯协议转换；HTTP 请求、secret 解析、retry、timeout、日志和取消属于 Gateway/upstream client。Provider-specific 策略通过类型化 `CompatibilityPolicy` 组合进 adapter，不使用任意字符串和分散分支。

### 10.2 Chat Completions 转换范围

首版支持的 Responses 子集必须用显式矩阵锁定：

| Responses 能力 | 首版策略 |
| --- | --- |
| string input / user-developer-system messages | translated |
| text output | supported |
| function tools | translated |
| `function_call` / `function_call_output` | translated，保留 `call_id` |
| streaming text/tool delta | translated |
| usage | translated，无法对齐的细分字段标记 unknown |
| reasoning summary | provider-dependent partial |
| structured output | 仅在上游和 adapter conformance 通过时支持 |
| image/audio/file input | `ADAPTER_UNSUPPORTED_INPUT_TYPE` |
| hosted tools / MCP / computer use | `ADAPTER_UNSUPPORTED_TOOL_TYPE` |
| `previous_response_id` | 按 9.6 contract 结果决定，不得忽略 |
| unknown request field | reject or documented preserve，不静默丢弃 |

每个 adapter 版本必须导出该矩阵，Runtime Plan 和 UI 使用同一份数据。首版交付范围：

- 非流式 text response。
- 流式 SSE：`choices[].delta` -> Responses stream events。
- tool calls：Chat Completions `tool_calls` 与 Responses tool call item 双向映射。
- usage：input/output/total token 映射。
- error：HTTP status、error code、message、retryability 映射。
- cancellation/timeout：上游中断要映射成明确 Responses error/event。

流式错误处理：

```text
- 上游连接中断：emit response.failed event with ADAPTER_UPSTREAM_DISCONNECTED。
- 畸形 SSE frame 或无法解析的 JSON：终止当前 response 并 emit response.failed，不 skip 后继分片。
- tool call arguments 分片中途中断：丢弃未完成调用并 emit response.failed，不伪造非标准 item status。
- 上游 5xx 错误中断 stream：emit response.failed event，附带 upstream status code。
- 客户端断开：立即传播 cancellation 给上游请求。
- 背压处理：使用有界 channel，容量由压力测试确定并作为配置常量；溢出时返回 `GATEWAY_BUFFER_OVERFLOW`。
```

流式转换使用显式状态机，而不是 handler 内的临时分支：

```text
created
  -> output_item_added
  -> content_part_added / function_arguments_delta
  -> content_part_done / function_arguments_done
  -> output_item_done
  -> completed | failed | cancelled
```

状态机必须定义 response/item/call ID 的稳定生成、事件顺序、tool arguments 分片重组、`finish_reason`、末尾 usage、客户端取消和部分输出后失败的行为。retry 不得向客户端重复已发送事件；一旦对外发送输出 delta，默认不自动重放上游请求。

Reasoning：

- 能映射的 reasoning content 标为 `supported` 或 `partial`。
- 无法可靠映射 encrypted reasoning/signature 的字段不得伪造。
- UI 和 health 显示 adapter reasoning support 级别。

### 10.3 DeepSeek Provider preset

DeepSeek 是 preset，不是硬编码：

```text
id suggestion: deepseek
name: DeepSeek
protocol: chat_completions
base_url: https://api.deepseek.com
upstream_path: /chat/completions
adapter: local(chat_completions_to_responses)
models:
  - deepseek-v4-flash
  - deepseek-v4-pro
secret.env_key: DEEPSEEK_API_KEY
```

模型列表要允许用户手动编辑，因为供应商模型会变化。

### 10.4 OpenAI 官方格式兼容

Adapter 必须以 OpenAI 官方 API 形态作为内部标准，而不是自造轻量格式。

对 Codex/Gateway 入站：

- 接收 OpenAI Responses API 风格的 request、response、stream event。
- 支持 Responses 的 output item、tool call item、tool result、usage、status、error 语义。
- 不支持的 Responses 字段必须显式保留、降级或返回结构化 `ADAPTER_UNSUPPORTED_FIELD`，不能静默丢弃。

对 OpenAI-compatible Chat Completions 上游：

- 发送 OpenAI Chat Completions 风格的 `messages`、`tools`、`tool_choice`、`stream`、sampling 参数。
- 解析 `choices[].message.content`、`choices[].message.tool_calls`、`choices[].delta`、`finish_reason`、`usage`、error body。
- 流式输出必须处理 `delta.content`、`delta.tool_calls`、tool call arguments 分片、`finish_reason` 和 `[DONE]`。

迁移映射必须有单独测试文件，不能把规则隐藏在 gateway handler 里：

```text
ResponsesRequest <-> ChatCompletionsRequest
ResponsesResponse <-> ChatCompletionsResponse
ResponsesStreamEvent <-> ChatCompletionsStreamEvent
ResponsesToolCall <-> ChatCompletionsToolCall
ResponsesUsage <-> ChatCompletionsUsage
ResponsesError <-> ChatCompletionsError
```

### 10.5 DeepSeek thinking mode 特殊适配

DeepSeek thinking mode 不是普通 Chat Completions 的简单文本返回。LAM 必须对 DeepSeek preset 增加 provider-level compatibility profile，但不能在核心流程里散落 `if deepseek`。

推荐抽象：

```text
ProviderCompatibilityProfile
  id: deepseek_chat_completions
  protocol: chat_completions
  reasoning_source: reasoning_content
  request_history_policy: preserve_reasoning_for_tool_call_turns
  tool_call_policy: deepseek_thinking_tool_call
  stream_policy: openai_chat_delta_with_reasoning
```

DeepSeek thinking mode 适配规则：

- 如果上游返回 `reasoning_content`，adapter 要把它映射到 Responses reasoning/summary 兼容字段；无法完整表示时标为 `partial`，不得伪造 encrypted reasoning 或签名字段。
- 继续多轮请求时，发送给 DeepSeek 的历史消息必须符合 DeepSeek 官方样例：不含 tool call 的已完成轮次可省略旧 `reasoning_content`；包含 tool call 的 assistant message 必须在后续请求中完整保留 `reasoning_content`，否则上游可能返回 400。任何情况下都不得把 reasoning 拼入普通 `content`。
- DeepSeek thinking + tool call 样例中，assistant 可能先产生 thinking，再产生 `tool_calls`；adapter 要保留 tool call 的 id、name、arguments，并把它们转换为 Responses tool call item。
- tool result 回传上游时，必须生成 Chat Completions `role = "tool"` 消息，并绑定正确 `tool_call_id`。
- 如果 Responses 请求包含 DeepSeek 无法支持或无法安全映射的 tool 类型，返回 `ADAPTER_UNSUPPORTED_TOOL_TYPE`，不要静默移除工具。
- 流式 thinking 内容如果以 DeepSeek 扩展字段出现，要进入 reasoning stream 兼容事件；如果 Codex 当前 Responses stream 不能表达该字段，则以 capability `reasoning = partial` 和结构化 warning 暴露。

DeepSeek compatibility profile 只负责 DeepSeek 特殊字段和历史策略，不负责通用协议转换。通用转换仍在 `chat_completions_to_responses` adapter 中完成。

### 10.6 Anthropic-compatible 预留

DeepSeek 也提供 Anthropic-compatible 相关接口时，不要把 Anthropic 映射塞进 Chat Completions adapter。预留独立 adapter：

```text
anthropic_messages_to_responses
```

当用户选择 Anthropic-compatible 协议时，runtime plan 选择对应 adapter。这样 DeepSeek 的 OpenAI-compatible 和 Anthropic-compatible 两种接入可以共存，但实现边界清楚。

### 10.7 错误契约

内部错误和 Gateway HTTP/Responses 错误使用同一分类，但不向客户端暴露 upstream body、secret、本地文件路径或 command stderr。

```text
GatewayError
  code
  category: validation | auth | capability | upstream | timeout | cancelled | internal
  message                 # 脱敏、面向用户
  retryable
  upstream_status?        # 仅状态码
  request_id
```

约定：

- 4xx 上游错误默认不重试；401/403 标记 auth，429 只在遵守 `Retry-After` 和最大尝试数时可重试。
- 5xx/连接错误只在尚未向客户端发送输出事件时可尝试有界重试。
- 不支持字段或 tool type 是 capability/validation 错误，不应伪装成 upstream 500。
- 每个错误 code 必须有一个稳定的测试和 UI 恢复建议映射。

## 11. UI 变更

### 11.1 Provider Center

Provider card 展示：

- protocol
- base URL
- default model
- models count
- secret mode
- upstream health
- gateway health
- adapter status
- used profiles
- capability matrix summary

动作：

- Add
- Edit
- Test upstream
- Test Codex route
- Attach
- Delete
- Start/Stop gateway when needed

### 11.2 Add/Edit Provider Modal

字段：

```text
Provider id
Name
Protocol
Base URL
Default model
Models
Secret storage
Env key / Keychain input / Auth command
Use local adapter proxy
Adapter upstream path
```

交互规则：

- Responses：隐藏或禁用 adapter checkbox。
- Chat Completions：展示 adapter checkbox。
- Chat Completions 未启用 adapter：显示“可保存，但不能作为 Codex runtime attach”。
- Chat Completions 启用 adapter：显示最终 Codex route preview。

### 11.3 Attach Provider Modal

Attach 前展示 runtime plan：

```text
Profile: ~/.codex-deepseek
Codex endpoint: http://127.0.0.1:<port>/v1
Upstream endpoint: https://api.deepseek.com/chat/completions
Model: deepseek-v4-pro
Secret: env(DEEPSEEK_API_KEY)
Adapter: chat_completions_to_responses
```

必须先 dry-run，dry-run 展示备份路径、写入内容摘要、blocked items。

### 11.4 Relay/Session UI

Session detail 增加：

- original provider/model
- current runtime provider/model
- provider protocol
- adapter enabled
- capability mismatch

警告示例：

```text
This handoff changes runtime from OpenAI Responses to DeepSeek through LAM adapter. Session context will move, but auth identity, billing, quota, and model behavior will follow the target profile.
```

### 11.5 API DTO 与前后端兼容

持久化模型、领域模型和 Tauri API DTO 不共用同一个 struct，避免 store 迁移字段意外暴露给前端。

```text
LegacyCreateProviderRequest   # 只在迁移窗口内读取 wireApi/envKey
CreateProviderRequestV2      # protocol/adapter/secret input
ProviderProfileView          # 只包含 secret reference，不包含 secret value
ProviderRuntimePlanView      # dry-run 预览
```

兼容规则：

- `CreateProviderRequestV2` 使用 `protocol`，不再让新 UI 提交任意 `wireApi` 字符串。
- legacy `wireApi = responses | openai` 在 API 边界转成 `protocol = responses`，并产生 migration warning；其他值拒绝。
- 新后端 schema 上线的同一切片必须同步更新 UI 最小表单和前端 type，不允许先删除 `wireApi/envKey/secretStorage` 后将 UI 推迟到后续 Phase。
- 兼容窗口的删除必须有版本化迁移说明和 contract test，不无期保留 legacy 字段。

## 12. 文件与模块改动

### 12.1 Rust core

现有：

- `apps/desktop/src-tauri/src/services/provider.rs`
- `apps/desktop/src-tauri/src/services/types.rs`
- `apps/desktop/src-tauri/src/services/session.rs`
- `apps/desktop/src-tauri/src/services/relay.rs`
- `apps/desktop/src-tauri/src/commands/mod.rs`

新增建议：

```text
apps/desktop/src-tauri/src/services/provider/
  mod.rs
  model.rs
  store.rs
  validation.rs
  runtime_plan.rs
  config_editor.rs
  secret_store.rs
  health.rs

apps/desktop/src-tauri/src/services/gateway/
  mod.rs
  server.rs
  routes.rs
  binding.rs
  auth.rs
  logging.rs

apps/desktop/src-tauri/src/bin/
  lam-provider-gateway.rs       # 独立 sidecar 入口，复用 services/gateway

apps/desktop/src-tauri/src/services/adapters/
  mod.rs
  registry.rs
  responses_passthrough.rs
  chat_completions.rs
  errors.rs
  schema.rs
```

如果不想一次拆太多文件，可以先用同样的模块边界建立子模块，再迁移代码。关键是边界清晰，不把 gateway、provider store、config editor 混在一个大文件里。

`provider.rs -> provider/` 的纯机械拆分与 schema/行为变更分开提交和验证：先在所有现有测试不变的情况下移动代码，再按纵向切片修改契约。不在一个大 diff 中同时完成文件拆分、store 迁移、API DTO 替换和 attach 行为改造。

### 12.2 Frontend

现有：

- `apps/desktop/src/lib/types.ts`
- `apps/desktop/src/lib/api.ts`
- `apps/desktop/src/stores/providers.ts`
- `apps/desktop/src/routes/views.tsx`
- `apps/desktop/src/App.tsx`

建议新增：

```text
apps/desktop/src/components/provider/
  provider-form.tsx
  provider-card.tsx
  capability-matrix.tsx
  runtime-plan-preview.tsx
  provider-health.tsx
```

`App.tsx` 中 modal 逻辑应逐步下沉到组件，避免继续膨胀。

### 12.3 Rust 依赖变更

依赖按纵向切片延迟引入，不在 schema-only Phase 预先加入未使用的 Gateway 依赖。预期依赖：

```toml
# Remote Provider Gateway dependencies
tokio = { version = "1", features = ["rt", "net", "sync", "macros"] }
axum = "0.7"
reqwest = { version = "0.12", default-features = false, features = ["json", "stream", "rustls-tls"] }
toml = "0.8"
toml_edit = "0.22"
tokio-stream = "0.1"
tower = "0.5"
tower-http = { version = "0.6", features = ["trace", "cors"] }
rand = "0.8"
```

测试依赖：

```toml
[dev-dependencies]
wiremock = "0.6"
```

选型理由：

- `axum`：独立 Gateway sidecar 的 Tokio 生态 HTTP server。
- `reqwest`：async HTTP client，支持 streaming 和 SSE，用于上游代理和健康检查。
- `toml` + `toml_edit`：结构化 TOML 校验和保留注释的非破坏性编辑，替代当前的 `format!()` 拼接。
- `tower` + `tower-http`：HTTP 中间件（日志、CORS、超时），axum 原生支持。
- `wiremock`：集成测试 mock HTTP server，支持请求匹配和响应模板。
- `rand`：Gateway bearer token 生成。

## 13. 自动化测试设计

测试必须先写，且每个变更源文件有对应测试或明确映射测试。

### 13.1 Rust 单元测试

Provider model/store：

- 创建 Responses Provider 正常。
- 创建 Chat Completions Provider 不启用 adapter 可保存但不可 attach。
- 创建 Chat Completions Provider 启用 adapter 可生成 runtime plan。
- 旧 provider store 可迁移读取。
- legacy array store -> versioned envelope 的 fixture 迁移是确定性的，读取本身不写回。
- 未知未来 `schema_version` 可读错误但不可被覆盖。
- revision 不匹配时返回结构化冲突，不丢失其他实例的更新。
- 原子写入失败不破坏旧 store。
- reserved provider id 返回结构化错误。
- 缺少 base_url/model/secret reference 返回结构化错误。

Config editor：

- Responses Provider 写官方 `[model_providers.<id>]` TOML。
- Chat Completions adapter 写本地 gateway base URL。
- `env_key` 只写变量名，不写 secret。
- 写前备份，写后 TOML parser 可解析。
- 解析失败时不破坏原 config。
- 现有 MCP、sandbox、approval、未知 table 和注释在 attach 后保留。
- 并发 hash 冲突时不写入；重复 attach 结果幂等。
- legacy `wire_api = openai` 读入后只向 Codex 写 `responses`。
- auth table 与 `env_key/requires_openai_auth` 冲突时阻止写入。

Secret store：

- env mode 只检查变量存在，不返回 secret。
- keychain 空 secret 不写 metadata。
- auth command 和 env_key 互斥。
- keychain Responses 直连会生成可用的 Codex auth helper 配置。
- gateway binding 每 profile 独立，detach 后 token 失效，轮换后旧 token 失效。
- auth helper timeout、超大 stdout、非零退出和多余输出均返回结构化错误。
- debug format 不包含 secret。

Runtime planner：

- responses -> direct plan。
- chat_completions + adapter -> gateway plan。
- chat_completions without adapter -> `PROVIDER_ADAPTER_REQUIRED`。
- capability resolver 正确汇总 declaration/verification/override warning。

Adapter：

- Responses request text input -> Chat Completions messages。
- Chat Completions non-stream response -> Responses response。
- Chat Completions stream delta -> Responses stream events。
- tool call request/response roundtrip。
- DeepSeek `reasoning_content` -> Responses reasoning/summary partial mapping。
- DeepSeek thinking tool call 样例：assistant thinking + `tool_calls` -> Responses tool call item。
- DeepSeek tool result replay：Responses tool result -> Chat Completions `role = "tool"` with `tool_call_id`。
- DeepSeek 普通完成轮次可省略旧 reasoning，但含 tool call 的 assistant message 必须完整回放 `reasoning_content`。
- 不把 `reasoning_content` 当作普通 assistant `content` 回放。
- upstream error -> structured Responses error。
- malformed upstream response/SSE -> 终止当前 response，不继续组装 tool arguments。
- usage 字段映射。

Gateway：

- `/healthz`。
- `/v1/responses` auth required。
- token binding 选择正确 provider。
- prompt/body/secret 不进入日志。
- upstream timeout 返回结构化错误。
- only loopback binding。
- 单实例锁防止两个 sidecar 同时管理同一 state。
- LAM UI 退出后，已 attached profile 的 Gateway 仍可用。
- 端口冲突不自动改写 profile；显式 rebind 中任一 config 失败时回滚。
- 客户端取消能中止上游请求。

Relay/session：

- API profile 可以作为 relay target。
- API profile 可以作为 relay source。
- target provider/model 决定 resume runtime。
- provider mismatch warning 准确。
- sync/relay 不复制 `auth.json`、API secret、provider store secret。

### 13.2 Rust 集成测试

使用本地 mock HTTP server：

- mock Responses Provider：验证 LAM direct config + test route。
- mock Chat Completions Provider：验证 gateway 转换请求、返回文本。
- mock streaming：验证 SSE 转换事件顺序。
- mock tool calls：验证 tool call id、name、arguments、result 映射。
- mock DeepSeek thinking mode：验证 `reasoning_content`、tool call、tool result、普通轮次省略与 tool-call 轮次完整保留。
- mock upstream 401/429/500：验证 health 和错误分类。

### 13.3 Frontend 测试

Provider form：

- Responses protocol 禁用 adapter checkbox。
- Chat Completions 显示 adapter checkbox。
- Chat Completions 未启用 adapter 时 attach 按钮 disabled 或 dry-run 返回阻塞。
- 启用 adapter 时显示 Codex route preview。

Provider card：

- 展示 upstream/gateway health。
- 展示 capability matrix。
- secret 不显示明文。

Attach flow：

- dry-run 展示 backup/config/gateway/upstream 摘要。
- execute 后刷新 accounts/providers/sessions。

Relay/session：

- provider mismatch 文案包含 original/current provider/model。
- adapter profile 的 warning 指向适配层兼容性，不说 DeepSeek 不支持 streaming/tools。

### 13.4 Smoke 测试

更新 `apps/desktop/scripts/ui-smoke.mjs`：

- Provider Center 有 protocol selector。
- 有 adapter checkbox。
- 有 runtime plan preview。
- 有 capability matrix。
- Tauri commands 暴露 gateway start/status/test。

### 13.5 安全测试

- `providers.json` 不包含 `sk-`、DeepSeek key、Bearer token。
- `config.toml` 不包含 secret。
- gateway logs 不包含 request body、prompt、tool arguments、secret。
- sync manifest 不包含 secret。
- command-backed auth helper stdout 只输出 token，stderr 不含 token。

## 14. 验收标准

### 14.1 Responses Provider

- 用户创建 Responses Provider。
- LAM 写入官方 Codex provider config。
- 原有 MCP、sandbox、approval、未知配置和注释保留不变。
- `codex` 在该 profile 下可以完成普通 prompt。
- 不启动本地 adapter。
- health 显示 upstream available。

### 14.2 DeepSeek Chat Completions Provider

- 用户创建 DeepSeek Provider：

```text
protocol = chat_completions
base_url = https://api.deepseek.com
adapter = local(chat_completions_to_responses)
env_key = DEEPSEEK_API_KEY
model = deepseek-v4-pro
```

- LAM 启动本地 gateway。
- Codex config 指向 `127.0.0.1` gateway。
- Gateway 将 `/v1/responses` 转为 DeepSeek `/chat/completions`。
- 普通文本、streaming、tool calls 至少各有一个自动化测试通过。
- thinking tool-call 多轮测试证明 `reasoning_content` 在必需轮次完整回传。
- 关闭 LAM UI 后，已 attached profile 仍能通过 sidecar 完成请求。
- UI 显示 DeepSeek 上游支持 streaming/tools，同时显示 adapter 对这些能力的验证状态。

### 14.3 Chat Completions 未启用 adapter

- 用户可保存 provider。
- Provider card 清楚显示不可作为 Codex runtime attach。
- Attach dry-run 返回 `PROVIDER_ADAPTER_REQUIRED`。
- 不写 Codex config。

### 14.4 Relay

- ChatGPT auth profile -> DeepSeek API profile 可以 relay/resume。
- DeepSeek API profile -> ChatGPT auth profile 可以 relay/resume。
- 接力后运行身份、计费、quota、provider/model 均取目标 profile。
- 不复制 `auth.json`、API key、gateway token。

### 14.5 本地统一管理

- Provider Center 可切换 active provider/model。
- Gateway status 可见。
- Codex profile 绑定和本地 gateway 使用同一套 Provider store、SecretStore 和 capability resolver。

## 15. 任务拆分

本节取代旧的“基础层一次扩展所有 struct” Phase 1 方案。向后兼容保证放在持久化迁移和 Tauri JSON DTO 边界，不要求 Rust 内部 struct 字段和函数参数永久不变。新契约和 legacy 输入通过独立 DTO 共存，领域模型只表达新语义。

Phase 0（契约与安全基线）：

1. 固定 legacy provider store、现有 Codex config 和前后端 JSON contract fixtures。
2. 用当前支持的 Codex 版本捕获 text、streaming、tool call、resume 请求，形成 Gateway contract fixtures。
3. 先写失败测试：现有 config 不可被整体覆盖、secret 不可序列化、legacy `openai` 不可写给 Codex。

Phase 1（Responses Provider 直连闭环）：

4. 实现 versioned ProviderStore、纯迁移函数、原子写入和并发冲突检测。
5. 实现 SecretReference/SecretStore 与 env、Keychain、Codex auth helper 映射。
6. 实现纯 `ProviderRuntimePlanner` 和非破坏性 `CodexConfigEditor`。
7. 同步更新最小 Provider UI 和前后端 DTO，保持 legacy request 的迁移入口。
8. 用 mock Responses provider 完成 create -> dry-run -> attach -> Codex config -> test route 纵向验收。

Phase 2（纯 Adapter 转换闭环）：

9. 实现受控 Responses 子集 schema、Adapter trait/registry 和显式 streaming 状态机。
10. 实现 text、streaming、function tool、usage 和 error 转换，每项先增加 fixture test。
11. 实现 DeepSeek compatibility policy，覆盖 thinking + tool call 中 `reasoning_content` 必须保留的回归样例。

Phase 3（Gateway 可运行闭环）：

12. 实现 sidecar、单实例锁、稳定端口、auth helper、binding 轮换/撤销和安全日志。
13. 实现 `/healthz`、`/v1/models`、`/v1/responses` 和经 contract 证明必需的 cancel/state 路由。
14. 实现 upstream client 的 timeout、取消、有界背压和不重复已发送事件的 retry 策略。
15. 用 mock DeepSeek 完成 create -> attach -> sidecar start -> Codex Responses request -> Chat Completions -> streaming/tool result 的端到端验收。

Phase 4（产品集成与接力）：

16. 完成 Provider Center、runtime plan preview、readiness/health/capability provenance 展示。
17. 完成 Session/relay/provider mismatch 联动与 secret 不迁移验收。
18. 完成 smoke、安全测试、README 和旧 v0.2 文档同步。

MVP 里程碑：Phase 0–3 全部通过后，才算 DeepSeek 端到端演示闭环。Phase 1 单独交付一个可用的 Responses Provider 直连功能，不是只铺设抽象。

每个纵向切片必须先补测试并确认失败，再实现；自动化测试不得推迟到最后阶段。

## 16. 设计约束

- 不允许在核心流程里散落 provider-specific 分支。
- 供应商差异放在 provider preset、capability metadata 或 adapter 实现里。
- 协议差异放在 `ProtocolAdapter`。
- Codex 配置差异放在 `ProviderRuntimePlan` 和 `CodexConfigEditor`。
- UI 展示差异来自同一份 runtime plan/capability resolver。
- 所有错误使用结构化 `AppError` code，不能只返回字符串。
- 文件写入必须备份、解析校验、失败不破坏原文件。
- 所有网络测试使用 mock server；真实 Provider 只用于手工验收。

## 17. 后续扩展

新增 Anthropic-compatible Provider 时，不修改 attach/gateway/config editor 主流程：

1. 新增 `anthropic_messages_to_responses` adapter。
2. 注册到 `ProtocolAdapterRegistry`。
3. 新增 provider preset。
4. 补 capability tests。

新增本地客户端统一 API 时，不影响 Codex profile：

1. 在 gateway 增加 `/v1/chat/completions` 入口。
2. 用同一套 ProviderRuntimePlan 选择上游。
3. 继续复用 SecretStore 和 capability resolver。
