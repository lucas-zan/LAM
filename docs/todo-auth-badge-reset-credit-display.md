# Todo: Auth badge and reset credit display

> Executor instructions: Follow this todo step by step. Generate tests from
> the "Test design" section before implementation. Run each verification command
> and confirm the expected result before moving to the next task. If a STOP
> condition occurs, stop and report instead of improvising.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: none
- **Category**: bugfix/ux
- **Planned at**: `agent/main-20260703094659-optimize-reset-credits-usage-display`

## Why this matters

**Background**: Codex login accounts are shown as `API Key` in Overview while the tray shows `Auth`. Reset-credit dots expose dates only through hover, making available cards and expiry hard to understand.

**Current state**: Backend account scanning derives `authMode` with loose auth.json heuristics. Overview maps `api_key` directly to `API Key`; tray collapses non-PAT modes to `Auth`. Reset-credit details are already present in `UsageQuotaSnapshot.resetCreditDetails`, but the UI primarily renders small colored dots.

**Impact**: Users misread Codex login accounts as manual API-key accounts, and cannot quickly understand reset-credit count or expiry.

**What improves**: Account cards and tray rows use one user-facing auth label, Codex login no longer appears as `API Key`, and reset credits show count plus nearest expiry while still preserving per-credit details.

## Scope

**In scope**:
- Backend auth-mode detection tests and logic in `apps/desktop/src-tauri/src/services/account.rs`.
- Frontend auth label helper and Overview/tray badge rendering.
- Frontend reset-credit display helper and Overview/tray rendering.
- Focused tests for changed source files.

**Out of scope**:
- New backend quota endpoints.
- Usage page mode decoupling.
- Provider configuration redesign.

## Design

- Treat `authMode` as a backend fact but translate it through a shared frontend helper for user-facing labels.
- Display `personal_token` as `PAT`; `uploaded` as `Uploaded`; `oauth`, `api_key`, `config`, and unknown non-empty auth modes as `Auth` unless a later backend contract can prove a manual provider API key.
- Keep backend detection conservative: Codex login structures with OAuth token fields should win over `OPENAI_API_KEY` when both appear.
- Use existing reset-credit detail fields to compute:
  - visible dot metadata
  - total count
  - nearest expiry
  - per-credit expiry list
- Avoid double timezone conversion in frontend because API detail expiry may already be normalized.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Auth badge consistency | Codex login/auth accounts render `Auth`, not `API Key`, in Overview and tray | 验证成功 |
| T2 | Reset-credit readable summary | Account rows render count and nearest expiry from existing snapshot details | 验证成功 |

### T1: Auth badge consistency

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Overview currently exposes backend `api_key` as `API Key`, which is misleading for Codex login accounts whose auth.json may include `OPENAI_API_KEY`.

**What to do**:
- Add a shared frontend auth-label helper.
- Update Overview and tray to use it.
- Make backend auth detection prefer OAuth-like token structures over `OPENAI_API_KEY`.

**Logic design**:
- Frontend helper returns `null` for absent mode; `PAT`, `Uploaded`, or `Auth` for known modes.
- Backend `infer_auth_type` and `detect_auth_mode` should check OAuth token evidence before `OPENAI_API_KEY`.
- Do not change account transport fields.

**Test design**:
- Frontend Overview test: account with `authMode: "api_key"` displays `Auth` and not `API Key`.
- Tray test: account with `authMode: "api_key"` displays `Auth`.
- Backend unit test: auth JSON with token evidence and `OPENAI_API_KEY` is detected as `oauth`.

**Acceptance**:
- `npm test -- --run src/routes/handoff.test.tsx src/components/tray-quota-panel.test.tsx src/lib/auth.test.ts`
- `cargo test test_detect_auth_mode_priority`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T2: Reset-credit readable summary

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Colored dots alone hide the concrete reset-credit count and expiry, despite the backend already returning per-credit details.

**What to do**:
- Extend reset-credit display data with summary text and detail titles.
- Render a compact text summary beside account names in Overview and tray.
- Keep dots as secondary visual indicators.

**Logic design**:
- Use `resetCreditDetails` when present, sorted by nearest expiry.
- Summary should show `<count> resets` and nearest expiry date/time when known.
- Details remain available through titles/aria labels.
- Do not add new backend fields or endpoints.

**Test design**:
- `resetCreditDisplay` test asserts summary and detail text from per-credit data.
- Overview test asserts readable reset-credit summary appears.

**Acceptance**:
- `npm test -- --run src/lib/quota.test.ts src/routes/handoff.test.tsx`

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation, or the exception is documented here
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Relevant suite/build/lint command passes when applicable
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Test plan

- Normal behavior: Codex auth/API-key-like modes display `Auth`; PAT displays `PAT`.
- Edge cases: missing auth mode renders no badge; uploaded mode remains distinct.
- Reset credits: per-credit details drive nearest expiry and detail labels; missing expiry still renders count.
- State/conflict: Overview and tray use the same label contract.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Frontend focused | `npm test -- --run src/lib/auth.test.ts src/lib/quota.test.ts src/routes/handoff.test.tsx src/components/tray-quota-panel.test.tsx` | exit 0 after implementation |
| Backend focused | `cargo test test_detect_auth_mode_priority` | exit 0 after implementation |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:

- The current code differs materially from the Current state description
- Required test infrastructure is missing and cannot be added safely
- A dependency or external service cannot be isolated by mock/stub
- Verification still fails after the configured fix loop
- Completing the task would require out-of-scope changes
