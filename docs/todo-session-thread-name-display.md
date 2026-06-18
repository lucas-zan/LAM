# Todo: Session Thread Name Display

> Executor instructions: Follow this todo step by step. Generate tests from the
> "Test design" section before implementation. Run verification commands and
> update task status immediately after validation.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: local Codex `session_index.jsonl` format observed on 2026-06-18
- **Category**: feature

## Why this matters

**Background**: Codex maintains a local `session_index.jsonl` with `id`, `thread_name`, and `updated_at`. The app currently shows session IDs in the Sessions table and Handoff selector, making relay decisions hard.

**Current state**: Backend `CodexSession` exposes `summary` but not `threadName`. `list_sessions` parses each transcript file but ignores `$CODEX_HOME/session_index.jsonl`. Sessions UI displays `session.id`; Handoff select displays `{id} · {cwd}`.

**Impact**: Users cannot reliably identify which conversation they are relaying when many sessions share the same project or similar timestamps.

**What improves**: Session list and Handoff selector show Codex's human-readable thread name while preserving session ID for resume/copy behavior.

## Scope

**In scope**:
- Add `thread_name/threadName` to backend/frontend session models.
- Read `$CODEX_HOME/session_index.jsonl` and map `id -> thread_name`.
- Display thread name in Sessions table and Handoff modal.
- Include thread name in session filtering.

**Out of scope**:
- Regenerating missing thread names.
- Reading ChatGPT/cloud-side names.
- Changing resume IDs or sync/relay file semantics.

## Design

- Parse `session_index.jsonl` line-by-line as JSON.
- Ignore malformed index lines and missing/empty names.
- Match by resolved session ID, including rollout IDs parsed from `session_meta.payload.id`.
- Keep `summary` parsing as fallback.
- UI display helper: `threadName || summary || id`.
- Keep `id` visible as secondary mono text so users can still inspect/copy exact resume target.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Backend thread name extraction | `list_sessions` returns `thread_name` from `session_index.jsonl` with fallback behavior | 验证成功 |
| T2 | UI thread name display | Sessions table and Handoff selector show thread names and search matches them | 验证成功 |

### T1: Backend thread name extraction

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Codex stores the friendly session names in `session_index.jsonl`, not in every transcript file.

**What to do**:
- Extend Rust `CodexSession` with `thread_name: Option<String>`.
- Add a helper to read `$CODEX_HOME/session_index.jsonl`.
- Populate `thread_name` after session ID resolution.

**Logic design**:
- Read the index once per `list_sessions` call.
- Use `serde_json::Value` to parse each line defensively.
- Shorten names with existing `short_text`.
- Do not fail `list_sessions` if the index is absent or partially malformed.

**Test design**:
- Add a Rust test with a rollout session whose ID exists in `session_index.jsonl`; expect `thread_name`.
- Add fallback assertion for an unindexed session; expect `None`.
- Expected initial failure: `CodexSession` lacks `thread_name` or returns `None`.

**Acceptance**:
- Focused command: `cargo test parses_session_thread_name_from_codex_session_index --manifest-path apps/desktop/src-tauri/Cargo.toml`
- Relevant suite: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Relevant suite command passes
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T2: UI thread name display

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Backend data is only useful if users see it at the decision points: session list and handoff selector.

**What to do**:
- Add `threadName?: string | null` to TypeScript `CodexSession`.
- Display `threadName || summary || id` as primary session text.
- Keep session ID as secondary text.
- Include `threadName` in session search.
- Update Handoff modal select and preview line to show the display name.

**Logic design**:
- Avoid changing selected value; the select value remains `session.id`.
- Use a small helper function where local to avoid duplicated fallback logic.
- Tests should render existing components, not inspect implementation details.

**Test design**:
- Update route test session fixture with `threadName`.
- Assert Sessions table shows the thread name and still shows session ID.
- Add App handoff test covering select option text with thread name.
- Expected initial failure: thread name is not rendered.

**Acceptance**:
- Focused command: `npm --prefix apps/desktop test -- handoff`
- Relevant UI suite: `npm --prefix apps/desktop test -- --run`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Relevant suite command passes
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Backend focused | `cargo test parses_session_thread_name_from_codex_session_index --manifest-path apps/desktop/src-tauri/Cargo.toml` | exit 0 after implementation |
| Backend suite | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` | exit 0 |
| UI focused | `npm --prefix apps/desktop test -- handoff` | exit 0 after implementation |
| UI suite | `npm --prefix apps/desktop test -- --run` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

- `session_index.jsonl` format is materially different from observed `id/thread_name/updated_at`
- Adding the field requires changing resume IDs
- Tests cannot isolate local session files safely
