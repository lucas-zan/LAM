# Todo: RPG-002 Pin and capture the Codex Gateway contract

> Executor instructions: Capture only against a temporary `CODEX_HOME`, a
> loopback server, fixed synthetic prompts, and a synthetic auth helper. Add the
> offline contract verifier before the capture harness or fixtures. Never read,
> copy, or normalize a real Codex session into this corpus.

## Status

- **Status**: 验证成功
- **Priority**: P0
- **Effort**: XL
- **Risk**: HIGH
- **Depends on**: RPG-001
- **Category**: test/contract/gateway
- **Parent tracker**: [`todo-remote-provider-gateway.md#rpg-002-pin-and-capture-the-codex-gateway-contract`](./todo-remote-provider-gateway.md#rpg-002-pin-and-capture-the-codex-gateway-contract)
- **Target CLI**: `codex-cli 0.144.1`
- **Target platform**: macOS 15.6, Darwin arm64
- **Support policy**: exact tested version; no minimum/range claim until another
  version passes the same capture and offline verifier
- **Capture date**: 2026-07-10

## Why this matters

The Gateway must implement the protocol Codex actually emits, not a generic
interpretation of the Responses API. Path joining, headers, stream lifecycle,
tool history, resume state, auth refresh, retry budgets, and cancellation all
affect routing, security, and whether the adapter can remain stateless.

Official Codex documentation establishes the supported custom-provider config
surface. The committed wire contract is determined by repeatable traffic from
the pinned local CLI against a controlled loopback provider.

## Source evidence

- Local executable: `/Users/zhanhd/.bun/bin/codex` reported
  `codex-cli 0.144.1` on 2026-07-10.
- Official Codex advanced configuration documents custom Provider `base_url`,
  `wire_api = "responses"`, env/header auth, and command-backed auth:
  <https://learn.chatgpt.com/docs/config-file/config-advanced#custom-model-providers>.
- Official Responses streaming guidance defines semantic SSE lifecycle events:
  <https://developers.openai.com/api/docs/guides/streaming-responses>.
- The `openai-docs` manual helper was attempted first but could not verify the
  manual because the response omitted `x-content-sha256`; the targeted official
  Docs MCP pages above were fetched instead.

## Safety boundary

- Use a newly created temporary `CODEX_HOME`; never point at the user's normal
  Codex home or auth/session files.
- Use `127.0.0.1` with an ephemeral port and a fixed Provider ID/model.
- Auth helper emits only `FIXTURE_AUTH_TOKEN`, optionally padded with whitespace.
- Prompts and tool output come from a fixed allowlist in the harness.
- Normalize temp paths, ports, timestamps, request/response/thread/item/call IDs,
  user agent versions where declared volatile, and timing measurements.
- Preserve unknown request fields and record their JSON paths; do not silently
  discard them during normalization.
- The harness writes only to a caller-selected staging directory. Promotion to
  committed fixtures is explicit and must pass the offline leak/integrity test.

## Capture architecture

### Reproducible harness

`apps/desktop/scripts/capture-codex-contract.mjs` owns orchestration:

1. Verify the exact Codex version and Darwin/arm64 target.
2. Create a temporary home, workspace, synthetic auth helper, and config.
3. Start an HTTP server on `127.0.0.1:0` and record every method/path/header/body.
4. Spawn `codex exec --json --ephemeral` for stateless/error cases and a
   persistent temporary session for resume.
5. Return scripted JSON/SSE, status, delay, malformed stream, or disconnect
   behavior per scenario.
6. Run deterministic scenarios twice, normalize only declared volatile fields,
   and fail if normalized results differ.
7. Emit a staging manifest plus scenario files; never read real user state.

### Offline verifier

`provider_codex_contract.rs` validates committed fixtures without invoking
Codex or the network:

- exact target/support metadata and required scenario inventory;
- checksums and sanitized content;
- fixed prompt/tool allowlist;
- captured request path, headers, body, stream flag, model, input and tool
  contracts;
- SSE event ordering and terminal event;
- tool output and resume state observations;
- route inventory, retry attempt counts, Retry-After, auth invocation/trim/empty
  behavior, malformed stream, and disconnect evidence;
- every unsupported observation has explicit evidence.

## Required scenarios

| Scenario | Required observation | Completion rule |
| --- | --- | --- |
| `text-stream` | path, headers, request body/defaults/unknowns, SSE lifecycle | two normalized runs match |
| `text-non-stream` | whether Codex ever sends `stream=false` and accepted body | captured or unsupported with CLI evidence |
| `function-tool` | offered tool schema, function call events, tool output next request | complete round trip or explicit unsupported evidence |
| `resume` | resumed request sends full input, `previous_response_id`, or another state route | two-turn persistent temp session |
| `route-inventory` | models/retrieve/cancel/other calls | all observed requests across cases |
| `auth-helper-trim` | invocation count, no stdin, trimmed bearer header | captured |
| `auth-helper-empty` | failure, HTTP request count, stable error class | captured |
| `auth-refresh-401` | helper refresh and request replay behavior | captured |
| `retry-429` | attempt count and Retry-After handling | captured |
| `retry-500` | request and stream retry budgets | captured |
| `malformed-sse` | error and retry behavior after malformed/partial data | captured |
| `disconnect` | server observes client close after Codex cancellation | captured |

## Tasks

| ID | Task | Acceptance summary | Status |
| --- | --- | --- | --- |
| T1 | Add offline contract verifier first | Fails because the manifest/corpus is absent | 验证成功 |
| T2 | Build isolated capture harness | Harness proves temp-home, loopback, auth, normalization, and no real-state access | 验证成功 |
| T3 | Capture normal text/tool/resume and route set | Deterministic normal-path fixtures establish request/state contract | 验证成功 |
| T4 | Capture auth/error/retry/disconnect behavior | Failure-path fixtures establish bounded budgets and cancellation | 验证成功 |
| T5 | Lock design decisions and run full gates | Offline/full suites pass and design conditional prose is resolved | 验证成功 |

### T1: Add offline contract verifier first

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Done criteria**:

- [x] Offline Rust test exists before harness/fixture implementation
- [x] Test owns the complete scenario and metadata inventory
- [x] Focused test fails for the expected missing manifest
- [x] Red evidence is recorded

**Red evidence on 2026-07-10**: all four offline contract tests failed only
because `codex-gateway-contract/manifest.json` and its artifacts were absent;
exit 101.

### T2: Build isolated capture harness

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Implementation constraints**:

- Harness functions stay focused: config generation, server scripting, process
  spawn, normalization, comparison, and staging output are separate units.
- No shell command strings for auth execution; use an absolute helper executable
  plus explicit args.
- Child processes have hard timeouts and are killed/reaped on every path.
- Authorization and helper stdout are compared to the synthetic marker in
  memory but emitted only as `<redacted>`.
- Staging output is deterministic JSON plus raw `.sse` files with LF endings.

**Test design**:

- Harness self-test mode uses a fake child to test timeout, normalization,
  secret redaction, stable ordering, and staging writes without Codex.
- A dry run against the pinned CLI proves config loading and loopback routing.

**Red evidence on 2026-07-10**: the five harness self-tests were added before
the harness and failed module resolution for the absent
`capture-codex-contract.mjs`; exit 1.

**Validation evidence on 2026-07-10**:

- Harness self-tests: 5/5 passed, including timeout/reap, redaction,
  normalization, safety rejection, and deterministic artifact writing.
- A fresh temporary `HOME`/`CODEX_HOME`, empty workspace, synthetic helper, and
  loopback Provider completed the fixed text scenario with Codex exit 0.
- First live observation: Codex called
  `GET /v1/models?client_version=0.144.1` twice, then
  `POST /v1/responses`; the helper was invoked twice with zero stdin bytes.
- The initial model-list stub used an OpenAI-style `data` array; Codex evidence
  showed its provider metadata route expects a top-level `models` field, so the
  harness now returns `{ "models": [] }`.

### T3: Capture normal text/tool/resume and route set

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Done criteria**:

- [x] Text request/stream fixtures captured twice and deterministic
- [x] Non-stream behavior explicitly unsupported by observed CLI paths
- [x] Tool round trip captures call arguments and next-turn output item
- [x] Resume behavior determines the `full-input` state model
- [x] Observed/unsupported route set is final for the pinned CLI

**Validation evidence on 2026-07-10**:

- Every observed `codex exec` request used `stream=true`; no non-stream path was
  observed.
- Tool follow-up replayed full input with `function_call` and
  `function_call_output`; `previous_response_id` was `null`.
- Resume replayed the first user item, first assistant result, and new user item;
  `previous_response_id` was `null`.
- Required routes are `GET /v1/models` and `POST /v1/responses`; retrieve and
  cancel routes were not observed.
- Two normalized normal-path runs matched exactly.

### T4: Capture auth/error/retry/disconnect behavior

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Done criteria**:

- [x] Auth trim/empty/401 observations are complete and redacted
- [x] 429/500 request and stream attempts are counted
- [x] Retry-After behavior is measured without an unbounded wait
- [x] Malformed SSE and disconnect/cancel behavior are recorded
- [x] Combined Codex/Gateway retry budget can be set numerically

**Validation evidence on 2026-07-10**:

- Trimmed helper token: 2 helper calls, zero stdin, valid bearer on 3 HTTP calls.
- Empty helper token: Codex reported the error but made 3 unauthenticated calls
  and completed because the permissive mock accepted them.
- First 401: 3 helper calls, 2 Responses attempts, then success.
- 429 with `Retry-After: 0`: 1 attempt, terminal error.
- 500: 30 attempts, proving default request × stream retry multiplication.
- Malformed SSE: 6 attempts, terminal error.
- Cancellation: process-group kill closed the client connection; an added
  regression test proves wrapper and descendant processes are both reaped.
- Two normalized failure-path runs matched exactly.

### T5: Lock design decisions and run full gates

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Acceptance**:

- Offline contract test passes without Codex/network.
- Harness self-tests pass.
- Full Rust/frontend suites and format/lint/build pass.
- Full design names the final MVP routes, state requirement, retry ownership,
  and exact-tested support policy.
- No fixture contains a real prompt, credential, home path, port, timestamp, or
  personal identifier.

**Done criteria**:

- [x] Offline contract verifier passes without Codex/network
- [x] Harness self-tests, including descendant process reaping, pass
- [x] Full Rust/frontend suites and all format/lint/build gates pass
- [x] Full design contains the proven route/state/auth/retry/version rules
- [x] Fixture checksum and sanitization checks pass
- [x] T5, RPG-002, and the master overview are `验证成功`

**Validation evidence on 2026-07-10**:

- Offline Codex contract verifier: 4/4 passed.
- Capture harness self-tests: 9/9 passed.
- Full Rust suite: 160 passed, 2 ignored, 0 failed.
- Full frontend suite: 15 files, 132 tests passed.
- Rust format, frontend lint/format, harness Prettier, frontend build, and
  `git diff --check`: passed.
- Generated `.fake-home` timestamp was restored; no capture process or generated
  cache diff is part of the issue.

## Verification commands

| Purpose | Command | Expected |
| --- | --- | --- |
| Offline red/focused | `cd apps/desktop/src-tauri && cargo test --test provider_codex_contract` | missing corpus before capture; exit 0 after promotion |
| Harness self-test | `cd apps/desktop && node --test scripts/capture-codex-contract.test.mjs` | exit 0 |
| Capture staging | `cd apps/desktop && node scripts/capture-codex-contract.mjs --output <staging>` | exact pinned CLI only |
| Rust full | `cd apps/desktop/src-tauri && cargo test` | exit 0 |
| Rust format | `cd apps/desktop/src-tauri && cargo fmt --check` | exit 0 |
| Frontend full | `cd apps/desktop && npm test` | exit 0 |
| Frontend quality | `cd apps/desktop && npm run lint && npm run format:check && npm run build` | exit 0 |
| Diff | `git diff --check` | exit 0 |

## STOP conditions

- Codex cannot route to the loopback Provider from a fresh temporary home.
- A scenario requires reading/modifying private user product state.
- Captured undocumented state cannot be normalized without losing semantics.
- The observed CLI version/platform differs from the pinned target.
