# Todo: Monthly Quota Window Support

> Executor instructions: Follow this todo step by step. Generate tests from
> the "Test design" section before implementation. Run each verification command
> and confirm the expected result before moving to the next task.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: existing Codex app-server quota endpoint
- **Category**: feature
- **Planned at**: current workspace

## Why this matters

**Background**: Some Codex accounts return only a monthly quota window from `account/rateLimits/read`. They do not have the usual 5h primary and weekly secondary windows.

**Current state**: Rust parses `primary` and `secondary` percent/reset fields but discards `windowDurationMins`. The frontend always renders `Session (5h)` and `Weekly (7d)`, so monthly-only accounts are mislabeled as 5h and show a fake weekly N/A row.

**Impact**: Users cannot tell whether an account has monthly quota or 5h quota, and the weekly row creates misleading unavailable data.

**What improves**: LAM will preserve quota window metadata and render the windows actually returned by Codex. Monthly-only accounts show a monthly quota only.

## Scope

**In scope**:
- Extend quota snapshot contract with primary/secondary window duration minutes.
- Parse `windowDurationMins` / compatible snake-case fields from app-server output.
- Add frontend helpers that derive display windows from snapshot data.
- Update overview and tray quota rows to hide missing weekly data for monthly-only accounts.
- Add focused Rust and frontend tests.

**Out of scope**:
- Changing Codex app-server behavior.
- Adding support for multiple non-codex limit ids beyond the current selected `rateLimits` object.
- Redesigning quota colors or refresh scheduling.

## Design

Expected inputs are Codex app-server JSON-RPC lines. Outputs are `UsageQuotaSnapshot` objects and UI quota window models.

The backend keeps the current primary/secondary percent fields for compatibility and adds optional duration fields. Missing duration remains `null`.

The frontend derives visible windows:
- If primary has data, render it with a label derived from `primaryWindowDurationMins`.
- If secondary has data, render it with a label derived from `secondaryWindowDurationMins`.
- If secondary is missing, do not render a weekly row.
- Known labels: `300 -> Session (5h)`, `10080 -> Weekly (7d)`, monthly-range windows around 30 days -> `Monthly`.
- Unknown durations remain readable via day/hour labels.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Preserve quota window duration | Rust test parses 5h/weekly and monthly-only durations | 验证成功 |
| T2 | Render dynamic quota windows | UI tests show monthly-only hides 5h/weekly and normal accounts still show both | 验证成功 |

### T1: Preserve quota window duration

**Status**:
- [x] 待执行
- [x] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The frontend cannot distinguish 5h, weekly, and monthly windows without backend metadata.

**What to do**:
- Add optional duration fields to `UsageQuotaSnapshot`.
- Parse duration from primary and secondary windows.
- Keep existing fields and cache compatibility.

**Logic design**:
- Read `windowDurationMins`, `window_duration_mins`, or `windowDurationMinutes`.
- Store primary and secondary durations as `Option<u64>`.
- Leave values as `None` when Codex omits them.

**Test design**:
- Update existing Rust app-server parse tests to assert `300`, `10080`, and monthly-only `43800`.
- Expected initial failure: new fields do not exist or are `None`.

**Acceptance**:
- Focused command: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml quota_app_server --test phase1_core`
- Expected result after implementation: exit 0.

**Done criteria**:
- [x] Tests were written before implementation
- [x] New tests were run and confirmed to fail for expected reason, or exception documented
- [x] Implementation follows this task design
- [x] Focused verification command passes
- [x] Task overview row status matches this task status

### T2: Render dynamic quota windows

**Status**:
- [x] 待执行
- [x] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Monthly-only accounts should not display 5h and weekly rows.

**What to do**:
- Add frontend quota window derivation helpers.
- Use helpers in overview cards and tray account rows.
- Keep existing normal 5h/weekly display for accounts with both windows.

**Logic design**:
- Build a small `QuotaDisplayWindow` model from `UsageQuotaSnapshot`.
- Use `variant` only for styling/reset-copy fallback, not for deciding fixed labels.
- For tray rows, render one or two actual windows instead of hardcoded left/right meanings.

**Test design**:
- Add helper tests for 5h/weekly and monthly-only labels.
- Add component or smoke coverage that monthly-only does not render weekly N/A.
- Expected initial failure: helper missing or UI still hardcodes 5h/weekly.

**Acceptance**:
- Focused command: `npm test -- --run src/lib/quota.test.ts src/components/tray-quota-panel.test.tsx`
- Smoke command: `npm run test:ui`

**Done criteria**:
- [x] Tests were written before implementation
- [x] New tests were run and confirmed to fail for expected reason, or exception documented
- [x] Implementation follows this task design
- [x] Focused verification commands pass
- [x] Task overview row status matches this task status

## Test plan

- Rust parser normal behavior: primary 5h and secondary weekly durations.
- Rust parser edge case: monthly-only primary with `secondary: null`.
- Frontend helper normal behavior: render 5h and weekly windows.
- Frontend helper edge case: render only monthly window.
- UI state: tray monthly-only row does not show weekly N/A.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Rust focused tests | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml quota_app_server --test phase1_core` | exit 0 |
| Frontend focused tests | `cd apps/desktop && npm test -- --run src/lib/quota.test.ts src/components/tray-quota-panel.test.tsx` | exit 0 |
| UI smoke | `cd apps/desktop && npm run test:ui` | exit 0 |

## Done criteria

- All task rows are `验证成功`.
- Monthly-only snapshots display one monthly quota window.
- 5h/weekly snapshots still display both quota windows.
- Existing cache and old snapshots without duration fields remain readable.
