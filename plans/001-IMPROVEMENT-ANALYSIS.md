# Plan 001 Improvement Analysis

## Executive Summary

The existing plan is comprehensive and well-structured but doesn't follow the writing-plans skill conventions that enable better executor experience. Key issues:

1. **Missing executor workflow header** - No reference to subagent-driven-development or executing-plans skills
2. **Step granularity too coarse** - Steps bundle multiple actions instead of 2-5 minute increments
3. **Missing TDD structure** - Doesn't follow test-first workflow with explicit verification loops
4. **Verification points unclear** - Commands listed in table but not integrated into step flow
5. **Code blocks lack context** - Large code insertions without clear "before" snapshots
6. **Missing commit points** - No guidance on when to commit work

## Detailed Issues

### 1. Header Format

**Current**: Generic status metadata
**Expected**: Executor workflow reference

```markdown
> **For agentic workers:** REQUIRED: Use executing-plans to implement this plan.
```

### 2. Step Structure

**Current Step 1** (lines 128-181):
- Adds 3 structs in one step
- Single verification point
- No commit guidance

**Should be 5 steps**:
- Write empty structs with type signatures
- Verify compilation
- Add first struct fields + serde attributes
- Verify compilation
- Commit

### 3. TDD Pattern Missing

Steps should follow:
1. Write failing test
2. Verify it fails
3. Write minimal implementation  
4. Verify it passes
5. Commit

**Current plan**: Tests come in Step 6, after all implementation complete.

### 4. File Path Precision

**Current**: "after the `normalize_note` function (currently near the end of the file, around line 674)"
**Should be**: Exact line ranges from actual file inspection

### 5. Code Context

Large code blocks (50+ lines) inserted without showing:
- What comes before
- What comes after
- How to locate insertion point

## Recommendations

### Priority 1: Add Executor Workflow Header

Add below the drift check section:

```markdown
> **For agentic workers:** REQUIRED: Use executing-plans (or subagent-driven-development if available) to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.
```

### Priority 2: Break Steps Into Bite-Sized Actions

Each step should be one action taking 2-5 minutes:

**Example refactor of Step 1:**

```markdown
### Step 1a: Add Struct Skeletons

**File**: `apps/desktop/src-tauri/src/services/account.rs`

- [ ] Locate `AccountNoteUpdate` struct (around line 95)
- [ ] Add three empty struct declarations after it
- [ ] Run `cargo check` to verify syntax
- [ ] Expected: Compilation warnings about unused structs (OK)

### Step 1b: Implement UploadedCredentials

- [ ] Add all fields to UploadedCredentials struct
- [ ] Add derives and serde attributes
- [ ] Run `cargo check`
- [ ] Expected: exit 0

### Step 1c: Commit Structures

```bash
git add apps/desktop/src-tauri/src/services/account.rs
git commit -m "feat(pat): add PAT metadata structures"
```
```

### Priority 3: Add TDD Structure for Functions

Reorder plan to write tests first:

```markdown
### Step 3a: Write Test for record_pat_metadata

**File**: `apps/desktop/src-tauri/src/services/account.rs`

- [ ] Add `#[cfg(test)] mod pat_tests` at end of file
- [ ] Add test_record_and_read_metadata test
- [ ] Run test: `cargo test --lib account::pat_tests::test_record_and_read_metadata`
- [ ] Expected: FAIL - function not found

### Step 3b: Implement record_pat_metadata

- [ ] Add function after normalize_note
- [ ] Run test again
- [ ] Expected: PASS
```

### Priority 4: Add Snapshot Context

For each code insertion, show:
- 5 lines before insertion point
- 5 lines after insertion point  
- Or use line-range notation: "Insert at line 674 (between `normalize_note` closing brace and next function)"

### Priority 5: Add Commit Checkpoints

After each logical unit (2-3 steps), add explicit commit step:

```markdown
- [ ] **Commit progress**
```bash
git add [files]
git commit -m "feat(pat): [description]"
```
```

## Proposed Restructure

### New Step Sequence

1. **Setup**: Branch creation, drift check
2. **Structures**: Add empty structs → verify → fill fields → verify → commit
3. **Storage helpers**: Add path functions → verify → commit
4. **Tests first**: Write metadata tests → verify fail
5. **Implement metadata**: record_pat_metadata → verify test passes → commit
6. **Tests first**: Write process_uploaded_credentials tests → verify fail
7. **Implement processing**: process_uploaded_credentials → verify passes → commit
8. **Tests first**: Write expiration check tests → verify fail
9. **Implement expiration**: check_token_expiration → verify passes → commit
10. **Auth detection**: Add helper → integrate → test → commit
11. **Commands**: Add command wrappers → register → verify → commit
12. **Integration test**: Write → run → commit
13. **Docs**: Update → commit
14. **Final verification**: Run all checks

This gives ~30-40 bite-sized steps instead of 8 large ones.

## Impact Assessment

**If NOT fixed**:
- Executors will need to chunk steps themselves
- Higher chance of missing verification points
- Harder to resume after interruption
- Can't track progress granularly

**If fixed**:
- Executor follows mechanical workflow
- Clear progress tracking (40% = 16/40 steps)
- Easy resume points
- Built-in verification at each step
- Follows Rust/TDD best practices

## Next Steps

1. User decides: Apply these improvements or execute as-is?
2. If apply: Rewrite plan with new structure (saves to new file)
3. If as-is: Execute with understanding plan needs chunking during execution
