# JTAGClient Unification Plan - Browser vs Server Harmony

## Current Differences

### Browser-Specific (JTAGClientBrowser)
```typescript
- private _sessionId: UUID
- private initializeSessionId(): UUID  // Uses sessionStorage
- public setSessionId(sessionId: UUID): void
- public get sessionId(): UUID
- protected updateClientSessionStorage(sessionId: UUID): void  // Uses sessionStorage
- private updateSystemConsoleDaemon(): void
- get isLocal(): boolean
```

### Server-Specific (JTAGClientServer)
```typescript
- protected updateClientSessionStorage(sessionId: UUID): void  // No-op
```

### Shared Base (JTAGClient)
```typescript
✅ static get sharedInstance(): Promise<JTAGClient>
✅ get daemons() { commands, events, data }
✅ protected _session: Session | null
✅ protected _connection: JTAGConnection | null
✅ protected abstract getLocalSystem(): Promise<JTAGSystem | null>
✅ protected abstract getTransportFactory(): Promise<ITransportFactory>
```

## Goal: Identical Public API

**Every JTAGClient should expose:**
```typescript
// Static
static sharedInstance: Promise<JTAGClient>

// Instance Properties
sessionId: UUID
context: JTAGContext
isLocal: boolean

// Methods
connect(options?): Promise<void>
disconnect(): Promise<void>

// Daemon Interface
daemons.commands.execute<T, U>(command, params): Promise<U>
daemons.events.broadcast<T>(eventData): Promise<void>
daemons.events.on<T>(eventName, handler): void
daemons.data.store<T>(key, value): Promise<void>
daemons.data.get<T>(key): Promise<T>
```

## Unification Strategy

### 1. Move sessionId to Base Class
**Current:** Browser has custom sessionId getter/setter
**Target:** Base class manages session, both environments use same pattern

```typescript
// In JTAGClient (shared)
public get sessionId(): UUID {
  return this._session?.sessionId ?? SYSTEM_SCOPES.UNKNOWN_SESSION;
}
```

### 2. Move isLocal to Base Class
**Current:** Only browser has it
**Target:** Both environments report if using local vs remote connection

```typescript
// In JTAGClient (shared)
public get isLocal(): boolean {
  return this._connection instanceof LocalConnection;
}
```

### 3. Standardize Session Persistence
**Current:** Browser uses sessionStorage, server uses nothing
**Target:** Both use appropriate persistence for their environment

```typescript
// In JTAGClientBrowser
protected updateClientSessionStorage(sessionId: UUID): void {
  sessionStorage.setItem('jtag_session_id', sessionId);
}

// In JTAGClientServer
protected updateClientSessionStorage(sessionId: UUID): void {
  // Server: No persistence needed, SessionDaemon manages it
  // Or: Could persist to file if needed for multi-process scenarios
}
```

### 4. Complete Events Interface
**Current:** events.broadcast throws "not implemented"
**Target:** Both environments can broadcast and subscribe to events

```typescript
daemons.events: {
  broadcast: async <T>(eventName: string, data: T): Promise<void> => {
    return await this.commands['events/broadcast']({ eventName, data });
  },

  on: <T>(eventName: string, handler: (data: T) => void): void => {
    // Subscribe through EventsDaemon
    // Browser: WebSocket listener
    // Server: Direct EventsDaemon subscription
  },

  off: (eventName: string, handler?: Function): void => {
    // Unsubscribe
  }
}
```

### 5. Complete Data Interface
**Current:** data.store throws "not implemented"
**Target:** Both environments can store/retrieve data

```typescript
daemons.data: {
  store: async <T>(key: string, value: T): Promise<void> => {
    return await this.commands['data/create']({ collection: 'KeyValue', data: { key, value } });
  },

  get: async <T>(key: string): Promise<T | null> => {
    const result = await this.commands['data/read']({ collection: 'KeyValue', filter: { key } });
    return result?.data?.value ?? null;
  },

  delete: async (key: string): Promise<void> => {
    return await this.commands['data/delete']({ collection: 'KeyValue', filter: { key } });
  }
}
```

## Implementation Steps

### Step 1: Move Common Properties to Base
- [x] sessionId getter (already in base)
- [ ] Add isLocal getter to base
- [ ] Verify context is accessible from base

### Step 2: Standardize Session Management
- [ ] Remove browser-specific _sessionId property
- [ ] Use base class _session everywhere
- [ ] Keep updateClientSessionStorage abstract for env-specific persistence

### Step 3: Complete Daemon Interfaces
- [ ] Implement events.broadcast via events/broadcast command
- [ ] Implement events.on via EventsDaemon subscription
- [ ] Implement data.store via data/create command
- [ ] Implement data.get via data/read command

### Step 4: Test Symmetry
```typescript
// This code should work identically in browser and server:
const jtag = await JTAGClient.sharedInstance;

// Commands
const result = await jtag.daemons.commands.execute('screenshot', params);

// Events
await jtag.daemons.events.broadcast('user:logged-in', { userId });
jtag.daemons.events.on('user:logged-in', (data) => console.log(data));

// Data
await jtag.daemons.data.store('last-room', 'general');
const lastRoom = await jtag.daemons.data.get('last-room');

// Properties
console.log(jtag.sessionId);
console.log(jtag.isLocal);
console.log(jtag.context.environment);
```

## Success Criteria

✅ Same public API in browser and server
✅ Same method signatures and behavior
✅ Environment-specific implementation hidden behind abstract methods
✅ Zero breaking changes to existing code
✅ Commands, Events, and Data all work symmetrically

## Benefits

1. **Consistency** - Write once, works everywhere
2. **Predictability** - No "works in browser but not server" surprises
3. **Testability** - Can test same code paths in both environments
4. **Documentation** - One set of docs for JTAGClient, not two
5. **Maintainability** - Changes to API happen once, not twice