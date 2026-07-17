# Todo: Complete Codex Compact Compatibility

> Executor instructions: Follow this todo in order. Write and run each task's
> tests before implementation, record the expected failure, and deliver only
> after every task reaches `验证成功`.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: HIGH
- **Depends on**: `docs/todo-codex-compact-compatibility.md`
- **Category**: compatibility/bugfix
- **Planned at**: current dirty workspace, 2026-07-16

## Why this matters

**Background**: The first compact compatibility pass reads model context from
the normal `~/.codex/models_cache.json`. That enables native automatic compact
when the cache exists, but a clean machine has no model fallback. LAM-managed
Provider names can also accidentally equal Codex's reserved `OpenAI` identity,
which makes Codex call `/responses/compact` even when the upstream API does not
implement it.

**Current state**: A built-in sanitized `config.toml` template exists. Model
defaults are optional and supplied only by a launcher environment path. API
Accounts route through the Gateway and local compact requests already work over
ordinary `/v1/responses`, but the Provider identity does not explicitly enforce
Codex's local-compaction branch.

**Impact**: Automatic compact is not guaranteed on a clean installation, and a
Provider named `OpenAI` can select an unavailable remote compact endpoint.

**What improves**: LAM ships a versioned sanitized model catalog, overlays newer
local Codex metadata when available, and guarantees Gateway Providers use Codex
local manual/automatic compaction through ordinary Responses streaming.

## Scope

**In scope**:

- A repository-owned sanitized Codex model-catalog template.
- Built-in catalog fallback plus validated local-cache overlay.
- Gateway Provider identity that cannot trigger Codex remote compaction.
- Tests for built-in context metadata, automatic threshold inputs, manual local
  compact, and absence of `/responses/compact` upstream traffic.
- Existing sanitized Codex config template verification.

**Out of scope**:

- Copying `auth.json`, sessions, SQLite, logs, hooks, MCP credentials, browser
  state, installation IDs, or other CODEX_HOME runtime state.
- Emulating the unary `/responses/compact` response format by rewriting its URL.
- Claiming arbitrary unknown third-party models have a guessed context window.
- Changing Codex itself or pretending LAM is the built-in OpenAI Provider.

## Design

- Store a minimal versioned model catalog containing only slug, context window,
  max context window, auto-compact limit, and effective-window percentage.
- Gateway always starts with the built-in catalog. A valid bounded local normal
  Codex cache overlays entries by slug; missing, malformed, or oversized local
  cache leaves built-in values intact.
- Keep `auto_compact_token_limit` absent when the official catalog leaves it
  null, so Codex derives its native model default.
- Prefix the Codex-facing name of every Gateway route with `LAM Gateway`. Codex
  then treats it as a custom Provider and selects local compact for both manual
  and automatic triggers. The Gateway base URL is loopback, so Azure endpoint
  detection cannot select remote compact either.
- Local compact remains a normal streamed `/v1/responses` request. LAM must not
  translate a unary compact payload directly because the response contracts are
  different.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|---|---|---|---|
| T1 | Built-in Codex model catalog | Clean installs expose known context metadata; local cache safely overrides it | 验证成功 |
| T2 | Force local Responses compact | Gateway Providers cannot be mistaken for remote-compact OpenAI/Azure Providers | 验证成功 |
| T3 | Manual and automatic compact contract | Manual and threshold-trigger inputs use `/v1/responses`, never `/responses/compact` | 验证成功 |

### T1: Built-in Codex model catalog

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Automatic compact needs accurate model context metadata even when a
normal Codex cache is absent.

**What to do**:

- Add the sanitized catalog asset under `resources/`.
- Add built-in loading and slug-based local overlay.
- Initialize the sidecar from built-in data before optional local data.

**Logic design**:

- Built-in JSON parse failure is a build/runtime invariant error.
- Local errors are non-fatal fallback conditions.
- Only positive bounded fields accepted by the existing parser are retained.
- Local matching entries replace built-in matching entries; other built-ins
  remain available.

**Test design**:

- No local cache still gives `gpt-5.6-sol` its bundled context metadata.
- A local matching entry overrides bundled context.
- Malformed local JSON leaves the bundled entry available.
- Unknown slugs remain without invented metadata.
- Expected initial failure: the empty/default catalog has no bundled models.

**Acceptance**:

- `cargo test --test provider_codex_catalog -- --nocapture`
- `cargo test --test provider_sidecar_production -- --nocapture`

**Observed red test**:

- The catalog test failed to compile because `builtin` and `overlay_json` did
  not exist; the production source assertion also lacked built-in startup.

**Done criteria**:

- [x] Tests written before implementation.
- [x] Expected failure observed and recorded.
- [x] Built-in and overlay precedence implemented.
- [x] Focused tests pass.
- [x] Status updated to `验证成功`.

### T2: Force local Responses compact

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Current Codex selects remote compact solely for OpenAI/Azure Provider
identity. User-provided display names must not accidentally opt into it.

**What to do**:

- Give Gateway config projections an unambiguous LAM Provider name.
- Preserve the user's Provider name as a suffix for diagnostics.
- Keep direct-route naming unchanged.

**Logic design**:

- Gateway name is `LAM Gateway · <provider name>`.
- It is never exactly `OpenAI` or `Azure`.
- Gateway base URL remains loopback and therefore cannot match Azure host
  markers.

**Test design**:

- A Gateway Provider whose user name is `OpenAI` receives a prefixed Codex name.
- A direct custom Provider retains its original name.
- Expected initial failure: Gateway projection currently uses the raw name.

**Acceptance**:

- `cargo test --test provider_planner -- --nocapture`
- `cargo test --test provider_config_editor -- --nocapture`

**Observed red test**:

- A Gateway Provider named `OpenAI` initially projected the exact reserved name
  `OpenAI` instead of `LAM Gateway · OpenAI`.

**Done criteria**:

- [x] Tests written before implementation.
- [x] Expected failure observed and recorded.
- [x] Gateway identity is unambiguous.
- [x] Focused tests pass.
- [x] Status updated to `验证成功`.

### T3: Manual and automatic compact contract

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: The fallback must be proven at the production Gateway route rather
than inferred from individual helper functions.

**What to do**:

- Add distinct manual and automatic compact-shaped fixtures.
- Assert both reach upstream `/responses` with a streamed summary request.
- Assert `/responses/compact` is never called.
- Verify the embedded config and catalog assets contain no account or machine
  state.

**Logic design**:

- Manual and automatic triggers differ in lifecycle origin, not upstream
  protocol for a custom Provider.
- Both require a native `response.completed` terminal event.
- No unary compact response is fabricated.

**Test design**:

- Manual compact prompt with long history completes over `/api/v1/responses`.
- Automatic compact-shaped request completes over the same route.
- A mock `/api/v1/responses/compact` endpoint has expected call count zero.
- Template assets reject auth, paths, headers, history, and runtime-state keys.
- Expected initial failure: automatic fixture and explicit no-compact-endpoint
  assertion do not exist.

**Acceptance**:

- `cargo test --test provider_gateway_routes -- --nocapture`
- `cargo test --test provider_phase4_api_account -- --nocapture`
- `cargo test --tests -- --test-threads=1`
- `git diff --check`

**Verification note**:

- The first full-suite run reached two pre-existing `phase1_core` failures: an
  environment-dependent quota fallback warning assertion, followed by a shared
  lock poison caused by that panic. Compact, Gateway, catalog, and template
  suites passed. Both failures passed in isolated serial reruns, and the second
  complete serial suite then passed without failures.

**Initial test-state exception**:

- T3 added contract tests before any T3-specific production change. They passed
  because T1 supplied model metadata, T2 forced Codex's local-compaction branch,
  and the existing production Responses route already carried summary streams.
  The missing behavior was therefore contract coverage, not another transport
  implementation; manufacturing a failure would have required reverting a
  verified dependency.

**Done criteria**:

- [x] Tests written before implementation.
- [x] Initial missing-contract state recorded.
- [x] Both compact paths use normal Responses.
- [x] Templates are sanitized and versioned.
- [x] Full relevant suite and patch hygiene pass.
- [x] Status updated to `验证成功`.

## Test plan

- Bundled, local override, malformed local, and unknown model catalog cases.
- Gateway OpenAI/Azure-looking names and direct-route control case.
- Manual and auto compact-shaped long-history streams.
- Explicit zero calls to `/responses/compact`.
- Embedded template safety scan.

## Verification commands

| Purpose | Command | Expected on success |
|---|---|---|
| Catalog | `cd apps/desktop/src-tauri && cargo test --test provider_codex_catalog` | exit 0 after initial failure |
| Provider identity | `cd apps/desktop/src-tauri && cargo test --test provider_planner` | exit 0 after initial failure |
| Compact route | `cd apps/desktop/src-tauri && cargo test --test provider_gateway_routes` | exit 0 |
| Full Rust | `cd apps/desktop/src-tauri && cargo test --tests -- --test-threads=1` | exit 0 |
| Hygiene | `git diff --check` | exit 0 |

## Done criteria

- [x] Every task is `验证成功`.
- [x] Built-in config and model catalog are present and sanitized.
- [x] Clean-install automatic compact metadata is available.
- [x] Manual and automatic compact use ordinary Responses for Gateway APIs.
- [x] No STOP condition remains.

## STOP conditions

- Codex current source no longer selects local compact for custom Providers.
- A required fallback would need copying authentication or runtime state.
- The upstream API cannot produce a normal Responses summary stream.
- Existing dirty changes materially conflict with these files.
- Relevant tests fail after five bounded fix attempts.
