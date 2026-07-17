# Todo: Account-title renewal and note popover

> Executor instructions: Follow this todo step by step. Write tests from the
> Test design before implementation, confirm their expected failure, then
> implement and verify. Stop rather than weakening accessibility or save
> behavior.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: existing account note persistence
- **Category**: UX/refactor
- **Planned at**: current working tree

## Why this matters

**Background**: Renewal dates and account notes currently occupy a large block
between account metadata and quota. The title row already contains reset-credit
date indicators, so adding another date badge there would create ambiguous
calendar semantics.

**Current state**: `AccountNotePanel` renders either a bordered summary or an
empty dashed placeholder in every account card. Clicking that block opens an
inline form and increases card height further.

**Impact**: Cards have inconsistent vertical rhythm, empty API accounts show an
oversized placeholder, and quota/action rows no longer align visually.

**What improves**: Renewal/note information becomes contextual metadata of the
account name: hover/focus previews it and click edits it without consuming card
body space.

## Scope

**In scope**:

- Replace the body `AccountNotePanel` with an interactive account-title
  component.
- Preview existing or empty renewal/note state on hover/focus.
- Open the existing date/note form as an anchored popover when the title is
  clicked.
- Preserve save/cancel, keyboard operation, and card-click isolation.
- Remove obsolete middle-block styles and add title tooltip/popover styles.

**Out of scope**:

- Changing note persistence or validation.
- Reusing the reset-credit date indicator for renewal information.
- Changing API configuration or account-selection behavior outside the title.

## Design

- The visual heading remains an `h3`, containing a native button styled as
  plain title text.
- The button has an account-specific accessible label and stops propagation so
  title clicks edit notes rather than selecting the card.
- A CSS hover/focus tooltip is anchored below the title. It shows renewal date,
  note, and “Click to edit”; empty accounts show an add hint.
- Clicking opens an absolute-positioned editor popover anchored to the same
  title wrapper, so no card body height is consumed.
- The editor reuses the current controlled date/note state and `onSave`
  callback. Save closes after success; cancel closes without persistence.
- Card body no longer renders renewal/note summaries or empty placeholders.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|---|---|---|---|
| T1 | Move renewal notes to title | Hover/focus previews and click edits without a body block | 验证成功 |

### T1: Move renewal notes to title

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Renewal metadata must remain available without dominating account-card
layout or conflicting with reset-credit indicators.

**What to do**:

- Refactor the note component into a title trigger plus tooltip/editor popover.
- Render it in `cardTitleRow` and remove its body placement.
- Replace obsolete summary CSS with anchored title interaction styles.
- Update card tests for populated, empty, save, cancel, and propagation paths.

**Logic design**:

- Use a native button for keyboard activation.
- Reset local drafts from the latest account values whenever editing starts.
- Stop propagation on trigger and form interactions.
- Tooltip content remains redacted to locally stored renewal/note metadata.
- Existing async save errors keep the editor open through the current `finally`
  behavior; successful save closes it.

**Test design**:

- Populated account: no middle summary exists; title button and tooltip contain
  renewal date, note, and edit hint.
- Empty account: no dashed body placeholder exists; tooltip contains the add
  hint.
- Clicking title does not select the card and opens date/note inputs.
- Editing and Save sends the exact existing request contract and closes.
- Cancel sends no request and closes; native button provides keyboard access.

**Acceptance**:

- `npm test -- --run src/routes/handoff.test.tsx src/App.handoff.test.tsx`
- Full frontend ESLint, changed-file Prettier, production build, and
  `git diff --check` exit 0.

**Failure-first record**: Both new tests failed as expected because account
names were plain headings, no title button or tooltip existed, and the large
`accountNoteSummary`/empty placeholder still occupied the card body. The other
18 tests in the focused file passed.

**Done criteria**:

- [x] Tests were written before implementation
- [x] Expected failure was recorded
- [x] Implementation matches the design
- [x] Focused tests pass
- [x] Lint, formatting, build, and diff checks pass
- [x] Task overview and status are `验证成功`

**Verification record**: 20 focused card tests and 39 App integration tests
passed. Full frontend ESLint, production build, changed-file Prettier, and
`git diff --check` all exit 0.

## Test plan

- Normal: preview populated metadata, edit and save.
- Edge: empty metadata preview, cancel without save.
- Invalid/error: persistence behavior remains covered by store/API tests; no
  new validation is introduced.
- State: title click does not select card; body has no placeholder.
- Accessibility: native named button and tooltip role.

## Verification commands

| Purpose | Command | Expected on success |
|---|---|---|
| Focused tests | `npm test -- --run src/routes/handoff.test.tsx src/App.handoff.test.tsx` | exit 0 after expected failure first |
| Lint/build | `npm run lint && npm run build` | exit 0 |
| Format/diff | `npx prettier --check src/routes/views.tsx src/routes/handoff.test.tsx && git diff --check` | exit 0 |

## Done criteria

- [x] Every task checklist is complete
- [x] Exactly one task status is checked and it is `验证成功`
- [x] Task overview shows `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

- The title already owns a conflicting click contract that cannot be isolated.
- The existing save API cannot be reused unchanged.
- A tooltip/editor would expose credentials or other sensitive data.
- Five implementation/verification repair loops fail.
