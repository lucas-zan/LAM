# Todo: RPG-101 Versioned Store Primitives

> Executor instructions: Write focused storage tests first and observe the expected
> compile/test failure. Implement only generic persistence; Provider domain migration
> belongs to RPG-102.

## Status

- **Priority**: P0
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: G1
- **Category**: persistence/infrastructure
- **Planned at**: `codex/202607071457-ui-optimize-20260710102915-remote-provider-gateway`

## Why this matters

**Background**: Provider, binding, Gateway state and journal stores all require the
same revision, lock and atomic-write guarantees.

**Current state**: `provider.rs` reads/writes a raw JSON array with no schema,
revision, cross-process lock, CAS or atomic replacement.

**Impact**: Building Provider V2 directly on raw filesystem calls would duplicate
unsafe persistence and allow lost updates or future-schema overwrite.

**What improves**: Domain services receive one reusable, fault-tested repository
primitive before migration starts.

## Scope

**In scope**:

- Generic flattened versioned JSON envelope and snapshots.
- Missing-store default, read-only load and future-version rejection.
- Installation-wide advisory flock with bounded timeout.
- CAS commit, same-directory private temp, fsync, rename and parent sync.
- Distinct structured errors and deterministic pre-commit fault injection.

**Out of scope**:

- Provider V2 types/migration.
- Keychain, config editor and journal state machine.
- Windows/Linux primitives.

## Design

`VersionedFileStore<T>` owns target/installation lock paths, current schema, size
limit and lock timeout. `T` serializes as an object flattened beside
`schemaVersion` and `revision`. `load_or_default` returns revision 0 without
creating the target. `compare_and_swap` takes expected revision, re-reads while
holding the exclusive installation lock, rejects future/invalid state, writes
revision + 1 through a same-directory file, syncs, renames and syncs the parent.
Fault injection happens before serialize/temp-write/temp-sync/rename so every
reported injected failure is pre-commit and preserves the old target.

## Tasks

### Task overview

| ID  | Task                                    | Acceptance summary                                                           | Status   |
| --- | --------------------------------------- | ---------------------------------------------------------------------------- | -------- |
| T1  | Define red-first generic store tests    | Missing/load/CAS/concurrency/lock/fault/future/read tests fail before module | 验证成功 |
| T2  | Implement locked atomic versioned store | Focused tests and relevant Rust suite pass repeatedly                        | 验证成功 |

### T1: Define red-first generic store tests

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:

Concurrency and fault semantics must be fixed before choosing implementation
details.

**What to do**:

- Add integration tests using a small domain-neutral payload.
- Cover missing/load/commit, two snapshots, two writers, lock timeout/kernel
  stale recovery, injected pre-commit failures, future schema and mtime.

**Logic design**:

- Tests share the same installation lock path across store instances.
- Concurrent writers begin from the same revision and exactly one succeeds.
- A child/independent file handle holds flock to force timeout; release proves
  stale ownership is kernel-recovered.

**Test design**:

- `versioned_store_loads_missing_and_commits_revision`.
- `versioned_store_rejects_stale_snapshot`.
- `versioned_store_serializes_processes_and_survives_atomic_write_failure`.
- `versioned_store_times_out_on_live_lock_and_recovers_after_release`.
- `versioned_store_rejects_future_schema_without_overwrite`.
- `versioned_store_read_has_no_target_mtime_side_effect`.

**Acceptance**:

- Focused test initially fails because storage API/module does not exist.
- Tests compile and pass after implementation.

**Done criteria**:

- [x] Tests written before implementation
- [x] Expected red state recorded: unresolved `localagentmanager_core::storage` import
- [x] All named behaviors represented
- [x] T1 becomes `验证成功`

### T2: Implement locked atomic versioned store

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:

All later Phase 1 stores need a small reusable persistence boundary.

**What to do**:

- Add `services/storage.rs` and public exports.
- Add direct `libc` dependency for Darwin flock.
- Keep domain types out and map errors to stable `AppError` codes.
- Add private modes and symlink/size checks within the generic boundary.

**Logic design**:

- Shared load and exclusive CAS use bounded nonblocking flock retry.
- Envelope metadata is parsed before full payload deserialization so future
  schema receives its own error.
- Rename is the commit point; injected failures occur before it.

**Test design**:

- T1 integration tests drive the implementation.
- Run focused test repeatedly to expose races.
- Run full Rust suite and formatter.

**Acceptance**:

- Focused suite passes repeatedly.
- Full Rust tests, clippy-level compile warnings and rustfmt pass.

**Done criteria**:

- [x] Implementation stays domain-neutral
- [x] Error taxonomy matches the design
- [x] Focused concurrency/fault suite passes repeatedly
- [x] Full Rust suite and formatting pass
- [x] T2 becomes `验证成功`

## Test plan

- Normal missing/load/commit and revision increments.
- Stale snapshot and concurrent writer conflict.
- Live lock timeout and release recovery.
- Invalid JSON, future schema, size and symlink rejection.
- Serialize/temp-write/temp-sync/rename injected failures preserve old bytes.
- Read preserves target bytes and mtime.

## Verification commands

| Purpose     | Command                                                                    | Expected on success                      |
| ----------- | -------------------------------------------------------------------------- | ---------------------------------------- |
| Red/focused | `cargo test --test provider_versioned_store` from `apps/desktop/src-tauri` | fails before implementation; then exit 0 |
| Race repeat | repeat focused suite 10 times                                              | every run exit 0                         |
| Full Rust   | `cargo test` from `apps/desktop/src-tauri`                                 | exit 0                                   |
| Format/diff | `cargo fmt --check` and `git diff --check`                                 | exit 0                                   |

## Done criteria

- [x] T1 and T2 are `验证成功`
- [x] Both task checklists complete
- [x] Master RPG-101 status/evidence updated
- [x] No STOP condition remains

## STOP conditions

- Cross-process flock cannot be tested on the exact MVP platform.
- Atomic rename cannot preserve the prior target for a pre-commit failure.
- Generic storage requires Provider-specific fields or migration logic.
