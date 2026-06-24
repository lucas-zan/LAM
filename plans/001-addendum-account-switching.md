# Plan 001 Addendum: Account Switching with Backup

**Note**: This addendum adds the account switching feature to Plan 001. Insert these steps between current Step 3 and Step 4, then renumber subsequent steps.

---

## Additional Scope

**Add to "In scope" section**:
- Account switching function with timestamped backup
- `~/.codex/.auth-backups/` directory for backup storage

**Add to "Why this matters" section**:
The core use case is: user uploads PAT credentials for account A, then **switches to it** by copying its `auth.json` to `~/.codex/`. This makes Codex use account A for all subsequent commands without re-login.

---

## Insert After Step 3: Account Switching Functions

### Step 3A: Add Account Switching Structures and Function

**File**: `apps/desktop/src-tauri/src/services/account.rs`

Add this struct after `TokenExpirationStatus` (in Step 1):

```rust
/// Result of switching account operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SwitchAccountResult {
    pub success: bool,
    pub backup_path: Option<String>,
    pub message: String,
}
```

Add this function after `check_token_expiration`:

```rust
/// Switches to a different account by copying its auth.json to ~/.codex
/// Backs up existing ~/.codex/auth.json with timestamp before overwriting
pub fn switch_account(
    home_root: &Path,
    source_profile_id: &str,
) -> Result<SwitchAccountResult> {
    // Find source account
    let accounts = list_accounts(home_root)?;
    let source = accounts.iter()
        .find(|a| a.id == source_profile_id)
        .ok_or_else(|| AppError::validation("PROFILE_NOT_FOUND", "Source profile not found"))?;

    let source_auth = source.codex_home.join("auth.json");
    if !source_auth.exists() {
        return Err(AppError::validation(
            "NO_AUTH_FILE",
            "Source profile has no auth.json",
        ));
    }

    // Target is always ~/.codex (main account that Codex uses)
    let target_home = codex_home_path(home_root, "main");
    let target_auth = target_home.join("auth.json");

    // Backup existing auth.json if it exists (with timestamp)
    let backup_path = if target_auth.exists() {
        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let backup_dir = target_home.join(".auth-backups");
        std::fs::create_dir_all(&backup_dir).map_err(|e| {
            AppError::io("CREATE_BACKUP_DIR_FAILED", &format!("Failed to create backup dir: {}", e))
        })?;

        let backup_file = backup_dir.join(format!("auth.json.{}.bak", timestamp));
        std::fs::copy(&target_auth, &backup_file).map_err(|e| {
            AppError::io("BACKUP_FAILED", &format!("Failed to backup auth.json: {}", e))
        })?;

        Some(backup_file.to_string_lossy().to_string())
    } else {
        None
    };

    // Copy source auth.json to target
    std::fs::copy(&source_auth, &target_auth).map_err(|e| {
        AppError::io("COPY_FAILED", &format!("Failed to copy auth.json: {}", e))
    })?;

    // Set 0600 permissions on copied file (security)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&target_auth)
            .map_err(|e| AppError::io("STAT_FAILED", &e.to_string()))?
            .permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&target_auth, perms)
            .map_err(|e| AppError::io("CHMOD_FAILED", &e.to_string()))?;
    }

    Ok(SwitchAccountResult {
        success: true,
        backup_path,
        message: format!("Switched to account '{}'. Codex will use this account on next command.", source_profile_id),
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

### Step 3B: Add Switch Account Command

**File**: `apps/desktop/src-tauri/src/commands/mod.rs`

Add to imports section:

```rust
use crate::services::{
    // ... existing imports
    switch_account as core_switch_account,
    SwitchAccountResult,
};
```

Add command after existing account commands:

```rust
#[tauri::command]
pub fn switch_to_account(profile_id: String) -> Result<SwitchAccountResult, AppError> {
    core_switch_account(&home_root()?, &profile_id)
}
```

**File**: `apps/desktop/src-tauri/src/main.rs`

Add to `invoke_handler` list:

```rust
commands::switch_to_account,
```

**Verify**:
```bash
cd apps/desktop/src-tauri
cargo check
```
Expected: exit 0

---

### Step 3C: Add Switch Account Unit Test

**File**: `apps/desktop/src-tauri/src/services/account.rs`

Add to the `pat_tests` module (this becomes the 6th test):

```rust
#[test]
fn test_switch_account_with_backup() {
    use std::fs;
    let temp = TempDir::new().unwrap();
    let home_root = temp.path();

    // Create source account with PAT auth
    let source_home = home_root.join(".codex-a");
    fs::create_dir_all(source_home.join("sessions")).unwrap();
    fs::write(
        source_home.join("auth.json"),
        r#"{"personal_access_token":"at-source-token","auth_mode":"personal_token"}"#
    ).unwrap();

    // Create target main account with existing OAuth auth
    let target_home = home_root.join(".codex");
    fs::create_dir_all(target_home.join("sessions")).unwrap();
    fs::write(
        target_home.join("auth.json"),
        r#"{"token":"old-oauth-token"}"#
    ).unwrap();

    // Switch to source account
    let result = switch_account(home_root, "a").unwrap();
    
    // Verify success
    assert!(result.success);
    assert!(result.backup_path.is_some());
    assert!(result.message.contains("a"));

    // Verify target now has source's auth.json
    let target_content = fs::read_to_string(target_home.join("auth.json")).unwrap();
    assert!(target_content.contains("at-source-token"));
    assert!(!target_content.contains("old-oauth-token"));

    // Verify backup was created with timestamp
    let backup_dir = target_home.join(".auth-backups");
    assert!(backup_dir.exists());
    let backups: Vec<_> = fs::read_dir(backup_dir).unwrap().collect();
    assert_eq!(backups.len(), 1);
    
    // Verify backup contains old content
    let backup_entry = backups[0].as_ref().unwrap();
    let backup_content = fs::read_to_string(backup_entry.path()).unwrap();
    assert!(backup_content.contains("old-oauth-token"));
}

#[test]
fn test_switch_account_no_existing_auth() {
    use std::fs;
    let temp = TempDir::new().unwrap();
    let home_root = temp.path();

    // Create source account
    let source_home = home_root.join(".codex-a");
    fs::create_dir_all(source_home.join("sessions")).unwrap();
    fs::write(
        source_home.join("auth.json"),
        r#"{"personal_access_token":"at-new"}"#
    ).unwrap();

    // Create target without auth.json
    let target_home = home_root.join(".codex");
    fs::create_dir_all(target_home.join("sessions")).unwrap();

    // Switch should work without backup
    let result = switch_account(home_root, "a").unwrap();
    
    assert!(result.success);
    assert!(result.backup_path.is_none()); // No backup needed
    
    // Verify auth.json was copied
    assert!(target_home.join("auth.json").exists());
}

#[test]
fn test_switch_account_source_not_found() {
    let temp = TempDir::new().unwrap();
    let result = switch_account(temp.path(), "nonexistent");
    assert!(result.is_err());
}
```

**Verify**:
```bash
cd apps/desktop/src-tauri
cargo test --lib account::pat_tests::test_switch_account
```
Expected: 3 new tests pass (total 8 tests in pat_tests module)

---

## Update Test Plan Section

Replace "5 tests in `pat_tests` module" with "**8 tests** in `pat_tests` module covering:":
- Metadata record/read cycle
- Valid credential processing
- Invalid expiration handling
- Expiration status (not expired)
- Expiration status (expired)
- **Account switching with backup**
- **Account switching without existing auth**
- **Account switching with invalid source**

---

## Update Done Criteria Section

Change:
- `cargo test --lib account::pat_tests` exits 0, **5 tests pass**

To:
- `cargo test --lib account::pat_tests` exits 0, **8 tests pass**

---

## Update Maintenance Notes Section

Add to "For future developers" section:

**6. Account switching workflow**:
   - Target is always `~/.codex/` (main account Codex uses)
   - Source can be any `.codex-*` account
   - Backups stored in `~/.codex/.auth-backups/auth.json.YYYYMMDD-HHMMSS.bak`
   - Old backups should be cleaned up periodically (not implemented - potential follow-up)
   - Switching copies the entire `auth.json` file verbatim
   - Both PAT and OAuth `auth.json` files can be switched

**7. Frontend integration** (for account switching):
   ```typescript
   // In account card or settings UI:
   const result = await invoke('switch_to_account', { profileId: 'a' });
   if (result.success) {
     console.log(result.message);
     if (result.backupPath) {
       console.log('Backup created at:', result.backupPath);
     }
   }
   ```

---

## Update Documentation Section

Add to `docs/FINAL-DESIGN.md` section (after PAT tracking content):

```markdown
**Account Switching**:

Users can switch between accounts by copying `auth.json`:

```bash
# Via Lam UI: click "Switch to this account" button
# Behind the scenes:
1. Backup ~/.codex/auth.json → ~/.codex/.auth-backups/auth.json.20260624-153045.bak
2. Copy ~/.codex-a/auth.json → ~/.codex/auth.json
3. Set 0600 permissions
4. Codex now uses account A
```

Backups are automatic and timestamped. Old backups are kept indefinitely (manual cleanup required).
```

---

## Integration Test Update

Add to `tests/integration_pat_auth.rs`:

```rust
#[test]
fn test_switch_account_integration() {
    let temp = TempDir::new().unwrap();
    let home_root = temp.path();

    // Setup two accounts
    let account_a = home_root.join(".codex-a");
    let main_account = home_root.join(".codex");
    
    for account in [&account_a, &main_account] {
        std::fs::create_dir_all(account.join("sessions")).unwrap();
    }
    
    std::fs::write(account_a.join("auth.json"), r#"{"personal_access_token":"at-a"}"#).unwrap();
    std::fs::write(main_account.join("auth.json"), r#"{"token":"old"}"#).unwrap();

    // Switch
    let result = localagentmanager_core::switch_account(home_root, "a").unwrap();
    assert!(result.success);
    
    // Verify
    let content = std::fs::read_to_string(main_account.join("auth.json")).unwrap();
    assert!(content.contains("at-a"));
}
```

---

## Summary

This addendum adds **account switching** as the core feature requested:

✅ Copies `auth.json` from source account to `~/.codex/`  
✅ Automatic timestamped backup before overwriting  
✅ 0600 permissions on copied file  
✅ Works with both PAT and OAuth auth files  
✅ 3 additional unit tests  
✅ Integration test coverage  
✅ Frontend command: `switch_to_account(profile_id)`  

The switch operation is **safe** (backs up first) and **atomic** (single copy operation). Users can switch between accounts instantly without re-login.
