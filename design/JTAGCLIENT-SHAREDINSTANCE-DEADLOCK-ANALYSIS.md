# JTAGClient.sharedInstance Deadlock Analysis

**Date**: 2025-10-19
**Status**: CRITICAL BUG - PersonaUser and server code cannot use Events.emit()
**Root Cause**: globalThis.jtag only set in browser, never in server

---

## Executive Summary

`JTAGClient.sharedInstance` polls `globalThis.jtag?.commands` every 50ms until it resolves. This works in browser (where `globalThis.jtag` is set after connection), but **FAILS PERMANENTLY on server** where `globalThis.jtag` is never set, causing infinite polling and complete failure of auto-context `Events.emit()` calls.

**Impact**: PersonaUser and ALL server code using `Events.emit()` auto-context form hangs indefinitely.

---

## The Four Connection Cases

### CASE 1: Browser RemoteConnection (browser client → remote server via WebSocket)

**Initialization Flow**:
```typescript
// browser-index.ts:27-62
async connect() {
  // 1. Register widgets
  BROWSER_WIDGETS.forEach(widget => customElements.define(...));

  // 2. Connect client (may use RemoteConnection if no local system)
  const connectionResult = await JTAGClientBrowser.connectLocal();
  const client = connectionResult.client;

  // 3. ✅ SET globalThis.jtag AFTER connection completes
  (globalThis as any).jtag = client;

  return { ...connectionResult, client };
}
```

**Connection Type**: RemoteConnection (if `JTAGClientBrowser.getLocalSystem()` returns null)

**globalThis.jtag Status**: ✅ **SET** at `browser-index.ts:52` after connection completes

**sharedInstance Behavior**: ✅ Works - resolves after `globalThis.jtag` is set

**Events.emit() auto-context**: ✅ Works - can call `sharedInstance` after initialization

---

### CASE 2: Browser LocalConnection (browser client → local browser system)

**Initialization Flow**:
```typescript
// browser-index.ts:27-62
async connect() {
  // 1. Register widgets
  BROWSER_WIDGETS.forEach(widget => customElements.define(...));

  // 2. Connect client
  const connectionResult = await JTAGClientBrowser.connectLocal();
  //   → JTAGClient.initialize() finds local JTAGSystemBrowser
  //   → Creates LocalConnection to local system
  const client = connectionResult.client;

  // 3. ✅ SET globalThis.jtag AFTER connection completes
  (globalThis as any).jtag = client;

  return { ...connectionResult, client };
}
```

**Connection Type**: LocalConnection (if `JTAGSystemBrowser.connect()` returns system instance)

**globalThis.jtag Status**: ✅ **SET** at `browser-index.ts:52` after connection completes

**sharedInstance Behavior**: ✅ Works - resolves after `globalThis.jtag` is set

**Events.emit() auto-context**: ✅ Works - can call `sharedInstance` after initialization

---

### CASE 3: Server RemoteConnection (server client → remote server via transport)

**Initialization Flow**:
```typescript
// server-index.ts:11-26
async connect(options?) {
  const targetEnv = options?.targetEnvironment ?? 'server';
  console.log(`🔌 Server: Connecting via JTAGClientServer (target: ${targetEnv})`);

  const connectionResult = await JTAGClientServer.connectRemote({
    targetEnvironment: targetEnv,
    sessionId: SYSTEM_SCOPES.UNKNOWN_SESSION
  });

  console.log(`✅ Server: JTAGClient connected with ${connectionResult.listResult.totalCount} commands`);

  // ❌ NO globalThis.jtag assignment!
  return connectionResult.client;
}
```

**Connection Type**: RemoteConnection (no local system available)

**globalThis.jtag Status**: ❌ **NEVER SET** - `server-index.ts` doesn't assign it!

**sharedInstance Behavior**: ❌ **INFINITE POLLING** - `globalThis.jtag?.commands` never resolves

**Events.emit() auto-context**: ❌ **HANGS FOREVER** - waits for sharedInstance that never resolves

---

### CASE 4: Server LocalConnection (server client → local server system)

**Initialization Flow**:
```typescript
// server-index.ts:11-26
async connect(options?) {
  const targetEnv = options?.targetEnvironment ?? 'server';
  console.log(`🔌 Server: Connecting via JTAGClientServer (target: ${targetEnv})`);

  const connectionResult = await JTAGClientServer.connectRemote({
    targetEnvironment: targetEnv,
    sessionId: SYSTEM_SCOPES.UNKNOWN_SESSION
  });
  //   → JTAGClient.initialize() checks getLocalSystem()
  //   → JTAGClientServer.getLocalSystem() returns JTAGSystemServer.instance
  //   → Creates LocalConnection to local system

  console.log(`✅ Server: JTAGClient connected with ${connectionResult.listResult.totalCount} commands`);

  // ❌ NO globalThis.jtag assignment!
  return connectionResult.client;
}
```

**Connection Type**: LocalConnection (if `JTAGSystemServer.instance` exists in same process)

**globalThis.jtag Status**: ❌ **NEVER SET** - `server-index.ts` doesn't assign it!

**sharedInstance Behavior**: ❌ **INFINITE POLLING** - `globalThis.jtag?.commands` never resolves

**Events.emit() auto-context**: ❌ **HANGS FOREVER** - waits for sharedInstance that never resolves

---

## The sharedInstance Implementation

```typescript
// system/core/client/shared/JTAGClient.ts:810-822
/**
 * Get shared instance from global context - works in browser and server
 * Browser: (window as WindowWithJTAG).jtag
 * Server: (globalThis as any).jtag
 */
static get sharedInstance(): Promise<JTAGClient> {
  return new Promise((resolve) => {
    const checkReady = (): void => {
      const jtag = (globalThis as any).jtag;  // ⚠️ Polls globalThis.jtag
      if (jtag?.commands) {
        resolve(jtag);
      } else {
        setTimeout(checkReady, 50);  // ⚠️ Polls every 50ms FOREVER
      }
    };
    checkReady();
  });
}
```

**Comment Claims**: "works in browser and server"
**Reality**: Only works in browser where `globalThis.jtag` is set!

---

## The Events.emit() Auto-Context Problem

```typescript
// system/core/shared/Events.ts:53-75
static async emit<T>(
  contextOrEventName: JTAGContext | string,
  eventNameOrData: string | T,
  eventDataOrOptions?: T | EventEmitOptions,
  optionsParam?: EventEmitOptions
): Promise<{ success: boolean; error?: string }> {
  try {
    let context: JTAGContext;
    let eventName: string;
    let eventData: T;
    let options: EventEmitOptions;

    if (typeof contextOrEventName === 'string') {
      // Form 1: emit(eventName, data, options?) - AUTO-CONTEXT
      // Auto-discover context from JTAGClient.sharedInstance
      const { JTAGClient } = await import('../client/shared/JTAGClient');
      const client = await JTAGClient.sharedInstance;  // ⚠️ DEADLOCK ON SERVER!
      context = client.context;
      eventName = contextOrEventName;
      eventData = eventNameOrData as T;
      options = (eventDataOrOptions as EventEmitOptions) ?? {};
    } else {
      // Form 2: emit(context, eventName, data, options?) - EXPLICIT CONTEXT
      context = contextOrEventName;
      eventName = eventNameOrData as string;
      eventData = eventDataOrOptions as T;
      options = optionsParam ?? {};
    }
    // ... rest of implementation
  }
}
```

**Two Forms**:
1. **Auto-context**: `await Events.emit('eventName', data)` → calls `sharedInstance` → **HANGS ON SERVER**
2. **Explicit context**: `await Events.emit(context, 'eventName', data)` → bypasses `sharedInstance` → ✅ **WORKS**

---

## The PersonaUser Failure Pattern

**Current Broken Pattern** (11 locations in PersonaUser.ts):
```typescript
// PersonaUser.ts:447 (and 10 other locations)
if (this.client) {
  (this.client.events as unknown as ScopedEventsInterface).room(messageEntity.roomId).emit(
    AI_DECISION_EVENTS.EVALUATING,
    eventData
  );
}
```

**Problems**:
1. Type-casts to `ScopedEventsInterface` which doesn't exist on `client.events`
2. Calls `.room()` method which doesn't exist
3. Results in `TypeError: this.client.events.room is not a function`

**Attempted Fix** (FAILED):
```typescript
// What I tried (PersonaUser completely stopped working)
await this.client.daemons.events.emit(AI_DECISION_EVENTS.EVALUATING, eventData);
```

**Why It Failed**:
```typescript
// JTAGClient.ts:885-892
daemons: {
  events: {
    emit: async <T>(eventName: string, data: T, options?) => {
      const { Events } = await import('../../shared/Events');
      return await Events.emit(eventName, data, options || {});  // ⚠️ AUTO-CONTEXT FORM!
    }
  }
}
```

This uses **auto-context form** → calls `sharedInstance` → infinite polling on server!

**Correct Fix** (not yet implemented):
```typescript
// PersonaUser should use explicit context form
await Events.emit(this.context, AI_DECISION_EVENTS.EVALUATING, eventData, {
  scope: EVENT_SCOPES.ROOM,
  scopeId: messageEntity.roomId,
  sessionId: this.sessionId
});
```

This bypasses `sharedInstance` entirely and works in all four cases.

---

## The Circular Dependency During Initialization

**Joel's Insight**: "The issue is more inside that method itself or how the connection happens. It's perfectly reasonable for most callers to wait for connection to be made, aka JTAGClient.sharedInstance. The issue is this itself causing the deadlock recursive case, when connection is dependent on itself."

**Current State**: NO circular dependency during initialization because:
- `browser-index.ts` sets `globalThis.jtag` AFTER connection completes
- No events are emitted during `JTAGClient.initialize()` that use auto-context form
- If they were, we'd have deadlock: initialize() → emit event → sharedInstance → wait for initialize() to complete → deadlock

**Future Risk**: If we add event emissions during initialization that use auto-context form, we WILL deadlock.

---

## Design Requirements for Solution

### 1. Symmetric JTAGClient.sharedInstance

**Must work for ALL FOUR cases**:
- ✅ Browser RemoteConnection
- ✅ Browser LocalConnection
- ✅ Server RemoteConnection (currently broken)
- ✅ Server LocalConnection (currently broken)

**Constraints**:
- Cannot rely on `globalThis.jtag` being set (server never sets it)
- Must handle initialization timing (don't deadlock if called during connection)
- Must be fast (no 50ms polling loops)
- Must be deterministic (no race conditions)

### 2. Universal Events.emit() and Events.on()

**User's Requirement**: "All cases should use Events.emit and you need to figure out how to make this happen without deadlock"

**Must support**:
- Auto-context form: `await Events.emit('event', data)` for convenience
- Explicit context form: `await Events.emit(context, 'event', data)` for control
- Both forms must work in all four connection cases
- No deadlocks during initialization

### 3. Consistent Commands.execute() Pattern

**Already working** (JTAGClient.ts:830-845):
```typescript
daemons: {
  commands: {
    execute: async <T, U>(command: string, params?: T): Promise<U> => {
      const response = await this.commands[command](params);

      // Unwrap CommandResponse if needed
      if (response && typeof response === 'object' && 'commandResult' in response) {
        return (response as CommandSuccessResponse).commandResult as U;
      }

      return response as U;
    }
  }
}
```

**Does NOT use sharedInstance** - uses `this.commands` directly
**Pattern to follow** for Events API!

---

## Proposed Solution Architecture

### Option A: Registry-Based sharedInstance (Preferred)

**Concept**: Maintain a static registry of clients, keyed by context or environment

```typescript
// JTAGClient.ts
export class JTAGClient extends JTAGBase {
  private static clientRegistry: Map<string, JTAGClient> = new Map();

  static registerClient(key: string, client: JTAGClient): void {
    this.clientRegistry.set(key, client);
  }

  static get sharedInstance(): Promise<JTAGClient> {
    return new Promise((resolve, reject) => {
      // 1. Check registry first (works for server LocalConnection)
      const registeredClient = this.clientRegistry.get('default');
      if (registeredClient) {
        resolve(registeredClient);
        return;
      }

      // 2. Check globalThis.jtag (works for browser after initialization)
      const jtag = (globalThis as any).jtag;
      if (jtag?.commands) {
        resolve(jtag);
        return;
      }

      // 3. Poll with timeout (fallback, but with safety limit)
      let attempts = 0;
      const maxAttempts = 100; // 5 seconds max
      const checkReady = (): void => {
        attempts++;

        // Check registry again
        const client = this.clientRegistry.get('default');
        if (client) {
          resolve(client);
          return;
        }

        // Check globalThis again
        const jtagNow = (globalThis as any).jtag;
        if (jtagNow?.commands) {
          resolve(jtagNow);
          return;
        }

        // Timeout after max attempts
        if (attempts >= maxAttempts) {
          reject(new Error('JTAGClient.sharedInstance: No client available after 5s timeout'));
          return;
        }

        setTimeout(checkReady, 50);
      };
      checkReady();
    });
  }
}
```

**Registration Points**:
```typescript
// browser-index.ts:52
(globalThis as any).jtag = client;
JTAGClient.registerClient('default', client);  // NEW!

// server-index.ts:23
JTAGClient.registerClient('default', connectionResult.client);  // NEW!
return connectionResult.client;
```

**Benefits**:
- ✅ Works for all four cases
- ✅ No infinite polling
- ✅ Symmetric between browser and server
- ✅ Fast (registry lookup is O(1))
- ✅ Deterministic (no race conditions)

**Drawbacks**:
- Need to register clients manually
- Static registry shared across all contexts (need key strategy)

### Option B: Context-Aware Lookup

**Concept**: Pass context through the call chain, never rely on global state

```typescript
// Events.ts
static async emit<T>(
  contextOrEventName: JTAGContext | string,
  eventNameOrData: string | T,
  eventDataOrOptions?: T | EventEmitOptions,
  optionsParam?: EventEmitOptions
): Promise<{ success: boolean; error?: string }> {
  let context: JTAGContext;

  if (typeof contextOrEventName === 'string') {
    // Auto-context form - NO LONGER USES sharedInstance!
    // Instead, use AsyncLocalStorage or similar context tracking
    const { getActiveContext } = await import('../context/ContextTracker');
    context = getActiveContext();
    if (!context) {
      throw new Error('Events.emit: No active context and none provided');
    }
    // ... rest of logic
  } else {
    // Explicit context form
    context = contextOrEventName;
    // ... rest of logic
  }
}
```

**Benefits**:
- ✅ No global state dependency
- ✅ Works in all environments
- ✅ Clean architecture

**Drawbacks**:
- ❌ Requires AsyncLocalStorage (Node.js 14+)
- ❌ More complex to implement
- ❌ May not work in Worker Threads without passing context explicitly

### Option C: Explicit Context Everywhere (Nuclear Option)

**Concept**: Remove auto-context form entirely, always require explicit context

```typescript
// NO AUTO-CONTEXT FORM!
await Events.emit(this.context, 'event', data, options);  // Always explicit
```

**Benefits**:
- ✅ Simple
- ✅ No ambiguity
- ✅ Works everywhere

**Drawbacks**:
- ❌ Breaks existing code that uses auto-context form
- ❌ Less ergonomic
- ❌ User explicitly wants auto-context to work: "All cases should use Events.emit"

---

## Recommendation

**OPTION A: Registry-Based sharedInstance**

**Implementation Steps**:
1. Add static `clientRegistry` Map to JTAGClient
2. Add `registerClient(key, client)` static method
3. Modify `sharedInstance` getter to check registry first, then globalThis, then poll with timeout
4. Update `browser-index.ts` to register client in registry
5. Update `server-index.ts` to register client in registry
6. Test all four connection cases
7. Fix PersonaUser to use auto-context form (now safe)
8. Deploy and validate AI responses work

**Timeline**: 2-3 hours implementation + testing

**Risk**: Low - additive change, doesn't break existing code

---

## Testing Strategy

### Test Case 1: Browser RemoteConnection + Auto-Context Events
```typescript
// browser-index.ts already sets globalThis.jtag
const client = await jtag.connect();
await Events.emit('test:event', { test: true });  // Should work
```

### Test Case 2: Browser LocalConnection + Auto-Context Events
```typescript
const client = await jtag.connect();
await Events.emit('test:event', { test: true });  // Should work
```

### Test Case 3: Server RemoteConnection + Auto-Context Events
```typescript
// server-index.ts needs to register client
const client = await jtag.connect();
await Events.emit('test:event', { test: true });  // Currently hangs, should work after fix
```

### Test Case 4: Server LocalConnection + Auto-Context Events
```typescript
// System already running, client connects to local system
const client = await jtag.connect();
await Events.emit('test:event', { test: true });  // Currently hangs, should work after fix
```

### Test Case 5: PersonaUser Event Emission
```typescript
// PersonaUser.ts (Worker Thread on server)
await Events.emit(this.context, AI_DECISION_EVENTS.EVALUATING, eventData, {
  scope: EVENT_SCOPES.ROOM,
  scopeId: roomId
});
// Should work in ALL cases (use explicit context to avoid Worker Thread issues)
```

---

## Related Documentation

- `PERSONA-USER-EVENTS-FIX-ANALYSIS.md` - PersonaUser event emission issues
- `JTAGCLIENT-EVENTS-ARCHITECTURE.md` - Complete Events and JTAGClient analysis
- `CLAUDE.md` - User instructions and architecture overview

---

## Status

- [x] Problem identified and documented
- [x] All four connection cases analyzed
- [x] Root cause identified (globalThis.jtag not set on server)
- [ ] Solution designed (Option A: Registry-Based sharedInstance)
- [ ] Implementation started
- [ ] Tests passing
- [ ] Deployed and validated
