# Todo: Save Gateway account model selections

- **Status**: 验证成功
- **Scope**: One vertical behavior: save the model allowlist/default of an exclusively bound API Provider from the account editor.
- **Baseline**: `release/v0.4.0`, `1ca2b47f79318fb45861cadbc5256dc7cbae421f`; clean worktree.

## Why

Gateway account Apply models only changes a draft. Save Provider rejects model changes while attached. The saved Provider, profile binding, Gateway snapshot and Codex catalog must agree after a successful save.

## Behavior

- **Input**: Existing Provider revision, nonempty selected models and a default included in that list.
- **Output**: Apply models persists once; Provider, bound account config, catalog and Gateway binding use the requested selection, with no need to change the default.
- **Constraints/errors**: Only model/settings edits of an exclusively bound account use automatic rebind. Shared Providers and connection/auth changes retain explicit rebind requirements. Stale revisions, invalid selections and config drift fail without partial writes. Recover interrupted saves using the existing attach journal. Preserve unrelated profiles, settings and secrets.

## Test first

- Normal: Add/remove models with unchanged default; select a new default; verify persisted catalog, config and authenticated Gateway snapshot.
- Edge/invalid: Empty/duplicate model lists, missing default, shared bindings, connection changes, stale store revision and stale config.
- Error/state: Catalog/config failure restores Provider and catalog; precommit interruption rolls back on recovery; postcommit interruption completes on recovery; retries use refreshed state.
- UI: Gateway Apply models calls save with the selected set/default; errors remain visible with the draft retained; duplicate submissions are disabled; create-provider drafts remain local.
- Expected RED: Backend rejects the model update with PROVIDER_HAS_BINDINGS_REBIND_REQUIRED; Gateway picker never calls onSave.

## Implementation

Extend the existing attach transaction with optional Provider/catalog changes and recovery data; route exclusive attached model updates through it. Reuse the Provider editor's validation/persistence for Apply models and propagate errors to the picker. No proxy extraction, live account changes or unrelated connection-edit expansion.

## Verification

- Focused: Rust provider API/attach transaction tests and frontend Provider editor/account flow tests.
- Regression: Full frontend/Rust suites, frontend build/lint, Rust fmt/clippy, UI smoke and diff checks; record pre-existing failures separately.

## Done

- [x] Behavioral RED observed before implementation.
- [x] Both reported save cases persist all representations.
- [x] Failure and interruption recovery verified.
- [x] Focused and relevant regression checks pass.

## Evidence

- RED: Gateway picker tests observed zero `onSave` calls for both defaults; backend test returned `PROVIDER_HAS_BINDINGS_REBIND_REQUIRED`. Additional tests exposed lost context windows, swallowed modal errors, stale account-card data, stale rollback revisions and replay accepted as success.
- GREEN: 276 frontend tests passed. Full Rust suite: 578 passed, 5 intentionally ignored. Production frontend build and Rust Clippy (`-D warnings`) passed.
- Recovery: journal v2 records Provider/catalog before and after values plus a private config-backup path; precommit recovery restores the exact config and catalog, postcommit recovery completes cleanup, and ticket replay/conflicting catalog paths fail closed. Legacy journal v1 remains readable. Raw config contents and API keys are not added to the journal.
- Boundaries: automatic updates require a single exclusively owned account binding. Shared Provider and connection changes still require explicit rebind. Reopen Codex sessions to load the changed profile configuration.
- Test mapping: `provider_api_v2.rs` → `provider_phase4_api_account.rs` and existing `provider_api_v2.rs` tests; `provider_attach_transaction.rs`/`provider_attach_update.rs` → `provider_attach_transaction.rs` tests; `App.tsx` → `App.handoff.test.tsx`; Provider editor/picker → `provider-center.test.tsx`; Provider store → `providers-v2.test.ts`.
- Existing repository checks: ESLint on the original HEAD App reproduced the same `set-state-in-effect` error at line 284 and unused `authCommand` warning. An isolated HEAD checkout reproduced the UI smoke `empty state` failure (the check expects `No sessions.`, while the UI says `No sessions match this query.`). Global Rust fmt still reports the unchanged ignored manual test in `provider_auth_runtime.rs`; changed Rust files are formatted. These baseline issues were not broadened into this fix.
- Live data: production account configuration and credentials were not changed; all mutation/recovery tests used temporary profile roots.
