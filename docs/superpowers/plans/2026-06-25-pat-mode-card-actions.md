# PAT Mode Card Actions Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the global PAT mode control account switching and disable Relay, Handoff, and Sync on every account card.

**Architecture:** Treat the global `authMode` value as the sole operation-mode switch. Account-level auth labels remain descriptive metadata and do not select the switching workflow.

**Tech Stack:** React, TypeScript, Vitest, Tauri

---

## Chunk 1: Global PAT Mode Behavior

### Task 1: Route Switch by Global Mode

**Files:**
- Modify: `apps/desktop/src/App.tsx`
- Test: `apps/desktop/src/App.handoff.test.tsx`

- [x] Change `handleSwitchAccount` to call `switchToPatAccount` whenever global mode is `pat`.
- [x] Keep the existing login flow whenever global mode is `oauth`.
- [x] Add tests covering both branches.

### Task 2: Disable Unsupported Card Actions

**Files:**
- Modify: `apps/desktop/src/routes/views.tsx`
- Test: `apps/desktop/src/routes/handoff.test.tsx`

- [x] Disable Relay, Handoff, and Sync when global mode is `pat`.
- [x] Preserve all existing disable conditions.
- [x] Add a rendering test asserting all three controls are disabled in PAT mode.

### Task 3: Verify and Package

- [x] Run `npm test -- --run` in `apps/desktop`.
- [x] Run `npm run build` in `apps/desktop`.
- [x] Run `cargo test` in `apps/desktop/src-tauri`.
- [x] Run `npm run tauri:build` and replace `/Applications/LAM.app`.
