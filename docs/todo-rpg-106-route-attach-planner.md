# RPG-106 Route/Attach Planner Execution TODO

Status: 验证成功

## Background and current state

Provider V2, credential references, authoritative profile bindings, and the
non-destructive Codex config projection exist, but callers still lack one pure
contract that explains whether a Provider can be attached and exactly which
state an approved dry-run represents. Without it, API/UI/executor code can
silently diverge or execute a preview after Provider, binding, config, adapter,
Gateway, or option state changes.

## Scope and design

- Add a side-effect-free `ProviderRoutePlan` for Responses direct routing and
  Chat Completions Gateway routing.
- Validate selected model, adapter registration/protocol pair, credential route,
  credential readiness, binding drift, and Gateway availability as typed
  blockers rather than hidden defaults.
- Add `ProfileAttachPlan` carrying expected store/binding revisions, config hash,
  Gateway generation, config projection, journal operations, and a redacted
  preview.
- Hash a canonical snapshot of every execution-relevant input. Store issued
  fingerprints in a bounded in-memory registry with TTL and one-shot consume.
- The planner receives only credential references/readiness flags. It performs
  no filesystem, environment, keychain, process, or network I/O.

## Tasks

- [x] T1: Add failing tests for direct/Gateway/missing-adapter routes and typed
      readiness blockers.
- [x] T2: Add failing tests proving every state input changes the fingerprint,
      previews contain no secret, and expiry/replay/tamper/staleness are rejected.
- [x] T3: Implement pure route/attach planning and canonical fingerprints.
- [x] T4: Implement bounded TTL/one-shot dry-run registry.
- [x] T5: Run focused and full regression; update master task evidence.

## Validation evidence

- Red phase: focused test failed to compile because `provider_planner` did not
  exist.
- Focused planner tests pass 4/4, covering routes/blockers, state-complete
  fingerprints, redacted previews, TTL, replay, tamper, and stale-state checks.
- Full Rust suite passes 193 tests with 2 ignored.
- Frontend passes 132/132; Phase 0 gate passes 15/15 and validates 12 scenarios
  across 15 artifacts; lint and build pass.

## Test and acceptance commands

```sh
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_planner
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
```

Acceptance requires the same serialized plan types to be reusable by API, UI,
executor, and relay; no secret value or unsafe command output may occur in the
plan or its preview; an executor can reject stale state without recomputing a
hidden default.

## STOP conditions

- Stop if implementing the planner requires reading a secret or performing I/O.
- Stop if a fingerprint omits an execution-relevant state input.
- Stop if existing user UI changes would need to be overwritten.
