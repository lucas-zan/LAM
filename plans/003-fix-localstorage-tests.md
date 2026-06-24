# Plan 003: Fix localStorage Test Failures

> **For agentic workers:** REQUIRED: Use `executing-plans` to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Drift check**: `git diff --stat HEAD -- apps/desktop/src/stores/app.ts apps/desktop/vitest.config.ts`

## Status

- **Priority**: P2 (test maintenance)
- **Effort**: XS (30 minutes, ~8 steps)
- **Risk**: VERY LOW (test setup only, no production code changes)
- **Depends on**: none
- **Category**: bugfix (tests)
- **Planned at**: commit `25a1cd1`, 2026-06-24

## Goal

Fix 5 failing tests caused by missing `localStorage` mock in Vitest environment. Add global localStorage mock to vitest setup and remove manual `localStorage.clear()` from tests.

## Architecture

**Test environment fix** — no production code changes. Two changes:

1. **Global localStorage mock**: Add to Vitest setup file
2. **Remove localStorage.clear()**: Tests should use `vi.clearAllMocks()` instead

**Tech Stack**: Vitest, jsdom environment, vi.stubGlobal

---

## Current State

### Test Failures

From test output:
```
TypeError: localStorage.getItem is not a function
 ❯ src/stores/app.ts:42:34 (during store initialization)

TypeError: localStorage.clear is not a function  
 ❯ src/components/tray-quota-panel.test.tsx:113:16
```

**Affected tests (5 total)**:
- `App.handoff.test.tsx` - fails during store init
- `stores/accounts.test.ts` - fails during store init
- `stores/app.test.ts` - fails during store init
- `stores/quota.test.ts` - fails during store init
- `components/tray-quota-panel.test.tsx` (3 tests) - fails on `localStorage.clear()`

### Current Vitest Config

From `apps/desktop/vitest.config.ts`, there may not be a setup file configured yet.

### Store Code Using localStorage

From `src/stores/app.ts:42`:
```typescript
themeMode: (() => {
  const saved = localStorage.getItem('lam-theme');
  if (saved === 'system' || saved === 'light' || saved === 'dark') return saved;
  return 'system' as ThemeMode;
})(),
```

---

## Scope

**In scope** (files you WILL modify):
- `apps/desktop/vitest.config.ts` - add setup file reference
- `apps/desktop/vitest.setup.ts` - NEW file with localStorage mock
- `apps/desktop/src/components/tray-quota-panel.test.tsx` - remove localStorage.clear()

**Out of scope** (DO NOT touch):
- Production code in `src/stores/app.ts` (works correctly)
- Other test files (they work once setup is fixed)

---

## Git Workflow

- Branch: `advisor/003-fix-localstorage-tests`
- Commit style: `test(fix):` or `fix(test):`
- Single commit for the fix

---

## Implementation Steps

### Task 1: Create Global localStorage Mock

- [ ] **Step 1.1: Create vitest setup file**

**File**: `apps/desktop/vitest.setup.ts` (NEW)

Create with content:

```typescript
import { beforeEach, vi } from 'vitest';

// Mock localStorage globally for all tests
const localStorageMock = {
  store: {} as Record<string, string>,
  getItem(key: string) {
    return this.store[key] || null;
  },
  setItem(key: string, value: string) {
    this.store[key] = value;
  },
  removeItem(key: string) {
    delete this.store[key];
  },
  clear() {
    this.store = {};
  },
  get length() {
    return Object.keys(this.store).length;
  },
  key(index: number) {
    const keys = Object.keys(this.store);
    return keys[index] || null;
  },
};

// Install mock before each test
beforeEach(() => {
  localStorageMock.clear();
  vi.stubGlobal('localStorage', localStorageMock);
});
```

- [ ] **Step 1.2: Configure Vitest to use setup file**

**File**: `apps/desktop/vitest.config.ts`

Check if `setupFiles` exists. If the file looks like:

```typescript
export default defineConfig({
  test: {
    environment: 'jsdom',
    // ... other config
  },
});
```

Add `setupFiles` to the `test` object:

```typescript
export default defineConfig({
  test: {
    environment: 'jsdom',
    setupFiles: ['./vitest.setup.ts'],
    // ... other existing config
  },
});
```

If `setupFiles` already exists as an array, append to it:

```typescript
setupFiles: ['./existing-setup.ts', './vitest.setup.ts'],
```

- [ ] **Step 1.3: Run tests to verify mock works**

```bash
cd apps/desktop && npm test -- stores/app.test.ts
```

Expected: Test should pass (no more localStorage.getItem errors)

---

### Task 2: Fix localStorage.clear() Usage

- [ ] **Step 2.1: Remove localStorage.clear() from test**

**File**: `apps/desktop/src/components/tray-quota-panel.test.tsx`

**Location**: Line 113

**Remove this line:**
```typescript
  localStorage.clear();
```

The global mock's `beforeEach` already clears localStorage, so this is redundant and causes errors.

- [ ] **Step 2.2: Run tray-quota-panel tests**

```bash
cd apps/desktop && npm test -- tray-quota-panel.test.tsx
```

Expected: All tests in this file should pass

---

### Task 3: Verify All Tests

- [ ] **Step 3.1: Run full test suite**

```bash
cd apps/desktop && npm test
```

Expected: All 23 tests pass, 0 failures

- [ ] **Step 3.2: Verify no new failures**

Check that the count is:
- Test Files: 9 passed
- Tests: 23 passed (or more if other tests were added)

- [ ] **Step 3.3: Commit the fix**

```bash
git add apps/desktop/vitest.setup.ts apps/desktop/vitest.config.ts apps/desktop/src/components/tray-quota-panel.test.tsx
git commit -m "fix(test): add global localStorage mock for Vitest"
```

---

## Done Criteria

ALL must hold:

- [ ] `npm test` exits 0 with all tests passing
- [ ] No `localStorage.getItem is not a function` errors
- [ ] No `localStorage.clear is not a function` errors
- [ ] `vitest.setup.ts` exists with localStorage mock
- [ ] `vitest.config.ts` references setup file
- [ ] `tray-quota-panel.test.tsx` no longer calls `localStorage.clear()`

---

## STOP Conditions

Stop and report if:

1. **Vitest config structure unexpected**
   - If config file uses completely different structure
   - STOP and show the actual structure

2. **Setup file conflicts**
   - If another setup file already mocks localStorage differently
   - STOP and report the conflict

3. **Test failures unrelated to localStorage**
   - If tests still fail after mock is added
   - STOP and show the error

**If stopped mid-execution**:
1. Commit WIP: `git commit -m "WIP: localStorage mock - [issue]"`
2. Document what worked and what didn't

---

## Maintenance Notes

**For future developers:**

1. **localStorage mock is global** - Available in all tests automatically via vitest.setup.ts

2. **No manual clear needed** - The `beforeEach` in setup file clears localStorage before each test

3. **Adding setup files** - To add another setup file, append to the `setupFiles` array in vitest.config.ts

4. **Testing localStorage** - To test localStorage behavior:
   ```typescript
   it('should save to localStorage', () => {
     localStorage.setItem('key', 'value');
     expect(localStorage.getItem('key')).toBe('value');
   });
   ```

5. **Mock is isolated** - Each test gets a fresh empty localStorage

**For PR reviewers:**

- Verify vitest.setup.ts includes all localStorage methods
- Check that setupFiles is an array (supports multiple setup files)
- Confirm no test manually creates localStorage mock
- Verify all 5 previously failing tests now pass
