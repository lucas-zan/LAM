# Plan 001: Personal Access Token Authentication Mode (v2)

> **For agentic workers:** REQUIRED: Use `executing-plans` (or `subagent-driven-development` if available) to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Drift check (run first)**: `git diff --stat 6e4471e..HEAD -- apps/desktop/src-tauri/src/services/account.rs apps/desktop/src-tauri/src/services/types.rs apps/desktop/src-tauri/src/commands/mod.rs apps/desktop/src-tauri/src/main.rs`
>
> If any in-scope file changed since this plan was written, compare the "Current state" excerpts against the live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L (3-5 days, ~40 steps)
- **Risk**: MED (auth detection logic, requires backward compatibility)
- **Depends on**: none
- **Category**: feature
- **Planned at**: commit `6e4471e`, 2026-06-24
- **Architecture**: TDD with bite-sized steps, test-first workflow

## Goal

Add personal access token (PAT) authentication tracking to Lam's UI, allowing users to upload credentials from external systems, view auth status, and receive token expiration warnings — without modifying Codex's runtime behavior.

## Architecture

**Lam-only feature** — tracks and displays PAT auth status but **never modifies Codex files**. Three layers:

1. **Metadata storage**: Lam stores PAT tracking data in `~/.config/agent-workspace/auth-metadata/{profile_id}.json`
2. **Detection**: Reads Codex's `auth.json` (read-only) to detect auth type for display
3. **Expiration tracking**: Warns users at 30/7-day thresholds

**Tech Stack**: Rust (Tauri backend), serde for JSON, chrono for date handling, TDD with cargo test

---

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

### Key File Locations

**`apps/desktop/src-tauri/src/services/account.rs`**:
- Line 95: End of `AccountNoteUpdate` struct (insert PAT structs after this)
- Line 674: End of `normalize_note` function (insert PAT functions after this)
- Line 675: End of file (insert test module here)

**`apps/desktop/src-tauri/src/services/types.rs`**:
- Line 51: After `config_root` function (insert storage helpers)
- Line 108-133: `CodexConfigBinding` and `parse_codex_config` (reference for auth mode)

**`apps/desktop/src-tauri/src/services/account.rs:148-151`** (CodexAccount construction):
```rust
            provider_id: config.provider_id,
            model: config.model,
            auth_mode: config.auth_mode,  // ← Will call enhanced detection
            renewal_date: note.and_then(|metadata| metadata.renewal_date.clone()),
```

### Repo Conventions

- **Error handling**: `Result<T, AppError>` pattern throughout
- **Serde naming**: `#[serde(rename_all = "camelCase")]` for frontend types
- **File permissions**: Security-sensitive files get 0600 on macOS/Linux
- **Testing**: Unit tests in `#[cfg(test)] mod tests` at end of file
- **TDD**: Write test → verify fail → implement → verify pass → commit

---

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

---

## Git Workflow

- Branch: `advisor/001-personal-access-token-auth`
- Commit style: Conventional commits (match repo pattern)
- Commit after each logical unit (every 3-5 steps)
- Do NOT push or open PR unless explicitly instructed

---

## Implementation Steps

### Phase 1: Setup & Dependencies

#### Task 1: Create Feature Branch

- [ ] **Step 1.1: Create and switch to feature branch**

```bash
git checkout -b advisor/001-personal-access-token-auth
```

Expected: `Switched to a new branch 'advisor/001-personal-access-token-auth'`

- [ ] **Step 1.2: Verify clean state**

```bash
git status
```

Expected: `On branch advisor/001-personal-access-token-auth` with no uncommitted changes

---

#### Task 2: Add Dev Dependencies

- [ ] **Step 2.1: Add tempfile dependency**

**File**: `apps/desktop/src-tauri/Cargo.toml`

Locate the `[dev-dependencies]` section and add:

```toml
tempfile = "3"
```

- [ ] **Step 2.2: Verify dependency resolution**

```bash
cd apps/desktop/src-tauri && cargo check
```

Expected: exit 0, downloads tempfile if needed

- [ ] **Step 2.3: Commit dependency change**

```bash
git add apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/Cargo.lock
git commit -m "build(deps): add tempfile dev dependency for PAT tests"
```

Expected: 2 files committed

---

### Phase 2: Data Structures (TDD)

#### Task 3: Add PAT Metadata Structures

- [ ] **Step 3.1: Add struct declarations**

**File**: `apps/desktop/src-tauri/src/services/account.rs`

**Location**: After line 95 (after `AccountNoteUpdate` struct closing brace)

Add:

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

- [ ] **Step 3.2: Verify compilation**

```bash
cd apps/desktop/src-tauri && cargo check
```

Expected: exit 0, may show unused struct warnings (OK at this stage)

- [ ] **Step 3.3: Commit structures**

```bash
git add apps/desktop/src-tauri/src/services/account.rs
git commit -m "feat(pat): add PAT metadata structures"
```

---

#### Task 4: Add Storage Path Helpers

- [ ] **Step 4.1: Add auth_metadata_dir function**

**File**: `apps/desktop/src-tauri/src/services/types.rs`

**Location**: After line 51 (after `config_root` function closing brace)

Add:

```rust

pub(crate) fn auth_metadata_dir(home_root: &Path) -> PathBuf {
    config_root(home_root).join("auth-metadata")
}

pub(crate) fn auth_metadata_path(home_root: &Path, profile_id: &str) -> PathBuf {
    auth_metadata_dir(home_root).join(format!("{}.json", profile_id))
}
```

- [ ] **Step 4.2: Verify compilation**

```bash
cd apps/desktop/src-tauri && cargo check
```

Expected: exit 0

- [ ] **Step 4.3: Commit storage helpers**

```bash
git add apps/desktop/src-tauri/src/services/types.rs
git commit -m "feat(pat): add auth metadata storage path helpers"
```

---

### Phase 3: Metadata Operations (TDD)

#### Task 5: Test & Implement record_pat_metadata

- [ ] **Step 5.1: Create test module skeleton**

**File**: `apps/desktop/src-tauri/src/services/account.rs`

**Location**: At end of file (after line 674)

Add:

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
}
```

- [ ] **Step 5.2: Run test to verify it fails**

```bash
cd apps/desktop/src-tauri && cargo test --lib account::pat_tests::test_record_and_read_metadata
```

Expected: FAIL with "cannot find function `record_pat_metadata`" or similar

- [ ] **Step 5.3: Implement record_pat_metadata**

**File**: `apps/desktop/src-tauri/src/services/account.rs`

**Location**: After line 674 (after `normalize_note` function, before test module)

Add:

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
```

- [ ] **Step 5.4: Implement read_pat_metadata**

**File**: `apps/desktop/src-tauri/src/services/account.rs`

**Location**: Immediately after `record_pat_metadata` function

Add:

```rust

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
```

- [ ] **Step 5.5: Run test to verify it passes**

```bash
cd apps/desktop/src-tauri && cargo test --lib account::pat_tests::test_record_and_read_metadata
```

Expected: PASS (1 test)

- [ ] **Step 5.6: Commit metadata functions**

```bash
git add apps/desktop/src-tauri/src/services/account.rs
git commit -m "feat(pat): implement record/read PAT metadata with test"
```

---
#### Task 6: Test & Implement process_uploaded_credentials

- [ ] **Step 6.1: Add test for valid credentials**

**File**: `apps/desktop/src-tauri/src/services/account.rs`

**Location**: Inside `pat_tests` module, after existing test

Add:

```rust

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
```

- [ ] **Step 6.2: Run tests to verify they fail**

```bash
cd apps/desktop/src-tauri && cargo test --lib account::pat_tests::test_process_valid_credentials
cd apps/desktop/src-tauri && cargo test --lib account::pat_tests::test_process_invalid_expiration
```

Expected: Both FAIL with "cannot find function `process_uploaded_credentials`"

- [ ] **Step 6.3: Implement process_uploaded_credentials**

**File**: `apps/desktop/src-tauri/src/services/account.rs`

**Location**: After `read_pat_metadata` function, before test module

Add:

```rust

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
```

- [ ] **Step 6.4: Run tests to verify they pass**

```bash
cd apps/desktop/src-tauri && cargo test --lib account::pat_tests
```

Expected: 3 tests PASS

- [ ] **Step 6.5: Commit credential processing**

```bash
git add apps/desktop/src-tauri/src/services/account.rs
git commit -m "feat(pat): implement credential processing with validation tests"
```

---

#### Task 7: Test & Implement Token Expiration Checking

- [ ] **Step 7.1: Add expiration check tests**

**File**: `apps/desktop/src-tauri/src/services/account.rs`

**Location**: Inside `pat_tests` module, after existing tests

Add:

```rust

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
```

- [ ] **Step 7.2: Run tests to verify they fail**

```bash
cd apps/desktop/src-tauri && cargo test --lib account::pat_tests::test_expiration
```

Expected: Both FAIL with "cannot find function `check_token_expiration`"

- [ ] **Step 7.3: Implement check_token_expiration**

**File**: `apps/desktop/src-tauri/src/services/account.rs`

**Location**: After `process_uploaded_credentials` function, before test module

Add:

```rust

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

- [ ] **Step 7.4: Run tests to verify they pass**

```bash
cd apps/desktop/src-tauri && cargo test --lib account::pat_tests
```

Expected: 5 tests PASS

- [ ] **Step 7.5: Commit expiration checking**

```bash
git add apps/desktop/src-tauri/src/services/account.rs
git commit -m "feat(pat): implement token expiration checking with tests"
```

---

### Phase 4: Auth Mode Detection

#### Task 8: Test & Implement Enhanced Auth Detection

- [ ] **Step 8.1: Add auth detection test**

**File**: `apps/desktop/src-tauri/src/services/account.rs`

**Location**: Inside `pat_tests` module, after existing tests

Add:

```rust

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
```

- [ ] **Step 8.2: Run test to verify it fails**

```bash
cd apps/desktop/src-tauri && cargo test --lib account::pat_tests::test_detect_auth_mode_priority
```

Expected: FAIL with "cannot find function `detect_auth_mode`"

- [ ] **Step 8.3: Implement detect_auth_mode**

**File**: `apps/desktop/src-tauri/src/services/account.rs`

**Location**: After `check_token_expiration` function, before test module

Add:

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

- [ ] **Step 8.4: Run test to verify it passes**

```bash
cd apps/desktop/src-tauri && cargo test --lib account::pat_tests::test_detect_auth_mode_priority
```

Expected: PASS (6 tests total now)

- [ ] **Step 8.5: Update list_accounts to use enhanced detection**

**File**: `apps/desktop/src-tauri/src/services/account.rs`

**Location**: Find line ~148 where `auth_mode: config.auth_mode,` appears in CodexAccount construction

**Replace this line:**
```rust
            auth_mode: config.auth_mode,
```

**With:**
```rust
            auth_mode: detect_auth_mode(home_root, &id, &home, &config),
```

- [ ] **Step 8.6: Verify compilation**

```bash
cd apps/desktop/src-tauri && cargo check
```

Expected: exit 0

- [ ] **Step 8.7: Run all tests**

```bash
cd apps/desktop/src-tauri && cargo test --lib account::pat_tests
```

Expected: 6 tests PASS

- [ ] **Step 8.8: Commit auth detection**

```bash
git add apps/desktop/src-tauri/src/services/account.rs
git commit -m "feat(pat): implement auth mode detection with priority levels"
```

---

### Phase 5: Tauri Commands & Integration

#### Task 9: Add Tauri Command Wrappers

- [ ] **Step 9.1: Add imports to commands module**

**File**: `apps/desktop/src-tauri/src/commands/mod.rs`

**Location**: Find existing `use localagentmanager_core::{` import block

Add to the existing import list:

```rust
    process_uploaded_credentials, check_token_expiration, read_pat_metadata,
    UploadedCredentials, AuthMetadata, TokenExpirationStatus,
```

- [ ] **Step 9.2: Add command functions**

**File**: `apps/desktop/src-tauri/src/commands/mod.rs`

**Location**: After existing command functions (around end of file)

Add:

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

- [ ] **Step 9.3: Verify compilation**

```bash
cd apps/desktop/src-tauri && cargo check
```

Expected: exit 0

---

#### Task 10: Register Commands in Tauri

- [ ] **Step 10.1: Register commands in main.rs**

**File**: `apps/desktop/src-tauri/src/main.rs`

**Location**: Find the `.invoke_handler` call (around line 52)

Add these three commands to the list:

```rust
            commands::upload_pat_credentials,
            commands::get_pat_metadata,
            commands::check_profile_token_expiration,
```

- [ ] **Step 10.2: Verify compilation**

```bash
cd apps/desktop/src-tauri && cargo check
```

Expected: exit 0

- [ ] **Step 10.3: Commit command integration**

```bash
git add apps/desktop/src-tauri/src/commands/mod.rs apps/desktop/src-tauri/src/main.rs
git commit -m "feat(pat): add Tauri commands for PAT management"
```

---

### Phase 6: Integration Testing

#### Task 11: Create Integration Test

- [ ] **Step 11.1: Create integration test file**

**File**: `apps/desktop/src-tauri/tests/integration_pat_auth.rs` (NEW)

Create with content:

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

- [ ] **Step 11.2: Run integration test**

```bash
cd apps/desktop/src-tauri && cargo test --test integration_pat_auth
```

Expected: test passes

- [ ] **Step 11.3: Commit integration test**

```bash
git add apps/desktop/src-tauri/tests/integration_pat_auth.rs
git commit -m "test(pat): add end-to-end integration test"
```

---

### Phase 7: Documentation

#### Task 12: Update Design Documentation

- [ ] **Step 12.1: Add PAT section to FINAL-DESIGN.md**

**File**: `docs/FINAL-DESIGN.md`

**Location**: After line 193 (in UsageQuotaSnapshot section)

Add:

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

- [ ] **Step 12.2: Verify documentation added**

```bash
grep -n "Personal Access Token" docs/FINAL-DESIGN.md
```

Expected: Shows line number where section was added

---

#### Task 13: Update README

- [ ] **Step 13.1: Add PAT feature to README**

**File**: `README.md`

**Location**: Find "Current capabilities" section (around line 95)

Add:

```markdown
- **Personal Access Token (PAT) tracking:** Track PAT expiration, display auth status (Lam UI only, doesn't modify Codex files).
```

- [ ] **Step 13.2: Verify README updated**

```bash
grep "Personal Access Token" README.md
```

Expected: Shows the line you added

- [ ] **Step 13.3: Commit documentation**

```bash
git add docs/FINAL-DESIGN.md README.md
git commit -m "docs(pat): document PAT authentication tracking feature"
```

---

### Phase 8: Final Verification

#### Task 14: Run All Verification Checks

- [ ] **Step 14.1: Type check**

```bash
cd apps/desktop/src-tauri && cargo check
```

Expected: exit 0, no errors

- [ ] **Step 14.2: Run unit tests**

```bash
cd apps/desktop/src-tauri && cargo test --lib account::pat_tests
```

Expected: 6 tests PASS

- [ ] **Step 14.3: Run integration test**

```bash
cd apps/desktop/src-tauri && cargo test --test integration_pat_auth
```

Expected: 1 test PASS

- [ ] **Step 14.4: Verify documentation mentions**

```bash
grep -c "Personal Access Token" docs/FINAL-DESIGN.md README.md
```

Expected: At least 2 matches total

- [ ] **Step 14.5: Verify storage helpers exist**

```bash
grep -c "auth_metadata_dir\|auth_metadata_path" apps/desktop/src-tauri/src/services/types.rs
```

Expected: 2 (one for each function)

- [ ] **Step 14.6: Verify no out-of-scope changes**

```bash
git status
```

Expected: Only in-scope files modified, working tree clean

---

## Done Criteria

ALL must hold:

- [ ] `cd apps/desktop/src-tauri && cargo check` exits 0
- [ ] `cargo test --lib account::pat_tests` exits 0, 6 tests pass
- [ ] `cargo test --test integration_pat_auth` exits 0
- [ ] `grep "Personal Access Token" docs/FINAL-DESIGN.md README.md` returns 2+ matches
- [ ] `git status` shows no modifications outside in-scope files
- [ ] `grep -c "auth_metadata_dir\|auth_metadata_path" apps/desktop/src-tauri/src/services/types.rs` returns 2
- [ ] All commits follow conventional commit format
- [ ] Feature branch exists with all changes committed

---

## STOP Conditions

Stop and report (do not improvise) if:

1. **Drift detected**: Files changed since commit `6e4471e`
   - Run: `git diff --stat 6e4471e..HEAD -- apps/desktop/src-tauri/src/services/account.rs apps/desktop/src-tauri/src/services/types.rs apps/desktop/src-tauri/src/commands/mod.rs apps/desktop/src-tauri/src/main.rs`
   - If output shows changes, compare "Current state" excerpts
   - Function signatures changed, moved, or removed = STOP

2. **Test fails twice** after reasonable fix attempt

3. **Existing OAuth breaks**: Run `list_accounts` and verify non-PAT accounts still show correct auth_mode

4. **Codex files modified**: Any change to `~/.codex*/auth.json` or `config.toml` (out of scope)

5. **Unrelated errors**: `cargo check` fails in files not listed in scope

6. **Export verification fails**: Missing `pub use account` in `services/mod.rs`

**If stopped mid-execution**:
1. Commit WIP: `git commit -m "WIP: [task name] - [blocker description]"`
2. Document blocker in commit message
3. Update `plans/README.md` status to `BLOCKED` with reason
4. Do NOT leave uncommitted changes

---

## Maintenance Notes

**For future developers:**

1. **Lam-only feature** — Never modifies Codex files, only reads and displays

2. **Auth detection priority**:
   - First: Lam metadata (`~/.config/agent-workspace/auth-metadata/`)
   - Second: Codex `auth.json` inspection (read-only)
   - Third: `config.toml` parsing

3. **Separation of concerns**:
   - **Lam**: Tracks, displays, provides UI
   - **Codex**: Uses whatever exists in `CODEX_HOME`

4. **Expiration thresholds**:
   - ok: >30 days
   - warning: 8-30 days
   - critical: 1-7 days
   - expired: <0 days

5. **Frontend TODO** (deferred):
   - Upload UI component
   - Dashboard expiration warnings
   - Account settings integration
   
   Frontend calls:
   - `upload_pat_credentials(profile_id, credentials)`
   - `get_pat_metadata(profile_id)`
   - `check_profile_token_expiration(profile_id)`

6. **Testing**:
   - Always test mixed PAT + OAuth accounts
   - Verify Codex files never modified
   - Check metadata directory permissions

**For PR reviewers:**

- Confirm no Codex `auth.json`/`config.toml` modified
- Verify metadata in `~/.config/agent-workspace/auth-metadata/` only
- Test auth detection with mixed account types
- Verify all 6 unit tests + integration test pass
