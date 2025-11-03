# Architecture Inconsistencies: Browser vs Server

## Current State Analysis

### Client Architecture
```
JTAGClient (shared base)
├── JTAGClientBrowser (browser-specific)
│   ├── sessionStorage for session persistence
│   ├── getLocalSystem() → JTAGSystemBrowser.instance
│   └── TransportFactoryBrowser
└── JTAGClientServer (server-specific)
    ├── No session storage (managed by SessionDaemon)
    ├── getLocalSystem() → JTAGSystemServer.instance (if exists)
    └── TransportFactoryServer (ISSUE: only supports 'server' role, not 'client')
```

### Command Execution Patterns

**Browser Commands:**
- Inherit from `CommandBase<TParams, TResult>`
- Use `remoteExecute()` to call server commands
- Example: ScreenshotBrowserCommand captures DOM → calls file/save on server

**Server Commands:**
- Inherit from `CommandBase<TParams, TResult>`
- Use `remoteExecute()` to call browser commands
- Example: ScreenshotServerCommand → calls browser if no dataUrl

### Event System Patterns

**Current Implementation:**
- Events are supposed to flow: Server → Database → EventsDaemon → Clients
- Browser and server should receive events symmetrically
- ISSUE: Event broadcasting not implemented in client daemons interface

## Key Inconsistencies

### 1. Session Management
**Browser:** Uses sessionStorage to persist session ID across page reloads
**Server:** Relies on SessionDaemon to assign session, no persistence
**Issue:** Different initialization patterns

### 2. Local System Access
**Browser:** Always gets local system (JTAGSystemBrowser.instance)
**Server:** Only gets local system if already exists in same process
**Issue:** Asymmetric - server forces remote when it shouldn't

### 3. Transport Factory Roles
**Browser:** TransportFactoryBrowser supports 'client' role
**Server:** TransportFactoryServer only supports 'server' role
**Issue:** Server can't make client connections to remote JTAG systems

### 4. Command Execution Flow
**Both:** Use same CommandBase + remoteExecute pattern ✅
**Issue:** But Commands.execute() requires JTAGClient.sharedInstance which may not be initialized

### 5. Static Execute API Design Flaw
**Current Design:**
```typescript
// This doesn't work because it needs client initialization:
await ScreenshotCommand.execute({ querySelector: 'body' });
// → Calls Commands.execute()
// → Needs JTAGClient.sharedInstance
// → Waits for globalThis.jtag to be ready
```

**Problem:**
- Works from browser console/widgets (client already initialized)
- Doesn't work from inside commands (different execution context)
- Doesn't work from standalone scripts (no client initialized)

### 6. Event System Not Unified
**Client daemons interface has:**
- `daemons.commands.execute()` ✅ Implemented
- `daemons.events.broadcast()` ❌ Not implemented
- `daemons.data.store()` ❌ Not implemented

**Issue:** Commands can execute across environments, but events can't broadcast symmetrically

## What Needs to be Consistent

### 1. Command Execution
**Inside Commands (already works):**
```typescript
// Use remoteExecute - already consistent ✅
await this.remoteExecute(params, 'file/save', 'server');
```

**Outside Commands (needs fixing):**
```typescript
// Should work the same in browser and server
const result = await Commands.execute('screenshot', { querySelector: 'body' });
```

### 2. Event System
**Should work symmetrically:**
```typescript
// Browser: emit event
await Events.broadcast('data:Room:updated', { roomId });

// Server: emit event
await Events.broadcast('data:Room:updated', { roomId });

// Both: receive events
Events.on('data:Room:updated', (data) => { ... });
```

### 3. Local vs Remote Detection
**Should be consistent:**
- Browser: Check if JTAG system exists locally
- Server: Check if JTAG system exists locally
- Both: Fall back to remote connection if not local

### 4. Transport Factory
**Should support both roles:**
- TransportFactoryBrowser: ✅ Supports 'client' role
- TransportFactoryServer: ❌ Needs 'client' role support

## Proposed Fixes

### Fix 1: Unified Command Execution
Remove static execute() from CommandBase - it's not the right pattern.

**For external callers:**
```typescript
// Use Commands.execute (already works)
await Commands.execute('screenshot', { querySelector: 'body' });
```

**For internal command-to-command:**
```typescript
// Use remoteExecute (already works)
await this.remoteExecute(params, 'file/save', 'server');
```

### Fix 2: Implement Event Broadcasting
Complete the `daemons.events.broadcast()` implementation in JTAGClient:
```typescript
events: {
  broadcast: async <T>(eventData: T): Promise<void> => {
    return await this.commands['events/broadcast'](eventData);
  },
  on: <T>(eventName: string, handler: (data: T) => void): void => {
    // Subscribe to events through EventsDaemon
  }
}
```

### Fix 3: Symmetric Local System Detection
Both browser and server should follow same pattern:
1. Check for local system instance
2. Use local if available
3. Connect remotely if not

### Fix 4: Transport Factory Client Support
Add client transport creation to TransportFactoryServer:
```typescript
// Server should be able to connect AS a client to other servers
const clientTransport = await factory.createTransport('websocket', 'client');
```

### Fix 5: Consistent Session Management
Either:
- Option A: Both use persistence (server persists to file)
- Option B: Both rely on daemon (browser doesn't persist)
- Recommendation: Option B - let SessionDaemon manage all sessions

## Next Steps

1. **Remove faulty static execute() API** - it doesn't solve the right problem
2. **Document Commands.execute() as the external API** - works in both environments
3. **Keep remoteExecute() for command-to-command** - already works perfectly
4. **Implement Events.broadcast() symmetrically** - browser and server same pattern
5. **Fix TransportFactoryServer** - add client role support
6. **Unify local system detection** - same logic both sides

## Success Criteria

✅ Browser and server use identical patterns for:
- Command execution (external and internal)
- Event broadcasting and subscription
- Local vs remote system detection
- Session management

✅ No special cases or environment-specific workarounds
✅ Same code works in both environments without changes