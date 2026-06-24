# Plan 004: PAT Account Switching (Core Feature)

> **Status**: DRAFT - Review before executing
>
> **Important**: This plan implements the ACTUAL PAT feature (switching accounts using PAT credentials).
> Plans 001-003 implemented metadata tracking, which is auxiliary.

## Goal

Implement PAT-based account switching: when user switches to a profile using PAT mode, copy credentials to `~/.codex/auth.json` instead of running `codex login`.

## Context: The Real Requirements

### Current OAuth Switching Flow
1. User clicks "Switch to account A"
2. Backend copies `~/.codex-a/config.toml` → `~/.codex/config.toml`
3. User manually runs `codex login` in terminal
4. Codex writes OAuth token to `~/.codex/auth.json`

### New PAT Switching Flow (This Plan)
1. User clicks "Switch to account A" → chooses "Use PAT"
2. User uploads credential JSON (from external system)
3. Backend:
   - Extracts `access_token` from `headers.authorization`
   - Generates `~/.codex/auth.json` with `personal_access_token` field
   - Copies `~/.codex-a/config.toml` → `~/.codex/config.toml`
   - Records expiration to Lam metadata (for warnings)
4. User can immediately use Codex (no manual login needed)

### Credential JSON Format (User Upload)
```json
{
  "access_token": "",
  "account_id": "0f8a3680-94af-4885-a91c-3232c7702257",
  "email": "user@example.com",
  "expired": "2030-12-31T10:00:00+08:00",
  "headers": {
    "authorization": "Bearer at-xxx-token-here"
  },
  "type": "codex",
  "websockets": true
}
```

### Generated auth.json (Lam Writes to ~/.codex/)
```json
{
  "OPENAI_API_KEY": null,
  "personal_access_token": "at-xxx-token-here"
}
```

## Architecture

**Key Changes:**
1. Add `switch_account_with_pat()` backend function
2. Modify frontend "Switch" button to show mode selection
3. PAT mode opens upload modal, calls new switch function
4. Function writes to `~/.codex/auth.json` (NOT read-only anymore)

## Scope

**In scope:**
- Backend: `switch_account_with_pat()` function
- Backend: Parse uploaded JSON, extract token from `headers.authorization`
- Backend: Write `~/.codex/auth.json` with `personal_access_token` field
- Backend: Copy `config.toml` (reuse existing logic)
- Frontend: Modify Account card "Switch" button
- Frontend: Add mode selection dialog: "OAuth Login" vs "Use PAT"
- Frontend: PAT mode → upload modal → call new command

**Out of scope:**
- Creating new accounts (only switching existing ones)
- Modifying existing `codex login` flow (OAuth still works)
- Auto-refresh tokens (manual re-upload when expired)


## Implementation Steps

### Phase 1: Backend - Switch Function

**Step 1.1: Add switch request types**

File: `apps/desktop/src-tauri/src/services/account.rs`

After `CreateRelayRequest` struct (around line 48), add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchAccountWithPatRequest {
    pub profile_id: String,
    pub credentials: UploadedCredentials,
}
```

**Step 1.2: Implement switch_account_with_pat function**

File: `apps/desktop/src-tauri/src/services/account.rs`

Add function after PAT metadata functions (around line 850):

```rust
/// Switches to a profile using PAT credentials
/// Writes auth.json to ~/.codex/ with personal_access_token field
pub fn switch_account_with_pat(
    home_root: &Path,
    req: &SwitchAccountWithPatRequest,
) -> Result<()> {
    // 1. Validate profile exists
    let profile = find_account(home_root, &req.profile_id)?;
    
    // 2. Extract token from headers.authorization
    let token = extract_bearer_token(&req.credentials)?;
    
    // 3. Generate auth.json content
    let auth_json = generate_pat_auth_json(&token);
    
    // 4. Write to ~/.codex/auth.json
    let main_codex = home_root.join(".codex");
    std::fs::create_dir_all(&main_codex)?;
    let auth_path = main_codex.join("auth.json");
    write_file_private(&auth_path, &auth_json)?;
    
    // 5. Copy config.toml
    let source_config = profile.codex_home.join("config.toml");
    let target_config = main_codex.join("config.toml");
    if source_config.exists() {
        std::fs::copy(&source_config, &target_config)?;
    }
    
    // 6. Record PAT metadata for expiration tracking
    let metadata = AuthMetadata {
        profile_id: req.profile_id.clone(),
        auth_type: "personal_token".to_string(),
        token_expiration: Some(req.credentials.expired.clone()),
        last_checked: chrono::Utc::now().to_rfc3339(),
    };
    record_pat_metadata(home_root, &req.profile_id, &metadata)?;
    
    Ok(())
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

**Step 1.3: Add Tauri command**

File: `apps/desktop/src-tauri/src/commands/mod.rs`

Add to imports:

```rust
use localagentmanager_core::switch_account_with_pat as core_switch_account_with_pat;
use localagentmanager_core::SwitchAccountWithPatRequest;
```

Add command:

```rust
#[tauri::command]
pub fn switch_account_with_pat(
    req: SwitchAccountWithPatRequest,
) -> Result<(), AppError> {
    core_switch_account_with_pat(&home_root()?, &req)
}
```

**Step 1.4: Register command**

File: `apps/desktop/src-tauri/src/main.rs`

Add to invoke_handler list:

```rust
commands::switch_account_with_pat,
```

### Phase 2: Frontend - Mode Selection

**Step 2.1: Add switch mode to types**

File: `apps/desktop/src/lib/types.ts`

After TokenExpirationStatus, add:

```typescript
export type SwitchAccountWithPatRequest = {
  profileId: string;
  credentials: UploadedCredentials;
};
```

**Step 2.2: Add API function**

File: `apps/desktop/src/lib/api.ts`

```typescript
export async function switchAccountWithPat(
  req: SwitchAccountWithPatRequest
): Promise<void> {
  return invoke<void>("switch_account_with_pat", { req });
}
```

**Step 2.3: Add switch mode modal**

File: `apps/desktop/src/App.tsx`

Add modal state:
```typescript
const [switchMode, setSwitchMode] = useState<'oauth' | 'pat' | null>(null);
const [switchTargetId, setSwitchTargetId] = useState('');
```

Add handler:
```typescript
async function handleSwitchWithPat(profileId: string, credentials: UploadedCredentials) {
  try {
    await api.switchAccountWithPat({ profileId, credentials });
    setModal(null);
    setSwitchMode(null);
    await loadAccounts();
    appState.set({ status: `Switched to ${profileId} using PAT` });
  } catch (err) {
    appState.set({ 
      status: 'Failed to switch account', 
      error: err instanceof Error ? err.message : String(err) 
    });
  }
}
```


Add modal UI (after uploadPat modal):

```typescript
{switchMode && switchTargetId ? (
  <Shell.Modal title="Choose Switch Method" close={() => setSwitchMode(null)}>
    <p className="modalHint">
      How would you like to authenticate for profile {switchTargetId}?
    </p>
    <div className="switchModeButtons">
      <button
        className="switchModeBtn"
        onClick={() => {
          setSwitchMode(null);
          // TODO: Trigger traditional OAuth login
          // This will copy config.toml and show "run codex login" instruction
        }}
      >
        <strong>OAuth Login</strong>
        <small>Traditional: copy config, then run codex login</small>
      </button>
      <button
        className="switchModeBtn"
        onClick={() => {
          setSwitchMode('pat');
          openModal('uploadPat');
          setUploadPatAccountId(switchTargetId);
        }}
      >
        <strong>Use PAT</strong>
        <small>Upload credentials JSON, no manual login needed</small>
      </button>
    </div>
  </Shell.Modal>
) : null}
```

**Step 2.4: Modify Account card button**

File: `apps/desktop/src/routes/views.tsx`

Find the "Switch" or "Relay Latest" button, modify to add switch with PAT option:

```typescript
<UIButton
  size="sm"
  variant="primary"
  className="accountActionBtn"
  aria-label="Switch Account"
  title={`Switch to ${account.displayName}`}
  onClick={(e) => {
    e.stopPropagation();
    openSwitchModeDialog(account.id);
  }}
>
  <IconPlay size={13} />
  Switch
</UIButton>
```

Pass `openSwitchModeDialog` from App.tsx:

```typescript
openSwitchModeDialog={(accountId) => {
  setSwitchTargetId(accountId);
  setSwitchMode('oauth'); // Default, will show choice modal
  openModal('switchMode');
}}
```

### Phase 3: Testing

**Step 3.1: Write backend test**

File: `apps/desktop/src-tauri/tests/integration_pat_switching.rs` (NEW)

```rust
use localagentmanager_core::{
    switch_account_with_pat, UploadedCredentials, SwitchAccountWithPatRequest,
};
use std::collections::HashMap;
use tempfile::TempDir;

#[test]
fn test_switch_account_with_pat() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    
    // Create test profile
    let profile_home = home.join(".codex-test");
    std::fs::create_dir_all(&profile_home).unwrap();
    std::fs::write(
        profile_home.join("config.toml"),
        r#"provider_id = "test""#
    ).unwrap();
    std::fs::write(profile_home.join(".managed-by-agent-workspace.json"), "{}").unwrap();
    
    // Prepare credentials
    let mut headers = HashMap::new();
    headers.insert("authorization".to_string(), serde_json::Value::String("Bearer at-test-token-123".to_string()));
    
    let creds = UploadedCredentials {
        access_token: "".to_string(),
        account_id: "test-id".to_string(),
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
    
    let req = SwitchAccountWithPatRequest {
        profile_id: "test".to_string(),
        credentials: creds,
    };
    
    // Execute switch
    switch_account_with_pat(home, &req).unwrap();
    
    // Verify auth.json was created
    let main_codex = home.join(".codex");
    let auth_json = std::fs::read_to_string(main_codex.join("auth.json")).unwrap();
    assert!(auth_json.contains("personal_access_token"));
    assert!(auth_json.contains("at-test-token-123"));
    
    // Verify config.toml was copied
    assert!(main_codex.join("config.toml").exists());
    
    // Verify metadata was recorded
    let metadata_path = home
        .join(".config/agent-workspace/auth-metadata/test.json");
    assert!(metadata_path.exists());
}
```

**Step 3.2: Manual verification checklist**

1. Start app with test account "a"
2. Click "Switch to account a"
3. Choose "Use PAT" 
4. Upload test JSON:
```json
{
  "access_token": "",
  "account_id": "test",
  "email": "test@example.com",
  "expired": "2030-12-31T10:00:00+08:00",
  "headers": {
    "authorization": "Bearer at-manual-test-token"
  },
  "type": "codex",
  "websockets": true
}
```
5. Verify:
   - `~/.codex/auth.json` contains `personal_access_token: "at-manual-test-token"`
   - `~/.codex/config.toml` copied from `~/.codex-a/config.toml`
   - Run `codex resume` works without manual login
   - Account card shows "PAT" badge

## Verification

**Automated:**
- [ ] `cargo test --test integration_pat_switching` passes
- [ ] `cargo check` passes
- [ ] `npm run build` passes

**Manual:**
- [ ] Switch mode dialog appears
- [ ] "Use PAT" flow writes correct auth.json
- [ ] OAuth flow still works (unchanged)
- [ ] Codex accepts generated auth.json
- [ ] No manual `codex login` needed after PAT switch

## Done Criteria

- [ ] Backend: `switch_account_with_pat()` implemented
- [ ] Backend: Extracts token from `headers.authorization`
- [ ] Backend: Writes `~/.codex/auth.json` with `personal_access_token`
- [ ] Backend: Copies config.toml
- [ ] Frontend: Switch mode dialog implemented
- [ ] Frontend: PAT upload flow calls new command
- [ ] Tests: Integration test passes
- [ ] Manual: Successfully switch using PAT JSON
- [ ] Manual: Codex works without manual login

## Relationship to Plans 001-003

**Plans 001-003 provided:**
- UploadedCredentials type ✅
- AuthMetadata type ✅
- record_pat_metadata() function ✅
- UI components (badges, modal) ✅

**Plan 004 adds the CORE feature:**
- Actually writes to ~/.codex/auth.json
- Implements account switching with PAT
- Completes the user workflow

Plans 001-003 are **auxiliary** (tracking/display), Plan 004 is **core** (switching).

