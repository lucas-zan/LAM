# Todo: Native Responses direct routing

## Status

- [x] Existing implementation and regression surface inspected
- [x] Failure-first tests added
- [x] Failure-first behavior confirmed
- [x] Implementation completed
- [x] Existing welfare profile migrated and verified

## Contract

- Native Responses providers always use Codex direct routing.
- Responses API accounts use Codex's native per-profile `auth.json` API-key
  login and `requires_openai_auth`; they never execute a LAM auth helper.
- Chat Completions providers require the Gateway adapter.
- API-account creation cannot force a Responses provider onto the Gateway.
- Desktop Supervisor and sidecar idle ownership count only active,
  non-revoked Chat Completions Gateway bindings.
- A stale legacy Responses Gateway binding cannot start or keep Gateway alive.

## Tests

- Route planning ignores the legacy Responses `route_via_gateway` flag.
- API-account planning and execution produce a Direct wrapper, remote base URL,
  and no Gateway binding.
- Responses readiness does not depend on Gateway availability.
- Shared binding activation predicate accepts only active Chat Completions
  bindings and is used by both Supervisor and sidecar.
- Native auth materialization and one-time legacy Keychain migration tests pass.
- Real welfare account succeeds while Gateway state remains unclaimed.

## Verification

- 129 focused Rust integration tests passed.
- Existing `idragon3` and `welfare` bindings migrated from `gateway` to
  `direct`; their legacy Gateway bindings are revoked.
- `codex-welfare exec` returned `OK` against the remote Responses base URL.
- Gateway runtime `processId` remained null and no Gateway process remained.

The earlier Keychain-helper implementation has since been replaced by the
native Codex auth contract described in
`docs/todo-native-codex-api-account-auth-editing.md`.
