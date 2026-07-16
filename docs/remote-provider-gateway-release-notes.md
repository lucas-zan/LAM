# Remote Provider Gateway v0.2.1 release notes

Status: Phase 0–4 implementation complete; G5 automated/package acceptance is recorded in the Phase 4 todo.

## Product behavior

- One Account owns one Profile and one `CODEX_HOME`.
- Add Account → API Account discovers the standard OpenAI `/models` list, then creates a named external API account
  from explicitly selected or custom models. New API Accounts always use the Codex-compatible Gateway. A Provider is
  exclusive by default; reuse is advanced and explicit.
- Gateway exposes every allowed Provider model through an exact-tested Codex 0.144.1 catalog. The selected model is
  the initial default, not the only model available at request time.
- Model switching keeps the Account and `CODEX_HOME`. Create/delete transactions recover after process interruption.
- Provider-aware relay classifies history before target writes. Unsupported/stateful content is blocked; harmless
  representation loss requires confirmation.

## Security and privacy

- Secrets are write-only and stored in macOS Keychain; no secret is included in plans, config projections, journals,
  capability evidence, relay reports, structured metrics, or packaged manifests.
- Gateway is loopback-only, bearer-protected, SSRF constrained, and exposes exactly `/v1/models` and `/v1/responses`.
- Model discovery ignores proxy environments and redirects, uses bounded response/time/model limits, and never persists
  or logs the write-only API key.
- Metrics contain only route/status/latency/usage/retry and hashed binding identity.

## Verified matrix

- Codex 0.144.1 request/resume/tool contract fixtures plus a non-empty strict model catalog captured on 2026-07-14.
- Responses Gateway byte-preserving JSON/SSE pass-through and DeepSeek-compatible non-stream, stream, and
  function-tool Gateway translation.
- Account-first create/reuse/switch/delete, runtime compensation, startup recovery, and stale-plan rejection.
- Text relay and completed verified function pairs; all explicitly unsupported history classes fail closed.

Not enabled: Responses retrieve/cancel/store, `previous_response_id`, hosted/MCP/computer/media adaptation, or
DeepSeek thinking plus tool history without a verified lossless mapping.
