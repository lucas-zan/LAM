# LAM Remote Provider Gateway Full Design

状态：Phase 0 / G1、Phase 1 / G2、Phase 2 / G3、Phase 3 / G4 与 Phase 4 / G5 已实现；Codex 0.144.1 严格模型目录、上游模型发现已实现；原生 Responses API 已收敛为 Codex 直连，Gateway 只承担 Chat Completions 协议适配；验证证据见 `docs/todo-strict-codex-provider-models.md` 与 `docs/todo-native-responses-direct-routing.md`
目标：通过可独立验收的纵向切片，完整交付外部 API Provider 接入、协议适配、本地统一网关、Codex profile 绑定、自动化测试和验收闭环。

## 1. 目标

LAM 要从“Codex 账号/session 管理工具”扩展为本地 Provider Hub：

- 管理外部模型 Provider、模型列表、密钥引用、健康状态和能力矩阵。
- 原生 `/v1/responses` Provider 始终由 Codex 直连，保持与 Codex 原生 Provider 配置一致。
- 支持 `/chat/completions` Provider 通过用户显式启用的本地适配器接入 Codex。
- Chat Completions Provider 通过 adapter 走 Gateway；External API Account 根据协议选择直连或适配路径。
- API profile 与 ChatGPT auth/session profile 同等参与 session 浏览、sync、relay、resume。
- 本地网关可作为统一外部接口管理层，后续可开放给其他本机客户端复用。

这不是 DeepSeek 专用补丁。DeepSeek 只是一个 Chat Completions Provider 示例。设计必须让 Mistral、OpenAI-compatible router、Azure、Ollama、LM Studio、内部代理等 Provider 以配置和小型 adapter 扩展接入。

### 1.1 已确定的架构决策

- Codex 边界只输出 `wire_api = "responses"`；旧 `wireApi = "openai"` 只是迁移输入，不得写回 Codex 配置。
- Responses Provider 默认直连；Chat Completions Provider 只能通过显式启用的本地 adapter 接入 Codex。
- `codex.route_via_gateway` 只作为旧数据兼容输入；Responses Provider 在构建和启动迁移时统一规范为 `false`。Chat Completions Provider 的协议类型本身决定必须走 Gateway。
- 上游 OpenAI `GET /models` 的 `{ data: [{ id }] }` 只用于发现；Codex 0.144.1 使用顶层 `models` 和受控 metadata 字段。两种 DTO 不复用。
- Provider metadata、secret reference、运行时状态和测试观测结果分开存储，不用一个字段同时表示配置、健康和能力。
- `ProfileProviderBinding` 是 profile 与 provider/model 关系的唯一权威来源；Codex config 和 Gateway binding 只是它的可重建投影。
- `config.toml` 采用非破坏性编辑，只更改 LAM 管理的 key，不覆盖用户其他配置。
- Gateway 是可被 LAM launcher 拉起的独立 sidecar，不与前端窗口生命周期绑定；首版只保证通过 LAM launcher 启动的 adapter profile 可用。
- 首版只声称支持经过 contract test 验证的 Responses 子集，不声称完整实现 OpenAI Responses API。
- 初始 Codex 支持策略是 exact-tested：只声明 `codex-cli 0.144.1`、macOS 15.6、Darwin arm64；其他版本必须重跑同一 capture/verifier 后才能扩展为版本范围。
- Gateway 的 Codex 必需路由固定为 `GET /v1/models` 与 `POST /v1/responses`；首版不实现未被观测到的 retrieve/cancel/state route。
- text、function tool follow-up 与 resume 都由 Codex 发送完整 input history，首版 Gateway/adapter 不保存 response history。
- LAM 写给 Gateway profile 的 Codex config 显式设置 `request_max_retries = 0` 和 `stream_max_retries = 0`，避免 Codex 默认两层重试相乘。

## 2. 文档依据

本设计需要同时兼容 Codex、OpenAI API 和 DeepSeek API 的公开行为。实现前后都要以这些文档和样例作为协议依据：

- OpenAI Responses migration guide: <https://developers.openai.com/api/docs/guides/migrate-to-responses>
- Codex custom model providers: <https://learn.chatgpt.com/docs/config-file/config-advanced#custom-model-providers>
- Codex configuration reference: <https://learn.chatgpt.com/docs/config-file/config-reference#configtoml>
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
- `ProfileBindingStore`：保存 profile 到 provider/model 的唯一权威绑定及配置投影版本。
- `SecretStore`：按 env/keychain/auth command 解析 secret，不暴露 secret 值。
- `ProviderProtocol`：描述上游协议能力。
- `GatewaySupervisor`：只负责 sidecar 的安装路径、单实例、启停和 readiness。
- `ProviderGateway`：对外提供 Codex 可消费的 Responses endpoint，并在请求开始时获取不可变 binding snapshot。
- `OpenAiModelDiscovery`：只负责有界、无重定向的上游 `/models` 请求和 `data[].id` 规范化。
- `CodexModelCatalog`：只负责把 Provider allowlist 序列化为 exact-tested Codex 0.144.1 目录。
- `ProtocolAdapter`：在协议之间转换请求、事件、工具调用、错误和 usage。
- `CodexConfigEditor`：只负责非破坏性编辑、校验和原子写入 `$CODEX_HOME/config.toml`。

业务流程依赖这些契约，而不是依赖具体 DeepSeek/OpenAI/Azure 实现细节。

### 4.2 简洁

每个模块只有一个职责：

- Provider metadata 只处理数据和校验。
- Secret 只处理密钥读取和 helper 配置。
- Config editor 只修改 LAM 管理的 TOML key，并负责备份/回滚。
- Gateway server 只组合 HTTP 路由；鉴权、binding 查找、upstream client 和日志脱敏是独立组件。
- Adapter 只处理协议转换。
- Relay 只处理 session/context 迁移，不处理密钥迁移。

### 4.3 易扩展

新增协议或供应商时，优先新增一个 adapter 或 provider preset，而不是修改一串分散的条件分支。

推荐模式：

```text
ProtocolAdapterRegistry
  chat_completions -> responses_to_chat_completions adapter
  anthropic_messages -> responses_to_anthropic_messages adapter later
```

业务层只问：

```text
ProviderRoutePlan:
  codex_wire_api
  upstream_base_url
  adapter_required
  adapter_id
  capability_matrix

ProfileAttachPlan:
  profile_id
  codex_base_url
  codex_auth
  gateway_binding_spec?
  config_patch
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

### 6.0 规范类型与序列化边界

以下定义是 MVP 的闭集。持久化和 Tauri DTO 使用 snake_case 字符串；读取未知
枚举值返回 `UNSUPPORTED_SCHEMA_VALUE`，不得映射到默认值。领域类型不得实现会
输出 secret 值的 `Debug`/`Display`，API view 只序列化 credential reference。

```text
ProviderProtocol = responses | chat_completions
RouteKind = direct | gateway

ProviderModel
  id: string                    # 非空，上游原始 model id
  label: string                 # 非空，仅展示，不参与路由
  capabilities?: CapabilityDeclaration

CodexProviderOptions
  display_name?: string
  stream_idle_timeout_ms?: integer       # 1_000..=900_000
  direct_request_max_retries?: integer   # 0..=3，仅 direct
  direct_stream_max_retries?: integer    # 0..=3，仅 direct
  route_via_gateway: boolean             # 旧数据兼容字段；Responses 永远规范为 false
  query_params: map<string, string>      # 键值长度受限，禁止控制字符
  env_http_headers: map<string, EnvVarName>
```

`wire_api` 不是用户字段，planner 固定输出 `responses`。MVP 不接受持久化的
`http_headers`，因为静态 header 可能携带 secret；只允许引用环境变量的
`env_http_headers`。`query_params` 不允许认证材料、URL/userinfo 或重复 key。
Gateway route 无条件把两种 retry 写成 `0`；direct route 使用上述
显式值，未配置时由 exact-tested Codex 默认负责，UI 必须标明 retry owner。

认证拆成“凭据从哪里来”和“如何放到上游请求”两层：

```text
CredentialSource
  env { env_key: EnvVarName }
  keychain { service: "lam.remote-provider", account: string, version: integer }
  auth_command { command: AuthCommand }
  none

UpstreamAuth
  none
  bearer { credential: CredentialSource }
  header { name: HttpHeaderName, credential: CredentialSource }

AuthCommand
  executable: absolute_path
  args: string[]                 # 直接 argv，无 shell
  timeout_ms: integer            # 100..=5_000，默认 5_000
  max_stdout_bytes: integer      # 1..=16_384，默认 8_192
  refresh_interval_ms: integer   # 0..=86_400_000

GatewayCredentialReference
  binding_id: string
  secret_ref: SecretReference    # MVP 必须是 versioned keychain ref
  token_hash: string             # domain-separated SHA-256，非 token
  generation: integer

CodexAuth
  env_key { env_key: EnvVarName }
  auth_command { command: AuthCommand }
  none
```

`CredentialSource::none` 只能与 `UpstreamAuth::none` 组合。RPG-103 已将受控 header
认证加入闭集：header name 必须通过 HTTP token 校验，禁止 `Authorization`、
`Proxy-Authorization`、`Host`、`Content-Length`、`Connection` 和
`Transfer-Encoding`，值只能来自 credential source；静态 header/query 仍禁止携带
credential。Gateway 自身永远不使用 `CodexAuth::none`，只生成引用
`GatewayCredentialReference` 的 profile-specific auth helper。

`AuthCommand.executable` 必须是已批准的绝对、非 symlink 普通文件；`cwd` 固定为
空临时目录，stdin 为 null，环境只继承 allowlist。stdout trim 后必须是单个非空
token 且不超过上限；stderr 永不进入用户错误、日志或 cache。超时杀进程组；非零
退出、NUL/换行内嵌、多余输出均失败。token 只在内存缓存到
`refresh_interval_ms`，401 时最多失效并刷新一次；不落盘、不进入 crash report。

仅修改 Provider JSON 不足以授权 command execution。用户在 UI 明示启用时生成
`CommandApproval`：安装 identity key 对 provider id、绝对 executable、argv、binary
SHA-256、owner/mode 和环境 allowlist 的 canonical fingerprint 做 HMAC。每次运行重新
`open` executable、拒绝 symlink/group/world-writable/非当前 uid 文件并复核 hash/HMAC；
任一 metadata 或 binary replacement 返回 `AUTH_COMMAND_APPROVAL_REQUIRED`。MVP 环境
allowlist 固定为 `PATH`（系统安全默认值，不继承用户 PATH）、`HOME`（空临时 home）、
`TMPDIR`（私有临时目录）及用户逐项批准的非敏感变量名；禁止 `DYLD_*`、`LD_*`、
shell startup、working-directory 和 stdin 注入。

初始能力闭集为：

```text
CapabilityKind = text | streaming | function_tools | structured_outputs | reasoning
CapabilitySupport = supported | partial | unsupported | unknown

ReadinessBlocker
  invalid_metadata { fields[] }
  missing_secret { credential_kind }
  secret_unavailable { error_code }
  model_not_found { model_id }
  adapter_required
  adapter_unavailable { adapter_id }
  unsupported_platform { platform }
  gateway_unavailable { error_code }
  config_unmanageable { error_code }
  config_drift { managed_keys[] }
  binding_conflict { expected_revision, actual_revision }
  conformance_required { capability }
```

`ready = blockers.is_empty()`，blocker 顺序按上述枚举固定，数组字段排序后输出，确保
前后端 snapshot 稳定。`reasoning = partial` 只代表 adapter 能保留/受控降级相关
语义，不代表原始 chain-of-thought 已成为 Responses reasoning summary。

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
  credential: SecretReference
  upstream_auth: none | bearer
  capabilities: CapabilityDeclaration
  adapter: AdapterConfig
  compatibility_profile: string | null
  codex: CodexProviderOptions
  created_at: string
  updated_at: string
```

模型与 endpoint 不受隐式字符串拼接规则影响：

- `default_model` 必须引用 `models` 中的一项；用户输入自定义模型时，先将它加入 `models`，不使用隐式例外。
- `base_url` 是规范化的上游 API root，可包含供应商要求的 path prefix，例如 `https://proxy.example/v1` 或 `https://api.deepseek.com`。
- adapter 的 `upstream_path` 是相对于 `base_url` 的受控 path；组合时保留 `base_url` 的全部 path prefix，只规范化两者交界处的斜杠，不替换已有 path segment，且禁止字符串拼接。
- preset 必须存储最终可用的 `base_url + upstream_path` 组合，contract test 覆盖带/不带尾随斜杠的情况。

`used_by_profile_ids` 是 profile store 的派生视图，不持久化到 provider 条目，避免两份索引不一致。Readiness 也根据 metadata、secret availability、profile binding 和 Gateway state 动态计算。健康观测和 adapter 验证结果不嵌入 `ProviderProfile`，分别写入 runtime state store 和 conformance cache。

### 6.1.1 模型发现与目录边界

External API 新建流程显式调用 `discover_provider_models_v2`。它使用 write-only API key 请求 `GET <base_url>/models`，关闭系统代理和重定向，并限制 15 秒总超时、1 MiB 响应体和 2048 个模型。解析器只接受标准 OpenAI 形状：

```json
{ "object": "list", "data": [{ "id": "vendor-model-id" }] }
```

缺少 `data`、空/非法/duplicate id、空列表和超限响应都 fail closed。发现结果只是候选集，不自动持久或勾选；用户选中或手工添加的 id 才进入 `ProviderProfile.models` allowlist，默认模型必须属于该集合。Base URL 或 API key 变更会清空旧结果，过期异步响应不得覆盖新请求。

Gateway 不把这个 OpenAI DTO 直接返回 Codex。`GET /v1/models` 从不可变 binding snapshot 内的全部 Provider allowlist 生成 exact-tested Codex 目录；`selected_model` 只是 config 默认值，不会隐藏其他允许模型。

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

Provider edit 使用 `expected_provider_store_revision` 做 CAS，并分两类：

- 非路由字段：`name`、model `label`、capability declaration/user override 可在被绑定时
  修改；新请求读取新 snapshot，in-flight request 保持旧 snapshot。
- 路由字段：`protocol`、`base_url`、model id 的增删、`default_model`、credential、
  `upstream_auth`、adapter、compatibility profile、Codex options 会改变 route/config/
  fingerprint。只要存在 binding，普通 edit 返回 `PROVIDER_REBIND_REQUIRED` 且零写入；
  必须走显式 provider-edit-and-rebind dry-run，列出所有受影响 profile，并对每个 binding
  和 config hash 做 CAS。任一 profile 不可管理则整批不提交。

删除仍被绑定的 Provider 返回 `PROVIDER_IN_USE`。MVP 不提供隐式 cascade detach，避免
metadata 已删但 config/Gateway projection 仍可运行。

### 6.2 ProfileProviderBinding

profile 绑定是一等领域模型：

```text
ProfileProviderBinding
  profile_id: string
  provider_id: string
  selected_model: string
  route_kind: direct | gateway
  gateway_binding_id?: string
  config_projection: ManagedConfigProjection
  revision: integer
  created_at: string
  updated_at: string
```

权威性规则：

- `ProfileBindingStore` 是 profile/provider/model 关系的唯一权威来源。
- `$CODEX_HOME/config.toml` 是可与用户其他配置合并的投影，Gateway token binding 是可撤销的运行时投影。
- Provider 的 used-by 视图只查询 `ProfileBindingStore`，不通过扫描 config 或 Gateway store 反推。
- attach、rebind、detach 都要求 caller 提供 expected binding revision；不匹配时返回 `PROFILE_BINDING_CONFLICT`。
- Gateway 在每个请求开始时根据 token 读取一份不可变 binding/provider snapshot，请求进行中的编辑只影响新请求。

`ProfileBindingStore` 只覆盖显式 attach 到 Remote API Provider 的 profile。ChatGPT
auth profile 和尚未 attach 的 metadata-only Provider 不创建占位 binding；因此
“binding 不存在”有唯一含义：LAM 不拥有该 profile 的 Provider 投影。

生命周期规则：

- 初次 attach：binding 不存在且 config 不含等价 Provider 投影时，创建
  `ownership = created|modified` projection。
- adoption：config 已有与计划完全等价且可解析的 provider/model/auth 引用时，dry-run
  明示 `adopt_existing_projection` 并要求确认；binding 使用 `ownership = adopted`、
  `managed_keys = []`。detach 只删除 binding/Gateway credential，不改 adoption 前配置。
- drift：binding 存在但任一 `applied_values` 不匹配时，read/test/launch 返回
  `CODEX_CONFIG_DRIFT`；attach/rebind/detach 均零写入，用户只能恢复受管值或显式
  forget binding。`forget` 不改 config，并撤销 Gateway credential。
- profile display-name 修改不改变稳定 `profile_id` 或 binding。移动 `CODEX_HOME` 是
  route-breaking rename，必须 dry-run + CAS 迁移 config path；不做路径猜测。
- 删除 attached profile 返回 `PROFILE_HAS_PROVIDER_BINDING`；必须先成功 detach/forget。
- invalid TOML、非普通文件、symlink、路径逃逸、非 owner、权限过宽且无法安全修复、
  只读目录或未来 schema 均为 `CONFIG_UNMANAGEABLE`，不自动重建或整文件覆盖。

```text
ManagedConfigProjection
  config_path: absolute_path
  ownership: created | modified | adopted
  before_hash: sha256
  applied_hash: sha256
  managed_keys: TomlKeyPath[]
  previous_values: map<TomlKeyPath, TomlValueOrMissing>
  applied_values: map<TomlKeyPath, TomlValueOrMissing>
  provider_table_created_by_lam: bool
```

`ManagedConfigProjection` 用于幂等 detach 和冲突检测，不保存 secret。如果当前受管 key 已被用户修改，detach 返回 `CODEX_CONFIG_OWNERSHIP_CONFLICT` 并不覆盖用户值。

```text
ManagedConfigPatch
  config_path: absolute_path
  expected_config_hash: sha256
  operations: ConfigOperation[]
  resulting_hash: sha256
  redacted_summary: string[]

ConfigOperation
  set { key: TomlKeyPath, expected: TomlValueOrMissing, value: TomlValue }
  remove { key: TomlKeyPath, expected: TomlValue }
```

Patch key path 只允许 planner 生成的 top-level `model`、`model_provider` 和目标
`model_providers.<lam-id>.*` allowlist；调用方不能提交任意 key。`expected` 在 apply 时
逐项比较，`expected_config_hash` 同时保护 comment/未知 key 等非受管内容。patch、
projection 和 redacted summary 均禁止 secret 值。

### 6.3 CredentialSource 与 SecretReference

```text
SecretReference
  kind: env | keychain | auth_command | none
  env_key?: string
  keychain_service?: "lam.remote-provider"
  keychain_account?: string
  keychain_version?: integer
  auth_command?: AuthCommand
```

`SecretReference` 是 persistence/API view 的无 secret 判别联合；加载到领域层后成为
6.0 的 `CredentialSource`。`ProviderProfile.upstream_auth` 决定引用如何进入上游请求，
两者不可再由 `env`/`keychain` 等 storage kind 隐式推断。legacy `envKey` 迁移为
`credential = env` 且 `upstream_auth = bearer`。

约束：

- 前端只能看到引用信息，不能看到 secret。
- Keychain 写入失败时不得持久化 provider metadata。
- auth command stdout 只能输出 bearer token。

Secret 到运行时认证的映射必须是单一、可预测的：

| SecretReference | Responses 直连                 | Gateway 访问上游                     | Codex 访问 Gateway                      |
| --------------- | ------------------------------ | ------------------------------------ | --------------------------------------- |
| `env`           | 写 Codex `env_key`             | sidecar 从明确允许的环境变量读取     | 不用于共享 Gateway token                |
| `codex_profile` | 写 profile `auth.json` 并启用 `requires_openai_auth` | 不允许进入 Gateway | 不用于 Gateway token |
| `keychain`      | 仅作为 Responses legacy 迁移输入 | sidecar 通过 `SecretStore` 读取    | 写 profile-specific gateway auth helper |
| `auth_command`  | 写 Codex provider `auth` table | sidecar 通过受限 command runner 调用 | 不复用上游 helper                       |
| `none`          | 仅允许无认证上游               | 仅允许无认证上游                     | 禁止；Gateway 必须鉴权                  |

`env_key`、provider `auth`、`experimental_bearer_token` 和 `requires_openai_auth` 是互斥认证模式，planner 必须在写配置前阻止冲突。`codex_profile` 是 `requires_openai_auth` 的唯一受管来源。Auth command 只允许结构化参数，禁止 shell 展开；必须定义 timeout、最大 stdout 大小、退出码、token 缓存/刷新和 stderr 脱敏规则。前端不展示 auth command 的运行结果。

Provider create/update 的 secret 变更使用窄化补偿：先验证 metadata 并准备带 expected store revision 的写入，再写入新 secret，最后 CAS commit metadata。commit 失败时只删除本操作新建且仍可安全识别的 secret；update 在 metadata commit 成功前保留旧 secret，成功后再清理旧版本。不与 profile attach 共享一个通用事务协调器。

### 6.4 AdapterConfig

```text
AdapterConfig
  none
  local:
    adapter_id: responses_to_chat_completions
    upstream_path: /chat/completions | validated_custom_path
```

Adapter 使用判别联合而不是 `enabled + mode` 两套状态，避免 `enabled = false, mode = local` 等非法组合。`local_base_url` 是 Gateway runtime state，不属于 Provider metadata。

Responses Provider 固定：

```text
adapter = none
```

Chat Completions Provider 可选：

```text
adapter.local.adapter_id = responses_to_chat_completions
adapter.local.upstream_path = /chat/completions
```

### 6.5 Capability、Readiness 与 Health

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
  provider_id
  model_id
  endpoint_fingerprint
  adapter_id
  adapter_version
  provider_compatibility_profile
  compatibility_profile_version
  conformance_suite_version
  tested_at
  expires_at
  results: CapabilitySet
  evidence_ids[]
```

verification cache 的完整 key 由 `provider_id + model_id + endpoint_fingerprint + adapter id/version + compatibility profile id/version + conformance suite version` 组成。任一组成项改变都不得复用旧结果；过期结果可展示为 historical evidence，不得表示当前 verified。

Provider 当前可否运行与网络健康分开：

```text
ProviderReadiness
  ready: bool
  blockers: ReadinessBlocker[]
  binding_count: integer
ProviderHealthObservation
  status: unknown | healthy | degraded | unavailable
  checked_at
  latency_ms
  error_code?
```

Readiness 不使用单值 `attached`：一个 provider 可以被多个 profile 绑定，且 `missing_secret`、`adapter_required`、`gateway_unavailable` 可以同时存在。`blockers` 保留所有可操作原因，`binding_count` 只是派生统计。

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
5. effective capability 优先级固定为：显式 user override > 未过期且 key 完全匹配的 verification > declaration > unknown。override 不得将 adapter 明确不可表达的能力强制变成 supported。
6. UI 必须区分 declared、verified、user override 和当前 health。
```

UI warning 应表达适配层兼容性，而不是说上游不支持：

```text
This provider uses a local protocol adapter. DeepSeek supports streaming and tool calls, but runtime behavior depends on LAM's Responses-to-Chat-Completions compatibility for this provider.
```

## 7. 运行时规划

核心规划分成两层，避免“纯函数”隐式依赖 Gateway 或 profile 状态：

```text
fn plan_provider_route(input: ProviderRouteInput) -> Result<ProviderRoutePlan>
fn plan_profile_attach(route, context: ProfileAttachContext) -> Result<ProfileAttachPlan>
```

`ProviderRouteInput` 只包含 provider、selected model、adapter catalog 和 capability snapshot，因此可纯计算。输出：

```text
ProviderRoutePlan
  provider_id
  selected_model
  route_kind: RouteKind
  codex_wire_api = responses
  upstream_base_url
  upstream_protocol
  adapter_required
  adapter_id
  effective_capabilities
  warnings[]
```

`ProfileAttachContext` 显式包含 `profile_id`、现有 binding/revision、Gateway endpoint/status 和当前 Codex config snapshot。输出：

```text
ProfileAttachPlan
  route: ProviderRoutePlan
  profile_id
  expected_binding_revision
  codex_base_url
  codex_auth: env_key | auth_command | none
  gateway_binding_spec?
  config_patch: ManagedConfigPatch
  operations[]
  blockers[]
  warnings[]
```

规则：

- Responses Provider：
  - route plan 始终选择 direct，`codex_base_url = provider.base_url`。
  - 读取到旧的 `route_via_gateway = true` 时先规范为 `false`，并将已有 binding、配置和 wrapper 幂等迁移为 direct。
  - `adapter_required = false`
  - API 帐号凭据写入该 profile 私有的 Codex `auth.json`，配置使用
    `requires_openai_auth = true`，运行时不调用 LAM helper 或 Keychain。
- Chat Completions + adapter：
  - route plan 选择 gateway；attach plan 从显式 context 读取 `local_gateway_url`。
  - `adapter_required = true`
  - 上游地址只保存在 LAM provider store。
- Chat Completions 未启用 adapter：
  - route plan 返回 `PROVIDER_ADAPTER_REQUIRED`
  - 可保存但不可 attach/run。

这样 test/UI/relay 共享 `ProviderRoutePlan`，attach/dry-run 共享 `ProfileAttachPlan`，不会迫使所有用例依赖一个过度宽泛的 plan。两层 planner 都不启动 Gateway、不读取 secret 值、不写文件；副作用由窄化的 attach/detach executor 执行。

Planner 必须验证：

- `selected_model` 存在于 `models`；自定义模型在规划前必须显式加入 model catalog。
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
wire_api = "responses"
requires_openai_auth = true
```

对应 profile 顶层固定写 `cli_auth_credentials_store = "file"`；API key 只存在于
mode 0600 的 `auth.json`（`auth_mode = "apikey"`），Provider store、plan、日志和
前端 view 均不保存或返回 secret。环境变量型的通用直连 Provider 仍可选择
`env_key`，但不得与 `requires_openai_auth` 同时出现。

Codex 还支持以下可选字段：

```toml
[model_providers.<provider_id>]
env_http_headers = { "X-Provider-Feature" = "PROVIDER_FEATURE" }
query_params = { version = "2024" }
```

`CodexConfigEditor` 首版只写受控 `env_http_headers` 和 `query_params`。MVP 不接受
静态 `http_headers` 或 `experimental_bearer_token`。只有 profile-owned
`codex_profile` 凭据写 `requires_openai_auth`；新增认证模式必须先扩展 6.0 的
闭集和威胁测试。

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
args = ["gateway-token", "--state-root", "<provider-hub-root>", "--profile", "<profile-id>", "--binding", "<binding-id>"]
timeout_ms = 5000
refresh_interval_ms = 0
```

`<lam-auth-helper>` 必须由安装 manifest、SHA-256 与 code-sign identity 校验后解析为绝对
路径。attach dry-run 预分配 binding id；Gateway credential prepare 必须使用同一 id，
否则 transaction 在写 config 前返回 `GATEWAY_BINDING_PLAN_MISMATCH`。禁止把 placeholder
binding 或相对 helper 命令写入 profile。

真实上游保存在 LAM store：

```text
provider.base_url = https://api.deepseek.com
provider.protocol = chat_completions
provider.adapter = local(responses_to_chat_completions)
provider.credential.env_key = DEEPSEEK_API_KEY
provider.upstream_auth = bearer
```

### 8.3 写入安全

- 读取现有 `config.toml`，用 `toml_edit` 只修改 `model`、`model_provider` 和目标 `[model_providers.<id>]`。
- 保留现有 MCP、sandbox、approval、reasoning、未知字段和注释。
- 写前备份 `config.toml`，并记录原文件 hash 以检测并发修改。
- 编辑后用 TOML parser 重新解析，写入同目录临时文件，flush/fsync 后原子 rename。
- 校验、并发检测或写入失败时原文件保持不变。
- attach 重复执行必须幂等；已存在的 provider table 含非 LAM 管理内容时必须保留或报冲突。
- LAM 按 `ManagedConfigProjection.managed_keys` 记录所有权；不得把“存在备份”当作可无条件整体恢复的授权。
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

### 8.5 Attach、Rebind 与 Detach 边界

Provider metadata 创建/编辑和 profile attach 是两个独立用例，不构建通用的跨资源事务框架。Attach executor 只协调 binding、Gateway token 和一个 config 投影，使用窄化 operation journal 与明确补偿：

```text
1. build + validate `ProviderRoutePlan` and `ProfileAttachPlan`
2. acquire profile binding/config locks and verify expected revision/hash
3. prepare a new gateway binding when route kind is gateway
4. prepare edited config in temporary file
5. validate all prepared artifacts
6. persist a narrow journal, commit config projection, then commit `ProfileProviderBinding`
7. on failure, revoke only the newly created gateway binding and restore only LAM-managed config keys
8. after a successful rebind, revoke the superseded gateway binding
```

dry-run 执行全部纯验证和内存中的 artifact 生成，但不写 binding、journal 或文件；返回预计操作、expected revision/hash、冲突、备份路径和脱敏后的配置摘要。恢复时以 journal 和当前 hash 共同判定，不盲目整文件回滚。

Detach 规则：

```text
1. lock binding/config and verify expected binding revision
2. compare current managed key values with ManagedConfigProjection.applied_hash/values
3. on mismatch return CODEX_CONFIG_OWNERSHIP_CONFLICT without mutation
4. restore previous values; remove provider table only if LAM created it and it has no unmanaged keys
5. atomically write and validate config
6. delete ProfileProviderBinding and revoke gateway binding
```

detach 重复请求在 binding 已不存在时返回幂等成功。备份文件用 UUID/高精度时间和 hash 命名，避免同秒操作覆盖；备份保留策略单独配置，不参与正确性判定。

### 8.6 Store、dry-run、journal 与恢复契约

Provider Hub 的 metadata、binding、journal 和 Gateway state 使用 versioned JSON
envelope。所有 mutation 先取得安装级 `provider-hub.lock`；锁文件位于 LAM 私有 state
目录，使用 advisory cross-process exclusive lock，持有者 metadata 仅用于诊断，不用于
强制解锁。MVP 不再叠加可逆序取得的 provider/binding 子锁；需要碰多个 config 时按
规范化绝对路径字典序处理。外部程序不会取得该锁，因此每个 config 仍必须在 rename
前复核 expected hash。

Store commit 统一执行：同目录创建 `0600` 随机临时文件 → 写完整 envelope → flush +
`fsync(file)` → 校验可重新解析且 revision 为旧 revision + 1 → atomic rename →
`fsync(parent directory)`。目标和临时文件必须是 owner 持有的普通文件，拒绝 symlink；
高于当前 schema 的 envelope 只读报错且永不覆盖。macOS rename 是首版保证，其他平台
不在 MVP 支持声明内。

dry-run 输出 opaque `plan_id` 和：

```text
PlanFingerprintInput
  operation: attach | adopt | rebind | detach | provider_edit_and_rebind
  profile_id
  provider_id
  selected_model
  provider_store_schema/revision
  canonical_provider_hash
  binding_store_schema/revision
  expected_binding_revision
  config_path + config_hash
  gateway_endpoint_generation
  adapter_id/version
  compatibility_profile_id/version
  secret_reference_identity/version       # 不含 secret value
  planner_contract_version
```

fingerprint 是 domain-separated canonical JSON SHA-256。plan cache 最多 128 项、TTL
5 分钟、仅进程内保存脱敏 plan；进程重启后要求重新 dry-run。execute 必须同时命中
`plan_id`、fingerprint、TTL 和所有当前 revision/hash，成功或进入 journal 后立即消费；
重复 execute 返回 `ATTACH_PLAN_ALREADY_CONSUMED`，过期/任一输入变化返回
`ATTACH_PLAN_STALE` 且零副作用。失败于取得 journal 之前可用同一未消费 plan 重试。

```text
AttachJournalRecord
  journal_version
  operation_id
  operation
  state: prepared | config_committed | binding_committed | completed |
         rolled_back | manual_intervention
  profile_id/provider_id/selected_model/route_kind
  expected_provider_store_revision
  expected_binding_store_revision + expected_binding_revision
  config_path + before_hash + intended_hash
  managed_patch + previous_projection?
  new_gateway_binding_ref? + superseded_gateway_binding_ref?
  prepared_binding
  created_at/updated_at
  last_error_code?
```

journal 不含 token、credential value、auth stdout/stderr 或 config 全文。journal 持久化
`prepared` 后的提交顺序固定为 config → binding CAS；`binding_committed` 是唯一 commit
point。获得安装锁的首个 desktop service 或 `lam codex` launcher 是 recovery owner：

- `prepared`/`config_committed`：若当前 hash 仍等于 before/intended 的已知状态，回滚
  LAM-managed keys并撤销本操作新建 Gateway binding，写 `rolled_back`；
- `binding_committed`：验证 binding 指向 intended projection，roll forward 撤销旧 Gateway
  binding并写 `completed`；
- 任一 config/binding 出现未知 hash/revision：不覆盖，写 `manual_intervention`，暴露
  `ATTACH_RECOVERY_OWNERSHIP_CONFLICT`；
- `completed`/`rolled_back` 是终态，recovery 幂等且不得重复撤销仍被当前 binding 使用的
  credential。

终态 journal 保留 30 天且至少保留最近 100 条，清理只在无 active journal 时进行；
`manual_intervention` 不自动清理。备份仅用于审计/人工恢复，自动恢复只依据 journal、
managed values 和当前 hash。

状态样例：

| 场景                         | 确定结果                                                                          |
| ---------------------------- | --------------------------------------------------------------------------------- |
| attach                       | config commit 后 binding CAS；CAS 失败则恢复 managed keys，最终无 binding         |
| adoption                     | 创建 `adopted` binding；detach/forget 不修改原 config                             |
| rebind                       | 新 config + binding commit 后才撤销旧 Gateway credential；失败保留旧 binding 可用 |
| detach                       | ownership 匹配才恢复 managed keys并删除 binding；重复 detach 幂等成功             |
| drift                        | 返回 `CODEX_CONFIG_DRIFT`，任何 attach/detach/recovery 不覆盖未知用户值           |
| route-breaking Provider edit | 有 binding 时要求全量 rebind dry-run；任一冲突则 Provider 也不更新                |
| crash before binding commit  | recovery rollback                                                                 |
| crash after binding commit   | recovery roll forward cleanup                                                     |

## 9. 本地 Gateway

### 9.0 MVP 平台、路径与 trust boundary

首版只支持并声称验证过 `macOS 15.6 / Darwin arm64`。这是产品安全边界，不是仅 CI
标签：Windows、Linux、其他 macOS/architecture 的 Remote Provider create/edit-
credential/attach/rebind/detach/launcher/Gateway 命令在任何写入或 secret 读取前返回
`UNSUPPORTED_REMOTE_PROVIDER_PLATFORM`。legacy Provider/account 可只读展示和导出；
UI 隐藏 enable/attach 操作并显示 exact-tested platform。扩大范围必须新增各平台的锁、
atomic replace、credential store、process/IPC、package identity 和 Codex contract gate。

macOS 路径以 bundle id `dev.localagentmanager.desktop` 为 namespace：

| Artifact                                          | Normative location                                                                          | Mode / identity                                                |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| Provider Hub state root                           | `~/Library/Application Support/dev.localagentmanager.desktop/provider-hub/`                 | directory `0700`, current uid                                  |
| provider/binding/gateway envelopes, journal, lock | state root 下固定 basename                                                                  | regular file `0600`, current uid, no symlink                   |
| backup/temp                                       | target 同目录的 `backups/`/random temp                                                      | directory `0700`, file `0600`, same filesystem                 |
| control socket                                    | `$DARWIN_USER_TEMP_DIR/dev.localagentmanager.desktop/gateway-<sha256(install-id)[0:16]>.sock` | parent `0700`, Unix socket `0600`, current uid；basename 有界，避免 Darwin AF_UNIX 路径超限 |
| install identity key                              | macOS Keychain service `dev.localagentmanager.desktop.provider-hub`, account `<install-id>` | non-exported-by-UI random 256-bit key                          |
| provider/gateway credential                       | macOS Keychain service `lam.remote-provider`, versioned account                             | secret value only in Keychain/process memory                   |
| sidecar/auth helper                               | signed app bundle `Contents/Resources/bin/<component>/<version>/...`                        | regular executable, code-sign Team ID + manifest SHA-256 match |
| redacted log                                      | `~/Library/Logs/LAM/provider-gateway.log`                                                   | regular file `0600`, rotation 5 × 2 MiB                        |

`install-id` 是首次初始化生成的 128-bit random public identifier，写入 `0600` state；
identity key 不写文件。任何 state/config/component path 在 open 前逐级 `lstat/openat`
验证 current uid、非 symlink、非 group/world writable；现有 config 可为只读，但 mutation
必须拒绝。新 `CODEX_HOME` directory 为 `0700`、config 为 `0600`；既有目录至少不得
group/world writable，LAM 不静默 chmod 用户目录。

平台 primitives 固定为：

- cross-process lock 使用 Darwin advisory `flock(LOCK_EX)`，锁 fd 持有整个 mutation；
  crash 由 kernel 释放，不依据 PID metadata 强制解锁；
- atomic store/config 采用同目录 temp、`fsync(file)`、`rename(2)`、`fsync(parent)`；
- Keychain 使用 Security framework generic-password item；access failure 不降级 plaintext；
- sidecar/helper/launcher 通过 Rust `Command`/`posix_spawn` 绝对路径直接 argv，永不经
  login shell、`sh -c` 或 PATH lookup；仅传 allowlisted env 和显式继承 fd；
- package manifest 含 component/version/protocol/state-schema/SHA-256/code-sign Team ID；
  launcher 在 spawn 前验证 manifest、hash 和 designated requirement，失败返回
  `GATEWAY_COMPONENT_INTEGRITY_FAILED`。

信任分为两条 plane：

```text
HTTP data plane (127.0.0.1 stable port)
  Codex -> bearer verification -> immutable GatewayBinding snapshot -> upstream

private control plane (gateway-control Unix socket)
  LAM/launcher -> same-uid peer check -> nonce challenge/HMAC -> versioned command
```

TCP listener 或 `/healthz` 只证明某进程占用端口，不能证明是 LAM sidecar。Supervisor
readiness 必须同时满足：bundle component identity 已验证；control socket peer uid 等于
当前 uid；随机 256-bit challenge 的 HMAC（install identity key）正确；响应的
`install_id/instance_id/control_protocol_version/component_version/state_schema/port` 与
manifest/state 完全一致；最后再用 profile bearer 验证 HTTP `/healthz` 的 matching
instance id。任一步失败均返回 `GATEWAY_IDENTITY_MISMATCH`，不向可疑 listener 发送
Provider metadata、token 或请求 body，也不自动改端口。

Control secret/Keychain token 不放 argv、environment、普通 state 或日志。sidecar 由
launcher 通过继承 pipe/fd 接收一次性 bootstrap nonce，随后从 Keychain/受控 state
加载 reference；control message 使用 length-prefixed canonical JSON、64 KiB 上限、
单调 request nonce、HMAC 和 protocol version。Unix peer uid 只是必要条件，不替代
HMAC。health response 只含版本、instance id、状态和 redacted counts。

Upgrade 使用 versioned side-by-side binaries。launcher 发现 protocol/component 不匹配时，
先在旧 control protocol 兼容范围内发 authenticated graceful shutdown；最多等待 30 秒
drain，不兼容或仍有请求则返回 `GATEWAY_UPGRADE_REQUIRED` 且不改 config/state。成功
退出后在同一 supervisor lock 下启动并验证新 sidecar。state schema 只能逐版本迁移，
高版本 state 拒绝降级写入。

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

token 只存在 macOS Keychain 和进程内受保护内存，不提供 plaintext file fallback。每个
attached profile 使用独立 token/binding，store 只持久化 token hash 和密钥引用。token
必须可轮换和撤销，detach 后立即失效。鉴权比较使用 constant-time compare，失败日志
不输出 token 或请求 body。

RPG-002 证明空 auth helper 在 `codex-cli 0.144.1` 中会报告错误但仍可能发出无 `Authorization` 请求；因此“helper 配置存在”不能视为已鉴权。Gateway 对缺失、空值、格式错误、未知或已撤销 bearer 一律返回 `401`，且在完成 token 校验前不解析/转发请求 body。

一个 Gateway binding 只能属于一个稳定 `profile_id`；不同 profile 即使 provider/model
相同也不得复用 binding/token。rotation 在 installation lock 下创建新 Keychain version，
CAS 更新 binding generation/token hash 后立即使旧 token 对新请求失效，再删除旧
Keychain item；已完成 bearer 校验的 in-flight request 使用旧 immutable snapshot 运行到
结束。并发旧请求收到 401 后依赖已捕获的 Codex 单次 helper refresh/replay 获取新 token。
detach commit 后旧/新 generation 均立即 401，不设置 grace window。Keychain 删除失败把
binding 标记 `revocation_pending`、禁止 readiness，并由 recovery 重试；绝不恢复已撤销
hash 的可用性。

### 9.2.1 Codex 契约基线

受控 capture 使用全新临时 `HOME`/`CODEX_HOME`、固定合成 prompt/token、loopback Provider，并禁用插件、remote plugin、apps、更新检查与 web search。fixture 位于 `apps/desktop/src-tauri/tests/fixtures/codex-gateway-contract/`，离线 verifier 不启动 Codex 或网络。

| 项目        | 已验证契约                                                               |
| ----------- | ------------------------------------------------------------------------ |
| CLI / 平台  | exact-tested `codex-cli 0.144.1`；macOS 15.6；Darwin arm64；2026-07-14 捕获 |
| HTTP 路由   | `GET /v1/models?client_version=0.144.1`、`POST /v1/responses`            |
| 模型目录    | 顶层 `models`；非空；13 个必需 metadata 字段；禁止 fallback metadata    |
| 请求模式    | 所有 text/tool/resume 请求均为 `stream = true`                           |
| 多轮状态    | tool follow-up 与 resume 都发送完整 input；`previous_response_id = null` |
| auth helper | stdin 为 0；stdout 会 trim；首个 `401` 触发一次 refresh + replay         |
| 默认重试    | `429 + Retry-After: 0` 为 1 次；malformed SSE 为 6 次；`500` 为 30 次    |
| 取消        | Codex 进程组终止后，loopback server 观测到 client close                  |
| 未观测      | non-stream、response retrieve、response cancel、其他 state route         |

升级 Codex、扩展 OS/architecture 或改变生成的 Provider retry 配置时，必须重跑 harness 两次并通过离线 checksum/sanitization verifier；不得依据“semver 看起来兼容”自动扩大支持范围。

### 9.3 路由

无条件必须实现：

```text
GET  /healthz
GET  /v1/models
POST /v1/responses
```

RPG-002 未观测到，首版不实现：

```text
GET/POST /v1/responses/{id}
POST /v1/responses/{id}/cancel
bounded response state routes
```

后续可给本地其他客户端提供：

```text
POST /v1/chat/completions
```

Codex profile 接入的最小必需入口是 `/v1/models` 与 `/v1/responses`。`/v1/models` 必须返回非空的顶级 `models` 数组；每项必须包含 6.1.1 定义的 13 个 exact-tested metadata 字段。标准 OpenAI `{ data: [...] }` 只用于上游发现，不是 Codex 目录响应。空目录、缺字段或 Codex fallback metadata warning 都是 release blocker。Gateway 从 authenticated immutable binding 对应 Provider 的完整 model allowlist 生成确定性目录；请求中的未知 model 在任何上游 I/O 前拒绝。

Responses Provider 不经过 Gateway；Codex 直接请求 Provider 的 `/responses` 接口。Chat Completions Provider 通过受控 adapter 转换。只有存在未撤销的 Chat Completions binding 时 Supervisor 才允许启动 Gateway，sidecar 的空闲计数也使用同一判定契约。

### 9.4 Provider 选择

避免在请求 body 或 path 里放 secret/provider 明文策略。MVP 只通过已验证 bearer token
解析唯一 provider binding：

```text
GatewayBinding
  token_hash
  binding_id
  profile_id
  provider_id
  selected_model       # config.toml 默认模型
  provider.models      # 请求时的完整 model allowlist snapshot
  created_at
  revoked_at?
```

Codex profile 调用 gateway 时，gateway 根据 token binding 找到 provider 和上游 secret 引用。

### 9.5 生命周期管理

Gateway 作为独立 sidecar 运行，不与 Tauri 窗口或前端页面生命周期绑定。原因是 attached Codex profile 在 LAM UI 关闭后仍应可用。Tauri 应用是管理者和 supervisor，Gateway 不直接共享内存 `AppState`，而是通过最小化的私有 state store 和窄 IPC/control endpoint 获取配置。

首版启动契约是显式的 `lam codex --profile <id> [-- <codex args>]` launcher：

```text
1. resolve versioned installed sidecar/auth-helper paths from the LAM installation manifest
2. load ProfileProviderBinding and validate its config projection
3. validate that the selected auth mode is consumable; env mode checks the named variable in the launch environment without reading it into logs/UI
4. when route_kind = gateway, ensure sidecar is ready under the single-instance lock
5. execute Codex with the selected CODEX_HOME, inherited/explicitly allowed environment, unchanged user arguments and propagated exit code
```

首版不声称任意 `CODEX_HOME=... codex` 直接启动都能在 Gateway 未运行时自动恢复。LAM UI 关闭不会停止已运行的 sidecar；系统重启后则由 launcher 重新拉起。若未来需要无 launcher 保证，应以独立的用户级后台服务设计扩展，不隐式塞进 auth helper。

生命周期规则：

```text
启动：
- LAM 启动、首次 attach 或 `lam codex` launcher 发现 Gateway 不可用时，通过单实例锁幂等地拉起 sidecar。
- sidecar 只有在获得单实例锁后才能绑定端口；其他实例复用已运行 Gateway。
- 无 attached adapter profile 时不自动启动。

端口管理：
- 使用 9.1 定义的安装级稳定端口。
- 多 provider 共享同一 Gateway 进程，通过 token binding 区分。

监控：
- LAM 打开时监控 sidecar 进程和 `/healthz`；`lam codex` launcher 在启动 Codex 前做一次 readiness check。
- 重启使用有上限的指数退避和 jitter；连续失败后报告 `GATEWAY_HEALTH_FAILED`，不无限重启。

关闭：
- 所有 adapter profile detach 后，Gateway 在无 in-flight request 且经过固定 idle grace period 后退出。
- LAM UI 退出不终止仍有 attached binding 的 Gateway。

崩溃恢复：
- LAM supervisor 和 `lam codex` launcher 都可以幂等拉起 sidecar，保留稳定端口。
- UI 显示 Gateway 状态变更（available -> restarting -> available）。
```

### 9.6 多轮对话状态模型

首版 Gateway 保持 response-history 无状态；这是 `codex-cli 0.144.1` contract test 的确定结果。

```text
- 普通 text、function tool follow-up 与 resume 都传完整 input items。
- function tool follow-up 包含原 `function_call` 与对应 `function_call_output`，并保留 `call_id`。
- resume 包含此前 user/assistant item 与本轮 user item。
- 所有已验证路径的 `previous_response_id` 都是 `null`，不需要 response store。
- 若收到 `previous_response_id`，返回结构化 `ADAPTER_UNSUPPORTED_FIELD`，错误中声明当前 adapter 版本和可恢复建议；不静默忽略。
- 首版不实现 response retrieve/cancel/state route，也不创建 response history store。
- 未来 Codex contract 若引入 `previous_response_id`，必须作为新设计切片增加有界 store、TTL、容量、清理、binding 隔离和单独威胁模型，不能在兼容层静默打开持久化。
```

### 9.7 资源、网络与日志边界

- Gateway 在读取 body 前验证 bearer；随后 request body 上限 4 MiB，单 SSE frame
  256 KiB，单 tool arguments 累计 1 MiB，单 response wire bytes 32 MiB，response 总时长
  15 分钟、连接/首 byte 10 秒、stream idle 60 秒。
- 并发上限为全局 32、每 binding 4、全局 upstream connection 16；等待队列全局 64、
  每 binding 8，满时立即 `429 GATEWAY_CONCURRENCY_LIMIT`。adapter channel 最多 64
  events 且累计 2 MiB，超限 `GATEWAY_BUFFER_OVERFLOW`。所有计数在 acquire 后用
  cancellation-safe guard 释放。
- 单 envelope 16 MiB、单 journal record 256 KiB、active journal 128、plan cache 128、
  backup 总量 256 MiB/100 files；达到上限拒绝新 mutation，不删除 active/manual journal。
- 上游 URL 只能来自通过 integrity/approval 校验的 Provider snapshot；request body、model、
  tool 或 header 不得改变 scheme/authority/path。只允许 `https` + public DNS hostname，
  禁止 URL userinfo、fragment、控制字符、IP literal、非默认 port（除非批准 fingerprint
  明确包含）、localhost/`.local`/single-label/metadata hostname。
- 每次新连接解析 DNS；若任一 A/AAAA 为 loopback、RFC1918、link-local、CGNAT、
  multicast、unspecified、documentation/benchmark 或 IPv6 ULA/mapped-private，则整次
  请求返回 `UPSTREAM_NETWORK_TARGET_FORBIDDEN`。connector 固定使用已验证 address，
  TLS SNI/Host 保持批准 hostname，连接池按 provider fingerprint 隔离并在 DNS TTL/
  60 秒较小者后重建。HTTP redirect 无条件关闭，proxy env 无条件忽略。
- Phase 1 direct Responses route 由 Codex 自己建立网络连接，LAM 无法 pin Codex 的 DNS
  socket。MVP 通过 route metadata HMAC、attach/launch 时重新解析并拒绝私网来降低风险，
  但明确接受“验证后权威 DNS rebinding”残余风险；UI 显示 direct-network trust warning。
  需要消除此风险的部署必须走后续受控 Gateway mediation，不能声称 direct path 已隔离。
- Gateway 不开启 CORS，除非后续有明确的本地浏览器客户端需求和 origin allowlist。
- 默认日志只包含 request id、opaque binding/provider hash、status、latency、脱敏错误码、
  byte/count buckets 和 token usage；不记录 URL query、prompt、tool arguments、response/
  reasoning text、Authorization、secret、auth helper stdout/stderr 或 control payload。
- provider id、model 和 URL 作为日志字段前移除控制字符，防止 log injection。
- conformance evidence 只保存 fixture case id、schema/capability、status、大小/hash 和
  synthetic content；真实 Provider 请求默认不产生 body evidence。debug 开关也不能解除
  secret/content redaction，crash context 使用同一 allowlist。

### 9.8 Retry 与幂等边界

Codex 和 Gateway 不得同时使用独立的默认重试预算，避免生成请求倍增和重复计费。首版规则：

- LAM 管理的 Gateway Provider config 必须显式写 `request_max_retries = 0` 与 `stream_max_retries = 0`。RPG-002 证明不写时，`500` 可产生 30 次请求、malformed SSE 可产生 6 次请求。
- Gateway 对 `POST /v1/responses` 默认不做应用层重试，把可重试错误映射给 Codex。
- 只在能证明上游未接收请求、未收到任何 response byte，且仍在同一总 deadline 内时，upstream client 可做最多一次建连重试。
- 收到任何上游响应或向客户端发送任何事件后绝不重试。
- 429 和 5xx 只映射 `retryable`/`Retry-After`，不在 Gateway 内自动重放生成请求。
- Gateway 返回 `401` 时 Codex 0.144.1 会 refresh auth helper 并 replay 一次；token 校验必须在任何副作用前完成，使该 replay 安全。
- 每个 request id 记录脱敏 retry count。正常 HTTP/stream 失败的组合应用层尝试上限为 1；只有“确认未送达”的建连失败可使底层连接尝试上限为 2。

## 10. Adapter 设计

### 10.1 Trait

Rust 侧定义协议适配契约：

```rust
trait ProtocolAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn source_protocol(&self) -> ProviderProtocol;
    fn target_protocol(&self) -> ProviderProtocol;
    fn capabilities(&self) -> AdapterCapabilities;
    fn begin(
        &self,
        request: ResponsesRequest,
        policy: &CompatibilityPolicy,
    ) -> Result<Box<dyn AdapterExchange + Send>>;
}

trait AdapterExchange: Send {
    fn upstream_request(&self) -> &ProtocolRequest;
    fn consume(&mut self, event: ProtocolEvent) -> Result<AdapterOutput>;
    fn finish(&mut self) -> Result<AdapterOutput>;
    fn cancel(&mut self);
}

AdapterOutput
  non_stream { response: ResponsesResponse }
  stream { events: ResponsesStreamEvent[] }
  pending
```

`source_protocol` 固定表示 Gateway 入站协议（当前为 Responses），`target_protocol` 表示上游 Provider 协议。adapter id 使用 `<source>_to_<target>` 命名；因此当前标准 ID 应为 `responses_to_chat_completions`。旧的 `chat_completions_to_responses` 只作为 provider store 迁移输入 alias，不再写回。response/event 的反向转换仍由同一 exchange 完成，不改变 adapter 路由方向的命名。

业务层通过 registry 获取 adapter：

```rust
adapter_registry.get(plan.adapter_id)
```

不在业务流程里写一堆协议分支。

`AdapterExchange` 是每请求独立的有状态转换器，持有 response/item/call ID、tool arguments 分片和事件顺序；registry 中的 `ProtocolAdapter` 本身保持无状态且可共享。`ProtocolRequest/Event` 是 adapter 模块内的受控协议联合，不暴露为全局业务 DTO；新协议的 schema 和解析器与对应 adapter 同模块演进，避免一个不断膨胀的通用 `UpstreamRequest`。

`begin` 根据 `ResponsesRequest.stream` 固定 exchange mode，整个生命周期不得混合
`non_stream` 和 `stream` variant。non-stream 在完整且验证过的 upstream response 到达前
只能返回 `pending`，完成时恰好返回一次 response；stream 可以增量返回 events，终态后
再次 `consume/finish` 返回 `ADAPTER_INVALID_STATE`。Exchange 由一个 Tokio task 独占，
不要求 `Sync`；并发隔离由每请求新建 exchange 保证。cancel 幂等，cancel 后不得再产生
completed output。

Adapter 只做协议转换；HTTP 请求、secret 解析、retry、timeout、日志和取消传播属于 Gateway/upstream client。`cancel()` 只终止转换状态，实际网络取消由 upstream client 负责。Provider-specific 策略通过类型化 `CompatibilityPolicy` 组合进 adapter，不使用任意字符串和分散分支。

### 10.2 Chat Completions 转换范围

首版支持的 Responses 子集必须用显式矩阵锁定：

| Responses 能力                                | 首版策略                                                                  |
| --------------------------------------------- | ------------------------------------------------------------------------- |
| string input / user-developer-system messages | translated                                                                |
| text output                                   | supported                                                                 |
| function tools                                | translated                                                                |
| `function_call` / `function_call_output`      | translated，保留 `call_id`                                                |
| streaming text/tool delta                     | translated                                                                |
| usage                                         | translated，无法对齐的细分字段标记 unknown                                |
| reasoning summary                             | provider-dependent partial                                                |
| structured output                             | 仅在上游和 adapter conformance 通过时支持                                 |
| image/audio/file input                        | `ADAPTER_UNSUPPORTED_INPUT_TYPE`                                          |
| hosted tools / MCP / computer use             | `ADAPTER_UNSUPPORTED_TOOL_TYPE`                                           |
| `previous_response_id`                        | `ADAPTER_UNSUPPORTED_FIELD`；当前 exact-tested CLI 不发送                 |
| unknown request field                         | 不在 RPG-002 allowlist 的字段返回 `ADAPTER_UNSUPPORTED_FIELD`，不静默丢弃 |

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
adapter: local(responses_to_chat_completions)
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

- 如果上游返回 `reasoning_content`，adapter 在内部 exchange 中保留它以满足 DeepSeek
  tool-call 历史规则，但 MVP 不把原始 chain-of-thought 标成 Responses `summary`，也不
  拼进普通 `content`。Codex-facing 输出抑制该原文，capability 标为
  `reasoning = partial` 并产生脱敏 warning。只有后续 contract fixture 证明某 Provider
  字段本身是可展示 summary，且 Responses schema 映射测试通过后，才能新增显式
  `reasoning_summary` policy。
- 继续多轮请求时，发送给 DeepSeek 的历史消息必须符合 DeepSeek 官方样例：不含 tool call 的已完成轮次可省略旧 `reasoning_content`；包含 tool call 的 assistant message 必须在后续请求中完整保留 `reasoning_content`，否则上游可能返回 400。任何情况下都不得把 reasoning 拼入普通 `content`。
- DeepSeek thinking + tool call 样例中，assistant 可能先产生 thinking，再产生 `tool_calls`；adapter 要保留 tool call 的 id、name、arguments，并把它们转换为 Responses tool call item。
- tool result 回传上游时，必须生成 Chat Completions `role = "tool"` 消息，并绑定正确 `tool_call_id`。
- 如果 Responses 请求包含 DeepSeek 无法支持或无法安全映射的 tool 类型，返回 `ADAPTER_UNSUPPORTED_TOOL_TYPE`，不要静默移除工具。
- 流式 thinking delta 只累计到有界内部 reasoning buffer，用于同轮 tool-call history；
  不向 Codex 发明 reasoning stream event。超限返回 `ADAPTER_REASONING_LIMIT_EXCEEDED`，
  日志与 conformance evidence 只记录长度/hash，不记录内容。

Codex 0.144.1 的锁定 `function-tool-followup-request` 只回传 `function_call` 与
`function_call_output`，`reasoning = null`，且 input 中没有 reasoning item。因而在本设计同时坚持
“完整历史、无 response store、原始 CoT 不进入普通 content/summary”时，Gateway 无法在后续请求中
恢复 DeepSeek thinking tool turn 所必需的 `reasoning_content`。在新的 Codex 捕获契约证明存在合法、
会被完整回传的受控 reasoning metadata 之前，`deepseek_chat_completions` 的 thinking + tools 组合必须
在首次上游调用前返回 `ADAPTER_REASONING_HISTORY_UNREPRESENTABLE`；不得用 model 名猜测模式、建立隐式
会话缓存或伪造 reasoning。DeepSeek thinking 无 tool，以及显式 disabled thinking 的 tool round-trip
仍由同一 compatibility profile 与 adapter registry 支持。

DeepSeek compatibility profile 只负责 DeepSeek 特殊字段和历史策略，不负责通用协议转换。通用转换仍在 `responses_to_chat_completions` adapter 中完成。

### 10.6 Anthropic-compatible 预留

DeepSeek 也提供 Anthropic-compatible 相关接口时，不要把 Anthropic 映射塞进 Chat Completions adapter。预留独立 adapter：

```text
responses_to_anthropic_messages
```

当用户选择 Anthropic-compatible 协议时，route plan 选择对应 adapter。这样 DeepSeek 的 OpenAI-compatible 和 Anthropic-compatible 两种接入可以共存，但实现边界清楚。

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

- Gateway application 层不重试 4xx/429/5xx；401/403 标记 auth，429/5xx 只映射
  status、`Retry-After` 与 retryable 分类给 Codex-facing error。
- 只有 upstream client 能证明请求尚未送达且未收到任何 response byte 的建连失败，
  才能在同一 deadline 内额外尝试一次连接；这不是应用层 request replay。
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

Attach 前展示 route/attach plan：

```text
Profile: ~/.codex-deepseek
Codex endpoint: http://127.0.0.1:<port>/v1
Upstream endpoint: https://api.deepseek.com/chat/completions
Model: deepseek-v4-pro
Secret: env(DEEPSEEK_API_KEY)
Adapter: responses_to_chat_completions
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

Relay 不以 warning 代替协议兼容性判定。在写入目标 session 前必须运行纯分析：

```text
analyze_relay(source_session, target_route) -> RelayCompatibility

RelayCompatibility
  compatible
  compatible_with_loss:
    dropped_or_transformed_items[]
    warnings[]
  blocked:
    unsupported_items[]
    recovery_actions[]
```

默认规则：

- 普通 text message 和已完成的 function call/result 可在经验证映射后迁移。
- `previous_response_id`、encrypted reasoning、hosted tool、MCP/computer-use item、image/audio/file input 或未完成 tool state 在目标 route 无明确能力时必须 blocked，不静默删除。
- `compatible_with_loss` 只用于不影响后续协议正确性的表示层降级，且必须由用户明确确认。
- relay executor 只接受 `compatible` 或已确认的 `compatible_with_loss`，对 `blocked` 不产生部分目标 session。
- source session 分析和目标转换不读取或复制任何 auth/secret。

### 11.5 API DTO 与前后端兼容

持久化模型、领域模型和 Tauri API DTO 不共用同一个 struct，避免 store 迁移字段意外暴露给前端。

```text
LegacyCreateProviderRequest   # 只在迁移窗口内读取 wireApi/envKey
CreateProviderRequestV2      # protocol/adapter/secret input
ProviderProfileView          # 只包含 secret reference，不包含 secret value
ProfileProviderBindingView   # binding revision + managed projection summary
PlanAttachRequest            # profile/provider/model + expected binding revision/config hash
ProfileAttachPlanView        # dry-run 预览
ExecuteAttachRequest         # dry-run plan id/fingerprint，防止执行过期计划
DetachProviderRequest        # profile id + expected binding revision
```

兼容规则：

- `CreateProviderRequestV2` 使用 `protocol`，不再让新 UI 提交任意 `wireApi` 字符串。
- legacy `wireApi = responses | openai` 在 API 边界转成 `protocol = responses`，并产生 migration warning；其他值拒绝。
- list/get binding 返回 revision；attach/rebind/detach 必须显式携带 expected revision，不通过服务端“先读后猜”屏蔽并发冲突。
- execute attach 必须验证 dry-run plan fingerprint 仍与 provider/binding/config/Gateway endpoint 版本匹配，否则要求重新 dry-run。
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
  route_plan.rs
  attach_plan.rs
  binding_store.rs
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
  lam.rs                        # lam codex launcher，负责 CODEX_HOME/readiness/exec

apps/desktop/src-tauri/src/services/adapters/
  mod.rs
  registry.rs
  chat_completions.rs
  errors.rs
  schema.rs
```

Responses Provider 在首版直连 Codex，因此 MVP 不实现未被运行路径使用的 `responses_passthrough` adapter。当未来确实需要 Gateway 统一代理 Responses Provider 时，再作为独立纵向切片增加。

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
- 创建 Chat Completions Provider 启用 adapter 可生成 route/attach plan。
- 旧 provider store 可迁移读取。
- legacy array store -> versioned envelope 的 fixture 迁移是确定性的，读取本身不写回。
- 未知未来 `schema_version` 可读错误但不可被覆盖。
- revision 不匹配时返回结构化冲突，不丢失其他实例的更新。
- 原子写入失败不破坏旧 store。
- reserved provider id 返回结构化错误。
- 缺少 base_url/model/secret reference 返回结构化错误。
- default model 不在 model catalog 时拒绝；自定义模型加入 catalog 后可用。
- endpoint join 保留 base URL path prefix，覆盖尾随斜杠、空 path、`..`、query 和 host injection。

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
- detach 只恢复 LAM-managed keys，保留用户后续新增的 table/key/comment。
- 用户修改受管 key 后 detach 返回 `CODEX_CONFIG_OWNERSHIP_CONFLICT`，不覆盖。
- LAM 创建且无 unmanaged key 的 provider table 可删除；预存或已扩展 table 保留。
- 同秒多次 attach/rebind 的备份名不冲突。

Profile binding：

- `ProfileBindingStore` 是 used-by 查询的唯一来源。
- expected revision 不匹配时 attach/rebind/detach 均无副作用。
- dry-run 后 provider/binding/config/Gateway endpoint 任一版本改变时 execute 拒绝过期 plan。
- config 写入后 binding commit 失败时按 journal 只恢复 managed keys。
- rebind 成功后旧 gateway token 失效，失败时旧 binding 仍可用。
- 每个 Gateway 请求使用开始时的不可变 snapshot，并发编辑不改变 in-flight route。

Secret store：

- env mode 只检查变量存在，不返回 secret。
- keychain 空 secret 不写 metadata。
- auth command 和 env_key 互斥。
- Responses API 帐号生成私有 Codex `auth.json`，不生成 auth helper；旧
  Keychain 帐号仅在一次性迁移时读取，成功后 Provider 引用改为 `codex_profile`。
- gateway binding 每 profile 独立，detach 后 token 失效，轮换后旧 token 失效。
- auth helper timeout、超大 stdout、非零退出和多余输出均返回结构化错误。
- debug format 不包含 secret。

Route/attach planner：

- responses -> direct plan。
- chat_completions + adapter -> gateway plan。
- chat_completions without adapter -> `PROVIDER_ADAPTER_REQUIRED`。
- capability resolver 正确汇总 declaration/verification/override warning。
- `ProviderRoutePlan` 不依赖 profile/Gateway 运行时状态。
- `ProfileAttachPlan` 只从显式 context 生成 gateway URL、binding spec 和 config patch。
- verification 的 provider/model/endpoint/adapter/policy/suite 任一指纹变化后不复用旧结果。

Adapter：

- Responses request text input -> Chat Completions messages。
- Chat Completions non-stream response -> Responses response。
- Chat Completions stream delta -> Responses stream events。
- tool call request/response roundtrip。
- DeepSeek `reasoning_content` 在内部有界保留但不伪装成 Responses summary/content，
  对外 capability/warning 为 partial。
- DeepSeek thinking tool call 样例：assistant thinking + `tool_calls` -> Responses tool call item。
- DeepSeek tool result replay：Responses tool result -> Chat Completions `role = "tool"` with `tool_call_id`。
- DeepSeek 普通完成轮次可省略旧 reasoning，但含 tool call 的 assistant message 必须完整回放 `reasoning_content`。
- 不把 `reasoning_content` 当作普通 assistant `content` 回放。
- upstream error -> structured Responses error。
- malformed upstream response/SSE -> 终止当前 response，不继续组装 tool arguments。
- usage 字段映射。
- 同一 adapter 并发创建的 `AdapterExchange` 之间不共享 ID、分片或状态。
- `finish/cancel` 在各合法和非法状态下返回稳定结果，不产生重复终止事件。

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
- `lam codex` 在 Gateway 停止和系统重启后可幂等拉起 sidecar，并传递原始 Codex args/exit code。
- 直接运行 `CODEX_HOME=... codex` 时 Gateway 不可用的情形有明确错误/恢复建议，不声称自动 bootstrap。
- 收到任何 upstream/response byte 后不重试；429/5xx 不在 Gateway 内重放。

Relay/session：

- API profile 可以作为 relay target。
- API profile 可以作为 relay source。
- target provider/model 决定 resume runtime。
- provider mismatch warning 准确。
- sync/relay 不复制 `auth.json`、API secret、provider store secret。
- 包含 unsupported hosted tool、encrypted reasoning、file/image/audio 或未完成 tool state 时 relay analysis 返回 blocked 且不创建部分目标 session。
- `compatible_with_loss` 未获用户确认时不执行。

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
- 有 route/attach plan preview。
- 有 capability matrix。
- Tauri commands 暴露 gateway start/status/test。

### 13.5 安全测试

- `providers.json` 不包含 `sk-`、DeepSeek key、Bearer token。
- `config.toml` 不包含 secret。
- gateway logs 不包含 request body、prompt、tool arguments、secret。
- sync manifest 不包含 secret。
- command-backed auth helper stdout 只输出 token，stderr 不含 token。

### 13.6 Phase 0 决策追踪

测试名是后续 issue 的规范名称；实现文件可调整，但不得删除覆盖关系。

| Phase 0 决策                                                      | 实现 issue                | 必须出现的验证测试                                                             |
| ----------------------------------------------------------------- | ------------------------- | ------------------------------------------------------------------------------ |
| `ProviderProtocol`/`RouteKind` 闭集，route plan 显式 route kind   | RPG-102, RPG-106          | `route_plan_selects_direct_or_gateway_and_rejects_unknown_protocol`            |
| `ProviderModel` catalog/default 约束                              | RPG-102                   | `provider_model_catalog_requires_default_and_unique_ids`                       |
| Codex options；Gateway retry 两项强制 0                           | RPG-105, RPG-106          | `gateway_projection_disables_both_codex_retry_budgets`                         |
| Credential storage 与 upstream bearer/none 分离                   | RPG-103                   | `credential_source_does_not_imply_upstream_auth`                               |
| Auth command argv/timeout/output/environment 边界                 | RPG-109                   | `auth_command_runner_rejects_shell_symlink_timeout_and_oversized_output`       |
| Gateway credential 仅持久化 ref/hash/generation                   | RPG-301                   | `gateway_credential_store_never_serializes_bearer_token`                       |
| Managed patch allowlist/CAS 与 projection ownership               | RPG-105                   | `config_patch_changes_only_allowlisted_keys_and_detects_ownership_conflict`    |
| readiness blocker 闭集、稳定顺序、多 blocker                      | RPG-102, RPG-401          | `readiness_reports_all_blockers_in_stable_order`                               |
| 非路由 edit 可用；路由 edit 要求全量 rebind                       | RPG-102, RPG-107          | `route_breaking_provider_edit_is_atomic_across_bound_profiles`                 |
| BindingStore 只表示 externally attached profiles                  | RPG-104                   | `binding_store_has_no_placeholder_for_chatgpt_or_unattached_profile`           |
| adoption 不取得原 config 所有权                                   | RPG-104, RPG-105          | `adopted_binding_detach_preserves_preexisting_config`                          |
| drift/rename/delete/unmanageable config 零破坏                    | RPG-104, RPG-105, RPG-107 | `binding_lifecycle_rejects_drift_path_move_delete_and_unmanageable_config`     |
| installation lock、revision CAS、原子 fsync/rename                | RPG-101                   | `versioned_store_serializes_processes_and_survives_atomic_write_failure`       |
| journal commit point 与 crash rollback/roll-forward               | RPG-107                   | `attach_journal_recovers_every_fault_before_and_after_binding_commit`          |
| dry-run fingerprint TTL、输入覆盖、一次性 replay                  | RPG-106                   | `attach_plan_rejects_expiry_replay_and_each_stale_fingerprint_input`           |
| Gateway application retry owner为 none；仅未送达建连重试一次      | RPG-303, RPG-308          | `upstream_retry_occurs_only_before_delivery_and_before_any_response_byte`      |
| Adapter registry `Send + Sync`、exchange 单 owner、输出模式不混用 | RPG-202, RPG-205          | `adapter_exchanges_are_request_isolated_and_enforce_output_mode`               |
| raw `reasoning_content` 不伪装 summary/content                    | RPG-208                   | `deepseek_raw_reasoning_is_bounded_preserved_for_tools_and_suppressed_on_wire` |

文档型 RPG-003 不制造代码红灯；上述测试必须在对应 issue 实现之前先加入并观察预期
失败。RPG-005 的 gate verifier 检查这张表存在且没有空的 issue/test 单元格。

### 13.7 RPG-004 threat-to-test matrix

Trust assumption：LAM 保护边界是其他普通本地进程、意外泄漏、恶意/损坏 metadata 和
远端 endpoint；已完全控制当前 macOS account、解锁 Keychain 和 LAM process memory 的
攻击者为 out of scope。所有 `accepted residual` 必须在 UI/docs 可见，不能标成 mitigated。

| Threat ID                | Attack / disposition                                                                                                                                                                                                                                          | Control owner                               | Required red-first test                                                         |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------- | ------------------------------------------------------------------------------- |
| `port_spoofing`          | 其他进程占稳定端口或伪造 `/healthz`；mitigated by bundle identity + same-uid Unix peer + nonce HMAC + matching HTTP instance，失败不发送数据/不换端口                                                                                                         | RPG-301, RPG-302, RPG-305                   | `sidecar_identity_rejects_port_only_health_spoof_and_wrong_control_hmac`        |
| `token_exposure`         | argv/env/files/log/crash/control/Keychain label 泄漏；mitigated by Keychain value、fd bootstrap、opaque label、redaction and marker scan；auth helper stdout 给 Codex 是 required trusted channel                                                             | RPG-108, RPG-109, RPG-301, RPG-405          | `gateway_tokens_never_appear_outside_keychain_and_helper_stdout_pipe`           |
| `auth_command_execution` | metadata shell injection、binary replacement、unsafe cwd/env、hang、large output/cache；mitigated by explicit HMAC approval, absolute open/hash, direct argv, empty cwd/home, allowlist, timeout/size/process-group kill                                      | RPG-103, RPG-109                            | `auth_command_requires_approved_fingerprint_and_enforces_runner_limits`         |
| `ssrf_dns`               | private/metadata target、userinfo/path/query injection、redirect/proxy、DNS rebinding；Gateway mitigated by canonical URL, public-address set, pinned connector, no redirects/proxy；direct Codex authoritative-DNS rebinding is accepted residual and warned | RPG-105, RPG-106, RPG-303, RPG-405          | `upstream_policy_rejects_private_dns_rebinding_redirect_userinfo_and_injection` |
| `sensitive_headers`      | static header stores API key or overrides auth/host；mitigated by no MVP static headers, env header name allowlist, forbidden auth/host headers                                                                                                               | RPG-102, RPG-105, RPG-303                   | `provider_rejects_static_sensitive_and_routing_headers`                         |
| `resource_exhaustion`    | oversized body/SSE/tool/response/store, concurrency, queue, connection, time/disk exhaustion；mitigated by 9.7 numeric limits before allocation/forwarding                                                                                                    | RPG-205, RPG-302–RPG-305, RPG-405           | `gateway_enforces_every_resource_budget_and_releases_permits_on_cancel`         |
| `path_symlink`           | config/store/backup/journal/token/component traversal, symlink swap or unsafe mode；mitigated by fixed roots, component validation, current-uid regular files, openat/lstat recheck, same-filesystem atomic replace                                           | RPG-101, RPG-105, RPG-108, RPG-305, RPG-306 | `provider_hub_rejects_symlink_traversal_owner_mode_and_component_swap`          |
| `binding_rotation`       | token shared across profiles or rotation/detach race keeps old token valid；mitigated by per-profile binding, atomic generation switch, no grace, immutable in-flight snapshot, revocation_pending recovery                                                   | RPG-107, RPG-301, RPG-308                   | `binding_rotation_is_profile_isolated_and_old_token_fails_immediately`          |
| `content_leakage`        | prompt/response/reasoning/tool/secret leaks to log/evidence/frontend/session/relay/fixture；mitigated by allowlist telemetry, synthetic-only evidence and whole-repo marker scan                                                                              | RPG-110, RPG-208, RPG-304, RPG-405          | `synthetic_secret_and_content_markers_never_cross_allowed_sinks`                |
| `version_mismatch`       | incompatible sidecar/control/state during upgrade or downgrade；mitigated by versioned manifest/protocol, authenticated graceful replacement, no mutation on mismatch, future-schema refusal                                                                  | RPG-101, RPG-305, RPG-306, RPG-308          | `sidecar_upgrade_rejects_protocol_state_and_binary_identity_mismatch`           |
| `unsupported_platform`   | partial feature writes unsafe state on unverified OS/arch；mitigated by capability gate before secret read or mutation and read-only legacy export                                                                                                            | RPG-101, RPG-110, RPG-306, RPG-405          | `unsupported_platform_is_read_only_and_has_zero_remote_provider_side_effects`   |

Repository-wide leak tests use exact synthetic markers; fixtures may contain them only in the
security-test input file explicitly allowlisted by RPG-405, never in golden output/checksum logs:

```text
LAM_TEST_SECRET_API_KEY_sk-rpg405-7f3a
LAM_TEST_SECRET_BEARER_rpg405.eyJub3QiOiJyZWFsIn0.signature
LAM_TEST_SECRET_COMMAND_STDOUT_cmd-rpg405-91bd
LAM_TEST_CONTENT_PROMPT_prompt-rpg405-2c6e
LAM_TEST_CONTENT_TOOL_ARGS_tool-rpg405-44aa
LAM_TEST_CONTENT_REASONING_reason-rpg405-08d1
LAM_TEST_CONTENT_RESPONSE_response-rpg405-b72f
```

Scanner targets至少包括 Provider/binding/gateway/journal JSON、Codex config/backup/temp、
logs/crash context、Tauri DTO/frontend state、sessions/relay/sync manifests、conformance
evidence、capture fixtures and test output。允许的 auth-helper stdout pipe 使用单独进程内
assertion，不能写 snapshot。RPG-005 gate 检查 threat ID、owner 和 test name 非空。

## 14. 验收标准

### 14.1 Responses Provider

- 用户创建 Responses Provider。
- LAM 写入官方 Codex provider config。
- 原有 MCP、sandbox、approval、未知配置和注释保留不变。
- `lam codex --profile <id>` 在该 profile 下可以完成普通 prompt，Responses 直连路径不启动 Gateway。
- 不启动本地 adapter。
- health 显示 upstream available。

### 14.2 DeepSeek Chat Completions Provider

- 用户创建 DeepSeek Provider：

```text
protocol = chat_completions
base_url = https://api.deepseek.com
adapter = local(responses_to_chat_completions)
env_key = DEEPSEEK_API_KEY
model = deepseek-v4-pro
```

- LAM 启动本地 gateway。
- Codex config 指向 `127.0.0.1` gateway。
- Gateway 将 `/v1/responses` 转为 DeepSeek `/chat/completions`。
- 普通文本、streaming、tool calls 至少各有一个自动化测试通过。
- thinking tool-call 多轮测试证明 `reasoning_content` 在必需轮次完整回传。
- 关闭 LAM UI 后，已运行 sidecar 的 attached profile 仍能完成请求。
- Gateway 未运行或系统重启后，`lam codex --profile <id>` 可重新拉起 sidecar 并完成请求。
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
- 只有 compatibility analysis 为 compatible，或用户已确认 compatible-with-loss 时才执行 relay。
- 不可映射的状态返回 blocked 和精确 item/recovery action，不产生部分目标 session。

### 14.5 Attach/Detach 一致性

- `ProfileBindingStore` 是 provider used-by、launcher 和 Gateway binding 投影的唯一权威来源。
- rebind/detach 在 revision 或 config ownership 冲突时拒绝写入，不覆盖用户修改。
- detach 恢复 LAM 修改前的受管 key，保留其他 TOML 内容，并立即撤销 Gateway token。
- attach/rebind/detach 在每个故障注入点后均能恢复为一个明确状态，不留下可用 token 与无 binding 配置的组合。

### 14.6 本地统一管理

- Provider Center 可为明确选中的 profile 切换 provider/model，不存在隐式全局 active provider。
- Gateway status 可见。
- Codex profile 绑定和本地 gateway 使用同一套 Provider store、SecretStore 和 capability resolver。

## 15. 任务拆分

本节取代旧的“基础层一次扩展所有 struct” Phase 1 方案。向后兼容保证放在持久化迁移和 Tauri JSON DTO 边界，不要求 Rust 内部 struct 字段和函数参数永久不变。新契约和 legacy 输入通过独立 DTO 共存，领域模型只表达新语义。

Phase 0（契约与安全基线）：

1. 固定 legacy provider store、现有 Codex config 和前后端 JSON contract fixtures。
2. 用当前支持的 Codex 版本捕获 text、streaming、tool call、resume 请求，形成 Gateway contract fixtures。
3. 先写失败测试：现有 config 不可被整体覆盖、secret 不可序列化、legacy `openai` 不可写给 Codex。
4. MVP 路由已由 RPG-002 锁定为 `/healthz`、`GET /v1/models`、`POST /v1/responses`；cancel/retrieve/state route 不进入首版。

Phase 1（Responses Provider 直连闭环）：

5. 实现 versioned ProviderStore、ProfileBindingStore、纯迁移函数、原子写入和 revision 冲突检测。
6. 实现 SecretReference/SecretStore 与 env、Keychain、Codex auth helper 映射。
7. 实现纯 `ProviderRoutePlanner`/`ProfileAttachPlanner`、`ManagedConfigProjection` 和非破坏性 `CodexConfigEditor`。
8. 先完成 attach/rebind/detach 的冲突、补偿和故障注入测试，再接 UI。
9. 同步更新最小 Provider UI 和前后端 DTO，保持 legacy request 的迁移入口。
10. 用 mock Responses provider 完成 create -> dry-run -> attach -> Codex config -> test route -> detach 纵向验收。

Phase 2（纯 Adapter 转换闭环）：

11. 实现受控 Responses 子集 schema、Adapter trait/registry 和每请求 `AdapterExchange` 状态机。
12. 实现 text、streaming、function tool、usage 和 error 转换，每项先增加 fixture test。
13. 实现 DeepSeek compatibility policy，覆盖 thinking + tool call 中 `reasoning_content` 必须保留的回归样例。

Phase 3（Gateway 可运行闭环）：

14. 实现 sidecar、`lam codex` launcher、安装 manifest、单实例锁、稳定端口、auth helper、binding 轮换/撤销和安全日志。
15. 实现 `/healthz`、`GET /v1/models`、`POST /v1/responses`；不实现 Phase 0 未观测到的 cancel/retrieve/state 路由。
16. 实现 upstream client 的 timeout、取消、有界背压和单重试所有者策略。
17. 用 mock DeepSeek 完成 create -> attach -> launcher/sidecar start -> Codex Responses request -> Chat Completions -> streaming/tool result 的端到端验收。

以上 Phase 3 项目已由 G4 验证：真实 loopback HTTP Gateway、版本化 Keychain/env 凭据边界、attach/detach 事务、non-stream/SSE/function tool、UI 进程退出后的 sidecar 存活、旧 token 拒绝、恢复/端口迁移/安全/重试矩阵以及最终 `.app`/DMG 完整性均通过。

Phase 4（产品集成与接力）：

18. 完成 Provider Center、route/attach plan preview、readiness/health/capability provenance 展示。
19. 先实现纯 `RelayCompatibilityAnalyzer` 和 blocked/no-partial-write 测试，再完成 Session/relay/provider mismatch 联动与 secret 不迁移验收。
20. 完成 smoke、安全测试、README 和旧 v0.2 文档同步。

MVP 里程碑：Phase 0–3 全部通过后，才算 DeepSeek 端到端演示闭环。Phase 1 单独交付一个可用的 Responses Provider 直连功能，不是只铺设抽象。

每个纵向切片必须先补测试并确认失败，再实现；自动化测试不得推迟到最后阶段。

## 16. 设计约束

- 不允许在核心流程里散落 provider-specific 分支。
- 供应商差异放在 provider preset、capability metadata 或 adapter 实现里。
- 协议差异放在 `ProtocolAdapter`。
- Codex 配置差异放在 `ProviderRoutePlan`/`ProfileAttachPlan` 和 `CodexConfigEditor`。
- UI 展示差异来自同一份 route/attach plan 与 capability resolver。
- Profile/provider/model 关系只以 `ProfileBindingStore` 为权威来源，config 和 Gateway binding 不反向篡改领域状态。
- Detach/rebind 必须尊重 config ownership 和 expected revision，不覆盖用户并发修改。
- 所有错误使用结构化 `AppError` code，不能只返回字符串。
- 文件写入必须备份、解析校验、失败不破坏原文件。
- 所有网络测试使用 mock server；真实 Provider 只用于手工验收。

## 17. 后续扩展

新增 Anthropic-compatible Provider 时，不修改 attach/gateway/config editor 主流程：

1. 新增 `responses_to_anthropic_messages` adapter。
2. 注册到 `ProtocolAdapterRegistry`。
3. 新增 provider preset。
4. 补 capability tests。

新增本地客户端统一 API 时，不影响 Codex profile：

1. 在 gateway 增加 `/v1/chat/completions` 入口。
2. 用同一套 ProviderRoutePlan 选择上游。
3. 继续复用 SecretStore 和 capability resolver。
