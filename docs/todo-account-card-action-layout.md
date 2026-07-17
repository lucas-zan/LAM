# Todo: Account card API actions and layout switch

> Executor instructions: Follow this todo step by step. Generate tests from the
> "Test design" section before implementation. Run each verification command and
> confirm the expected result before moving to the next task.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: existing API Account editor and compact-card preference
- **Category**: feature/refactor
- **Planned at**: current workspace

## Why this matters

**Background**: API Accounts authenticate with a configured API key and endpoint, so the
account-card `Login` action is misleading. Their editor is currently a separate pencil in
the card header. Users also need a quick Accounts-level switch between compact overflow
menus and a fully expanded action layout.

**Current state**: `Accounts` always renders the overflow menu, renders an API-only pencil
beside it, and receives the persisted `compactButtons` preference only as a read-only prop.
Expanded mode reveals only a subset of menu operations.

**Impact**: API users may start an irrelevant Codex login flow, configuration actions are
split between two places, and the display preference can only be changed from Settings.

**What improves**: API configuration replaces Login in the same action system. A single
Accounts header control switches between a compact two-button-plus-menu card and an expanded
card containing all applicable operations.

## Scope

**In scope**:
- Remove the API Account header pencil.
- Replace API Account Login with `View&Edit` in compact menus.
- Add an Accounts header layout toggle backed by the existing persisted preference.
- In expanded mode, hide overflow menus and render every applicable management action below
  the card without duplicating primary actions.
- Preserve normal account Login and all existing action guards/confirmations.

**Out of scope**:
- Changing API Account editor fields or native contracts.
- Changing Relay/Handoff semantics.
- Redesigning the Settings preference.

## Design

`compactButtons` remains the single source of truth; `setCompactButtons` is passed from App
through Overview to Accounts. The header toggle exposes `aria-pressed` and an intention-based
label. Compact cards retain the overflow menu. Expanded cards omit that menu and show the
same applicable management operations as buttons below the card. API Accounts never receive
Login: their corresponding management action invokes `editApiAccount`. Normal OAuth/PAT
Login and update behavior remains unchanged.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Correct API Account actions | No header pencil/API Login; editor action lives in menu | 验证成功 |
| T2 | Add layout switch and expanded actions | Toggle persists mode; expanded cards flatten all actions | 验证成功 |
| T3 | Regression and documentation | Tests, lint, build and smoke pass | 验证成功 |

### T1: Correct API Account actions

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: API key accounts do not require interactive Codex login.

**What to do**:
- Remove the card-header pencil for API Accounts.
- Render `View&Edit` in their compact overflow menu.
- Ensure normal accounts retain Login and API Accounts do not expose it.

**Logic design**:
- Branch on the existing `apiAccountIds` membership, not provider-name heuristics.
- Invoke the existing `editApiAccount(account)` callback and close the menu first.

**Test design**:
- Assert an API card header has only `More options` and no editor pencil.
- Open its menu, assert editor action exists and Login does not, then verify the callback.
- Open a normal account menu and assert Login still exists.
- Expected initial failure: header pencil exists and API menu still contains Login.

**Acceptance**:
- `npm test -- --run src/routes/handoff.test.tsx`

**Done criteria**:
- [x] Tests written before implementation and failed on the existing API header pencil (2026-07-17).
- [x] API/normal account action behavior matches the design.
- [x] Focused tests pass (21/21).
- [x] Status is `验证成功`.

### T2: Add layout switch and expanded actions

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Users need to change card density where they see the cards.

**What to do**:
- Pass `setCompactButtons` from App through Overview to Accounts.
- Add the layout toggle immediately after the Accounts heading.
- Hide overflow menus in expanded mode and flatten applicable actions below each card.

**Logic design**:
- The toggle inverses the persisted `compactButtons` value.
- Compact mode preserves current primary actions and overflow behavior.
- Expanded API cards show Switch Model, Handoff, View&Edit, Rename and Delete once each.
- Expanded normal OAuth cards expose Relay/Handoff plus applicable Reset, Login, Rename and
  Delete operations; existing disabled rules and confirmations remain intact.

**Test design**:
- Assert the header toggle label/state and callback direction in both modes.
- Assert compact mode has overflow menus.
- Assert expanded mode has no overflow menu and shows flattened API actions.
- Verify clicking expanded API edit invokes the existing callback without selecting the card.
- Expected initial failure: no Accounts header toggle and expanded mode retains overflow.

**Acceptance**:
- `npm test -- --run src/routes/handoff.test.tsx src/App.handoff.test.tsx`

**Done criteria**:
- [x] Tests written before implementation and failed because the Accounts layout toggle does not exist (2026-07-17).
- [x] Toggle and expanded action behavior match the design.
- [x] Focused tests pass (61/61).
- [x] Status is `验证成功`.

### T3: Regression and documentation

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: The shared card component covers OAuth, PAT and API variants.

**What to do**:
- Update current README wording for the two display modes if needed.
- Run the complete frontend suite, lint, production build, UI smoke and diff check.

**Logic design**:
- No native/API contract changes are required.
- Existing Settings selector remains synchronized because it uses the same store value.

**Test design**:
- Run all frontend behavioral tests for normal, PAT and API cards.
- Compile TypeScript to catch missing callback propagation.

**Acceptance**:
- `npm test -- --run`
- `npm run lint && npm run build && npm run test:ui`
- `git diff --check`

**Done criteria**:
- [x] Relevant docs match the UI.
- [x] Full tests/static/build checks pass (225/225).
- [x] Status is `验证成功`.

## Test plan

- Normal: API editor action opens; normal Login remains; layout switch changes presentation.
- Edge: main account Rename/Delete guards remain disabled; API cards never gain quota/reset.
- Invalid input: absent API binding continues to behave as a normal account.
- Error: editor loading/saving errors remain owned by the existing editor flow.
- State/conflict: opening/closing menus and switching layout do not select the card.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused behavior | `npm test -- --run src/routes/handoff.test.tsx src/App.handoff.test.tsx` | exit 0 |
| Full frontend | `npm test -- --run` | exit 0 |
| Static/build/smoke | `npm run lint && npm run build && npm run test:ui` | exit 0 |
| Diff hygiene | `git diff --check` | exit 0 |

## Done criteria

- [x] Every task is `验证成功`.
- [x] API Accounts expose configuration editing and no Login action.
- [x] Both layout modes expose the intended action set.
- [x] Existing normal/PAT account behavior remains covered and green.
- [x] No STOP condition remains unresolved.

## STOP conditions

- API Account identity cannot be determined from the existing binding list.
- Persisting the display mode requires a new native contract.
- Existing unrelated workspace changes conflict with the required UI files.
