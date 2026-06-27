# Plan 005: Split PAT runtime auth from quota refresh auth

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` unless a reviewer says they maintain the index.
>
> **Drift check (run first)**:
> `git diff --stat 9d9f9ae..HEAD -- apps/desktop/src-tauri/src/services/account.rs apps/desktop/src-tauri/src/services/quota.rs apps/desktop/src-tauri/tests/integration_pat_accounts.rs apps/desktop/src-tauri/tests/phase1_core.rs plans/README.md`
>
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding. A source
> behavior mismatch is a STOP condition. If only `plans/README.md` changed,
> reconcile its Plan 005 row without reverting other plan-index edits.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: none
- **Category**: direction
- **Planned at**: commit `9d9f9ae`, 2026-06-26
- **Execution status**: DONE on 2026-06-26. Initial isolated execution blocked
  at Step 5 because `cargo fmt -- --check` required formatting-only edits to
  `apps/desktop/src-tauri/src/services/types.rs` and
  `apps/desktop/src-tauri/src/commands/mod.rs`, outside the original source
  scope. Scope was explicitly expanded for those formatting-only changes, and
  all required gates now pass.

## Why this matters

PAT accounts need two credential representations with separate purposes.
Codex switching must use a minimal `auth.json` containing only the entered
`personal_access_token`, while quota refresh must authenticate with the
original uploaded Codex auth document. Keeping both documents prevents the PAT
runtime format from destroying the refresh credentials and prevents quota
refresh from reporting the active PAT slot instead of the selected account.

## Required behavior

Only when `AddPatAccountRequest.personal_access_token` remains non-empty after
trimming whitespace:

1. Serialize `req.auth_json` to `<account home>/auth-f.json` without adding,
   removing, or changing any JSON fields. “Unchanged” means semantic
   JSON-object equality after parsing; uploaded whitespace, formatting, and key
   order are not preserved because the frontend already sends a parsed map.
2. Save `<account home>/auth.json` with exactly:

   ```json
   {
     "OPENAI_API_KEY": null,
     "personal_access_token": "<trimmed token>"
   }
   ```

3. Keep `switch_to_pat_account` behavior unchanged: copy the account's
   `auth.json` to `~/.codex/auth.json`.
4. During realtime quota refresh, prefer `auth-f.json` when it exists. Present
   it to Codex app-server as `auth.json` inside a unique temporary
   `CODEX_HOME`; never overwrite or rename either persistent account file.
5. If `auth-f.json` does not exist, retain the current quota behavior and use
   the account directory directly.
6. If `auth-f.json` exists but is unreadable or invalid, fail that realtime
   refresh and use the existing cache/unavailable path. Do not silently fall
   back to `auth.json`, because that can return quota for the wrong credential.

When `personal_access_token` is absent, empty, or whitespace-only, preserve
current behavior exactly:

- Serialize the uploaded JSON to `auth.json` without changing any fields.
- Do not create `auth-f.json`.
- Keep quota refresh using the account directory's existing `auth.json`.
- Do not change Auth/OAuth account storage, login, switching, or quota refresh.

## Current state

Relevant files:

- `apps/desktop/src-tauri/src/services/account.rs` — creates PAT account files
  and performs auth switching.
- `apps/desktop/src-tauri/src/services/quota.rs` — launches Codex app-server
  with the account directory as `CODEX_HOME`.
- `apps/desktop/src-tauri/tests/integration_pat_accounts.rs` — covers PAT
  account creation and switching.
- `apps/desktop/src-tauri/tests/phase1_core.rs` — contains fake-Codex quota
  integration tests and environment serialization helpers.

Current PAT storage merges the token into the uploaded document and writes one
file:

```rust
// apps/desktop/src-tauri/src/services/account.rs:941-964
let mut auth_json = req.auth_json.clone();
if let Some(token) = req
    .personal_access_token
    .as_deref()
    .map(str::trim)
    .filter(|token| !token.is_empty())
{
    auth_json.insert("OPENAI_API_KEY".to_string(), serde_json::Value::Null);
    auth_json.insert(
        "personal_access_token".to_string(),
        serde_json::Value::String(token.to_string()),
    );
}
let auth_path = codex_dir.join("auth.json");
let auth_content = serde_json::to_string_pretty(&auth_json)?;
write_file_private(&auth_path, &auth_content)?;
```

Current switching already reads only `auth.json`, validates it as an object,
and atomically replaces the main auth slot:

```rust
// apps/desktop/src-tauri/src/services/account.rs:1105-1144
let source_auth = codex_dir.join("auth.json");
let source_content = fs::read(&source_auth)?;
let parsed: serde_json::Value = serde_json::from_slice(&source_content)?;
let target_auth = target_codex.join("auth.json");
let temp_auth = target_codex.join(format!(".auth.json.lam-{}.tmp", std::process::id()));
// write temp, sync, chmod 0600, rename to target
```

Do not alter that source selection or atomic replacement logic.

Current quota refresh passes the persistent account home directly:

```rust
// apps/desktop/src-tauri/src/services/quota.rs:239-243, 391-398
fn try_codex_app_server_quota(
    home_root: &Path,
    account: &CodexAccount,
) -> Result<UsageQuotaSnapshot> {
    let mut child = spawn_codex_app_server(home_root, account)?;
    // ...
}

command
    .env("PATH", path_env)
    .env("CODEX_HOME", &account.codex_home)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
```

Repository conventions to preserve:

- Use `AppError` and the local `Result<T>` alias for filesystem failures.
- Use `write_file_private`, `set_file_private`, and `set_dir_private` from
  `services/types.rs`; credential files must remain mode `0600` and temporary
  directories mode `0700` on Unix.
- Tests use `tempfile::TempDir`, fake executable shell scripts, and
  `env_lock()` to serialize process-wide environment mutation.
- Commit messages use Conventional Commits, for example
  `feat: restart Codex app after account switch`.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Focused PAT tests | `cd apps/desktop/src-tauri && cargo test --test integration_pat_accounts` | exit 0; all integration PAT tests pass |
| Focused quota tests | `cd apps/desktop/src-tauri && cargo test --test phase1_core quota -- --test-threads=1` | exit 0; all quota and quota-refresh tests pass |
| Rust format check | `cd apps/desktop/src-tauri && cargo fmt -- --check` | exit 0; no diff |
| Rust lint | `cd apps/desktop/src-tauri && cargo clippy --all-targets -- -D warnings` | exit 0; no warnings, including integration-test targets |
| Full project gate | `make check` | exit 0; frontend build/UI smoke/Rust checks/tests pass |

No new Rust dependency is required. `uuid` and the filesystem helpers needed
for unique temporary directories already exist in the project. Note that
`make check` may run the repository's existing `npm install` step when
`apps/desktop/node_modules` is absent.

## Scope

**In scope**:

- `apps/desktop/src-tauri/src/services/account.rs`
- `apps/desktop/src-tauri/src/services/quota.rs`
- `apps/desktop/src-tauri/tests/integration_pat_accounts.rs`
- `apps/desktop/src-tauri/tests/phase1_core.rs`
- `plans/README.md` — status update only

If re-executing this plan, first decide how to handle the known fmt baseline
blocker recorded in "Status". Do not let an executor silently edit
`apps/desktop/src-tauri/src/services/types.rs` or
`apps/desktop/src-tauri/src/commands/mod.rs` unless the maintainer explicitly
expands scope to allow formatting-only changes in those files.

**Out of scope**:

- Frontend types, forms, labels, and API calls.
- Changes to `AddPatAccountRequest` or Tauri command signatures.
- Changes to `switch_to_pat_account` beyond tests proving it still copies
  `auth.json`.
- Migration or reconstruction of `auth-f.json` for accounts created before
  this change; the original uploaded JSON cannot be recovered safely from a
  merged PAT file.
- Persistent quota staging directories, background cleanup jobs, feature
  flags, or new dependencies.
- Renaming `auth.json` in OAuth accounts or changing non-PAT account scanning.

## Git workflow

- Branch: `codex/005-split-pat-auth-and-quota-source`
- One logical commit is sufficient.
- Commit message: `feat: split PAT auth from quota credentials`
- Do not push or open a PR unless explicitly instructed.

## Steps

### Step 1: Characterize the dual-file PAT storage contract

Update `test_add_and_switch_pat_account` in
`apps/desktop/src-tauri/tests/integration_pat_accounts.rs` before changing
production code.

For a request with a non-empty PAT, assert:

- `.codex-test-pat-account/auth-f.json` exists.
- Parsed `auth-f.json` is semantically equal to the original uploaded
  `auth_json` object; it must not gain `OPENAI_API_KEY` or
  `personal_access_token`.
- Parsed `auth.json` equals exactly the two-field object from "Required
  behavior"; assert object length is `2`.
- After `switch_to_pat_account`, `~/.codex/auth.json` equals the account's
  generated minimal `auth.json`, not `auth-f.json`.
- Existing `config.toml` and session-preservation assertions remain.

Add one table-driven regression test that runs the same assertions for:

- `personal_access_token: None`
- `personal_access_token: Some("")`
- `personal_access_token: Some("   ")`

- `auth.json` equals the uploaded object.
- `auth-f.json` does not exist.

Do not duplicate coverage for metadata, duplicate account IDs, or UI behavior.

**Verify**:
`cd apps/desktop/src-tauri && cargo test --test integration_pat_accounts`
→ the new assertions fail against the current implementation for the expected
file-layout reason, while unrelated tests still compile.

### Step 2: Write the uploaded and runtime auth files separately

In `add_pat_account` in
`apps/desktop/src-tauri/src/services/account.rs`, replace only the current
token-merging/write block.

Implementation shape:

1. Normalize the optional token once with the existing
   `as_deref().map(str::trim).filter(...)` pattern.
2. If a trimmed token is present:
   - Serialize `req.auth_json` without changing its fields and write it privately to
     `codex_dir.join("auth-f.json")`.
   - Build a new `serde_json::Map` or `serde_json::json!` value containing only
     `OPENAI_API_KEY: null` and `personal_access_token: token`.
   - Serialize it and write it privately to `codex_dir.join("auth.json")`.
3. If no trimmed token is present:
   - Serialize `req.auth_json` without changing its fields and write only
     `auth.json`.
   - Do not create `auth-f.json`; quota therefore follows its existing direct
     account-home path.
4. Keep directory creation, expiration parsing, config creation, marker
   creation, metadata, and return values unchanged.

Use `serde_json::to_string_pretty`; do not hand-build JSON strings and do not
add a helper used only here.

**Verify**:
`cd apps/desktop/src-tauri && cargo test --test integration_pat_accounts`
→ all tests pass.

### Step 3: Stage `auth-f.json` in an isolated temporary quota home

In `apps/desktop/src-tauri/src/services/quota.rs`:

1. Add a small private RAII type local to this module that owns a temporary
   directory path and removes it in `Drop`.
2. Add one private preparation function called from
   `try_codex_app_server_quota`:
   - Determine presence with `auth_f_path.try_exists()`. Do not use
     `Path::exists()` for this decision because metadata/access errors must not
     be treated as absence.
   - On `Ok(false)`, return no staging guard and use
     `account.codex_home` unchanged.
   - On `Ok(true)`, read and parse it as a JSON object.
   - On `Err(err)`, return `AppError::new("QUOTA_AUTH_METADATA_FAILED", ...)`.
   - Return `QUOTA_AUTH_READ_FAILED` when reading fails and
     `QUOTA_AUTH_INVALID` when JSON is invalid or its root is not an object.
     None of these errors may fall back to the account's runtime `auth.json`.
   - Create a unique directory under `std::env::temp_dir()` using the account
     ID plus `uuid::Uuid::new_v4()`.
   - Set the directory private.
   - Write the original `auth-f.json` bytes/content to
     `<temporary home>/auth.json` using `write_file_private`.
   - Return a guard that directly owns the staging `PathBuf`; do not put an
     `Option<PathBuf>` inside the guard. The no-`auth-f.json` branch simply
     creates no guard.
3. Hold the guard in `try_codex_app_server_quota` for the entire child-process
   lifetime. After spawn, every stdin/stdout/stderr, protocol, child-exit, and
   timeout error path must kill the child when still running and call `wait`
   before the guard is dropped. Successful parsing must likewise kill and wait
   before returning.
4. Change `spawn_codex_app_server` to accept a `&Path` for `codex_home` instead
   of the full `CodexAccount`; keep `account.id` in the caller for parsing the
   returned snapshot.
5. Pass the selected path to `.env("CODEX_HOME", codex_home)`.

The staging directory must be unique per refresh so concurrent account
refreshes cannot overwrite each other. Cleanup is best-effort in `Drop`.
Persistent account files must never be renamed, copied over, or temporarily
swapped.

Do not copy `config.toml`, sessions, sqlite files, or caches into the temporary
home; quota auth needs only the selected credential document. If a verified
Codex app-server test demonstrates that config is mandatory, STOP rather than
expanding the copy set.

**Verify**:
`cd apps/desktop/src-tauri && cargo test --test phase1_core quota -- --test-threads=1`
→ existing quota tests pass.

### Step 4: Prove quota source priority and cleanup

Add focused tests in `apps/desktop/src-tauri/tests/phase1_core.rs`, following
the existing fake-Codex shell-script pattern.

Add three focused tests and strengthen one existing test:

1. `quota_prefers_auth_f_json_when_present`
   - Seed an account directory with different marker values in `auth.json` and
     `auth-f.json`.
   - The fake Codex script reads `$CODEX_HOME/auth.json` and exits non-zero
     unless it sees the marker from `auth-f.json`.
   - Return a valid rate-limit response and assert a fresh snapshot.
   - Record the received `CODEX_HOME` path in a harmless test output file.
   - Assert the received path differs from the persistent account directory.
   - After refresh returns, assert the temporary path no longer exists.
   - Assert both persistent auth files remain byte-for-byte unchanged by the
     refresh operation.

2. `quota_rejects_unusable_auth_f_without_runtime_fallback`
   - Cover two subcases in a small table: malformed/non-object JSON and a read
     failure represented by an `auth-f.json` path that is a directory.
   - Seed a valid runtime `auth.json` whose neutral marker the fake Codex would
     accept if incorrectly launched.
   - Assert refresh returns the existing cached/unavailable behavior and that
     the fake Codex launch marker/output file was never created.
   - This proves invalid, unreadable, or wrong-type `auth-f.json` does not
     silently fall back to runtime `auth.json`.

3. `quota_removes_staging_home_after_app_server_failure`
   - Seed valid, distinct `auth.json` and `auth-f.json`.
   - The fake Codex script records `CODEX_HOME` and exits non-zero.
   - After refresh returns through the existing cached/unavailable path, assert
     the recorded staging directory no longer exists.

Strengthen the existing
`quota_app_server_uses_each_profile_codex_home` test rather than adding a
duplicate fallback test:

- Keep its current main and non-main account assertions.
- Explicitly assert the seeded accounts have no `auth-f.json`.
- Treat it as the regression proof that accounts without `auth-f.json`,
  including Auth/OAuth accounts, continue to use their persistent
  `CODEX_HOME/auth.json`.

Do not add a 15-second timeout test. The guard must cover timeout by control
flow, but this plan's machine-checked cleanup claim is limited to success and
app-server failure paths.

Use `env_lock()` and restore/remove every environment variable set by each
test. Do not expose token-like fixture values in failure messages; use neutral
markers such as `"source": "uploaded"` and `"source": "runtime"`.

**Verify**:
`cd apps/desktop/src-tauri && cargo test --test phase1_core quota -- --test-threads=1`
→ all quota tests pass, including the three new cases and strengthened
per-profile fallback test.

### Step 5: Run the repository gates

Before editing, record `git status --short -uall` as the worktree baseline and
preserve every pre-existing user change. Run formatting first after
implementation. If it reports a diff, run `cargo fmt` only on this
implementation branch, then repeat the check.

**Verify in order**:

1. `cd apps/desktop/src-tauri && cargo fmt -- --check`
   → exit 0.
2. `cd apps/desktop/src-tauri && cargo clippy --all-targets -- -D warnings`
   → exit 0.
3. `make check`
   → exit 0.
4. `git status --short -uall`
   → compared with the recorded baseline, no new changed path appears outside
   the in-scope list; all pre-existing user changes remain untouched.

## Test plan

- Extend `test_add_and_switch_pat_account` for the exact dual-file contents and
  unchanged Switch source.
- Add one table-driven no-PAT storage regression covering `None`, empty, and
  whitespace-only values.
- Add one quota-priority test proving app-server receives `auth-f.json` as its
  staged `auth.json`.
- Add one unusable-`auth-f.json` test proving malformed, non-object, and read
  failures never fall back to runtime `auth.json`.
- Add one app-server-failure cleanup test.
- Strengthen the existing per-profile `CODEX_HOME` test to prove accounts
  without `auth-f.json`, including Auth/OAuth accounts, retain current
  behavior.
- Re-run all existing quota fallback, delayed-response, parsing, and cache
  tests through the `quota` filter.
- Finish with `make check`.

## Done criteria

- [ ] PAT upload with a non-empty trimmed token writes private `auth-f.json`
      containing the same JSON fields and values as the uploaded object.
- [ ] The same upload writes private `auth.json` containing exactly
      `OPENAI_API_KEY: null` and the trimmed `personal_access_token`.
- [ ] Upload with an absent, empty, or whitespace-only PAT retains the previous
      single-`auth.json` behavior.
- [ ] Switch still copies only the account's `auth.json`.
- [ ] Quota refresh uses a unique temporary `CODEX_HOME` backed by
      `auth-f.json` when present.
- [ ] Quota refresh uses the persistent account home when `auth-f.json` is
      absent.
- [ ] Invalid existing `auth-f.json` does not silently fall back to
      `auth.json`.
- [ ] Temporary quota homes are removed after success and app-server failure
      paths; timeout cleanup follows the same guard lifetime but is not
      separately delayed-tested.
- [ ] Persistent `auth.json` and `auth-f.json` remain unchanged by refresh.
- [ ] Auth/OAuth accounts without `auth-f.json` retain their existing storage,
      login, switching, and persistent-`CODEX_HOME` quota behavior.
- [ ] No new Rust dependency, frontend change, migration, or unrelated
      refactor is included.
- [ ] `make check` exits 0.
- [ ] `plans/README.md` marks Plan 005 DONE after implementation.

## STOP conditions

Stop and report instead of improvising if:

- The current request or account storage schema no longer matches the excerpts.
- Codex app-server demonstrably requires files beyond the staged `auth.json`;
  do not copy the full account directory without review.
- Supporting `auth-f.json` requires changing the frontend request shape or
  Tauri command signatures.
- The implementation would need to overwrite, rename, or swap persistent
  `auth.json` during quota refresh.
- Tests reveal concurrent refreshes share a staging path.
- A verification command fails twice after a reasonable scoped correction.
- Any file outside the in-scope list must be changed.

## Maintenance notes

- `auth.json` remains the runtime/Switch credential; `auth-f.json` is solely
  the preferred realtime quota credential.
- Reviewers should scrutinize file permissions, temporary-directory uniqueness
  and cleanup, and any accidental fallback from a broken `auth-f.json`.
- Existing accounts are intentionally not migrated. Re-upload is required to
  obtain an original `auth-f.json`.
- Auth/OAuth mode is outside this change. Split-file behavior is activated
  solely by a non-empty trimmed `personal_access_token`.
- If Codex later supports an explicit auth-file environment variable, replace
  temporary staging with that native mechanism and remove the staging code.
