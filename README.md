# LocalAgentManager (Lam)

**A local-first AI coding agent workspace manager.** LAM is **Codex-first** and supports ChatGPT/PAT accounts plus external Responses or Gateway-adapted Chat Completions API accounts.

The product invariant is **one Account = one Profile = one `CODEX_HOME`**. Use **Add Account → API Account** to name an account, enter its endpoint/models/API key, review the dry-run plan, and create it atomically. LAM creates an exclusive internal Provider connection by default; Provider reuse is an explicit advanced option. Switching models rebinds the same Account and never creates another `CODEX_HOME`.

**中文说明：** [`README.zh-CN.md`](README.zh-CN.md)

---

## Background & motivation

Common pain points when using Codex CLI day to day:

- **Multiple accounts side by side** — `~/.codex`, `~/.codex-a`, etc. each keep their own `auth.json`, `config.toml`, and `sessions/`. Switching is manual and easy to run the wrong wrapper or `CODEX_HOME`.
- **Sessions do not follow account switches** — Codex stores conversation state under a specific `CODEX_HOME`. If you change account (or quota runs out on A and you open B) **without moving the session assets**, the new account does not inherit the prior thread. You often re-open the repo, re-explain context, and **re-read code — wasting tokens**.
- **Unsafe session copy** — Copying account A’s tree into B can drag along `auth.json`, sqlite state, or cache and corrupt the target profile.
- **Provider vs runtime drift** — A transcript may still `resume`, but model/proxy/cost/tool behavior may differ; that should be visible, not silent.
- **No unified desktop surface** — The CLI is powerful but there is no single place to see accounts, sessions, quota, and handoffs.

Lam addresses this **without uploading sessions, code, or prompts**: account boundaries, provider references, session assets, and handoff live in a local Rust core + desktop UI.

Authoritative design: [`docs/FINAL-DESIGN.md`](docs/FINAL-DESIGN.md). Task tracking: [`docs/TODO.md`](docs/TODO.md), [`docs/IMPLEMENTATION-ISSUES.md`](docs/IMPLEMENTATION-ISSUES.md).

---

## Goals

| Area | Intent |
|------|--------|
| **Local-first** | Data stays on disk; no cloud session sync. |
| **Clear boundaries** | Account (`CODEX_HOME`) · Provider (metadata + secret refs) · Session (`sessions/` assets). |
| **Safe defaults** | Handoff copies only the selected session asset; **never copy `auth.json`**. |
| **Auditable writes** | Create account and attach-provider mutations use **plan → confirm → execute**. |
| **Codex-first delivery** | Scan, resume, single-session handoff, and provider center; `AgentAdapter` hook for Phase 2. |
| **Native desktop** | **Tauri v2 + Rust + React** on **macOS** (not Electron; other OSes not planned yet). See [`docs/DESKTOP-RUNTIME.md`](docs/DESKTOP-RUNTIME.md). |

---

## Cross-account session handoff — what exists today

This is the core problem behind “switch account → session not inherited → token waste.” LAM provides a guided handoff that analyzes Provider compatibility before writing the target session, copies only allowed session data, and resumes with the target Account runtime.

### Why Codex does not inherit across accounts

Each profile is an isolated home directory:

```text
~/.codex-a/sessions/...   ← only visible when CODEX_HOME=~/.codex-a
~/.codex-b/sessions/...   ← only visible when CODEX_HOME=~/.codex-b
```

`codex resume <session-id>` always resolves sessions **under the current `CODEX_HOME`**. Changing wrapper/account without copying the right `sessions/` tree means Codex starts from scratch for that home.

### What LAM implements

| Step | Feature in app | What it does |
|------|----------------|--------------|
| 1 | **Create Relay** (`Relay` route / “+ New Relay”) | Creates a dedicated relay profile (e.g. `~/.codex-b-relay-a`) with **B’s login**, separate from source A. |
| 2 | **Handoff** (account card or Sessions) | Copies one selected session asset to the target profile when needed. It never copies account credentials. |
| 3 | **Login** on relay | Opens Terminal with `CODEX_HOME=<relay> codex login` so B’s auth lives in the relay home — not copied from A. |
| 4 | **Resume** (Sessions → Copy / Terminal / Details) | Builds `CODEX_HOME=<profile> codex resume <id>` and opens Terminal or clipboard. |
| 5 | **Relay / Continue** (Overview account card, tray popover) | With an active session selected, copies the session asset to the target profile when needed (`relay_resume_session`), then opens Terminal with `codex resume` (diverged-session strategies in Settings). |

End-to-end relay flow (documented in [`docs/FINAL-DESIGN.md`](docs/FINAL-DESIGN.md) §4.3–4.5):

```text
Account A (quota low)  --[Handoff one session]-->  Relay home (B’s auth)
                                                      |
                                                      v
                                            codex resume <session-id>
                                            with CODEX_HOME=relay
```

### Deliberate limits

- **No in-app Codex process** — Lam does not embed the CLI; it prepares commands and opens **Terminal.app**.
- **No automatic session copy on account switch** — switching the Sessions filter only changes which directory is listed.
- **`history.jsonl` merge** — intentionally out of scope for Phase 1.

Planned improvements: guided relay wizard, cross-profile session browsing, clearer handoff from Overview when quota is low (see **Future roadmap**).

---

## Current capabilities (initial / Phase 1)

**Rust core (`apps/desktop/src-tauri`)**

- Scan `~/.codex` and `~/.codex-*` and parse session metadata.
- Managed account / relay workspace plan & execute.
- Resume command builder (shell-escaped) + Terminal launch.
- **`relay_resume_session`:** copy/merge a single session into a target profile (with diverged strategies) and return a resume command.
- Account-first external API lifecycle: atomic create, same-home model switch, delete, and startup recovery.
- Provider metadata CRUD; API keys are write-only inputs stored in macOS Keychain and never returned in DTOs, config, plans, journals, relay reports, or metrics.
- Direct Responses and a local bearer-protected `/v1/models` + `/v1/responses` Gateway for verified Chat Completions adapters.
- Provider-aware relay analyzer: unsupported/stateful history fails closed before target writes; representation-only loss requires confirmation.
- Attach provider to profile; **provider mismatch** warnings on sessions.
- Account/quota **disk cache** for faster startup (`accounts-cache.json`, cached quota snapshots).
- **Personal Access Token (PAT) tracking:** Track PAT expiration, display auth status (Lam UI only, doesn't modify Codex files).

**Desktop UI (`apps/desktop`)**

- Routes: **Overview** (accounts + quota), **Usage**, **Sessions**, **Providers** (advanced connection management), **Settings**.
- The Accounts heading toggles compact overflow menus or fully expanded card actions; API Accounts use **View&Edit** instead of Login.
- **Add Account → API Account** is the normal external API entry point; model switching is available on that Account.
- Quota via Codex app-server (5h / weekly); shows **N/A** when unavailable (no fake percentages).
- **Menu bar tray (macOS):** **left-click** opens a compact **quota popover** (5h / weekly meters, per-account **Relay** or **Resume** when a latest session exists). **Right-click** for Refresh / Open app. Click outside the panel or **Close** dismisses it (main window stays hidden unless you choose **Open**). Background refresh every 5 minutes (startup also loads cached accounts/quota first, then refreshes per account in parallel).

**Explicitly out of Phase 1**

- `history.jsonl` merge.
- Cloud sync.
- Non-Codex agents (Phase 2).
- Fake quota / token estimates presented as official limits.

See [`docs/PHASE1-ACCEPTANCE.md`](docs/PHASE1-ACCEPTANCE.md), [`docs/CORRECTION-PLAN.md`](docs/CORRECTION-PLAN.md).

---

## Safety defaults

- **Never** copy `auth.json` during a handoff.
- **Block** `config.toml`, sqlite, `cache/`, `tmp/`, `logs/`, `installation_id` by default.
- Prefer relay profiles (e.g. `~/.codex-b-relay-a`) so source accounts are not polluted.
- Account creation and provider-binding write flows require a reviewed plan.

---

## Roadmap

### Shipped

| Area | Delivered |
|------|-----------|
| **Accounts & relay** | Scan `~/.codex*`; create managed accounts and relay workspaces; hand off one session at a time. |
| **Quota** | Codex app-server **5h / weekly** in Overview; **menu bar tray popover**; disk cache; **per-account parallel refresh**; N/A when unavailable. |
| **Handoff** | `codex resume` commands; **`relay_resume_session`** (copy/merge session → target profile) from Overview **Relay/Continue** and tray **Relay/Resume**; diverged-session strategies in Settings. |
| **External API accounts** | Account-first Direct Responses / Gateway setup, Keychain credentials, atomic lifecycle, same-home model switch, startup recovery. |
| **Provider** | Advanced connection CRUD/reuse, health/readiness/capability provenance, attach plans, mismatch warnings. |
| **Relay safety** | Provider-aware compatibility report and fingerprint; blocked history produces zero target writes. |
| **Desktop** | Tauri v2 app; tray-first launch (main window via **Open**); click-outside to dismiss popover. |

### Next (Phase 1 wrap-up — macOS only)

**Platform scope:** **macOS only** for now. No Linux/Windows port planned; focus on polishing tray, popover, Terminal integration, and Phase 1 acceptance on Mac.

| Theme | Direction |
|-------|-----------|
| **Product & UI** | Activity timeline; **unified cross-profile Sessions** list; settings polish; [`docs/PHASE1-ACCEPTANCE.md`](docs/PHASE1-ACCEPTANCE.md) sign-off. |
| **Handoff UX** | **Guided wizard** when quota is low (single flow: choose session → target → resume). |
| **Quota & relay tuning** | Edge cases, staleness UX, docs/tests alignment for provider + quota paths. |
| **macOS polish** | Menu bar tray UX, popover focus/dismiss, app-server quota reliability, signed bundle readiness. |

### Later

| Phase | Direction |
|-------|-----------|
| **Phase 2** | `AgentAdapter`; Claude Code / OpenCode without weakening Codex safety defaults. |

Details: [`docs/FINAL-DESIGN.md`](docs/FINAL-DESIGN.md), [`docs/TODO.md`](docs/TODO.md).

---

## Requirements

- **Node.js** + **npm**
- **Rust** + **Cargo**
- **macOS** — supported target platform (tray, Terminal integration); Linux/Windows not planned yet. Full Xcode only for signed bundles
- Optional: **Codex CLI** logged in (real quota/sessions); repo **`.fake-home`** for offline tests

---

## Quick start

```bash
make install
make start
```

Fixture home (does not touch real `~/.codex*`):

```bash
LAM_HOME="$(pwd)/.fake-home" make start
```

- `make start` launches the **Tauri app** (menu bar tray on macOS). The **main window starts hidden**; open it from the tray (**Open** in the popover or right-click → Open LocalAgentManager). The Vite URL in the terminal is only the embedded dev server, not a browser-only product.
- By default Lam scans your real home. Set `LAM_HOME` explicitly to use fixtures.

---

## Common commands

| Command | Purpose |
|---------|---------|
| `make start` | Tauri dev (foreground; Ctrl+C to stop) |
| `make stop` | Stop Tauri/Vite started from this repo |
| `make check` | Frontend build + UI smoke + `cargo fmt --check` + `cargo test` |
| `make build` | Production frontend + Tauri bundle (local `.dmg` under `src-tauri/target/release/bundle/dmg/`) |
| Git tag `v*` | Push e.g. `v0.1.0` → [`.github/workflows/release.yml`](.github/workflows/release.yml) builds macOS `.dmg` on GitHub Releases |
| `make status` | Node / Rust / Tauri info |
| `make accounts` | CLI scan (`lam-core`) |
| `make help` | All Make targets |

---

## Tests

**Rust:**

```bash
cd apps/desktop/src-tauri
cargo test
LAM_HOME="$(pwd)/../../.fake-home" cargo test
```

**Frontend:**

```bash
cd apps/desktop
npm run build
npm run test:ui
npm run test:gateway-phase0
npm run test:gateway-g5
```

**Scanner only:**

```bash
cd apps/desktop/src-tauri
LAM_HOME=/path/to/.fake-home cargo run --bin lam-core
```

---

## Repository layout

```text
apps/desktop/              # React + Vite UI
apps/desktop/src-tauri/    # Tauri + Rust core
.fake-home/                # Test fixtures (no sqlite/cache/real auth in git)
account-manager/           # HTML prototypes (reference)
docs/                      # Specs, TODO, acceptance
Makefile                   # Dev entrypoint
```

---

## Documentation index

| Doc | Purpose |
|-----|---------|
| [`docs/FINAL-DESIGN.md`](docs/FINAL-DESIGN.md) | Master product & technical spec |
| [`docs/TODO.md`](docs/TODO.md) | Implementation checklist |
| [`docs/IMPLEMENTATION-ISSUES.md`](docs/IMPLEMENTATION-ISSUES.md) | Issue-level tracking |
| [`docs/DESKTOP-RUNTIME.md`](docs/DESKTOP-RUNTIME.md) | Tauri architecture & troubleshooting |
| [`docs/CORRECTION-PLAN.md`](docs/CORRECTION-PLAN.md) | UI/spec gap plan |
| [`docs/PHASE1-ACCEPTANCE.md`](docs/PHASE1-ACCEPTANCE.md) | Phase 1 manual acceptance |

---

## License

[MIT License](LICENSE) — free to use, modify, merge, publish, distribute, sublicense, and sell copies.

Redistributions should include the copyright notice and MIT license text from [`LICENSE`](LICENSE). **Attribution in README or About** (e.g. “Based on LocalAgentManager”) is appreciated but not required by the license.
