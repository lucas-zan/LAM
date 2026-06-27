# PAT Active Auth and Quota Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Determine the active PAT profile from `tokens.account_id`, keep main non-switchable, and verify quota requests use each profile's own `CODEX_HOME`.

**Architecture:** The Rust account service reads only the last four characters of `tokens.account_id` and exposes a boolean active-auth result. A non-main profile is active only when its suffix uniquely matches the main `~/.codex/auth.json` suffix; missing or ambiguous identities produce no active profile. Quota collection continues to launch Codex app-server with each account directory as `CODEX_HOME`.

**Tech Stack:** Rust, Tauri, React, TypeScript, Vitest

---

## Chunk 1: Active PAT Identity

### Task 1: Add Backend Identity Matching

**Files:**
- Modify: `apps/desktop/src-tauri/src/services/account.rs`
- Test: `apps/desktop/src-tauri/tests/integration_pat_accounts.rs`

- [x] Add `is_active_auth` to account results.
- [x] Parse `tokens.account_id` and retain only its final four characters.
- [x] Mark exactly one non-main account active when it uniquely matches main.
- [x] Mark none active for missing identity or duplicate suffixes.
- [x] Verify Switch changes the backend-derived active account.

### Task 2: Render Backend Active State

**Files:**
- Modify: `apps/desktop/src/lib/types.ts`
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/routes/views.tsx`
- Test: `apps/desktop/src/routes/handoff.test.tsx`
- Test: `apps/desktop/src/App.handoff.test.tsx`

- [x] Disable main Switch unconditionally.
- [x] Disable only the uniquely active non-main profile Switch in PAT mode.
- [x] Display `Active auth: Unrecognized` when no unique match exists.
- [x] Refresh accounts after Switch so UI uses backend identity matching.

## Chunk 2: Quota Isolation

### Task 3: Prove Profile-Specific CODEX_HOME

**Files:**
- Test: `apps/desktop/src-tauri/tests/phase1_core.rs`

- [x] Add or strengthen a test proving quota app-server receives each profile's own `CODEX_HOME`.
- [x] Verify main and non-main profiles do not share auth paths.

## Chunk 3: Verification

- [x] Run full frontend tests and build.
- [x] Run full Rust tests.
- [x] Build and reinstall `/Applications/LAM.app`.
- [x] Verify main Switch is disabled and Active auth follows unique suffix matching.
