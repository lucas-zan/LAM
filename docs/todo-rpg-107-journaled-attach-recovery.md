# Todo: RPG-107 Journaled Attach Lifecycle and Crash Recovery

> Executor instructions: Follow this todo step by step. Generate tests from
> each Test design before implementation, confirm the expected red state, and
> update a task immediately after focused verification. Stop rather than
> weakening the installation-lock, ownership, or recovery contract.

## Status

- **Priority**: P0
- **Effort**: L / XL issue slice
- **Risk**: HIGH
- **Depends on**: RPG-106
- **Category**: feature/security/recovery
- **Planned at**: current uncommitted Phase 0/1 workspace

## Why this matters

**Background**: RPG-101–106 provide versioned stores, authoritative bindings,
ownership-safe config projection, credentials, and expiring plans. They do not
yet make config and binding mutations recoverable as one operation.

**Current state**: `VersionedFileStore` acquires the installation lock inside
each read/CAS, `CodexConfigEditor` applies one config file atomically, and
`ProfileBindingRepository` mutates bindings independently. There is no journal,
commit point, rebind compensation, detach coordinator, or startup recovery.

**Impact**: A crash after config rename but before binding CAS can leave Codex
using a route that the binding store does not claim. Blind backup restoration
could overwrite user edits. Rebind can also revoke the old route too early.

**What improves**: Every externally visible mutation is preceded by a durable,
secret-free journal; `binding_committed` is the commit point; pre-commit crashes
roll back only managed values, post-commit crashes roll forward cleanup, and
unknown ownership becomes explicit manual intervention.

## Scope

**In scope**:

- A versioned, bounded journal collection and validated transition rules.
- A single installation-lock transaction context with locked read/CAS methods.
- Attach, rebind, and idempotent detach orchestration for direct Responses.
- Dry-run ticket/fingerprint/revision/config-hash revalidation under the lock.
- Deterministic fault points around journal/config/binding/cleanup steps.
- Bounded restart recovery, ownership conflicts, terminal retention, and corrupt
  journal fail-closed behavior.
- Fake gateway-binding lifecycle hooks so Gateway preparation/revocation remains
  testable before Phase 3.

**Out of scope**:

- Real Keychain lifecycle (RPG-108), Tauri DTO/commands (RPG-110), UI (RPG-111).
- Gateway server/control plane, adapter execution, launcher wiring outside a
  reusable startup recovery entry point.
- Blind full-file backup restore or storing config/secret contents in journals.

## Design

The coordinator obtains `provider-hub.lock` once. Store operations inside the
critical section accept an unforgeable lock guard and never reacquire the lock.
Before journal creation it recomputes/validates current revisions, config hash,
plan ticket/fingerprint, blockers, and ownership; failure consumes no plan and
causes no mutation. The ticket is consumed immediately after the durable
`prepared` journal is written.

`AttachJournalRecord` contains operation identity, expected revisions, config
before/intended hashes, managed projection metadata, previous/prepared binding,
opaque gateway references, timestamps and sanitized error codes. It never
contains credentials, helper output, config text, or tokens. Valid transitions:

```text
prepared -> config_committed -> binding_committed -> completed
prepared/config_committed -> rolled_back | manual_intervention
binding_committed -> completed | manual_intervention
```

Attach/rebind commits config first and binding second. A failed pre-commit
operation restores only values described by `ConfigManagedProjection`; rebind
keeps the old binding/reference until the new binding is committed. Detach first
validates managed ownership, journals intent, restores managed config, removes
the binding, then revokes only the superseded opaque reference. Already-detached
is success without a new journal.

Recovery treats `prepared`/`config_committed` as rollback candidates and
`binding_committed` as roll-forward. Current config hash and binding identity
must be one of the journal's known states; otherwise recovery records
`manual_intervention` and returns `ATTACH_RECOVERY_OWNERSHIP_CONFLICT`. Terminal
records are retained for 30 days with at least the newest 100 kept;
`manual_intervention` is never automatically removed.

## Tasks

### Task overview

| ID  | Task                        | Acceptance summary                                                          | Status   |
| --- | --------------------------- | --------------------------------------------------------------------------- | -------- |
| T1  | Journal model/store         | Valid bounded records, transitions, retention, corrupt/secret rejection     | 验证成功 |
| T2  | Locked attach/rebind/detach | One lock, lock-time revalidation, deterministic commit/compensation         | 验证成功 |
| T3  | Restart recovery            | Pre-commit rollback, post-commit roll-forward, manual conflict, idempotence | 验证成功 |
| T4  | Integration and regression  | Startup entry point and all project gates remain green                      | 验证成功 |

### T1: Journal model and versioned store

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Recovery decisions require a durable, closed state machine rather than
inferring intent from backups or partial current state.

**What to do**:

- Add journal operation/state/record/collection types and repository.
- Enforce record/count/serialized-size limits and legal transitions.
- Reject duplicate active profile operations, secret-bearing fields/markers,
  future/corrupt envelopes, and invalid terminal cleanup.
- Implement terminal retention without removing active/manual records.

**Logic design**: Use `VersionedFileStore` with schema 1 and an explicit maximum
of 256 KiB per record / 128 active records. Mutation uses revision CAS. Transition
validation is pure; retention receives an explicit timestamp for deterministic
tests.

**Test design**:

- Normal create and every legal transition.
- Illegal skip/backward/terminal transition and duplicate active profile.
- Oversized/secret-shaped journal rejection.
- Terminal retention keeps newest 100, recent 30-day records, active, and manual.
- Corrupt/future store fails closed without overwrite.

**Acceptance**: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_attach_transaction journal_`

**Done criteria**:

- [x] Tests were written before implementation and failed for missing module.
- [x] Journal contains no secret/config body surface and transitions are closed.
- [x] Focused tests pass (4/4 journal tests).
- [x] Task overview and this status are `验证成功`.

### T2: Locked attach, rebind, and detach coordinator

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Independent atomic writes are insufficient; plan validation and all
commits must be serialized under the same installation lock.

**What to do**:

- Add guarded store operations that cannot reacquire the installation lock.
- Add coordinator inputs/outputs, fake gateway lifecycle, and explicit faults.
- Implement attach/rebind/detach and managed-only compensation.

**Logic design**: Validate the plan/ticket and all snapshots before writing the
journal. Persist `prepared`, consume the plan, apply config, persist
`config_committed`, binding CAS, persist `binding_committed`, revoke superseded
gateway reference, then persist `completed`. Faults simulate a process stop and
leave the durable state for T3 recovery; ordinary errors attempt compensation.

**Test design**:

- Attach/rebind/detach success and idempotent repeat.
- Stale ticket/fingerprint/provider/binding/config/Gateway state has zero side
  effect and remains retryable before journal creation.
- Fault before/after journal, config, binding and cleanup steps.
- Binding CAS failure restores managed values and keeps old rebind binding.
- Detach ownership conflict and revision conflict mutate nothing.
- New gateway reference is revoked on failed pre-commit; old reference is only
  revoked after commit.

**Acceptance**: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_attach_transaction transaction_`

**Done criteria**:

- [x] Tests were written first and confirmed red (missing coordinator/planner APIs).
- [x] All mutation is under one installation lock and no nested lock occurs.
- [x] Focused success/conflict/fault tests pass (6/6 transaction tests).
- [x] Task overview and this status are `验证成功`.

### T3: Deterministic restart recovery

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Persisting a journal is useful only if a fresh process can safely and
idempotently resolve every nonterminal state.

**What to do**:

- Add bounded `recover_pending` over active records.
- Reconcile known config/binding states and gateway references.
- Record rolled-back/completed/manual-intervention terminal outcomes.

**Logic design**: Process records deterministically by creation/operation ID up
to a configured bound. Rollback only when config is before/intended and binding
is old/absent as recorded. Roll forward only when the prepared binding and
intended projection are authoritative. Never overwrite unknown hashes/revisions.

**Test design**:

- Fresh coordinator recovery from every persisted nonterminal state.
- Repeat recovery performs no duplicate gateway revocation.
- Unknown config hash, binding revision/identity and corrupt journal become a
  stable conflict/manual state without mutation.
- Recovery bound leaves remaining records for a later invocation.

**Acceptance**: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_attach_transaction recovery_`

**Done criteria**:

- [x] Recovery tests were written first and confirmed red (missing recovery API).
- [x] Every state ends in the documented recoverable or manual state.
- [x] Focused recovery tests pass (6/6) and repeat recovery is idempotent.
- [x] Task overview and this status are `验证成功`.

### T4: Integration and regression gate

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: RPG-108/110 and launch paths need one narrow recovery entry point, and
the high-risk transaction change must preserve all existing contracts.

**What to do**:

- Export a bounded startup recovery service API without wiring unrelated UI.
- Update RPG-107 master status/evidence and run all quality gates.

**Logic design**: The entry point delegates to the same coordinator recovery
path and reports structured outcomes; it does not hide manual intervention.

**Test design**: Source-to-test mapping, serialization/redaction audit, full Rust,
frontend, Phase 0, lint/build, rustfmt/Prettier/diff checks.

**Acceptance**: All verification commands below exit 0.

**Done criteria**:

- [x] Startup recovery test was written first and failed for the missing entry point.
- [x] Startup recovery API is covered by focused tests.
- [x] Full regression and quality gates pass.
- [x] Master issue and execution TODO evidence are current.
- [x] Task overview and this status are `验证成功`.

## Test plan

- Normal: attach, rebind, detach, rollback recovery, roll-forward recovery.
- Edge: repeat operations, terminal retention, recovery bound, missing config.
- Invalid: illegal journal transition, blockers, malformed/oversized journal.
- Error: every durable boundary fault, config write error, binding CAS conflict.
- State/conflict: stale revisions/fingerprint/hash, drift, unknown recovery state,
  concurrent profile operation, superseded/new gateway reference ownership.
- Security: journal/Debug/serialization synthetic marker scan and no config body.

## Verification commands

| Purpose  | Command                                                                                           | Expected on success              |
| -------- | ------------------------------------------------------------------------------------------------- | -------------------------------- |
| Focused  | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_attach_transaction` | all RPG-107 tests pass           |
| Rust     | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`                                    | all pass except approved ignores |
| Phase 0  | `pnpm test:gateway-phase0` in `apps/desktop`                                                      | 15/15 and gate pass              |
| Frontend | `pnpm test && pnpm lint && pnpm build` in `apps/desktop`                                          | all exit 0                       |
| Format   | `cargo fmt --check`, Prettier check, `git diff --check`                                           | all exit 0                       |

## Done criteria

- [x] Every task has exactly one checked status and is `验证成功`.
- [x] Every task-specific Done criteria is checked.
- [x] Full regression evidence is recorded.
- [x] No STOP condition remains unresolved.

## Validation evidence

- T1 red state: journal tests failed to compile before the module existed; journal
  tests now pass 4/4.
- T2 red state: transaction tests failed for missing coordinator, detach planner,
  and fault APIs; transaction tests now pass 8/8.
- T3 red state: recovery tests failed for the missing recovery API; recovery
  tests now pass 6/6.
- T4 red state: startup test failed for the missing bounded entry point; the
  complete RPG-107 focused suite passes 19/19.
- Full Rust suite passes 212 tests with 2 ignored. Frontend passes 132/132.
- Phase 0 gate passes 15/15 and validates 12 scenarios / 15 artifacts. Frontend
  lint and production build pass.

## STOP conditions

- The transaction cannot retain one installation lock without weakening store
  security or introducing a nested-lock deadlock.
- Recovery would need a blind full-file restore or overwrite an unknown hash.
- A secret/token/helper output/config body would need to enter the journal.
- Completion requires RPG-108 Keychain, RPG-110 API, RPG-111 UI, or changes to
  the weekly quota popover.
- Focused verification remains failing after five repair loops.
