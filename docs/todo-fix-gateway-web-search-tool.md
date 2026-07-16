# Todo: 修复 Gateway 对 Codex web_search 工具的解析

> Executor instructions: Follow this todo step by step. Generate tests from
> the "Test design" section before implementation. Run each verification command
> and confirm the expected result before moving to the next task.

## Status

- **Priority**: P0
- **Effort**: S
- **Risk**: MEDIUM
- **Depends on**: controlled Responses request schema
- **Category**: bugfix
- **Planned at**: current working tree

## Why this matters

**Background**: A real `codex-idragon3` prompt reaches the local Gateway but fails with `unknown variant web_search, expected function or namespace`.

**Current state**: Responses passthrough validates the request through `ResponsesTool`, whose enum only models `function` and `namespace`. The request is rejected before the configured upstream API is called.

**Impact**: Normal Codex startup cannot complete whenever Codex advertises its built-in web search tool.

**What improves**: Responses Providers can receive Codex `web_search` definitions unchanged, while Responses-to-Chat-Completions adapters continue to fail closed for hosted tools they cannot represent.

## Scope

**In scope**:
- Parse and preserve the current `web_search` Responses tool shape.
- Explicitly reject it in Chat Completions translation.
- Protocol, route, adapter, and real-command verification.

**Out of scope**:
- Emulating web search locally.
- Translating hosted search into a function tool.
- Supporting unrelated future hosted tool variants.

## Design

- `ResponsesTool::WebSearch` stores additional fields without interpreting or dropping them.
- Responses passthrough continues forwarding the original body byte-for-byte after validation.
- Chat Completions translation reports `UnsupportedTool` at the exact tool type path.
- Launcher ownership validation compares only LAM-managed fields, allowing Codex to add project/TUI/runtime settings while still rejecting endpoint, auth, model, or retry drift.
- Production SSRF validation permits RFC 2544 `198.18.0.0/15` proxy fake-IP answers used by local TUN resolvers, while continuing to reject loopback, private, link-local, metadata, multicast, and unspecified targets.
- Existing tool-count and definition-size limits remain in force.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Accept web_search on Responses passthrough | Real prompt no longer fails with `GATEWAY_INVALID_REQUEST` | 验证成功 |

### T1: Accept web_search on Responses passthrough

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: The controlled schema is narrower than the observed Codex request contract.

**What to do**:
- Extend `ResponsesTool` with a lossless `web_search` variant.
- Add protocol/route acceptance tests and adapter rejection tests.
- Rebuild/restart the development Gateway and execute a real prompt.

**Logic design**:
- Use a flattened JSON map for hosted-tool options so current optional fields remain forward-compatible.
- Do not translate this variant to Chat Completions.
- Do not alter credentials, endpoint, model, or other tools.

**Test design**:
- Normal: parse a `web_search` tool with optional fields.
- Passthrough: Responses route forwards the original body and accepts the model.
- Edge: additional web-search options survive deserialization/serialization.
- Error: Chat Completions translation returns `UnsupportedTool` at `$.tools[0].type`.
- Regression: existing function and namespace behavior remains unchanged.
- State: unrelated Codex config additions are accepted; a managed Provider value change remains blocked.
- Network: the observed proxy fake IP is allowed and sensitive local address classes remain forbidden.

**Acceptance**:
- `cargo test --test provider_adapter_schema --test provider_adapter_request --test provider_gateway_routes web_search`
- Relevant full adapter and route test suites pass.
- `codex-idragon3 exec "hello"` reaches the upstream and returns a model response without `GATEWAY_INVALID_REQUEST`.

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Relevant full suites and formatting pass
- [x] Real command verification passes
- [x] Task overview and status are `验证成功`

## Test plan

- Protocol parse/round-trip for `web_search` options.
- Adapter fail-closed behavior.
- Gateway passthrough behavior with unchanged request body.
- Real API account prompt.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused tests | `cargo test --test provider_adapter_schema --test provider_adapter_request --test provider_gateway_routes web_search` | exit 0 after implementation; fail before implementation |
| Relevant suites | `cargo test --test provider_adapter_schema --test provider_adapter_request --test provider_gateway_routes` | exit 0 |
| Formatting | `cargo fmt --all -- --check` | exit 0 |
| Real prompt | `codex-idragon3 exec "hello"` | upstream model response; no Gateway parse error |

## Done criteria

- [x] Every task status is `验证成功`
- [x] All task Done criteria are checked
- [x] No STOP condition remains

## STOP conditions

- The observed tool shape cannot be represented losslessly.
- Upstream rejects `web_search` after local parsing is fixed and requires a product decision to strip it.
- Verification would require changing credentials or Provider endpoints.
- Tests still fail after five repair iterations.
