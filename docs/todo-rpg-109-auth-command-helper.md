# RPG-109 Restricted Auth Command and Helper Execution TODO

Status: 验证成功

## Background and current state

Provider credentials can reference an approved auth command, and the config
editor can represent a Codex auth table, but command execution is not yet a
bounded security boundary. A real implementation must never invoke a shell,
must bind approval to the executable, and must ensure tokens only cross the
explicit request/helper stdout boundary.

## Scope and design

- Define structured executable/argv/cwd/timeout/output/cache metadata and an
  approval fingerprint tied to the executable bytes and execution policy.
- Require an absolute, regular, non-symlink executable with safe ownership and
  write permissions; reject unsafe cwd and missing/replaced executables.
- Spawn directly with a minimal environment, null stdin, bounded stdout/stderr,
  timeout plus process-group termination, and guaranteed reap.
- Accept exactly one non-empty token line after surrounding whitespace trim;
  errors and Debug output are always redacted.
- Cache only the secret wrapper, support explicit invalidation/401 refresh, and
  never serialize the token.
- Project an approved helper command into the official Codex provider `auth`
  table, mutually exclusive with all other auth modes.

## Tasks

- [x] T1: Add failing tests for success/trim and empty, multiline, oversized,
      timeout, nonzero, missing, symlink/unsafe path and cwd failures.
- [x] T2: Add failing tests for literal metacharacter argv, approval replacement,
      cache/refresh, redaction, and subprocess cleanup.
- [x] T3: Add failing config tests for the approved direct helper auth table and
      mutual exclusion.
- [x] T4: Implement command approval, runner, token validation/cache and helper
      stdout boundary.
- [x] T5: Integrate approved helper projection, run focused/full regression, and
      update master task evidence.

## Validation evidence

- Red phase: focused test failed to compile because `provider_auth_command` did
  not exist; the first implementation also exposed a deliberately over-broad
  redaction assertion, which was narrowed to the synthetic marker.
- Focused auth command tests pass 4/4 across literal argv, token shape/output
  bounds, timeout/nonzero/missing/symlink/cwd policy, cache invalidation,
  redaction, helper stdout, and official Codex auth projection.
- The runner drains bounded stdout/stderr concurrently, starts a separate Unix
  process group, terminates the group on timeout, and reaps the child.
- Config editor and credential regressions pass 4/4 each; full Rust/frontend,
  Phase 0, lint, and build validation pass.

## Test and acceptance commands

```sh
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_auth_command
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --test provider_config_editor
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
```

Acceptance requires direct Keychain/auth-command Responses Providers to produce
official Codex auth configuration, no shell evaluation, no plaintext token in
DTO/Debug/error/log output, and reaped subprocesses on every result path.

## STOP conditions

- Stop if a command would be executed without an explicit approval fingerprint.
- Stop if a token must be placed in argv, environment, a file, or diagnostics.
- Stop if implementation would alter the weekly quota popover UI.
