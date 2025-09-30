# Events System Unification Plan

## Current State Analysis

### What Exists ✅
```typescript
// Events.ts - Universal static interface
Events.emit<T>(eventName, data, options) // Works in both
Events.subscribe<T>(pattern, listener, filter) // Browser-focused

// EventsDaemon - Cross-context bridging
EventsDaemonBrowser - Handles DOM events
EventsDaemonServer - Handles server-side events

// Event Bridge - Cross-environment propagation
- Browser → Server bridging ✅
- Server → Browser bridging ✅
- Recursion prevention ✅
```

### What's Inconsistent ❌
1. **subscribe() is browser-only** - Uses `document.addEventListener`
2. **No unified registration API** - Each environment handles differently
3. **JTAGClient.daemons.events not implemented** - Throws "not implemented"
4. **Pattern matching only in browser** - Wildcard/elegant subscriptions browser-only

## Goal: Identical API in Both Environments

### Unified Interface
```typescript
const jtag = await JTAGClient.sharedInstance;

// 1. Emit events (ALREADY WORKS!)
await jtag.daemons.events.emit('data:User:created', userData);
await Events.emit('data:User:created', userData); // Alternative static API

// 2. Subscribe to events (NEEDS UNIFICATION)
const unsubscribe = jtag.daemons.events.on('data:User:created', (data) => {
  console.log('User created:', data);
});

// 3. Pattern subscriptions (NEEDS UNIFICATION)
jtag.daemons.events.on('data:users {created,updated}', (event) => {
  console.log(event.action, event.entity); // Enhanced event data
});

// 4. Unsubscribe (NEEDS UNIFICATION)
unsubscribe(); // Clean up
jtag.daemons.events.off('data:User:created', handler);
```

## Architecture: How Events Flow

### Browser Emits → Server Receives
```
1. Browser: Events.emit('data:User:created', user)
2. → EventsDaemonBrowser receives
3. → Routes via WebSocket to server
4. → EventsDaemonServer receives
5. → Triggers server-side subscribers
6. → Also dispatches to local EventManager
```

### Server Emits → Browser Receives
```
1. Server: Events.emit('data:Room:updated', room)
2. → EventsDaemonServer receives
3. → Routes via WebSocket to browser
4. → EventsDaemonBrowser receives
5. → Dispatches as DOM CustomEvent
6. → Browser subscribers triggered
```

### Local Emit (Same Environment)
```
1. Browser: Events.emit('ui:theme-changed', theme)
2. → EventsDaemonBrowser receives
3. → Dispatches as DOM CustomEvent locally
4. → ALSO bridges to server (if not already bridged)
5. → Browser subscribers triggered immediately
```

## Problem: subscription() Environment Differences

### Browser (Current)
```typescript
Events.subscribe('data:User:created', (data) => {
  // Uses document.addEventListener
  // Wildcard patterns work
  // Elegant patterns work
});
```

### Server (Missing)
```typescript
Events.subscribe('data:User:created', (data) => {
  // ❌ No document object
  // ❌ Need EventManager subscription
  // ❌ Wildcards don't work
  // ❌ Elegant patterns don't work
});
```

## Solution: Unified Subscription Manager

### Create EventSubscriptionManager (Environment-Agnostic)
```typescript
// system/events/shared/EventSubscriptionManager.ts

export class EventSubscriptionManager {
  private subscriptions = new Map<string, Set<Function>>();
  private wildcardSubscriptions = new Map<string, { pattern: RegExp, handlers: Set<Function> }>();
  private elegantSubscriptions = new Map<string, { pattern: ParsedPattern, handlers: Set<Function> }>();

  /**
   * Subscribe to an event - works in browser and server
   */
  on<T>(
    patternOrEventName: string,
    handler: (data: T) => void
  ): () => void {
    // Parse pattern
    if (isElegantPattern(patternOrEventName)) {
      return this.subscribeElegant(patternOrEventName, handler);
    } else if (hasWildcard(patternOrEventName)) {
      return this.subscribeWildcard(patternOrEventName, handler);
    } else {
      return this.subscribeExact(patternOrEventName, handler);
    }
  }

  /**
   * Unsubscribe from an event
   */
  off(eventName: string, handler?: Function): void {
    if (!handler) {
      // Remove all handlers for this event
      this.subscriptions.delete(eventName);
    } else {
      // Remove specific handler
      const handlers = this.subscriptions.get(eventName);
      handlers?.delete(handler);
    }
  }

  /**
   * Trigger all matching subscribers for an event
   */
  trigger(eventName: string, data: any): void {
    // 1. Exact match subscribers
    const exactHandlers = this.subscriptions.get(eventName);
    exactHandlers?.forEach(handler => {
      try {
        handler(data);
      } catch (error) {
        console.error(`❌ Event handler error for ${eventName}:`, error);
      }
    });

    // 2. Wildcard pattern matches
    this.checkWildcardMatches(eventName, data);

    // 3. Elegant pattern matches
    this.checkElegantMatches(eventName, data);
  }
}
```

### Browser Implementation
```typescript
// daemons/events-daemon/browser/EventsDaemonBrowser.ts

export class EventsDaemonBrowser extends EventsDaemon {
  private subscriptionManager = new EventSubscriptionManager();

  protected handleLocalEventBridge(eventName: string, eventData: unknown): void {
    // 1. Dispatch DOM event (backward compatibility)
    const domEvent = new CustomEvent(eventName, {
      detail: eventData,
      bubbles: true
    });
    document.dispatchEvent(domEvent);

    // 2. Trigger subscription manager (new unified API)
    this.subscriptionManager.trigger(eventName, eventData);
  }

  // Expose subscription manager to JTAGClient
  public getSubscriptionManager(): EventSubscriptionManager {
    return this.subscriptionManager;
  }
}
```

### Server Implementation
```typescript
// daemons/events-daemon/server/EventsDaemonServer.ts

export class EventsDaemonServer extends EventsDaemon {
  private subscriptionManager = new EventSubscriptionManager();

  protected handleLocalEventBridge(eventName: string, eventData: unknown): void {
    // Trigger subscription manager (unified with browser!)
    this.subscriptionManager.trigger(eventName, eventData);
  }

  // Expose subscription manager to JTAGClient
  public getSubscriptionManager(): EventSubscriptionManager {
    return this.subscriptionManager;
  }
}
```

## Unified JTAGClient.daemons.events Interface

### Implementation in JTAGClient
```typescript
// system/core/client/shared/JTAGClient.ts

get daemons() {
  return {
    commands: { ... }, // Already implemented

    events: {
      /**
       * Emit an event - works in browser and server
       */
      emit: async <T>(
        eventName: string,
        data: T,
        options?: EventEmitOptions
      ): Promise<{ success: boolean; error?: string }> => {
        return await Events.emit(eventName, data, options);
      },

      /**
       * Subscribe to events - works in browser and server
       */
      on: <T>(
        patternOrEventName: string,
        handler: (data: T) => void
      ): () => void => {
        // Get EventsDaemon subscription manager
        const eventsDaemon = this.getEventsDaemon();
        return eventsDaemon.getSubscriptionManager().on(patternOrEventName, handler);
      },

      /**
       * Unsubscribe from events
       */
      off: (eventName: string, handler?: Function): void => {
        const eventsDaemon = this.getEventsDaemon();
        eventsDaemon.getSubscriptionManager().off(eventName, handler);
      }
    },

    data: { ... } // TODO
  };
}

/**
 * Get EventsDaemon instance from local system
 */
private getEventsDaemon(): EventsDaemonBrowser | EventsDaemonServer {
  const localSystem = (this.connection as LocalConnection)?.localSystem;
  if (!localSystem) {
    throw new Error('EventsDaemon only available in local connections');
  }
  return localSystem.daemons.events as any;
}
```

## Implementation Steps

### Step 1: Create EventSubscriptionManager ✅
- [ ] Create `system/events/shared/EventSubscriptionManager.ts`
- [ ] Implement exact, wildcard, and elegant pattern matching
- [ ] Add trigger() method for event dispatch
- [ ] Unit tests for pattern matching

### Step 2: Integrate into EventsDaemon ✅
- [ ] Add `subscriptionManager` to EventsDaemonBrowser
- [ ] Add `subscriptionManager` to EventsDaemonServer
- [ ] Update `handleLocalEventBridge` to trigger manager
- [ ] Expose `getSubscriptionManager()` method

### Step 3: Implement JTAGClient.daemons.events ✅
- [ ] Add `events.emit()` using Events.emit()
- [ ] Add `events.on()` using subscription manager
- [ ] Add `events.off()` using subscription manager
- [ ] Handle both local and remote connections

### Step 4: Test Symmetry ✅
```typescript
// Browser test
const jtag = await JTAGClient.sharedInstance;
const unsubscribe = jtag.daemons.events.on('data:User:created', (user) => {
  console.log('Browser received:', user);
});
await jtag.daemons.events.emit('data:User:created', { id: '123', name: 'Joel' });

// Server test (SAME CODE!)
const jtag = await JTAGClient.sharedInstance;
const unsubscribe = jtag.daemons.events.on('data:User:created', (user) => {
  console.log('Server received:', user);
});
await jtag.daemons.events.emit('data:User:created', { id: '123', name: 'Joel' });
```

## Success Criteria

✅ Same `daemons.events` API in browser and server
✅ `emit()` works cross-environment (already does!)
✅ `on()` works with exact, wildcard, and elegant patterns
✅ `off()` cleans up subscriptions properly
✅ Events bridge between environments automatically
✅ Pattern matching works identically in both environments
✅ Zero breaking changes to existing Events.emit() callers

## Benefits

1. **Consistency** - Same event API everywhere
2. **Type Safety** - Full generic support for event data
3. **Pattern Matching** - Elegant subscriptions work in server too
4. **No DOM Dependency** - Server doesn't need `document`
5. **Backward Compatible** - Events.emit/subscribe still work
6. **Easy Migration** - `JTAGClient.daemons.events` is the new way