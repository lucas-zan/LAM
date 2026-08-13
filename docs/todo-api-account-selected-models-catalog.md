# Todo: Keep selected API models and Codex catalog consistent

- **Status**: 验证成功
- **Scope**: The API account editor and launched Codex must use the account's saved model selection, with explicit on-demand discovery.
- **Baseline**: `agent/20260812-handoff-session-pagination` with the current uncommitted provider-model lifecycle changes preserved.

## Why

The editor replaces the saved selection with every discovered upstream model, producing a noisy list. Codex `/model` still shows the OpenAI catalog because LAM writes an internal cache filename but does not configure the supported `model_catalog_json` key.

## Behavior

- **Input**: An API account with saved Provider models and an optional user-triggered `/models` refresh.
- **Output**: The editor initially shows only saved models; Fetch models exposes live candidates for selection; the profile config explicitly points Codex at the account catalog.
- **Constraints/errors**: Fetch failure preserves saved models; refresh does not persist every discovered model until the user saves a selection; catalog paths are profile-local and TOML-safe.

## Test first

- Normal: Editor starts with saved models, fetch displays live choices without replacing them, saving persists the chosen set; generated config contains `model_catalog_json` and `/model` catalog contains only selected models.
- Edge/invalid: Removed/default model selection stays valid; empty discovery does not erase the saved set; paths with special characters are escaped.
- Error/state conflict: Failed discovery preserves editor state; stale Provider revision fails without partial model mutation.
- Expected RED: Refresh currently persists every discovered model immediately, and generated API-account config lacks `model_catalog_json`.

## Implementation

Separate discovery preview from persistence in the editor/API, add selected model updates to account save, and project the official `model_catalog_json` setting into managed profile config. Do not depend on Codex's undocumented cache lookup behavior.

## Verification

- Focused: API account editor/store tests and Rust config/catalog/account lifecycle tests.
- Regression/lint/build: frontend and Rust suites, formatting, production build, and diff check.

## Evidence

- RED: Editor test failed because it rendered “Available models” and immediate refresh behavior; API account test failed because config lacked `model_catalog_json`.
- GREEN: Editor focused tests passed; the existing-account startup migration test passed; all 25 frontend test files / 251 tests, the complete Rust suite, and the production frontend build passed. Rust formatting and `git diff --check` passed.
- Changed files: API account editor/store flow, Provider discovery semantics, managed Codex config projection/planner, catalog writer/startup migration, styles, and mapped frontend/Rust tests.
- Remaining limitation: Fetch models is intentionally read-only. Changing the saved model selection still uses the existing account creation/model-switch workflows rather than treating every discovered upstream model as selected.

## Done

- [x] Test was written first and RED observed, or exception recorded.
- [x] Implementation stays in scope.
- [x] Focused and relevant regression checks pass.
- [x] Status is 验证成功.
