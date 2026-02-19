# Command Architecture Audit

**Date**: 2025-11-26
**Purpose**: Analyze current command structure for patterns, anti-patterns, and improvement opportunities

## Structure Overview

**Total**: 548 directories, ~100 registered commands

**Standard Pattern**:
```
commands/
  └── category/
      └── action/
          ├── browser/          # Browser-specific logic (DOM, window APIs)
          ├── server/           # Server-specific logic (Node.js, FS)
          └── shared/           # Cross-platform business logic
              ├── CategoryActionTypes.ts
              ├── CategoryActionCommand.ts (abstract base)
```

## Current Categories

### Well-Organized
- **ai/**: 12 subcommands (model, rag, generate, status, etc.)
- **data/**: CRUD operations (create, read, update, delete, list, query-*)
- **chat/**: Message operations (send, export, poll, analyze)
- **genome/**: LoRA operations (paging-*, capture-*, batch-micro-tune)
- **task/**: Task management (create, list, complete)

### Single-Level (No Subcategories)
- **click**, **scroll**, **type**, **screenshot**, **navigate**: UI automation primitives
- **ping**: Health check
- **exec**: Shell execution
- **tree**: Directory listing

### Inconsistent Nesting
- **theme/**: Has shared/ at both `theme/shared/` AND `theme/get/shared/`
- **test/**: Mixed structure - `test/routing-chaos/` vs `test/run/suite/`
- **decision/**: No shared/ at category level (only in subcommands)

## Patterns Analysis

### ✅ GOOD Patterns

#### 1. Environment Separation
- **Shared** files have NO Node.js or DOM dependencies
- **Server** has FS access, crypto, path (chat/export, screenshot, etc.)
- **Browser** delegates to server via `remoteExecute()`

#### 2. Type Safety
- Commands use `CommandParams` and `CommandResult` (not JTAGPayload directly)
- Proper typing with UUID for IDs, not loose strings
- Example: `PingParams`, `PingResult` with structured interfaces

#### 3. Single Sources of Truth
- Collection names: `ChatMessageEntity.collection` (not magic strings)
- No hardcoded UUIDs or table names in logic

#### 4. Delegation Pattern
- Browser commands delegate to server:
  ```typescript
  protected async executeChatExport(params: ChatExportParams): Promise<ChatExportResult> {
    return await this.remoteExecute(params);
  }
  ```

#### 5. Command Discovery
- Auto-discovered from directory structure
- Generator creates schemas from Types files
- No manual registration required

### ⚠️ ANTI-PATTERNS Found

#### 1. Inconsistent Parameter Naming
- Some use `room` (string, name OR ID)
- Some use `roomId` (UUID)
- Some use BOTH but don't resolve properly

**Example**: `chat/analyze` initially took `room: string` but should use `roomId: UUID`

#### 2. Missing Result Helpers
- Some commands manually construct results (missing `context`, `sessionId`)
- Should ALWAYS use `this.createResult(params, data)`

**Bad**:
```typescript
return {
  success: true,
  roomId,
  totalMessages: 10,
  // Missing context/sessionId!
};
```

**Good**:
```typescript
return this.createResult(params, {
  success: true,
  roomId,
  totalMessages: 10,
});
```

#### 3. Over-Complicated Logic in Commands
- Commands should coordinate, not implement business logic
- Complex algorithms belong in shared utilities or daemons
- Example: `chat/analyze` duplicate detection could be a utility function

#### 4. Inconsistent Error Handling
- Some throw errors
- Some return `{ success: false, error: string }`
- No standard pattern across commands

#### 5. Direct DAO/Database Access
- Some commands access DataDaemon directly
- Should delegate to `data/*` commands via `Commands.execute()`
- Violates separation of concerns

**Example**: `chat/export` uses `DataDaemon` directly instead of `data/list`

#### 6. File System Operations in Command Logic
- `chat/export` does direct `fs.writeFileSync()`
- Should delegate to `file/save` command
- Makes testing harder, violates single responsibility

### 🔴 CRITICAL Issues

#### 1. Shared Files with Node.js Dependencies
**Status**: ✅ CLEAN - No violations found

#### 2. Magic Strings for Collections/IDs
**Status**: ⚠️ SOME - Fixed in recent work (chat/analyze now uses `ChatMessageEntity.collection`)

#### 3. Type Safety Violations
**Status**: ⚠️ SOME - `chat/analyze` initially used loose `string` for room instead of UUID

## Recommendations

### Immediate (High Priority)

1. **Standardize Parameter Patterns**
   - Always use `xxxId: UUID` for IDs
   - If accepting names, use `xxxName?: string` separately
   - Resolve names to IDs at start of command

2. **Fix Result Construction**
   - Audit ALL commands for direct result object construction
   - Replace with `this.createResult(params, data)`
   - Ensures context/sessionId propagation

3. **Error Handling Standard**
   ```typescript
   // For expected failures (not found, validation)
   return this.createResult(params, {
     success: false,
     error: 'User-friendly message'
   });

   // For unexpected errors (system failures)
   throw new Error('Technical error message');
   ```

4. **Eliminate Direct DAO Access**
   - Replace `DataDaemon.*` with `Commands.execute('data/*', ...)`
   - Proper layering and testability
   - Example: `chat/export` should use `data/list`, not `DataDaemon.queryOpen()`

### Medium Priority

5. **Extract Business Logic to Utilities**
   ```
   commands/chat/analyze/server/
     ├── ChatAnalyzeServerCommand.ts (coordinator)
     └── utils/
         ├── duplicateDetection.ts
         └── timestampAnalysis.ts
   ```

6. **Consistent File Operations**
   - All FS operations via `file/*` commands
   - Never direct `fs.*` calls in command logic
   - Server-side file commands delegate to file daemon

7. **Reorganize Flat Commands**
   - Move UI primitives under `ui/` category:
     ```
     ui/
       ├── click/
       ├── type/
       ├── scroll/
       └── screenshot/
     ```
   - Move system commands:
     ```
     system/
       ├── ping/
       ├── exec/
       └── daemons/ (already here)
     ```

### Low Priority (Nice to Have)

8. **Command Metadata**
   - Add `@deprecated` tags for old patterns
   - Document expected params in JSDoc
   - Generate OpenAPI-style schemas

9. **Testing Infrastructure**
   - Standard test structure for ALL commands
   - Integration tests via `Commands.execute()`
   - Mock daemon responses, not implementations

10. **Command Versioning**
    - Support `command/v2/action` for breaking changes
    - Deprecation warnings in old versions
    - Auto-redirect to latest

## Migration Strategy

### Phase 1: Fix Critical Issues (Week 1)
- Audit all `createResult()` usage
- Fix parameter typing (string vs UUID)
- Document standard patterns in CLAUDE.md

### Phase 2: Refactor Data Access (Week 2)
- Replace `DataDaemon` with `data/*` commands
- Extract business logic to utilities
- Add comprehensive tests

### Phase 3: Reorganize Structure (Week 3)
- Move flat commands to categories
- Consolidate shared types
- Update all documentation

### Phase 4: Add Tooling (Week 4)
- Command linter (detect anti-patterns)
- Auto-generate boilerplate
- Validate schemas on deploy

## Examples of Good Commands

### 1. ping (Simple, Clean)
```typescript
// Params: Just optional flags
interface PingParams extends CommandParams {
  verbose?: boolean;
}

// Result: Structured data
interface PingResult extends CommandResult {
  success: boolean;
  server?: ServerEnvironmentInfo;
  browser?: BrowserEnvironmentInfo;
  timestamp: string;
}
```

### 2. data/list (Proper Generics)
```typescript
interface DataListParams<T extends BaseEntity> extends CommandParams {
  collection: string;
  filter?: Record<string, any>;
  orderBy?: OrderBy[];
  limit?: number;
}

interface DataListResult<T extends BaseEntity> extends CommandResult {
  success: boolean;
  items: readonly T[];
  count: number;
}
```

### 3. chat/export (Good until FS access)
- Accepts both `room` and `filter`
- Properly typed with ChatMessageEntity
- **BUT**: Should use `file/save` instead of `fs.writeFileSync()`

## Action Items

- [ ] Create command linter script
- [ ] Audit all 100 commands for `createResult()` usage
- [ ] Document standard patterns in CLAUDE.md
- [ ] Create migration guide for old commands
- [ ] Add pre-commit hook to check new commands

## Notes

- Commands are auto-discovered, so renaming requires careful migration
- Generator reads Types files for schema extraction
- Breaking changes need version strategy
- Some commands (`exec`, `screenshot`) will always need platform-specific code

---

**Next Steps**: Prioritize Phase 1 fixes, starting with chat/analyze and other recently touched commands.
