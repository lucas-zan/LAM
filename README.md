# LAM — Codex Account, Quota & Session Manager for macOS

Manage multiple Codex CLI accounts, quota, usage, and sessions on macOS—and continue an existing session with another account.

**Early Preview** · **macOS only** · **Codex only** · **Local-first, not fully offline**

LAM is a macOS menu bar app for people who keep multiple Codex CLI profiles. It scans local `CODEX_HOME` directories, shows quota and local token usage, browses and resumes sessions, and safely hands one existing session from one account to another.

![LAM account, quota, usage, and session overview](docs/assets/lam-overview.png)

LAM does not embed Codex. A resume or handoff launches Codex in an external terminal—Terminal.app by default—with the selected profile's `CODEX_HOME`.

## Why LAM

A Codex session belongs to the `CODEX_HOME` in which it was created. When one account reaches its usage limit, switching to another account does not automatically make the previous session available. LAM helps prepare and resume that session with another profile.

Without a dedicated tool, this usually means finding the correct session JSONL, copying it without leaking account state, selecting the right target profile, and running `codex resume` with the correct environment. Copying an entire `~/.codex*` directory is unsafe because it can also copy authentication, configuration, databases, caches, and other account-specific state.

In practical terms, LAM combines a Codex CLI account manager and Codex session manager with Codex quota, Codex token usage, a local Codex usage tracker, an estimated Codex cost tracker, and safe Codex session handoff.

LAM provides one local surface for:

- multiple Codex accounts and `CODEX_HOME` profiles;
- Codex quota and reset-credit visibility;
- local Codex token usage, calls, threads, and estimated cost;
- session browsing, resume, conflict handling, and cross-account handoff;
- custom OpenAI-compatible Provider profiles; and
- quick quota access from the macOS menu bar.

## Is LAM for you?

LAM is intended for:

- macOS users who use Codex CLI heavily;
- developers who maintain `~/.codex` plus one or more `~/.codex-*` profiles;
- people who regularly switch between multiple Codex accounts;
- people who need to continue an existing session after another account runs out of quota;
- people who want local Codex quota, token, call, thread, and cost analytics; and
- advanced users of custom OpenAI-compatible Responses or Chat Completions providers.

LAM is probably not a good fit if you:

- use Windows or Linux;
- only use one Codex account and do not need local usage analytics;
- need Claude Code or OpenCode support today;
- need cloud sync or access from multiple machines;
- expect to run Codex inside the LAM window; or
- are not comfortable handling PATs, session JSON, or authentication files.

## Core workflow

1. **Discover profiles.** LAM scans `~/.codex` and `~/.codex-*`, then shows accounts, session counts, quota, and the latest local sessions.
2. **Choose a session.** Open **Handoff Session** from an account or session and select the source account, source session, and target account in the UI.
3. **Prepare the target safely.** LAM copies only the selected session JSONL into the corresponding path under the target `CODEX_HOME`. Provider compatibility is checked before a cross-provider write. Existing target sessions are compared and handled with an explicit divergence strategy.
4. **Continue in Codex.** LAM builds the target command and launches `CODEX_HOME=<target> codex resume <session-id>` in the selected external terminal.

## Features

### Account and Profile management

- Scans the main `~/.codex` profile and sibling `~/.codex-*` profiles.
- Shows profile paths, authentication state, session counts, active-auth state, Provider/model metadata, renewal dates, and notes.
- Creates isolated managed profiles and shell wrappers.
- Renames and deletes managed non-main profiles with conflict checks.
- Opens `codex login` with the selected `CODEX_HOME`.
- Keeps accounts isolated: one Profile maps to one `CODEX_HOME`.

### Quota

- Shows the rate-limit windows returned by Codex or ChatGPT, commonly the 5-hour and weekly windows, including reset times when available.
- Shows cached real quota when live refresh fails and `N/A` when no real value is available.
- Shows reset-credit counts and expiry details when upstream responses provide them.
- Refreshes accounts independently and surfaces quota in the main window and menu bar popover.
- Does not invent quota percentages for custom external API accounts.

Quota is different from local Usage analytics: quota is upstream account state; Usage is derived from local Codex event data.

### Sessions, Resume, and Handoff

- Browses sessions for a selected account and reads IDs, working directories, names, summaries, models, timestamps, and Provider mismatch metadata.
- Copies or opens a shell-escaped resume command.
- Launches `codex resume` in Terminal.app by default. Ghostty and cmux targets are also present in Settings.
- Lets the user select the source account, source session, and target account in one Handoff dialog.
- Copies a missing session, extends a target that is an older prefix, or detects a divergence.
- Supports divergence strategies that preserve a backup, prefer the source, keep the target and create a source fork, merge timelines into a fork, or prepare handoff material for target-account summarization.
- Blocks incompatible cross-Provider histories before writing the target; representation-only loss requires confirmation.

### Safe Session Sync

Safe Session Sync is the copy step inside Session Handoff. It is intentionally narrow:

- Only the selected session JSONL under `sessions/` is copied.
- The relative session path is preserved under the target `CODEX_HOME`.
- An existing divergent target is backed up before a configured conflict strategy proceeds.
- There is no exposed bulk `sessions/` directory sync command.

Because LAM copies one selected session file rather than a profile tree, the handoff does **not** copy:

- `auth.json` or `auth-f.json`;
- `config.toml`;
- `history.jsonl`;
- SQLite files such as `logs_2.sqlite`, `state_*.sqlite`, or LAM's usage database;
- `cache/`;
- `tmp/`;
- `log/` or `logs/`; or
- `installation_id`.

Session Handoff is not a dry-run workflow. Provider compatibility is analyzed before target writes, while session divergence is handled through backups and explicit strategies.

### Local Usage Analytics

The **Usage** route indexes local Codex JSONL events into a LAM-owned SQLite database under `.codex/lam/usage/`. It supports account/workspace scopes, active or archived history, time ranges, search, model, effort, pricing-confidence, and pagination filters.

The UI currently shows:

- total, input, cached input, uncached input, output, and reasoning-output tokens;
- call counts, thread counts, activity heatmaps, streaks, and longest-turn metrics;
- per-call model, effort, duration, context, cache ratio, and estimated cost;
- per-thread calls, sessions, tokens, cache ratio, recommendations, and estimated cost;
- pricing coverage, unknown models, parser diagnostics, and skipped events; and
- on-demand local request, assistant, and tool-output details from the source log.

Cost values are local estimates from the built-in rate card, not OpenAI invoices or billing balances. The current “Codex Credits” card mirrors estimated cost; it is not a real credit balance.

![Local Codex token usage, calls, threads, and estimated cost](docs/assets/lam-usage.png)

### Provider Profiles

- Creates, edits, tests, and deletes Provider profiles.
- Discovers or configures model allowlists and a default model.
- Supports direct OpenAI Responses endpoints and a local authenticated Gateway for verified Chat Completions adapters.
- Stores credentials by reference using environment variables, macOS Keychain, approved auth commands, or a Codex profile login.
- Previews Provider attach, rebind, and detach operations before execution.
- Attaches a Provider/model to a Profile and writes the managed projection to that profile's `config.toml`.
- Shows Provider/model mismatch warnings on sessions.
- Provides an account-first **External API Account** flow and same-`CODEX_HOME` model switching.

Custom Provider support is for advanced users. Compatibility varies by protocol, model, tools, streaming behavior, and upstream implementation.

![Create an External API Account and configure its models](docs/assets/lam-external-api.png)

### macOS Menu Bar

- Provides a compact account quota popover.
- Supports refresh, open-app, resume, and handoff shortcuts.
- Can run as a menu bar accessory with the Dock icon hidden.
- Keeps the main window separate from the terminal process running Codex.

![LAM menu bar quota popover with account relay actions](docs/assets/lam-menu-bar.png)

The current application routes are **Overview**, **Usage**, **Sessions**, **Providers**, and **Settings**.

## Authentication modes

LAM exposes OAuth/Profile and PAT-oriented account workflows. These modes have different write behavior.

Mode visibility and the terminal used for handoff, resume, and login are configurable in Settings.

![LAM Settings for Profile or PAT visibility and handoff terminal selection](docs/assets/lam-settings.png)

### OAuth / Profile mode

- Uses isolated Codex profiles and their existing authentication state.
- **Login** opens `CODEX_HOME=<profile> codex login`; Codex owns the login flow and writes authentication in that profile.
- Session Handoff copies the selected session only. It does not copy authentication from the source account.
- A normal Profile can also be created from pasted ChatGPT session JSON; that import converts the supplied credentials into a new profile's `auth.json`.

![Import trusted ChatGPT Session JSON into a separate Codex Profile](docs/assets/lam-import-session.png)

### PAT mode

PAT management is an advanced feature and **does modify authentication files**:

- Adding a PAT account creates a new `~/.codex-<name>` profile, `auth.json`, a minimal `config.toml`, and metadata.
- If a separate personal access token is supplied, LAM writes the PAT runtime form to `auth.json` and keeps the uploaded session credentials in `auth-f.json`.
- Uploading credentials can replace a profile's `auth.json`.
- Updating session authentication replaces that PAT profile's `auth-f.json`.
- Switching a PAT account atomically writes the selected profile's `auth.json` into the active `~/.codex/auth.json`, copies or removes `auth-f.json` to match, verifies the result, and then the UI attempts to restart the ChatGPT app.

Do not use PAT mode unless you understand which profile is the active authentication slot and have protected backups where appropriate.

## Privacy and network behavior

LAM is local-first, but it is not fully offline.

- Sessions, source code, and prompts are not uploaded to a LAM-operated server.
- There is no LAM cloud account, cloud session store, or cloud synchronization service.
- Account scanning, session browsing, handoff, and local Usage indexing happen on the Mac.
- The Usage SQLite database does not persist raw prompt/response content, although the UI can read raw call details on demand from the original local JSONL.
- Quota refresh may start `codex app-server` and may call Codex or ChatGPT services using the selected profile's authentication.
- The ChatGPT Web Backend paths used for usage and reset-credit support are upstream internal interfaces and may change without notice.
- Provider testing and Provider-backed Codex sessions contact the configured upstream Provider.
- Antigravity quota reads a configured local Antigravity language-server endpoint after inspecting the local process.
- Normal development and installation may contact npm, Cargo registries, GitHub, and configured API providers.

## Security

LAM handles sensitive local state. Review these boundaries before using advanced features:

- **Handoff boundary:** only the selected session JSONL is copied. Authentication, configuration, history, databases, caches, temporary files, logs, and installation IDs are outside the copy path.
- **Conflict boundary:** incompatible Provider history fails before target writes. Diverged local sessions are not silently overwritten; configured strategies preserve a backup or fork.
- **Credential boundary:** Provider secrets are write-only inputs and are stored or resolved through references such as Keychain or environment variables. Provider DTOs and plans are designed to remain redacted.
- **PAT boundary:** PAT import, update, and switching create or replace authentication files. This is separate from Session Handoff's no-auth-copy guarantee.
- **Export boundary:** ChatGPT Session JSON import and CPA export can contain access tokens, refresh tokens, ID tokens, session tokens, account identifiers, or authorization headers.

**Treat exported credential files like passwords.**

Do not commit exported credentials, paste them into issues, or send them through untrusted channels. The repository fixtures use synthetic data; never replace them with real authentication files.

See [Security and data safety](docs/03-security-and-data-safety.md) for the existing security notes. Some statements in older design documents describe superseded plans, so source code and tests are the current authority.

## Advanced and experimental features

These features exist, but they are not part of the safest core Profile → Session → Handoff path:

- **PAT account import and switching** — creates and replaces auth files and can restart the ChatGPT app.
- **ChatGPT Session JSON import** — converts pasted access/refresh/ID/session token data into a new Codex profile.
- **CPA Credential Export** — exports credential material for compatible tooling.
- **Reset Credits** — consumes an upstream reset credit when available; the action requires confirmation and changes upstream account state.
- **Antigravity Quota** — queries a locally running Antigravity language server on a manually configured port.
- **ChatGPT usage path and fallback** — uses internal ChatGPT Web Backend endpoints when usable credentials are present, with Codex app-server and cached-data fallback behavior.
- **External Provider Gateway** — adapts supported Chat Completions providers to the Responses contract; unsupported or stateful histories fail closed.

Internal upstream APIs and third-party Provider behavior can change independently of LAM.

## Current limitations

- **Early Preview:** expect rough edges, upstream compatibility changes, and incomplete manual validation.
- **macOS only:** Windows and Linux are not supported.
- **Codex only:** Claude Code and OpenCode adapters are not implemented.
- **Apple Silicon download:** the current v0.3.0 Release provides an `aarch64` DMG. No Intel or universal DMG is published there.
- **Signing status:** the repository and v0.3.0 Release metadata do not establish Developer ID signing or Apple notarization. Confirm before treating the DMG as a production-distributed app.
- **External terminal required:** Codex is not embedded in LAM. Terminal.app is the default target; Ghostty and cmux are optional targets.
- **Per-account Sessions view:** the Sessions page filters one profile at a time. The Handoff dialog provides the cross-profile source/session/target workflow.
- **No bulk Safe Sync:** bulk `sessions/` directory synchronization was removed. Handoff copies one selected session JSONL.
- **No `history.jsonl` merge:** handoff does not merge command history.
- **Provider compatibility is bounded:** unsupported tool or stateful history can block cross-Provider handoff.
- **Quota is best effort:** it depends on Codex/ChatGPT authentication and upstream behavior. External API accounts show no fabricated quota.
- **Usage is local and approximate:** missing or archived logs affect totals; cost is estimated, not billed cost.
- **No cloud sync:** profiles and sessions remain on one Mac unless the user moves them independently.
- **Manual acceptance is incomplete:** every item in `docs/PHASE1-ACCEPTANCE.md` is currently unchecked.
- **Repository check is not green at this audit:** frontend build, UI smoke, Vitest, and `cargo test` pass, but `make check` stops on four existing Clippy warnings treated as errors.

## Installation

### Download the Preview release

The latest published release is [v0.3.0](https://github.com/lucas-zan/LAM/releases/tag/v0.3.0) for Apple Silicon:

- `LAM_0.3.0_aarch64.dmg`
- `LAM_0.3.0_aarch64.dmg.sha256`

Treat it as a Preview build. Signing and notarization are not confirmed by the repository metadata.

LAM requires a working Codex CLI installation for session resume and live Codex features.

### Install from source

Requirements:

- macOS;
- Node.js and npm;
- Rust and Cargo; and
- Codex CLI for real accounts, quota, sessions, and resume.

```bash
git clone https://github.com/lucas-zan/LAM.git
cd LAM
make install
make start
```

`make start` runs the native Tauri development app. Vite is only the embedded renderer development server.

To start with the repository's synthetic fixtures instead of scanning real `~/.codex*` profiles:

```bash
LAM_HOME="$(pwd)/.fake-home" make start
```

The fixtures contain synthetic auth-shaped JSON for scanner tests. They are not usable credentials.

## Development

Common commands from the repository root:

| Command | Purpose |
| --- | --- |
| `make install` | Install frontend dependencies when `node_modules` is absent |
| `make start` | Package development sidecars and run `tauri dev` |
| `LAM_HOME="$(pwd)/.fake-home" make start` | Run against the tracked synthetic fixture home |
| `make accounts` | Scan accounts through the `lam-core` CLI |
| `make check` | Frontend build, UI smoke, Rust format, Clippy, and Rust tests |
| `make build` | Build the macOS `.app` bundle |
| `make dmg` | Build the `.app` and a versioned DMG |
| `make status` | Show Node, npm, Rust, and Tauri environment information |

Focused test commands:

```bash
cd apps/desktop
npm test
npm run test:ui

cd src-tauri
cargo test
```

Audit result for commit `e2e41bd` on 2026-07-27:

- `npm run build`: passed;
- `npm run test:ui`: passed;
- `npm test`: 22 files and 228 tests passed;
- `cargo test`: passed, with explicitly ignored environment/load probes;
- `make check`: failed before `cargo test` because four existing Clippy warnings are promoted to errors; and
- manual acceptance: not recorded as complete.

Do not interpret unit and integration test results as signed-release, notarization, or real-account end-to-end validation.

## Repository structure

```text
apps/desktop/                 React, TypeScript, Vite, Zustand, and UI tests
apps/desktop/src-tauri/       Tauri commands, Rust services, binaries, and Rust tests
.fake-home/                   Tracked synthetic fixture home used by local development/tests
examples/fake-home/           Smaller example profile fixture
docs/                         Product, security, runtime, contracts, designs, and TODO records
docs/assets/                  Product screenshots used by this README
plans/                        Historical implementation plans and reports
Makefile                      Supported root development commands
LICENSE                       MIT license
```

The current user-facing routes live in `apps/desktop/src/routes/`; Tauri command registration lives in `apps/desktop/src-tauri/src/main.rs`; command adapters live in `apps/desktop/src-tauri/src/commands/`; and core behavior lives in `apps/desktop/src-tauri/src/services/`.

Useful existing documents:

- [Desktop runtime](docs/DESKTOP-RUNTIME.md)
- [Security and data safety](docs/03-security-and-data-safety.md)
- [Tauri command contracts](docs/05-tauri-command-contracts.md)
- [Phase 1 manual acceptance](docs/PHASE1-ACCEPTANCE.md)
- [Remote Provider Gateway contract coverage](docs/remote-provider-gateway-contract-coverage.md)

Several existing design documents contain obsolete Phase plans or removed bulk-sync behavior. Future documentation cleanup should extract current, source-aligned guides into:

- `docs/ARCHITECTURE.md`
- `docs/SECURITY.md`
- `docs/PAT-MODE.md`
- `docs/SESSION-HANDOFF.md`
- `docs/PROVIDERS.md`
- `docs/DEVELOPMENT.md`
- `docs/TROUBLESHOOTING.md`

These focused files do not exist yet and are not linked as completed documentation.

## Roadmap

- Claude Code adapter.
- OpenCode adapter.
- Windows and Linux support, if maintainers choose to expand beyond the current macOS scope.
- Signed and notarized macOS distribution, plus Intel or universal builds.
- Complete and record the real-account manual acceptance matrix.
- Replace internal ChatGPT Web Backend dependencies where stable public interfaces become available.
- Finish the focused documentation split listed above.

Roadmap items are not current features.

## License

[MIT](LICENSE) © 2026 LocalAgentManager contributors.
