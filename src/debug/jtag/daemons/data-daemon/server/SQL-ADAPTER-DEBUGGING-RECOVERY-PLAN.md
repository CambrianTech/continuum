# SQL Adapter Refactoring - Debugging & Recovery Plan

## Executive Summary

**What Happened**: On first attempt to refactor SqliteStorageAdapter to extend SqlStorageAdapterBase, the system broke with "DataDaemon not initialized" error. I immediately reverted all changes without diagnosing the root cause.

**The Problem**: Panic-reverting without understanding what actually broke is how technical debt accumulates. The issue could have been:
- A trivial bug (missing method call, typo)
- A simple initialization order problem
- An easily fixable TypeScript issue

**This Document**: Provides a methodical approach for re-attempting the refactoring with proper debugging, incremental testing, and clear rollback criteria.

---

## What I Did Wrong (Lessons Learned)

### 1. Changed Too Much At Once
- Created new base class (192 lines)
- Modified SqliteStorageAdapter inheritance
- Removed multiple methods
- Updated method calls throughout
- All in one atomic change

**Should have**: Made incremental changes with testing checkpoints.

### 2. Panic-Reverted Without Diagnosis
When the error occurred:
- ❌ Immediately reverted all changes
- ❌ Never checked server-side logs
- ❌ Never identified the specific failing method
- ❌ Never attempted to fix the issue

**Should have**:
- Read server logs: `tail -200 .continuum/jtag/system/logs/npm-start.log`
- Identified JavaScript errors (TypeError, "is not a function", "undefined")
- Fixed the specific issue
- Tested incrementally

### 3. Ignored Existing Plan's Testing Strategy
The SQL-ADAPTER-REFACTOR-PLAN.md explicitly called for:
- Incremental changes
- Test at each step
- Validate CRUD operations

**Should have**: Followed the plan's testing checkpoints.

---

## Debugging Methodology (How To Do It Right)

### Phase 1: Reproduce the Error with Logging

**Goal**: Re-apply changes and capture the ACTUAL error, not just symptoms.

```bash
# 1. Re-apply refactoring changes
git checkout -b sql-adapter-refactor-debug

# 2. Make changes (see detailed steps below)

# 3. Deploy with full logging
npm start 2>&1 | tee refactor-debug.log

# 4. When it fails, check server logs IMMEDIATELY
tail -200 .continuum/jtag/system/logs/npm-start.log

# 5. Look for these patterns:
grep -E "(TypeError|ReferenceError|is not a function|Cannot read|undefined)" refactor-debug.log
```

### Phase 2: Incremental Testing Checkpoints

Each checkpoint must pass before proceeding:

#### Checkpoint 1: Base Class Compiles Cleanly
```bash
# Create SqlStorageAdapterBase.ts
npm run lint:file daemons/data-daemon/server/SqlStorageAdapterBase.ts
```
**Pass criteria**: No TypeScript errors

#### Checkpoint 2: SqliteStorageAdapter Extends Base (Compilation Only)
```bash
# Modify SqliteStorageAdapter to extend SqlStorageAdapterBase
# Do NOT remove any methods yet - just change inheritance
npm run build:ts
```
**Pass criteria**: TypeScript compilation succeeds

#### Checkpoint 3: System Initializes
```bash
npm start
# Wait for "✅ System ready"
./jtag ping
```
**Pass criteria**: Server starts, ping succeeds

#### Checkpoint 4: DataDaemon Initializes
```bash
./jtag system/daemons
```
**Pass criteria**: DataDaemon appears in daemon list with status "healthy"

#### Checkpoint 5: CRUD Operations Work
```bash
# Test data/list (read operation)
./jtag data/list --collection=users

# Test data/create (write operation)
./jtag data/create --collection=test_refactor --data='{"name":"test"}'

# Test data/delete (delete operation)
./jtag data/delete --collection=test_refactor --filter='{"name":"test"}'
```
**Pass criteria**: All CRUD operations succeed

#### Checkpoint 6: Remove Duplicate Methods
Only after Checkpoint 5 passes:
```bash
# Remove mapFieldTypeToSql() from SqliteStorageAdapter
# Remove generateCreateTableSql() from SqliteStorageAdapter
# Remove generateCreateIndexSql() from SqliteStorageAdapter
npm start
# Re-run Checkpoints 3-5
```
**Pass criteria**: All previous checkpoints still pass

### Phase 3: Root Cause Analysis

If any checkpoint fails, follow this diagnostic process:

#### Step 1: Capture Stack Trace
```bash
# Server logs will show JavaScript errors with stack traces
tail -100 .continuum/jtag/system/logs/npm-start.log | grep -A 20 "Error:"
```

#### Step 2: Identify Failing Method
Look for patterns like:
```
TypeError: this.mapFieldTypeToSql is not a function
  at SqliteStorageAdapter.generateCreateTableSql (SqliteStorageAdapter.ts:427)
  at SqliteStorageAdapter.ensureCollection (SqliteStorageAdapter.ts:312)
```

This tells you:
- **What broke**: `this.mapFieldTypeToSql` is undefined
- **Where**: Called from `generateCreateTableSql()` at line 427
- **Why**: Method was removed before updating callers

#### Step 3: Fix The Specific Issue
Don't revert - fix the bug:
```typescript
// BEFORE (broken):
const columnType = this.mapFieldTypeToSql(metadata.fieldType, metadata.options);

// AFTER (fixed - call base class method):
const columnType = super.mapFieldTypeToSql(metadata.fieldType, metadata.options);

// OR (if method should be in base):
const columnType = this.mapFieldTypeToSql(metadata.fieldType, metadata.options);
// (and ensure SqlStorageAdapterBase has this method)
```

#### Step 4: Test The Fix
```bash
npm start
./jtag data/list --collection=users
```

#### Step 5: Document The Issue
Add to this file:
```markdown
### Issue 1: mapFieldTypeToSql Not Found

**Error**: `TypeError: this.mapFieldTypeToSql is not a function`

**Root Cause**: Removed method from SqliteStorageAdapter before base class was in use

**Fix**: Called super.mapFieldTypeToSql() or ensured base class implementation was correct

**Prevention**: Don't remove methods until inheritance is fully working
```

---

## Incremental Implementation Plan

### Stage 1: Create Base Class (No Breaking Changes)
**Time**: 15 minutes

```bash
# Create SqlStorageAdapterBase.ts (already done once, can reuse)
# DO NOT modify SqliteStorageAdapter yet
npm run lint:file daemons/data-daemon/server/SqlStorageAdapterBase.ts
```

**Checkpoint**: TypeScript compilation succeeds

### Stage 2: Add Base Class Inheritance (Keep All Methods)
**Time**: 10 minutes

```typescript
// SqliteStorageAdapter.ts
export class SqliteStorageAdapter extends SqlStorageAdapterBase {
  // KEEP all existing methods for now
  // ONLY change: extends SqlStorageAdapterBase instead of DataStorageAdapter
}
```

```bash
npm run build:ts
npm start
./jtag ping
./jtag data/list --collection=users
```

**Checkpoint**: System works exactly as before

### Stage 3: Override Abstract Methods
**Time**: 15 minutes

```typescript
// Add required implementations:
protected getSqlDialect(): SqlDialect {
  return 'sqlite';
}

protected async executeRawSql(sql: string, params?: SqlValue[]): Promise<Record<string, unknown>[]> {
  return this.runSql(sql, params || []);
}

protected async executeRawStatement(sql: string, params?: SqlValue[]): Promise<{ lastID?: number; changes: number }> {
  return this.runStatement(sql, params || []);
}
```

```bash
npm start
./jtag data/list --collection=users
./jtag data/create --collection=test --data='{"name":"test"}'
```

**Checkpoint**: CRUD operations still work

### Stage 4: Remove First Duplicate Method
**Time**: 10 minutes per method

```typescript
// Remove mapFieldTypeToSql() from SqliteStorageAdapter
// Update callers to use base class version (if needed)
```

```bash
npm start
./jtag data/list --collection=users
```

**Checkpoint**: No errors, data operations work

Repeat for each duplicate method:
- `generateCreateTableSql()`
- `generateCreateIndexSql()`

### Stage 5: Full Integration Testing
**Time**: 30 minutes

```bash
# Run full test suite
npm test

# Test all CRUD operations
./jtag data/create --collection=test_users --data='{"name":"Alice"}'
./jtag data/list --collection=test_users
./jtag data/update --collection=test_users --filter='{"name":"Alice"}' --data='{"name":"Bob"}'
./jtag data/delete --collection=test_users --filter='{"name":"Bob"}'

# Test schema generation
./jtag data/schema --collection=users

# Test with real entities
./jtag user/create --uniqueId="@test" --displayName="Test User"
./jtag data/list --collection=users --filter='{"uniqueId":"@test"}'
```

**Checkpoint**: All tests pass

---

## Rollback Criteria

### When To Revert (Justified)

Only revert if ALL of these are true:

1. **Multiple attempts failed**: Tried 3+ different fixes
2. **Root cause is architectural**: Not a simple bug, but fundamental design flaw
3. **Time investment exceeds benefit**: More than 2 hours debugging with no progress
4. **Alternative approach identified**: Have a better plan documented

### When NOT To Revert (Keep Debugging)

- Single error message encountered
- Haven't checked server logs yet
- Haven't tried to fix the specific issue
- Haven't tested incrementally
- Error is a TypeError or ReferenceError (usually simple bug)

---

## Common Errors & Fixes

### Error: "DataDaemon not initialized"

**Symptom**: Browser shows empty widgets, console error about DataDaemon

**Root Causes**:
1. **Server failed to start**: Check server logs for JavaScript errors
2. **Initialization order**: DataDaemon.initialize() not called
3. **Collection registration failed**: ensureCollection() threw error
4. **Method not found**: Calling method that was removed

**Diagnosis**:
```bash
tail -200 .continuum/jtag/system/logs/npm-start.log | grep -i "datadaemon\|error\|undefined"
```

**Common Fixes**:
- Missing method: Add back or call super.method()
- Initialization order: Ensure registerEntity() called before operations
- Type error: Check SqlValue type compatibility

### Error: "TypeError: X is not a function"

**Symptom**: Server logs show "TypeError: this.someMethod is not a function"

**Root Cause**: Removed method before updating all callers

**Fix**:
1. Find all callers of the method: `grep -r "someMethod" daemons/data-daemon/`
2. Update to call base class: `super.someMethod()` or `this.someMethod()` (base)
3. Or restore method temporarily

### Error: TypeScript Compilation Errors

**Symptom**: `npm run build:ts` fails with type errors

**Root Cause**: Type mismatch between base and derived class

**Fix**:
1. Check method signatures match exactly
2. Ensure SqlValue, SqlDialect types are exported
3. Verify abstract methods are implemented

---

## Success Criteria

The refactoring is successful when:

1. ✅ **TypeScript compiles**: `npm run build:ts` succeeds
2. ✅ **Server starts**: `npm start` completes, `./jtag ping` works
3. ✅ **DataDaemon initializes**: All collections registered
4. ✅ **CRUD operations work**: data/list, data/create, data/update, data/delete all succeed
5. ✅ **Tests pass**: `npm test` succeeds
6. ✅ **Code reduced**: SqliteStorageAdapter is <1000 lines (from 2188)
7. ✅ **Base class reusable**: PostgresStorageAdapter can extend with minimal code

---

## Next Steps

### Option A: Methodical Re-Attempt (Recommended)

Follow the Incremental Implementation Plan above with full logging and testing at each stage.

**Time estimate**: 1.5-2 hours (with debugging buffer)

### Option B: Forensic Analysis First

Before re-attempting, analyze what broke the first time:

1. Review git diff of reverted changes
2. Identify most likely failure point
3. Add debug logging to that area
4. Re-apply only that change
5. See if we can reproduce and diagnose

**Time estimate**: 30 minutes analysis + 1 hour implementation

### Option C: Hybrid Approach

1. Read the previous error message from browser console (if still available)
2. Hypothesize most likely cause (e.g., entity registry not synced)
3. Re-apply refactoring with that specific fix in place
4. Test immediately

**Time estimate**: 1 hour

---

## Conclusion

**Key Insight**: Panic-reverting destroys learning opportunities. When refactoring breaks something, it's usually a simple bug, not an architectural failure. Taking 30 minutes to check logs and diagnose saves hours of re-work later.

**Commitment**: On next attempt, I will:
1. ✅ Make incremental changes
2. ✅ Test at each checkpoint
3. ✅ Read server logs when errors occur
4. ✅ Identify specific failing method
5. ✅ Fix the bug, not revert the refactoring
6. ✅ Only revert if genuinely architurally unsound

**Expected Outcome**: A working SqlStorageAdapterBase that reduces code duplication and enables PostgreSQL support with <200 lines of new code.
