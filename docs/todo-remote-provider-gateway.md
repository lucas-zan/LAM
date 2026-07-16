# Todo: Remote Provider Gateway

> Executor instructions: Execute this plan issue by issue. For every source-file
> change, write or update behavior-focused tests first, run them, and confirm the
> expected failure before implementation. Do not start an issue whose dependencies
> or phase gate are incomplete. Stop and report when a STOP condition is met
> instead of inventing a new contract during implementation.

## Status

- **Overall status**: 待执行
- **Priority**: P0
- **Risk**: HIGH
- **Delivery model**: contract-first, test-driven, vertical slices
- **Primary design**: [`remote-provider-gateway-full-design.md`](./remote-provider-gateway-full-design.md)
- **Planning baseline**: 2026-07-10 workspace state
- **Observed Codex CLI**: `codex-cli 0.144.1`; Phase 0 must confirm and pin the supported contract target
- **Supersedes for this feature**: [`todo-v0.2-remote-api-provider.md`](./todo-v0.2-remote-api-provider.md)

This document is the authoritative implementation tracker for the Remote
Provider Gateway. The older v0.2 todo remains historical context and must not be
used to override the contracts, phase gates, or acceptance criteria below.

Each issue gets a focused `docs/todo-rpg-<id>-<slug>.md` execution contract when
work starts. That execution file owns the TDD status checklists and is linked
from the issue before source code changes.

## Outcome

LAM becomes a local Provider Hub that:

- manages versioned Provider metadata, model catalogs, credential references,
  bindings, health, readiness, and capability evidence;
- attaches Responses-compatible providers directly to an isolated `CODEX_HOME`;
- attaches Chat Completions providers through an authenticated loopback Gateway;
- presents Codex only with a verified `/v1/responses` contract;
- keeps Provider differences in presets and compatibility policies rather than
  scattered vendor branches;
- preserves user-owned Codex configuration and never persists plaintext provider
  secrets in LAM metadata, frontend state, logs, sessions, relay artifacts, or
  generated wrappers;
- keeps API profiles usable for session browse, resume, sync, and relay according
  to explicit compatibility analysis.

## Non-negotiable engineering rules

1. **Tests first**: tests or fixtures must be added and observed failing before
   implementation, except for a documentation-only issue.
2. **One issue, one coherent behavior change**: avoid mixing mechanical module
   moves, schema migration, API changes, UI changes, and runtime behavior in one
   large diff.
3. **No hidden authority**: Provider/profile/model relationships come from
   `ProfileBindingStore` after migration. Codex config and Gateway binding are
   projections with explicit drift detection.
4. **No secret values in domain or view models**: only credential references may
   cross service and API boundaries.
5. **No distributed provider conditionals**: vendor behavior belongs in a preset
   or typed compatibility policy registered with the adapter.
6. **No unbounded work**: network, subprocess, request body, SSE frame, tool
   arguments, channel capacity, retries, response duration, and local stores all
   have explicit limits.
7. **No destructive config recovery**: only LAM-managed keys may be restored.
   Whole-file rollback is never inferred merely because a backup exists.
8. **No development past a failed gate**: fix the gate or record an approved
   scope change in the design and this todo before continuing.
9. **Keep public functions small and testable**: core functions should normally
   remain 10–20 lines; split flows that exceed 30 lines and justify any function
   over 40 lines.
10. **Keep the worktree safe**: preserve unrelated user changes and never use a
    destructive Git operation to recover generated artifacts.

## Status values

Each issue must have exactly one status:

- `待执行`
- `待测试验证`
- `验证成功`
- `验证失败`

An issue is not `验证成功` until:

- all issue-specific tests were written before implementation and their expected
  initial failure was recorded;
- focused tests pass;
- relevant Rust/frontend suites, formatter, linter, and build pass;
- security and failure-path assertions for the issue pass;
- source behavior and related documentation are updated together;
- the issue row and detailed section show the same status.

## Scope boundaries

### In scope

- Rust domain, persistence, config editing, credential resolution, adapters,
  Gateway, supervisor, launcher, and Tauri command boundaries.
- Frontend Provider Center, attach/rebind/detach flows, readiness and capability
  display, and session/relay integration.
- Legacy Provider/config/DTO migration.
- Mock-backed contract, integration, fault-injection, security, frontend, smoke,
  packaging, and manual acceptance tests.

### Out of scope for the first MVP

- A public network-facing Gateway.
- Full OpenAI Responses API compatibility.
- Silent support for unknown Responses fields or unsupported tool/input types.
- Automatic bootstrap for arbitrary `CODEX_HOME=... codex` commands that bypass
  the LAM launcher.
- Migrating API keys, Gateway tokens, `auth.json`, identity, billing, or quota
  state during relay/sync.
- Anthropic Messages adaptation; only its extension seam is preserved.
- Hosted tools, MCP, computer use, image/audio/file inputs unless a later design
  explicitly promotes them into the verified subset.

## Phase gates

| Gate               | Status   | Required before                             | Exit criteria                                                                                                                                                 |
| ------------------ | -------- | ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| G0 Baseline        | 验证成功 | Contract capture and feature implementation | Existing Rust/frontend tests and build are green; lint/format baseline is normalized; reproducible baseline failures are fixed or explicitly owned            |
| G1 Contract        | 验证成功 | Phase 1                                     | Legacy fixtures frozen; supported Codex version pinned; request/stream/tool/resume/retry fixtures captured and sanitized; normative design decisions recorded |
| G2 Direct Provider | 验证成功 | Phase 2 and merge of Phase 1                | Responses Provider create → dry-run → attach → launch/test → detach passes; config preservation, drift, migration, and secret tests pass                      |
| G3 Adapter         | 验证成功 | Phase 3                                     | Pure adapter text, streaming, tool, usage, error, DeepSeek thinking/history fixtures pass without HTTP/Gateway dependencies                                   |
| G4 Gateway         | 验证成功 | Phase 4 and MVP claim                       | Sidecar, auth, stable port, launcher, packaging, recovery, retry budget, and mock DeepSeek end-to-end tests pass                                              |
| G5 Product         | 验证成功 | Release                                     | Account-first lifecycle, capability, provider-aware relay, metrics, docs, full gates, and packaged artifact verification pass                                  |

G0 evidence: [`RPG-000`](./todo-rpg-000-green-baseline.md) and
[`RPG-006`](./todo-rpg-006-frontend-quality-baseline.md) are both `验证成功`;
Rust/frontend tests, frontend build, lint, format, and diff checks pass.

G1 evidence: [`RPG-001`](./todo-rpg-001-legacy-contract-fixtures.md),
[`RPG-002`](./todo-rpg-002-codex-contract-capture.md),
[`RPG-003`](./todo-rpg-003-normative-contracts.md),
[`RPG-004`](./todo-rpg-004-threat-platform-boundaries.md), and
[`RPG-005`](./todo-rpg-005-phase-zero-gate.md) are `验证成功`; the
[coverage report](./remote-provider-gateway-contract-coverage.md) and versioned
approval artifact are enforced by `pnpm test:gateway-phase0`.

G3 evidence: [`RPG-201`](./todo-rpg-201-controlled-protocol-schemas.md) through
[`RPG-208`](./todo-rpg-208-deepseek-g3.md) are `验证成功`; the pure adapter owns
typed schemas, registry/exchange isolation, request/non-stream/stream conversion,
function tools, terminal normalization, and DeepSeek thinking/history without an
HTTP/Gateway dependency. `provider_phase2_g3` enforces official-source metadata,
named resource limits, fixture sanitization, and SHA-256 checksums.

G4 evidence: [`RPG-301`](./todo-rpg-301-gateway-binding-token.md) through
[`RPG-308`](./todo-rpg-308-phase3-g4.md) are `验证成功`. The real G4 vertical
test covers attach, bearer helper, generated Codex retry ownership, loopback
`/v1/models` and `/v1/responses`, mock DeepSeek non-stream/SSE/function tools,
UI-close survival, detach, immediate old-token rejection, and persisted/logged
secret scans. Focused suites cover restart recovery, rebind/rotation/drift,
foreign-port migration rollback, control authentication, SSRF/redirect/limits,
fault injection, and the one-connect-retry budget. Final `.app` component hashes,
deep code signature, and the regenerated DMG checksum are verified.

## Dependency flow

```text
RPG-000 -> RPG-006 -> G0
  -> RPG-001 -> RPG-002 -> RPG-003/RPG-004 -> RPG-005 -> G1
  -> Phase 1 domain/persistence/config/credential vertical slice -> RPG-112 -> G2
  -> Phase 2 pure adapter slice -> RPG-208 -> G3
  -> Phase 3 Gateway/sidecar/launcher slice -> RPG-308 -> G4
  -> Phase 4 product/relay/release slice -> RPG-406 -> G5
```

Within a phase, issues may run in parallel only when their dependencies are
complete and they do not edit the same source module or contract.

## Issue overview

### Phase 0 — contract and safety baseline

| ID      | Issue                                                               | Priority | Effort | Depends on       | Status   |
| ------- | ------------------------------------------------------------------- | -------- | ------ | ---------------- | -------- |
| RPG-000 | Restore a green repository test baseline                            | P0       | S      | none             | 验证成功 |
| RPG-006 | Normalize the existing frontend lint and format baseline            | P0       | M      | RPG-000          | 验证成功 |
| RPG-001 | Freeze legacy Provider, config, account, and DTO fixtures           | P0       | M      | RPG-006          | 验证成功 |
| RPG-002 | Pin and capture the Codex Gateway contract                          | P0       | L      | RPG-001          | 验证成功 |
| RPG-003 | Complete normative domain, auth, retry, and recovery contracts      | P0       | M      | RPG-001, RPG-002 | 验证成功 |
| RPG-004 | Complete threat model, platform scope, and sidecar trust boundaries | P0       | M      | RPG-002          | 验证成功 |
| RPG-005 | Automate and approve the Phase 0 gate                               | P0       | S      | RPG-003, RPG-004 | 验证成功 |

### Phase 1 — Responses Provider direct vertical slice

| ID      | Issue                                                                       | Priority | Effort | Depends on                                                             | Status   |
| ------- | --------------------------------------------------------------------------- | -------- | ------ | ---------------------------------------------------------------------- | -------- |
| RPG-101 | Add cross-process versioned store primitives                                | P0       | L      | G1                                                                     | 验证成功 |
| RPG-102 | Implement Provider V2 model and legacy migration                            | P0       | L      | RPG-101                                                                | 验证成功 |
| RPG-103 | Split credential source from upstream authentication and add env resolution | P0       | M      | RPG-101                                                                | 验证成功 |
| RPG-104 | Implement ProfileBindingStore adoption, drift, and profile lifecycle        | P0       | L      | RPG-101, RPG-102                                                       | 验证成功 |
| RPG-105 | Implement non-destructive CodexConfigEditor and managed projection          | P0       | L      | RPG-102, RPG-103                                                       | 验证成功 |
| RPG-106 | Implement route/attach planners and expiring dry-run fingerprints           | P0       | M      | RPG-102, RPG-103, RPG-104, RPG-105                                     | 验证成功 |
| RPG-107 | Implement journaled attach, rebind, detach, and crash recovery              | P0       | XL     | RPG-106                                                                | 验证成功 |
| RPG-108 | Implement versioned Keychain credential lifecycle                           | P1       | M      | RPG-103, RPG-107                                                       | 验证成功 |
| RPG-109 | Implement restricted auth command and direct-provider auth helper           | P1       | L      | RPG-103, RPG-105                                                       | 验证成功 |
| RPG-110 | Introduce V2 Tauri DTOs and compatibility commands                          | P0       | M      | RPG-102, RPG-103, RPG-104, RPG-105, RPG-106, RPG-107, RPG-108, RPG-109 | 验证成功 |
| RPG-111 | Deliver the minimal Responses Provider frontend flow                        | P0       | L      | RPG-110                                                                | 验证成功 |
| RPG-112 | Pass the Responses Provider vertical-slice gate                             | P0       | L      | RPG-107, RPG-110, RPG-111                                              | 验证成功 |

### Phase 2 — pure protocol adapter

| ID      | Issue                                                               | Priority | Effort | Depends on       | Status   |
| ------- | ------------------------------------------------------------------- | -------- | ------ | ---------------- | -------- |
| RPG-201 | Define the controlled Responses and Chat Completions schemas        | P0       | L      | G2, RPG-002      | 验证成功 |
| RPG-202 | Implement object-safe adapter registry and exchange contracts       | P0       | M      | RPG-201          | 验证成功 |
| RPG-203 | Translate Responses requests and reject unsupported input           | P0       | L      | RPG-202          | 验证成功 |
| RPG-204 | Translate non-stream Chat Completions responses                     | P0       | M      | RPG-203          | 验证成功 |
| RPG-205 | Implement the bounded SSE conversion state machine                  | P0       | XL     | RPG-203          | 验证成功 |
| RPG-206 | Implement function tool-call round trips                            | P0       | L      | RPG-204, RPG-205 | 验证成功 |
| RPG-207 | Normalize usage, errors, cancellation, and retry classification     | P0       | L      | RPG-204, RPG-205 | 验证成功 |
| RPG-208 | Implement DeepSeek thinking compatibility and pass the adapter gate | P0       | XL     | RPG-206, RPG-207 | 验证成功 |

### Phase 3 — Gateway, sidecar, and launcher

| ID      | Issue                                                                         | Priority | Effort | Depends on                         | Status |
| ------- | ----------------------------------------------------------------------------- | -------- | ------ | ---------------------------------- | ------ |
| RPG-301 | Implement Gateway binding credentials and token lifecycle                     | P0       | L      | G3, RPG-104, RPG-108               | 验证成功 |
| RPG-302 | Build the authenticated loopback server and verifiable health endpoint        | P0       | L      | RPG-301                            | 验证成功 |
| RPG-303 | Build the bounded upstream client and authentication injection                | P0       | XL     | RPG-103, RPG-302                   | 验证成功 |
| RPG-304 | Compose `/v1/responses` non-stream and streaming routes                       | P0       | XL     | RPG-208, RPG-303                   | 验证成功 |
| RPG-305 | Implement sidecar state, stable port, supervisor, and private control channel | P0       | XL     | RPG-302, RPG-304                   | 验证成功 |
| RPG-306 | Implement launcher, auth helper, install manifest, and application packaging  | P0       | XL     | RPG-301, RPG-305                   | 验证成功 |
| RPG-307 | Route all supported Codex entry points through one launch planner             | P0       | L      | RPG-306                            | 验证成功 |
| RPG-308 | Pass Gateway recovery, security, retry, and mock DeepSeek gates               | P0       | XL     | RPG-304, RPG-305, RPG-306, RPG-307 | 验证成功 |

### Phase 4 — product integration, relay, and release

| ID      | Issue                                                                        | Priority | Effort | Depends on       | Status |
| ------- | ---------------------------------------------------------------------------- | -------- | ------ | ---------------- | ------ |
| RPG-401 | Implement readiness, health, capability resolution, and conformance evidence | P1       | L      | G4               | 验证成功 |
| RPG-402 | Complete Provider Center and attach/rebind/detach UX                         | P1       | XL     | RPG-401, RPG-110 | 验证成功 |
| RPG-403 | Implement the pure RelayCompatibilityAnalyzer                                | P0       | L      | RPG-201, RPG-401 | 验证成功 |
| RPG-404 | Integrate API profiles with session, resume, sync, and relay                 | P0       | XL     | RPG-307, RPG-403 | 验证成功 |
| RPG-405 | Complete smoke, security, migration, observability, and documentation suites | P0       | L      | RPG-402, RPG-404 | 验证成功 |
| RPG-406 | Complete manual acceptance and release the feature                           | P0       | L      | RPG-405          | 验证成功 |

## Phase 0 issues

### RPG-000: Restore a green repository test baseline

- **Status**: 验证成功
- **Suggested label**: `phase:0`, `area:test`, `priority:p0`
- **Suggested commit**: `test: restore green pre-gateway baseline`

**Objective**

Establish a trustworthy baseline so later failures can be attributed to the
Gateway work. At planning time, Rust tests were green, while the frontend suite
had one reproducible failure in
`src/routes/handoff.test.tsx` around the PAT switch action.

**Scope**

- Reproduce the full frontend failure without concurrent Rust compilation.
- Decide from observable behavior whether the test expectation or production UI
  is wrong.
- Fix only that behavior/test and any deterministic test isolation issue.
- Record the passing baseline counts and commands in this document.

**Test design**

- Run the full frontend suite and the focused handoff test before changing code.
- If the defect is in production behavior, add the smallest failing assertion
  that describes the intended button availability.
- If it is stale test markup, update the test to assert public accessible behavior
  rather than private DOM structure.
- Confirm fake timers and shared Zustand state are reset between tests.

**Implementation tasks**

- Remove the root cause, not merely the timeout.
- Keep Gateway/Provider behavior untouched.
- Ensure test runs do not leave tracked fixture changes.

**Acceptance**

- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` exits 0.
- `cd apps/desktop && npm test` exits 0 in two consecutive runs.
- `cd apps/desktop && npm run build` exits 0.
- Focused lint and diff checks for changed files pass.
- `git status --short` contains no test-generated changes.

**STOP conditions**

- The failure depends on an unrelated unfinished user change.
- Fixing it requires changing product behavior without an existing requirement.

### RPG-006: Normalize the existing frontend lint and format baseline

- **Status**: 验证成功
- **Suggested label**: `phase:0`, `area:quality`, `priority:p0`
- **Suggested commit**: `style: normalize frontend lint and format baseline`
- **Depends on**: RPG-000

**Objective**

Make the repository's declared frontend lint and format commands trustworthy
before contract fixtures and feature code begin.

**Current evidence**

- The initial red baseline contained three lint errors, seven warnings, and 33
  format failures.
- Semantic fixes are covered by pagination and stale-request regression tests.
- `npm run lint` and `npm run format:check` now exit 0.
- The 14-file, 129-test frontend suite passed twice after formatting; the
  production build and `git diff --check` also passed.

**Scope**

- Create a focused execution TODO before source changes.
- Split semantic ESLint fixes from a mechanical Prettier-only rewrite.
- Add/update tests before each semantic fix.
- Preserve public behavior and avoid Gateway/Provider work.
- Run the full frontend suite/build/lint/format after both parts.

**Test design**

- Capture the exact current lint/format output as red baseline evidence.
- Add behavior tests for each semantic lint correction where state/effect/finally
  control flow changes.
- Require `npm run lint` and `npm run format:check` to exit 0 afterward.
- Prove the mechanical formatting commit changes no generated behavior through
  the full test/build suite.

**Acceptance**

- All frontend tests pass twice after normalization.
- Frontend build, lint, and format checks exit 0.
- The formatting diff is isolated from semantic fixes for review.
- G0 can be marked passed after RPG-000 and RPG-006 are both successful.

**STOP conditions**

- A lint correction requires a product behavior decision not established by
  existing tests/design.
- Formatting overlaps unrelated uncommitted user source changes.

### RPG-001: Freeze legacy Provider, config, account, and DTO fixtures

- **Status**: 验证成功
- **Suggested label**: `phase:0`, `area:migration`, `priority:p0`
- **Suggested commit**: `provider: freeze legacy contract fixtures`
- **Depends on**: RPG-006

**Objective**

Capture every existing persisted/API shape that Phase 1 must read without
silently rewriting or losing data.

**Fixture inventory**

- Legacy `providers.json` array using `wireApi/openai`, `wireApi/responses`,
  `defaultModel`, `envKey`, health, and secret-storage fields.
- Empty and missing Provider stores.
- Malformed JSON and future schema versions.
- Existing Codex config with only top-level model/provider keys.
- Official `[model_providers.<id>]` config with comments and unknown tables.
- Legacy flat provider keys, dotted/quoted Provider IDs, trailing comments,
  CRLF, and non-ASCII display names.
- Config containing mutually exclusive auth modes and synthetic secret-bearing
  keys, using non-secret placeholder values.
- Existing account cache, account metadata, frontend Provider JSON, attach request
  JSON, and command result JSON.

**Test design**

- Add golden deserialization tests for each legacy store and DTO.
- Add round-trip tests proving fixture reads do not write or change mtime.
- Add negative tests for unknown future versions and malformed data.
- Add snapshots for current Tauri camelCase response shapes.
- Expected initial failure: V2 migration APIs and fixture loaders do not exist.

**Implementation tasks**

- Place fixtures under focused Rust/frontend test fixture directories.
- Add a manifest with fixture ID, origin, schema/version, expected migration, and
  whether write-back is allowed.
- Sanitize all fixture secrets and paths.
- Do not implement migration yet.

**Acceptance**

- All current legacy fixtures are checked in and immutable through tests.
- No fixture contains a token pattern, real home path, or personal identifier.
- The full baseline remains green.

**Validation evidence**

- The [RPG-001 execution TODO](./todo-rpg-001-legacy-contract-fixtures.md) locks
  19 logical cases and 18 physical fixtures with size/checksum validation.
- Focused contract tests cover no-write/mtime, malformed/future rejection,
  config variants, current DTO round trips, frontend shapes, and sanitization.
- An existing `secret.envKey` versus Rust `secret.env_key` compatibility gap is
  frozen explicitly for RPG-110; RPG-001 does not alter production behavior.
- Full Rust/frontend suites and all applicable format/lint/build checks pass.

### RPG-002: Pin and capture the Codex Gateway contract

- **Status**: 验证成功
- **Suggested label**: `phase:0`, `area:contract`, `priority:p0`
- **Suggested commit**: `gateway: capture codex responses contract`
- **Depends on**: RPG-001

**Objective**

Replace assumptions about Codex HTTP behavior with sanitized fixtures tied to an
explicit supported Codex version and platform.

**Required observations**

- Exact path composition from Provider `base_url`.
- Request headers, auth-helper invocation, request JSON, default fields, model,
  instructions, input items, tools, stream flag, store/state fields, and unknown
  fields.
- Non-stream response body required by Codex.
- Streaming event names, ordering, IDs, terminal events, and error handling.
- Function tool request, function call output, and subsequent-turn history.
- Resume behavior: full input items versus `previous_response_id` or bounded
  state routes.
- Whether Codex calls `/v1/models`, cancel, retrieve, or other endpoints.
- Client disconnect and cancellation behavior.
- Current `request_max_retries` and `stream_max_retries` behavior, including
  Retry-After.
- Auth command timeout, refresh, retry, stdout trimming, and empty-token behavior.

**Test design**

- Build a local capture server that stores sanitized request metadata and returns
  controlled response fixtures.
- Add scripted cases for text non-stream, text stream, one function tool round
  trip, resume, 401 auth refresh, 429, 500, malformed SSE, and disconnect.
- Verify captured fixtures contain no prompt content beyond fixed synthetic test
  strings and no real token.
- Run each capture twice and flag nondeterministic fields explicitly.

**Implementation tasks**

- Pin the initial contract target, expected to start from observed
  `codex-cli 0.144.1` unless the project chooses another supported version.
- Record OS, architecture, Codex version, capture timestamp, config, and expected
  volatile fields in fixture metadata.
- Produce a support policy: exact version, minimum version, or tested version
  range.
- Update the full design with the proven MVP route and state requirements.

**Acceptance**

- Every required observation is either captured or explicitly marked unsupported
  with evidence.
- The route set for Phase 3 is no longer conditional prose.
- Fixtures can drive offline adapter/Gateway contract tests.

**Validation evidence**

- The [RPG-002 execution TODO](./todo-rpg-002-codex-contract-capture.md) and
  `codex-gateway-contract/manifest.json` pin exact-tested Codex 0.144.1 on
  macOS 15.6 / Darwin arm64.
- Two-run deterministic fixtures cover text stream, function tool, resume,
  route inventory, auth trim/empty/401, 429, 500, malformed SSE, and disconnect.
- Proven routes are `GET /v1/models` and `POST /v1/responses`; state mode is
  full-input with no `previous_response_id` or response store.
- Defaults can multiply a 500 into 30 requests and malformed SSE into 6; the
  design now requires both Codex retry counts to be explicitly zero for Gateway
  profiles.
- Offline verifier, harness tests, full Rust/frontend suites, format/lint/build,
  sanitization/checksum, and diff checks pass.

**STOP conditions**

- Codex cannot be made to use the local capture server without modifying private
  product state.
- Critical requests contain undocumented state that cannot be sanitized or
  reproduced.

### RPG-003: Complete normative domain, auth, retry, and recovery contracts

- **Status**: 验证成功
- **Suggested label**: `phase:0`, `area:architecture`, `priority:p0`
- **Suggested commit**: `docs: lock remote gateway implementation contracts`
- **Depends on**: RPG-001, RPG-002

**Objective**

Turn remaining conceptual names and ambiguous policies into implementable,
testable types before Phase 1 code starts.

**Required decisions**

- Define `ProviderProtocol`, `RouteKind`, `ProviderModel`,
  `CodexProviderOptions`, `CredentialSource`, `UpstreamAuth`, `AuthCommand`,
  `GatewayCredentialReference`, `ManagedConfigPatch`,
  `ManagedConfigProjection`, `ReadinessBlocker`, and the initial capability set.
- Add `route_kind` to `ProviderRoutePlan`.
- Separate credential storage from bearer/header/no-auth transport.
- Define Provider fields that are mutable while in use and the outcome for a
  route-breaking edit.
- Define whether ProfileBindingStore covers all profiles or only externally
  attached profiles.
- Define initial binding adoption, config drift, profile rename, profile delete,
  and unmanageable config states.
- Choose installation-wide locking and atomic-store semantics. The default plan
  is a versioned file store protected by a cross-process lock; any different
  decision must update this todo and the design.
- Define journal record fields, states, lock order, commit point, recovery owner,
  rollback/roll-forward rules, and retention.
- Define dry-run plan fingerprint inputs, TTL, replay behavior, and redaction.
- Choose the single retry owner and explicit Codex/Gateway retry counts.
- Define non-stream versus stream adapter output contracts and `Send + Sync`
  requirements.
- Define whether raw DeepSeek `reasoning_content` is suppressed or mapped; do not
  call it a Responses summary without contract evidence.

**Test design**

- Documentation-only issue, but every normative rule must map to a named test in
  a later issue.
- Add a traceability table from decision → implementing issue → verifying test.
- Review example states for attach, rebind, detach, crash recovery, drift, and
  Provider edit.

**Acceptance**

- No critical runtime struct in the design is name-only.
- Every state transition has one owner and one deterministic outcome.
- No `reject or preserve`, `later decide`, or implicit retry policy remains in
  the MVP contract.

**Validation evidence**

- The [RPG-003 execution TODO](./todo-rpg-003-normative-contracts.md) is
  `验证成功`.
- The full design defines all required closed types, Provider mutation and
  binding lifecycle rules, installation-wide store/lock/atomic-write semantics,
  dry-run replay and journal recovery, retry ownership, adapter output/thread
  boundaries, and raw reasoning suppression.
- The Phase 0 decision traceability table maps every contract group to an
  implementing issue and named red-first test.
- Required-name scan, ambiguity review, Prettier check, and `git diff --check`
  pass.

### RPG-004: Complete threat model, platform scope, and sidecar trust boundaries

- **Status**: 验证成功
- **Suggested label**: `phase:0`, `area:security`, `priority:p0`
- **Suggested commit**: `docs: define gateway threat and platform model`
- **Depends on**: RPG-002

**Objective**

Define the security boundary before introducing an auth helper, arbitrary
Provider URLs, local bearer tokens, command-backed credentials, or a sidecar.

**Threats to cover**

- Other local processes occupying/spoofing the stable port or `/healthz`.
- Token theft from argv, environment, logs, debug output, crash reports, files,
  Keychain labels, or control messages.
- Malicious edits to Provider metadata causing arbitrary command execution.
- Auth command shell injection, unbounded stdout/stderr, hanging commands, unsafe
  cwd, relative executable replacement, and token caching.
- SSRF, local/private network targets, DNS rebinding, redirects, URL userinfo,
  path/query injection, and sensitive static headers.
- Oversized request, SSE, tool arguments, response, concurrency, and disk state.
- Symlink/path traversal and permissions on config, store, backup, journal, token,
  sidecar, and helper paths.
- Gateway binding reuse across profiles and token rotation races.
- Prompt/response/reasoning leakage through logs and conformance evidence.
- Sidecar/control protocol version mismatch during application upgrade.

**Required platform decisions**

- State whether MVP is macOS-only or cross-platform.
- Define paths, permissions, process spawning, file locking, atomic replacement,
  Keychain support, and packaging per supported platform.
- Define the security response for unsupported platforms.

**Test design**

- Produce a threat-to-test matrix consumed by RPG-103, RPG-105, RPG-108,
  RPG-109, RPG-301–RPG-308, and RPG-405.
- Define synthetic secret markers used by repository-wide leak tests.

**Acceptance**

- Every listed threat is mitigated, explicitly accepted, or out of scope.
- Sidecar health/control identity cannot be inferred solely from a process
  listening on the configured port.

**Validation evidence**

- The [RPG-004 execution TODO](./todo-rpg-004-threat-platform-boundaries.md) is
  `验证成功`.
- MVP support is fixed to macOS 15.6 / Darwin arm64 with deterministic
  unsupported-platform no-write behavior, private paths/modes, Darwin locking
  and atomic replacement, Keychain-only Gateway tokens, direct argv spawn, and
  signed versioned components.
- Sidecar identity requires same-uid Unix control peer, nonce-HMAC, manifest and
  matching HTTP instance; port or `/healthz` alone is insufficient.
- Every threat has a disposition, implementation owner, named red-first test,
  numeric resource budgets where applicable, and fixed synthetic leak markers.
- Threat/platform scans, Prettier, and `git diff --check` pass.

### RPG-005: Automate and approve the Phase 0 gate

- **Status**: 验证成功
- **Suggested label**: `phase:0`, `area:ci`, `priority:p0`
- **Suggested commit**: `test: enforce remote gateway phase zero gate`
- **Depends on**: RPG-003, RPG-004

**Objective**

Make Phase 0 completion reproducible rather than a verbal approval.

**Implementation tasks**

- Add one command or test target that validates fixture manifests, sanitization,
  schema/version metadata, and expected route coverage.
- Add a contract coverage report listing captured and unsupported behaviors.
- Add a checklist recording the approved Codex version/platform, retry owner,
  store strategy, credential model, state model, and MVP route set.
- Link the approved artifacts from the full design and this todo.

**Test design**

- Corrupt a copied fixture manifest and prove validation fails.
- Insert a synthetic secret marker and prove sanitization fails.
- Remove one mandatory contract case and prove coverage validation fails.

**Acceptance**

- G0 and G1 are explicitly marked passed.
- Phase 1 developers need no undocumented decision to implement the direct
  Provider slice.

**Validation evidence**

- The [RPG-005 execution TODO](./todo-rpg-005-phase-zero-gate.md), versioned
  `remote-provider-gateway-phase0-approval.json`, and generated
  [contract coverage report](./remote-provider-gateway-contract-coverage.md)
  are mutually verified offline.
- `pnpm test:gateway-phase0`: 15/15 Node tests pass, including corrupted
  checksum, synthetic secret, missing case, approval mismatch, and stale report;
  12 scenarios and 15 artifacts verify.
- Focused Rust legacy/Codex contract suites pass 10/10.
- Normal gate execution does not invoke Codex or network.

## Phase 1 issues

### RPG-101: Add cross-process versioned store primitives

- **Status**: 验证成功
- **Suggested label**: `phase:1`, `area:persistence`, `priority:p0`
- **Suggested commit**: `store: add locked versioned atomic persistence`
- **Depends on**: G1

**Objective**

Provide one reusable persistence primitive for Provider, binding, Gateway state,
and journals without coupling domain services to raw JSON or filesystem details.

**Design**

- Define a narrow repository/storage contract with load, snapshot revision,
  compare-and-swap commit, and future-version rejection.
- Protect read/check/write transactions with the Phase 0 approved
  installation-wide cross-process lock.
- Write a same-directory temporary file, flush and fsync it, atomically replace
  the target, then sync the parent directory where supported.
- Reads never write back migrations.
- Errors identify invalid data, future version, lock timeout, revision conflict,
  and I/O failure separately.

**Test design**

- Normal load/commit and missing-store behavior.
- Revision conflict with two snapshots.
- Two concurrent writers: one succeeds and one conflicts; no update is lost.
- Lock timeout and stale lock recovery according to the approved platform rule.
- Serialization/temp-write/fsync/rename fault injection preserves the old file.
- Future schema can be reported but never overwritten.
- Read has no write or mtime side effect.

**Implementation tasks**

- Add focused storage module and tests before Provider migration.
- Inject filesystem/clock/ID operations where needed for deterministic faults.
- Keep domain types out of the generic storage module.

**Acceptance**

- All concurrency and fault-injection tests pass repeatedly.
- Store functions remain small and do not expose global mutable state.

**Validation evidence**

- The [RPG-101 execution TODO](./todo-rpg-101-versioned-store-primitives.md) is
  `验证成功`.
- Generic flattened envelopes, missing-store snapshots, bounded shared/exclusive
  Darwin flock, revision CAS, future-schema and invalid/size/path errors, private
  temp files, fsync + rename + parent sync, and deterministic pre-commit fault
  injection are implemented in `services/storage.rs`.
- Focused storage tests pass 8/8 and pass 10/10 repeated race/fault runs.
- Full Rust suite passes 168 tests with 2 ignored; Phase 0 gate remains 15/15;
  rustfmt and diff checks pass.

### RPG-102: Implement Provider V2 model and legacy migration

- **Status**: 验证成功
- **Suggested label**: `phase:1`, `area:provider`, `priority:p0`
- **Suggested commit**: `provider: add v2 model and legacy migration`
- **Depends on**: RPG-101

**Objective**

Replace the flat Provider model with a typed domain model while preserving
deterministic reads of every RPG-001 legacy fixture.

**Design**

- Use enums/discriminated unions for protocol and adapter.
- Treat Provider IDs as opaque TOML keys; validate allowed syntax and prove
  dotted IDs are safely quoted or reject them.
- Require `default_model` to exist in `models`.
- Canonicalize `base_url` without losing a path prefix.
- Validate adapter path as a relative controlled path with no host, query, or
  parent traversal.
- Keep health, readiness, used-by, and conformance observations out of persisted
  Provider metadata.
- Preserve `created_at` during update and change `updated_at` deterministically.

**Test design**

- Normal Responses and Chat Completions Provider creation.
- Empty/invalid ID, reserved IDs, invalid URL, userinfo, insecure remote HTTP,
  invalid model catalog, invalid adapter/path, and unknown compatibility profile.
- Legacy `openai/responses` mapping, default model expansion, env-key migration,
  adapter alias migration, deterministic output, and no implicit write-back.
- Base/path join cases with trailing slash and preserved prefixes.
- Provider debug/serialization contains no secret value.

**Implementation tasks**

- Separate persistence DTO, domain model, API DTO, and view model.
- Keep migrations pure and version-by-version.
- Add Provider repository operations using RPG-101 revision semantics.

**Acceptance**

- All legacy fixtures migrate deterministically.
- New writes contain only V2 schema and never write legacy `wireApi = openai`.

**Validation evidence**

- The [RPG-102 execution TODO](./todo-rpg-102-provider-v2-migration.md) is
  `验证成功`.
- `provider_v2.rs` defines closed protocol/model/adapter/capability/Codex and
  credential-reference metadata, validation, prefix-preserving URL join, pure V0
  array migration, and revisioned repository CRUD without changing frozen legacy
  Tauri DTOs.
- Focused V2 tests pass 5/5 and legacy contract tests pass 6/6.
- Full Rust suite passes 173 tests with 2 ignored after one transient process-scan
  permission failure was isolated and passed on immediate focused/full rerun.
- Phase 0 gate remains 15/15; rustfmt and diff checks pass.

### RPG-103: Split credential source from upstream authentication and add env resolution

- **Status**: 验证成功
- **Suggested label**: `phase:1`, `area:security`, `priority:p0`
- **Suggested commit**: `auth: separate credential source and transport`
- **Depends on**: RPG-101

**Objective**

Create an extensible authentication model that can represent Bearer,
header-based, and unauthenticated upstreams without exposing the credential.

**Design**

- `CredentialSource`: env, keychain reference, auth command reference, none.
- `UpstreamAuth`: bearer(source), header(name, source), none.
- Header names are validated and sensitive values are never stored in static
  `http_headers` or `query_params`.
- Env resolution returns a secret wrapper usable only inside the request/auth
  boundary; it never returns through DTOs or Debug.
- Missing env is a readiness blocker, not a capability downgrade.
- Direct Codex mapping uses `env_key` or `env_http_headers` as appropriate.

**Test design**

- Bearer env and named-header env normal paths.
- Missing, empty, invalid-name, conflicting, and none-auth paths.
- Unsupported auth scheme for a route returns a structured planner error.
- Serialize/Debug/view/log tests cannot reveal a synthetic secret marker.
- Query/static-header attempts to store a credential are rejected.

**Acceptance**

- Azure/Anthropic-style header auth can be represented without a vendor branch.
- No public function returns a plaintext `String` token outside the narrow secret
  boundary.

**Validation evidence**

- [RPG-103 execution TODO](./todo-rpg-103-credential-auth.md) is `验证成功`.
- Focused credential tests pass 4/4; Provider V2 regression passes 5/5.
- Credential values are non-serializable and Debug-redacted, with closure-only exposure.

### RPG-104: Implement ProfileBindingStore adoption, drift, and profile lifecycle

- **Status**: 验证成功
- **Suggested label**: `phase:1`, `area:binding`, `priority:p0`
- **Suggested commit**: `binding: add authoritative profile provider state`
- **Depends on**: RPG-101, RPG-102

**Objective**

Make profile/provider/model binding authoritative without losing existing
profiles or silently ignoring user-edited Codex config.

**Design**

- Persist one binding per profile with route kind, selected model, Provider
  revision/fingerprint, optional Gateway binding ID, config projection, and
  revision.
- Implement Phase 0 approved adoption from existing config.
- Represent unbound, adopted, managed, drifted, stale-provider, and conflicted
  states explicitly.
- Profile rename updates the binding key/projection safely.
- Profile delete requires detach/revoke before the profile directory is removed.
- Provider edit that invalidates a bound model or changes route/auth follows the
  approved block/rebind/stale policy.
- Used-by queries read only BindingStore after adoption completes.

**Test design**

- Adopt existing built-in/custom/legacy config and repeat idempotently.
- Unknown Provider, malformed config, unsupported auth, and ambiguous ownership.
- Config drift in managed versus unmanaged keys.
- Profile rename/delete success, revision conflict, and rollback.
- Provider model removal/protocol/adapter/base URL/auth changes while in use.
- Concurrent edit returns `PROFILE_BINDING_CONFLICT` with no side effect.

**Acceptance**

- Account/session/provider views use one documented binding/config reconciliation
  result.
- No code scans config or Gateway state to reconstruct used-by after migration.

**Validation evidence**

- [RPG-104 execution TODO](./todo-rpg-104-binding-lifecycle.md) is `验证成功`.
- Focused binding tests pass 4/4 across adoption, drift/stale reconciliation,
  store-only used-by, rename/delete/detach, idempotence and revision conflicts.

### RPG-105: Implement non-destructive CodexConfigEditor and managed projection

- **Status**: 验证成功
- **Suggested label**: `phase:1`, `area:config`, `priority:p0`
- **Suggested commit**: `config: add ownership-safe codex toml editor`
- **Depends on**: RPG-102, RPG-103

**Objective**

Safely project a binding into user-level Codex TOML while preserving all
unmanaged content and comments.

**Design**

- Parse with `toml_edit` and validate with the TOML parser.
- Manage only top-level `model`, `model_provider`, and allowlisted keys in the
  target Provider table.
- Quote Provider IDs through the TOML library.
- Record source file hash for plan concurrency and a typed fingerprint of managed
  values for detach ownership.
- Store previous values only when they are known non-secret; reject ownership of
  pre-existing secret-bearing values.
- Explicitly support `env_key`, `env_http_headers`, non-sensitive headers/query
  params, auth table, retry fields, stream idle timeout, and approved options.
- Create private, collision-free backups but never treat them as rollback
  authority.

**Test design**

- Create config from empty and update a complex commented config.
- Preserve MCP, sandbox, approval, profiles, unknown tables, order where
  supported, and comments.
- Direct and Gateway Provider projections always write `wire_api = responses`.
- Conflicting auth modes and sensitive static header/query values are rejected.
- Same Provider ID with dots/hyphens is correctly represented.
- Concurrent hash conflict, parse failure, temp-write failure, and repeat attach.
- Detach restores only owned values; managed drift conflicts; unmanaged additions
  survive.
- Backup names cannot collide within the same timestamp.

**Acceptance**

- No test performs whole-file expected replacement except initial empty creation.
- The current string-building attach implementation is no longer on the V2 path.

**Validation evidence**

- [RPG-105 execution TODO](./todo-rpg-105-codex-config-editor.md) is `验证成功`.
- Focused editor tests pass 4/4 across comment preservation, dotted IDs,
  direct/Gateway auth and retry projection, options, hash/fault conflicts,
  ownership-safe detach, unmanaged additions and unique private backups.
- Batch regression: full Rust suite passes 181 tests with 2 ignored; frontend
  passes 132/132; Phase 0 gate passes 15/15; lint, build and format/diff checks pass.

### RPG-106: Implement route/attach planners and expiring dry-run fingerprints

- **Status**: 验证成功
- **Suggested label**: `phase:1`, `area:planner`, `priority:p0`
- **Suggested commit**: `provider: add pure route and attach planning`
- **Depends on**: RPG-102, RPG-103, RPG-104, RPG-105

**Objective**

Make routing and attach behavior pure, explicit, reusable by API/UI/tests, and
safe against executing a stale preview.

**Design**

- `ProviderRoutePlan` includes route kind, upstream endpoint, protocol, adapter,
  model, auth requirements, effective static capabilities, blockers, and warnings.
- `ProfileAttachPlan` includes expected Provider/binding/store revisions, source
  config hash, Gateway endpoint version when relevant, config patch, credential
  projection, journal operations, and redacted preview.
- Chat Completions without adapter returns a plan blocker for attach while still
  allowing a separate upstream-test plan.
- Dry-run fingerprints include every state input and expire after the approved
  TTL.
- Planner reads no secret value and performs no I/O.

**Test design**

- Responses direct, Chat Completions Gateway, Chat Completions missing adapter.
- Invalid model, adapter registry mismatch, unsupported credential route, drift,
  missing secret, and Gateway-unavailable context.
- Fingerprint changes on Provider/model/binding/config/Gateway/options changes.
- Expired/replayed/tampered plan is rejected.
- Redacted preview contains no secret or unsafe command output.

**Acceptance**

- API, UI, executor, and relay consume the same plan types.
- No executor recomputes hidden defaults after plan validation.

**Validation evidence**

- [RPG-106 execution TODO](./todo-rpg-106-route-attach-planner.md) is `验证成功`.
- Focused planner tests pass 4/4 for direct/Gateway routing, typed blockers,
  state-complete fingerprints, redacted preview, TTL, replay, tamper and stale
  execution state.

### RPG-107: Implement journaled attach, rebind, detach, and crash recovery

- **Status**: 验证成功
- **Suggested label**: `phase:1`, `area:transaction`, `priority:p0`
- **Suggested commit**: `binding: add journaled attach lifecycle`
- **Depends on**: RPG-106

**Objective**

Coordinate binding, config projection, and optional Gateway-binding preparation
with deterministic recovery at every crash/failure point.

**Design**

- Acquire locks in the Phase 0 approved order.
- Revalidate dry-run fingerprint and revisions under lock.
- Persist journal before the first externally visible mutation.
- Use explicit prepared/config-applied/binding-committed/finalized or approved
  equivalent states.
- Preserve the old binding/token through failed rebind.
- Detach is idempotent when already absent.
- Recovery uses journal plus managed-value fingerprint, never blind backup restore.
- Startup/launcher invokes bounded recovery before using the binding.

**Test design**

- Success and idempotent repeat for attach, rebind, detach.
- Revision, config hash, managed ownership, and stale-plan conflicts.
- Fault injection before/after every journal, config, binding, and token step.
- Process-restart recovery from every persisted journal state.
- Rebind failure keeps the old route usable; success revokes the old projection.
- Detach conflict makes no mutation.
- Journal retention and corrupt-journal behavior.

**Acceptance**

- Every injected failure ends in a documented recoverable state.
- No state leaves a usable new token without an authoritative binding or a
  binding claiming a config projection that was not applied.

**Validation evidence**

- [RPG-107 execution TODO](./todo-rpg-107-journaled-attach-recovery.md) is
  `验证成功`.
- Focused transaction suite passes 19/19 across journal validation/retention,
  one-lock attach/rebind/detach, dry-run/revision/hash conflicts, every durable
  fault boundary, Gateway reference compensation, restart rollback/roll-forward,
  manual intervention, recovery bounds, and startup recovery.
- Batch regression: full Rust suite passes 212 tests with 2 ignored; frontend
  passes 132/132; Phase 0 gate passes 15/15; lint and build pass.

### RPG-108: Implement versioned Keychain credential lifecycle

- **Status**: 验证成功
- **Suggested label**: `phase:1`, `area:keychain`, `priority:p1`
- **Suggested commit**: `auth: add versioned keychain credentials`
- **Depends on**: RPG-103, RPG-107

**Objective**

Store and rotate Provider/Gateway credentials without exposing values or making
compensation delete an older valid secret.

**Design**

- Use stable service plus generated credential ID/version, not Provider ID alone.
- Create new → verify reference → commit metadata → retire old.
- On metadata conflict, delete only the newly created identifiable version.
- Keychain access is behind a trait with a fake implementation for tests.
- Views expose kind and reference metadata only.

**Test design**

- Create/read-use/rotate/revoke through the secret boundary.
- Empty write, unavailable Keychain, permission denial, missing item.
- Metadata CAS conflict preserves old credential and removes only new version.
- Debug/serialization/error/log output leak tests.
- Unsupported platform behavior follows RPG-004.

**Acceptance**

- No real Keychain is required by automated tests.
- A failed update cannot destroy the credential used by the active binding.

**Validation evidence**

- [RPG-108 execution TODO](./todo-rpg-108-versioned-keychain.md) is `验证成功`.
- Focused Keychain tests pass 8/8 across fake-backed read-use/revoke, redacted
  platform/backend failures, monotonic version rotation, exact CAS compensation,
  named-header preservation, cleanup pending, and official Codex helper config.
- The production macOS backend uses Security.framework without placing the
  credential in process argv; automated tests never touch the real Keychain.
- Batch regression: full Rust suite passes 220 tests with 2 ignored; frontend
  passes 132/132; Phase 0 gate passes 15/15; lint and build pass.

### RPG-109: Implement restricted auth command and direct-provider auth helper

- **Status**: 验证成功
- **Suggested label**: `phase:1`, `area:auth-helper`, `priority:p1`
- **Suggested commit**: `auth: add bounded command credential helper`
- **Depends on**: RPG-103, RPG-105

**Objective**

Support command-backed credentials and Keychain-to-Codex token output through a
bounded, non-shell execution boundary.

**Design**

- Command and args are structured; no shell evaluation.
- Validate executable policy, cwd, timeout, output limit, exit code, stdout
  token shape, stderr redaction, cache, refresh, and retry semantics.
- Helper writes only the token plus newline to stdout; diagnostics go to
  sanitized stderr or structured logs according to the threat model.
- Config auth table never coexists with `env_key`,
  `experimental_bearer_token`, or `requires_openai_auth`.

**Test design**

- Successful token, surrounding whitespace, empty token, extra lines, oversized
  output, timeout, nonzero exit, missing executable, unsafe path/cwd.
- Arguments containing shell metacharacters are passed literally.
- Token never appears in Debug/error/log/frontend DTO.
- Auth refresh/retry behavior matches RPG-002 fixtures.

**Acceptance**

- Direct Keychain and auth-command Responses Providers can produce a valid
  official Codex auth table.
- All subprocesses are reaped on success, error, and timeout.

**Validation evidence**

- [RPG-109 execution TODO](./todo-rpg-109-auth-command-helper.md) is `验证成功`.
- Focused auth command tests pass 4/4; the runner verifies executable approval,
  bypasses shells, bounds/drains output, kills the timeout process group, reaps
  children, caches/refreshes redacted secrets, and emits official Codex auth
  configuration.
- Batch regression: full Rust suite passes 193 tests with 2 ignored; frontend
  passes 132/132; Phase 0 gate passes 15/15; lint and build pass.

### RPG-110: Introduce V2 Tauri DTOs and compatibility commands

- **Status**: 验证成功
- **Suggested label**: `phase:1`, `area:api`, `priority:p0`
- **Suggested commit**: `api: expose provider v2 planning contracts`
- **Depends on**: RPG-102, RPG-103, RPG-104, RPG-105, RPG-106, RPG-107, RPG-108, RPG-109

**Objective**

Keep persistence/domain types private while exposing versioned, redacted,
concurrency-aware Tauri APIs.

**Required DTOs**

- `LegacyCreateProviderRequest` accepted only during the migration window.
- `CreateProviderRequestV2` and `UpdateProviderRequestV2`.
- `ProviderProfileView` and `ProviderReadinessView`.
- `ProfileProviderBindingView`.
- `PlanAttachRequest` and `ProfileAttachPlanView`.
- `ExecuteAttachRequest` carrying fingerprint and expected versions.
- Rebind/detach requests carrying expected binding revision.
- Structured error DTO with stable codes and recovery actions.

**Test design**

- Golden camelCase serialization/deserialization for every DTO.
- Close the RPG-001 legacy env secret casing gap: accept frontend
  `secret.envKey` at the compatibility boundary and never expose
  `secret.env_key` in a V2 frontend DTO.
- Legacy `openai` input maps to Responses with a migration warning.
- Unknown protocol/adapter/auth values are rejected.
- Views never serialize credential values, previous sensitive config, token hash,
  local private paths not intended for display, or command output.
- Stale revisions/fingerprints return stable conflict errors.

**Implementation tasks**

- Keep commands thin; run blocking file/Keychain work off the async UI thread.
- Update command registration and TypeScript API wrappers in the same issue.
- Do not remove legacy fields until fixture compatibility is proven.

**Acceptance**

- Rust and TypeScript contract fixtures match.
- No frontend code imports persistence/domain structs by shape.

**Validation evidence**

- [RPG-110 execution TODO](./todo-rpg-110-v2-tauri-api.md) is `验证成功`.
- Rust V2 API tests pass 9/9 for camelCase goldens, closed enums, legacy
  `envKey/openai` compatibility, redacted views/errors, revision-aware CRUD,
  Keychain rotation, plan/execute/replay/stale/detach, and command registration.
- TypeScript V2 wrapper tests pass 5/5; legacy wrappers remain registered, and
  no RPG-111 UI component was changed.
- Batch regression: full Rust passes 229 tests with 2 ignored; frontend passes
  137/137; Phase 0 passes 15/15; lint and build pass.

### RPG-111: Deliver the minimal Responses Provider frontend flow

- **Status**: 验证成功
- **Suggested label**: `phase:1`, `area:frontend`, `priority:p0`
- **Suggested commit**: `provider-ui: add responses direct attach flow`
- **Depends on**: RPG-110

**Objective**

Ship the smallest usable UI for the direct Responses vertical slice without
premature Gateway/capability complexity.

**Scope**

- V2 Provider create/edit form for protocol, URL, models/default model,
  credential source/auth strategy, and supported Codex options.
- Provider list/card with protocol, selected/default model, credential reference,
  binding count, readiness blockers, and upstream-test action.
- Attach modal that always performs dry-run first and shows redacted config,
  operations, warnings, blockers, backup path, and plan expiry.
- Execute, rebind, and detach actions with revision conflict recovery.
- Move Provider modal/card logic out of `App.tsx` into focused components.

**Test design**

- Normal create/edit/list and direct attach/detach.
- Default model validation, missing credential, invalid URL/options, server error.
- Dry-run blockers disable execute.
- Stale plan/conflict forces refresh and a new preview.
- Secret input is cleared after submission and never reappears from a view DTO.
- Existing account/session/provider stores refresh after mutation.

**Acceptance**

- Components have focused test files.
- No Chat Completions Provider is presented as attachable before Phase 3.
- Frontend test, build, lint, and format checks pass.

**Evidence (2026-07-13)**

- The Provider route now uses focused V2 Provider Center/editor/binding
  components; legacy Provider forms were removed from App.tsx.
- Store and component coverage passes 25/25 for revision-aware V2 refresh,
  create/edit, write-only Keychain input, Responses-only attachment, preview
  blockers/expiry, attach/rebind/detach, and conflict recovery.
- V2 API/upstream validation coverage passes 11/11 Rust and 7/7 TypeScript;
  upstream validation returns redacted direct-route metadata and rejects
  Chat Completions/Gateway routes.
- New Keychain Provider creation is atomic: Keychain write precedes metadata,
  store failure compensates the new item, and the secret remains input-only.
  V2 views round-trip adapter and Codex options so edits are non-destructive.
- Full frontend passes 164/164; lint, build, and Prettier checks pass.
- Weekly quota popover components and styles were not modified.

### RPG-112: Pass the Responses Provider vertical-slice gate

- **Status**: 验证成功
- **Suggested label**: `phase:1`, `area:acceptance`, `priority:p0`
- **Suggested commit**: `provider: complete responses direct slice`
- **Depends on**: RPG-107, RPG-110, RPG-111

**Objective**

Prove Phase 1 is a usable feature, not only an abstraction layer.

**End-to-end scenarios**

- Create an env-backed Responses Provider against a local mock.
- Test upstream without recording body/secret.
- Dry-run and attach to an existing profile containing complex user config.
- Verify official Codex Provider TOML and preserved unmanaged content.
- Exercise the contract-tested direct route using the pinned Codex version or
  approved offline harness.
- Rebind model, detect stale preview, detect config drift, and detach.
- Migrate a legacy Provider/config/profile and repeat the flow.
- Repeat with Keychain and auth-command credentials; all three supported
  credential paths must pass before G2.

**Test design**

- Local mock only; no real Provider in automated tests.
- Include all normal, invalid, error, state/conflict, migration, and security
  paths from RPG-102–RPG-111.
- Run source-file-to-test mapping audit.

**Acceptance**

- G2 is marked passed.
- No Responses direct path starts Gateway.
- Full Rust/frontend/build/lint/format suites pass.

**Evidence (2026-07-13)**

- The offline G2 harness passes 5/5: local mock GET /v1/models and POST
  /v1/responses against the pinned route manifest; env create/validate/attach/
  rebind/stale/detach; legacy migration; Keychain and approved auth-command
  direct projections; secret/body non-recording and persisted-secret scans; and
  Phase 1 source-to-test mapping.
- All direct plans assert RouteKind::Direct and contain no Gateway operation;
  the direct credential mappings emit official Codex auth references without
  serializing token values.
- Full Rust all-targets passes 236 tests with 2 ignored. Full frontend passes
  164/164. Phase 0 passes 15/15 with 12 scenarios and 15 artifacts.
- Frontend lint/build/Prettier, Rust format, fixture checksums/sanitization, and
  git diff checks pass.

## Phase 2 issues

Execution contracts: [`RPG-201`](./todo-rpg-201-controlled-protocol-schemas.md),
[`RPG-202`](./todo-rpg-202-adapter-registry-exchange.md),
[`RPG-203`](./todo-rpg-203-request-translation.md),
[`RPG-204`](./todo-rpg-204-nonstream-response.md),
[`RPG-205`](./todo-rpg-205-bounded-sse-state-machine.md),
[`RPG-206`](./todo-rpg-206-function-tool-roundtrip.md),
[`RPG-207`](./todo-rpg-207-terminal-normalization.md), and
[`RPG-208`](./todo-rpg-208-deepseek-g3.md).

### RPG-201: Define the controlled Responses and Chat Completions schemas

- **Status**: 验证成功
- **Suggested label**: `phase:2`, `area:adapter`, `priority:p0`
- **Suggested commit**: `adapter: define verified protocol schemas`
- **Depends on**: G2, RPG-002

**Objective**

Define only the protocol subset proven necessary by contract fixtures, while
retaining unknown-field detection and precise unsupported errors.

**Design**

- Keep Responses request, response, output item, stream event, tool, usage, and
  error types in the adapter boundary.
- Keep Chat Completions request, response, stream chunk, message, tool, usage, and
  error types in its protocol module.
- Do not create one global vendor-neutral request with hundreds of optional
  fields.
- Preserve enough raw field identity to report an unsupported field path.
- Treat IDs, roles, content parts, nullable fields, and absent versus empty
  values according to fixtures.

**Test design**

- Deserialize every RPG-002 request/response/event fixture.
- Serialize canonical synthetic equivalents and compare semantic JSON.
- Reject wrong types, missing required fields, duplicate/inconsistent IDs,
  unsupported input/tool types, and oversized nested values.
- Detect unknown fields according to the approved reject/preserve matrix.
- Ensure schema Debug output contains no synthetic secret or raw prompt beyond
  fixed fixture text.

**Acceptance**

- Schema types can represent every supported fixture without `serde_json::Value`
  leaking into core business services.
- Unsupported fixtures fail with stable field/item-specific errors.

### RPG-202: Implement object-safe adapter registry and exchange contracts

- **Status**: 验证成功
- **Suggested label**: `phase:2`, `area:adapter`, `priority:p0`
- **Suggested commit**: `adapter: add registry and per-request exchange`
- **Depends on**: RPG-201

**Objective**

Provide an extensible, thread-safe adapter seam that models both non-stream and
stream output without coupling conversion logic to HTTP.

**Design**

- `ProtocolAdapter: Send + Sync` is immutable and registry-owned.
- `AdapterExchange: Send` is created per request and owns IDs, state, partial tool
  arguments, and terminal status.
- Model upstream request plus an output enum or separate APIs for non-stream
  `ResponsesResponse` and streaming `ResponsesStreamEvent`.
- Inject deterministic ID/time factories for tests.
- Registry validates adapter ID, source protocol, target protocol, version, and
  compatibility-policy compatibility.
- Adapter never resolves credentials, sends HTTP, retries, logs bodies, or owns
  the network cancellation handle.

**Test design**

- Registry success, missing ID, duplicate ID, and protocol mismatch.
- Concurrent exchanges never share IDs, fragments, or state.
- Invalid lifecycle calls such as finish twice, consume after terminal, and
  cancel after completion return stable results.
- Non-stream output cannot be accidentally emitted as an incomplete stream.

**Acceptance**

- A fake adapter can be registered and used from concurrent Tokio tasks.
- No Gateway or reqwest dependency is required by adapter unit tests.

### RPG-203: Translate Responses requests and reject unsupported input

- **Status**: 验证成功
- **Suggested label**: `phase:2`, `area:adapter-request`, `priority:p0`
- **Suggested commit**: `adapter: translate responses requests`
- **Depends on**: RPG-202

**Objective**

Translate the verified Responses request subset into Chat Completions without
silently losing context or changing Provider/model routing.

**Mappings**

- String input and supported user/developer/system message items.
- Assistant output messages replayed as history when fixtures require them.
- Model from the authoritative binding; request body cannot select another
  Provider, host, or unapproved model.
- Instructions and role ordering according to the approved policy.
- Supported text controls, max output tokens, tool choice, parallel tool setting,
  and structured output only when verified.
- Stream flag and upstream stream-options needed for usage.
- Function calls and outputs are delegated to RPG-206.

**Test design**

- Normal string and multi-message input.
- Empty input, conflicting instructions, invalid role order, duplicate call IDs,
  and model mismatch.
- Image/audio/file, hosted tool, MCP, computer-use, and unknown item errors.
- `previous_response_id` behavior exactly matches RPG-002 decision.
- Parameters unsupported by target Provider are rejected or warned according to
  the typed compatibility policy.

**Acceptance**

- Every request field in the supported schema has a tested mapping or explicit
  rejection.
- No mapping branch compares a concrete Provider name.

### RPG-204: Translate non-stream Chat Completions responses

- **Status**: 验证成功
- **Suggested label**: `phase:2`, `area:adapter-response`, `priority:p0`
- **Suggested commit**: `adapter: translate nonstream chat responses`
- **Depends on**: RPG-203

**Objective**

Produce a complete Responses-compatible JSON response for non-stream requests.

**Design**

- Map one supported choice to typed output items and terminal status.
- Generate stable response/item/call IDs through the injected generator.
- Preserve text ordering and finish reason semantics.
- Multiple choices are rejected unless the approved contract defines a
  deterministic single-choice rule.
- Empty content, content filter, length, tool-only, and Provider-specific finish
  reasons have explicit outcomes.
- Usage/error mapping delegates shared parts to RPG-207.

**Test design**

- Text success, empty string, null content, length, content-filter, and tool-only
  response.
- Zero/multiple choices, malformed objects, inconsistent model/ID, and unknown
  finish reason.
- Response status/output ordering matches Codex contract fixtures.
- Repeated conversion with fixed ID factory is deterministic.

**Acceptance**

- Non-stream conversion returns a `ResponsesResponse`, not a collection of stream
  events that the Gateway must reconstruct heuristically.

### RPG-205: Implement the bounded SSE conversion state machine

- **Status**: 验证成功
- **Suggested label**: `phase:2`, `area:adapter-stream`, `priority:p0`
- **Suggested commit**: `adapter: add bounded responses stream state machine`
- **Depends on**: RPG-203

**Objective**

Translate Chat Completions SSE chunks into the exact verified Responses event
sequence with bounded memory and deterministic terminal behavior.

**State requirements**

- created → output item added → content/tool deltas → part/item done →
  completed/failed/cancelled.
- Stable response/item/call IDs across all events.
- Incremental UTF-8/content/tool-argument fragment handling.
- Explicit finish reason and terminal usage behavior.
- At most one terminal event.
- No events after failure/cancel.

**Test design**

- Text stream fixture event-for-event.
- Arbitrary chunk boundaries, split UTF-8, empty deltas, role-only first chunk,
  usage-only final chunk, and `[DONE]`.
- Malformed SSE field, malformed JSON, oversized frame, missing DONE, connection
  drop, upstream error after partial output, and duplicate terminal.
- Slow consumer/backpressure model and buffer overflow outcome.
- Cancellation in every legal state.
- Property-style chunk partition tests produce the same semantic output.

**Implementation tasks**

- Keep SSE framing separate from protocol conversion.
- Use a bounded accumulator for tool arguments and content metadata.
- Do not skip malformed frames and continue a corrupted response.

**Acceptance**

- Stream fixtures match RPG-002 ordering exactly.
- Memory use is bounded by named constants covered by limit tests.

### RPG-206: Implement function tool-call round trips

- **Status**: 验证成功
- **Suggested label**: `phase:2`, `area:adapter-tools`, `priority:p0`
- **Suggested commit**: `adapter: add function tool roundtrip`
- **Depends on**: RPG-204, RPG-205

**Objective**

Translate function definitions, calls, streamed arguments, and tool results while
preserving `call_id` identity across turns.

**Design**

- Responses function tool → Chat Completions function tool.
- Tool choice and supported parallel-call semantics.
- Non-stream/stream Chat tool calls → Responses function-call output items.
- Responses `function_call_output` → Chat `role = tool` with matching
  `tool_call_id`.
- Validate tool names, schema size, arguments size, and call/result linkage.
- Do not execute tools inside the adapter.

**Test design**

- One tool call round trip, multiple calls when supported, and tool-only response.
- Arguments split across arbitrary stream chunks and interleaved call indexes.
- Missing/duplicate/unknown call ID, result before call, malformed JSON arguments,
  oversized arguments, and unsupported tool type.
- Subsequent request history matches contract fixtures exactly.

**Acceptance**

- A full synthetic tool loop passes for non-stream and stream paths.
- Hosted/MCP/computer tools fail before any upstream request is produced.

### RPG-207: Normalize usage, errors, cancellation, and retry classification

- **Status**: 验证成功
- **Suggested label**: `phase:2`, `area:adapter-error`, `priority:p0`
- **Suggested commit**: `adapter: normalize terminal metadata and errors`
- **Depends on**: RPG-204, RPG-205

**Objective**

Give Gateway/UI one stable, redacted error and usage model without deciding HTTP
retry inside the adapter.

**Design**

- Map input/output/total tokens and known details; unavailable detail stays
  unknown, never fabricated.
- Classify validation, auth, capability, upstream, timeout, cancelled, and
  internal errors.
- Preserve upstream HTTP status only; do not expose raw upstream body.
- Calculate retryable classification according to the Phase 0 policy.
- Distinguish client cancellation, upstream disconnect, timeout, malformed
  response, and buffer overflow.
- Streaming failures emit one verified failure event after any already-emitted
  deltas.

**Test design**

- Full/partial/missing usage and inconsistent totals.
- Upstream 400/401/403/408/409/429/5xx plus malformed error bodies.
- Retry-After date/seconds parsing where owned by the shared error layer.
- Timeout/cancel/disconnect before and after first byte.
- Error message/log serialization leak tests.

**Acceptance**

- Every stable error code has a UI recovery-action mapping.
- Adapter output contains no decision to replay an HTTP generation request.

### RPG-208: Implement DeepSeek thinking compatibility and pass the adapter gate

- **Status**: 验证成功
- **Suggested label**: `phase:2`, `area:deepseek`, `priority:p0`
- **Suggested commit**: `adapter: add deepseek thinking compatibility`
- **Depends on**: RPG-206, RPG-207

**Objective**

Implement DeepSeek-specific request, response, streaming, and history rules as a
typed compatibility policy while keeping the generic adapter vendor-neutral.

**Design**

- Map approved Responses reasoning settings to DeepSeek `thinking.type` and
  `reasoning_effort`.
- Handle DeepSeek parameters ignored or unsupported in thinking mode according
  to the documented policy.
- Read non-stream and stream `reasoning_content` separately from normal content.
- Preserve complete reasoning content for every assistant tool-call turn in all
  subsequent requests.
- Allow omission of prior non-tool reasoning only where official fixtures permit.
- Never concatenate reasoning into normal assistant content.
- Follow the Phase 0 decision for exposing/suppressing Responses reasoning events.
- Keep model names in the preset and user-editable model catalog.

**Test design**

- Official/sanitized normal and streaming thinking fixtures.
- Thinking enabled/disabled and supported effort values.
- Tool call → tool result → second tool call → final response across multiple user
  turns.
- Omitted required reasoning produces a preflight error in tests rather than an
  upstream 400.
- Non-tool turn history policy.
- Unsupported reasoning signature/encrypted fields are never fabricated.
- Generic OpenAI-compatible tests prove no DeepSeek policy leaks into them.

**Gate acceptance**

- All Phase 2 fixtures pass offline.
- Text non-stream, text stream, function tools, usage, errors, and DeepSeek
  thinking/history are complete.
- G3 is marked passed.

## Phase 3 issues

### RPG-301: Implement Gateway binding credentials and token lifecycle

- **Status**: 验证成功
- **Suggested label**: `phase:3`, `area:gateway-auth`, `priority:p0`
- **Suggested commit**: `gateway: add profile token bindings`
- **Depends on**: G3, RPG-104, RPG-108

**Objective**

Give each attached Gateway profile a separately revocable local bearer token and
an immutable request-start routing snapshot.

**Design**

- Gateway binding stores binding/profile/Provider/model IDs, token hash,
  credential reference, creation/revocation metadata, and schema revision.
- Auth helper reads the token through the credential reference; Gateway stores no
  plaintext token.
- Generate cryptographically strong tokens and compare verified hashes in
  constant time.
- Rotate by creating new credential/binding, committing projection, then revoking
  old according to RPG-107.
- Snapshot binding, Provider, compatibility policy, model, and credential
  reference at request start.

**Test design**

- Create/authenticate/rotate/revoke/detach per profile.
- Wrong, empty, malformed, expired/revoked, and cross-profile token.
- Rotation race and in-flight request snapshot behavior.
- Hash/reference store and logs contain no plaintext token.
- Auth helper cache/refresh behavior matches Codex fixtures.

**Acceptance**

- Detach immediately rejects new requests with the old token.
- Editing a Provider affects only new request snapshots according to the approved
  mutation policy.

### RPG-302: Build the authenticated loopback server and verifiable health endpoint

- **Status**: 验证成功
- **Suggested label**: `phase:3`, `area:gateway-server`, `priority:p0`
- **Suggested commit**: `gateway: add loopback server foundation`
- **Depends on**: RPG-301

**Objective**

Create a small HTTP composition root with authentication, limits, safe request
IDs, sanitized logging, and an instance-verifiable readiness endpoint.

**Design**

- Bind only IPv4 loopback unless the approved platform contract adds IPv6.
- `/healthz` exposes version/readiness and proves the expected installation
  instance through the approved nonce/control mechanism.
- `/v1/responses` requires bearer authentication.
- Auth, request limits, logging, timeout, binding lookup, and handlers are
  separate middleware/components.
- CORS is disabled.
- Unknown routes/methods return stable sanitized errors.

**Test design**

- Loopback binding and failure on non-loopback configuration.
- Health identity success, foreign/spoof response, version mismatch, and not-ready.
- Missing/invalid/revoked auth.
- Request body/header/concurrency/time limits.
- Request IDs reject control characters and are unique.
- Captured logs omit body, prompt, response, tool args, auth, and token.

**Acceptance**

- Server can run entirely against fake binding/upstream services.
- A process merely occupying the configured port is never accepted as LAM
  Gateway.

### RPG-303: Build the bounded upstream client and authentication injection

- **Status**: 验证成功
- **Suggested label**: `phase:3`, `area:gateway-client`, `priority:p0`
- **Suggested commit**: `gateway: add secure upstream client`
- **Depends on**: RPG-103, RPG-302

**Objective**

Own all HTTP, credential injection, redirects, deadlines, cancellation, and
approved retry behavior outside the adapter.

**Design**

- Compose normalized base URL and controlled path without dropping prefixes.
- Request body/model/header cannot change host or route binding.
- Resolve credential immediately before request and inject bearer or named header.
- Disable redirects by default; if enabled, validate every hop.
- Apply DNS/private-network policy from RPG-004.
- Use total deadline, connect timeout, read/stream idle timeout, bounded pool, and
  bounded concurrency.
- Propagate client cancellation to the upstream request.
- Apply the single-owner retry budget and record attempt count without body.

**Test design**

- Bearer/header/no-auth requests against wiremock.
- URL prefix/trailing slash and injection cases.
- Redirect, DNS/private target policy, TLS/error, connect/read/idle timeout.
- Cancellation before connect, during body, and during stream.
- Retry only in the exact approved pre-delivery case; no retry after any byte.
- 429/5xx attempt-count tests with Codex outer retry settings.
- Secret/log/header redaction.

**Acceptance**

- No handler or adapter constructs reqwest requests directly.
- Maximum combined attempt count is proven by an integration test.

### RPG-304: Compose `/v1/responses` non-stream and streaming routes

- **Status**: 验证成功
- **Suggested label**: `phase:3`, `area:gateway-route`, `priority:p0`
- **Suggested commit**: `gateway: serve verified responses subset`
- **Depends on**: RPG-208, RPG-303

**Objective**

Compose auth, immutable binding snapshot, schema validation, adapter exchange,
upstream client, and wire response into the contract-tested Gateway endpoint.

**Design**

- Route selects Provider solely from authenticated binding.
- Validate body/schema/limits before credential resolution and upstream send.
- Non-stream returns the complete verified Responses JSON shape.
- Stream returns exact SSE event framing/order and flush behavior.
- Client disconnect cancels upstream and adapter state.
- Add only RPG-002-proven model/cancel/state routes.
- Unsupported fields/items return capability/validation errors without upstream
  traffic.

**Test design**

- Text non-stream/stream and function tool round trip through wiremock.
- Wrong body model cannot switch Provider/model.
- Unsupported input/tool/field and `previous_response_id` policy.
- Upstream malformed JSON/SSE, early/late error, timeout, cancel, overflow.
- Correct status/content type/cache headers and no body logging.
- Request-start snapshot survives concurrent Provider edit.

**Acceptance**

- Gateway wire output matches Codex contract fixtures byte/semantic-event
  expectations as appropriate.

### RPG-305: Implement sidecar state, stable port, supervisor, and private control channel

- **Status**: 验证成功
- **Suggested label**: `phase:3`, `area:sidecar`, `priority:p0`
- **Suggested commit**: `gateway: add sidecar lifecycle supervisor`
- **Depends on**: RPG-302, RPG-304

**Objective**

Keep Gateway available independently of the Tauri window while enforcing one
installation instance, a stable port, bounded restart, and authenticated control.

**Design**

- Versioned private Gateway state with port, instance identity, process/version,
  and install-manifest compatibility.
- Installation-wide single-instance lock acquired before bind.
- Explicit port-change dry-run/reprojection operation.
- Private authenticated control channel or IPC; never unauthenticated control on
  a public/guessable endpoint.
- Supervisor readiness, bounded exponential restart with jitter, idle shutdown,
  and no UI-close shutdown while bindings exist.
- Stale PID/lock/state recovery according to platform rules.

**Test design**

- First start, reuse running instance, two concurrent starts, stale lock/PID.
- Foreign process on stable port and spoofed health.
- Version mismatch and upgrade/restart.
- Port change across multiple profiles with rollback on one config failure.
- Crash/restart, restart limit, idle exit, in-flight request prevents exit.
- UI/supervisor exit does not kill a needed sidecar.

**Acceptance**

- There is exactly one authoritative running sidecar per installation.
- Port changes never happen silently.

### RPG-306: Implement launcher, auth helper, install manifest, and application packaging

- **Status**: 验证成功
- **Suggested label**: `phase:3`, `area:packaging`, `priority:p0`
- **Suggested commit**: `launcher: package gateway and profile runner`
- **Depends on**: RPG-301, RPG-305

**Objective**

Ship resolvable, version-compatible binaries for `lam codex`, Gateway, and the
token helper in development and packaged installations.

**Design**

- Versioned install manifest names executable paths, versions, hashes/identity,
  state schema compatibility, and supported platform.
- `lam codex --profile <id> [-- <args>]` validates/recover binding projection,
  ensures Gateway only for Gateway routes, sets exact `CODEX_HOME`, preserves
  user args, signals, stdio, cwd, and exit code.
- Auth helper retrieves only the requested binding token and prints only token.
- Tauri bundle includes and resolves sidecar/helper without development paths.
- Upgrade handles running old sidecar and schema compatibility explicitly.

**Test design**

- Argument/exit/signal/stdin/stdout/stderr propagation with fake Codex.
- Direct route does not start Gateway; Gateway route starts/reuses it.
- Missing/drifted/stale binding and failed recovery.
- Manifest missing/tampered/version mismatch.
- Packaged path resolution without source-tree assumptions.
- Auth helper wrong binding/revoked credential/output leak.

**Acceptance**

- Development and packaged smoke tests resolve the same logical manifest.
- No token appears in argv, config, wrapper, or manifest.

### RPG-307: Route all supported Codex entry points through one launch planner

- **Status**: 验证成功
- **Suggested label**: `phase:3`, `area:launch`, `priority:p0`
- **Suggested commit**: `launch: unify profile codex entry points`
- **Depends on**: RPG-306

**Objective**

Prevent resume/relay/wrapper/UI paths from bypassing Gateway recovery after a
restart.

**Scope**

- Introduce `CodexLaunchPlanner`/command specification used by normal launch,
  resume, exec-resume, relay handoff, generated wrappers, copy-command actions,
  and terminal targets.
- Direct profiles may still launch Codex directly through the same planner
  decision.
- Gateway profiles always use the launcher.
- Login behavior remains separate and does not copy auth.

**Test design**

- Every existing command-builder test for direct profiles remains compatible.
- Gateway profile commands use `lam codex --profile` and preserve session/cwd/args.
- Shell escaping remains safe; frontend cannot inject arbitrary shell.
- System restart with stopped Gateway followed by each supported entry point.
- Unsupported bypass path displays a precise recovery instruction.

**Acceptance**

- No supported product action constructs raw `CODEX_HOME=... codex` independently.
- Existing relay/resume behavior stays green for non-Gateway profiles.

### RPG-308: Pass Gateway recovery, security, retry, and mock DeepSeek gates

- **Status**: 验证成功
- **Suggested label**: `phase:3`, `area:acceptance`, `priority:p0`
- **Suggested commit**: `gateway: complete deepseek runtime slice`
- **Depends on**: RPG-304, RPG-305, RPG-306, RPG-307

**Objective**

Prove the complete adapter profile works through the packaged runtime boundary
and remains correct under faults.

**End-to-end scenarios**

- Create DeepSeek-style Chat Completions Provider with env credential.
- Dry-run/attach prepares binding, token, config, and launcher projection.
- Launcher starts/reuses sidecar and fake Codex sends non-stream/stream/tool
  contract requests.
- Gateway translates through wiremock DeepSeek thinking fixtures.
- UI process closes while sidecar continues.
- Sidecar crash/system-restart simulation followed by launcher recovery.
- Rebind, rotate, drift conflict, detach, and old-token rejection.
- Port occupied by foreign server and explicit port migration rollback.

**Security/failure tests**

- Repository-wide secret marker scan across stores/config/logs/sessions/wrappers/
  manifests/journals/frontend serialization.
- Request/body/tool/reasoning log capture.
- Limits, redirect, auth, control-channel, spoofed-health, and path attacks.
- Failure injection at every attach and recovery journal step.
- Combined Codex/Gateway upstream attempt budget.

**Acceptance**

- G4 is marked passed.
- Phase 0–3 automated suites are green.
- The project may claim a DeepSeek-compatible MVP only for the verified matrix.

## Phase 4 issues

### RPG-401: Implement readiness, health, capability resolution, and conformance evidence

- **Status**: 验证成功
- **Suggested label**: `phase:4`, `area:capability`, `priority:p1`
- **Suggested commit**: `provider: resolve readiness and capability evidence`
- **Depends on**: G4

**Objective**

Expose truthful configuration readiness, current health, static capability, and
test evidence without letting one overwrite another.

**Design**

- Readiness returns all blockers plus binding count.
- Health observation is time-bound and never mutates capability declarations.
- Effective capability combines Provider declaration, adapter static capability,
  current matching verification, and constrained user override according to the
  approved precedence.
- Verification key includes Provider/model/endpoint/adapter/policy/suite versions
  and expiry.
- Evidence contains sanitized fixture/test references, never prompt/response
  bodies or reasoning content.

**Test design**

- Multiple simultaneous blockers and health transitions.
- Verification key/expiry invalidation for every key component.
- Network failure does not turn supported capability into unsupported.
- User override cannot upgrade an adapter-impossible feature.
- Provider/model edits invalidate evidence.
- View provenance and redaction.

**Acceptance**

- UI can explain why a route is blocked and where each capability claim came
  from.

### RPG-402: Complete Provider Center and attach/rebind/detach UX

- **Status**: 验证成功
- **Suggested label**: `phase:4`, `area:frontend`, `priority:p1`
- **Suggested commit**: `provider-ui: complete gateway management flows`
- **Depends on**: RPG-401, RPG-110

**Objective**

Extend the minimal Phase 1 UI to cover Chat Completions adapters, Gateway state,
capability provenance, conformance tests, and safe lifecycle operations.

**Scope**

- Provider card protocol/model/credential, upstream health, Gateway health,
  adapter, used profiles, readiness, and capability summary.
- Add/edit discriminated protocol/adapter/auth fields and endpoint preview.
- Separate Test upstream and Test Codex route actions.
- Attach/rebind/detach dry-run preview with conflicts and recovery actions.
- Gateway start/status/restart/explicit port migration actions where allowed.
- Accessible loading, error, empty, degraded, stale, and expired-evidence states.

**Test design**

- Protocol-dependent field visibility and validation.
- Chat Completions without adapter remains saveable but unattachable.
- Gateway unavailable/restarting/foreign-port/version mismatch.
- Capability declared/verified/override/expired labels.
- Concurrent edits and stale dry-run recovery.
- No secret/token/body appears in rendered DOM or frontend store.

**Acceptance**

- Provider UI is split into focused components and does not further inflate
  `App.tsx`.
- All actions use V2 plans/DTOs, not duplicated frontend routing logic.

### RPG-403: Implement the pure RelayCompatibilityAnalyzer

- **Status**: 验证成功
- **Suggested label**: `phase:4`, `area:relay`, `priority:p0`
- **Suggested commit**: `relay: add provider compatibility analysis`
- **Depends on**: RPG-201, RPG-401

**Objective**

Analyze whether a source session can execute on a target Provider route before
any target session file is written.

**Design**

- Output is compatible, compatible-with-loss, or blocked.
- Report exact item IDs/types, proposed transformations/drops, warnings, and
  recovery actions.
- Ordinary text and completed verified function-call/result pairs may map.
- Unsupported hosted/MCP/computer tools, encrypted reasoning, file/image/audio,
  previous-response state without support, and unfinished tool state block.
- Compatible-with-loss is limited to representation loss that cannot break
  subsequent protocol correctness and requires explicit confirmation.
- Analyzer reads no credential and performs no writes.

**Test design**

- Text-only compatible route.
- Completed tool round trip on a verified target.
- Every blocked item type independently and in combinations.
- Compatible-with-loss confirmation requirement.
- Unknown/corrupt/partial session item fails closed.
- Analyzer output contains no auth/session secret.

**Acceptance**

- Blocked analysis cannot be passed to the executor.
- Every supported transformation is backed by adapter fixtures.

### RPG-404: Integrate API profiles with session, resume, sync, and relay

- **Status**: 验证成功
- **Suggested label**: `phase:4`, `area:session`, `priority:p0`
- **Suggested commit**: `relay: integrate provider-aware api profiles`
- **Depends on**: RPG-307, RPG-403

**Objective**

Make direct and Gateway API profiles first-class session/relay participants
without moving identity or secrets.

**Scope**

- Session view uses authoritative current binding plus original session
  Provider/model metadata.
- Resume uses `CodexLaunchPlanner`.
- Relay invokes compatibility analysis before target write.
- Executor accepts only compatible or explicitly confirmed compatible-with-loss.
- Sync/relay deny `auth.json`, credential/token stores, Provider private state,
  journals, config ownership state, and Gateway logs.
- Target runtime Provider/model/auth/billing/quota always comes from target.
- Refresh account/provider/session state after execution.

**Test design**

- ChatGPT → direct API, direct API → ChatGPT, ChatGPT → Gateway, Gateway →
  ChatGPT, and Gateway → Gateway.
- Provider/model mismatch messaging.
- Blocked analysis produces no partial target.
- Compatible-with-loss without/with confirmation.
- Gateway stopped after system restart resumes via launcher.
- Manifest and copied-file scans prove no secret/private state migration.

**Acceptance**

- All pre-existing non-API relay tests remain green.
- API profile relay behavior is deterministic and reversible according to the
  existing session-copy policy.

### RPG-405: Complete smoke, security, migration, observability, and documentation suites

- **Status**: 验证成功
- **Suggested label**: `phase:4`, `area:quality`, `priority:p0`
- **Suggested commit**: `test: complete provider gateway release suites`
- **Depends on**: RPG-402, RPG-404

**Objective**

Close cross-cutting release gaps and remove conflicting historical guidance.

**Scope**

- Rust unit/integration/contract/fault/concurrency/security suites.
- Frontend component/store/API tests.
- UI smoke coverage for protocol, adapter, plans, capabilities, and Gateway.
- Packaged launcher/sidecar/helper smoke.
- Legacy store/config/DTO migration matrix.
- Structured metrics for request count/status/latency/usage/retry without bodies.
- README, architecture, command/API, troubleshooting, privacy/security, release
  notes, and old v0.2 todo/design status.

**Test design**

- Repository/private-runtime synthetic secret scan.
- Log snapshot allowlist.
- Fresh install, upgrade, future schema, corrupt state, and rollback fixtures.
- Supported platform build/package tests.
- Accessibility and stable error-code/recovery-action mapping.
- Documentation commands are copied and executed in an isolated fake home where
  safe.

**Acceptance**

- No old document claims text-only adapter behavior that contradicts the released
  verified matrix.
- All automated release commands below pass from a clean checkout/fake home.

### RPG-406: Complete manual acceptance and release the feature

- **Status**: 验证成功
- **Suggested label**: `phase:4`, `area:release`, `priority:p0`
- **Suggested commit**: `docs: record remote gateway release acceptance`
- **Depends on**: RPG-405

**Objective**

Validate the packaged product against approved real Providers without making real
credentials or network tests part of the automated suite.

**Release acceptance matrix**

The repeatable G5 gate uses the pinned Codex 0.144.1 contract plus synthetic local Responses and
DeepSeek-compatible upstreams, a temporary HOME, real packaged processes, and the final app/DMG.
An optional real-account smoke may be performed by a user without recording credentials or content;
it is not required to make CI depend on a paid or private external account.

- Responses-compatible Provider: create, test, attach, normal prompt,
  restart, resume, rebind, detach.
- DeepSeek Chat Completions Provider: text non-stream, stream, function tool
  loop, thinking/tool history, UI close, system restart/launcher recovery,
  rebind, detach.
- Chat Completions without adapter: save/test upstream, attach blocked, no config
  mutation.
- Relay in both directions for supported text/tool sessions.
- Config preservation using a representative user config.
- Packaged install/upgrade/uninstall behavior and sidecar cleanup.
- Network/auth/rate-limit/provider outage recovery messages.

**Evidence rules**

- Record version, platform, scenario, result, sanitized request/error IDs, and
  reviewer.
- Never paste API key, token, prompt, response, tool arguments, raw reasoning,
  private paths, or account identity.
- Any failed mandatory scenario reopens the owning issue; it is not waived in
  release notes without explicit scope change.

**Acceptance**

- G5 is marked passed.
- The design, this todo, release notes, and UI claim the same capability matrix.
- Feature status changes from `待执行` to `验证成功` only after all mandatory
  evidence is approved.

## Cross-issue test matrix

| Behavior class | Required coverage                                                                                       |
| -------------- | ------------------------------------------------------------------------------------------------------- |
| Normal         | create/read/update/delete, plan/execute, direct non-stream/stream, tool round trip, launch/resume/relay |
| Edge           | empty/missing/unknown values, path prefixes, quoted IDs, chunk boundaries, multiple blockers, expiry    |
| Invalid input  | malformed store/TOML/JSON/SSE/URL/header/model/tool/command/config                                      |
| Error          | I/O, lock, Keychain, helper, upstream status, timeout, disconnect, parse, packaging, recovery           |
| State/conflict | revisions, stale plans, drift, concurrent writers, Provider edit, rotation, crash journal, restart      |
| Security       | secret/log/body redaction, auth isolation, SSRF/redirect, limits, symlink/path, spoofed sidecar/control |
| Migration      | every legacy/future fixture, read-no-write, deterministic migration, rollback                           |
| UI             | loading/empty/error/degraded/stale/conflict, accessibility, no secret in DOM/store                      |

Every changed source file must have a corresponding test file or an explicitly
recorded mapping to an integration/contract test in the owning issue.

## Verification commands

Commands may be refined by RPG-005, but equivalent coverage cannot be removed.

| Purpose            | Command                                                                                       | Expected                       |
| ------------------ | --------------------------------------------------------------------------------------------- | ------------------------------ |
| Rust format        | `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check`                      | exit 0                         |
| Rust lint          | `cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings` | exit 0                         |
| Rust full tests    | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`                                | exit 0                         |
| Frontend tests     | `cd apps/desktop && npm test`                                                                 | exit 0                         |
| Frontend build     | `cd apps/desktop && npm run build`                                                            | exit 0                         |
| Frontend lint      | `cd apps/desktop && npm run lint`                                                             | exit 0                         |
| Frontend format    | `cd apps/desktop && npm run format:check`                                                     | exit 0                         |
| UI smoke           | `cd apps/desktop && npm run test:ui`                                                          | exit 0                         |
| Working tree check | `git status --short`                                                                          | only intentional issue changes |

Focused commands must also be documented inside each implementation PR/issue and
should select the smallest relevant test set before the full suite.

## Recommended issue/commit boundaries

- One issue should normally produce one focused commit or a short coherent commit
  series using `scope: summary`.
- Pure mechanical `provider.rs -> provider/` module movement must be separate
  from schema or behavior changes and preserve all tests unchanged.
- Do not commit generated capture data before sanitization validation passes.
- Do not combine backend DTO replacement and a deferred frontend update; both
  sides of one public contract land in the same vertical issue.
- Do not mix real-provider manual evidence with implementation commits.

## Global done criteria

- [ ] Every issue has exactly one status and all are `验证成功`.
- [ ] G0–G5 are explicitly passed with linked evidence.
- [ ] Every changed source file has mapped normal, edge, invalid, error, and
      state/conflict tests where applicable.
- [ ] All verification commands pass from the approved supported platform.
- [ ] Legacy migration and future-version refusal are proven.
- [ ] No secret/body/tool/reasoning leak test fails.
- [ ] Codex config ownership and crash recovery tests pass at every injection
      point.
- [ ] Direct Provider does not start Gateway.
- [ ] Gateway Provider survives UI close and recovers through the launcher after
      restart.
- [ ] Combined retry budget is bounded and tested.
- [ ] Relay never copies identity or secret state and never partially writes a
      blocked target.
- [ ] Full design, TODO, README, UI claims, and release notes agree.

## Global STOP conditions

Stop the active issue and report if:

- its dependency or current phase gate is not complete;
- current Codex behavior differs materially from the captured contract;
- an implementation would silently drop an unknown Responses field/item;
- a route requires storing or exposing a plaintext secret outside the approved
  credential boundary;
- multi-process correctness cannot be proven with the approved store/lock model;
- crash recovery has more than one plausible outcome for the same journal state;
- config mutation would overwrite or persist user-owned secret-bearing values;
- a real network service is required for an automated test and cannot be replaced
  with a local mock/fixture;
- completing the issue requires an unapproved platform, public-listener, protocol,
  or product-scope expansion;
- relevant tests remain red after the focused fix loop;
- unrelated user changes overlap the same source area and cannot be preserved.
