# Unified JTAGClient API - Works Identically Everywhere

## Achievement: Browser/Server Harmony

The `JTAGClient` now provides the **same exact API** in both browser and server environments.

## Unified Public API

### Properties (Available in Both Environments)
```typescript
const jtag = await JTAGClient.sharedInstance;

jtag.sessionId    // UUID - current session ID
jtag.context      // JTAGContext - environment and config
jtag.isLocal      // boolean - true if local, false if remote
```

### Commands (Type-Safe Execution)
```typescript
// Registry-based with automatic type inference
const result = await Commands.execute('screenshot', {
  querySelector: 'body'
});
// result is automatically typed as ScreenshotResult!

// Manual typing for commands not in registry
const state = await Commands.execute<StateCreateParams, StateCreateResult>(
  'state/create',
  params
);
```

### Daemon Interface (Work in Progress)
```typescript
// Commands - ✅ WORKING
await jtag.daemons.commands.execute('screenshot', params);

// Events - 🚧 TODO
await jtag.daemons.events.broadcast('user:login', { userId });
jtag.daemons.events.on('user:login', (data) => { ... });

// Data - 🚧 TODO
await jtag.daemons.data.store('key', value);
const value = await jtag.daemons.data.get('key');
```

## What's Unified (✅ Complete)

### 1. Static Shared Instance
```typescript
// Works identically in browser and server
const jtag = await JTAGClient.sharedInstance;
```

**Implementation:**
- Polls `globalThis.jtag` until client is ready
- Browser: `window.jtag`
- Server: `globalThis.jtag`
- No environment-specific code needed

### 2. Session Management
```typescript
// Same property in both environments
console.log(jtag.sessionId); // UUID
```

**Implementation:**
- Base class provides `sessionId` getter
- Returns `_session.sessionId ?? UNKNOWN_SESSION`
- SessionDaemon manages session lifecycle
- Browser: Can persist to sessionStorage (optional)
- Server: Ephemeral, managed by SessionDaemon

### 3. Connection Type Detection
```typescript
// New! Works in both environments
console.log(jtag.isLocal); // true or false
```

**Implementation:**
- Base class provides `isLocal` getter
- Checks if `connection instanceof LocalConnection`
- Browser: Usually true (connects to local JTAGSystemBrowser)
- Server: True if in-process, false if remote

### 4. Type-Safe Commands
```typescript
// Elegant execution with type inference
const screenshot = await Commands.execute('screenshot', {
  querySelector: 'body',
  filename: 'test.png'
});
```

**Implementation:**
- `CommandRegistry` maps command names to types
- Overloaded signatures support both registry and manual typing
- Auto-injects context/sessionId
- Works via `JTAGClient.sharedInstance`

## Environment-Specific Implementation (Hidden)

### Browser (JTAGClientBrowser)
```typescript
protected async getLocalSystem(): Promise<JTAGSystem | null> {
  return JTAGSystemBrowser.instance;
}

protected async getTransportFactory(): Promise<ITransportFactory> {
  return new TransportFactoryBrowser(this.context);
}

protected updateClientSessionStorage(sessionId: UUID): void {
  sessionStorage.setItem('jtag_session_id', sessionId);
}
```

### Server (JTAGClientServer)
```typescript
protected async getLocalSystem(): Promise<JTAGSystem | null> {
  return JTAGSystemServer.instance; // If exists in same process
}

protected async getTransportFactory(): Promise<ITransportFactory> {
  return new TransportFactoryServer(this.context);
}

protected updateClientSessionStorage(sessionId: UUID): void {
  // No-op: Server doesn't persist sessions
}
```

## Usage Examples

### Browser Console
```typescript
// Get client
const jtag = await JTAGClient.sharedInstance;

// Check connection
console.log('Local?', jtag.isLocal);        // true
console.log('Environment:', jtag.context.environment); // "browser"

// Execute commands
const shot = await Commands.execute('screenshot', { querySelector: 'body' });
console.log('Saved to:', shot.filepath);
```

### Server Script
```typescript
// Get client (same code!)
const jtag = await JTAGClient.sharedInstance;

// Check connection (same properties!)
console.log('Local?', jtag.isLocal);        // depends on connection
console.log('Environment:', jtag.context.environment); // "server"

// Execute commands (same API!)
const data = await Commands.execute('data/read', {
  collection: 'Users',
  filter: { active: true }
});
console.log('Found users:', data.items.length);
```

### Widget Code
```typescript
class MyWidget extends BaseWidget {
  async connectedCallback() {
    const jtag = await JTAGClient.sharedInstance;

    // Load data
    const users = await Commands.execute('data/list', {
      collection: 'Users'
    });

    // Render
    this.users = users.items;
    this.render();
  }
}
```

## Next Steps (🚧 TODO)

### 1. Complete Events Interface
```typescript
daemons.events: {
  broadcast: (eventName, data) => commands['events/broadcast']({ ... }),
  on: (eventName, handler) => EventsDaemon.subscribe(...),
  off: (eventName, handler) => EventsDaemon.unsubscribe(...)
}
```

### 2. Complete Data Interface
```typescript
daemons.data: {
  store: (key, value) => commands['data/create']({ collection: 'KeyValue', ... }),
  get: (key) => commands['data/read']({ collection: 'KeyValue', ... }),
  delete: (key) => commands['data/delete']({ collection: 'KeyValue', ... })
}
```

### 3. Auto-Generate Command Registry
Currently hardcoded in `CommandRegistry.ts`. Should be generated from command type files.

## Benefits Achieved

✅ **Write Once, Run Everywhere** - Same code works in browser, server, tests
✅ **Predictable Behavior** - No environment-specific surprises
✅ **Type Safety** - Full IntelliSense and compile-time checking
✅ **Clean Abstractions** - Environment details hidden from callers
✅ **Easy Testing** - Can test same code paths in both environments
✅ **Single Source of Truth** - One API to learn and document