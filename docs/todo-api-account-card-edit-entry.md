# Todo: API account card configuration editor entry

> Executor instructions: Follow this todo step by step. Write the tests from
> Test design before implementation, confirm the expected failure, then
> implement and verify. Stop instead of bypassing a failed UI or security
> contract.

## Status

- **Priority**: P0
- **Effort**: S
- **Risk**: MEDIUM
- **Depends on**: `docs/todo-native-codex-api-account-auth-editing.md`
- **Category**: feature/UX
- **Planned at**: current working tree

## Why this matters

**Background**: Native Responses API accounts already have a redacted detail
and update service plus a reusable editor, but the editor is discoverable only
from the advanced Providers page.

**Current state**: Overview account cards identify External API accounts and
show a menu button. They do not expose the requested pencil action next to that
menu, so account configuration appears non-editable from the primary workflow.

**Impact**: Users cannot naturally find the URL/protocol/key configuration for
the account they are looking at, and installed behavior does not match the
account-centric requirement.

**What improves**: Each External API card gets a dedicated pencil action that
opens the existing account editor in place. Normal login accounts remain
unchanged.

## Scope

**In scope**:

- Add an External-API-only pencil button to Overview account cards.
- Connect it to the existing redacted API-account detail/update store actions.
- Reuse `ApiAccountConnectionEditor` in an application-level modal.
- Cover visibility, event propagation, loading, display, save, and close paths.

**Out of scope**:

- Returning or displaying the existing API key value.
- Changing backend DTOs or credential storage.
- Editing Chat Completions shared Provider definitions from the account card.
- Packaging/installing the macOS application.

## Design

- `Overview` and `Accounts` receive an explicit `editApiAccount(account)`
  callback; UI components do not access stores directly.
- A pencil icon is rendered immediately before More options only when the
  account id is in `apiAccountIds`.
- The button has an account-specific accessible name and stops card click
  propagation before invoking the callback.
- `App` owns the selected profile id, asks the Provider store to load its
  redacted connection, and renders the existing editor in a modal.
- Closing clears both selected id and cached connection. Successful save uses
  the existing revision-aware update action and then closes. Load/update errors
  remain represented by existing store/application error handling.
- The modal shows protocol as metadata and never renders the existing key.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|---|---|---|---|
| T1 | Account card editor entry | External API cards open and save the existing configuration editor | 验证成功 |

### T1: Account card editor entry

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: The implemented API-account editor must be reachable from the account
card shown in the user's primary Overview workflow.

**What to do**:

- Extend account-card props with an explicit edit callback.
- Render the requested pencil action only for External API accounts.
- Add App-level loading/editor modal orchestration using existing store APIs.
- Keep normal accounts, card selection, and Providers-page editing unchanged.

**Logic design**:

- Card action calls `stopPropagation` and passes the exact account.
- App clears stale connection data before loading the selected profile.
- The modal renders a deterministic loading state until detail matches the
  selected profile.
- Save delegates to `updateApiAccountConnection`; success clears editor state.
- Cancel/close clears cached connection and selected profile.

**Test design**:

- Component test: External API card has an accessible pencil button immediately
  before More options; a normal account does not.
- Component test: clicking the pencil invokes only the edit callback and does
  not select the card.
- Integration test: clicking the pencil requests the exact profile detail and
  opens a modal showing base URL, Responses protocol, model, and redacted key
  status.
- Integration test: editing URL and submitting calls the existing update API
  without an `apiKey` when the key field is empty, then closes the modal.
- Error/edge behavior remains covered by the existing reusable editor tests.

**Acceptance**:

- `npm test -- --run src/routes/handoff.test.tsx src/App.handoff.test.tsx src/components/provider-center.test.tsx`
- Changed-file ESLint, TypeScript build, and `git diff --check` exit 0.

**Failure-first record**: Both new tests failed because the account card had
only More options and no accessible pencil action. The integration test could
therefore not open the configuration modal. The same combined run also exposed
an unrelated existing order-dependent Settings timeout test; the 68 other
selected tests passed and that baseline test will be rechecked after isolation.

**Done criteria**:

- [x] Tests were written before implementation
- [x] Expected failure was recorded
- [x] Implementation follows the design
- [x] Focused tests pass
- [x] Changed-file lint and build pass
- [x] Task overview and status are `验证成功`

**Verification record**: 71 focused tests passed. The TypeScript production
build, full frontend ESLint, changed-file Prettier, and `git diff --check` all
exit 0. The existing Settings timeout test was corrected to select its Advanced
tab before querying the Advanced-only input; its underlying controlled draft
was also made lint-compliant without a state-setting effect.

## Test plan

- Normal: open, display, URL-only save, close.
- Edge: editor detail still loading; normal account card.
- Invalid/error: delegated to the already-tested reusable editor/store.
- State: pencil click does not select the underlying card; close clears cache.
- Security: existing key is represented only as configured/missing.

## Verification commands

| Purpose | Command | Expected on success |
|---|---|---|
| Focused tests | `npm test -- --run src/routes/handoff.test.tsx src/App.handoff.test.tsx src/components/provider-center.test.tsx` | exit 0 after implementation; expected failure first |
| Build/lint | `npx eslint src/App.tsx src/App.handoff.test.tsx src/routes/views.tsx src/routes/handoff.test.tsx && npm run build` | exit 0 |
| Diff | `git diff --check` | exit 0 |

## Done criteria

- [x] Every task checklist is complete
- [x] Every task has exactly one status checked and it is `验证成功`
- [x] Task overview shows `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

- External API account identity cannot be determined from existing bindings.
- Opening the editor would require exposing an existing API key value.
- The reusable backend/editor contract differs materially from this design.
- Five implementation/verification repair loops fail.
