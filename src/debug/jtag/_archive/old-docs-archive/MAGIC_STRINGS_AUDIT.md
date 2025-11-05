# Magic Strings Audit - Hunt for Hardcoded Patterns

## Goal
Replace ALL magic strings with typed constants from centralized locations:
- `EventConstants.ts` for event names
- `CommandConstants.ts` for command names (to be created)
- `CollectionConstants.ts` for collection names (possibly consolidate with existing COLLECTIONS)

## Event System Issues Found

### 1. Old Event System Usage (`EventManager.on()`)

**Problem**: Some code still uses old `EventManager` instead of universal `Events`

**Files to Fix**:
- `/daemons/user-daemon/server/UserDaemonServer.ts` (lines 41, 46, 51)
  ```typescript
  // ❌ OLD:
  this.eventManager.events.on('data:User:created', async (userData) => {

  // ✅ NEW:
  Events.subscribe(DATA_EVENTS.USERS.CREATED, async (userData: UserEntity) => {
  ```

**Pattern**: Search for `.on('data:` to find all old event subscriptions

### 2. Hardcoded Event Names

**Files with hardcoded event strings**:
- `/daemons/user-daemon/server/UserDaemonServer.ts`
  - `'data:User:created'` → `DATA_EVENTS.USERS.CREATED`
  - `'data:User:updated'` → `DATA_EVENTS.USERS.UPDATED`
  - `'data:User:deleted'` → `DATA_EVENTS.USERS.DELETED`

**Note**: `DATA_EVENTS.USERS` uses lowercase `users` collection name, need to verify collection naming convention

### 3. Collection Name Inconsistencies

**Issue**: Event system uses `users` but some code uses `User` (capitalized)

**Resolution Needed**:
- Check `COLLECTIONS` constant in `/system/data/config/DatabaseConfig.ts`
- Verify BaseEntity.collection values for all entities
- Ensure EventConstants matches actual collection names

---

## Command System Issues Found

### Hardcoded Command Names

**Pattern Search**: `Commands.execute('command/name'` or `executeCommand('command/name'`

**Examples Found**:
```typescript
// Throughout widgets and commands:
Commands.execute('data/list', params)
Commands.execute('data/create', params)
Commands.execute('data/read', params)
Commands.execute('data/update', params)
Commands.execute('data/delete', params)
Commands.execute('screenshot', params)
Commands.execute('state/get', params)
Commands.execute('state/create', params)
Commands.execute('user/create', params)
```

**Need**: `CommandConstants.ts` with structured command names

---

## Proposed CommandConstants Structure

```typescript
export const COMMAND_NAMES = {
  DATA: {
    LIST: 'data/list',
    CREATE: 'data/create',
    READ: 'data/read',
    UPDATE: 'data/update',
    DELETE: 'data/delete',
    TRUNCATE: 'data/truncate',
    CLEAR: 'data/clear',
    SCHEMA: 'data/schema',
  },

  STATE: {
    GET: 'state/get',
    CREATE: 'state/create',
    UPDATE: 'state/update',
  },

  USER: {
    CREATE: 'user/create',
  },

  FILE: {
    LOAD: 'file/load',
    SAVE: 'file/save',
    APPEND: 'file/append',
  },

  DEBUG: {
    LOGS: 'debug/logs',
    HTML_INSPECTOR: 'debug/html-inspector',
    WIDGET_STATE: 'debug/widget-state',
    WIDGET_EVENTS: 'debug/widget-events',
    SCROLL_TEST: 'debug/scroll-test',
  },

  UI: {
    SCREENSHOT: 'screenshot',
    CLICK: 'click',
    TYPE: 'type',
    SCROLL: 'scroll',
    NAVIGATE: 'navigate',
  },

  THEME: {
    GET: 'theme/get',
    LIST: 'theme/list',
    SET: 'theme/set',
  },

  SESSION: {
    CREATE: 'session/create',
    DESTROY: 'session/destroy',
  },

  TEST: {
    RUN_SUITE: 'test/run/suite',
  },
} as const;
```

---

## Collection Name Verification

**Check**: Do all collection names match between:
1. Entity `static collection` values
2. `COLLECTIONS` constant
3. `EventConstants` DATA_EVENTS keys

**Entities to verify**:
- UserEntity → `users` or `User`?
- RoomEntity → `rooms` or `Room`?
- ChatMessageEntity → `chat_messages` or `ChatMessage`?
- UserStateEntity → `user_states` or `UserState`?
- ContentType → `ContentType` or `content_types`?
- TrainingSession → `TrainingSession` or `training_sessions`?

---

## Search Commands for Audit

```bash
# Find old event subscription patterns
grep -rn "\.on('data:" --include="*.ts" --exclude-dir=node_modules --exclude-dir=dist .

# Find hardcoded event strings
grep -rn "'data:[a-zA-Z_]*:[a-z]*'" --include="*.ts" --exclude-dir=node_modules --exclude-dir=dist .

# Find hardcoded command strings
grep -rn "Commands\.execute('[a-z/]*'" --include="*.ts" --exclude-dir=node_modules --exclude-dir=dist .

# Find executeCommand usage
grep -rn "executeCommand('[a-z/]*'" --include="*.ts" --exclude-dir=node_modules --exclude-dir=dist .

# Find collection references
grep -rn "collection.*=.*['\"]" --include="*.ts" --exclude-dir=node_modules --exclude-dir=dist .
```

---

## Priority Fix List

### High Priority (Breaking Pattern)
1. ✅ **UserDaemonServer** - Convert from EventManager to universal Events
2. ✅ **Command Constants Created** - Modular constants in each command domain
3. ⬜ **All Widgets** - Update to use CommandConstants for command execution
4. ⬜ **All Commands** - Update internal usage to use constants

### Medium Priority (Consistency)
4. ⬜ **Collection Names** - Verify and standardize across codebase
5. ⬜ **Test Files** - Update to use constants

### Low Priority (Documentation)
6. ⬜ **Comments/Docs** - Update examples to use constants
7. ⬜ **Error Messages** - Use constants in error messages for consistency

---

## Testing Strategy

After each fix category:
1. **Compilation**: `npx tsc --noEmit`
2. **Unit Tests**: `npm test`
3. **Integration Test**: `npm start` → verify system works
4. **Event Flow Test**: Create/update/delete entities, verify events fire
5. **Command Test**: Execute various commands, verify they work

---

## Benefits of Constant Unification

1. **Type Safety**: Catch typos at compile time
2. **Autocomplete**: IDE suggests available events/commands
3. **Refactoring**: Change in one place updates everywhere
4. **Documentation**: Constants serve as API documentation
5. **Discoverability**: Easy to see what events/commands exist
6. **Validation**: Can build validators from constant definitions
7. **Testing**: Mock/stub using known constants
8. **Migration**: Clear path when deprecating old patterns

---

## Next Steps

1. Create `CommandConstants.ts`
2. Fix UserDaemonServer to use Events.subscribe()
3. Update all Commands.execute() calls to use CommandConstants
4. Verify collection name consistency
5. Update tests to use constants
6. Document the unified architecture
