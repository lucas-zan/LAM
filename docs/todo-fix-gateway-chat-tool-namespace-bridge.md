# Todo: 完整实现 Gateway Responses↔Chat 工具双向桥接

> Executor instructions: Follow this todo step by step. Generate or update
> behavior tests before implementation. Run the expected RED command before
> each implementation task, then run focused tests, related regressions, and
> formatting before marking the task successful.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: HIGH
- **Depends on**: RPG-203, RPG-204, RPG-205, RPG-206, controlled Responses schema
- **Category**: bugfix / protocol compatibility
- **Planned at**: current working tree
- **Implementation status**: namespace function bridge and automated verification
  complete; the configured Command Code smoke test now reaches the next
  capability boundary (`web_search`) and is blocked pending an upstream search
  contract

## Why this matters

Codex sends the Responses wire format. A Responses-native Provider may be
forwarded directly, but a Chat Completions Provider must be reached through a
complete bidirectional bridge:

```text
Codex Responses request
  → Gateway request translation
  → upstream Chat Completions
  → Gateway JSON/SSE response translation
  → Codex Responses response
```

LAM already has this broad route in
`apps/desktop/src-tauri/src/services/gateway/routes.rs`. The namespace function
part of the bridge is now implemented. A real Command Code turn then exposed a
separate boundary: the generic Chat adapter still rejects hosted
`web_search` before the upstream request is sent:

- `services/adapters/request.rs::translate_tools`
- `services/adapters/protocol.rs::ResponsesTool`
- `tests/provider_adapter_request.rs::unsupported_items_tools_and_policy_fields_fail_before_upstream_request`

The original namespace error was caused by an incomplete compatibility bridge,
not by the Responses ingress or by a malformed Codex request. After the
namespace fix, the current observed error is:

```text
ADAPTER_UNSUPPORTED_TOOL
web_search and other hosted tools are not representable by generic Chat Completions at $.tools[9].type
```

The current error also groups capabilities with different solutions. A
namespace containing function tools can be represented by Chat Completions via
an invertible name mapping. Generic Chat Completions cannot, by itself,
implement hosted search or computer semantics. Those capabilities require a
native Responses route, a provider-specific adapter, or an explicit
capability error.

## Behavior contract

### Required behavior

- Codex-facing input and output remain Responses format.
- Responses-native Providers continue to receive the original request and
  response without Chat conversion.
- Chat Completions Providers receive a valid Chat request at
  `/chat/completions`.
- Namespace child functions, including MCP and `multi_agent_v1` function
  tools, are flattened only at the Chat boundary and restored before returning
  to Codex.
- `call_id`, Chat `tool_call_id`, tool names, namespaces, argument bytes, and
  JSON/SSE event ordering remain correlated across the complete tool loop.
- Non-streaming and streaming conversions use the same request-scoped
  translation context.
- A tool that cannot be represented by the selected upstream is never silently
  removed.
- The Gateway never changes Codex configuration to disable `multi_agent` as a
  workaround.

### Explicit capability behavior

- `function`: supported for Chat Completions.
- `namespace` containing function children: supported through flatten/restore.
- MCP namespace function children: supported through the same flatten/restore
  seam; MCP execution remains on the Codex side.
- `web_search`, hosted tools, and computer tools: remain explicit capability
  decisions. They must either use a native/provider-specific mapping or return
  a precise unsupported-capability error. They must not be dropped.
- `store` and `previous_response_id`: remain explicitly unsupported on the
  current full-history Chat adapter until a response/history store is added.
  This is a separate capability boundary and must not be confused with the
  namespace fix.

### Non-goals

- Emulating web search or computer execution inside the generic Chat adapter.
- Silently converting hosted tools into unrelated function tools.
- Disabling Codex features globally or mutating Provider configuration during
  request translation.
- Supporting unknown future Responses tool types without a defined semantic
  mapping.
- Changing the native Responses passthrough contract.

## Proposed design

### Request-scoped translation context

Change the Chat request translation seam from a request body only to a body plus
an invertible context:

```rust
struct TranslatedChatRequest {
    request: ChatCompletionRequest,
    context: ToolTranslationContext,
}
```

`ToolTranslationContext` must contain at least:

```text
flat Chat function name
  → original function name
  → optional namespace
  → original tool kind
  → original tool identity/schema metadata
```

The context is created once from the incoming Responses request and is passed
to both the non-streaming and streaming response converters. It must not be a
global cache or shared between concurrent requests.

### Namespace flattening

For a request such as:

```json
{
  "type": "namespace",
  "name": "mcp__files__",
  "tools": [
    { "type": "function", "name": "read", "parameters": {} }
  ]
}
```

the Chat request may contain:

```json
{
  "type": "function",
  "function": {
    "name": "mcp__files____read",
    "parameters": {}
  }
}
```

The flattening algorithm must:

- preserve description, parameters, and strictness;
- recursively handle supported namespace/function children;
- produce a valid Chat function name of at most 64 bytes;
- detect collisions with top-level functions and other namespaces;
- use deterministic truncation/hash suffixes when names exceed the limit;
- fail explicitly on a collision instead of choosing an ambiguous mapping.

The reverse map restores `name` and `namespace` in Responses function-call
items and related SSE events.

### Request item and tool-choice preservation

Extend the controlled protocol model so the adapter does not discard namespace
identity while deserializing:

- `ResponsesInputItem::FunctionCall` carries an optional namespace;
- namespaced function tool choices can be mapped to their flattened name;
- namespace tool definitions are represented recursively or through a lossless
  raw representation that can be inspected by the bridge;
- output function-call items carry an optional namespace;
- unsupported tool variants retain their type and exact request path for a
  precise capability error.

For each input function call:

```text
Responses function_call(namespace, name, call_id, arguments)
  → Chat tool_calls(function.name = flat_name, id = call_id)
```

For each function result:

```text
Responses function_call_output(call_id, output)
  → Chat role=tool(tool_call_id = call_id, content = output)
```

The adapter must validate duplicate IDs, unknown result IDs, argument JSON, and
the existing size limits before making the upstream request.

### Response conversion

#### Non-streaming

`convert_nonstream_response` receives `ToolTranslationContext` and resolves
each Chat tool call name through the reverse map. It returns a Responses
function-call item with the original namespace, name, call ID, and arguments.
Unknown upstream tool names must produce a stable conversion error or a plain
declared top-level function call; they must never be silently renamed or
dropped.

#### Streaming

`StreamingAdapter` receives the same context and must:

- buffer a tool-call accumulator when the first Chat delta has no ID or name;
- wait for the identity fields before emitting `response.output_item.added`;
- aggregate arguments by Chat tool index with bounded memory;
- preserve parallel call ordering and unique call IDs;
- restore namespace/name in `response.output_item.added`, argument-delta,
  argument-done, output-item-done, and terminal response events;
- emit at most one terminal event and retain existing malformed-frame,
  overflow, cancellation, and missing-DONE protections.

### Provider capability boundary

Extend the compatibility policy or provider capability profile so the route can
distinguish:

```text
native Responses passthrough
Chat + function tools
Chat + namespace function bridge
Chat + provider-specific hosted/search bridge
Chat + computer bridge
```

The generic Chat profile should advertise the namespace function bridge after
this Todo is complete. It must not advertise hosted/computer support unless an
actual reversible provider adapter exists.

Error responses should identify the exact unsupported tool type and path, for
example `$.tools[1].type`, instead of using one broad message for namespace,
MCP, hosted, and computer tools.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Extend controlled tool schema and mapping context | Namespace identity and unsupported variants survive parsing without silent loss | 已完成 |
| T2 | Implement Responses→Chat namespace request bridge | Plain functions, namespace children, tool calls, and tool results translate reversibly | 已完成 |
| T3 | Implement Chat→Responses JSON/SSE namespace restoration | Non-streaming and fragmented streaming tool calls restore namespace and call IDs | 已完成 |
| T4 | Integrate provider capabilities and end-to-end route tests | Native passthrough is unchanged and Chat routing has explicit capability behavior | 已完成 |
| T5 | Resolve hosted web_search for the configured Chat Provider | Search semantics use a declared native/provider-specific executor; no silent drop | 阻塞：缺上游契约 |

### T1: Extend controlled tool schema and mapping context

**Status**:
- [x] 实现完成
- [x] 已测试验证
- [x] 验证成功
- [ ] 验证失败

**Expected RED**:

- Add schema tests for a namespace containing real function children and a
  namespaced function-call input item.
- Run:

  ```bash
  cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml \
    --test provider_adapter_schema --test provider_adapter_request \
    namespace_mapping
  ```

- The tests must fail because the current input/output/tool-choice types do not
  retain the namespace mapping required by the bridge.

**Implementation**:

- Add the minimum namespace fields to request and output types.
- Introduce a request-scoped `ToolTranslationContext`.
- Add deterministic flatten-name generation, collision detection, and bounds
  tests.
- Keep native Responses passthrough parsing lossless for fields it does not
  interpret.

**Verification**:

- Normal: one namespace with one or more function children.
- Edge: nested namespace, long names, duplicate names, and empty children.
- Invalid: malformed namespace shape, collision, oversized definition, and
  unsupported tool type.
- Security: flattening cannot inject invalid function names or exceed existing
  request/tool limits.

### T2: Implement Responses→Chat namespace request bridge

**Status**:
- [x] 实现完成
- [x] 已测试验证
- [x] 验证成功
- [ ] 验证失败

**Expected RED**:

- Replace the current rejection-only namespace test with behavior tests that
  assert a flattened Chat tool and mapped function call.
- Add function-output follow-up tests and run:

  ```bash
  cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml \
    --test provider_adapter_request namespace
  ```

- The tests must fail at the current `UnsupportedTool` branch.

**Implementation**:

- Make translation return `TranslatedChatRequest` rather than only
  `ChatCompletionRequest`.
- Flatten namespace function children into Chat function tools.
- Map namespaced function calls and function outputs while preserving
  `call_id`/`tool_call_id`.
- Map supported namespaced function tool choices.
- Reject namespace choices or tool variants that cannot be represented with an
  exact capability error; do not silently degrade to `auto`.
- Keep `web_search`, hosted, and computer tools out of the generic Chat body
  unless a provider-specific translator is added.

**Verification**:

- Normal: plain function and namespace function tools can coexist.
- Edge: parallel calls, repeated tool names in different namespaces, and
  arguments at the 1 MiB boundary.
- Invalid: duplicate call ID, unknown function output ID, invalid JSON
  arguments, name collision, and unsupported hosted/computer tool.
- State: a follow-up request maps the previously returned namespaced call back
  to the same flattened Chat name.

### T3: Implement Chat→Responses JSON/SSE namespace restoration

**Status**:
- [x] 实现完成
- [x] 已测试验证
- [x] 验证成功
- [ ] 验证失败

**Expected RED**:

- Add non-streaming and streaming round-trip tests using a Chat response with
  flattened names.
- Add SSE fixtures where ID, name, and arguments arrive in separate chunks.
- Run:

  ```bash
  cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml \
    --test provider_adapter_nonstream --test provider_adapter_sse \
    namespace
  ```

- If these test targets do not exist, create the mapped equivalents before
  implementation and record the actual command in Evidence.

**Implementation**:

- Pass the context into `convert_nonstream_response`.
- Pass the context into `StreamingAdapter`.
- Restore namespace/name on all function-call output items and relevant SSE
  events.
- Defer the first tool output event until ID and name are available, within
  existing bounded limits.
- Preserve usage, finish status, malformed-stream errors, cancellation, and
  terminal-event guarantees.

**Verification**:

- Normal: text-only, tool-only, and mixed text/tool responses.
- Edge: split UTF-8/SSE frames, split JSON arguments, parallel tool indexes,
  late ID/name fields, and usage-only chunks.
- Invalid: duplicate IDs, malformed arguments, missing identity, unknown
  flattened name, missing `[DONE]`, and oversized arguments.
- Round trip: Responses request → Chat request → Chat response → Responses
  response preserves namespace, function name, call ID, and arguments.

### T4: Integrate provider capabilities and end-to-end route tests

**Status**:
- [x] 实现完成
- [x] 已测试验证
- [x] 验证成功
- [ ] 验证失败

**Expected RED**:

- Add mock-upstream route tests for both native Responses and Chat providers.
- Run the focused Gateway suite before implementation; the namespace Chat
  case must currently return `ADAPTER_UNSUPPORTED_TOOL`.

**Implementation**:

- Thread `TranslatedChatRequest.context` through the route's non-streaming and
  streaming branches.
- Add provider capability metadata for namespace function bridging and
  hosted/computer support.
- Keep native Responses request bytes and tool declarations unchanged.
- Return precise Responses-compatible errors for unsupported capabilities.
- Update the Codex provider compatibility documentation/catalog only where it
  reflects an implemented capability; never disable `multi_agent` globally.

**Required fixture correction**:

The current captured Codex fixtures redact the namespace `tools` field. Add a
separate de-identified fixture that preserves the nested function structure;
the existing redacted fixture may remain for privacy/contract tests but cannot
prove namespace round-trip behavior.

**Verification**:

- Native Responses: `web_search`, namespace, and unknown future-compatible
  fields pass through unchanged when the provider is configured for native
  Responses.
- Chat request: upstream receives `/chat/completions`, flat function tools,
  `messages`, and preserved IDs.
- Chat response: Codex receives Responses JSON/SSE with restored namespace and
  call IDs.
- Negative: hosted/computer/web_search on a generic Chat provider is rejected
  explicitly and is never absent from the diagnostic without explanation.
- Concurrency: two simultaneous requests with identical flat names do not
  share or corrupt translation context.

### T5: Resolve hosted web_search for the configured Chat Provider

**Status**:
- [ ] 实现完成
- [ ] 已测试验证
- [ ] 验证成功
- [x] 阻塞：缺少上游搜索契约

**Observed smoke evidence**:

- The active binding is `account-cmd`, protocol `chat_completions`, model
  `deepseek/deepseek-v4-flash`, with the local Gateway translating to the
  Command Code Provider API.
- The namespace rejection no longer occurs. The request reaches
  `$.tools[9].type`, where `web_search` is rejected by the generic Chat
  adapter.
- Command Code's published Provider API exposes standard Chat Completions,
  Anthropic Messages, and models endpoints. It does not publish a hosted
  `web_search` or standalone search endpoint for this binding.
- An ordinary Chat function named `web_search` would only change the wire
  shape; without a real executor it would not perform hosted search or return
  valid search citations/results.

**Required decision before implementation**:

- [ ] Attach a Responses-native Provider that supports `web_search`.
- [ ] Provide a documented Command Code/provider-specific search contract,
      including request, response, streaming, auth, and citation semantics.
- [ ] Authorize a Gateway-owned search backend and its security/credential
      configuration; then implement the full hosted-tool lifecycle.

Until one of these contracts exists, returning the precise capability error is
the only behavior that preserves the user's requirement not to silently remove
or counterfeit model capabilities.

## Test plan

### Unit tests

- Protocol parse/round-trip for recursive namespace tools and namespaced call
  items.
- Deterministic flatten names, collision handling, and length limits.
- Request mapping for plain functions, namespace functions, tool calls, tool
  results, and tool choices.
- Non-streaming response restoration.
- Streaming restoration with arbitrary byte partitioning and late identity
  fragments.
- Precise unsupported-capability errors.

### Gateway integration tests

- Mock upstream asserts exact Chat request path/body.
- Mock Chat JSON response is returned as Responses JSON.
- Mock Chat SSE response is returned as Responses SSE with valid terminal
  ordering.
- Native Responses provider receives the original request body unchanged.
- Two concurrent requests use independent mappings.

### Existing regressions

- Existing plain function round-trip tests remain green.
- Existing web_search Responses passthrough tests remain green.
- Existing reasoning, DeepSeek, image, model-binding, auth, and response-limit
  tests remain green.
- Existing adapter and Gateway tests that currently assert namespace rejection
  are intentionally changed to assert namespace function bridging; tests for
  genuinely unsupported hosted tools continue to assert explicit rejection.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Schema/request focus | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_adapter_schema --test provider_adapter_request namespace` | namespace schema and request mapping pass |
| Response/SSE focus | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_adapter_nonstream --test provider_adapter_sse namespace` | JSON/SSE restoration passes |
| Gateway focus | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_gateway_routes` | native and Chat route integration pass |
| Related adapter regression | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_adapter_request --test provider_adapter_schema --test provider_gateway_routes --test provider_adapter_deepseek` | existing adapter behavior remains green |
| Formatting | targeted `rustfmt --edition 2021` on changed adapter/Gateway files | exit 0; workspace check is blocked only by unrelated pre-existing user edits |
| Diff hygiene | `git diff --check` | exit 0 |
| Real command | configured Codex Command Code account smoke test | no `ADAPTER_UNSUPPORTED_TOOL`; model can complete a namespace function turn |

## Acceptance criteria

- [x] Codex still sends Responses format at the Gateway boundary.
- [x] Native Responses Providers remain byte-preserving passthroughs.
- [x] Chat Providers convert namespace function tools without disabling
      `multi_agent`.
- [x] Namespace function calls and outputs survive a complete non-streaming
      and streaming round trip.
- [x] `call_id` and tool arguments remain correct across follow-up turns.
- [x] Unsupported hosted/computer capabilities are explicit and never silently
      dropped.
- [x] Tests are written before implementation and expected RED is recorded.
- [x] Focused and related regression checks pass; targeted changed files are formatted.
- [x] No unrelated working-tree changes are overwritten.

## Evidence

- Initial RED compile run failed at the missing
  `translate_responses_request_with_context`, response context converter, and
  namespaced output fields, confirming the tests covered the missing bridge.
- `ToolTranslationContext` now maps flat Chat names to original name/namespace
  identities, uses deterministic bounded hashing for long names, and rejects
  collisions.
- Namespace child functions translate to Chat names such as
  `remote__lookup`; non-streaming JSON and streaming SSE restore `remote` and
  `lookup` while preserving `call_id` and argument bytes.
- Streaming accumulation accepts arguments before a later ID/name delta and
  emits one bounded, ordered Responses tool item after identity is available.
- Generic Chat hosted-tool rejection now includes the exact request path;
  the compatibility policy explicitly advertises namespace function support,
  and `multi_agent` is not disabled.
- Added route tests prove both Chat JSON/SSE conversion and unchanged native
  Responses passthrough behavior.
- Focused tests passed: provider request 11, non-stream 5, SSE 7, Gateway
  routes 29. Full `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`
  passed with 0 failures (ignored tests remain unchanged).
- `git diff --check` passed. Targeted changed files were formatted. Full
  `cargo fmt -- --check` still reports only pre-existing formatting differences
  in the unrelated user-modified `provider_api_v2.rs` and
  `provider_phase4_api_account.rs`.
- The configured real Codex Command Code smoke test was run by the user after
  the namespace bridge: the failure advanced from namespace rejection to
  `web_search` at `$.tools[9].type`.
- The active Command Code binding uses the documented standard Chat Provider
  API. Its public contract has no hosted-search operation; unauthenticated
  probes of `/alpha/search` and `/provider/v1/alpha/search` returned `404`, so
  the undocumented standalone Codex search route cannot be enabled safely for
  this Provider.

## STOP conditions

- The real Codex namespace shape cannot be represented without changing the
  public tool execution contract.
- A provider-specific hosted/computer semantic is required but no upstream
  contract is available. This condition is active for T5.
- Flattening produces an unresolved name collision or cannot preserve call
  identity.
- Tests demonstrate that an upstream response cannot be mapped back to valid
  Responses events without silently changing tool semantics.
- Verification would require changing credentials, endpoints, or unrelated
  working-tree files.
- The same failure remains after five focused repair iterations; record the
  evidence and request a protocol/product decision.
