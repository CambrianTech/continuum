# Command Architecture Violations - Complete Audit

**Date**: 2025-11-26
**Scope**: All 125 server command files analyzed

## Summary

| Violation Type | Count | Severity |
|---------------|-------|----------|
| Direct DataDaemon access (non-data commands) | 7 (was 10) | 🔴 HIGH |
| Direct FS operations | 27 | 🟡 MEDIUM |
| Missing file delegation | 27 | 🟡 MEDIUM |
| Environment-specific code in shared/ | 0 | ✅ NONE |

## 🔴 HIGH PRIORITY: Direct DataDaemon Access

**Issue**: Commands directly accessing DataDaemon instead of delegating to `data/*` commands.
**Impact**: Violates separation of concerns, makes testing harder, bypasses data layer abstractions.

### Commands Violating This (10 total):

#### Chat Commands (0 - ALL FIXED)
1. ✅ **chat/export** - FIXED - Now uses `Commands.execute('data/list', ...)`
   - Replaced DataDaemon.query with proper command delegation
   - Uses entity collection constants (RoomEntity.collection, ChatMessageEntity.collection)
   - Still has direct FS operations (will fix when file/* commands created)

2. ✅ **chat/poll** - DELETED - Redundant with chat/export --afterMessageId

3. ✅ **chat/send** - FIXED - Now uses `Commands.execute('data/create', ...)` and `Commands.execute('data/list', ...)`
   - All DataDaemon access replaced with proper command delegation
   - Uses entity collection constants throughout

#### AI Commands (3)
4. **ai/generate** - Uses `DataDaemon` for conversation history
   - Should use: `Commands.execute('data/list', ...)`

5. **ai/status** - Uses `DataDaemon` for user queries
   - Should use: `Commands.execute('data/list', ...)`

6. **ai/thoughtstream** - Uses `DataDaemon` for message queries
   - Should use: `Commands.execute('data/list', ...)`

#### Task Commands (3)
7. **task/create** - Uses `DataDaemon.create`
   - Should use: `Commands.execute('data/create', ...)`

8. **task/list** - Uses `DataDaemon.queryOpen`
   - Should use: `Commands.execute('data/list', ...)`

9. **task/complete** - Uses `DataDaemon.update`
   - Should use: `Commands.execute('data/update', ...)`

#### Other (1)
10. **session/get-user** - Uses `DataDaemon.read`
    - Should use: `Commands.execute('data/read', ...)`

### ✅ EXCEPTION: data/* commands

These commands SHOULD use DataDaemon directly (they ARE the abstraction layer):
- data/create, data/read, data/update, data/delete
- data/list, data/query-open, data/query-next, data/query-close
- data/vector-search, data/generate-embedding, data/backfill-vectors
- data/clear, data/truncate

## 🟡 MEDIUM PRIORITY: Direct File System Operations

**Issue**: Commands using `fs.*` directly instead of delegating to `file/*` commands.
**Impact**: Makes testing harder, violates single responsibility, can't be sandboxed.

### Commands with FS Access (27 total):

#### Should Delegate to file/* Commands (20):

**AI Commands**:
1. `ai/report` - Writes report files with `fs.writeFileSync`
2. `ai/report/decisions` - Writes decision reports
3. `ai/thoughtstream` - Dynamic `import('fs')` for file operations
4. `ai/dataset/create` - Uses `fs.promises` for dataset files
5. `ai/dataset/list` - Uses `fs.promises` to list datasets

**Chat Commands**:
6. `chat/export` - Uses `fs.writeFileSync` and `fs.mkdirSync`
   - **Fix**: Use `Commands.execute('file/save', ...)`

**Code Commands**:
7. `code/pattern-search` - Direct `fs.*` for code search
8. `code/read` - Direct `fs.*` for reading code files

**Media Commands**:
9. `media/process` - Image processing with FS
10. `media/resize` - Image resizing with FS

**Training Commands**:
11. `training/import` - LoRA import with FS

**Genome Commands**:
12. `genome/job-create` - Job file creation

**Schema Commands**:
13. `schema/generate` - Schema file generation

**Security Commands**:
14. `security/setup` - Security file setup

**Theme Commands**:
15. `theme/list` - Lists theme files from FS

**Recipe Commands**:
16. `recipe/load` - Loads recipe files

**System Commands**:
17. `list` - Directory listing (should use tree command?)
18. `process-registry` - Registry file access

**RAG Commands**:
19. `rag/load` - RAG index file loading

**Ping Command**:
20. `ping` - Dynamic `import('fs')` for package.json reading

#### Legitimate FS Usage (7):

These commands are LOW-LEVEL file operations, so direct FS is acceptable:

21. `screenshot` - Image capture (needs direct FS for performance)
22. `exec/test` - Test file
23-27. `screenshot/test/*` - Test validators (5 files)

## Recommendations

### Phase 1: Fix High Priority (This Week)

**Commands to Refactor (10 commands)**:

```typescript
// BEFORE (chat/export)
const handle = await DataDaemon.queryOpen(collection, filter, orderBy, limit);
while (true) {
  const result = await DataDaemon.queryNext(handle, pageSize);
  // ...
}
await DataDaemon.queryClose(handle);

// AFTER
const result = await Commands.execute('data/list', {
  collection,
  filter,
  orderBy,
  limit
});
const messages = result.items;
```

### Phase 2: Fix Medium Priority (Next 2 Weeks)

**File Operations Standardization (20 commands)**:

```typescript
// BEFORE (chat/export)
import * as fs from 'fs';
fs.mkdirSync(dir, { recursive: true });
fs.writeFileSync(filepath, content, 'utf-8');

// AFTER
await Commands.execute('file/save', {
  path: filepath,
  content,
  encoding: 'utf-8',
  createDirectories: true
});
```

**Note**: Need to ensure `file/save` command exists and supports all required operations.

### Phase 3: Create Missing Abstractions

**File Commands Needed**:
- `file/save` - Write file with directory creation
- `file/load` - Read file with error handling
- `file/exists` - Check file existence
- `file/list` - List directory contents
- `file/delete` - Remove files
- `file/mkdir` - Create directories

### Implementation Strategy

1. **Week 1**: Fix chat/* commands (export, poll, send)
   - High visibility, frequently used
   - Good test cases for the pattern

2. **Week 2**: Fix task/* and ai/* commands
   - Critical for PersonaUser functionality
   - Well-defined interfaces

3. **Week 3**: Create file/* command suite
   - Build comprehensive file abstraction
   - Migrate low-hanging fruit (chat/export, etc.)

4. **Week 4**: Migrate remaining commands
   - ai/report, training/import, etc.
   - Add comprehensive tests

## Testing Strategy

### For Each Refactored Command:

1. **Unit Tests**: Mock `Commands.execute` responses
2. **Integration Tests**: Use real data/* commands
3. **Regression Tests**: Ensure same behavior as before

### Example Test Pattern:

```typescript
describe('ChatExportServerCommand', () => {
  it('should use data/list instead of DataDaemon', async () => {
    const executespy = jest.spyOn(Commands, 'execute');

    await command.execute({ roomId: 'test-room' });

    expect(executeSpy).toHaveBeenCalledWith('data/list', expect.objectContaining({
      collection: 'chat_messages',
      filter: { roomId: 'test-room' }
    }));
  });
});
```

## Migration Checklist

### Per Command:

- [ ] Identify all `DataDaemon.*` calls
- [ ] Replace with `Commands.execute('data/*', ...)`
- [ ] Update types (DataListParams, DataListResult, etc.)
- [ ] Add error handling for command failures
- [ ] Write unit tests mocking Commands.execute
- [ ] Write integration tests with real data commands
- [ ] Update command documentation
- [ ] Test in development environment
- [ ] Deploy and monitor for issues

## Benefits of Refactoring

### Immediate:
- **Better Testing**: Mock at command boundary, not daemon level
- **Type Safety**: Use well-defined command interfaces
- **Consistency**: All data access through same pattern

### Long-term:
- **Easier Refactoring**: Change data layer without touching 50 commands
- **Better Monitoring**: Track data access patterns at command level
- **Sandboxing**: File operations can be sandboxed/validated
- **API Generation**: Auto-generate REST/GraphQL from commands

## Violations by Category

### By Command Category:
- **chat**: 3 violations (export, poll, send)
- **ai**: 6 violations (generate, status, thoughtstream, dataset create/list, report)
- **task**: 3 violations (create, list, complete)
- **file ops**: 20 commands with direct FS
- **data**: 0 violations (correct usage)

### By Severity:
- 🔴 **Critical** (10): Direct DAO bypass
- 🟡 **Medium** (20): Direct FS without delegation
- 🟢 **Low** (7): Legitimate low-level FS usage
- ✅ **Clean** (88): No violations

## Next Actions

1. [ ] Create refactoring task issues for each command
2. [ ] Prioritize by usage frequency and impact
3. [ ] Create `file/*` command suite
4. [ ] Start with chat/export as proof of concept
5. [ ] Document migration pattern in CLAUDE.md
6. [ ] Add pre-commit linter to catch violations

---

**Status**: Initial audit complete. Ready for Phase 1 implementation.
