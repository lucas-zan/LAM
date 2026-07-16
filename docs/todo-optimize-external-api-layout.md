# Todo: 优化 External API 添加页面布局

> Executor instructions: Follow this todo step by step. Generate tests from the
> "Test design" section before implementation. Run each verification command and
> confirm the expected result before moving to the next task. If a STOP condition
> occurs, stop and report instead of improvising.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: existing `ApiAccountFlow` and dedicated `externalApi` modal
- **Category**: refactor / UX
- **Planned at**: current working tree (contains pre-existing uncommitted work)

## Why this matters

**Background**: The External API flow is functionally complete, but the default 1280px app window opens a 500px modal containing a two-column form. Protocol selection, connection reuse, model discovery, review, and creation are presented at nearly the same visual level.

**Current state**: `App.tsx` opens the dedicated modal without `wide`; `ApiAccountFlow` mixes two protocols with “Reuse Existing Connection” in one segmented control; the form footer scrolls with the body; both Review and disabled Create actions are always rendered; the model checklist has no bounded scrolling or long-name handling.

**Impact**: Inputs and model controls become cramped, the sequential dependency from credentials to model discovery is weak, advanced reuse looks like a protocol, and the primary action hierarchy is unclear.

**What improves**: The flow gets an appropriately wide shell, clearer protocol/advanced-option hierarchy, numbered content sections, bounded model selection, a sticky action area, and a single stage-appropriate primary action without changing backend requests or account lifecycle behavior.

## Scope

**In scope**:
- `apps/desktop/src/App.tsx`
- `apps/desktop/src/components/api-account-flow.tsx`
- `apps/desktop/src/components/api-account-flow.test.tsx`
- `apps/desktop/src/App.handoff.test.tsx`
- `apps/desktop/src/styles.css`
- Relevant static UI smoke expectations if needed

**Out of scope**:
- Provider/account domain logic and Tauri commands
- API request/response types
- Model discovery behavior and credential security behavior
- Localization or a full multi-page wizard

## Design

- Open External API in the existing wide modal shell.
- Keep Responses and Chat Completions as the only protocol choices in a labelled control; show concise contextual help.
- Move Provider reuse into a collapsible Advanced options area. If there are no providers, explain that reuse is unavailable and disable the choice.
- Keep connection and model sections in a wide two-column layout with explicit step numbers; collapse to one column based on the modal/content width rather than the global 1240px application breakpoint.
- Bound the discovered model list and make long model labels wrap safely.
- Use one primary action per stage: Review before a plan exists, Create after a valid plan exists. Keep Cancel separate.
- Make the flow action bar sticky within the scrolling modal body.
- Preserve all existing request construction, model-selection validation, discovery reset, and secret handling.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Wide modal and semantic option hierarchy | External API uses wide shell; protocol group has two choices; reuse is under Advanced options | 验证成功 |
| T2 | Content and model layout resilience | Numbered sections, content-responsive columns, bounded/wrapping model list and reuse empty state | 待执行 |
| T3 | Stage-aware sticky actions | Only Review or Create is shown for the current stage and actions remain visually anchored | 待执行 |

### T1: Wide modal and semantic option hierarchy

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: The default 500px shell is too narrow, and Provider reuse is not a protocol.

**What to do**:
- Pass `wide` to the dedicated External API modal.
- Replace the three-way segmented control with a labelled two-choice protocol control.
- Add an Advanced options disclosure containing the reuse control.

**Logic design**:
- Protocol buttons update the existing `protocol` state and leave reuse disabled.
- Opening Advanced options does not mutate form values.
- Enabling reuse keeps the existing reset/default-provider behavior.

**Test design**:
- App handoff test asserts the External API heading belongs to a `modalWide` shell.
- Component test asserts an `API protocol` group contains only Responses and Chat Completions.
- Component test asserts reuse is absent until Advanced options is expanded, then remains operable.
- Expected initial failure: modal lacks `modalWide`; no labelled protocol group or Advanced options disclosure exists.

**Acceptance**:
- `npm test -- --run src/components/api-account-flow.test.tsx src/App.handoff.test.tsx`

**Done criteria**:
- [x] Tests were written before implementation
- [x] New tests failed for the expected reason before implementation
- [x] Implementation follows the design and preserves reuse request behavior
- [x] Focused verification passes
- [x] Task overview and status match `验证成功`

### T2: Content and model layout resilience

**Status**:
- [ ] 待执行
- [x] 待测试验证
- [ ] 验证成功
- [ ] 验证失败

**Why**: The flow needs clearer sequence and model lists must not expand the entire modal indefinitely.

**What to do**:
- Add numbered section headers and an API-flow-specific grid.
- Add a bounded model checklist container and safe long-name wrapping.
- Add an explicit no-existing-connections message and disabled reuse option.
- Add API-flow-specific responsive CSS.

**Logic design**:
- Keep both sections visible so users can understand the complete setup.
- CSS owns width/overflow behavior; model selection semantics remain unchanged.
- Reuse cannot be enabled when `providers` is empty.

**Test design**:
- Assert the two section headings are exposed with step numbers.
- Assert discovered models render inside the dedicated bounded checklist container.
- Assert Advanced options explains unavailable reuse and exposes a disabled reuse checkbox when providers are empty.
- Expected initial failure: current headings have no step labels; current empty reuse state is selectable and unexplained.

**Acceptance**:
- `npm test -- --run src/components/api-account-flow.test.tsx`
- `npm run build`

**Done criteria**:
- [x] Tests were written before implementation
- [x] New tests failed for the expected reason before implementation
- [ ] CSS and markup implement resilient layout without changing discovery behavior
- [ ] Focused verification and build pass
- [ ] Task overview and status match `验证成功`

### T3: Stage-aware sticky actions

**Status**:
- [x] 待执行
- [ ] 待测试验证
- [ ] 验证成功
- [ ] 验证失败

**Why**: Showing Review and disabled Create simultaneously weakens the action hierarchy, and the current footer scrolls away.

**What to do**:
- Render Review before a plan exists and Create after a plan exists.
- Give the action area an API-specific sticky class.
- Keep Cancel available and preserve blocker/busy disabling.

**Logic design**:
- Existing field mutations already clear `plan`, naturally returning the flow to Review.
- A blocked plan shows Create disabled, preserving current safety behavior.
- No new workflow state is introduced.

**Test design**:
- Assert initial state has Review and no Create.
- After planning, assert Create is present and Review is absent.
- Assert editing a plan-dependent field clears the plan and restores Review.
- Assert the action container has the dedicated sticky-action class.
- Expected initial failure: Create is always present and action container lacks the new class.

**Acceptance**:
- `npm test -- --run src/components/api-account-flow.test.tsx`
- `npm run test:ui`

**Done criteria**:
- [ ] Tests were written before implementation
- [ ] New tests failed for the expected reason before implementation
- [ ] Stage behavior and busy/blocker safety are preserved
- [ ] Focused verification and UI smoke pass
- [ ] Task overview and status match `验证成功`

## Test plan

- Normal: protocol selection, discovery, model selection, review, create.
- Advanced reuse: disclosure hierarchy and existing Provider request.
- Empty reuse: disabled choice plus explanatory message.
- Stage transition: Review → Create → field edit → Review.
- Layout contract: wide shell, step headings, API-specific grid/checklist/action classes.
- Regression: existing stale discovery, fallback, allowlist, and secret tests remain green.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused tests | `cd apps/desktop && npm test -- --run src/components/api-account-flow.test.tsx src/App.handoff.test.tsx` | exit 0 after implementation; expected failure before implementation |
| Full frontend suite | `cd apps/desktop && npm test -- --run` | exit 0 |
| Static UI smoke | `cd apps/desktop && npm run test:ui` | exit 0 |
| Lint | `cd apps/desktop && npm run lint` | exit 0 |
| Format | `cd apps/desktop && npm run format:check` | exit 0 |
| Build | `cd apps/desktop && npm run build` | exit 0 |

## Done criteria

- [ ] Every task's Done criteria is complete
- [ ] Every task has exactly one checked status and it is `验证成功`
- [ ] Task overview shows every task as `验证成功`
- [ ] Full frontend tests, UI smoke, lint, format check, and build pass
- [ ] No backend/API behavior is changed
- [ ] No STOP condition remains unresolved

## STOP conditions

- Current External API behavior differs materially from the inspected implementation.
- Layout changes require backend/type contract changes.
- Existing unrelated working-tree changes prevent safe verification.
- Focused tests still fail after five fix cycles.
