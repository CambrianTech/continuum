# User State Architecture

## Overview

Every client (browser, test, persona) that connects to JTAG gets a **User** object with persistent state. This document defines how UserState is initialized and managed across different client types.

## User Hierarchy

```
BaseUser (abstract)
├── entity: UserEntity       // Database record (id, displayName, etc.)
├── state: UserState         // Preferences, open tabs, UI state
│
├── HumanUser extends BaseUser
│   └── Storage: localStorage (browser) or SQLite (test client)
│
└── AIUser extends BaseUser (abstract)
    ├── AgentUser extends AIUser
    │   └── Storage: In-memory or temporary SQLite
    │
    └── PersonaUser extends AIUser
        └── Storage: Dedicated SQLite per persona (child process isolation)
```

## Storage Strategy by Client Type

### Browser Client (HumanUser)
- **UserState location:** Browser localStorage
- **Persistence:** Survives browser restarts
- **Identity:** BrowserDeviceIdentity (deviceId + anonymous userId, encrypted)
- **Use case:** Anonymous users, passkey authentication later

### Test Client (HumanUser)
- **UserState location:** In-memory or temp SQLite
- **Persistence:** Ephemeral, cleared after test
- **Identity:** Generated test userId
- **Use case:** Integration tests, CI/CD

### Persona Client (PersonaUser)
- **UserState location:** Dedicated SQLite database per persona
- **Persistence:** Permanent, isolated from main database
- **Identity:** Persona UUID
- **Use case:** AI agents with memory, child process isolation
- **Path:** `.continuum/personas/{personaId}/state.sqlite`

### Agent Client (AgentUser)
- **UserState location:** In-memory
- **Persistence:** Ephemeral
- **Identity:** Agent session UUID
- **Use case:** External AI portals (Claude, GPT API connections)

## Architecture Principles

### 1. Initialization in SessionDaemon
**SessionDaemonServer** is responsible for:
1. Creating session
2. Creating or loading User (BaseUser subclass)
3. Initializing User.state (UserState entity)
4. Returning User object to client

### 2. Access Pattern
All code accesses state through the User object:

```typescript
// ❌ WRONG: Widget creating UserState
await Commands.execute('data/create', {
  collection: 'UserState',
  data: { theme: 'cyberpunk' }
});

// ✅ CORRECT: Widget updating via User
const jtagClient = await JTAGClient.sharedInstance;
jtagClient.user.state.preferences.theme = 'cyberpunk';
await jtagClient.user.state.save(); // Generic save method
```

### 3. Storage Abstraction
UserState doesn't know where it's stored:

```typescript
class UserState {
  async save(): Promise<void> {
    // Delegates to storage backend (localStorage, SQLite, etc.)
    await this.storageBackend.save(this);
  }

  async load(): Promise<void> {
    await this.storageBackend.load(this);
  }
}
```

Storage backends:
- `LocalStorageStateBackend` - Browser localStorage
- `SQLiteStateBackend` - SQLite database (tests, personas)
- `MemoryStateBackend` - In-memory (agents)

## Implementation Plan

### Phase 1: User Object Structure
1. Create `BaseUser` abstract class with `.entity` and `.state`
2. Implement `HumanUser`, `AgentUser`, `PersonaUser` subclasses
3. Add storage backend interface

### Phase 2: SessionDaemon Integration
1. Modify `SessionDaemonServer.createSession()` to initialize User
2. Add User object to session metadata
3. Expose User via `JTAGClient.user`

### Phase 3: Storage Backends
1. Implement `LocalStorageStateBackend` for browser
2. Implement `SQLiteStateBackend` for personas
3. Add persona-specific database path resolution

### Phase 4: Widget Migration
1. Update ThemeWidget to use `jtagClient.user.state`
2. Remove UserState creation logic from widgets
3. Remove `getUserStateId()` temporary method

## Benefits

1. **Unified API:** All clients use same `user.state` interface
2. **Proper abstraction:** Storage is implementation detail
3. **Isolation:** Personas get their own SQLite (child process safe)
4. **Testability:** Easy to mock storage backends
5. **Future-proof:** Passkey authentication, multi-device sync, etc.

## Example Usage

### Theme Widget
```typescript
// Get current theme
const theme = jtagClient.user.state.preferences.theme;

// Update theme
jtagClient.user.state.preferences.theme = 'cyberpunk';
await jtagClient.user.state.save();
```

### Persona Initialization
```typescript
// SessionDaemonServer creates persona
const persona = new PersonaUser({
  personaId: generateUUID(),
  databasePath: `.continuum/personas/${personaId}/state.sqlite`
});

await persona.state.load(); // Loads from persona's SQLite
```

### Test Client
```typescript
// Test creates ephemeral user
const testUser = new HumanUser({
  storageBackend: new MemoryStateBackend()
});

await testUser.state.load(); // No-op, uses defaults
```

## Migration Path

1. **Don't break existing code:** Add User object alongside current system
2. **Gradual migration:** Update widgets one at a time
3. **Keep getUserStateId():** Temporarily for backwards compatibility
4. **Remove after migration:** Clean up temporary methods

## Open Questions

1. Where does BaseUser live? `system/user/` directory?
2. Should User.state auto-save on property change or require explicit `.save()`?
3. How to handle User.state for anonymous browser users vs authenticated users?
4. Multi-device sync strategy for HumanUser?

## Next Steps

1. Review with Joel - verify architecture aligns with vision
2. Create BaseUser/HumanUser/PersonaUser classes
3. Implement storage backend interface
4. Modify SessionDaemonServer to create User objects
5. Update JTAGClient to expose `user` property