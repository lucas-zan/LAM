# Todo: Provider Gateway Stage 4 observability and load validation

> Executor instructions: Follow this todo step by step. Generate tests from the
> "Test design" section before implementation. Run each verification command and
> confirm the expected result before moving to the next task. If a STOP condition
> occurs, stop and report instead of improvising.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: Stage 3 verified; fixed 32/4/64/8/16 admission contract
- **Category**: feature/observability/performance
- **Planned at**: current `20260716-gateway-external-provider` worktree after Stage 3

## Why this matters

**Background**: Stage 5 cannot safely introduce an Admission Controller until
the existing Gateway can explain running, queueing, timeout, upstream failure,
stream lifecycle, and memory pressure without exposing credentials or prompts.

**Current state**: `GatewayServerActivity` exposes only dispatch-level inflight
and idle time. `RequestLogMetadata` records status, total latency, Binding hash,
retry count, and usage. Queue wait/body bytes, running/queued gauges, active
streams, TTFT, upstream status, timeout stage, rejection reason, and client
cancellation are absent. A 4 MiB body with 64 queue slots permits a theoretical
256 MiB retained-body envelope before allocator/HTTP overhead.

**Impact**: Rejections cannot be attributed to Gateway, Binding, or upstream
capacity. Queue timeout and byte limits would be guesses, and stream disconnect
leaks could not be detected.

**What improves**: A framework-independent observability contract provides
bounded, redacted snapshots and request records. Deterministic load analysis and
one explicit macOS RSS probe establish Stage 5 recommendations without changing
current scheduling or production limits.

## Scope

**In scope**:
- Global running/queued/queued-body/active-stream gauges and lifecycle counters.
- Per-request Binding/provider/capacity hashes, queue wait, TTFT, upstream status,
  timeout stage, and stable outcome code.
- Stream body completion/drop tracking for active streams and client disconnects.
- A non-secret candidate upstream capacity identity for analysis only.
- 4 MiB × 64 retained-body/RSS baseline and Stage 5 limit/timeout recommendations.
- Bounded-cardinality/redaction tests and parent plan update.

**Out of scope**:
- Enforcing queued-body byte limits or the recommended queue timeout.
- Changing admission, queue counts, 32/4/64/8/16 capacity, or error codes.
- Fair scheduling, CapacityKey admission, burst, borrowing, or adaptive limits.
- A public metrics endpoint or UI.

## Design

Create `gateway/observability.rs` for pure DTOs, stable enums, candidate capacity
identity hashing, and Stage 5 recommendations. Identity input contains only a
normalized endpoint origin/path and credential reference metadata; secret values,
raw URLs, Provider IDs, Binding IDs, models, request IDs, and messages are never
metrics labels.

`GatewayServerActivity` owns atomic global gauges/counters and returns an immutable
snapshot. Queue and running RAII guards update counts and queued body bytes. A
tracked Body stream holds an active-stream guard until terminal completion or
drop; early drop increments client cancellation. These observations do not hold
or release admission/upstream permits.

`SecureUpstreamClient` records elapsed milliseconds until response headers as a
TTFT approximation. Routes attach TTFT and upstream status to
`GatewayHttpResponse`; stable error codes map to bounded timeout stages. Server
request records sample current global/Binding state and use stable short hashes.
Rejected/timeout requests are observed through the same record builder.

Recommendations are analysis outputs only: global queued bodies 32 MiB,
per-Binding queued bodies 8 MiB, and queue wait timeout 30 seconds, distinct from
the existing 15-minute total request timeout. Stage 5 owns enforcement.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Define redacted observability contract | Stable DTOs/enums/hashes reject secret or high-cardinality labels | 验证成功 |
| T2 | Instrument admission and stream lifecycle | Gauges/counters and rejected request records return to zero correctly | 验证成功 |
| T3 | Record upstream outcomes and TTFT | Request records distinguish upstream status and timeout stages | 验证成功 |
| T4 | Establish load envelope and recommendations | 4 MiB × 64 baseline and Stage 5 limits/timeout are recorded | 验证成功 |
| T5 | Run regression gate and update parent plan | Relevant suites/quality checks pass; only Stage 4 closes | 验证成功 |

### T1: Define redacted observability contract

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Metrics and logs need one bounded vocabulary before instrumentation.

**What to do**:
- Add snapshot/request outcome DTOs and stable timeout/rejection enums.
- Add a fixed-length candidate capacity identity hash derived without secrets.
- Define Stage 5 recommendation constants separately from active config.

**Logic design**:
- Serialize enums as stable snake_case values.
- Hash canonical endpoint and credential reference metadata; never resolve a secret.
- Same endpoint/reference yields one identity across Provider IDs; different
  credential references yield different identities.

**Test design**:
- Same capacity inputs hash identically; different credential identities differ.
- Hash is fixed-length lowercase hex and contains no raw endpoint/reference.
- Timeout/rejection values serialize to a fixed allowlist.
- Expected initial failure: the observability module/contracts do not exist.

**Acceptance**:
- `cargo test --test provider_gateway_observability capacity_identity -- --test-threads=1`

**Done criteria**:
- [x] Tests were written before implementation
- [x] New tests failed for the missing contract
- [x] Contract contains no secret-resolution dependency
- [x] Focused tests pass (3 capacity identity tests)
- [x] Task overview and task status are `验证成功`

### T2: Instrument admission and stream lifecycle

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Stage 5 needs proof that running, queued, bytes, streams, and disconnects
return to zero.

**What to do**:
- Extend activity with immutable snapshots.
- Update queue/running guards and tracked response Body streams.
- Observe queue overflow and queue/handler timeout with the same redacted schema.

**Logic design**:
- Atomic gauges use RAII; cancellation/drop cannot skip decrements.
- Active stream starts only for an explicitly streaming response.
- Normal terminal completion and early client drop are distinguished.
- Rejection records contain hashes/codes, never request bodies or auth headers.

**Test design**:
- Queued request snapshot reports request/byte counts and returns to zero.
- Running request count is exact around handler lifetime.
- Stream count stays nonzero until Body consumption/drop; early drop increments
  client cancellation.
- Queue overflow record has stable rejection reason and redacted fields.
- Expected initial failure: snapshot fields and streaming lifecycle APIs are absent.

**Acceptance**:
- `cargo test --test provider_gateway_server -- --test-threads=1`
- `cargo test gateway_observability --lib -- --test-threads=1`

**Done criteria**:
- [x] Tests were written before implementation
- [x] New tests failed for missing snapshot/lifecycle behavior
- [x] RAII metrics return to zero on all tested exits
- [x] Existing status/error behavior remains unchanged
- [x] Focused tests pass
- [x] Task overview and task status are `验证成功`

### T3: Record upstream outcomes and TTFT

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: A 503 from upstream must be distinguishable from Gateway admission or
timeout failures.

**What to do**:
- Measure response-header latency in the upstream client.
- Propagate TTFT/upstream status/outcome code through route responses.
- Map stable timeout codes to bounded timeout stages.

**Logic design**:
- TTFT is monotonic elapsed milliseconds from upstream attempt start to headers.
- Upstream response status is recorded separately from downstream status.
- Unknown errors remain `other`; raw messages never enter structured labels.

**Test design**:
- Delayed local upstream produces nonzero TTFT and correct upstream status.
- 429 and 503 remain downstream-compatible while recording upstream status.
- first-byte/idle/total/Gateway queue/handler timeout codes map correctly.
- Expected initial failure: upstream responses and request records lack these fields.

**Acceptance**:
- `cargo test --test provider_gateway_upstream --test provider_gateway_routes --test provider_gateway_server -- --test-threads=1`

**Done criteria**:
- [x] Tests were written before implementation
- [x] New tests failed for missing TTFT/outcome fields
- [x] Stable outcomes are propagated without raw error messages
- [x] Focused suites pass
- [x] Task overview and task status are `验证成功`

### T4: Establish load envelope and recommendations

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Count-only queue bounds allow a 256 MiB retained request-body envelope.

**What to do**:
- Add deterministic envelope tests for 4 MiB × queue limits.
- Add an explicit ignored macOS RSS probe and run it once for the record.
- Validate recommendations are below current worst case and queue timeout is
  independent of total timeout.

**Logic design**:
- Load probe allocates/touches 64 independent 4 MiB bodies and records peak RSS.
- Unit tests do not enforce recommendations in production.
- Stage 5 recommendations: 32 MiB global, 8 MiB/Binding, 30-second queue timeout.

**Test design**:
- Current global envelope equals 256 MiB and Binding envelope equals 32 MiB.
- Recommendations bound each envelope and are nonzero.
- Queue timeout is 30 seconds and less than the 15-minute total timeout.
- Expected initial failure: recommendation/envelope APIs do not exist.

**Acceptance**:
- `cargo test --test provider_gateway_observability load_envelope -- --test-threads=1`
- `cargo test --test provider_gateway_observability rss_probe -- --ignored --nocapture --test-threads=1`

**Done criteria**:
- [x] Tests were written before implementation
- [x] New tests failed for missing load APIs
- [x] RSS probe result is recorded: peak delta 264,421,376 bytes (252.17 MiB)
- [x] Recommendations remain analysis-only
- [x] Focused tests pass
- [x] Task overview and task status are `验证成功`

### T5: Run regression gate and update parent plan

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Observability must not change scheduling, responses, secrets, or streaming.

**What to do**:
- Run server/routes/upstream/Binding/sidecar suites and observability tests.
- Run Rust format, changed-library Clippy, and diff checks.
- Update only Stage 4 in the parent plan and record baseline exceptions.

**Logic design**: Do not fix unrelated lint findings or start Stage 5 enforcement.

**Test design**: Combined regression commands below.

**Acceptance**: All relevant commands pass; known unrelated strict-Clippy baseline
is recorded rather than broadened into this stage.

**Done criteria**:
- [x] All relevant regression suites pass (66 passed; explicit RSS probe run separately)
- [x] Format, changed-library Clippy, and diff checks pass
- [x] Parent plan and execution record are accurate
- [x] Task overview and task status are `验证成功`

## Test plan

- Normal: nonstream and stream requests record bounded metrics.
- Edge: max queue body envelope and zero/nonzero gauges.
- Invalid input: malformed endpoint/credential metadata cannot form an identity.
- Error: Gateway rejection, upstream 429/5xx, and timeout stages are distinct.
- State/cancellation: queue/stream cancellation returns gauges to zero.
- Security: serialized records contain no body, token, auth header, raw URL, model,
  Provider ID, Binding ID, or error message.

### Post-completion upstream stream lifecycle correction (2026-07-17)

Downstream Body drop now cancels the attached upstream task immediately instead
of waiting for a channel send or the 60-second idle timeout. A separate
`active_upstream_streams` gauge remains nonzero until the background task exits
and its upstream permit is released. Normal completion and early client-drop
tests both verify gauge and permit cleanup. See
`docs/todo-provider-gateway-stage2-stage4-lifecycle-handoff-fix.md`.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Observability | `cargo test --test provider_gateway_observability -- --test-threads=1` | exit 0 after implementation; compile failure before implementation |
| Server/upstream/routes | `cargo test --test provider_gateway_server --test provider_gateway_upstream --test provider_gateway_routes -- --test-threads=1` | exit 0 |
| Gateway regression | `cargo test --test provider_gateway_binding --test provider_gateway_sidecar -- --test-threads=1` | exit 0 |
| RSS probe | `cargo test --test provider_gateway_observability rss_probe -- --ignored --nocapture --test-threads=1` | exit 0 and print peak delta |
| Rust quality | `cargo fmt -- --check`; changed-library Clippy with recorded baseline exceptions | exit 0 |
| Diff hygiene | `git diff --check` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

- Observability requires changing admission capacity or permit ownership.
- Client disconnect cannot be observed without retaining prompt/body/secret data.
- Candidate capacity identity requires resolving or hashing secret values.
- RSS probing would destabilize the development machine.
- Stage 5 enforcement is required to make Stage 4 tests pass.
- Relevant verification still fails after five fix loops.
