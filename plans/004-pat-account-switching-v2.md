# Plan 004 v2: PAT Account Management (Correct Architecture)

> **Status**: DRAFT - Ready for review
> **Priority**: P0 - Core feature
> **Depends on**: Plans 001-003 (types and metadata functions)

## Goal

Implement PAT account management: add PAT accounts by uploading credentials, switch between accounts by copying auth files. PAT accounts are lightweight - no separate directories, share config.toml and sessions.

---

## Architecture

### File Organization

**Lam PAT Storage** (new):
```
~/.config/agent-workspace/pat-accounts/
  ├── auth-{account_id}.json        # Generated from upload, contains PAT
  └── metadata-{account_id}.json    # Expiration, email, type
```

**Codex Shared Directory** (existing):
```
~/.codex/
  ├── auth.json                     # Current active account (copied on switch)
  ├── config.toml                   # SHARED by all accounts, never modified
  ├── sessions/                     # SHARED by all accounts
  └── history.jsonl                 # SHARED
```

**OAuth Directories** (existing, unchanged):
```
~/.codex-a/                         # Independent OAuth account
~/.codex-b/                         # Independent OAuth account
```

### Two Account Types

| Type | Storage | config.toml | sessions/ | Switch Method |
|------|---------|-------------|-----------|---------------|
| **OAuth** | `~/.codex-{id}/` | Per-account | Per-account | Copy entire dir + manual login |
| **PAT** | `~/.config/.../pat-accounts/auth-{id}.json` | Shared | Shared | Copy auth file only |

---

## User Workflows

### Workflow 1: Add PAT Account

**User action:**
1. Click "Add Account" → Choose "Use PAT"
2. Upload credentials JSON
3. Account appears in list immediately

**Backend:**
```rust
1. Parse JSON → extract account_id, headers.authorization, expired
2. Generate auth-{account_id}.json:
   {
     "OPENAI_API_KEY": null,
     "personal_access_token": "at-xxx"  // from headers.authorization
   }
3. Save to ~/.config/agent-workspace/pat-accounts/auth-{id}.json
4. Save metadata-{id}.json (expired, email, type)
```

**No directory created, no config.toml copied.**

### Workflow 2: Switch to PAT Account

**User action:**
1. Click "Switch to account {id}"
2. Instant switch (no login needed)

**Backend:**
```bash
cp ~/.config/agent-workspace/pat-accounts/auth-{id}.json → ~/.codex/auth.json
# Done! config.toml and sessions/ already shared
```

### Workflow 3: List Accounts

**Backend scans:**
```
OAuth accounts: ~/.codex-*/  (existing logic)
  +
PAT accounts: ~/.config/agent-workspace/pat-accounts/auth-*.json
```

**UI shows both types with different badges.**


---

## Implementation Steps

### Phase 1: Backend - Storage Layer

**File**: `apps/desktop/src-tauri/src/services/types.rs`

Add helper functions after `auth_metadata_path()` (around line 59):

```rust
/// Returns the PAT accounts directory
pub(crate) fn pat_accounts_dir(home_root: &Path) -> PathBuf {
    config_root(home_root).join("pat-accounts")
}

/// Returns auth file path for a PAT account
pub(crate) fn pat_auth_path(home_root: &Path, account_id: &str) -> PathBuf {
    pat_accounts_dir(home_root).join(format!("auth-{}.json", account_id))
}

/// Returns metadata file path for a PAT account
pub(crate) fn pat_metadata_path(home_root: &Path, account_id: &str) -> PathBuf {
    pat_accounts_dir(home_root).join(format!("metadata-{}.json", account_id))
}
```

---

### Phase 2: Backend - Add PAT Account

**File**: `apps/desktop/src-tauri/src/services/account.rs`

Add request type after `CreateRelayRequest`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddPatAccountRequest {
    pub credentials: UploadedCredentials,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddPatAccountResult {
    pub account_id: String,
    pub email: String,
    pub expired: String,
}
```

Add function after PAT metadata functions (around line 850):

```rust
/// Adds a new PAT account by storing credentials in Lam storage
pub fn add_pat_account(
    home_root: &Path,
    req: &AddPatAccountRequest,
) -> Result<AddPatAccountResult> {
    let account_id = &req.credentials.account_id;
    
    // 1. Validate account_id not empty
    if account_id.trim().is_empty() {
        return Err(AppError::new("INVALID_ACCOUNT_ID", "account_id cannot be empty"));
    }
    
    // 2. Check if account already exists (both OAuth and PAT)
    if codex_home_path(home_root, account_id).exists() {
        return Err(AppError::new("ACCOUNT_EXISTS", 
            format!("OAuth account '{}' already exists", account_id)));
    }
    
    let auth_path = pat_auth_path(home_root, account_id);
    if auth_path.exists() {
        return Err(AppError::new("ACCOUNT_EXISTS", 
            format!("PAT account '{}' already exists", account_id)));
    }
    
    // 3. Extract token from headers.authorization
    let token = extract_bearer_token(&req.credentials)?;
    
    // 4. Generate and save auth-{id}.json
    let auth_json = generate_pat_auth_json(&token);
    let dir = pat_accounts_dir(home_root);
    std::fs::create_dir_all(&dir).map_err(|e| {
        AppError::new("CREATE_DIR_FAILED", format!("Failed to create pat-accounts dir: {}", e))
    })?;
    write_file_private(&auth_path, &auth_json)?;
    
    // 5. Save metadata
    let metadata = serde_json::json!({
        "accountId": account_id,
        "email": req.credentials.email,
        "expired": req.credentials.expired,
        "type": req.credentials.type_,
        "addedAt": chrono::Utc::now().to_rfc3339(),
    });
    let metadata_path = pat_metadata_path(home_root, account_id);
    let metadata_str = serde_json::to_string_pretty(&metadata).map_err(|e| {
        AppError::new("SERIALIZE_FAILED", format!("Metadata serialize failed: {}", e))
    })?;
    write_file_private(&metadata_path, &metadata_str)?;
    
    Ok(AddPatAccountResult {
        account_id: account_id.clone(),
        email: req.credentials.email.clone(),
        expired: req.credentials.expired.clone(),
    })
}

fn extract_bearer_token(creds: &UploadedCredentials) -> Result<String> {
    let headers = creds.headers.as_ref()
        .ok_or_else(|| AppError::new("MISSING_HEADERS", "Credentials missing headers field"))?;
    
    let auth_value = headers.get("authorization")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::new("MISSING_AUTH_HEADER", "Missing authorization header"))?;
    
    if let Some(token) = auth_value.strip_prefix("Bearer ") {
        Ok(token.to_string())
    } else {
        Err(AppError::new("INVALID_AUTH_FORMAT", "Authorization must be 'Bearer <token>'"))
    }
}

fn generate_pat_auth_json(token: &str) -> String {
    format!(r#"{{
  "OPENAI_API_KEY": null,
  "personal_access_token": "{}"
}}"#, json_escape(token))
}
```

---

### Phase 3: Backend - Switch to PAT Account

**File**: `apps/desktop/src-tauri/src/services/account.rs`

Add function:

```rust
/// Switches to a PAT account by copying its auth file to ~/.codex/auth.json
pub fn switch_to_pat_account(
    home_root: &Path,
    account_id: &str,
) -> Result<()> {
    // 1. Verify PAT account exists
    let source_auth = pat_auth_path(home_root, account_id);
    if !source_auth.exists() {
        return Err(AppError::new("ACCOUNT_NOT_FOUND", 
            format!("PAT account '{}' not found", account_id)));
    }
    
    // 2. Copy to ~/.codex/auth.json
    let codex_home = home_root.join(".codex");
    std::fs::create_dir_all(&codex_home).map_err(|e| {
        AppError::new("CREATE_DIR_FAILED", format!("Failed to create .codex dir: {}", e))
    })?;
    
    let target_auth = codex_home.join("auth.json");
    std::fs::copy(&source_auth, &target_auth).map_err(|e| {
        AppError::new("COPY_FAILED", format!("Failed to copy auth file: {}", e))
    })?;
    
    // Set private permissions
    set_file_private(&target_auth)?;
    
    Ok(())
}
```

---

### Phase 4: Backend - List PAT Accounts

**File**: `apps/desktop/src-tauri/src/services/account.rs`

Modify `list_accounts()` function to include PAT accounts:

Add after scanning OAuth directories (around line 560):

```rust
// Scan PAT accounts
let pat_dir = pat_accounts_dir(home_root);
if pat_dir.exists() {
    if let Ok(entries) = std::fs::read_dir(&pat_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("auth-") && name.ends_with(".json") {
                    // Extract account_id from "auth-{id}.json"
                    let account_id = name
                        .strip_prefix("auth-")
                        .and_then(|s| s.strip_suffix(".json"))
                        .unwrap_or("");
                    
                    if account_id.is_empty() {
                        continue;
                    }
                    
                    // Read metadata
                    let metadata_path = pat_metadata_path(home_root, account_id);
                    let metadata: Option<serde_json::Value> = std::fs::read_to_string(&metadata_path)
                        .ok()
                        .and_then(|s| serde_json::from_str(&s).ok());
                    
                    let email = metadata.as_ref()
                        .and_then(|m| m.get("email"))
                        .and_then(|e| e.as_str())
                        .unwrap_or("");
                    
                    let expired = metadata.as_ref()
                        .and_then(|m| m.get("expired"))
                        .and_then(|e| e.as_str());
                    
                    accounts.push(CodexAccount {
                        id: account_id.to_string(),
                        display_name: format!("PAT: {} ({})", account_id, email),
                        codex_home: home_root.join(".codex"), // Shared
                        wrapper_path: None,
                        has_auth: true,
                        has_config: true, // Shares config.toml
                        has_history: false,
                        session_count: 0, // TODO: count shared sessions?
                        latest_session_modified_at: None,
                        managed: true,
                        is_relay: false,
                        relay_source: None,
                        relay_identity: None,
                        provider_id: Some("anthropic".to_string()), // Assume Codex
                        model: None,
                        auth_mode: Some("personal_token".to_string()),
                        renewal_date: expired.map(|s| s.to_string()),
                        note: None,
                    });
                }
            }
        }
    }
}
```


---

### Phase 5: Backend - Tauri Commands

**File**: `apps/desktop/src-tauri/src/commands/mod.rs`

Add imports:

```rust
use localagentmanager_core::{
    add_pat_account as core_add_pat_account,
    switch_to_pat_account as core_switch_to_pat_account,
    AddPatAccountRequest,
    AddPatAccountResult,
};
```

Add commands:

```rust
#[tauri::command]
pub fn add_pat_account(
    req: AddPatAccountRequest,
) -> Result<AddPatAccountResult, AppError> {
    core_add_pat_account(&home_root()?, &req)
}

#[tauri::command]
pub fn switch_to_pat_account(
    account_id: String,
) -> Result<(), AppError> {
    core_switch_to_pat_account(&home_root()?, &account_id)
}
```

**File**: `apps/desktop/src-tauri/src/main.rs`

Register commands:

```rust
commands::add_pat_account,
commands::switch_to_pat_account,
```

---

### Phase 6: Frontend - Types and API

**File**: `apps/desktop/src/lib/types.ts`

Add types:

```typescript
export type AddPatAccountRequest = {
  credentials: UploadedCredentials;
};

export type AddPatAccountResult = {
  accountId: string;
  email: string;
  expired: string;
};
```

**File**: `apps/desktop/src/lib/api.ts`

Add API functions:

```typescript
export async function addPatAccount(
  req: AddPatAccountRequest
): Promise<AddPatAccountResult> {
  return invoke<AddPatAccountResult>("add_pat_account", { req });
}

export async function switchToPatAccount(accountId: string): Promise<void> {
  return invoke<void>("switch_to_pat_account", { accountId });
}
```

---

### Phase 7: Frontend - Add Account Flow

**File**: `apps/desktop/src/App.tsx`

Modify account creation modal to support PAT mode:

Add state:

```typescript
const [createMode, setCreateMode] = useState<'oauth' | 'pat'>('oauth');
```

Add handler:

```typescript
async function handleAddPatAccount(credentials: UploadedCredentials) {
  try {
    const result = await api.addPatAccount({ credentials });
    setModal(null);
    await loadAccounts();
    appState.set({ 
      status: `PAT account '${result.accountId}' added successfully` 
    });
  } catch (err) {
    appState.set({ 
      status: 'Failed to add PAT account', 
      error: err instanceof Error ? err.message : String(err) 
    });
  }
}
```

Add modal UI (modify existing create account modal or add new one):

```typescript
{modal === 'createAccount' && (
  <Shell.Modal title="Add Account" close={closeModal}>
    <div className="createModeTabs">
      <button
        className={createMode === 'oauth' ? 'active' : ''}
        onClick={() => setCreateMode('oauth')}
      >
        OAuth (Traditional)
      </button>
      <button
        className={createMode === 'pat' ? 'active' : ''}
        onClick={() => setCreateMode('pat')}
      >
        PAT (Upload Credentials)
      </button>
    </div>

    {createMode === 'oauth' ? (
      <div>
        {/* Existing OAuth account creation form */}
      </div>
    ) : (
      <div>
        <p className="modalHint">
          Upload credentials JSON from your PAT provider:
        </p>
        <form onSubmit={(e) => {
          e.preventDefault();
          const formData = new FormData(e.currentTarget);
          const jsonStr = formData.get('credentialsJson') as string;
          try {
            const creds = JSON.parse(jsonStr) as UploadedCredentials;
            handleAddPatAccount(creds);
          } catch (err) {
            appState.set({ error: 'Invalid JSON format' });
          }
        }}>
          <textarea
            name="credentialsJson"
            className="uploadPatTextarea"
            placeholder={`{
  "access_token": "",
  "account_id": "your-account-id",
  "email": "you@example.com",
  "expired": "2030-12-31T10:00:00+08:00",
  "headers": {
    "authorization": "Bearer at-xxx"
  },
  "type": "codex",
  "websockets": true
}`}
            rows={15}
            required
          />
          <div className="modalFoot">
            <UIButton type="button" variant="ghost" onClick={closeModal}>
              Cancel
            </UIButton>
            <div className="modalFootPrimary">
              <UIButton type="submit" variant="primary">
                Add Account
              </UIButton>
            </div>
          </div>
        </form>
      </div>
    )}
  </Shell.Modal>
)}
```

---

### Phase 8: Frontend - Switch Account Flow

**File**: `apps/desktop/src/routes/views.tsx`

Modify account card "Switch" button to detect account type:

```typescript
<UIButton
  size="sm"
  variant="primary"
  className="accountActionBtn"
  aria-label="Switch Account"
  title={`Switch to ${account.displayName}`}
  onClick={async (e) => {
    e.stopPropagation();
    try {
      if (account.authMode === 'personal_token') {
        // PAT account: direct switch
        await api.switchToPatAccount(account.id);
        // Reload accounts to update UI
        onRefresh();
      } else {
        // OAuth account: existing relay/handoff flow
        openHandoff(account);
      }
    } catch (err) {
      console.error('Switch failed:', err);
    }
  }}
>
  <IconPlay size={13} />
  Switch
</UIButton>
```

Pass `onRefresh` callback from App.tsx:

```typescript
onRefresh={loadAccounts}
```

---

### Phase 9: Testing

**File**: `apps/desktop/src-tauri/tests/integration_pat_accounts.rs` (NEW)

```rust
use localagentmanager_core::{
    add_pat_account, switch_to_pat_account, list_accounts,
    AddPatAccountRequest, UploadedCredentials,
};
use std::collections::HashMap;
use tempfile::TempDir;

#[test]
fn test_add_and_switch_pat_account() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    
    // Prepare credentials
    let mut headers = HashMap::new();
    headers.insert(
        "authorization".to_string(), 
        serde_json::Value::String("Bearer at-test-token-123".to_string())
    );
    
    let creds = UploadedCredentials {
        access_token: "".to_string(),
        account_id: "test-pat-account".to_string(),
        disabled: false,
        email: "test@example.com".to_string(),
        expired: "2030-12-31T23:59:59+00:00".to_string(),
        headers: Some(headers),
        id_token: None,
        last_refresh: "2026-06-24T00:00:00+00:00".to_string(),
        refresh_token: None,
        type_: "codex".to_string(),
        websockets: true,
    };
    
    // Test: Add PAT account
    let req = AddPatAccountRequest { credentials: creds };
    let result = add_pat_account(home, &req).unwrap();
    assert_eq!(result.account_id, "test-pat-account");
    assert_eq!(result.email, "test@example.com");
    
    // Verify: auth file created
    let auth_path = home
        .join(".config/agent-workspace/pat-accounts/auth-test-pat-account.json");
    assert!(auth_path.exists());
    let auth_content = std::fs::read_to_string(&auth_path).unwrap();
    assert!(auth_content.contains("personal_access_token"));
    assert!(auth_content.contains("at-test-token-123"));
    
    // Verify: metadata file created
    let metadata_path = home
        .join(".config/agent-workspace/pat-accounts/metadata-test-pat-account.json");
    assert!(metadata_path.exists());
    
    // Verify: appears in list_accounts
    let accounts = list_accounts(home).unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, "test-pat-account");
    assert_eq!(accounts[0].auth_mode, Some("personal_token".to_string()));
    
    // Test: Switch to PAT account
    switch_to_pat_account(home, "test-pat-account").unwrap();
    
    // Verify: auth.json copied to ~/.codex/
    let target_auth = home.join(".codex/auth.json");
    assert!(target_auth.exists());
    let target_content = std::fs::read_to_string(&target_auth).unwrap();
    assert!(target_content.contains("at-test-token-123"));
}

#[test]
fn test_add_duplicate_account_fails() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    
    let mut headers = HashMap::new();
    headers.insert(
        "authorization".to_string(), 
        serde_json::Value::String("Bearer at-test".to_string())
    );
    
    let creds = UploadedCredentials {
        access_token: "".to_string(),
        account_id: "dup".to_string(),
        disabled: false,
        email: "test@example.com".to_string(),
        expired: "2030-12-31T23:59:59+00:00".to_string(),
        headers: Some(headers.clone()),
        id_token: None,
        last_refresh: "2026-06-24T00:00:00+00:00".to_string(),
        refresh_token: None,
        type_: "codex".to_string(),
        websockets: true,
    };
    
    // Add first time - should succeed
    let req = AddPatAccountRequest { credentials: creds.clone() };
    add_pat_account(home, &req).unwrap();
    
    // Add again - should fail
    let req2 = AddPatAccountRequest { credentials: creds };
    let result = add_pat_account(home, &req2);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("already exists"));
}
```


---

## Verification

### Automated Tests

```bash
# Backend tests
cd apps/desktop/src-tauri
cargo test --test integration_pat_accounts

# Frontend build
cd apps/desktop
npm run build
npm test
```

**Expected:**
- [ ] Integration tests pass (2 tests)
- [ ] No compilation errors
- [ ] All existing tests still pass

### Manual Verification

**Test Case 1: Add PAT Account**

1. Start app: `npm run tauri:dev`
2. Click "Add Account" button
3. Switch to "PAT (Upload Credentials)" tab
4. Paste test JSON:
```json
{
  "access_token": "",
  "account_id": "manual-test-001",
  "email": "test@example.com",
  "expired": "2030-12-31T10:00:00+08:00",
  "headers": {
    "authorization": "Bearer at-manual-test-token"
  },
  "type": "codex",
  "websockets": true
}
```
5. Click "Add Account"

**Verify:**
- [ ] Success message appears
- [ ] Account "manual-test-001" appears in account list
- [ ] Badge shows "PAT"
- [ ] File exists: `~/.config/agent-workspace/pat-accounts/auth-manual-test-001.json`
- [ ] File contains: `"personal_access_token": "at-manual-test-token"`

**Test Case 2: Switch to PAT Account**

1. Click "Switch" on PAT account card
2. Wait for operation to complete

**Verify:**
- [ ] File copied: `~/.codex/auth.json` 
- [ ] Content matches: `at-manual-test-token`
- [ ] Run in terminal: `codex resume --last --all` (should work without login)
- [ ] config.toml unchanged (still exists at `~/.codex/config.toml`)

**Test Case 3: List Shows Both Account Types**

**Verify:**
- [ ] OAuth accounts show (from `~/.codex-*/`)
- [ ] PAT accounts show (from pat-accounts/)
- [ ] PAT accounts have badge "PAT"
- [ ] Both types clickable and functional

**Test Case 4: Duplicate Account Prevention**

1. Try to add same account_id again

**Verify:**
- [ ] Error message: "Account already exists"
- [ ] No duplicate created

---

## Done Criteria

**Backend:**
- [ ] `add_pat_account()` implemented
- [ ] `switch_to_pat_account()` implemented
- [ ] `list_accounts()` modified to scan PAT accounts
- [ ] Tauri commands registered
- [ ] Integration tests pass (2 tests)

**Frontend:**
- [ ] "Add Account" supports PAT mode
- [ ] Upload modal accepts credentials JSON
- [ ] "Switch" button detects PAT accounts
- [ ] PAT switch calls correct API
- [ ] Account list displays PAT accounts

**Manual:**
- [ ] Add PAT account works
- [ ] Switch to PAT account works
- [ ] Codex accepts generated auth.json
- [ ] No manual login needed
- [ ] OAuth accounts still work (no regression)

---

## Rollout Plan

**Phase 1: Backend + Tests** (1-2 hours)
- Implement storage layer
- Implement add/switch functions
- Write integration tests
- Verify: `cargo test` passes

**Phase 2: Frontend UI** (1 hour)
- Add account creation UI
- Modify switch button
- Test manually with sample JSON

**Phase 3: Polish** (30 minutes)
- Error messages
- Loading states
- Badge styling
- Documentation

---

## Migration Notes

**For existing users:**
- OAuth accounts (`.codex-*/`) continue working unchanged
- No migration needed
- PAT is opt-in for new accounts

**For Plans 001-003:**
- Types reused: `UploadedCredentials`
- Functions NOT reused (different architecture)
- Metadata functions can be adapted later for expiration tracking

---

## Summary

**What this plan does:**
1. ✅ Add PAT accounts by uploading credentials
2. ✅ Store auth files in Lam directory (not separate `.codex-*` dirs)
3. ✅ Switch accounts by copying auth file
4. ✅ Share config.toml and sessions/ between PAT accounts
5. ✅ List both OAuth and PAT accounts
6. ✅ No manual `codex login` needed for PAT accounts

**What it doesn't do:**
- ❌ Auto-refresh tokens (manual re-upload when expired)
- ❌ Migrate existing OAuth accounts to PAT
- ❌ Separate sessions per account (all shared)
- ❌ Edit PAT accounts (delete + re-add instead)

**Architecture:**
- Lightweight PAT storage (auth-{id}.json only)
- Shared Codex infrastructure (config, sessions, history)
- Both account types coexist peacefully

**Ready for implementation!** 🚀


---

## ADDENDUM: Quota API Support for PAT

### How Quota Works with PAT

**Current Implementation (OAuth):**
```
spawn_codex_app_server()
  ↓ env: CODEX_HOME=~/.codex-a
  ↓ Codex reads: ~/.codex-a/auth.json {"token": "oauth-token"}
  ↓ Codex sends MCP request: account/rateLimits/read
  ↓ Codex calls API with OAuth token
  ↓ Returns quota
```

**With PAT (Plan 004 v2):**
```
spawn_codex_app_server()
  ↓ env: CODEX_HOME=~/.codex
  ↓ Codex reads: ~/.codex/auth.json {"personal_access_token": "at-xxx"}
  ↓ Codex sends MCP request: account/rateLimits/read
  ↓ Codex calls API with: Authorization: Bearer at-xxx
  ↓ Returns quota
```

### Critical Requirement

**Codex MUST support the `personal_access_token` field in auth.json.**

When Codex sees:
```json
{
  "OPENAI_API_KEY": null,
  "personal_access_token": "at-xxx"
}
```

It MUST:
1. Detect the field
2. Extract the token value
3. Add HTTP header: `Authorization: Bearer at-xxx`
4. Make API request to Anthropic

### Verification Test

After implementing Plan 004 v2, test quota fetch:

```bash
# 1. Switch to PAT account
# 2. Check auth.json
cat ~/.codex/auth.json
# Should show: "personal_access_token": "at-xxx"

# 3. Trigger quota refresh in Lam UI
# Click refresh button on PAT account

# 4. Check logs/console for errors
# Success: Quota displayed
# Failure: "Authentication failed" or similar error
```

**If quota fails:**
- Check Codex version (may need update)
- Check if `personal_access_token` is supported
- Check Codex logs for auth errors

### Fallback (If Codex Doesn't Support PAT)

If Codex doesn't support `personal_access_token`, we'll need **Plan 005: Direct API Quota Fetch**:

1. Detect PAT account
2. Skip `spawn_codex_app_server()`
3. Make direct HTTP request to Anthropic API:
   ```rust
   reqwest::Client::new()
       .get("https://api.anthropic.com/v1/account/quota")
       .header("Authorization", format!("Bearer {}", token))
       .send()
   ```
4. Parse response

**But this is only needed if Codex doesn't support PAT natively.**

### Assumption

**We assume Codex supports `personal_access_token` field.**

The current implementation in Plan 004 v2 relies on this. If it doesn't work, we create Plan 005.

