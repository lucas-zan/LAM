# Plan 004 v2 - Patch: Fix Add Account Modal

**Issue:** Add Account modal has OAuth/PAT tabs, but both modes might create OAuth directories.

**Root Cause:** The UI shows PAT tab correctly, but we need to verify the flow is complete.

## Current Implementation Review

Looking at the executor's report, the implementation includes:

**Frontend (App.tsx):**
- ✅ Create mode tabs (OAuth/PAT)
- ✅ PAT tab with JSON textarea
- ✅ `handleAddPatAccount()` handler
- ✅ Form submission calls `api.addPatAccount()`

**Expected Flow:**
```
User selects PAT tab
  ↓
Pastes credentials JSON
  ↓
Submits form
  ↓
Calls api.addPatAccount() (NOT executeCreateAccount)
  ↓
Backend: add_pat_account() creates auth-{id}.json in Lam storage
  ↓
No .codex-{id} directory created ✓
```

## Verification Needed

Let me check if the implementation is actually correct by reviewing the code:

**Check 1: PAT form submission handler**
- Does PAT tab call `api.addPatAccount()`? 
- Or does it incorrectly call `api.executeCreateAccount()`?

**Check 2: OAuth form submission handler**
- OAuth tab should call `api.executeCreateAccount()`
- This creates `.codex-{id}` directory (correct for OAuth)

## Action Plan

1. Review App.tsx lines 597-700 (PAT form section)
2. Verify form onSubmit handler
3. If wrong: Patch to use correct API call
4. If correct: Document that it works as designed

**Most likely:** Implementation is correct, just needs verification.

**If broken:** Simple fix - change form onSubmit to call `api.addPatAccount()` instead of `api.executeCreateAccount()`.

## Manual Test to Verify

1. Click "Add Account"
2. Select "PAT (Upload Credentials)" tab  
3. Submit with test JSON
4. Check if `.codex-test-pat-001` directory was created:
   ```bash
   ls -la ~/ | grep codex-test-pat-001
   ```
5. Expected: No directory (only Lam storage files)
6. If directory exists: Bug confirmed, needs patch

Let me check the actual code now...
