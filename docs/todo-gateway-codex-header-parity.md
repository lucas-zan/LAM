# Todo: Gateway Codex upstream header parity

## Status

- [x] Root cause reproduced
- [x] Contract tests designed before implementation
- [x] Failure-first tests confirmed
- [x] Implementation completed
- [x] Focused verification passed
- [x] Native Responses real-upstream path superseded by direct routing and verified

## Root cause

Codex direct routing sends a captured set of client metadata headers. LAM's
Gateway previously rebuilt the upstream request with only `Content-Type` and
upstream `Authorization`. The welfare nginx rejects requests without a Codex
`User-Agent` with 403 before API authentication.

Forwarding the bounded Codex metadata changed the welfare failure from nginx
403 to a Gateway transport 502 because the sidecar intentionally does not use
the host proxy. Native Responses routing now bypasses both failure modes. The
header contract remains as defense-in-depth for Gateway transports.

## Contract

Forward the exact safe metadata-header set observed in the pinned Codex
contract:

- `accept`
- `originator`
- `session-id`
- `thread-id`
- `user-agent`
- `x-client-request-id`
- `x-codex-beta-features`
- `x-codex-turn-metadata`
- `x-codex-window-id`

Future bounded `x-codex-*` metadata is forwarded as part of the Codex-owned
namespace so a compatible CLI point release does not silently lose new contract
metadata.

Gateway remains the sole owner of `authorization`, `host`, `content-length`,
and `content-type`. Unknown headers are dropped. Values and total forwarded
metadata are bounded. No forwarded header is logged.

## Tests

- Capture the complete pinned Codex allowlist and drop unknown/sensitive heads.
- Reject oversized values without affecting allowed bounded values.
- Prove the upstream transport sends safe Codex metadata while replacing local
  authorization with the Keychain-backed upstream credential.
- Prove the loopback server carries captured metadata into the route contract.
- Run server, routes, upstream, launcher, and production-wiring regressions.
- Execute one minimal welfare request through the final native direct route.
