# Todo: Stage 2 CLI readiness recovery fix

## Status

- [x] Root cause confirmed
- [x] Failure tests added
- [x] Implementation completed
- [x] Focused verification passed

## Problem

`lam codex`, including API-account wrappers such as `codex-welfare`, still uses
the pre-Stage-2 readiness path. It unconditionally calls verified process
termination whenever `gateway-state.json` contains a PID. A terminated child
that remains a macOS zombie also passes `kill(pid, 0)`, causing
`GATEWAY_PROCESS_TERMINATION_TIMEOUT`.

## Expected behavior

- A healthy authenticated Gateway is reused and never signalled.
- Missing and zombie processes are treated as stopped.
- A reused PID is cleared without signalling the unrelated process.
- Transient identity failure does not authorize termination or replacement.
- Only a repeated authenticated identity mismatch authorizes bounded recovery.
- CLI readiness uses the same Supervisor observation/reconciliation contracts as
  the desktop monitor.

## Tests

- Spawn an exited but unreaped child and prove system inspection returns no live
  process.
- Production wiring test proves `lam codex` uses `inspect_gateway_claim` and
  `reconcile_gateway_claim`, rather than unconditional recovery.
- Run focused recovery, Supervisor, launcher, and production-wiring suites.

## Verification record

- Failure-first zombie test failed with `GATEWAY_PROCESS_INSPECTION_FAILED`
  before the macOS status fix.
- Recovery, Supervisor, launcher, and production-wiring regression: 34 passed,
  0 failed.
- Development components and integrity manifest were synchronized with the
  repository's `package-gateway-components.mjs dev` workflow.
- Real-environment smoke test: `codex-welfare --version` returned
  `codex-cli 0.144.5` with exit code 0.
