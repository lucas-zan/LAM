# Todo: RPG-006 Normalize frontend lint and format baseline

> Executor instructions: Complete T1 before T2. For T1, use the lint command as
> the red test and add behavior-focused characterization/regression tests before
> semantic refactors. For T2, use format:check as the red test and keep the
> rewrite mechanical. Run all verification commands before changing status.

## Status

- **Status**: 验证成功
- **Priority**: P0
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: RPG-000
- **Category**: refactor/quality
- **Parent tracker**: [`todo-remote-provider-gateway.md#rpg-006-normalize-the-existing-frontend-lint-and-format-baseline`](./todo-remote-provider-gateway.md#rpg-006-normalize-the-existing-frontend-lint-and-format-baseline)
- **Planned at**: 2026-07-10 workspace state

## Why this matters

**Background**: Phase 0 must start from enforceable quality commands. RPG-000
restored the test/build baseline, but validation showed the declared frontend
lint and format commands already fail before Gateway work begins.

**Current state**:

- `npm run lint` reports three errors:
  - synchronous state update inside an effect in `App.tsx`;
  - synchronous pagination resets inside an effect in `routes/usage.tsx`;
  - a return statement inside `finally` in `stores/usage.ts`.
- It also reports seven warnings covering unused state/test variables, unstable
  fallback callbacks/diagnostics, an unused prop, and a dead tray component.
- `npm run format:check` reports 33 existing files.

**Impact**: New feature diffs cannot distinguish newly introduced style failures
from historical debt, and CI-quality commands cannot be used as a gate.

**What improves**: Semantic lint issues become tested, small, explicit changes;
formatting becomes one reviewable mechanical rewrite; G0 can enforce lint and
format for every later issue.

## Scope

**In scope**:

- Semantic lint fixes in:
  - `apps/desktop/src/App.tsx`
  - `apps/desktop/src/routes/usage.tsx`
  - `apps/desktop/src/stores/usage.ts`
  - `apps/desktop/src/components/tray-quota-panel.tsx`
  - `apps/desktop/src/App.handoff.test.tsx`
- Tests in:
  - `apps/desktop/src/App.handoff.test.tsx`
  - `apps/desktop/src/routes/usage.test.tsx`
  - `apps/desktop/src/stores/usage.test.ts`
- Mechanical Prettier formatting under `apps/desktop/src/`.
- Master/execution TODO state updates.

**Out of scope**:

- Product redesign.
- Remote Provider Gateway behavior.
- Changing pagination semantics, auth-mode semantics, quota semantics, or API
  contracts.
- Adding a new formatter/linter or weakening existing rules.
- Formatting Rust, docs outside the current TODOs, generated files, or vendored
  dependencies.

## Design

### Semantic lint corrections

`App.tsx`:

- Initialize create-account mode from current mode availability.
- Update create-account mode in the explicit availability-change callback.
- Remove the synchronous effect that derives one React state from another.
- Remove the unused PAT token React value/setter because the form already reads
  the password through `FormData`.
- Preserve existing Profile-only, PAT-only, and both-mode behavior.

`routes/usage.tsx`:

- Replace effect-driven pagination reset with query-keyed pagination state or an
  equivalent explicit design that reads offset zero immediately when filters,
  scope, archive mode, sort, or window change.
- Keep next/previous pagination behavior.
- Make fallback loader/refresh callbacks stable.
- Use a stable empty diagnostics value.
- Stop destructuring an unused legacy refresh prop while preserving the public
  prop until a separate contract change.

`stores/usage.ts`:

- Do not return from `finally`.
- Clear loading state only when the request ID is still current.
- Preserve stale-response protection and ensure an older request cannot clear a
  newer request's loading flag.

Other warnings:

- Remove the unused dead tray bucket component and now-unused type imports.
- Keep the CPA test's `window.confirm` mock only if behavior needs it; otherwise
  remove the unused binding without changing test intent.

### Mechanical formatting

- Run the repository's existing Prettier write command only after T1 is verified.
- Treat T2 as a mechanical change: do not hand-edit logic while formatting.
- Re-run the full suite/build/lint to prove formatting preserves behavior.

## Tasks

### Task overview

| ID  | Task                                      | Acceptance summary                                               | Status |
| --- | ----------------------------------------- | ---------------------------------------------------------------- | ------ |
| T1  | Resolve semantic lint errors and warnings | `npm run lint` exits 0 and focused behavior suites pass          | 验证成功 |
| T2  | Normalize existing frontend formatting    | `npm run format:check` exits 0 and full tests/build remain green | 验证成功 |

### T1: Resolve semantic lint errors and warnings

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:

The lint failures identify state/effect and request-finalization patterns that
can produce unnecessary renders or stale state bugs. They require behavior-aware
refactoring, not rule suppression.

**What to do**:

- Re-run `npm run lint` and record the exact red output after this TODO exists.
- Add a UsagePage test proving a paginated request resets to offset zero after a
  query/filter input changes.
- Confirm existing mode-availability tests cover Profile-only and PAT-only
  create modes; strengthen them only if the current assertions cannot detect the
  effect removal regression.
- Confirm/add a usage-store test proving a stale request cannot clear the current
  request's loading state.
- Implement only the semantic lint corrections in the Design section.
- Do not run Prettier across all source files during T1.

**Logic design**:

- Prefer explicit event/state transitions over synchronous effects.
- Model pagination as state associated with its query key, so a changed query
  reads offset zero before the next load effect.
- Use module constants or `useCallback/useMemo` for stable fallbacks.
- In `finally`, guard the state update with an `if` instead of returning.
- Remove dead/unused code instead of prefixing names unless the value is required
  for behavior.

**Test design**:

- Red test: `npm run lint` must fail with the recorded three errors before
  implementation.
- Red evidence on 2026-07-10: lint exited 1 with 3 errors and 7 warnings in the
  files listed under Current state.
- Characterization: existing App mode-availability tests remain green.
- New pagination regression:
  - navigate calls to the next page;
  - change search/filter or rerender with a new scope/window;
  - latest load request has offset zero.
- Store state/conflict:
  - start an older and newer request for the same section;
  - settle the older request;
  - newer request remains loading until it settles.
- Existing full tests cover CPA export and tray rendering after dead-code removal.
- If a new characterization test passes before implementation, record that the
  linter—not behavior—is the intentional red signal for this behavior-preserving
  refactor.
- Characterization evidence on 2026-07-10: the new pagination and stale-request
  tests passed with the existing behavior; ESLint remains the intentional red
  signal for the refactor.

**Acceptance**:

- `cd apps/desktop && npm run lint` exits 0.
- Focused App/Usage route/Usage store/tray tests exit 0.
- `cd apps/desktop && npm test` exits 0.
- `cd apps/desktop && npm run build` exits 0.
- Only T1 semantic files/tests and TODOs are changed before T2.

**Done criteria**:

- [x] Red lint evidence recorded after TODO creation
- [x] Behavior tests added/confirmed before implementation
- [x] Test failure exception documented where lint is the red signal
- [x] Semantic implementation follows the Design section
- [x] Focused tests pass (4 files, 67 tests)
- [x] Full frontend suite and build pass (14 files, 129 tests)
- [x] Full lint passes with no errors or warnings
- [x] T1 status and overview are `验证成功`

**Validation evidence on 2026-07-10**:

- `npm run lint`: exit 0, no errors or warnings.
- Focused App/Usage route/Usage store/tray suites: 4 files, 67 tests passed.
- Full frontend suite: 14 files, 129 tests passed.
- Frontend production build: passed.

### T2: Normalize existing frontend formatting

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:

The current format check reports 33 files, so later issues cannot use it as a
regression gate.

**What to do**:

- Re-run `npm run format:check` after T1 and record the red file list.
- Run `npm run format` once.
- Inspect the diff for logic changes, generated files, secrets, or files outside
  `apps/desktop/src/`.
- Do not make semantic fixes during the formatting pass.

**Logic design**:

- Formatting is fully delegated to the repository's pinned Prettier.
- Any semantic issue discovered while reviewing the formatted diff goes back to
  T1 or a new issue; it is not fixed inside T2.

**Test design**:

- Red test: `npm run format:check` fails before the formatting pass.
- No new behavior test is required because this is a mechanical rewrite; full
  tests/build/lint are the preservation proof.
- Red evidence on 2026-07-10: `npm run format:check` exited 1 and reported the
  same 33 source files recorded during planning.

**Acceptance**:

- `cd apps/desktop && npm run format:check` exits 0.
- `cd apps/desktop && npm run lint` exits 0.
- `cd apps/desktop && npm test` exits 0 twice.
- `cd apps/desktop && npm run build` exits 0.
- `git diff --check` exits 0.

**Done criteria**:

- [x] Red format evidence recorded
- [x] Mechanical formatter ran without manual semantic edits
- [x] Formatted diff is limited to `apps/desktop/src/` and intentional TODOs
- [x] Format and lint checks pass
- [x] Full frontend suite passes twice (14 files, 129 tests per run)
- [x] Frontend build and diff checks pass
- [x] T2 status and overview are `验证成功`

**Validation evidence on 2026-07-10**:

- Pinned Prettier rewrote the 33 files reported by the red format check.
- `npm run format:check`: exit 0.
- `npm run lint`: exit 0, no errors or warnings.
- Full frontend suite passed twice: 14 files, 129 tests per run.
- Frontend production build and `git diff --check`: passed.

## Test plan

- App normal/state: availability changes choose valid auth and create modes.
- Usage normal: load active section with correct request.
- Usage edge/state: page offset resets for a new query key.
- Usage conflict: stale section completion cannot clear current loading state.
- CPA/tray regression: removing unused bindings/dead component changes no
  accessible behavior.
- Mechanical preservation: all frontend tests/build/lint pass after formatting.

## Verification commands

| Purpose             | Command                                                                   | Expected                          |
| ------------------- | ------------------------------------------------------------------------- | --------------------------------- |
| Red/full lint       | `cd apps/desktop && npm run lint`                                         | fails before T1; exits 0 after T1 |
| Focused App         | `cd apps/desktop && npm test -- src/App.handoff.test.tsx`                 | exit 0                            |
| Focused Usage route | `cd apps/desktop && npm test -- src/routes/usage.test.tsx`                | exit 0                            |
| Focused Usage store | `cd apps/desktop && npm test -- src/stores/usage.test.ts`                 | exit 0                            |
| Focused tray        | `cd apps/desktop && npm test -- src/components/tray-quota-panel.test.tsx` | exit 0                            |
| Red/full format     | `cd apps/desktop && npm run format:check`                                 | fails before T2; exits 0 after    |
| Full frontend run 1 | `cd apps/desktop && npm test`                                             | exit 0                            |
| Full frontend run 2 | `cd apps/desktop && npm test`                                             | exit 0                            |
| Frontend build      | `cd apps/desktop && npm run build`                                        | exit 0                            |
| Diff quality        | `git diff --check`                                                        | exit 0                            |

## Done criteria

- [ ] T1 and T2 Done criteria are fully checked
- [ ] T1 and T2 each have exactly one checked status: `验证成功`
- [ ] Task overview shows both tasks as `验证成功`
- [ ] RPG-006 and G0 are `验证成功` in the master tracker
- [ ] No STOP condition remains unresolved

## STOP conditions

Stop and report if:

- a semantic lint fix requires an unestablished product behavior decision;
- pagination or request-race behavior cannot be isolated with deterministic tests;
- Prettier changes files outside `apps/desktop/src/`;
- formatting overlaps unrelated user source changes;
- full tests/build remain red after five focused fix attempts;
- resolving lint requires disabling or weakening a rule.
