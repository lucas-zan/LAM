# Todo: RPG-003 Normative Contracts

> Executor instructions: Treat this file as the execution contract for RPG-003.
> This issue is documentation-only: it must make every Phase 1 contract
> implementable and traceable before any Phase 1 source code is written.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: HIGH
- **Depends on**: RPG-001, RPG-002
- **Category**: architecture/contract
- **Planned at**: `codex/202607071457-ui-optimize-20260710102915-remote-provider-gateway`

## Why this matters

**Background**: Legacy and Codex 0.144.1 fixtures are frozen, but the design still
contains name-only types and policies with more than one valid implementation.

**Current state**: `remote-provider-gateway-full-design.md` identifies the
domain, planner, auth, retry, attach and recovery concepts without fully defining
their fields, invariants, ownership or deterministic transition outcomes.

**Impact**: Phase 1 code would otherwise invent persistence, recovery and auth
contracts while implementation is already in progress, making fixtures and
security tests unstable.

**What improves**: Every Phase 1 implementation decision becomes explicit and
maps to a later named test.

## Scope

**In scope**:

- Normative domain/auth/config/planner types and invariants.
- Provider mutation and profile binding lifecycle policy.
- Store locking, atomic replacement, journal and recovery policy.
- Dry-run fingerprint and replay policy.
- Retry ownership and adapter concurrency/output contract.
- DeepSeek reasoning handling and decision-to-test traceability.

**Out of scope**:

- Rust or frontend implementation.
- Threat/platform/sidecar security policy owned by RPG-004.
- Automated Phase 0 gate owned by RPG-005.

## Design

The full design is the normative artifact. Types use closed discriminated unions
for MVP behavior; unknown values fail validation. Profile bindings cover only
externally attached API profiles. One installation-wide cross-process mutation
lock serializes Provider Hub writes; config ownership is still protected with an
expected hash because Codex and users do not honor that lock. A journal has one
commit point: binding CAS after config commit. Dry-run plans are bounded,
short-lived and one-shot. Gateway application retries are disabled, and raw
provider reasoning is not relabeled as a Responses summary without evidence.

## Tasks

### Task overview

| ID  | Task                                      | Acceptance summary                                                            | Status   |
| --- | ----------------------------------------- | ----------------------------------------------------------------------------- | -------- |
| T1  | Lock normative contracts and traceability | No critical type or transition is ambiguous; all decisions map to later tests | 验证成功 |

### T1: Lock normative contracts and traceability

**Status**:

- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:

Phase 1 must implement one agreed domain and recovery model instead of choosing
among unresolved alternatives.

**What to do**:

- Define every type and policy listed in RPG-003.
- Add deterministic examples for attach, adoption, rebind, detach, drift,
  route-breaking Provider edits and crash recovery.
- Add a decision → issue → named test traceability table.
- Update the master todo issue status and evidence.

**Logic design**:

- Closed enums reject unknown protocol/auth/route/capability values.
- Domain models contain references only; resolved secret values never enter
  serialization or debug output.
- Installation mutations use one lock and CAS/hash checks at external boundaries.
- Recovery rolls back before binding commit and rolls forward after it; ownership
  conflicts stop automatic recovery without destructive replacement.
- Adapter registry objects are `Send + Sync`; per-request exchanges are `Send`
  and single-owner, with disjoint non-stream and stream outputs.

**Test design**:

- Documentation-only exception: no executable failure state is manufactured.
- Review each required RPG-003 decision against the full design and reject the
  issue if any concept remains name-only or has an `either/or` MVP outcome.
- Validate that every normative decision maps to a concrete later test name.

**Acceptance**:

- `rg` finds all required type names in normative definitions.
- `rg` finds no unresolved phrases such as `later decide`, `reject or preserve`,
  or an implicit retry owner in the MVP contract.
- Markdown formatting and repository diff checks pass.

**Done criteria**:

- [x] Documentation-only test exception is recorded; no executable failure state exists before a normative document change, so review scans and later named red tests are the substitute
- [x] All required domain/auth/config types have fields and invariants
- [x] All mutation, drift, recovery and replay transitions are deterministic
- [x] Retry and adapter concurrency/output ownership are explicit
- [x] Reasoning behavior does not relabel raw provider reasoning as summary
- [x] Traceability table covers every RPG-003 decision group
- [x] Focused review commands pass
- [x] Task overview and master issue status match `验证成功`

## Test plan

- Required-name coverage using `rg`.
- Ambiguous-language scan using `rg`.
- Prettier Markdown check.
- Manual cross-check against RPG-003 required decisions.

## Verification commands

| Purpose            | Command                                                                                                                                                                                                                                                                             | Expected on success                   |
| ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- |
| Required contracts | `rg -n -e ProviderProtocol -e RouteKind -e ProviderModel -e CodexProviderOptions -e CredentialSource -e UpstreamAuth -e AuthCommand -e GatewayCredentialReference -e ManagedConfigPatch -e ManagedConfigProjection -e ReadinessBlocker docs/remote-provider-gateway-full-design.md` | Every name has a normative definition |
| Ambiguity scan     | `rg -n -e 'later decide' -e 'reject or preserve' -e TBD -e 待定 docs/remote-provider-gateway-full-design.md`                                                                                                                                                                        | No unresolved MVP decision            |
| Format             | `pnpm exec prettier --check ../../docs/remote-provider-gateway-full-design.md ../../docs/todo-remote-provider-gateway.md ../../docs/todo-rpg-003-normative-contracts.md` from `apps/desktop`                                                                                        | exit 0                                |

## Done criteria

- [x] T1 is `验证成功`
- [x] T1 checklist is complete
- [x] Master todo and full design link the evidence
- [x] No STOP condition remains

## STOP conditions

- Captured Codex behavior contradicts a proposed contract.
- A required decision depends on the RPG-004 threat model and cannot be isolated.
- Completing the contract would silently expand the exact-tested platform or
  Responses subset.
