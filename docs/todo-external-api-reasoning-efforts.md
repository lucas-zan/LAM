# Todo: External API reasoning efforts

- **Status**: 验证成功
- **Scope**: LAM-managed External API accounts expose the Codex Desktop reasoning-effort choices and preserve the selected effort through the Responses-to-Chat-Completions gateway.
- **Baseline**: `agent/20260812-handoff-session-pagination` at `687bf50`; unrelated untracked `.novel/` is preserved.

## Why

External API profiles omit the Desktop reasoning-effort preference, generate an empty model reasoning catalog, and reject Codex's `none`, `ultra`, and `max` values. Users therefore cannot select an effort and the default `none` request fails before reaching the upstream API.

## Behavior

- **Input**: Codex Desktop selects `low`, `medium`, `high`, `xhigh`, `ultra`, or `max`; unsupported/default Desktop state may send `none`.
- **Output**: Generated External API profiles advertise the six selectable efforts; Chat Completions translation forwards an active effort as `reasoning_effort`; `none` is treated as no active reasoning control.
- **Constraints/errors**: Existing strict request validation remains; unrelated provider fields and private Desktop state are not inherited; DeepSeek compatibility keeps its existing effort mapping.

## Test first

- Normal: catalog contains the six native Desktop efforts in stable order; generic translation forwards `ultra` and `max`.
- Edge/invalid: `none` parses and is omitted from the upstream Chat Completions request; an unknown effort remains rejected.
- Error/state conflict: API-account template contains only the approved Desktop reasoning-effort preference and does not copy other Desktop machine state.
- Expected RED: current catalog assertions return `[]`; protocol parsing rejects `none/ultra/max`; generic translation rejects reasoning; generated config lacks `[desktop].enabled-reasoning-efforts`.

## Implementation

Add the native Desktop preference to the managed template and its narrow inheritance allowlist, populate the generated Codex model catalog with the native six efforts, extend the controlled Responses schema, and translate active generic OpenAI-compatible efforts to Chat Completions `reasoning_effort`. Do not add provider-specific UI or infer which individual upstream models truly support each effort.

## Verification

- Focused: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_codex_catalog --test provider_adapter_schema --test provider_adapter_request --test provider_phase4_api_account`
- Regression/lint/build: relevant gateway route/DeepSeek tests, full Rust test suite if practical, and desktop build.

## Evidence

- RED: Focused Cargo command failed with `E0599` because `ReasoningEffort::{None,Max,Ultra}` do not exist; catalog/template assertions have not yet reached execution.
- GREEN: Focused tests passed (45 tests); related adapter/Gateway/contract tests passed (37 tests); full `cargo test` passed with only four pre-existing explicit ignores; `cargo fmt --check`, `git diff --check`, and Desktop `npm run build` passed.
- Changed files: External API config template and safe preference projection; Codex model catalog; controlled reasoning protocol and Chat Completions translation; DeepSeek route compatibility; focused and integration tests.
- Remaining limitation: LAM advertises the configured six efforts for every External API model because standard `/models` discovery does not report per-model effort support. Upstreams that support fewer levels can still reject a user-selected value. Existing profiles need recreation or a deliberate config/catalog refresh; this change avoids silently rewriting live managed profiles.

## Done

- [x] Test was written first and RED observed, or exception recorded.
- [x] Implementation stays in scope.
- [x] Focused and relevant regression checks pass.
- [x] Status is 验证成功.
