# Todo: Gateway 上游错误来源包装

> Executor instructions: Follow this todo step by step. Generate tests from the
> "Test design" section before implementation. Run each verification command and
> confirm the expected result before moving to the next task. If a STOP condition
> occurs, stop and report instead of improvising.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: MEDIUM
- **Depends on**: existing Responses passthrough route and upstream transport
- **Category**: bugfix/diagnostics
- **Planned at**: current working tree on 2026-07-17

## Why this matters

**Background**: Responses Providers currently pass an upstream non-2xx status and body through unchanged. Codex therefore reports the loopback Gateway URL together with an upstream message, making a remote 503 look like a local Gateway failure.

**Current state**: `passthrough_nonstream` and `passthrough_stream` preserve upstream error bodies. Local Gateway errors use stable `GATEWAY_*` JSON, but upstream HTTP errors have no stable source classification. `Retry-After` is captured by the upstream client but is not returned to the caller.

**Impact**: Users cannot reliably distinguish local admission/authentication failures, transport failures, and remote API overload. Support diagnostics must infer the source from free-form text.

**What improves**: Remote non-2xx responses use a stable, OpenAI-compatible JSON error envelope that identifies the upstream source, sanitized host, provider, status, local request ID, retryability, and a bounded sanitized upstream message. The original HTTP status and valid `Retry-After` value are retained.

## Scope

**In scope**:
- Responses passthrough non-stream and stream initial non-2xx responses.
- A focused upstream HTTP error contract in `gateway/routes.rs`.
- Optional `Retry-After` propagation in `GatewayHttpResponse`.
- Route and server tests for the public behavior.

**Out of scope**:
- Automatic retries or scheduling changes.
- Chat Completions adapter redesign.
- Logging raw upstream response bodies.
- Frontend UI changes.

## Design

- Preserve the upstream HTTP status.
- Return JSON with `error.type=upstream_error`, `error.source=upstream`, a stable status-derived code, provider ID, sanitized hostname, upstream status, local request ID, and retryable flag.
- Extract a useful message from common JSON error shapes or text bodies, collapse control/whitespace, and cap it at 512 characters. Fall back to the HTTP reason when no safe message exists.
- Never expose URL paths, query strings, credentials, headers, or unbounded response bodies.
- Treat 429, 502, 503, and 504 as retryable metadata only; do not replay the request.
- For stream requests receiving an initial non-2xx response, collect the bounded upstream error body before creating the JSON response. Successful SSE behavior remains unchanged.
- Carry `Retry-After` as an optional response property and only emit it when it is a valid HTTP header value.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Define upstream HTTP error contract | Stream and non-stream remote failures are clearly classified and sanitized | 验证成功 |
| T2 | Propagate Retry-After and run regressions | Valid retry guidance reaches the caller without changing success responses | 验证成功 |

### T1: Define upstream HTTP error contract

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Raw passthrough makes the loopback URL look like the source of a remote failure.

**What to do**:
- Add route tests for non-stream and stream upstream 503 responses.
- Add a small route-level builder for stable upstream error envelopes.
- Preserve status while replacing only non-2xx upstream bodies.

**Logic design**:
- Build an explicit diagnostic context before moving the binding into the upstream request.
- Parse common JSON/text messages, sanitize and bound them, then construct one stable error envelope.
- Keep successful passthrough byte-for-byte compatible.

**Test design**:
- A text 503 becomes `UPSTREAM_SERVICE_UNAVAILABLE`, identifies `source=upstream`, includes host/provider/request ID and retryability, and does not expose URL paths or control characters.
- A streaming request that receives initial 503 gets the same JSON contract rather than an SSE/raw-text response.
- An oversized/empty upstream error is bounded or falls back safely.
- Existing successful non-stream and SSE passthrough tests remain unchanged.
- Expected initial failure: current code returns the raw upstream body and content type.

**Acceptance**:
- `cargo test --test provider_gateway_routes -- --test-threads=1`

**Done criteria**:
- [x] Tests listed above were written before implementation
- [x] New tests failed for the expected raw-passthrough reason
- [x] Implementation matches the error contract and stays in scope
- [x] Focused route tests pass
- [x] Formatting passes
- [x] Task overview and status are `验证成功`

### T2: Propagate Retry-After and run regressions

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Remote overload/rate-limit responses may tell the caller when retry is appropriate, but Gateway currently drops that header.

**What to do**:
- Add optional Retry-After metadata to `GatewayHttpResponse`.
- Emit only syntactically valid header values.
- Cover response conversion and run Gateway server/upstream regressions.

**Logic design**:
- Keep header ownership in the transport response type rather than coupling routes to Axum internals.
- Invalid upstream header values remain absent and never fail the response.

**Test design**:
- A wrapped upstream error retains `Retry-After` metadata.
- Converting `GatewayHttpResponse` to an HTTP response emits a valid value and ignores an invalid value.
- Existing Gateway response headers and success paths remain unchanged.
- Expected initial failure: `GatewayHttpResponse` has no retry metadata/header support.

**Acceptance**:
- `cargo test --test provider_gateway_routes --test provider_gateway_server --test provider_gateway_upstream -- --test-threads=1`
- `cargo fmt -- --check`
- `git diff --check`

**Done criteria**:
- [x] Tests listed above were written before implementation
- [x] New tests failed for the expected missing-header reason
- [x] Implementation matches the response-boundary design and stays in scope
- [x] Focused and regression tests pass
- [x] Formatting and diff checks pass
- [x] Task overview and status are `验证成功`

## Test plan

- Non-stream plain-text 503 overload.
- Stream request receiving an initial plain-text 503.
- Common JSON upstream error message.
- Empty, control-character, and oversized upstream messages.
- Stable status-derived code and retryability.
- Valid and invalid `Retry-After` handling.
- Existing 2xx JSON/SSE passthrough behavior.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Failure-first route tests | `cargo test --test provider_gateway_routes -- --test-threads=1` | new tests fail before implementation, exit 0 afterward |
| Gateway regression | `cargo test --test provider_gateway_routes --test provider_gateway_server --test provider_gateway_upstream -- --test-threads=1` | exit 0 |
| Format | `cargo fmt -- --check` | exit 0 |
| Diff hygiene | `git diff --check` | exit 0 |

## Done criteria

- [x] Every task's Done criteria is fully checked
- [x] Every task has exactly one checked status and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

- Codex requires byte-for-byte passthrough of non-2xx Responses bodies.
- The error envelope would need to expose credentials, full upstream URLs, or unbounded bodies.
- Retry support would require automatic request replay.
- Relevant Gateway regressions still fail after five fix loops.
