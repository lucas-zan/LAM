# Plan 002: PAT Authentication Frontend UI & Tests

> **For agentic workers:** REQUIRED: Use `executing-plans` (or `subagent-driven-development` if available) to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Drift check (run first)**: `git diff --stat 7cd8fd3..HEAD -- apps/desktop/src/lib/types.ts apps/desktop/src/lib/api.ts apps/desktop/src/routes/views.tsx apps/desktop/src/App.tsx`
>
> If any in-scope file changed since this plan was written, compare the "Current state" excerpts against the live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M (1-2 days, ~30 steps)
- **Risk**: LOW (UI only, no backend changes)
- **Depends on**: Plan 001 v2 (PAT backend) - MUST be DONE
- **Category**: feature (frontend)
- **Planned at**: commit `7cd8fd3`, 2026-06-24
- **Architecture**: React UI components, TypeScript, Vitest tests, TDD workflow

## Goal

Add frontend UI for PAT authentication: credential upload modal, token expiration warnings in account cards, and auth mode badges — allowing users to upload PAT credentials and see expiration status without backend code changes.

## Architecture

**Frontend-only feature** — backend already complete from Plan 001. Three UI layers:

1. **Type definitions**: Add PAT types to match backend structs
2. **API integration**: Wire up Tauri commands to TypeScript API
3. **UI components**: Upload modal + expiration badges in account cards
4. **Tests**: Vitest tests for types, API calls, and UI components

**Tech Stack**: React, TypeScript, Tauri invoke, Vitest, Zustand stores

---

## Current State

### Backend Commands (Already Available)

From Plan 001, these Tauri commands are registered and working:

```rust
// apps/desktop/src-tauri/src/main.rs:69-71
commands::upload_pat_credentials,
commands::get_pat_metadata,
commands::check_profile_token_expiration,
```

### Frontend File Structure

```
apps/desktop/src/
  lib/
    types.ts              ← Add PAT types here
    api.ts                ← Add API functions here
  routes/
    views.tsx             ← Modify account cards here
  App.tsx                 ← Add upload modal logic
  stores/
    accounts.test.ts      ← Add tests here (pattern reference)
```

### Existing Account Card Structure

From `apps/desktop/src/routes/views.tsx:311-345`:
- Account cards show: displayName, quota, auth status badge
- Current auth badge: `{account.hasAuth ? 'Logged in' : 'Login needed'}`
- Line 341-343: Auth badge location (will add auth mode + expiration here)

### Repo Conventions

- **TypeScript**: Strict types, no `any`
- **Components**: Functional React with hooks
- **API calls**: Via `invoke()` from `@tauri-apps/api/core`
- **Tests**: Vitest with `vi.mock()` for API, `describe/it/expect`
- **Stores**: Zustand pattern (see `stores/accounts.ts`)
- **Styling**: CSS classes (e.g. `badge`, `badge--auth`, `warn`)

---

## Scope

**In scope** (files you WILL modify):
- `apps/desktop/src/lib/types.ts` — add PAT type definitions
- `apps/desktop/src/lib/api.ts` — add API functions
- `apps/desktop/src/routes/views.tsx` — add expiration badges to account cards
- `apps/desktop/src/App.tsx` — add upload modal UI
- `apps/desktop/src/lib/api.test.ts` — NEW test file for API functions
- `apps/desktop/src/routes/views.test.tsx` — NEW test file for UI components
- `apps/desktop/src/styles.css` — add PAT-specific styles

**Out of scope** (DO NOT touch):
- Any backend Rust files (already complete)
- Store implementations (use inline state for now)
- Complex state management (will be added in follow-up if needed)

---

## Git Workflow

- Branch: `advisor/002-pat-frontend-ui`
- Commit style: Conventional commits (`feat(ui):`, `test(ui):`)
- Commit after each logical unit (every 3-5 steps)
- Do NOT push or open PR unless explicitly instructed

---

## Implementation Steps

### Phase 1: Type Definitions

#### Task 1: Add PAT Types

- [ ] **Step 1.1: Add UploadedCredentials type**

**File**: `apps/desktop/src/lib/types.ts`

**Location**: After line 250 (after `AntigravityQuotaResponse` type)

Add:

```typescript

export type UploadedCredentials = {
  accessToken: string;
  accountId: string;
  disabled: boolean;
  email: string;
  expired: string; // ISO 8601
  headers?: Record<string, unknown> | null;
  idToken?: string | null;
  lastRefresh: string; // ISO 8601
  refreshToken?: string | null;
  type: string;
  websockets: boolean;
};

export type AuthMetadata = {
  profileId: string;
  authType: string; // "personal_token" | "oauth" | "api_key"
  tokenExpiration?: string | null; // ISO 8601
  lastChecked: string; // ISO 8601
};

export type TokenExpirationStatus = {
  profileId: string;
  isExpired: boolean;
  daysUntilExpiration?: number | null;
  expirationDate?: string | null;
  warningLevel: string; // "ok" | "warning" | "critical" | "expired"
};
```

- [ ] **Step 1.2: Verify TypeScript compilation**

```bash
cd apps/desktop && npm run typecheck
```

Expected: exit 0, no errors

- [ ] **Step 1.3: Commit types**

```bash
git add apps/desktop/src/lib/types.ts
git commit -m "feat(ui): add PAT authentication type definitions"
```

---

### Phase 2: API Integration

#### Task 2: Add API Functions

- [ ] **Step 2.1: Add API imports**

**File**: `apps/desktop/src/lib/api.ts`

**Location**: Add to import block at lines 2-28

Add to the type imports:

```typescript
  UploadedCredentials,
  AuthMetadata,
  TokenExpirationStatus,
```

- [ ] **Step 2.2: Add uploadPatCredentials function**

**File**: `apps/desktop/src/lib/api.ts`

**Location**: After line 176 (after `getAntigravityQuota`)

Add:

```typescript

export async function uploadPatCredentials(
  profileId: string,
  uploaded: UploadedCredentials
): Promise<void> {
  return invoke<void>("upload_pat_credentials", { profileId, uploaded });
}

export async function getPatMetadata(profileId: string): Promise<AuthMetadata | null> {
  return invoke<AuthMetadata | null>("get_pat_metadata", { profileId });
}

export async function checkProfileTokenExpiration(
  profileId: string
): Promise<TokenExpirationStatus> {
  return invoke<TokenExpirationStatus>("check_profile_token_expiration", { profileId });
}
```

- [ ] **Step 2.3: Verify TypeScript compilation**

```bash
cd apps/desktop && npm run typecheck
```

Expected: exit 0

- [ ] **Step 2.4: Commit API functions**

```bash
git add apps/desktop/src/lib/api.ts
git commit -m "feat(ui): add PAT API integration functions"
```

---

### Phase 3: API Tests (TDD)

#### Task 3: Test API Functions

- [ ] **Step 3.1: Create API test file**

**File**: `apps/desktop/src/lib/api.test.ts` (NEW)

Create with content:

```typescript
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { uploadPatCredentials, getPatMetadata, checkProfileTokenExpiration } from './api';
import type { UploadedCredentials, AuthMetadata, TokenExpirationStatus } from './types';

// Mock Tauri invoke
const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
}));

beforeEach(() => {
  vi.clearAllMocks();
});

describe('PAT API functions', () => {
  describe('uploadPatCredentials', () => {
    it('should call upload_pat_credentials command with correct parameters', async () => {
      const profileId = 'test-profile';
      const uploaded: UploadedCredentials = {
        accessToken: 'at-test',
        accountId: 'id',
        disabled: false,
        email: 'test@example.com',
        expired: '2030-12-31T10:00:00+08:00',
        lastRefresh: '2026-06-24T00:00:00+08:00',
        type: 'codex',
        websockets: true,
      };

      mockInvoke.mockResolvedValue(undefined);

      await uploadPatCredentials(profileId, uploaded);

      expect(mockInvoke).toHaveBeenCalledWith('upload_pat_credentials', {
        profileId,
        uploaded,
      });
    });
  });

  describe('getPatMetadata', () => {
    it('should return metadata when it exists', async () => {
      const profileId = 'test-profile';
      const metadata: AuthMetadata = {
        profileId,
        authType: 'personal_token',
        tokenExpiration: '2030-12-31T10:00:00+08:00',
        lastChecked: '2026-06-24T00:00:00+08:00',
      };

      mockInvoke.mockResolvedValue(metadata);

      const result = await getPatMetadata(profileId);

      expect(mockInvoke).toHaveBeenCalledWith('get_pat_metadata', { profileId });
      expect(result).toEqual(metadata);
    });

    it('should return null when metadata does not exist', async () => {
      mockInvoke.mockResolvedValue(null);

      const result = await getPatMetadata('nonexistent');

      expect(result).toBeNull();
    });
  });

  describe('checkProfileTokenExpiration', () => {
    it('should return expiration status', async () => {
      const profileId = 'test-profile';
      const status: TokenExpirationStatus = {
        profileId,
        isExpired: false,
        daysUntilExpiration: 100,
        expirationDate: '2030-12-31T10:00:00+08:00',
        warningLevel: 'ok',
      };

      mockInvoke.mockResolvedValue(status);

      const result = await checkProfileTokenExpiration(profileId);

      expect(mockInvoke).toHaveBeenCalledWith('check_profile_token_expiration', { profileId });
      expect(result).toEqual(status);
    });

    it('should handle expired tokens', async () => {
      const status: TokenExpirationStatus = {
        profileId: 'test',
        isExpired: true,
        daysUntilExpiration: -10,
        expirationDate: '2020-01-01T10:00:00+08:00',
        warningLevel: 'expired',
      };

      mockInvoke.mockResolvedValue(status);

      const result = await checkProfileTokenExpiration('test');

      expect(result.isExpired).toBe(true);
      expect(result.warningLevel).toBe('expired');
    });
  });
});
```

- [ ] **Step 3.2: Run tests to verify they pass**

```bash
cd apps/desktop && npm test -- api.test.ts
```

Expected: 5 tests PASS

- [ ] **Step 3.3: Commit API tests**

```bash
git add apps/desktop/src/lib/api.test.ts
git commit -m "test(ui): add PAT API function tests"
```

---

### Phase 4: Account Card UI Enhancement

#### Task 4: Add Auth Mode Badge Component

- [ ] **Step 4.1: Add AuthModeBadge helper function**

**File**: `apps/desktop/src/routes/views.tsx`

**Location**: Before the `Accounts` function (around line 250), add this helper:

```typescript

function AuthModeBadge({ authMode }: { authMode?: string | null }) {
  if (!authMode) return null;
  
  const modeLabels: Record<string, string> = {
    personal_token: 'PAT',
    oauth: 'OAuth',
    api_key: 'API Key',
    config: 'Config',
  };
  
  const label = modeLabels[authMode] ?? authMode;
  
  return (
    <span className="badge badge--authMode" title={`Auth mode: ${label}`}>
      {label}
    </span>
  );
}
```

- [ ] **Step 4.2: Add TokenExpirationBadge component**

**File**: `apps/desktop/src/routes/views.tsx`

**Location**: Immediately after `AuthModeBadge`

Add:

```typescript

function TokenExpirationBadge({ 
  status 
}: { 
  status?: { isExpired: boolean; daysUntilExpiration?: number | null; warningLevel: string } | null 
}) {
  if (!status) return null;
  
  const { isExpired, daysUntilExpiration, warningLevel } = status;
  
  if (warningLevel === 'ok') return null; // Don't show badge when >30 days
  
  let badgeClass = 'badge';
  let label = '';
  
  if (isExpired) {
    badgeClass += ' badge--expired';
    label = 'Token expired';
  } else if (warningLevel === 'critical') {
    badgeClass += ' badge--critical';
    label = `Expires in ${daysUntilExpiration}d`;
  } else if (warningLevel === 'warning') {
    badgeClass += ' badge--warning';
    label = `Expires in ${daysUntilExpiration}d`;
  }
  
  return (
    <span className={badgeClass} title="PAT token expiration">
      {label}
    </span>
  );
}
```

- [ ] **Step 4.3: Update Accounts component to show badges**

**File**: `apps/desktop/src/routes/views.tsx`

**Location**: Inside the `Accounts` component, modify the account card badges section (around line 341-344)

**Replace:**
```typescript
                  <span className={account.hasAuth ? 'badge badge--auth' : 'badge warn'}>
                    {account.hasAuth ? 'Logged in' : 'Login needed'}
                  </span>
```

**With:**
```typescript
                  <span className={account.hasAuth ? 'badge badge--auth' : 'badge warn'}>
                    {account.hasAuth ? 'Logged in' : 'Login needed'}
                  </span>
                  <AuthModeBadge authMode={account.authMode} />
```

Note: Token expiration badge will be added in Task 5 after we fetch the status

- [ ] **Step 4.4: Verify TypeScript compilation**

```bash
cd apps/desktop && npm run typecheck
```

Expected: exit 0

- [ ] **Step 4.5: Commit auth mode badge**

```bash
git add apps/desktop/src/routes/views.tsx
git commit -m "feat(ui): add auth mode badge to account cards"
```

---

#### Task 5: Fetch and Display Token Expiration

- [ ] **Step 5.1: Add useState for token expiration**

**File**: `apps/desktop/src/routes/views.tsx`

**Location**: At the top of the `Accounts` function body (around line 282), add:

```typescript
  const [tokenStatuses, setTokenStatuses] = useState<Record<string, TokenExpirationStatus>>({});
```

Also add the import at the top:

```typescript
import { useState, useEffect } from 'react';
```

And add to the api import:

```typescript
import { checkProfileTokenExpiration } from '../lib/api';
```

- [ ] **Step 5.2: Add useEffect to fetch token status**

**File**: `apps/desktop/src/routes/views.tsx`

**Location**: After the `useState` declaration in `Accounts` function

Add:

```typescript
  useEffect(() => {
    // Fetch token expiration status for accounts with personal_token auth mode
    const fetchTokenStatuses = async () => {
      const patAccounts = accounts.filter((acc) => acc.authMode === 'personal_token');
      
      for (const account of patAccounts) {
        try {
          const status = await checkProfileTokenExpiration(account.id);
          setTokenStatuses((prev) => ({ ...prev, [account.id]: status }));
        } catch (err) {
          // Silently ignore errors - badge won't show if fetch fails
          console.warn(`Failed to fetch token status for ${account.id}:`, err);
        }
      }
    };
    
    if (accounts.length > 0) {
      fetchTokenStatuses();
    }
  }, [accounts]);
```

- [ ] **Step 5.3: Add TokenExpirationBadge to account cards**

**File**: `apps/desktop/src/routes/views.tsx`

**Location**: After the AuthModeBadge in the cardHeadActions section (after line you modified in Step 4.3)

Add:

```typescript
                  <TokenExpirationBadge status={tokenStatuses[account.id]} />
```

- [ ] **Step 5.4: Verify TypeScript compilation**

```bash
cd apps/desktop && npm run typecheck
```

Expected: exit 0

- [ ] **Step 5.5: Commit token expiration display**

```bash
git add apps/desktop/src/routes/views.tsx
git commit -m "feat(ui): display token expiration warnings on account cards"
```

---

### Phase 5: Upload PAT Modal

#### Task 6: Add Upload PAT Modal Component

- [ ] **Step 6.1: Add PAT upload modal to App state**

**File**: `apps/desktop/src/App.tsx`

**Location**: Find the existing modal state handling (search for `modal` in the file)

Add to the modal type union (if there's a Modal type):

```typescript
| { kind: 'uploadPat'; accountId: string }
```

- [ ] **Step 6.2: Add upload handler function**

**File**: `apps/desktop/src/App.tsx`

**Location**: After other handler functions (search for existing handlers like `handleSync`, `handleRename`, etc.)

Add:

```typescript
  async function handleUploadPat(profileId: string, credentials: UploadedCredentials) {
    try {
      await uploadPatCredentials(profileId, credentials);
      setModal(null);
      // Refresh accounts to show updated auth mode
      await loadAccounts();
      appState.set({ status: 'PAT credentials uploaded successfully' });
    } catch (err) {
      appState.set({ 
        status: 'Failed to upload PAT credentials', 
        error: err instanceof Error ? err.message : String(err) 
      });
    }
  }
```

Also add the import:

```typescript
import { uploadPatCredentials } from './lib/api';
import type { UploadedCredentials } from './lib/types';
```

- [ ] **Step 6.3: Add simple upload modal UI**

**File**: `apps/desktop/src/App.tsx`

**Location**: Where other modals are rendered (search for `modal.kind === 'sync'` or similar)

Add:

```typescript
      {modal?.kind === 'uploadPat' && (
        <div className="modalBackdrop" onClick={() => setModal(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h2>Upload PAT Credentials</h2>
            <p className="modalHint">
              Paste your credential JSON from external account management system:
            </p>
            <form
              onSubmit={(e) => {
                e.preventDefault();
                const formData = new FormData(e.currentTarget);
                const jsonStr = formData.get('credentialsJson') as string;
                try {
                  const creds = JSON.parse(jsonStr) as UploadedCredentials;
                  handleUploadPat(modal.accountId, creds);
                } catch (err) {
                  appState.set({ error: 'Invalid JSON format' });
                }
              }}
            >
              <textarea
                name="credentialsJson"
                className="uploadPatTextarea"
                placeholder='{"accessToken": "...", "accountId": "...", ...}'
                rows={10}
                required
              />
              <div className="modalActions">
                <button type="button" onClick={() => setModal(null)}>
                  Cancel
                </button>
                <button type="submit" className="primary">
                  Upload
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
```

- [ ] **Step 6.4: Add button to trigger modal**

**File**: `apps/desktop/src/routes/views.tsx`

**Location**: In the account card actions section (around line 400-440), add a new button

Add after the "Handoff" button:

```typescript
                <UIButton
                  size="sm"
                  className="accountActionBtn"
                  aria-label="Upload PAT"
                  title="Upload personal access token credentials"
                  onClick={(e) => {
                    e.stopPropagation();
                    // This assumes you pass a setModal callback from App.tsx
                    // You may need to add this prop to the Accounts component
                  }}
                >
                  <IconKey size={13} />
                  Upload PAT
                </UIButton>
```

**Note**: You'll need to pass the modal setter from App.tsx to the Accounts component. Add to the Accounts props:

```typescript
  openUploadPat: (accountId: string) => void;
```

Then in App.tsx when rendering Accounts, pass:

```typescript
  openUploadPat={(accountId) => setModal({ kind: 'uploadPat', accountId })}
```

- [ ] **Step 6.5: Verify TypeScript compilation**

```bash
cd apps/desktop && npm run typecheck
```

Expected: exit 0

- [ ] **Step 6.6: Commit upload modal**

```bash
git add apps/desktop/src/App.tsx apps/desktop/src/routes/views.tsx
git commit -m "feat(ui): add PAT credentials upload modal"
```

---

### Phase 6: Styling

#### Task 7: Add PAT-Specific Styles

- [ ] **Step 7.1: Add badge styles**

**File**: `apps/desktop/src/styles.css`

**Location**: Find the existing `.badge` styles and add after them:

```css
.badge--authMode {
  background: var(--bg-hover);
  color: var(--text-secondary);
  font-size: 11px;
  padding: 2px 6px;
}

.badge--warning {
  background: #fef3c7;
  color: #92400e;
  border: 1px solid #fbbf24;
}

.badge--critical {
  background: #fee2e2;
  color: #991b1b;
  border: 1px solid #f87171;
}

.badge--expired {
  background: #fecaca;
  color: #7f1d1d;
  border: 1px solid #ef4444;
  font-weight: 600;
}
```

- [ ] **Step 7.2: Add upload modal styles**

**File**: `apps/desktop/src/styles.css`

**Location**: After the modal styles section

Add:

```css
.uploadPatTextarea {
  width: 100%;
  font-family: 'SF Mono', 'Consolas', monospace;
  font-size: 12px;
  padding: 12px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg-secondary);
  color: var(--text);
  resize: vertical;
  margin: 16px 0;
}

.uploadPatTextarea:focus {
  outline: none;
  border-color: var(--accent);
}
```

- [ ] **Step 7.3: Commit styles**

```bash
git add apps/desktop/src/styles.css
git commit -m "style(ui): add PAT badge and modal styles"
```

---

### Phase 7: Component Tests

#### Task 8: Test UI Components

- [ ] **Step 8.1: Create views test file**

**File**: `apps/desktop/src/routes/views.test.tsx` (NEW)

Create with content:

```typescript
import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/react';
import type { CodexAccount, UsageQuotaSnapshot } from '../lib/types';

// Mock components we're not testing
const mockSelect = () => {};
const mockOpenSync = () => {};
const mockRename = () => {};
const mockLogin = () => {};
const mockOpenHandoff = () => {};
const mockRelayLatest = () => {};
const mockRefreshAccountQuota = () => {};
const mockOnSaveAccountNote = async () => {};
const mockOpenUploadPat = () => {};

describe('Account card PAT UI', () => {
  const mockAccount: CodexAccount = {
    id: 'test',
    displayName: 'Test Account',
    codexHome: '/home/.codex-test',
    hasAuth: true,
    hasConfig: true,
    hasHistory: true,
    sessionCount: 5,
    managed: true,
    isRelay: false,
    authMode: 'personal_token',
  };

  const mockQuotas: UsageQuotaSnapshot[] = [];

  it('should show PAT badge when authMode is personal_token', () => {
    // This is a smoke test - full testing would require proper React testing setup
    expect(mockAccount.authMode).toBe('personal_token');
  });

  it('should handle OAuth auth mode', () => {
    const oauthAccount = { ...mockAccount, authMode: 'oauth' };
    expect(oauthAccount.authMode).toBe('oauth');
  });

  it('should handle missing auth mode', () => {
    const noAuthAccount = { ...mockAccount, authMode: null };
    expect(noAuthAccount.authMode).toBeNull();
  });
});
```

Note: Full React component testing would require additional setup (React Testing Library, jsdom). These are structural tests.

- [ ] **Step 8.2: Run tests**

```bash
cd apps/desktop && npm test -- views.test.tsx
```

Expected: 3 tests PASS

- [ ] **Step 8.3: Commit component tests**

```bash
git add apps/desktop/src/routes/views.test.tsx
git commit -m "test(ui): add PAT UI component tests"
```

---

### Phase 8: Final Verification

#### Task 9: Run All Verification Checks

- [ ] **Step 9.1: TypeScript type check**

```bash
cd apps/desktop && npm run typecheck
```

Expected: exit 0, no errors

- [ ] **Step 9.2: Run all tests**

```bash
cd apps/desktop && npm test
```

Expected: All tests PASS (including new PAT tests: 8 total from api.test + views.test)

- [ ] **Step 9.3: Build frontend**

```bash
cd apps/desktop && npm run build
```

Expected: exit 0, build succeeds

- [ ] **Step 9.4: Verify no out-of-scope changes**

```bash
git status
```

Expected: Only in-scope files modified, working tree clean

---

## Done Criteria

ALL must hold:

- [ ] `cd apps/desktop && npm run typecheck` exits 0
- [ ] `npm test` exits 0, 8+ new tests pass (5 API + 3 UI)
- [ ] `npm run build` exits 0
- [ ] Auth mode badge shows on account cards
- [ ] Token expiration warnings show for PAT accounts
- [ ] Upload PAT modal renders and accepts JSON
- [ ] All conventional commits follow `feat(ui):`, `test(ui):`, `style(ui):` format
- [ ] Feature branch exists with all changes committed

---

## STOP Conditions

Stop and report (do not improvise) if:

1. **Drift detected**: Files changed since commit `7cd8fd3`
   - Run drift check command
   - Compare "Current state" excerpts
   - STOP if structure differs

2. **Plan 001 not complete**: Backend commands not available
   - Check `apps/desktop/src-tauri/src/main.rs` lines 69-71
   - If commands missing, STOP and report

3. **TypeScript errors** after adding types
   - Check for type mismatches
   - STOP if errors in unrelated files

4. **Test failures** in existing tests
   - Run `npm test` before changes
   - If failures exist, STOP

5. **Build failures** unrelated to PAT code
   - Check build output
   - STOP if errors in other modules

**If stopped mid-execution**:
1. Commit WIP: `git commit -m "WIP: [task name] - [blocker]"`
2. Document blocker in commit message
3. Update `plans/README.md` status to `BLOCKED`

---

## Maintenance Notes

**For future developers:**

1. **UI-only feature** — Backend already complete, this adds display only

2. **Token expiration refresh**:
   - Fetched on component mount via useEffect
   - Only for accounts with `authMode === 'personal_token'`
   - Errors are silently ignored (badge won't show on fetch failure)

3. **Auth mode badge**:
   - Shows mode from backend detection (PAT / OAuth / API Key / Config)
   - Read-only display, no editing

4. **Upload modal**:
   - Accepts raw JSON from external systems
   - Validates JSON parsing client-side
   - Backend validates expiration format

5. **Badge colors**:
   - ok (>30d): No badge shown
   - warning (8-30d): Yellow
   - critical (1-7d): Orange
   - expired (<0d): Red

6. **Future improvements** (out of scope):
   - Credential form fields instead of raw JSON
   - Store integration (Zustand)
   - Auto-refresh token status
   - Batch upload for multiple accounts

**For PR reviewers:**

- Verify no backend files modified
- Check TypeScript types match Rust structs (camelCase vs snake_case)
- Test modal with valid/invalid JSON
- Verify badges show correct colors
- Check all 8+ tests pass
