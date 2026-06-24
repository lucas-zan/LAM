# LAM (LocalAgentManager)

**LAM** is a **macOS menu bar app** for people who run **multiple [Codex CLI](https://github.com/openai/codex) accounts** (`~/.codex`, `~/.codex-a`, …). It shows quota at a glance, lists sessions per account, and helps you **move conversations to another account safely** — without uploading your code or chat history to the cloud.

**中文说明：** [`README.zh-CN.md`](README.zh-CN.md)

---

## Why use LAM?

If you use more than one Codex profile, you have probably run into this:

- Each profile is a **separate folder** with its own `auth.json`, `config.toml`, and `sessions/`.
- `codex resume` only finds sessions under the **current** `CODEX_HOME`.
- When account A runs out of quota and you switch to B, **the conversation does not come with you** unless you copy the right files — and copying the whole folder can break login or pollute B’s environment.

LAM keeps everything **on your Mac**. It scans local profiles, reads real quota (5h / weekly), and helps you **copy only what is safe**, then opens Terminal with the right `codex resume` command.

---

## What LAM does

| Area | What you get |
|------|----------------|
| **Menu bar tray** | Left-click for a quota panel; Relay / Resume shortcuts when a recent session is known |
| **Overview** | All Codex accounts, quota bars, renewal notes, per-account actions |
| **Sessions** | Browse, search, copy resume command, open Terminal, hand off a session |
| **Sync** | Bulk-copy `sessions/` between profiles (dry-run first) |
| **Providers** | Manage API endpoint metadata and attach to profiles (secrets stay in Keychain / env) |
| **Settings** | Theme, home root, strategy when two accounts edited the same session differently |

LAM does **not** run Codex inside the app. It prepares commands and opens **Terminal.app**.

---

## Install

### From a release DMG (recommended)

1. Download **`LAM_<version>_*.dmg`** from [GitHub Releases](https://github.com/lucas-zan/LAM/releases).
2. Open the DMG and drag **LAM** into **Applications**.
3. Launch LAM. A **menu bar icon** appears; the main window opens as a normal app.

**Unsigned builds:** macOS may block the first launch. Use **right-click → Open**, or allow the app under **System Settings → Privacy & Security**.

**You need:**

- macOS (match the DMG architecture — e.g. Apple Silicon vs Intel).
- **Codex CLI** installed; at least one profile logged in for real quota and sessions.

### From source (developers)

Requirements: **Node.js**, **npm**, **Rust**, **Cargo**, **macOS**.

```bash
git clone https://github.com/lucas-zan/LAM.git
cd LAM
make install
make start
```

Test fixtures without touching your real `~/.codex*`:

```bash
LAM_HOME="$(pwd)/examples/fake-home" LAM_ALLOW_FAKE_HOME=1 make start
```

Build a DMG locally: `make dmg`  
Release builds: push a tag like `v0.1.0` (see [`.github/workflows/release.yml`](.github/workflows/release.yml)).

---

## Daily use

### Menu bar tray

- **Left-click** — quota popover for every account (5h and weekly bars).
- **Relay / Resume** on a row — same as **Relay Latest** on that account (see below).
- **Right-click** — refresh quotas or open the main window.
- Click outside the panel or **Close** to dismiss it.

### Main window

Bottom navigation: **Overview · Sessions · Providers · Sync · Settings**

Toolbar: **Refresh**, **New Account**, **New Provider**

On **Overview → Codex**, each account card shows quota, notes, and action buttons explained in the next section.

---

## Account actions: Relay Latest, Handoff, Sync Sessions, Rename

These four buttons solve **different** problems. Pick the one that matches what you want to do.

### Relay Latest — “Continue my **most recent** session on this account”

**What it does**

1. Finds the **single most recently modified session across all your Codex profiles** (“latest active session”).
2. If that session file is not already on the **target account** you clicked, LAM **copies that one session file** into the target’s `sessions/` folder (never `auth.json`).
3. Opens **Terminal** with `CODEX_HOME=<target> codex resume <session-id>` so you can keep working there.

**When to use**

- You were just coding on account A and want to **continue the same thread on account B** (e.g. B still has quota).
- You do not need to pick a session manually — you want “whatever I was doing last.”

**When not to use**

- You need a **specific older session**, not the latest one → use **Handoff**.
- You want to copy **many sessions at once** → use **Sync Sessions**.
- The button is disabled if LAM cannot find any session yet.

---

### Handoff — “Continue a **chosen** session on another account”

**What it does**

1. Opens a dialog: **source account**, **session** (list from that account), **target account**.
2. Copies **only that session’s transcript file** to the target if needed (same safe rules as Relay Latest).
3. Opens **Terminal** with `codex resume` on the target account.

**How to open it**

- On an account card: **Handoff** — target is preset to that card; you choose source + session.
- On **Sessions** tab: **Relay To…** on a row — that session is preset; you choose target (and can change source).

**When to use**

- You know **exactly which session** to move (not necessarily the latest).
- You want control before anything is copied.

**If both sides already edited the same session differently**, behavior follows **Settings → Diverged session strategy** (backup, fork, or stop and ask). LAM will not silently overwrite without a rule.

---

### Sync Sessions — “Copy **all** session files from A to B (bulk)”

**What it does**

1. Opens the **Sync** dialog with source and target profiles.
2. You run **Dry Run** first — LAM lists every file under `sessions/` that would be copied, skipped, or blocked.
3. After you confirm, it copies the **entire `sessions/` tree** from source to target (file by file).
4. Backs up the target’s existing `sessions/` folder before overwriting.
5. Writes a manifest of what changed.

**What it never copies**

- `auth.json`, `config.toml`, sqlite, `cache/`, `tmp/`, `logs/`, etc.

**When to use**

- You are setting up a **relay workspace** or second account and want **many or all** conversations available there before resuming.
- You are doing a **one-time migration** of session assets between homes.

**When not to use**

- You only need **one** conversation and want Terminal opened immediately → use **Relay Latest** or **Handoff**.
- Sync does **not** run `codex resume` for you; after sync, use Sessions or Relay Latest to resume.

**Note:** Syncing into a **primary** profile (not a relay-style home) works but LAM will warn you — a dedicated relay directory is safer so you do not mix accounts’ runtime state.

---

### Rename — “Rename a managed account folder and wrapper”

**What it does**

1. Renames `~/.codex-{oldName}` → `~/.codex-{newName}` (moves the whole directory).
2. Creates a new wrapper `~/bin/codex-{newName}` and removes the old wrapper script.
3. **`auth.json` moves with the folder** — it is not deleted or copied separately.

**When to use**

- You created a profile as `~/.codex-b` and want a clearer name like `~/.codex-work`.
- You want LAM and your shell wrappers to stay in sync with a new account id.

**Limits**

- **`main` (`~/.codex`) cannot be renamed.**
- Target name must not already exist.
- **Dry run** first; close any Codex process using that profile before renaming.

Rename does **not** move sessions to another account — it only changes the **same** account’s directory name.

---

### Quick comparison

| Button | Copies | What gets copied | Opens Terminal? | You choose session? |
|--------|--------|------------------|-----------------|---------------------|
| **Relay Latest** | One file | Latest session across all profiles | Yes (`codex resume`) | No (automatic) |
| **Handoff** | One file | Session you pick | Yes (`codex resume`) | Yes |
| **Sync Sessions** | Many files | Whole `sessions/` tree | No | N/A (all sessions) |
| **Rename** | — | Renames one profile’s home dir | No | N/A |

---

## Other common tasks

**New Account** — Create `~/.codex-{name}` + `~/bin/codex-{name}` (dry-run → create). Optional: copy `config.toml` from an existing profile.

**Login** — Open Terminal with `codex login` for that profile’s `CODEX_HOME`.

**Edit note** — Store a renewal date and short memo in LAM’s local metadata (not inside Codex auth).

**Sessions tab** — Filter by account; **Copy** / **Terminal** / **Details** / **Relay To…**

**Providers** — Register proxy or custom API endpoints; attach to a profile; API keys stay in env or Keychain.

---

## How LAM is built (short)

```text
React UI  →  Tauri (macOS)  →  Rust core  →  ~/.codex*  +  Terminal
```

- **UI:** `apps/desktop/src` — tray popover + main window.
- **Core:** `apps/desktop/src-tauri` — filesystem scan, sync, quota via Codex app-server, command generation.
- **LAM metadata:** `~/.config/agent-workspace/` — account cache, notes, provider registry.
- **Your Codex data:** stays in `~/.codex*`; LAM does not sync it to the cloud.

More detail: [`docs/DESKTOP-RUNTIME.md`](docs/DESKTOP-RUNTIME.md)

---

## Safety rules

- Sync and handoff **never copy `auth.json`**.
- Destructive actions use **dry-run → confirm → execute** where applicable.
- Provider secrets are **not** shown in the UI or written into `config.toml` as plaintext.

---

## Develop & test

| Command | Purpose |
|---------|---------|
| `make start` | Run app in dev mode |
| `make check` | Build, UI smoke, Rust tests |
| `make dmg` | Build installable DMG |
| `make clean` | Remove `target/` and `dist/` (frees disk space; `target/` can grow to several GB during dev) |

---

## License

[MIT License](LICENSE) — use, modify, and distribute freely. Please include the copyright notice when redistributing.
