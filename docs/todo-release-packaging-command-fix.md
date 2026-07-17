# Todo: Fix release packaging commands and artifact metadata

> Executor instructions: Follow this todo step by step. Write tests from each
> task's Test design before implementation, confirm the expected failure, then
> implement and verify before advancing.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: MEDIUM
- **Depends on**: macOS Tauri packaging pipeline
- **Category**: bugfix/refactor
- **Planned at**: current workspace

## Why this matters

**Background**: `make dmg VERSION=0.3.0` builds a 0.3.0 app but writes it to the hard-coded
`LAM_0.2.1_aarch64.dmg` path. `make build` and `make dmg` also share one recipe, while the
`tauri:build` npm script always creates a DMG.

**Current state**: release version synchronization updates package.json, tauri.conf.json and
Cargo.toml only. Gateway component manifests, DMG filenames, architecture metadata and the
package lock root remain hard-coded to 0.2.1/aarch64.

**Impact**: successful releases appear missing, overwrite artifacts from another version,
and make command names misrepresent their outputs.

**What improves**: one release metadata source drives app manifests and artifact names;
`make build` produces an app, while `make dmg` explicitly adds the installer.

## Scope

**In scope**:
- Derive Gateway manifest version and DMG name from package release metadata.
- Derive architecture labels from the Rust host triple.
- Synchronize package-lock root versions.
- Separate Make/npm app-build and DMG workflows.
- Add deterministic unit/source-contract tests without performing a real release build.

**Out of scope**:
- Running a full signed DMG build during automated tests.
- Universal binaries, notarization or changing signing identities.
- Deleting the already-generated incorrectly named artifact.

## Design

`package.json` is the packaging metadata source after `sync-release-version.mjs` runs.
`releaseMetadata()` validates its semver, maps supported macOS host triples to artifact
architecture labels (`aarch64`, `x64`) and returns the DMG basename. Prepare, dev, finalize
and DMG stages consume this contract. Make targets call distinct npm scripts: app build stops
after final signing; DMG calls app build then the installer stage. The version synchronizer
updates both package.json and the two npm lockfile root version fields atomically per file.

## Tasks

### Task overview

| ID | Task | Acceptance summary | Status |
|----|------|--------------------|--------|
| T1 | Dynamic release metadata | No 0.2.1/aarch64 hard-coding in packaging outputs | 验证成功 |
| T2 | Separate build and DMG commands | `make build` app-only; `make dmg` adds DMG | 验证成功 |
| T3 | Version sync and regression | Lockfile synchronized; packaging tests pass | 验证成功 |

### T1: Dynamic release metadata

**Status**:
- [x] 验证成功

**Why**: The hard-coded output silently overwrites an artifact with the wrong filename.

**What to do**:
- Add testable release metadata helpers to the Gateway packaging script.
- Use dynamic version/architecture for component manifests and DMG/checksum names.
- Permit supported Apple Silicon and Intel macOS triples; reject other targets explicitly.

**Logic design**:
- Read and validate package.json version once per packaging phase.
- Map `aarch64-apple-darwin` → `aarch64`, `x86_64-apple-darwin` → `x64`.
- Generate `LAM_<version>_<architecture>.dmg` and the matching checksum entry.

**Test design**:
- Unit test 0.3.0/aarch64 and prerelease/x64 artifact metadata.
- Test unsupported triples and invalid/missing versions.
- Before implementation, assert source has no literal 0.2.1 output and confirm failure.

**Acceptance**:
- `node --test apps/desktop/scripts/package-gateway-components.test.mjs`

**Done criteria**:
- [x] Tests written first and failed because the release metadata module did not exist (2026-07-17).
- [x] All packaging phases use dynamic metadata.
- [x] Focused tests pass (4/4).
- [x] Status is `验证成功`.

### T2: Separate build and DMG commands

**Status**:
- [x] 验证成功

**Why**: Command names must accurately describe build side effects.

**What to do**:
- Make npm `tauri:build` stop after app finalization.
- Add npm `tauri:dmg` to build the app and then create/verify the DMG.
- Give Makefile `build` and `dmg` separate recipes, both optionally syncing VERSION.

**Logic design**:
- Shared prerequisites and version synchronization may be expressed through a helper target.
- `make build` invokes only `npm run tauri:build`.
- `make dmg` invokes `npm run tauri:dmg`.

**Test design**:
- Source-contract test package scripts and Make recipes.
- Expected initial failure: tauri:build contains package-gateway-dmg and Make combines targets.

**Acceptance**:
- `node --test apps/desktop/scripts/package-gateway-components.test.mjs`
- `make -n build` and `make -n dmg VERSION=0.3.0` show different final npm commands.

**Done criteria**:
- [x] Failure-first test recorded: `tauri:build` still invoked `package:gateway-dmg` (2026-07-17).
- [x] Build/DMG responsibilities are distinct.
- [x] Focused tests and root-level dry-run inspection pass.
- [x] Status is `验证成功`.

### T3: Version sync and regression

**Status**:
- [x] 验证成功

**Why**: A release version should not leave npm metadata internally inconsistent.

**What to do**:
- Extend version synchronization to package-lock.json root fields.
- Update tests and current packaging documentation/help.
- Run Node packaging tests, frontend checks and diff hygiene.

**Logic design**:
- Preserve all dependency versions; update only lockfile top-level version and root package version.
- Missing lockfile is a clear error for this repository workflow.

**Test design**:
- Fixture test verifies both lockfile fields change while a dependency version is preserved.
- Invalid semver remains rejected without partial mutation.

**Acceptance**:
- `node --test apps/desktop/scripts/package-gateway-components.test.mjs apps/desktop/scripts/sync-release-version.test.mjs`
- `npm run lint && npm run build` from apps/desktop.
- `git diff --check`.

**Done criteria**:
- [x] Failure-first lockfile test recorded: root lock version remained 0.2.0 (2026-07-17).
- [x] Version metadata remains consistent across package, lockfile, Tauri and Cargo manifests.
- [x] Relevant tests/static checks pass (11 Node tests, ESLint, frontend build and G5 verification).
- [x] Status is `验证成功`.

## Test plan

- Normal: 0.3.0 arm64 creates metadata for `LAM_0.3.0_aarch64.dmg`.
- Edge: prerelease versions and Intel host triple produce deterministic valid names.
- Invalid input: unsupported triple and invalid semver fail before packaging.
- Error: missing/inconsistent release files produce actionable errors.
- State/conflict: build never creates DMG; dmg always runs app build first.

## Verification commands

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Packaging tests | `node --test apps/desktop/scripts/package-gateway-components.test.mjs apps/desktop/scripts/sync-release-version.test.mjs` | exit 0 |
| Make contracts | `make -n build` / `make -n dmg VERSION=0.3.0` | distinct npm commands |
| Frontend static | `npm run lint && npm run build` | exit 0 |
| Diff hygiene | `git diff --check` | exit 0 |

## Done criteria

- [x] Every task is `验证成功`.
- [x] No release artifact metadata is hard-coded to 0.2.1.
- [x] App and DMG commands have distinct side effects.
- [x] Version and lock metadata stay consistent.
- [x] No STOP condition remains unresolved.

## STOP conditions

- Tauri requires DMG creation inside its app bundle command.
- Supporting the detected host requires an unavailable sidecar target.
- Existing unrelated changes conflict with packaging files.
