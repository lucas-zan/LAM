# Todo: RPG-000 Restore green repository baseline

> Executor instructions: Follow this todo step by step. Use the existing failing
> regression test as the required red test, then implement the smallest behavior
> correction. Run every verification command and confirm the expected result
> before marking the task complete. Stop and report if a STOP condition occurs.

## Status

- **Status**: 验证成功
- **Priority**: P0
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: bugfix
- **Parent tracker**: [`todo-remote-provider-gateway.md#rpg-000-restore-a-green-repository-baseline`](./todo-remote-provider-gateway.md#rpg-000-restore-a-green-repository-baseline)
- **Planned at**: 2026-07-10 workspace state

## Why this matters

**Background**: Remote Provider Gateway development requires a green baseline so
new failures can be attributed to the active issue. Rust tests are green, but the
frontend handoff suite has one reproducible PAT account-action failure.

**Current state**: In PAT mode, `Accounts` treats `main` as if it were the active
backend-auth account when choosing the primary card action:

```text
(isActiveAccount || account.id === 'main') ? Reset Quota : Switch
```

The backend-derived `isActiveAuth` flag identifies `codex-a` as active in the
failing fixture, while `main` is explicitly inactive. The current branch renders
`Reset Quota` for `main`, so the existing test cannot find the required disabled
`Switch to this account` button.

**Established requirement**: The existing PAT active-auth design requires:

- `main` Switch is always disabled;
- only the uniquely backend-matched non-main profile is active;
- the active non-main profile does not render Switch;
- an inactive non-main profile renders an enabled Switch.

**Impact**: The frontend suite is red and the UI conflates the reserved main auth
slot with the actual backend-matched active account.

**What improves**: PAT card actions again follow `isActiveAuth` as the only active
identity signal, while `main` remains visibly non-switchable.

## Scope

**In scope**:

- `apps/desktop/src/routes/views.tsx` PAT primary-action branch.
- Existing regression coverage in `apps/desktop/src/routes/handoff.test.tsx`.
- Focused and full frontend verification.
- Master/execution TODO status updates.

**Out of scope**:

- Remote Provider Gateway implementation.
- Backend PAT identity matching.
- Quota reset behavior for the actual active account.
- Tray UI behavior unless verification proves the same regression exists there.
- General account-card refactoring or formatting cleanup.

## Design

Expected behavior:

- In PAT mode, `isActiveAccount` is derived only from
  `account.isActiveAuth === true`.
- The actual active account renders `Reset Quota`.
- Every inactive account follows the Switch branch.
- `main` reaches the Switch branch but the button is disabled and explains that
  the main profile is the reserved active-auth slot.
- Other inactive profiles render an enabled Switch.

Inputs and constraints:

- `isActiveAuth` is optional on legacy frontend objects; only explicit `true` is
  active.
- There may be no uniquely active account.
- Card actions must stop click propagation as before.
- Existing reset and switch callbacks must remain unchanged.

Boundary:

- Backend remains authoritative for identity.
- The React view only selects which existing action to render.
- No new state or API call is introduced.

Error/state behavior:

- Missing or false `isActiveAuth` never grants active-account reset behavior.
- Reserved `main` never invokes `switchAccount` because its button is disabled.

## Tasks

### Task overview

| ID  | Task                                     | Acceptance summary                                                             | Status   |
| --- | ---------------------------------------- | ------------------------------------------------------------------------------ | -------- |
| T1  | Restore backend-derived PAT card actions | Existing regression test and full frontend suite pass; main Switch is disabled | 验证成功 |

### T1: Restore backend-derived PAT card actions

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:

The quota-reset UI change accidentally promoted `main` into the reset branch even
when the backend identifies another account as active.

**What to do**:

- Re-run the focused existing test and record the expected red failure.
- Change only the PAT action condition in `views.tsx` so reset is selected by
  `isActiveAccount` alone.
- Preserve the Switch button's existing `main` disabled/title behavior.
- Do not alter the existing test unless its public accessibility query is proven
  invalid after the behavior is corrected.

**Logic design**:

- Keep `isActiveAccount` calculation unchanged.
- Replace the reset-branch predicate with `isActiveAccount`.
- Let inactive `main` enter the existing Switch branch, where
  `disabled={account.id === 'main'}` already enforces the requirement.
- Continue to use the same callbacks and button components.

**Test design**:

- Red test already exists:
  `handoff.test.tsx > uses backend auth identity for PAT switch availability`.
- Expected initial failure: the main account article has no
  `Switch to this account` element because it renders Reset Quota.
- Red evidence on 2026-07-10: focused Vitest exited 1 with 16 passing tests and
  this single expected failure at `handoff.test.tsx:243`.
- Normal: uniquely active non-main account has no Switch.
- Edge: inactive `main` has a disabled Switch.
- State behavior: inactive non-main account has an enabled Switch.
- Full frontend suite must confirm quota reset and Profile-mode visibility do not
  regress.

**Acceptance**:

- Focused:
  `cd apps/desktop && npm test -- src/routes/handoff.test.tsx` exits 0 after
  implementation and fails for the expected reason before implementation.
- Full frontend:
  `cd apps/desktop && npm test` exits 0 in two consecutive runs.
- Build and changed-file quality:
  `cd apps/desktop && npm run build`,
  focused ESLint for `src/routes/views.tsx`, and `git diff --check` exit 0.
- Baseline exception: full lint currently has three unrelated errors and full
  format check reports 33 historical files. RPG-006 owns normalization before G0.
- Worktree contains no test-generated tracked fixture changes.

**Done criteria**:

- [x] Existing regression test was run and confirmed to fail for the expected
      reason before implementation
- [x] Implementation follows this task's Logic design and stays inside scope
- [x] Focused verification passes
- [x] Full frontend suite passes twice
- [x] Frontend build passes
- [x] Changed-file lint and diff checks pass
- [x] Test runs leave no generated tracked changes
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

**Verification evidence**:

- Focused handoff suite: 17/17 passed.
- Full frontend suite: 127/127 passed in two consecutive runs.
- Frontend production build: passed.
- Changed-file ESLint and `git diff --check`: passed.
- Rust suite: 150 passed, 2 ignored, 0 failed.
- Full frontend lint/format baseline failures were confirmed unrelated and are
  owned by RPG-006 before G0.

## Test plan

- Normal behavior: active non-main account renders reset, not switch.
- Edge behavior: inactive main renders disabled switch.
- State behavior: inactive non-main renders enabled switch.
- Regression behavior: Profile mode still renders no Switch actions.
- Reset behavior: active-account reset confirmation and zero-credit disable tests
  continue to pass.

## Verification commands

| Purpose             | Command                                                      | Expected                                                            |
| ------------------- | ------------------------------------------------------------ | ------------------------------------------------------------------- |
| Red/focused test    | `cd apps/desktop && npm test -- src/routes/handoff.test.tsx` | fails before implementation at main Switch assertion; exits 0 after |
| Full frontend run 1 | `cd apps/desktop && npm test`                                | exit 0                                                              |
| Full frontend run 2 | `cd apps/desktop && npm test`                                | exit 0                                                              |
| Frontend build      | `cd apps/desktop && npm run build`                           | exit 0                                                              |
| Changed-file lint   | `cd apps/desktop && npx eslint src/routes/views.tsx`         | exit 0                                                              |
| Diff whitespace     | `git diff --check`                                           | exit 0                                                              |
| Worktree safety     | `git status --short`                                         | only intentional docs/source changes                                |

## Done criteria

- [x] T1's own Done criteria checklist is fully checked
- [x] T1 has exactly one checked Status value and it is `验证成功`
- [x] Task overview shows T1 as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:

- the focused test no longer reproduces the documented failure before source
  changes;
- backend `isActiveAuth` semantics differ from the established PAT design;
- fixing the baseline requires changing quota reset, backend identity, or tray
  behavior outside this issue;
- an unrelated user change overlaps `views.tsx` or the regression test;
- verification still fails after five focused fix attempts.
