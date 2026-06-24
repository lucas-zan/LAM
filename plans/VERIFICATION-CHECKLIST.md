# PAT Feature Verification Checklist

## Pre-Launch Verification ✅

### Backend (Rust)
- ✅ `cargo check` - compiles without errors
- ✅ `cargo test --lib` - 6 unit tests pass (account::pat_tests)
- ✅ `cargo test --test integration_pat_auth` - 1 integration test passes
- ✅ All PAT functions exported and available

### Frontend (TypeScript/React)
- ✅ `npm run build` - compiles without errors (tsc + vite)
- ✅ `npm test` - 36 tests pass (including 5 new PAT API tests)
- ✅ No TypeScript errors
- ✅ All components render without crashes

### Integration Points
- ✅ Types match between Rust and TypeScript (camelCase conversion)
- ✅ Tauri commands registered in main.rs:69-71
- ✅ API functions call correct commands
- ✅ localStorage mock prevents test failures

---

## Manual Verification TODO

### 1. App Launch
- [ ] App starts without crashes
- [ ] No console errors in DevTools
- [ ] Overview page loads correctly
- [ ] Account cards display properly

### 2. Auth Mode Badge
- [ ] Badge shows for accounts with authMode set
- [ ] "PAT" displayed for personal_token accounts
- [ ] "OAuth" displayed for oauth accounts
- [ ] Badge has correct styling (gray background)

### 3. Token Expiration Badge (requires PAT account)
- [ ] No badge shown for accounts >30 days until expiration
- [ ] Yellow "warning" badge for 8-30 days
- [ ] Orange "critical" badge for 1-7 days
- [ ] Red "expired" badge for expired tokens
- [ ] Badge shows "Expires in Xd" text

### 4. Upload PAT Modal
- [ ] "Upload PAT" button visible on account cards
- [ ] Button click opens modal
- [ ] Modal has textarea for JSON input
- [ ] Modal has Cancel and Upload buttons
- [ ] Modal closes on Cancel

### 5. Upload PAT Flow
Test JSON:
```json
{
  "accessToken": "at-test-token-12345",
  "accountId": "test-account",
  "disabled": false,
  "email": "test@example.com",
  "expired": "2030-12-31T23:59:59+00:00",
  "lastRefresh": "2026-06-24T10:00:00+00:00",
  "type": "codex",
  "websockets": true
}
```

Validation steps:
- [ ] Paste valid JSON into textarea
- [ ] Click Upload
- [ ] Modal closes
- [ ] Success message appears
- [ ] Account list refreshes
- [ ] Account now shows "PAT" badge
- [ ] Check `~/.config/agent-workspace/auth-metadata/` for profile JSON file

### 6. Invalid JSON Handling
- [ ] Paste invalid JSON (e.g., `{invalid}`)
- [ ] Click Upload
- [ ] Error message "Invalid JSON format" appears
- [ ] Modal stays open

### 7. Token Expiration Detection
Create test file manually:
```bash
mkdir -p ~/.config/agent-workspace/auth-metadata
cat > ~/.config/agent-workspace/auth-metadata/test.json << 'EOF'
{
  "profileId": "test",
  "authType": "personal_token",
  "tokenExpiration": "2026-07-01T23:59:59+00:00",
  "lastChecked": "2026-06-24T10:00:00+00:00"
}
EOF
```

Then verify:
- [ ] Refresh account list
- [ ] "test" account shows PAT badge
- [ ] Token expiration badge appears (7 days = critical)

### 8. Backend File Verification
Check files created:
- [ ] `~/.config/agent-workspace/auth-metadata/{profile_id}.json` exists
- [ ] File has 0600 permissions
- [ ] JSON structure matches AuthMetadata type
- [ ] Codex `auth.json` files remain untouched (Lam never writes to them)

### 9. Auth Mode Detection Priority
Test priority levels:
1. [ ] Create Lam PAT metadata for account "a"
2. [ ] Account shows "PAT" (from Lam metadata = priority 1)
3. [ ] Delete Lam metadata file
4. [ ] If `~/.codex-a/auth.json` contains `"personal_access_token"`, shows "PAT" (priority 2)
5. [ ] If `~/.codex-a/auth.json` contains `"token"`, shows "OAuth" (priority 2)
6. [ ] Otherwise falls back to config.toml detection (priority 3)

### 10. Edge Cases
- [ ] Account with no authMode: no badge shown
- [ ] Account with authMode but no token expiration: PAT badge only, no expiration badge
- [ ] Network error during checkProfileTokenExpiration: no expiration badge (error silently ignored)
- [ ] Upload with expired date (past): shows "Token expired" badge immediately

---

## Known Limitations (By Design)

✅ **Not bugs - intentional design:**
- Frontend UI only (backend already complete from Plan 001)
- No automatic token refresh (users must re-upload when expired)
- Main account target only (switches always copy to `~/.codex/`)
- Manual cleanup of old backups (no auto-cleanup)
- No form fields for credentials (raw JSON only for now)

---

## Success Criteria

All manual verification items checked → **Feature ready for production**

---

## Rollback Plan

If critical issues found:
1. `git checkout main` (or original branch)
2. Report issues with specific steps that failed
3. Create follow-up fix plan

If minor issues found:
1. Document in GitHub issues
2. Mark as "enhancement" for future iteration
3. Ship current version if core functionality works
