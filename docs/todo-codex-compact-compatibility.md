# Todo: Codex Compact Compatibility

> Executor instructions: Follow this todo step by step. Generate tests from
> each task's Test design before implementation, confirm the expected failure,
> and do not mark delivery complete until every task is verified successfully.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: HIGH
- **Depends on**: Gateway first-response timeout setting
- **Category**: bugfix/compatibility
- **Planned at**: current dirty workspace, 2026-07-16

## Why this matters

**Background**: LAM-routed Codex sessions can grow beyond the effective model
context and fail mid-task instead of compacting like a normal Codex session.
Manual `/compact` uses Codex local compaction for LAM's custom provider, so it
still depends on the ordinary Responses stream completing correctly.

**Current state**: LAM's Codex model catalog omits `context_window`,
`max_context_window`, and `auto_compact_token_limit`. For the currently selected
`gpt-5.6-sol`, the normal Codex cache advertises a 272000-token context window,
while the LAM profile inferred 353400. LAM also closes the downstream stream
silently on upstream read errors and records HTTP 200 before the stream body
finishes.

**Impact**: Codex can trigger compaction too late, manual compaction can end with
`stream closed before response.completed`, while Codex receives no actionable
Gateway error category for timeout, transport failure, or incomplete EOF.

**What improves**: LAM exposes Codex-compatible model context metadata, leaves
the auto-compact threshold unset so Codex derives its native 90% default, and
turns incomplete/error streams into explicit terminal SSE errors.

## Scope

**In scope**:

- A sanitized, repository-owned Codex configuration template for newly created
  API Account CODEX_HOME directories, with safe local preference inheritance.
- Codex model catalog context metadata and native auto-compact derivation.
- Backward-compatible Provider model metadata needed for third-party overrides.
- Manual/local compact compatibility over the normal `/v1/responses` route.
- Responses SSE completion/failure/incomplete-stream validation and diagnostics.
- Focused contract tests plus relevant Rust regression suites.

**Out of scope**:

- Pretending LAM is the built-in OpenAI/Azure provider.
- Remote `/v1/responses/compact` and ChatGPT backend OAuth endpoints.
- WebSocket Responses transport.
- Killing or mutating the user's currently running Codex/Gateway processes.
- Copying unrelated Cockpit Tools account-pool or multi-protocol features.

## Design

- New API Accounts start from a versioned built-in template, so creation works
  even when `~/.codex/config.toml` does not exist. When it does exist, only an
  explicit allowlist of account-independent Codex preferences is merged into the
  template. Authentication, model routing, Provider tables, project trust,
  paths, commands, MCP headers, plugins, hooks, sessions, caches, and databases
  are never copied. LAM then applies the selected model/Provider projection.
- Treat the installed normal Codex model catalog as the preferred compatibility
  reference for matching model slugs. Provider-declared model metadata remains an
  explicit override for external models whose capabilities differ.
- Catalog output includes the model's real `context_window` and
  `max_context_window` when known. It omits `auto_compact_token_limit` by default;
  current Codex then derives `context_window * 90%`, preserving the native default
  rather than freezing a LAM-owned threshold.
- Unknown model slugs remain supported but must not receive an invented context
  size. Users/providers may declare an explicit positive context window.
- Manual `/compact` continues to use Codex local compaction through ordinary
  `/v1/responses`; LAM must not require the remote compact endpoint.
- Responses streams track terminal protocol events. `response.completed` is
  success; `response.failed`, `response.incomplete`, upstream read errors, idle
  timeout, and EOF before a terminal event are observable terminal failures.
- Once HTTP 200 headers have been committed, Gateway emits a terminal SSE error
  frame and records a stable error category rather than silently closing.
- Do not synthesize `response.completed` for a native Responses upstream.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|---|---|---|---|
| T0 | Safe Codex config template | API Accounts inherit safe Codex behavior without copying identity or machine state | 验证成功 |
| T1 | Native Codex model context contract | Matching models expose the normal Codex window and leave compact threshold to Codex | 验证成功 |
| T2 | Compact-safe Responses stream lifecycle | Complete streams succeed; failed/incomplete/error streams terminate explicitly | 验证成功 |
| T3 | End-to-end compact verification | Manual/local compact fixture and relevant suites pass | 验证成功 |

### T0: Safe Codex config template

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:

Creating every API Account from an empty `config.toml` discards normal Codex
behavior and makes compatibility depend on Gateway-owned guesses. Copying an
entire CODEX_HOME would instead leak identity, credentials, history, and
machine-specific state.

**What to do**:

- Add a repository-owned TOML template derived from the safe, portable behavior
  settings in the user's normal Codex config.
- During API Account creation, optionally merge allowlisted settings from
  `~/.codex/config.toml` over that built-in template.
- Apply the API Account model and Provider projection after the template is
  written.

**Logic design**:

- The built-in template is always available and valid TOML.
- Local source absence or invalid TOML falls back to the built-in template and
  does not block account creation.
- Only allowlisted scalar behavior preferences and allowlisted `[features]`
  entries may be inherited.
- `model`, `model_provider`, `model_providers`, auth, projects, MCP, paths,
  commands, notification hooks, plugins, and other sections are excluded.
- The destination file is private and must not pre-exist.

**Test design**:

- A local config with safe preferences plus fake Provider, project, MCP header,
  and notify secret preserves only the safe preferences.
- No local config produces the checked-in template defaults.
- Malformed local TOML falls back to the checked-in template.
- The final API Account config still contains the selected LAM model and
  `model_provider` projection.
- Expected initial failure: current creation writes an empty file and therefore
  retains none of the template or local preferences.

**Observed red test**:

- `cargo test --test provider_phase4_api_account -- --nocapture` failed all
  three new template tests with `index not found`, confirming the old empty-file
  path exposes neither inherited preferences nor built-in fallback defaults.

**Acceptance**:

- `cargo test --test provider_phase4_api_account -- --nocapture`
- `git diff --check`

**Done criteria**:

- [x] Tests were written before implementation.
- [x] Expected failure was observed and recorded.
- [x] Built-in fallback and allowlisted inheritance behave as designed.
- [x] No identity, secret, or machine-specific state is copied.
- [x] Focused tests pass.
- [x] Status is updated to `验证成功`.

### T1: Native Codex model context contract

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:

Codex can only apply its native auto-compaction threshold when it knows the
effective context window. LAM currently replaces richer normal Codex metadata
with a generic model record.

**What to do**:

- Extend the Provider model contract with optional context metadata without
  breaking existing stored providers/bindings.
- Add a controlled compatibility lookup for matching normal Codex model slugs.
- Emit `context_window`/`max_context_window` in the Gateway Codex catalog.
- Keep `auto_compact_token_limit` absent unless an explicit provider override is
  introduced later, allowing Codex to derive its default.

**Logic design**:

- Explicit positive Provider metadata wins.
- A matching normal Codex catalog entry is the fallback.
- Unknown/invalid metadata is omitted, never guessed.
- The canonical catalog path is passed through a controlled launcher environment
  variable after `env_clear()`; Gateway validates and parses only the expected
  JSON fields.

**Test design**:

- Add a catalog test proving a known context window serializes correctly and
  `auto_compact_token_limit` stays absent.
- Add a compatibility-catalog parsing test for matching slug, unknown slug,
  malformed JSON, and non-positive values.
- Add launcher production assertions for controlled catalog-path propagation.
- Expected initial failure: context fields and compatibility lookup do not exist.

**Acceptance**:

- `cargo test --test provider_codex_catalog -- --nocapture`
- `cargo test --test provider_sidecar_production -- --nocapture`

**Observed red tests**:

- Catalog tests initially failed to compile because the compatibility catalog
  and context-aware builder did not exist.
- The production wiring assertion then failed because the sanitized model-cache
  path was not propagated through the launcher's cleared environment.

**Done criteria**:

- [x] Tests were written before implementation.
- [x] Expected failure was observed and recorded.
- [x] Implementation follows the precedence and omission rules.
- [x] Focused tests pass.
- [x] Relevant production wiring test passes.
- [x] Status is updated to `验证成功`.

### T2: Compact-safe Responses stream lifecycle

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:

Local/manual compact is an ordinary streamed Responses request and is unusable
when LAM silently turns upstream errors or incomplete EOF into a clean body end.

**What to do**:

- Add a bounded Responses SSE terminal-state observer to passthrough streams.
- Preserve upstream bytes while recognizing terminal event names/types.
- Emit a stable terminal SSE error for upstream read error, idle timeout, or EOF
  without a terminal event.
- Preserve a stable terminal reason in the downstream SSE error without
  buffering the full stream.

**Logic design**:

- Parsing must tolerate SSE frames split across arbitrary chunks and enforce a
  bounded pending-frame buffer.
- Native `response.completed` is passed through unchanged.
- `response.failed`/`response.incomplete` are passed through and treated as
  terminal upstream failures, not rewritten as success.
- EOF before any terminal event produces `GATEWAY_UPSTREAM_STREAM_INCOMPLETE`.
- Transport/idle errors preserve their upstream error code in the emitted error
  frame and diagnostics.

**Test design**:

- Complete event split across chunks succeeds without an extra error.
- Clean EOF before completion emits an incomplete-stream SSE error.
- Upstream idle/transport failure emits its stable error code.
- `response.failed` is recognized as terminal and not followed by a fabricated
  completion.
- Expected initial failure: current passthrough silently returns on EOF/error.

**Acceptance**:

- `cargo test --test provider_gateway_routes -- --nocapture`
- `cargo test --test provider_gateway_upstream -- --nocapture`

**Observed red test**:

- The new incomplete-stream route test initially received the partial upstream
  bytes with no `event: error`, reproducing the silent EOF behavior.

**Done criteria**:

- [x] Tests were written before implementation.
- [x] Expected failure was observed and recorded.
- [x] Parser is bounded and chunk-boundary safe.
- [x] No native completion event is fabricated.
- [x] Focused tests pass.
- [x] Status is updated to `验证成功`.

### T3: End-to-end compact verification

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:

Unit-level SSE and catalog tests are insufficient unless a Codex-shaped local
compact request can traverse the Gateway and finish with the contract Codex
expects.

**What to do**:

- Add a Codex-shaped compact request fixture over `/v1/responses`.
- Verify long-history input remains accepted and the response reaches
  `response.completed`.
- Run relevant Gateway, provider, and build checks.

**Logic design**:

- The fixture represents local compaction, not the remote compact endpoint.
- It must exercise the same passthrough stream path used by normal Codex turns.
- Verification must not call a real external model service.

**Test design**:

- Mock upstream receives a streamed Responses request containing the compaction
  summary instruction and history.
- Gateway returns a valid completed SSE sequence.
- A missing terminal event fails with the new explicit code.
- Expected initial failure: explicit completion enforcement is absent.

**Acceptance**:

- `cargo test --test provider_gateway_routes --test provider_codex_contract -- --nocapture`
- `cargo test --tests -- --test-threads=1`
- `git diff --check`

**Observed red test**:

- Before terminal enforcement, the compact route's missing-terminal case failed
  to produce the explicit error required by this end-to-end contract. The long
  history completion fixture was then added against the production route.

**Done criteria**:

- [x] Tests were written before implementation.
- [x] Expected failure was observed and recorded.
- [x] Compact fixture uses the production Gateway route.
- [x] Focused and relevant full suites pass.
- [x] `git diff --check` passes.
- [x] Status is updated to `验证成功`.

## Test plan

- Known and unknown model catalog context behavior.
- Backward-compatible Provider model deserialization.
- Native 90% auto-compact derivation by omitting a LAM override.
- SSE framing across chunk boundaries.
- Completed, failed, incomplete, idle-timeout, transport-error, and client-cancel
  terminal paths.
- Codex-shaped local/manual compact request through `/v1/responses`.

## Verification commands

| Purpose | Command | Expected on success |
|---|---|---|
| Catalog contract | `cd apps/desktop/src-tauri && cargo test --test provider_codex_catalog` | exit 0 after implementation; fail first |
| Gateway streams | `cd apps/desktop/src-tauri && cargo test --test provider_gateway_routes --test provider_gateway_upstream` | exit 0 after implementation; fail first |
| Wiring | `cd apps/desktop/src-tauri && cargo test --test provider_sidecar_production` | exit 0 |
| Relevant full Rust suite | `cd apps/desktop/src-tauri && cargo test --tests -- --test-threads=1` | exit 0 |
| Patch hygiene | `git diff --check` | exit 0 |

## Done criteria

- [x] Every task's Done criteria checklist is fully checked.
- [x] Every task has exactly one checked status and it is `验证成功`.
- [x] Task overview shows every task as `验证成功`.
- [x] Manual `/compact` and native automatic compact behavior are both covered.
- [x] No STOP condition remains unresolved.

## STOP conditions

- The installed Codex version does not use local compaction for LAM's custom
  provider.
- Matching normal Codex model metadata cannot be obtained through a controlled,
  non-secret path and no safe explicit Provider fallback exists.
- SSE lifecycle observation requires unbounded buffering.
- Existing dirty-worktree changes conflict materially with required files.
- Relevant full tests expose unrelated failures requiring scope expansion.
