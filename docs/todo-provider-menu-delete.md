# Todo: Delete Provider Connections From Providers Menu

- **Status**: 验证成功
- **Scope**: Allow an unbound V2 Provider connection to be deleted from the Providers page without deleting any Codex account home.
- **Baseline**: `687bf50`

## Why

The Providers page can create, edit, attach, and detach V2 connections but cannot remove an obsolete connection. Provider deletion must remain distinct from API-account deletion because the latter removes the managed Codex home and wrapper.

## Behavior

- **Input**: User chooses Delete on a Provider card and confirms; client submits provider id plus the observed V2 store revision.
- **Output**: The unbound Provider is removed, its credential reference is revoked when applicable, and the Providers list refreshes.
- **Constraints/errors**: A Provider with bindings is not deletable; stale revisions and missing providers fail without deleting accounts or bindings.

## Test first

- Normal: Provider card exposes Delete and confirmation invokes the V2 delete action; backend removes an unbound Provider.
- Edge/invalid: Missing Provider and stale store revision are rejected.
- Error/state conflict: Bound Provider is rejected with `PROVIDER_IN_USE` and remains stored.
- Expected RED: UI lacks the Delete callback/action and the V2 backend delete contract does not exist.

## Implementation

Add a revision-checked V2 delete request/service/command, client/store action, and explicit confirmation dialog. Do not call API-account deletion and do not remove `~/.codex-*` or wrapper files.

## Verification

- Focused: Provider center Vitest; provider API Rust integration tests.
- Regression/lint/build: desktop TypeScript build and relevant provider API suite.

## Evidence

- RED: `npm test -- --run src/components/provider-center.test.tsx` failed because `Delete Provider` was absent; focused Rust test failed to compile because the V2 delete request/service did not exist.
- GREEN: 44 focused UI/store tests passed; 21 provider API integration tests passed; desktop production build and targeted ESLint passed; `git diff --check` passed.
- Changed files: V2 provider delete DTO/service/command, TypeScript invoke/store action, Providers card confirmation UI, and focused tests.
- Remaining limitation: Repository-wide ESLint remains blocked by the pre-existing `react-hooks/set-state-in-effect` error at `apps/desktop/src/App.tsx:278`.

## Done

- [x] Test was written first and RED observed, or exception recorded.
- [x] Implementation stays in scope.
- [x] Focused and relevant regression checks pass.
- [x] Status is 验证成功.
