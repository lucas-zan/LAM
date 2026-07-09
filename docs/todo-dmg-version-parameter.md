# Todo: DMG version parameter

> Executor instructions: Follow this todo step by step. Generate tests from the
> "Test design" section before implementation. Run each verification command and
> confirm the expected result before moving to the next task.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: feature
- **Planned at**: current working tree

## Why this matters

**Background**: `make dmg` currently builds with the versions already stored in `tauri.conf.json`, `package.json`, and `Cargo.toml`. The user wants `make dmg VERSION=x.y.z` to set all three versions before packaging, while plain `make dmg` should preserve existing versions.

**Current state**: `Makefile` maps `build dmg` directly to `cd apps/desktop && npm run tauri:build`. Version values are manually duplicated in three files.

**Impact**: Release builds require manual version edits and are easy to make inconsistent.

**What improves**: A single build parameter can produce a correctly versioned DMG while keeping the current no-parameter behavior unchanged.

## Scope

**In scope**:
- Add a small version synchronization script.
- Add focused tests for the script.
- Update `Makefile` so `VERSION` triggers synchronization before packaging.

**Out of scope**:
- Changing Tauri bundle configuration beyond version inputs.
- Adding release tagging, git commits, changelog generation, or notarization.

## Design

When `VERSION` is not provided, `make dmg` should behave exactly as it does now. When `VERSION` is provided, Makefile should call a script before `npm run tauri:build`. The script should update:

- `apps/desktop/package.json` JSON `version`
- `apps/desktop/src-tauri/tauri.conf.json` JSON `version`
- `apps/desktop/src-tauri/Cargo.toml` package `version`

The script should validate basic semver-like versions before writing. JSON files should be updated with JSON parsing/stringifying. `Cargo.toml` is small and only needs the package-level `version = "..."` line updated before dependency sections.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Add tested version sync script | Focused node test proves all three files update and invalid versions fail | 验证成功 |
| T2 | Wire Makefile VERSION parameter | `make dmg VERSION=x.y.z` invokes sync before build; plain `make dmg` does not mutate versions | 验证成功 |

### T1: Add tested version sync script

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
Release version synchronization should be isolated and testable instead of embedding fragile shell edits in Makefile.

**What to do**:
- Create a script under `apps/desktop/scripts/`.
- Support `--root <path> <version>` so tests can run against temp fixtures.
- Update JSON files via JSON APIs and Cargo package version via a constrained regex.
- Reject empty or invalid version values.

**Logic design**:
- Parse CLI args.
- Validate version with `major.minor.patch` plus optional prerelease/build suffix.
- Resolve repo root, update all three target files.
- Fail with a clear message and nonzero exit when input or files are invalid.

**Test design**:
- Node test copies minimal fixture files to a temp root, runs the script, and asserts all three versions are updated.
- Node test runs an invalid version and asserts a nonzero exit.
- Expected initial failure: script file does not exist or command exits nonzero.

**Acceptance**:
- `node --test apps/desktop/scripts/sync-release-version.test.mjs` passes after implementation.

**Done criteria**:
- [x] Tests listed in this task's Test design were written before implementation
- [x] New tests were run and confirmed to fail for the expected reason before implementation
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification command passes
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

### T2: Wire Makefile VERSION parameter

**Status**:
- [ ] 待执行
- [ ] 待测试验证
- [x] 验证成功
- [ ] 验证失败

**Why**:
The user-facing command is `make dmg VERSION=...`; the script alone does not satisfy the workflow.

**What to do**:
- Update `make help` text.
- Add conditional Makefile logic so `VERSION` calls the sync script before `npm run tauri:build`.
- Preserve current behavior when `VERSION` is unset.

**Logic design**:
- Keep the existing `build dmg` target.
- Insert a shell conditional in the target body:
  - if `VERSION` is non-empty, run the sync script
  - then run Tauri build

**Test design**:
- Use `make -n dmg VERSION=1.2.3` to assert the sync command appears before build.
- Use `make -n dmg` to assert no sync command appears.

**Acceptance**:
- `make -n dmg VERSION=1.2.3` shows the version sync command.
- `make -n dmg` keeps the existing build flow without sync.

**Done criteria**:
- [x] Tests listed in this task's Test design were run before implementation or the exception is documented
- [x] Implementation follows this task's Logic design and stays inside this task's What to do
- [x] Focused verification commands pass
- [x] Task overview row status matches this task status
- [x] This task status is updated to `验证成功`

## Test plan

- Normal behavior: valid version updates all three files.
- Invalid input: invalid version exits nonzero.
- Make integration: dry-run includes sync when `VERSION` is set and excludes it when unset.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Focused tests | `node --test apps/desktop/scripts/sync-release-version.test.mjs` | exit 0 after implementation; expected failure before implementation |
| Make dry run with version | `make -n dmg VERSION=1.2.3` | sync command appears before build |
| Make dry run without version | `make -n dmg` | sync command absent |

## Done criteria

- [x] Every task's own Done criteria checklist is fully checked
- [x] Every task has exactly one checked Status value, and it is `验证成功`
- [x] Task overview shows every task as `验证成功`
- [x] No STOP condition remains unresolved

## STOP conditions

Stop and report if:

- The current Makefile no longer owns `make dmg`.
- The Tauri version source is no longer `tauri.conf.json`.
- Tests cannot isolate file writes in a temp directory.
