# Todo: Fix gateway `input_image` compatibility

> Executor instructions: Follow this todo step by step. Generate tests from the Test design before implementation, confirm the regression test fails, then implement and verify.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: MEDIUM
- **Depends on**: current remote-provider gateway adapter worktree
- **Category**: bugfix
- **Planned at**: current working tree on 2026-07-15

## Why this matters

**Background**: A Responses API request containing an OpenAI `input_image` content part is rejected locally with `GATEWAY_INVALID_REQUEST: unsupported content type: input_image`.

**Current state**: `adapters/protocol.rs` only admits `input_text` and `output_text`; `adapters/request.rs` flattens all accepted message content into a string before serializing an OpenAI-compatible Chat Completions request.

**Impact**: Vision-capable remote models cannot receive images through the LAM gateway even though their Chat Completions endpoints accept `image_url` content parts.

**What improves**: The gateway will parse Responses `input_image` parts and translate user multimodal messages to Chat Completions `image_url` parts while preserving existing plain-text wire output.

## Scope

**In scope**:
- Responses request schema support for URL/data-URL `input_image` parts.
- Translation of mixed user text/image content to OpenAI-compatible Chat Completions content arrays.
- Stable rejection of image content in non-user history roles.
- Focused schema and request-translation tests.

**Out of scope**:
- Uploading local files, resolving `file_id`, downloading image URLs, model capability discovery, or provider-specific image preprocessing.
- Changes to response image generation APIs.

## Design

- Add `InputImage { image_url, detail }` to `ResponsesContentPart`; `detail` is optional and omitted when absent.
- Add an untagged Chat Completions user-content representation: keep text-only messages as a JSON string for backward compatibility, but serialize multimodal messages as an array of `text` and `image_url` parts.
- Preserve the original part order. Map Responses `input_text` to Chat `text`, and `input_image` to Chat `image_url: { url, detail? }`.
- Reject `input_image` for system, developer, and assistant history roles at the exact content path because those roles are not safely representable by this adapter contract.
- Reject empty/whitespace image URLs before an upstream request.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Parse and translate `input_image` | Focused schema/request tests and relevant adapter suite pass | 验证成功 |

### T1: Parse and translate `input_image`

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The gateway currently rejects a standard Responses multimodal content part before provider translation.

**What to do**:
- Update `apps/desktop/src-tauri/src/services/adapters/protocol.rs`.
- Update `apps/desktop/src-tauri/src/services/adapters/request.rs`.
- Add regression coverage in `provider_adapter_schema.rs` and `provider_adapter_request.rs`.

**Logic design**:
- Parse `input_image` with a required string `image_url` and optional `detail`.
- For user messages, select string serialization when every part is text; otherwise build ordered typed Chat content parts.
- Fail locally for empty URLs or image parts attached to unsupported roles.

**Test design**:
- Schema test parses and round-trips `input_image` with a data URL and `detail`.
- Translation test verifies mixed text/image/text ordering and exact Chat Completions JSON shape.
- Translation test verifies existing text-only user content remains a string.
- Translation tests verify empty image URL and non-user image content fail with `UnsupportedInput` and stable paths.
- Expected initial failure: schema parsing returns `ProtocolErrorCode::UnsupportedInput` for `input_image`.

**Acceptance**:
- `cargo test --test provider_adapter_schema --test provider_adapter_request`
- `cargo test --test provider_adapter_deepseek --test provider_gateway_adapter_integration`
- `cargo fmt --check`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design and stays inside scope
- [x] Focused verification command passes
- [x] Relevant suite and formatting pass
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Test plan

- Parse valid `input_image` and preserve `image_url`/`detail`.
- Translate ordered mixed content into Chat Completions multimodal JSON.
- Preserve text-only serialization.
- Reject blank image URLs.
- Reject image content on system/developer/assistant history roles.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused tests | `cargo test --test provider_adapter_schema --test provider_adapter_request` | exit 0 after implementation; regression fails before implementation |
| Relevant integration | `cargo test --test provider_adapter_deepseek --test provider_gateway_adapter_integration` | exit 0 |
| Format | `cargo fmt --check` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

- Existing provider adapters require a conflicting user-content wire shape.
- Supporting the observed request requires file upload or `file_id` resolution.
- Focused or relevant tests still fail after five fix attempts.
