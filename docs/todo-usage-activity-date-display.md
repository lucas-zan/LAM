# Todo: Usage Activity Date Display

> Executor instructions: Follow this todo step by step. Generate tests from the
> "Test design" section before implementation. Keep changes scoped to usage date
> aggregation/display and the cockpit-tools format confirmation.

## Status

- **Priority**: P0
- **Effort**: S
- **Risk**: MEDIUM
- **Depends on**: none
- **Category**: bugfix
- **Planned at**: current worktree

## Why this matters

**Background**: The Usage heatmap shows month labels ending at Jun even when the
current date is 2026-07-06 and local usage data includes July events.

**Current state**: Backend activity SQL groups by `current_date` before falling
back to `event_timestamp`. Imported or parsed historical events may all carry the
same `turn_context.current_date`, collapsing many historical events into one day.
Frontend month labels also skip the final month when the final month is only a
short tail after the previous label.

**Impact**: Activity, peak daily tokens, streaks, and visible month labels can
misrepresent actual usage dates.

**What improves**: Usage activity will be based on actual event timestamps, and
the visible heatmap will expose the actual current/end month.

## Scope

**In scope**:
- `apps/desktop/src-tauri/src/services/usage.rs` activity-date aggregation.
- `apps/desktop/src/routes/usage.tsx` heatmap month labels.
- Existing Rust and React usage tests.
- Confirmation of `docs/cockpit-tools-main` token/session JSON parsing behavior.

**Out of scope**:
- Redesigning Usage layout or unrelated frontend components.
- Changing account import behavior in LAM.
- Modifying files inside `docs/cockpit-tools-main`.

## Design

Backend activity date should use `substr(event_timestamp, 1, 10)` for usage
timeline metrics. `current_date` remains stored for diagnostics/context but must
not drive activity buckets, peak daily tokens, or activity date lists.

Frontend month labels should still avoid dense overlap, but the final visible
month should be included when the range ends in a different month than the last
label. This makes short current-month tails visible without changing cell data.

Cockpit confirmation is read-only: inspect parser functions and report which
input formats are accepted and how they become Codex-readable credentials or
session files.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Fix backend activity dates | Rust tests prove `current_date` does not collapse event dates | 验证成功 |
| T2 | Fix heatmap month labels | React test proves final July label is rendered | 验证成功 |
| T3 | Confirm cockpit format handling | Report backed by code references | 验证成功 |

### T1: Fix backend activity dates

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Usage activity must reflect when events actually occurred, not the local context
date attached to a turn.

**What to do**:
- Add/adjust Rust tests in `usage.rs` so events with old `event_timestamp` and
newer `current_date` remain grouped by event date.
- Update activity SQL for peak daily tokens, activity buckets, and activity date
lists.

**Logic design**:
- Introduce one SQL expression or consistently use `substr(event_timestamp, 1,
  10)` in the relevant aggregation queries.
- Preserve filters, cumulative bucket filling, and existing data schema.

**Test design**:
- Add a normal/edge test where `event_timestamp` dates differ from
  `current_date`.
- Expected pre-fix failure: buckets collapse to the `current_date` day or peak
  daily tokens is inflated.

**Acceptance**:
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml usage_activity_uses_event_timestamp_for_calendar_days`
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml local_headline_and_activity_buckets_follow_local_usage`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Relevant existing usage test passes
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T2: Fix heatmap month labels

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
When the all-time 365-day range ends in early July, the UI currently can show Jun
as the final label even though July cells are present.

**What to do**:
- Add a React test for a range ending on July 6 that expects a final `Jul` label
  after `Jun`.
- Update month label generation to append or retain the range end month.

**Logic design**:
- Keep existing spacing behavior for intermediate labels.
- After normal label generation, ensure the end month is represented if it is
  different from the last rendered label.

**Test design**:
- Use fake system time `2026-07-06`.
- Render all-time activity containing July 6 and assert at least two `Jul`
  labels exist, covering the start July and final July.

**Acceptance**:
- `npm --prefix apps/desktop test -- --run src/routes/usage.test.tsx -t "renders the ending month label"`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design
- [x] Focused verification command passes
- [x] Relevant existing usage page tests pass
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T3: Confirm cockpit format handling

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The user needs to know whether cockpit-tools can use access-token-only ChatGPT
session JSON and why Codex app can still recognize imported data.

**What to do**:
- Inspect `docs/cockpit-tools-main` parser code for both provided formats.
- Explain whether this is account credential import or session rollout import.

**Logic design**:
- Do not modify cockpit files.
- Base conclusions on parser/extractor functions and command wiring.

**Test design**:
- Read-only verification by code references; no code changes required.

**Acceptance**:
- Final answer identifies supported format(s), required fields, and Codex app
  recognition mechanism with file references.

**Done criteria**:
- [x] Required parser functions were inspected
- [x] Conclusion distinguishes credential import from conversation/session import
- [x] Final answer includes code references
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Backend focused | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml usage_activity_uses_event_timestamp_for_calendar_days` | exit 0 after implementation |
| Backend regression | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml local_headline_and_activity_buckets_follow_local_usage` | exit 0 |
| Frontend focused | `npm --prefix apps/desktop test -- --run src/routes/usage.test.tsx -t "renders the ending month label"` | exit 0 after implementation |
| Frontend regression | `npm --prefix apps/desktop test -- --run src/routes/usage.test.tsx` | exit 0 |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:
- Usage parser behavior differs materially from the current-state analysis.
- Existing tests cannot be run in this workspace.
- Fixing the issue requires broad frontend redesign or account import changes.
