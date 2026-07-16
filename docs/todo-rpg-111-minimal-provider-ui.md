# Todo: RPG-111 minimal Responses Provider frontend flow

> Follow this todo in order. Write focused tests before implementation, confirm
> the expected red state, and only mark a task verified after its gates pass.

## Status

- **Priority**: P0
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: RPG-110
- **Category**: feature
- **Planned at**: current uncommitted Phase 0/1 worktree

## Why this matters

**Background**: RPG-101–RPG-110 provide the Responses direct domain, stores,
transaction/recovery, credentials, V2 Tauri DTOs, and wrappers.

**Current state**: The desktop still renders and mutates the legacy Provider
shape from App.tsx and cannot preview/execute V2 attach/detach plans.

**Impact**: Phase 1 is not usable through the desktop and G2 cannot pass.

**What improves**: A focused Provider Center owns V2 create/edit,
credential setup, attach/rebind/detach preview and execution while secrets
remain write-only.

## Scope

**In scope**:

- Replace the Provider store with V2 state/actions and dependent refresh.
- Add focused Provider cards, editor, and binding plan components.
- Wire the components into the Providers route and remove legacy Provider modal
  logic from App.tsx.
- Support Responses/direct only; env, Keychain rotation, auth-command reference,
  bearer/header/none auth, and supported Codex options.
- Show redacted preview, operations, warnings, blockers, backup operation/path,
  expiry, and conflict recovery.

**Out of scope**:

- Chat Completions/Gateway attachment and Phase 4 conformance UX.
- Unrelated weekly quota/popover UI.
- Real network health probing; the Phase 1 route-test action validates the
  complete direct route plan without starting Gateway.

## Design

- The Provider store is the V2 frontend boundary. Refresh loads providers and
  bindings together. Create/update use the visible revision; attach/detach
  execute server-issued plan tickets only.
- Forms expose only Responses in Phase 1. Chat Completions records may be listed
  but cannot be attached.
- Keychain secret text is component-local, sent through rotation, and cleared
  after submission; view DTOs never hydrate it.
- A stale/revision conflict clears the plan, refreshes state, and requires a new
  preview.
- Provider presentation is extracted from App.tsx; weekly UI files are untouched.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
| --- | --- | --- | --- |
| T1 | V2 Provider store and conflict recovery | V2-only actions refresh dependent stores | 验证成功 |
| T2 | Focused Provider editor/cards | Create/edit/list and write-only credentials tested | 验证成功 |
| T3 | Attach/rebind/detach preview | Plans gate execution; conflicts require re-preview | 验证成功 |
| T4 | App integration and regression | Legacy modals removed; frontend gates pass | 验证成功 |

### T1: V2 Provider store and conflict recovery

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: The legacy store cannot enforce V2 revisions or plan tickets.

**What to do**: Model providers, bindings, plans, loading and structured
recovery; implement refresh/create/update/rotate/preview/execute actions;
refresh accounts after binding changes.

**Logic design**: Use RPG-110 wrappers exclusively, detect structured recovery
codes, and make execution impossible without the current fingerprint.

**Test design**: Refresh combines Provider/binding results; mutations use the
current revision; attach/detach refresh all dependent state; stale/revision
conflicts clear preview and refresh.

**Red-state evidence**: The focused run on 2026-07-13 failed 8/8 new tests
because the legacy store had no refresh/save/preview/execute V2 actions.

**Acceptance**: pnpm test -- src/stores/providers-v2.test.ts

**Done criteria**:
- [x] Tests written before implementation
- [x] Expected red failure recorded
- [x] Focused tests pass
- [x] Relevant lint/type checks pass
- [x] Status is 验证成功

### T2: Focused Provider editor/cards

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Provider form/card logic must leave App.tsx and expose V2 safely.

**What to do**: Add focused Provider Center/card/editor components covering
protocol, URL, models/default, auth/credential, supported Codex options,
credential reference, binding count, blockers and route-test action.

**Logic design**: Only Responses is attachable. Validate URL, model membership,
credentials and numeric options. Keychain secret is always cleared.

**Test design**: Render V2 cards/blockers; disable Chat Completions attach;
valid create/edit emits normalized DTO; invalid values do not submit; Keychain
secret clears after success/error and never hydrates from a view.

**Red-state evidence**: The focused component run failed before collection
because provider-center.tsx did not exist.

**Acceptance**: pnpm test -- src/components/provider-center.test.tsx

**Done criteria**:
- [x] Tests written before implementation
- [x] Expected red failure recorded
- [x] Focused tests pass
- [x] Relevant lint/type checks pass
- [x] Status is 验证成功

### T3: Attach/rebind/detach preview

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: Transaction safety depends on a current dry-run ticket.

**What to do**: Add attach/rebind and detach dialogs showing redacted TOML,
operations, warnings, blockers, backup operation/path and expiry.

**Logic design**: Every input change clears the plan; execute sends only plan
id/fingerprint; blockers/expiry disable execute; conflicts refresh and keep the
dialog open with recovery guidance.

**Test design**: Preview mandatory; details render; blockers/expiry disable;
model change and stale error invalidate; normal attach/rebind/detach actions run.

**Red-state evidence**: The focused run failed before collection because the
binding dialog component did not exist.

**Acceptance**: pnpm test -- src/components/provider-binding-dialog.test.tsx

**Done criteria**:
- [x] Tests written before implementation
- [x] Expected red failure recorded
- [x] Focused tests pass
- [x] Relevant lint/type checks pass
- [x] Status is 验证成功

### T4: App integration and regression

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**: The new flow must be the actual Providers route.

**What to do**: Wire Provider Center into App.tsx, remove legacy Provider state
and modals, preserve startup refresh, expose a redacted V2 direct-route
upstream validation command, and avoid weekly UI files/styles.

**Logic design**: Provider Center owns dialog state; App supplies profiles only.

**Test design**: Provider route smoke verifies V2 refresh and component
ownership; the Rust/API boundary rejects Gateway protocols and returns only a
redacted direct models endpoint; full frontend catches account/session/weekly
regressions.

**Red-state evidence**: The integrated Provider Center run failed 2/2 because
the route-level component and its V2 upstream-test action were not implemented.

**Acceptance**: pnpm test; pnpm lint; pnpm build; pnpm format:check

**Done criteria**:
- [x] Tests written before implementation
- [x] Expected red failure recorded or integration exception documented
- [x] Full frontend tests pass
- [x] Lint/build/format pass
- [x] Weekly popover files unchanged
- [x] Status is 验证成功

## Test plan

- Store normal/revision/conflict/dependent-refresh cases.
- Component create/edit validation, secret lifecycle and server errors.
- Plan blocker/expiry/change/stale/rebind/detach cases.
- Full frontend regression including existing weekly hover tests.

## Verification commands

| Purpose | Command | Expected |
| --- | --- | --- |
| Store | pnpm test -- src/stores/providers-v2.test.ts | exit 0 |
| Components | pnpm test -- src/components/provider-center.test.tsx src/components/provider-binding-dialog.test.tsx | exit 0 |
| Frontend | pnpm test | exit 0 |
| Quality | pnpm lint; pnpm build; pnpm format:check | exit 0 |

## Done criteria

- [x] Every task is 验证成功
- [x] RPG-111 master status/evidence updated
- [x] No STOP condition remains

## STOP conditions

- RPG-110 cannot express a required operation safely.
- A secret would need to enter persisted/global UI state or a view.
- Completion requires touching unrelated weekly quota/popover UI.
