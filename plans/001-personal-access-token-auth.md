# Plan 001: Personal Access Token Authentication Mode (REVISED)

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving to the next step. If anything in the "STOP conditions" section occurs, stop and report — do not improvise. When done, update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 6e4471e..HEAD -- apps/desktop/src-tauri/src/services/account.rs apps/desktop/src-tauri/src/services/types.rs apps/desktop/src-tauri/src/commands/mod.rs apps/desktop/src-tauri/src/main.rs`
>
> If any in-scope file changed since this plan was written, compare the "Current state" excerpts against the live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L (3-5 days)
- **Risk**: MED (auth detection logic, requires backward compatibility)
- **Depends on**: none
- **Category**: feature
- **Planned at**: commit `6e4471e`, 2026-06-24

## Why this matters

Users need to switch between multiple Codex accounts quickly using personal access tokens (PAT) instead of OAuth. Current workflow requires manual `codex login` for each account. PAT mode allows:

1. Uploading credentials from external account management systems
2. Quick account switching by copying `auth.json` to `~/.codex`
3. Token expiration tracking with warnings (30/7-day thresholds)
4. Displaying authentication status in the UI

**Critical constraint**: This feature is **Lam-only** — it tracks and displays PAT auth status but **does not modify Codex's runtime behavior**. Codex continues to use whatever `auth.json` exists in its `CODEX_HOME`. Lam stores PAT metadata in its own config directory (`~/.config/agent-workspace/auth-metadata/`) to avoid interfering with Codex operation.

## Current State

### Persistence Directory

From `apps/desktop/src-tauri/src/services/types.rs:49-50`:
```rust
pub(crate) fn config_root(home_root: &Path) -> PathBuf {
    home_root.join(".config/agent-workspace")
}
```

**Already in use for**:
- `accounts-cache.json`
- `account-notes.json`
- `providers.json`
- `quota-cache/`

We'll add: **`auth-metadata/`** subdirectory for PAT tracking.

### Auth Mode Detection

From `apps/desktop/src-tauri/src/services/types.rs:108-133`:
```rust
#[derive(Debug, Default)]
pub(crate) struct CodexConfigBinding {
    pub provider_id: Option<String>,
    pub model: Option<String>,
    pub auth_mode: Option<String>,  // Currently: "config" if provider exists, else None
}

pub(crate) fn parse_codex_config(path: &Path) -> Result<CodexConfigBinding> {
    // ... parses config.toml
    let auth_mode = if provider_id.is_some() {
        Some("config".into())
    } else {
        None
    };
    // ...
}
```

From `apps/desktop/src-tauri/src/services/account.rs:123`:
```rust
let config = parse_codex_config(&home.join("config.toml"))?;
```

From `apps/desktop/src-tauri/src/services/account.rs:148-151` (in CodexAccount construction):
```rust
            provider_id: config.provider_id,
            model: config.model,
            auth_mode: config.auth_mode,  // ← This line will be enhanced
            renewal_date: note.and_then(|metadata| metadata.renewal_date.clone()),
```

**Current behavior**: `auth_mode` only detects if `config.toml` has a provider; doesn't inspect `auth.json` structure.

### Repo Conventions

- **Error handling**: Uses `Result<T, AppError>` pattern throughout services layer
- **Serde naming**: `#[serde(rename_all = "camelCase")]` for frontend types
- **File permissions**: Security-sensitive files get 0600 on macOS/Linux
- **Testing**: Unit tests in `#[cfg(test)] mod tests` at end of file

## Commands You Will Need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Type check | `cd apps/desktop/src-tauri && cargo check` | exit 0, no errors |
| Unit tests | `cd apps/desktop/src-tauri && cargo test --lib account::pat_tests` | 6 tests pass |
| Integration test | `cd apps/desktop/src-tauri && cargo test --test integration_pat_auth` | test passes |
| Frontend build | `cd apps/desktop && npm run build` | exit 0 |
| Start app (test) | `LAM_HOME="$(pwd)/.fake-home" make start` | app starts, no crashes |

## Scope

**In scope** (files you WILL modify):
- `apps/desktop/src-tauri/src/services/account.rs` — add PAT data structures, metadata storage, expiration checking
- `apps/desktop/src-tauri/src/services/types.rs` — add auth metadata storage functions
- `apps/desktop/src-tauri/src/commands/mod.rs` — add Tauri commands for frontend
- `apps/desktop/src-tauri/src/main.rs` — register new commands
- `apps/desktop/src-tauri/Cargo.toml` — add `tempfile` dev dependency
- `apps/desktop/src-tauri/tests/integration_pat_auth.rs` — NEW integration test
- `docs/FINAL-DESIGN.md` — add PAT auth section after line 193
- `README.md` — add PAT feature to current capabilities list

**Out of scope** (DO NOT touch):
- Any `~/.codex*/auth.json` or `config.toml` files — Codex owns these, Lam only reads them
- `apps/desktop/src-tauri/src/services/sync.rs` — sync safety rules unchanged
- OAuth flow code — must coexist
- Frontend React components — deferred to follow-up plan

## Git Workflow

- Branch: `advisor/001-personal-access-token-auth`
- Commit style: Conventional commits (match repo pattern)
- Do NOT push or open PR unless explicitly instructed

## Steps

### Step 1: Add PAT Metadata Structures

**File**: `apps/desktop/src-tauri/src/services/account.rs`

Add these structs **after the `AccountNoteUpdate` struct** (currently around line 95):

```rust
/// User-uploaded credentials from external account management
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UploadedCredentials {
    pub access_token: String,
    pub account_id: String,
    pub disabled: bool,
    pub email: String,
    pub expired: String,  // ISO 8601 format
    #[serde(default)]
    pub headers: Option<serde_json::Map<String, serde_json::Value>>,
    pub id_token: Option<String>,
    pub last_refresh: String,
    pub refresh_token: Option<String>,
    #[serde(rename = "type")]
    pub credential_type: String,
    pub websockets: bool,
}

/// Lam-tracked PAT metadata (stored in ~/.config/agent-workspace/auth-metadata/)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthMetadata {
    pub profile_id: String,
    pub auth_type: String,  // "personal_token" | "oauth" | "api_key"
    pub token_expiration: Option<String>,  // ISO 8601
    pub last_checked: String,  // ISO 8601
}

/// Token expiration status for UI display
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TokenExpirationStatus {
    pub profile_id: String,
    pub is_expired: bool,
    pub days_until_expiration: Option<i64>,
    pub expiration_date: Option<String>,
    pub warning_level: String,  // "ok" | "warning" | "critical" | "expired"
}
```

**Verify**:
```bash
cd apps/desktop/src-tauri
cargo check
```
Expected: exit 0, no compilation errors

---

### Step 2: Add Auth Metadata Storage Functions

**File**: `apps/desktop/src-tauri/src/services/types.rs`

Add these functions **after the `config_root` function** (currently around line 51):

```rust
pub(crate) fn auth_metadata_dir(home_root: &Path) -> PathBuf {
    config_root(home_root).join("auth-metadata")
}

pub(crate) fn auth_metadata_path(home_root: &Path, profile_id: &str) -> PathBuf {
    auth_metadata_dir(home_root).join(format!("{}.json", profile_id))
}
```

**Verify**:
```bash
cd apps/desktop/src-tauri
cargo check
```
Expected: exit 0

---

### Step 3: Add Credential Transformation and Metadata Functions

**File**: `apps/desktop/src-tauri/src/services/account.rs`

Add these functions **after the `normalize_note` function** (currently near the end of the file, around line 674):

```rust
/// Records PAT metadata for a profile (Lam-only, doesn't touch Codex files)
pub fn record_pat_metadata(
    home_root: &Path,
    profile_id: &str,
    expiration: Option<String>,
) -> Result<()> {
    use crate::services::types::{auth_metadata_dir, auth_metadata_path};
    
    let metadata = AuthMetadata {
        profile_id: profile_id.to_string(),
        auth_type: "personal_token".to_string(),
        token_expiration: expiration,
        last_checked: chrono::Utc::now().to_rfc3339(),
    };

    let dir = auth_metadata_dir(home_root);
    std::fs::create_dir_all(&dir).map_err(|e| {
        AppError::io("CREATE_DIR_FAILED", &format!("Failed to create auth-metadata dir: {}", e))
    })?;

    let path = auth_metadata_path(home_root, profile_id);
    let content = serde_json::to_string_pretty(&metadata).map_err(|e| {
        AppError::internal("SERIALIZE_FAILED", &format!("Serialize failed: {}", e))
    })?;

    std::fs::write(&path, content).map_err(|e| {
        AppError::io("WRITE_METADATA_FAILED", &format!("Write failed: {}", e))
    })?;

    Ok(())
}

/// Reads PAT metadata for a profile
pub fn read_pat_metadata(home_root: &Path, profile_id: &str) -> Result<Option<AuthMetadata>> {
    use crate::services::types::auth_metadata_path;
    
    let path = auth_metadata_path(home_root, profile_id);
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path).map_err(|e| {
        AppError::io("READ_METADATA_FAILED", &format!("Failed to read: {}", e))
    })?;

    let metadata: AuthMetadata = serde_json::from_str(&content).map_err(|e| {
        AppError::validation("INVALID_METADATA", &format!("Invalid metadata: {}", e))
    })?;

    Ok(Some(metadata))
}

/// Transforms uploaded credentials and records metadata
pub fn process_uploaded_credentials(
    home_root: &Path,
    profile_id: &str,
    creds: &UploadedCredentials,
) -> Result<()> {
    if creds.access_token.is_empty() {
        return Err(AppError::validation(
            "INVALID_CREDENTIALS",
            "access_token is required",
        ));
    }

    // Validate expiration date format
    if chrono::DateTime::parse_from_rfc3339(&creds.expired).is_err() {
        return Err(AppError::validation(
            "INVALID_EXPIRATION",
            "expired field must be valid ISO 8601 date",
        ));
    }

    // Record metadata in Lam's config
    record_pat_metadata(home_root, profile_id, Some(creds.expired.clone()))?;

    Ok(())
}

/// Checks token expiration from metadata
pub fn check_token_expiration(
    home_root: &Path,
    profile_id: &str,
) -> Result<TokenExpirationStatus> {
    let metadata = read_pat_metadata(home_root, profile_id)?;

    let (expiration_date, is_expired, days_until, warning_level) = match metadata {
        Some(meta) if meta.token_expiration.is_some() => {
            let exp_str = meta.token_expiration.unwrap();
            let expiry = chrono::DateTime::parse_from_rfc3339(&exp_str).map_err(|e| {
                AppError::validation("INVALID_EXPIRATION_FORMAT", &e.to_string())
            })?;

            let now = chrono::Utc::now();
            let days = (expiry.timestamp() - now.timestamp()) / 86400;

            let level = if days < 0 {
                "expired"
            } else if days <= 7 {
                "critical"
            } else if days <= 30 {
                "warning"
            } else {
                "ok"
            };

            (Some(exp_str), days < 0, Some(days), level.to_string())
        }
        _ => (None, false, None, "ok".to_string()),
    };

    Ok(TokenExpirationStatus {
        profile_id: profile_id.to_string(),
        is_expired,
        days_until_expiration: days_until,
        expiration_date,
        warning_level,
    })
}
```

**Verify**:
```bash
cd apps/desktop/src-tauri
cargo check
```
Expected: exit 0

---

### Step 4a: Add Auth Mode Detection Helper

**File**: `apps/desktop/src-tauri/src/services/account.rs`

Add this helper function **after the `normalize_note` function** (near the functions you just added in Step 3):

```rust
/// Detects auth mode by checking both Lam metadata and Codex auth.json
fn detect_auth_mode(
    home_root: &Path,
    profile_id: &str,
    codex_home: &Path,
    config: &CodexConfigBinding,
) -> Option<String> {
    // Priority 1: Check if Lam has recorded PAT metadata
    if let Ok(Some(metadata)) = read_pat_metadata(home_root, profile_id) {
        if metadata.auth_type == "personal_token" {
            return Some("personal_token".to_string());
        }
    }

    // Priority 2: Check Codex auth.json structure (read-only inspection)
    let auth_path = codex_home.join("auth.json");
    if auth_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&auth_path) {
            // Simple heuristic: check for personal_access_token field
            if content.contains("\"personal_access_token\"") {
                return Some("personal_token".to_string());
            }
            if content.contains("\"token\"") {
                return Some("oauth".to_string());
            }
            if content.contains("\"OPENAI_API_KEY\"") {
                return Some("api_key".to_string());
            }
        }
    }

    // Priority 3: Fall back to config.toml detection
    config.auth_mode.clone()
}
```

**Verify**:
```bash
cd apps/desktop/src-tauri
cargo check
```
Expected: exit 0

---

### Step 4b: Update Auth Mode Detection in list_accounts

**File**: `apps/desktop/src-tauri/src/services/account.rs`

Locate the `list_accounts` function where the `CodexAccount` struct is being constructed (currently around line 147-151). You'll see:

```rust
            provider_id: config.provider_id,
            model: config.model,
            auth_mode: config.auth_mode,
            renewal_date: note.and_then(|metadata| metadata.renewal_date.clone()),
```

**Replace ONLY the `auth_mode: config.auth_mode,` line** with this enhanced detection:

```rust
            auth_mode: detect_auth_mode(home_root, &id, &home, &config),
```

**Verify**:
```bash
cd apps/desktop/src-tauri
cargo check
```
Expected: exit 0

**Important note**: This detection is **read-only inspection** of Codex files. Lam never modifies Codex's `auth.json` or `config.toml` — it only reads them to display status in the UI.

---

### Step 5: Add Tauri Commands

**File**: `apps/desktop/src-tauri/src/commands/mod.rs`

**Important**: These functions are already exported from the crate root via `pub use services::*;` in `lib.rs`.

Add to the existing import list **at the top of the file with the other `use localagentmanager_core::{...}` imports**:

```rust
use localagentmanager_core::{
    // ... keep all existing imports ...
    process_uploaded_credentials, check_token_expiration, read_pat_metadata,
    UploadedCredentials, AuthMetadata, TokenExpirationStatus,
};
```

Add these command functions **after the existing commands** (currently around line 153):

```rust
#[tauri::command]
pub fn upload_pat_credentials(
    profile_id: String,
    uploaded: UploadedCredentials,
) -> Result<(), AppError> {
    process_uploaded_credentials(&home_root()?, &profile_id, &uploaded)
}

#[tauri::command]
pub fn get_pat_metadata(profile_id: String) -> Result<Option<AuthMetadata>, AppError> {
    read_pat_metadata(&home_root()?, &profile_id)
}

#[tauri::command]
pub fn check_profile_token_expiration(
    profile_id: String,
) -> Result<TokenExpirationStatus, AppError> {
    check_token_expiration(&home_root()?, &profile_id)
}
```

**Note**: `home_root()` is already defined in this file and returns `Result<PathBuf, AppError>`.

**File**: `apps/desktop/src-tauri/src/main.rs`

Add to the `invoke_handler` list (around line 52):

```rust
commands::upload_pat_credentials,
commands::get_pat_metadata,
commands::check_profile_token_expiration,
```

**Verify**:
```bash
cd apps/desktop/src-tauri
cargo check
```
Expected: exit 0

---

### Step 5.5: Verify Public Exports

The new structs and functions must be accessible from the crate root for commands and integration tests.

**Verify structs are public**:
```bash
cd apps/desktop/src-tauri
grep -E "^pub struct (UploadedCredentials|AuthMetadata|TokenExpirationStatus)" src/services/account.rs | wc -l
```
Expected: 3

**Verify functions are public**:
```bash
grep -E "^pub fn (process_uploaded_credentials|check_token_expiration|read_pat_metadata|record_pat_metadata)" src/services/account.rs | wc -l
```
Expected: 4

**Verify re-export exists**:
```bash
grep "pub use account" src/services/mod.rs
```
Expected: Should show re-export pattern (either `pub use account::*;` or explicit list)

**Note**: The `services/mod.rs` should already have the re-export pattern. If not present, this is a STOP condition.

---

### Step 6: Add Unit Tests

**File**: `apps/desktop/src-tauri/Cargo.toml`

Add to `[dev-dependencies]` section:

```toml
tempfile = "3"
```

**File**: `apps/desktop/src-tauri/src/services/account.rs`

Add at the very end of the file:

```rust
#[cfg(test)]
mod pat_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_record_and_read_metadata() {
        let temp = TempDir::new().unwrap();
        let home_root = temp.path();

        record_pat_metadata(home_root, "test-profile", Some("2030-12-31T10:00:00+08:00".to_string())).unwrap();

        let metadata = read_pat_metadata(home_root, "test-profile").unwrap().unwrap();
        assert_eq!(metadata.profile_id, "test-profile");
        assert_eq!(metadata.auth_type, "personal_token");
        assert_eq!(metadata.token_expiration, Some("2030-12-31T10:00:00+08:00".to_string()));
    }

    #[test]
    fn test_process_valid_credentials() {
        let temp = TempDir::new().unwrap();
        let creds = UploadedCredentials {
            access_token: "at-test".to_string(),
            account_id: "id".to_string(),
            disabled: false,
            email: "test@example.com".to_string(),
            expired: "2030-12-31T10:00:00+08:00".to_string(),
            headers: None,
            id_token: None,
            last_refresh: "2026-06-23T22:19:32+08:00".to_string(),
            refresh_token: None,
            credential_type: "codex".to_string(),
            websockets: true,
        };

        process_uploaded_credentials(temp.path(), "test", &creds).unwrap();

        let metadata = read_pat_metadata(temp.path(), "test").unwrap().unwrap();
        assert_eq!(metadata.auth_type, "personal_token");
    }

    #[test]
    fn test_process_invalid_expiration() {
        let temp = TempDir::new().unwrap();
        let creds = UploadedCredentials {
            access_token: "at-test".to_string(),
            account_id: "id".to_string(),
            disabled: false,
            email: "test@example.com".to_string(),
            expired: "not-a-date".to_string(),
            headers: None,
            id_token: None,
            last_refresh: "2026-06-23T22:19:32+08:00".to_string(),
            refresh_token: None,
            credential_type: "codex".to_string(),
            websockets: true,
        };

        assert!(process_uploaded_credentials(temp.path(), "test", &creds).is_err());
    }

    #[test]
    fn test_expiration_not_expired() {
        let temp = TempDir::new().unwrap();

        record_pat_metadata(temp.path(), "test", Some("2030-12-31T10:00:00+08:00".to_string())).unwrap();

        let status = check_token_expiration(temp.path(), "test").unwrap();
        assert!(!status.is_expired);
        assert_eq!(status.warning_level, "ok");
        assert!(status.days_until_expiration.unwrap() > 0);
    }

    #[test]
    fn test_expiration_expired() {
        let temp = TempDir::new().unwrap();

        record_pat_metadata(temp.path(), "test", Some("2020-01-01T10:00:00+08:00".to_string())).unwrap();

        let status = check_token_expiration(temp.path(), "test").unwrap();
        assert!(status.is_expired);
        assert_eq!(status.warning_level, "expired");
        assert!(status.days_until_expiration.unwrap() < 0);
    }

    #[test]
    fn test_detect_auth_mode_priority() {
        let temp = TempDir::new().unwrap();
        let home_root = temp.path();
        let codex_home = temp.path().join("codex-a");
        std::fs::create_dir_all(&codex_home).unwrap();

        // Create PAT metadata (priority 1 - should override config)
        record_pat_metadata(home_root, "a", Some("2030-12-31T10:00:00+08:00".to_string())).unwrap();

        let config = CodexConfigBinding {
            provider_id: Some("test".to_string()),
            model: None,
            auth_mode: Some("config".to_string()),
        };

        let detected = detect_auth_mode(home_root, "a", &codex_home, &config);
        assert_eq!(detected, Some("personal_token".to_string()));
    }
}
```

**Verify**:
```bash
cd apps/desktop/src-tauri
cargo test --lib account::pat_tests
```
Expected: 6 tests pass (includes new auth mode detection test)

---

### Step 7: Create Integration Test

**File**: `apps/desktop/src-tauri/tests/integration_pat_auth.rs` (NEW)

```rust
use localagentmanager_core::{
    UploadedCredentials, process_uploaded_credentials,
    read_pat_metadata, check_token_expiration,
};
use tempfile::TempDir;

#[test]
fn test_pat_auth_end_to_end() {
    let temp = TempDir::new().unwrap();
    let home_root = temp.path();

    let creds = UploadedCredentials {
        access_token: "at-integration".to_string(),
        account_id: "id".to_string(),
        disabled: false,
        email: "test@example.com".to_string(),
        expired: "2030-12-31T23:59:59+00:00".to_string(),
        headers: None,
        id_token: None,
        last_refresh: "2026-06-24T00:00:00+00:00".to_string(),
        refresh_token: None,
        credential_type: "codex".to_string(),
        websockets: true,
    };

    process_uploaded_credentials(home_root, "test-profile", &creds).unwrap();

    let metadata = read_pat_metadata(home_root, "test-profile").unwrap().unwrap();
    assert_eq!(metadata.auth_type, "personal_token");

    let status = check_token_expiration(home_root, "test-profile").unwrap();
    assert!(!status.is_expired);
    assert_eq!(status.warning_level, "ok");
}
```

**Verify**:
```bash
cd apps/desktop/src-tauri
cargo test --test integration_pat_auth
```
Expected: test passes

---

### Step 8: Update Documentation

**File**: `docs/FINAL-DESIGN.md`

Find line 193 (in UsageQuotaSnapshot section) and add after it:

```markdown

#### Personal Access Token Authentication Tracking

**Added:** 2026-06-24

LocalAgentManager tracks authentication modes for display purposes only:

| Mode | Detection Method | Lam Storage |
|------|------------------|-------------|
| OAuth (traditional) | Codex `auth.json` contains `"token"` field | None |
| Personal Access Token | Lam metadata file or `auth.json` contains `"personal_access_token"` | `~/.config/agent-workspace/auth-metadata/{profile_id}.json` |
| API Key | `auth.json` contains `"OPENAI_API_KEY"` | None |

**Lam-only feature**: This is a **display and tracking feature** for Lam's UI. Lam **never modifies** Codex's `auth.json` or `config.toml` files. It only:
- Records PAT metadata when user uploads credentials via Lam UI
- Reads Codex files to detect auth type for display
- Tracks token expiration for warnings

**Auth metadata structure** (`~/.config/agent-workspace/auth-metadata/{profile_id}.json`):
```json
{
  "profileId": "a",
  "authType": "personal_token",
  "tokenExpiration": "2030-12-31T23:59:59+00:00",
  "lastChecked": "2026-06-24T10:00:00+00:00"
}
```

**Expiration warnings**: ok (>30d) | warning (8-30d) | critical (1-7d) | expired (<0d)
```

**File**: `README.md`

Find "Current capabilities" section (around line 95) and add:

```markdown
- **Personal Access Token (PAT) tracking:** Track PAT expiration, display auth status (Lam UI only, doesn't modify Codex files).
```

**Verify**:
```bash
grep -n "Personal Access Token" docs/FINAL-DESIGN.md README.md | wc -l
```
Expected: 2 or more matches

---

## Test Plan

**Backend only** — frontend components deferred to follow-up plan.

**Unit tests** (6 tests in `pat_tests` module):
- Metadata record/read cycle
- Valid credential processing
- Invalid expiration handling
- Expiration status (not expired)
- Expiration status (expired)
- Auth mode detection priority (PAT metadata > auth.json > config.toml)

**Integration test**: End-to-end flow from upload → metadata storage → expiration check

**Manual verification**:
1. Start app: `LAM_HOME="$(pwd)/.fake-home" make start`
2. Use app UI or CLI to upload PAT credentials for a test account
3. Verify metadata file created at `~/.config/agent-workspace/auth-metadata/{profile_id}.json`
4. Verify auth_mode shows "personal_token" in account list
5. Verify existing OAuth accounts still work

## Done Criteria

Machine-checkable. ALL must hold:

- [ ] `cd apps/desktop/src-tauri && cargo check` exits 0
- [ ] `cargo test --lib account::pat_tests` exits 0, 6 tests pass
- [ ] `cargo test --test integration_pat_auth` exits 0
- [ ] `grep "Personal Access Token" docs/FINAL-DESIGN.md README.md` returns 2+ matches
- [ ] `git status` shows no modifications outside in-scope files
- [ ] `grep -c "auth_metadata_dir\|auth_metadata_path" apps/desktop/src-tauri/src/services/types.rs` returns 2

## STOP Conditions

Stop and report (do not improvise) if:

1. **Code at locations in "Current state" doesn't match excerpts (drift detected)**:
   - Run the drift check command first: `git diff --stat 6e4471e..HEAD -- apps/desktop/src-tauri/src/services/account.rs apps/desktop/src-tauri/src/services/types.rs apps/desktop/src-tauri/src/commands/mod.rs apps/desktop/src-tauri/src/main.rs`
   - If output shows changes in any listed file, compare Current State excerpts against live code
   - If function signatures changed, functions moved/renamed/removed, or struct fields differ, STOP and report
2. A verification command fails twice after reasonable fix attempt
3. Existing OAuth accounts break (test by running list_accounts and checking auth_mode for non-PAT accounts)
4. You accidentally modify any `~/.codex*/auth.json` or `config.toml` (out of scope — Lam only reads, never writes)
5. `cargo check` fails with unrelated errors (errors in files not listed in "In scope")
6. Step 5.5 export verification fails (missing `pub use account` in services/mod.rs)

**If you must stop mid-execution**:
1. Commit work in progress to feature branch with `WIP: [step name]` prefix
2. Document in commit message which step was in progress and what the blocker is
3. Update `plans/README.md` status to `BLOCKED` with reason
4. Do NOT leave uncommitted changes to backend files

## Maintenance Notes

**For future developers:**

1. **Lam-only feature** — This feature tracks and displays PAT status but **never modifies Codex files**. Lam's metadata storage is in `~/.config/agent-workspace/auth-metadata/`, completely separate from Codex's `CODEX_HOME`.

2. **Auth mode detection priority**:
   - First: Check Lam metadata (`auth-metadata/{profile_id}.json`)
   - Second: Inspect Codex `auth.json` structure (read-only)
   - Third: Fall back to `config.toml` parsing

3. **Separation of concerns**:
   - **Lam**: Tracks metadata, displays status, provides upload UI
   - **Codex**: Uses whatever `auth.json` exists in its `CODEX_HOME`
   - Never merge the two — Lam observes, doesn't control

4. **Expiration thresholds** (in `check_token_expiration`):
   - ok: >30 days | warning: 8-30 days | critical: 1-7 days | expired: <0 days

5. **Frontend TODO** (deferred):
   - React component for credential upload UI
   - Dashboard warnings for expiring tokens
   - Account settings integration
   
   Frontend should call:
   - `upload_pat_credentials(profile_id, credentials)` — records metadata
   - `get_pat_metadata(profile_id)` — retrieves metadata
   - `check_profile_token_expiration(profile_id)` — gets expiration status

6. **Testing additions**:
   - Always test with both PAT and OAuth accounts
   - Verify Codex files are never modified
   - Check metadata directory permissions

**For PR reviewers:**

- Confirm no Codex `auth.json` or `config.toml` files are modified
- Verify metadata stored in `~/.config/agent-workspace/auth-metadata/` only
- Test auth mode detection with mixed account types
- Check 6 unit tests pass + integration test passes
