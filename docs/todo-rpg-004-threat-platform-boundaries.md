# Todo: RPG-004 Threat, Platform, and Sidecar Boundaries

> Executor instructions: Treat this file as the execution contract for RPG-004.
> This documentation-only issue must close every listed threat with a mitigation,
> explicit residual acceptance, or out-of-scope statement and named later test.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: HIGH
- **Depends on**: RPG-002
- **Category**: security/architecture
- **Planned at**: `codex/202607071457-ui-optimize-20260710102915-remote-provider-gateway`

## Why this matters

**Background**: The Gateway introduces a local bearer boundary, command-backed
credentials, arbitrary upstream URLs, a stable port and a long-lived sidecar.

**Current state**: The design lists threats but does not yet define the supported
platform, filesystem/process primitives, authenticated control plane, resource
limits or deterministic unsupported-platform response.

**Impact**: Implementing sidecar or credential code without these boundaries can
enable port spoofing, token leakage, command injection, SSRF, path attacks,
unbounded resource consumption or unsafe upgrade behavior.

**What improves**: Phase 1 and later security-sensitive issues receive one threat
model, fixed limits, exact platform primitives and named adversarial tests.

## Scope

**In scope**:

- MVP platform declaration and unsupported-platform behavior.
- Private paths, permissions, locking, atomic replacement, Keychain, process
  spawning, code identity and packaging rules.
- HTTP data-plane versus authenticated Unix control-plane trust boundary.
- Auth command, upstream URL/DNS, token, logging, resource and upgrade threats.
- Threat-to-test matrix and synthetic leak markers.

**Out of scope**:

- Implementing security controls in Rust.
- Windows/Linux support.
- Protecting against an attacker who fully controls the current macOS account,
  Keychain session and LAM process memory; this is an explicit trust assumption.

## Design

MVP support is exact-tested macOS 15.6 / Darwin arm64. All persistent state lives
under an owner-only Application Support directory; ephemeral control IPC uses the
per-user Darwin temporary directory. Sidecar identity requires code-manifest
verification plus an authenticated Unix socket challenge and same-uid peer check,
never only a listening TCP port or `/healthz`. Route-bearing metadata and command
approval are integrity-bound to an installation Keychain key. Arbitrary commands
never use a shell. Public-network HTTPS is the default upstream policy; redirects
and private destinations are rejected. Fixed limits apply before allocation or
forwarding.

## Tasks

### Task overview

| ID  | Task                                      | Acceptance summary                                              | Status   |
| --- | ----------------------------------------- | --------------------------------------------------------------- | -------- |
| T1  | Lock platform and trust boundaries        | Supported platform and every trust transition are deterministic | 验证成功 |
| T2  | Complete threat-to-test and leak matrices | Every listed threat has a disposition and named test            | 验证成功 |

### T1: Lock platform and trust boundaries

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:

Filesystem, process, Keychain and IPC behavior cannot be safely implemented from
portable-looking pseudocode without choosing an exact MVP platform.

**What to do**:

- Declare macOS 15.6 / Darwin arm64 as the only supported MVP platform.
- Define paths, modes, ownership checks, locks, atomic writes, spawn and package
  identity.
- Define data/control planes, authenticated identity and upgrade behavior.
- Define unsupported-platform errors and no-write behavior.

**Logic design**:

- Unsupported platforms can read legacy metadata but cannot create/attach/run a
  Remote Provider and never write new Gateway state/config.
- Control operations require same-uid Unix peer identity and HMAC challenge with
  an installation secret not present in argv/env/state files.
- Spawn only versioned, code-verified binaries directly by absolute path.

**Test design**:

- Documentation-only exception: no executable failure state is manufactured.
- Review every supported primitive for a named later fault/security test.
- Verify health identity cannot be established from TCP listener state alone.

**Acceptance**:

- Platform, path, permission, process, lock, Keychain, packaging and control
  rules have one MVP outcome.
- Unsupported platform returns a structured error before mutation.

**Done criteria**:

- [x] Documentation-only test exception recorded; normative platform text has no executable pre-change failure state, so named later tests and focused review are the substitute
- [x] Platform and unsupported-platform behavior fixed
- [x] All paths/modes/ownership and primitives fixed
- [x] Sidecar identity and upgrade protocol fixed
- [x] Focused review passes
- [x] T1 becomes `验证成功`

### T2: Complete threat-to-test and leak matrices

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:

A prose threat list is not enforceable unless each threat has a disposition,
control owner and adversarial test.

**What to do**:

- Add the full threat matrix and fixed resource budgets.
- Cover tokens, command execution, SSRF/DNS, path/symlink, binding rotation,
  logging/evidence and version mismatch.
- Define synthetic secret markers for repository-wide leak tests.

**Logic design**:

- Reject dangerous inputs before body parsing, process spawn, DNS connect or
  file mutation.
- State explicit residual risk where direct Codex networking prevents address
  pinning rather than claiming a control that is not present.
- Tests scan serialized state, config, logs, evidence, frontend data and fixtures.

**Test design**:

- Documentation-only exception: matrix review substitutes for a red executable
  test; every row names the red-first implementation test.
- Confirm every RPG-004 threat appears exactly once in the matrix.
- Confirm markers cover API key, bearer, command stdout, prompt/tool/reasoning.

**Acceptance**:

- Every required threat is mitigated, explicitly accepted or out of scope.
- Every row maps to RPG-103/105/108/109/301–308/405 and a named test.

**Done criteria**:

- [x] Documentation-only test exception recorded; threat matrix text has no executable pre-change failure state, so coverage review and named later red tests are the substitute
- [x] Every required threat has a disposition and owner
- [x] Resource limits are numeric and testable
- [x] Synthetic leak markers are fixed
- [x] Threat coverage scan and formatting pass
- [x] T2 becomes `验证成功`

## Test plan

- Threat term coverage scan.
- Platform/path/control identity review.
- Named-test completeness review.
- Prettier and `git diff --check`.

## Verification commands

| Purpose         | Command                                                                                                                                                                                                                        | Expected on success                   |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------- |
| Threat coverage | `rg -n -e port_spoofing -e token_exposure -e auth_command_execution -e ssrf_dns -e resource_exhaustion -e path_symlink -e binding_rotation -e content_leakage -e version_mismatch docs/remote-provider-gateway-full-design.md` | All threat IDs are present            |
| Platform        | `rg -n -e 'macOS 15.6' -e UNSUPPORTED_REMOTE_PROVIDER_PLATFORM -e gateway-control docs/remote-provider-gateway-full-design.md`                                                                                                 | Exact platform and control path exist |
| Format          | `pnpm exec prettier --check ../../docs/remote-provider-gateway-full-design.md ../../docs/todo-remote-provider-gateway.md ../../docs/todo-rpg-004-threat-platform-boundaries.md` from `apps/desktop`                            | exit 0                                |

## Done criteria

- [x] T1 and T2 are both `验证成功`
- [x] Both task checklists are complete
- [x] Master todo and design link validation evidence
- [x] No STOP condition remains

## STOP conditions

- A required sidecar identity control requires unsupported OS capabilities.
- Exact-tested Codex behavior contradicts the auth/retry assumptions.
- A threat cannot be mitigated or explicitly accepted within the declared MVP
  trust assumptions.
