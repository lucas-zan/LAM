# Todo: 严格对接 Codex Provider 模型目录与运行时

> Executor instructions: Follow this todo step by step. Generate tests from
> each task's "Test design" before implementation. Confirm the expected red
> state, implement only the active task, run its acceptance commands, and update
> the task status immediately. Stop instead of weakening a Codex contract.

## Status

- **Priority**: P0
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: RPG-002, RPG-201..RPG-208, RPG-301..RPG-308, Phase 4 API Account
- **Category**: bugfix/feature/protocol/security
- **Planned at**: current dirty workspace; no branch/commit requested
- **Exact Codex target**: `codex-cli 0.144.1`, Darwin arm64

## Why this matters

**Background**: External API Account currently accepts a manually maintained
model list and writes the selected model into Codex configuration. The user
requires strict Codex compatibility and expects an upstream that implements the
standard OpenAI `GET /v1/models` endpoint to supply the usable model list.

**Current state**:

- `ApiAccountFlow` renders a hard-coded list (`5.6 Sol`, `5.6 Terra`, and so on)
  and never calls the upstream models endpoint.
- the existing Provider health probe only verifies that either `data` or
  `models` is an array; it does not return parsed models to the UI.
- the Codex capture server responds with `{ "models": [] }`, so the fixture
  proves the route but not a non-empty model item schema.
- the Gateway responds with `{ models: [{ id, display_name, owned_by }] }` and
  exposes only `binding.selected_model`.
- a live `codex-cli 0.144.1` probe rejects the OpenAI standard
  `{ object: "list", data: [...] }` as missing `models`, and rejects the current
  Gateway item as missing `slug`.
- Responses Providers are routed directly to the upstream, so Codex sees the
  upstream's OpenAI model-list shape rather than a LAM-owned Codex catalog.

**Impact**: Account creation may succeed while Codex falls back to incomplete
model metadata. Model picker, reasoning controls, tool behavior, and context
handling can be wrong or degraded. Checked models other than the default are
not usable through the Gateway.

**What improves**: LAM owns a versioned Codex-facing catalog contract, converts
standard upstream model discovery into internal models, exposes every explicitly
allowed model, routes LAM API Accounts through one Codex-compatible Gateway, and
proves the result with the exact Codex binary rather than JSON-only assertions.

## Scope

**In scope**:

- exact-tested non-empty Codex model catalog schema for `codex-cli 0.144.1`;
- conservative metadata for unknown third-party models without claiming
  unverified reasoning or parallel-tool capability;
- standard OpenAI `{ object, data[] }` model-list parsing with bounded response,
  deterministic normalization, duplicate/invalid/empty handling, and bearer auth;
- a write-only Tauri discovery command used before API Account planning;
- removal of hard-coded External API model candidates;
- Gateway catalog for all Provider-allowed models and model allowlist enforcement;
- Gateway pass-through for Responses Providers so Codex never depends on an
  upstream's non-Codex model catalog shape;
- capture harness, fixtures, Rust/frontend tests, design and command contracts.

**Out of scope**:

- claiming compatibility with Codex versions other than `0.144.1`;
- inferring context windows, reasoning efforts, verbosity, image input, search,
  or parallel-tool support from model names;
- auto-persisting every upstream model without explicit user selection;
- Chat Completions support beyond the existing controlled adapter;
- arbitrary provider-specific model discovery formats without a separate parser.

## Design

### Contract boundaries

1. `OpenAiModelDiscovery` owns upstream `{ data: [...] }` parsing. It produces
   normalized internal `{ id, label }` values and never emits Codex JSON.
2. `CodexModelCatalog` owns Codex 0.144.1 JSON. It consumes only internal
   Provider models and conservative capability metadata; it never parses an
   upstream response.
3. `GatewayRouteComposer` authenticates the binding before route handling,
   returns the Codex catalog, and accepts a request model only when it is in the
   immutable Provider model allowlist carried by the binding snapshot.
4. Responses pass-through preserves the validated request body and upstream SSE
   bytes. Chat Completions continues through the existing adapter.
5. The frontend only displays discovery output or explicit custom models. The API
   key is write-only state, passed to discovery/execution, cleared on completion
   or error, and never included in a plan or persisted as Provider metadata.

### Codex catalog policy

The minimum exact-tested item must contain the fields that Codex 0.144.1 proved
required: `slug`, `display_name`, `supported_reasoning_levels`, `shell_type`,
`visibility`, `supported_in_api`, `priority`, `base_instructions`,
`supports_reasoning_summaries`, `support_verbosity`, `truncation_policy`,
`supports_parallel_tool_calls`, and `experimental_supported_tools`.

Unknown third-party models use conservative values: empty reasoning levels,
no reasoning summaries, no verbosity control, no parallel tool claim, no
experimental tools, bounded token truncation, and a stable generic coding-agent
base instruction. Optional Codex fields are omitted instead of invented.

### Model selection policy

- Upstream discovery is advisory input; the user explicitly chooses the active
  allowlist and default model.
- `ProviderProfileV2.models` is the allowed runtime set.
- `ProfileProviderBinding.selected_model` remains the default model written to
  `config.toml`, not the only model permitted at runtime.
- Gateway rejects any request model outside `provider.models` before upstream I/O.
- `/v1/models` returns every model in `provider.models` in deterministic order.

### Failure policy

- malformed, oversized, empty, duplicate-only, or missing upstream model lists
  return stable structured errors and do not mutate form/provider state;
- discovery never falls back to hard-coded models;
- invalid Codex catalog output is a release blocker, not a warning;
- Responses pass-through rejects response-store use and non-allowlisted models,
  preserves cancellation, and does not retry application requests.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
| --- | --- | --- | --- |
| T1 | Lock non-empty Codex catalog contract | Exact Codex accepts fixture model without fallback warning | 验证成功 |
| T2 | Gateway catalog and multi-model runtime | All allowed models listed/usable; unknown model rejected | 验证成功 |
| T3 | Responses Provider Gateway pass-through | API Accounts use Gateway and preserve Responses SSE | 验证成功 |
| T4 | Standard OpenAI model discovery API | Bounded authenticated discovery returns normalized models | 验证成功 |
| T5 | External API discovery UI | No hard-coded models; discovered/custom selection drives plan | 验证成功 |
| T6 | Contract/docs/full release gate | Live capture, all suites, lint/format/build pass | 验证成功 |

### T1: Lock non-empty Codex catalog contract

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Empty catalog fixtures cannot prove item compatibility; current
production output is rejected by the pinned Codex decoder.

**What to do**:

- add a focused Codex catalog module with typed serialized output;
- change the capture server to return one non-empty exact-tested fixture model;
- record model request paths and assert no fallback metadata warning;
- update offline fixtures/checksums only after two deterministic live runs.

**Logic design**:

- keep the Codex DTO private to the Gateway boundary;
- serialize required fields explicitly with stable deterministic values;
- reject empty/invalid internal model IDs before serialization;
- do not reuse OpenAI `Model` DTOs or expose `owned_by`/`id` as Codex fields.

**Test design**:

- Rust test: catalog uses `slug`, contains every required field, omits `id`, and
  preserves deterministic ordering;
- Rust invalid-input test: blank/duplicate slugs fail before a response is built;
- Node test: capture server returns a non-empty catalog;
- live exact-version test: Codex reaches `/responses` without the
  `Model metadata ... not found` fallback warning.
- Expected red state: production route lacks `slug`; capture returns an empty list.

**Acceptance**:

- `cargo test --test provider_gateway_routes --test provider_codex_contract`
- `node --test scripts/capture-codex-contract.test.mjs`
- exact `codex-cli 0.144.1` live text capture succeeds twice deterministically.

**Done criteria**:
- [x] Tests listed above were written before implementation
- [x] Expected red state was recorded: Rust cannot resolve `gateway::catalog`;
  Node cannot import the non-empty catalog/fallback assertion helpers
- [x] Focused tests and live exact-version probe pass; two runs observed
  `GET /v1/models?client_version=0.144.1` followed by `POST /v1/responses`
  with exit code 0 and no metadata fallback warning
- [x] Fixture sanitization/checksums pass
- [x] Task overview and task status are `验证成功`

### T2: Gateway catalog and multi-model runtime

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: The UI calls multiple models “active”, but Gateway currently lists and
permits only the binding default.

**What to do**:

- return all Provider allowlisted models in `/v1/models`;
- validate request `model` against the Provider model set;
- translate Chat Completions using the requested allowed model;
- keep unknown models blocked before upstream I/O.

**Logic design**:

- binding `selected_model` is the configuration default;
- Provider model collection is the runtime allowlist;
- model lookup is exact/case-sensitive and bounded by Provider validation;
- response conversion reports the actual request model.

**Test design**:

- normal: two models appear in deterministic Codex catalog order;
- normal: second allowlisted model reaches Chat Completions upstream;
- invalid: unlisted model returns structured 400 with zero upstream requests;
- state: changing the binding default does not remove other Provider models.
- Expected red state: current route filters to one model and rejects the second.

**Acceptance**:

- `cargo test --test provider_gateway_routes --test provider_gateway_adapter_integration`

**Done criteria**:
- [x] Tests-first red state recorded: the catalog exposes one legacy `id`
  item and the second Provider-allowlisted model receives HTTP 400
- [x] All allowed/invalid/state cases pass
- [x] Existing tool, SSE, retry and cancellation tests pass
- [x] Task overview and task status are `验证成功`

### T3: Responses Provider Gateway pass-through

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Direct Responses routing exposes Codex to the upstream's standard
OpenAI model schema, which Codex 0.144.1 does not decode as its model catalog.

**What to do**:

- add an explicit `routeViaGateway` Codex Provider option;
- set it for new External API Accounts;
- plan Responses Providers with this option through Gateway without an adapter;
- pass validated `/responses` requests and SSE/error responses through securely;
- preserve existing direct Provider behavior when the option is false.

**Logic design**:

- route preference is explicit metadata, not inferred from credentials or names;
- Gateway owns Codex routes while upstream protocol remains Responses;
- request body bytes are forwarded only after strict parsing, store/history/model
  checks, and binding auth;
- streaming uses bounded channels, byte limits and cancellation propagation;
- no application retry is introduced.

**Test design**:

- planner: opt-in Responses route is Gateway; default remains Direct;
- config: Gateway base URL/auth/retry projection is written for opt-in Responses;
- route: upstream receives the selected allowed model and exact supported fields;
- stream: valid Responses SSE is forwarded in order and cancellation closes upstream;
- errors: disallowed model/store/history/invalid content type fail deterministically.
- Expected red state: planner selects Direct and Gateway rejects Responses protocol.

**Acceptance**:

- `cargo test --test provider_planner --test provider_config_editor`
- `cargo test --test provider_gateway_routes --test provider_gateway_binding`
- `cargo test --test provider_phase4_api_account`

**Done criteria**:
- [x] Tests-first red state recorded: planner, Gateway, and API Account tests
  cannot compile because the explicit `route_via_gateway` contract is absent
- [x] Direct backward compatibility and Gateway opt-in tests pass
- [x] Streaming/error/cancellation cases pass
- [x] Task overview and task status are `验证成功`

### T4: Standard OpenAI model discovery API

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: A valid standard `/models` response is currently only health-checked;
its model IDs cannot enter the account creation flow.

**What to do**:

- add a focused discovery parser/service and Tauri command;
- accept a validated Base URL and write-only bearer token;
- fetch `GET <base>/models` without proxy/redirect and within existing limits;
- normalize `data[].id` into sorted unique `{ id, label }` results;
- expose the command through TypeScript API/types and command contract docs.

**Logic design**:

- transport, parser and UI contracts stay separate;
- request `Debug` redacts the token;
- endpoint construction reuses the validated Provider URL rules;
- parser requires a standard `data` array, ignores harmless standard fields,
  rejects missing/blank IDs and duplicate IDs, and caps model count/body size;
- discovery has no persistence side effect.

**Test design**:

- parser normal/order cases;
- invalid JSON/missing `data`/blank ID/duplicate/empty/oversized/count-limit cases;
- transport auth/path/no-redirect/HTTP error cases with a fake or loopback server;
- command registration and frontend invoke payload contract;
- secret redaction test.
- Expected red state: no discovery command/types/parser exist.

**Acceptance**:

- `cargo test --test provider_model_discovery --test provider_api_v2`
- `npm test -- --run src/lib/provider-api-v2.test.ts`

**Done criteria**:
- [x] Tests-first red state recorded: Rust cannot resolve the discovery module
  and the TypeScript discovery wrapper is not a function
- [x] Parser/transport/security/command tests pass
- [x] No discovery secret is persisted or logged
- [x] Task overview and task status are `验证成功`

### T5: External API discovery UI

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Hard-coded model labels can be invalid upstream IDs and make account
creation appear successful before requests fail.

**What to do**:

- remove `MODERN_MODELS`;
- add explicit `Fetch models` with loading/error/empty states;
- render discovered IDs as selectable models and retain custom model entry;
- reset stale discovery when Base URL or API key changes;
- require the default model to be in the selected allowlist;
- set `routeViaGateway` for new External API Accounts.

**Logic design**:

- discovery does not auto-select or persist models; user choice remains explicit;
- stale async responses cannot replace results for newer credentials/URL;
- API key is cleared after create/error and never placed in plan payload;
- accessibility uses real checkbox/button semantics and stable labels.

**Test design**:

- normal: fetch invokes backend, renders models, selection/default enters plan;
- state: URL/key change clears stale discovered models and plan;
- race: an older discovery response cannot overwrite a newer request;
- error/empty: clear actionable message and custom-model fallback remain;
- invalid: plan blocked when default is outside selected models;
- regression: reuse-existing Provider flow remains unchanged.
- Expected red state: hard-coded candidates render and discovery API is never called.

**Acceptance**:

- `npm test -- --run src/components/api-account-flow.test.tsx src/lib/provider-api-v2.test.ts`
- `npm run lint && npm run format:check && npm run build`

**Done criteria**:
- [x] Tests-first red state recorded: hard-coded `5.6 Sol` renders, no `Fetch
  models` control exists, and discovery/custom accessibility queries fail
- [x] Normal/state/race/error/invalid/regression tests pass
- [x] Source changes have mapped tests
- [x] Task overview and task status are `验证成功`

### T6: Contract/docs/full release gate

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Strict compatibility is only credible when the exact binary, offline
fixtures, source tests and product documentation all describe one contract.

**What to do**:

- regenerate sanitized normal/failure captures twice;
- update fixture manifest/checksums and offline verifier expectations;
- update design, security and Tauri command docs;
- run full Rust/frontend/format/lint/build gates;
- verify no probe/background Codex processes remain.

**Logic design**:

- exact-version live test is a release gate, not a normal offline test dependency;
- fixtures contain no real credentials, paths, prompts or user state;
- design states OpenAI discovery schema and Codex catalog schema are different;
- future Codex upgrades must rerun non-empty catalog capture.

**Test design**:

- offline checksum/sanitization verifier rejects stale artifacts;
- live capture asserts both `/models` and `/responses` and no fallback warning;
- full suites cover all changed modules.
- Expected red state: fixture manifest/checksums become stale after T1 changes.

**Acceptance**:

- `cargo test`
- `cargo fmt --check`
- `npm test -- --run`
- `npm run lint`
- `npm run format:check`
- `npm run build`
- `git diff --check`

**Verification evidence**:

- exact `codex-cli 0.144.1` normal and failure captures each completed twice on
  Darwin arm64/macOS 15.6; both required routes were observed and no fallback
  metadata warning was emitted;
- Phase 0 offline gate passed 12 scenarios and 16 sanitized/checksummed artifacts;
- full `cargo test`, `cargo fmt --check`, and
  `cargo clippy --all-targets -- -D warnings` passed;
- frontend passed 22 files / 187 tests, ESLint, Prettier check, and production build
  without the previous Vitest/Vite deprecation warning;
- `git diff --check` passed, and process inspection found no capture/probe,
  temporary Gateway, or fake Codex process left running.

**Done criteria**:
- [x] Tests-first red state recorded: two-run live capture refreshed fixtures
  and the offline gate rejected the stale manifest checksum
- [x] Live and offline contract gates pass
- [x] Full Rust/frontend/quality gates pass
- [x] Documentation matches implemented behavior
- [x] No unresolved warning, stale fixture or background process remains
- [x] Task overview and task status are `验证成功`

## Test plan

- Normal: standard model discovery, multiple model catalog, default and secondary
  runtime requests, Responses and Chat Completions streaming.
- Edge: deterministic order, maximum sizes/counts, one model, custom model.
- Invalid: OpenAI schema missing/blank/duplicate models, Codex catalog invalid input,
  model outside allowlist, response-store fields, malformed SSE/content type.
- Error: auth/HTTP/redirect/timeout/oversize, upstream disconnect, stale discovery.
- State/conflict: URL/key changes, concurrent discovery response order, binding
  default versus Provider allowlist, direct Provider backward compatibility.

## Verification commands

| Purpose | Command | Expected on success |
| --- | --- | --- |
| Codex catalog | `cargo test --test provider_gateway_routes --test provider_codex_contract` | exit 0 |
| Gateway runtime | `cargo test --test provider_gateway_adapter_integration --test provider_planner --test provider_config_editor` | exit 0 |
| Discovery backend | `cargo test --test provider_model_discovery --test provider_api_v2` | exit 0 |
| Frontend | `npm test -- --run src/components/api-account-flow.test.tsx src/lib/provider-api-v2.test.ts` | exit 0 |
| Full Rust | `cargo test && cargo fmt --check` | exit 0 |
| Full frontend | `npm test -- --run && npm run lint && npm run format:check && npm run build` | exit 0 |
| Diff hygiene | `git diff --check` | exit 0 |

## Done criteria

- [x] Every task's status is `验证成功`
- [x] Every task's own Done criteria are checked
- [x] Exact Codex 0.144.1 accepts a non-empty catalog without fallback warning
- [x] Standard OpenAI discovery drives explicit UI model selection
- [x] All allowed Gateway models work and unlisted models cannot reach upstream
- [x] Responses API Accounts use the Codex-compatible Gateway path
- [x] All full verification commands pass
- [x] No STOP condition remains unresolved

## STOP conditions

- the pinned Codex binary requires undocumented fields that cannot be established
  through the official manual plus controlled local capture;
- strict pass-through would require silently dropping a Codex request field;
- secure discovery cannot be implemented without broadening the established
  network/credential trust boundary;
- current user changes conflict with a required source edit and cannot be merged;
- a task still fails after five focused fix loops;
- completion would require claiming support for an untested Codex version.
