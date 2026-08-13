# Todo: Fix External API model lifecycle

- **Status**: 验证成功
- **Scope**: Keep External API Provider models consistent across config parsing, refresh/edit, Switch Model, and Codex `/model`.
- **Baseline**: `agent/20260812-handoff-session-pagination` with the current uncommitted session-management and permission work preserved.

## Why

External API accounts can have saved models while Switch Model renders an empty list, editing cannot refresh models, and Codex `/model` falls back to the main OpenAI catalog. The account/provider association and per-profile Codex model catalog must use the same persisted source of truth.

## Behavior

- **Input**: An External API account, its Provider binding, stored credential, and upstream OpenAI-compatible `/models` response.
- **Output**: Correct Provider association; refreshable persisted model list; non-empty Switch Model fallback; profile-local `models_cache.json` containing only that Provider's models.
- **Constraints/errors**: Secrets remain write-only; failed refresh keeps the saved list and reports an actionable error; catalog writes are private and atomic; unrelated Codex profiles are unchanged.

## Test first

- Normal: Parse model/provider after comments and blank lines; refresh and persist models using stored profile credential; Switch Model resolves through binding; create/update/switch writes a profile-local catalog.
- Edge/invalid: Missing account `providerId` falls back to binding; duplicate/invalid upstream models fail without overwriting; stale Provider revision fails closed.
- Error/state conflict: Missing credential and failed catalog write do not silently report success.
- Expected RED: TOML parsing returns `None`, no stored-credential discovery API exists, Switch Model only matches `account.providerId`, and no per-profile catalog is written.

## Implementation

Fix the parser; add a Provider model refresh service/command using the existing credential resolver; expose refresh in API account editing and model switching; synchronize a Codex-compatible catalog after account create, Provider model update, connection update, and model switch. Keep manual model entry as fallback.

## Verification

- Focused: Rust parser/discovery/API-account/catalog tests and frontend Provider/Switch Model tests.
- Regression/lint/build: Rust and frontend suites, lint, build, format, and diff check.

## Evidence

- RED: Parser skipped the complete config after a leading comment; no frontend binding fallback or profile-local model catalog existed; focused parser, model-switch resolution, and API-account catalog tests failed.
- GREEN: Frontend 25 files / 251 tests passed; complete Rust test suite passed; `npm run build`, TypeScript checking, Rust formatting, and `git diff --check` passed.
- Changed files: Provider parser/API/store/editor/model-switch flow, Codex catalog writer and startup synchronization, plus focused frontend and Rust tests.
- Remaining limitation: Model discovery intentionally supports direct OpenAI-compatible Responses Providers with bearer credentials; Chat Completions adapter Providers keep their manually configured model list.

## Done

- [x] Test was written first and RED observed, or exception recorded.
- [x] Implementation stays in scope.
- [x] Focused and relevant regression checks pass.
- [x] Status is 验证成功.
