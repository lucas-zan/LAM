# Todo: 统一 External API 配置页与接口后缀展示

> Executor instructions: Follow this todo step by step. Generate tests from the
> "Test design" section before implementation. Run each verification command and
> confirm the expected result before moving to the next task. If a STOP condition
> occurs, stop and report instead of improvising.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: current uncommitted `ApiAccountFlow` layout work
- **Category**: feature / UX
- **Planned at**: current working tree on 2026-07-15

## Why this matters

**Background**: The External API add flow currently presents Responses and Chat Completions as a top-level segmented tab group. The revised product logic treats them as one configuration flow whose only protocol-specific input is the upstream API route.

**Current state**: `apps/desktop/src/components/api-account-flow.tsx` renders the protocol selector above the entire form. `Base URL` is a single editable input and does not expose the route suffix. The Fetch models button uses the default medium button dimensions.

**Impact**: The top-level tabs imply two separate workflows, users can mistakenly include the endpoint suffix in the editable base URL, and the model discovery action occupies more visual space than necessary.

**What improves**: The page becomes one coherent configuration form, protocol choice sits next to the URL configuration it affects, the immutable route suffix is always visible, and Fetch models has a lighter compact footprint.

## Scope

**In scope**:
- `apps/desktop/src/components/api-account-flow.tsx`
- `apps/desktop/src/components/api-account-flow.test.tsx`
- `apps/desktop/src/styles.css`

**Out of scope**:
- Backend/provider contracts and model discovery endpoint behavior
- Existing Provider reuse behavior
- Review/Create workflow refactors from the prior layout task
- Other External API or Provider Center pages

## Design

- Remove the top-level Responses / Chat Completions segmented control and its protocol help block.
- Inside new-connection `Connection details`, render an `API type` selector immediately above `Base URL`; default remains `responses`.
- Render Base URL as a compound row: editable base input followed by a read-only endpoint suffix input.
- The suffix is `/responses` for `responses` and `/chat/completions` for `chat_completions`.
- Switching API type clears the existing plan but preserves current URL, credentials, and discovery state because those inputs are still valid connection data.
- Preserve Chat Completions compatibility selection and request construction.
- Apply a dedicated compact class and small button size to Fetch models without changing its disabled/loading behavior.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Unify protocol and URL configuration | No top tabs; API type is above Base URL; read-only suffix follows selection; compact Fetch models; tests/build pass | 验证成功 |

### T1: Unify protocol and URL configuration

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The protocol choice is a URL routing detail rather than a separate page mode, so it should be visually and logically grouped with Base URL.

**What to do**:
- Update component tests to specify default protocol, selector placement, suffix read-only behavior, protocol switching, request preservation, and compact discovery button markup.
- Update `ApiAccountFlow` markup while retaining current state and request contracts.
- Add API-flow-specific CSS for the selector, compound Base URL row, suffix width/read-only appearance, and compact Fetch models button.

**Logic design**:
- Keep `protocol` initialized to `responses`.
- Use a labelled select for the API type to avoid recreating tab styling.
- Derive suffix directly from `protocol`; do not store duplicate suffix state.
- On protocol change call `setProtocol(...)` and `setPlan(null)` only.
- Keep `baseUrl` passed to discovery/planning unchanged; the visible suffix is explanatory and not concatenated into stored Base URL.
- Continue rendering Compatibility only for `chat_completions`.

**Test design**:
- Assert the former `API protocol` tab group and its two buttons are absent.
- Assert `API type` defaults to `responses`, is located before `Base URL`, and `/responses` is displayed in a read-only input directly after the Base URL input.
- Switch to `chat_completions`; assert suffix becomes `/chat/completions` and Compatibility appears.
- Plan a Chat Completions account and assert the request still uses the raw Base URL, chat protocol, local adapter path, and selected compatibility profile.
- Assert Fetch models uses the small button variant and a dedicated compact class.
- Expected initial failure: current top-level group exists; no `API type` select or suffix input exists; Fetch models has default medium styling.

**Acceptance**:
- `cd apps/desktop && npm test -- --run src/components/api-account-flow.test.tsx`
- `cd apps/desktop && npm run build`
- `cd apps/desktop && npm run lint`
- `cd apps/desktop && npm run format:check`

**Done criteria**:
- [x] Tests listed above were written before implementation
- [x] New tests were run and failed for the expected reason before implementation
- [x] Implementation follows the design and does not concatenate suffix into stored Base URL
- [x] Focused tests pass
- [x] Build, lint, and format check pass
- [x] Task overview and status match `验证成功`

## Test plan

- Default: Responses selected and `/responses` shown read-only.
- Interaction: switching API type updates suffix and conditional Compatibility field.
- Request regression: Chat Completions plan request retains raw Base URL and adapter configuration.
- Layout contract: protocol tabs absent, API type precedes Base URL, suffix follows Base URL, Fetch models is compact.
- Existing component suite remains green for discovery, stale requests, manual models, allowlist, and Provider reuse.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused tests | `cd apps/desktop && npm test -- --run src/components/api-account-flow.test.tsx` | exit 0 after implementation; expected failure before implementation |
| Full frontend suite | `cd apps/desktop && npm test -- --run` | 22 files / 197 tests pass |
| Static UI smoke | `cd apps/desktop && npm run test:ui` | UI smoke passed |
| Build | `cd apps/desktop && npm run build` | exit 0 |
| Lint | `cd apps/desktop && npm run lint` | exit 0 |
| Format | `cd apps/desktop && npm run format:check` | exit 0 |

## Done criteria

- [x] T1's Done criteria checklist is fully checked
- [x] T1 has exactly one checked Status value and it is `验证成功`
- [x] Task overview shows T1 as `验证成功`
- [x] No backend/API behavior is changed
- [x] No STOP condition remains unresolved

## STOP conditions

- Current component behavior differs materially from the inspected implementation.
- The suffix must be persisted or sent to the backend, contrary to the stated UI-only requirement.
- Existing unrelated working-tree changes prevent focused verification.
- Focused tests still fail after five fix cycles.
