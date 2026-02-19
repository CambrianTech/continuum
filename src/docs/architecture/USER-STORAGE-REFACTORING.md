# User Storage Refactoring - Unified Architecture (DAEMON-BASED)

**Goal**: Eliminate duplication between HumanUser and PersonaUser using EXISTING daemon architecture, not manager bloat.

## Problem Statement

**Before**:
- PersonaUser.ts: 1262 lines (too large!)
- Each user type had different database paths (`.continuum/personas/` vs no structure for humans)
- Storage logic embedded directly in PersonaUser
- State management scattered across BaseUser and PersonaUser
- No consistent pattern for user directories

## CRITICAL LESSON: Use Daemons, NOT Managers

### ❌ WRONG APPROACH (What I Did First)
Created "manager" classes that run in main thread:
- `IUserStorage` interface
- `SQLiteUserStorage` implementation
- `IUserStateManager` interface

**Why This is WRONG**:
1. **Main-thread bloat** - Managers run synchronously, blocking event loop
2. **Violates RTOS principles** - This is a real-time OS, not a web app
3. **Ignores existing architecture** - DataDaemon and UserDaemon ALREADY exist
4. **async/await cancer** - Leads to "rube goldberg" setTimeout logic

**User feedback**: "you write managers instead of daemons, which also COULD Be written WELL but you insist on pulling more and more energy from the main thread"

### ✅ CORRECT APPROACH (Daemon Architecture)

**Use existing daemons**:
- **DataDaemon** - Already provides storage with message-passing concurrency
- **UserDaemon** - Already manages user lifecycle and PersonaUser instances
- **SQLiteStateBackend** - Already exists as thin adapter

**Create ONLY**:
- **UserDirectoryManager** - Lightweight path utility (synchronous is OK for paths)

## Solution: Use Existing Daemon Infrastructure

### Architecture (Corrected)

```
system/user/
├── directory/server/
│   └── UserDirectoryManager.ts  ✅ KEEP (path utility only)
│       - Unified .continuum/users/{userId}/ structure
│       - Path resolution: database, logs, config, preferences
│       - Lightweight, synchronous (just path strings)
│
└── shared/
    └── BaseUser.ts  🔨 REFACTOR
        - Use DataDaemon for storage (via Commands.execute)
        - Use UserDirectoryManager for paths
        - Remove embedded storage logic

daemons/
├── data-daemon/  ✅ ALREADY EXISTS
│   - Concurrent storage via message-passing
│   - Pluggable backends (SQLite, Memory, etc.)
│   - Use via: Commands.execute('data/store', { key, value })
│
└── user-daemon/  ✅ ALREADY EXISTS
    - Manages PersonaUser lifecycle
    - Already uses SQLiteStateBackend
    - Handles user:created, user:deleted events
```

### Unified Directory Structure

**ALL users** (human or AI) now use:
```
.continuum/users/{userId}/
├── data/
│   └── longterm.db  (UserStateEntity + memories + everything)
├── logs/
│   └── activity.log
└── config/
    └── preferences.json
```

**Benefits**:
1. **Consistency** - Same structure for Joel, Helper AI, Claude, etc.
2. **No special cases** - PersonaUser and HumanUser are 95% identical
3. **Single source of truth** - One database name (`longterm.db`)
4. **True concurrency** - Daemon message-passing, not main-thread async

## Implementation Status

### ✅ Phase 1: Path Utility (COMPLETED)

**Created File**:
- `system/user/directory/server/UserDirectoryManager.ts` (166 lines)
  - ✅ **TESTED** - 12 unit tests passing
  - `getPaths(userId)` - Get all paths for a user
  - `ensureDirectories(userId)` - Create directory structure
  - `getDatabasePath(userId)` - Most common operation
  - `listUsers()` - List all user directories
  - `getUserDiskUsage(userId)` - Get database size

**Why This is OK**:
- Just a path resolution utility (synchronous)
- No heavy operations (no I/O, just string manipulation)
- Returns data structures, doesn't manage state

### ❌ Files to DELETE (Manager Bloat)

**Delete These**:
- `system/user/storage/shared/IUserStorage.ts` - Use DataDaemon instead
- `system/user/storage/server/SQLiteUserStorage.ts` - Use DataDaemon instead
- `system/user/state/shared/IUserStateManager.ts` - Use DataDaemon instead

**Why Delete**:
- DataDaemon already provides storage with proper concurrency
- UserDaemon already manages user state lifecycle
- These "managers" would run in main thread (bad!)

### 🔨 Phase 2: Refactor BaseUser (NEXT)

**Changes to BaseUser.ts**:

```typescript
export abstract class BaseUser {
  // ONLY use lightweight path utility
  protected directoryManager: UserDirectoryManager;

  constructor(entity: UserEntity) {
    this.entity = entity;
    this.directoryManager = new UserDirectoryManager();
  }

  async initialize(): Promise<void> {
    // Ensure directory structure exists
    await this.directoryManager.ensureDirectories(this.entity.id);

    // Load user state via DataDaemon (concurrent!)
    const result = await Commands.execute('data/read', {
      key: 'user_state',
      userId: this.entity.id
    });

    if (result.success && result.data) {
      this.state = result.data as UserStateEntity;
    } else {
      // Create default state
      this.state = new UserStateEntity();
      this.state.userId = this.entity.id;
      await this.saveState();
    }
  }

  async saveState(): Promise<void> {
    if (this.state) {
      // Save via DataDaemon (concurrent!)
      await Commands.execute('data/store', {
        key: 'user_state',
        value: this.state,
        userId: this.entity.id
      });
    }
  }

  // Get database path when needed
  getDatabasePath(): string {
    return this.directoryManager.getDatabasePath(this.entity.id);
  }
}
```

**Key Changes**:
- ✅ Use `Commands.execute('data/store')` instead of direct SQLite calls
- ✅ Use `Commands.execute('data/read')` for loading state
- ✅ Keep UserDirectoryManager for paths only
- ✅ No manager bloat in constructor

### 🔨 Phase 3: Update UserDaemon (NEXT)

**Changes to daemons/user-daemon/server/UserDaemonServer.ts**:

```typescript
import { UserDirectoryManager } from '../../../system/user/directory/server/UserDirectoryManager';

export class UserDaemonServer extends UserDaemon {
  private directoryManager: UserDirectoryManager;

  async initialize(): Promise<void> {
    this.directoryManager = new UserDirectoryManager();

    // Ensure all users have unified directory structure
    const users = await Commands.execute('data/list', {
      collection: 'users'
    });

    for (const user of users) {
      await this.directoryManager.ensureDirectories(user.id);
    }

    await super.initialize();
  }

  protected async ensureUserHasState(userId: UUID): Promise<boolean> {
    // Use unified paths
    const dbPath = this.directoryManager.getDatabasePath(userId);

    // DataDaemon will handle actual storage
    const result = await Commands.execute('data/read', {
      key: 'user_state',
      userId
    });

    if (!result.success || !result.data) {
      // Create default state
      const defaultState = new UserStateEntity();
      defaultState.userId = userId;
      await Commands.execute('data/store', {
        key: 'user_state',
        value: defaultState,
        userId
      });
    }

    return true;
  }
}
```

### 🔨 Phase 4: Slim Down PersonaUser (FUTURE)

**Current**: 1262 lines (storage + state + cognition)
**Target**: ~400 lines (cognition orchestration only)

**Remove**:
- Direct SQLite operations → Use DataDaemon via Commands.execute
- Embedded directory management → Use UserDirectoryManager
- State persistence logic → Use DataDaemon

**Keep**:
- PersonaInbox (cognition-specific)
- PersonaState (energy, mood tracking)
- PersonaGenome (LoRA paging)
- Autonomous servicing loop

## Migration Strategy

### Backward Compatibility

**Old path**: `.continuum/personas/{userId}/data/longterm.db`
**New path**: `.continuum/users/{userId}/data/longterm.db`

**UserDirectoryManager supports fallback**:
```typescript
getPaths(userId: UUID): UserDirectoryPaths {
  // Try new path first
  const newPath = path.join('.continuum/users', userId);
  if (fs.existsSync(newPath)) {
    return this.buildPaths(newPath);
  }

  // Fall back to legacy persona path if exists
  const legacyPath = path.join('.continuum/personas', userId);
  if (fs.existsSync(legacyPath)) {
    return this.buildPaths(legacyPath);
  }

  // Return new path for creation
  return this.buildPaths(newPath);
}
```

### Migration Script

Add to package.json:
```json
{
  "scripts": {
    "migrate:user-directories": "tsx scripts/migrate-user-directories.ts"
  }
}
```

Script moves `.continuum/personas/` → `.continuum/users/` preserving structure.

## BAD PATTERNS TO FIX SYSTEM-WIDE

This refactoring revealed system-wide anti-patterns that must be eliminated:

### 🚨 Anti-Pattern 1: Main-Thread Managers

**What to look for**:
```typescript
// ❌ BAD - Manager runs in main thread
export class FooManager {
  async doThing(): Promise<void> {
    // Heavy operation in main thread
  }
}
```

**Fix**:
```typescript
// ✅ GOOD - Use daemon message-passing
const result = await Commands.execute('daemon/operation', params);
```

### 🚨 Anti-Pattern 2: setTimeout/async Rube Goldberg

**What to look for**:
```typescript
// ❌ BAD - Polling with setTimeout
private pollLoop(): void {
  setTimeout(() => {
    this.checkThings();
    this.pollLoop(); // Recurse
  }, 1000);
}
```

**Fix**:
```typescript
// ✅ GOOD - Event-driven via daemon
Events.subscribe('thing:changed', () => {
  // React to changes, don't poll
});
```

### 🚨 Anti-Pattern 3: Direct Database Access

**What to look for**:
```typescript
// ❌ BAD - Direct SQLite in application code
const db = new Database(dbPath);
const row = db.prepare('SELECT * FROM foo').get();
```

**Fix**:
```typescript
// ✅ GOOD - Use DataDaemon
const result = await Commands.execute('data/read', { key: 'foo' });
```

### 🚨 Anti-Pattern 4: Scattered Storage Logic

**What to look for**:
- Storage operations embedded in PersonaUser
- Different paths for different user types
- Multiple database names (state.db vs longterm.db)

**Fix**:
- ONE path pattern: `.continuum/users/{userId}/data/longterm.db`
- ONE database name: `longterm.db`
- ONE storage interface: DataDaemon via Commands.execute

## Daemon Audit TODO

Search for these patterns system-wide and FIX them:

```bash
# Find setTimeout usage in daemons
grep -r "setTimeout" daemons/

# Find direct SQLite usage (should only be in DataDaemon backends)
grep -r "new Database" --exclude-dir="daemons/data-daemon"

# Find async methods in non-daemon code
grep -r "async.*(" system/user/

# Find manager classes (probably need to be daemons or utilities)
find . -name "*Manager.ts" -not -path "*UserDirectoryManager*"
```

## Testing Strategy

### 1. Unit Tests (Isolated)
```bash
# UserDirectoryManager (✅ DONE - 12 tests passing)
npx vitest tests/unit/user/UserDirectoryManager.test.ts

# DataDaemon storage operations
npx vitest tests/unit/daemons/DataDaemon.test.ts
```

### 2. Integration Tests
```bash
# BaseUser with daemon-based storage
npx vitest tests/integration/user-storage.test.ts

# PersonaUser initialization with unified paths
npx vitest tests/integration/persona-user-init.test.ts
```

### 3. System Tests
```bash
npm start  # Deploy
./jtag ping  # Verify server + browser connected

# Verify personas initialize correctly
./jtag data/list --collection=users --filter='{"type":"persona"}'

# Check database paths
ls -la .continuum/users/*/data/longterm.db
```

## Benefits

### For Developers

1. **Smaller files** - PersonaUser.ts: 1262 → ~400 lines
2. **Clear separation** - Cognition in PersonaUser, storage in DataDaemon
3. **Reusable daemons** - DataDaemon works for ALL storage needs
4. **Testable** - Each daemon independently testable
5. **True concurrency** - Message-passing, not main-thread async

### For System

1. **Unified paths** - One pattern for all users
2. **No special cases** - Human and AI users identical at storage layer
3. **RTOS-compliant** - Daemon message-passing for true concurrency
4. **Better performance** - No main-thread blocking
5. **Cleaner architecture** - Single responsibility per daemon

## Next Steps

1. ✅ **DONE**: Create and test UserDirectoryManager
2. **NEXT**: Refactor BaseUser to use Commands.execute for storage
3. **NEXT**: Update UserDaemon to use unified paths
4. **NEXT**: Slim down PersonaUser (remove storage, keep cognition)
5. **NEXT**: Audit system-wide for bad patterns (setTimeout, managers, direct SQLite)
6. **LATER**: Migration script for `.continuum/personas/` → `.continuum/users/`

## References

- **DAEMON-ARCHITECTURE.md** - "85% Shared, 15% Context-Specific"
- **User insight**: "is there any reason to keep memories and state in separate dbs?" - NO
- **Design principle**: "users and persona are really the same just that persona requires memories and all sorts of rtos cognition"
- **RTOS requirement**: "We are writing an rtos. do not forget that"
- **Code quality principle**: "good coders refine ALL CODE over time"
