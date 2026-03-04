# Daemon Base Class Extraction - Pattern Analysis

**Date**: 2025-12-08
**Goal**: Identify common daemon patterns for extraction into DaemonBase to improve stability and responsiveness

## Executive Summary

After analyzing 13+ daemons for logging standardization, clear patterns emerged around:
- Event subscription management
- Interval lifecycle management
- Cleanup coordination

**3 out of 13 daemons** explicitly implement cleanup patterns. These patterns should be extracted into DaemonBase to:
1. Reduce code duplication
2. Prevent resource leaks
3. Ensure consistent cleanup behavior
4. Make daemon development easier

## Pattern Discovery Process

### Step 1: Identify Daemons with Cleanup
```bash
grep -r "async cleanup()" daemons/*/server/*.ts
# Found 6 files

grep -r "unsubscribe" daemons/*/server/*.ts
# Found 3 files with cleanup patterns
```

### Step 2: Analyze Common Patterns
Read and compared:
- **UserDaemonServer.ts** (472 lines) - Most complex lifecycle
- **TrainingDaemonServer.ts** (332 lines) - Event-driven cleanup
- **RoomMembershipDaemonServer.ts** (262 lines) - Simple cleanup

### Step 3: Identify DaemonBase Gaps
- DaemonBase has `shutdown()` stub but no cleanup helpers
- Every daemon reimplements event subscription tracking
- Interval management done manually in each daemon

---

## Common Pattern 1: Event Subscription Management

### Current Implementation (3 daemons duplicate this)

**UserDaemonServer.ts** (lines 32, 76, 108, 114, 120, 459-463):
```typescript
export class UserDaemonServer extends UserDaemon {
  private unsubscribeFunctions: (() => void)[] = [];

  // Store unsubscribe function
  private subscribeToSystemReady(): void {
    const unsubReady = Events.subscribe('system:ready', async (payload: any) => {
      // ... handler logic ...
    });
    this.unsubscribeFunctions.push(unsubReady);  // ← Pattern
  }

  // Cleanup
  async shutdown(): Promise<void> {
    await super.shutdown();
    for (const unsubscribe of this.unsubscribeFunctions) {
      unsubscribe();
    }
    this.unsubscribeFunctions = [];
  }
}
```

**TrainingDaemonServer.ts** (lines 55, 127, 324-330):
```typescript
export class TrainingDaemonServer extends TrainingDaemon {
  private unsubscribeFunctions: (() => void)[] = [];

  private async setupEventSubscriptions(): Promise<void> {
    const unsubCreated = Events.subscribe<ChatMessageEntity>(
      DATA_EVENTS.CHAT_MESSAGES.CREATED,
      async (messageEntity: ChatMessageEntity) => {
        await this.handleMessageCreated(messageEntity);
      }
    );
    this.unsubscribeFunctions.push(unsubCreated);  // ← Pattern
  }

  async cleanup(): Promise<void> {
    this.log.info('🧠 TrainingDaemon: Cleaning up subscriptions...');
    for (const unsub of this.unsubscribeFunctions) {
      unsub();
    }
    this.unsubscribeFunctions = [];
  }
}
```

**RoomMembershipDaemonServer.ts** (lines 34, 135, 255-259):
```typescript
export class RoomMembershipDaemonServer extends RoomMembershipDaemon {
  private unsubscribeFunctions: (() => void)[] = [];

  private async setupEventSubscriptions(): Promise<void> {
    const unsubCreated = Events.subscribe<UserEntity>(
      DATA_EVENTS.USERS.CREATED,
      async (userData: UserEntity) => {
        await this.handleUserCreated(userData);
      }
    );
    this.unsubscribeFunctions.push(unsubCreated);  // ← Pattern
  }

  async shutdown(): Promise<void> {
    this.unsubscribeFunctions.forEach(unsub => unsub());
    this.unsubscribeFunctions = [];
    await super.shutdown();
  }
}
```

### Proposed DaemonBase Enhancement

**DaemonBase.ts** (proposed additions):
```typescript
export abstract class DaemonBase extends JTAGModule implements MessageSubscriber {
  protected log: DaemonLogger;

  // NEW: Event subscription tracking
  private unsubscribeFunctions: (() => void)[] = [];

  /**
   * Register an event subscription for automatic cleanup
   * Called by subclasses when subscribing to events
   */
  protected registerSubscription(unsubscribe: () => void): void {
    this.unsubscribeFunctions.push(unsubscribe);
  }

  /**
   * Unsubscribe from all registered events
   * Called automatically during shutdown
   */
  protected cleanupSubscriptions(): void {
    this.log.debug(`Cleaning up ${this.unsubscribeFunctions.length} event subscription(s)`);
    for (const unsub of this.unsubscribeFunctions) {
      try {
        unsub();
      } catch (error) {
        this.log.error(`Failed to unsubscribe:`, error);
      }
    }
    this.unsubscribeFunctions = [];
  }

  async shutdown(): Promise<void> {
    this.log.info(`🔄 ${this.toString()}: Shutting down...`);

    // Call subclass cleanup first
    await this.cleanup();

    // Then automatic cleanup
    this.cleanupSubscriptions();
  }

  /**
   * Override in subclasses for custom cleanup logic
   * Called before automatic cleanup in shutdown()
   */
  protected async cleanup(): Promise<void> {
    // Default: no-op
  }
}
```

### Benefits
- ✅ Eliminates duplicate code across 3+ daemons
- ✅ Prevents subscription leaks (guaranteed cleanup)
- ✅ Consistent cleanup order (subclass first, then automatic)
- ✅ Error handling for failed unsubscribes
- ✅ Debug logging for cleanup operations

### Migration Example

**Before** (UserDaemonServer.ts):
```typescript
export class UserDaemonServer extends UserDaemon {
  private unsubscribeFunctions: (() => void)[] = [];

  private subscribeToSystemReady(): void {
    const unsubReady = Events.subscribe('system:ready', async (payload: any) => {
      // ... handler ...
    });
    this.unsubscribeFunctions.push(unsubReady);
  }

  async shutdown(): Promise<void> {
    await super.shutdown();
    for (const unsubscribe of this.unsubscribeFunctions) {
      unsubscribe();
    }
    this.unsubscribeFunctions = [];
    // ... persona cleanup ...
  }
}
```

**After** (using DaemonBase helpers):
```typescript
export class UserDaemonServer extends UserDaemon {
  // No more unsubscribeFunctions field!

  private subscribeToSystemReady(): void {
    const unsubReady = Events.subscribe('system:ready', async (payload: any) => {
      // ... handler ...
    });
    this.registerSubscription(unsubReady);  // ← Use base class method
  }

  // Override cleanup for persona-specific logic
  protected async cleanup(): Promise<void> {
    // Shutdown all persona clients
    for (const userId of this.personaClients.keys()) {
      // TODO: Add shutdown method to PersonaUser
    }
    this.personaClients.clear();
  }

  // shutdown() is now inherited and automatic!
}
```

---

## Common Pattern 2: Interval Management

### Current Implementation (1 daemon, but pattern should be universal)

**UserDaemonServer.ts** (lines 30-31, 340-356, 435-451):
```typescript
export class UserDaemonServer extends UserDaemon {
  private monitoringInterval?: ReturnType<typeof setInterval>;
  private reconciliationInterval?: ReturnType<typeof setInterval>;

  protected startMonitoringLoops(): boolean {
    // User monitoring loop - every 5 seconds
    this.monitoringInterval = setInterval(() => {
      this.runUserMonitoringLoop().catch((error: Error) => {
        this.log.error('❌ UserDaemon: Monitoring loop error:', error);
      });
    }, 5000);

    // State reconciliation loop - every 30 seconds
    this.reconciliationInterval = setInterval(() => {
      this.runStateReconciliationLoop().catch((error: Error) => {
        this.log.error('❌ UserDaemon: Reconciliation loop error:', error);
      });
    }, 30000);

    return true;
  }

  protected stopMonitoringLoops(): boolean {
    let stopped = false;

    if (this.monitoringInterval) {
      clearInterval(this.monitoringInterval);
      this.monitoringInterval = undefined;
      stopped = true;
    }

    if (this.reconciliationInterval) {
      clearInterval(this.reconciliationInterval);
      this.reconciliationInterval = undefined;
      stopped = true;
    }

    return stopped;
  }
}
```

### Proposed DaemonBase Enhancement

**DaemonBase.ts** (proposed additions):
```typescript
export abstract class DaemonBase extends JTAGModule implements MessageSubscriber {
  // NEW: Interval tracking
  private intervals: Map<string, ReturnType<typeof setInterval>> = new Map();

  /**
   * Register a named interval for automatic cleanup
   * Replaces manual interval tracking in each daemon
   */
  protected registerInterval(
    name: string,
    callback: () => void | Promise<void>,
    intervalMs: number
  ): void {
    // Clear existing interval with same name
    this.clearInterval(name);

    const interval = setInterval(() => {
      const result = callback();
      if (result instanceof Promise) {
        result.catch((error: Error) => {
          this.log.error(`Interval '${name}' error:`, error);
        });
      }
    }, intervalMs);

    this.intervals.set(name, interval);
    this.log.debug(`Registered interval '${name}' (${intervalMs}ms)`);
  }

  /**
   * Clear a specific named interval
   */
  protected clearInterval(name: string): boolean {
    const interval = this.intervals.get(name);
    if (interval) {
      clearInterval(interval);
      this.intervals.delete(name);
      this.log.debug(`Cleared interval '${name}'`);
      return true;
    }
    return false;
  }

  /**
   * Clear all registered intervals
   * Called automatically during shutdown
   */
  protected cleanupIntervals(): void {
    this.log.debug(`Cleaning up ${this.intervals.size} interval(s)`);
    for (const [name, interval] of this.intervals) {
      clearInterval(interval);
      this.log.debug(`Cleared interval '${name}'`);
    }
    this.intervals.clear();
  }

  async shutdown(): Promise<void> {
    this.log.info(`🔄 ${this.toString()}: Shutting down...`);

    // Call subclass cleanup first
    await this.cleanup();

    // Then automatic cleanup
    this.cleanupSubscriptions();
    this.cleanupIntervals();  // ← NEW
  }
}
```

### Benefits
- ✅ Named intervals (self-documenting)
- ✅ Automatic error handling for async callbacks
- ✅ Prevents interval leaks
- ✅ Consistent cleanup order
- ✅ Easier testing (can mock intervals by name)

### Migration Example

**Before** (UserDaemonServer.ts):
```typescript
export class UserDaemonServer extends UserDaemon {
  private monitoringInterval?: ReturnType<typeof setInterval>;
  private reconciliationInterval?: ReturnType<typeof setInterval>;

  protected startMonitoringLoops(): boolean {
    this.monitoringInterval = setInterval(() => {
      this.runUserMonitoringLoop().catch((error: Error) => {
        this.log.error('❌ UserDaemon: Monitoring loop error:', error);
      });
    }, 5000);

    this.reconciliationInterval = setInterval(() => {
      this.runStateReconciliationLoop().catch((error: Error) => {
        this.log.error('❌ UserDaemon: Reconciliation loop error:', error);
      });
    }, 30000);

    return true;
  }

  protected stopMonitoringLoops(): boolean {
    // ... manual cleanup ...
  }
}
```

**After** (using DaemonBase helpers):
```typescript
export class UserDaemonServer extends UserDaemon {
  // No more interval fields!

  protected startMonitoringLoops(): void {
    this.registerInterval('user-monitoring', async () => {
      await this.runUserMonitoringLoop();
    }, 5000);

    this.registerInterval('state-reconciliation', async () => {
      await this.runStateReconciliationLoop();
    }, 30000);
  }

  // stopMonitoringLoops() no longer needed - automatic cleanup!
}
```

---

## Common Pattern 3: System Ready Coordination

### Current Implementation

**UserDaemonServer.ts** (lines 65-83):
```typescript
export class UserDaemonServer extends UserDaemon {
  private subscribeToSystemReady(): void {
    const unsubReady = Events.subscribe('system:ready', async (payload: any) => {
      if (payload?.daemon === 'data') {
        this.log.info('📡 UserDaemon: Received system:ready from DataDaemon, initializing personas...');

        await this.ensurePersonaClients().catch((error: Error) => {
          this.log.error('❌ UserDaemon: Failed to initialize persona clients:', error);
        });
      }
    });
    this.unsubscribeFunctions.push(unsubReady);

    // Initialize ToolRegistry immediately
    this.initializeToolRegistry().catch((error: Error) => {
      this.log.error('❌ UserDaemon: Failed to initialize ToolRegistry:', error);
    });
  }
}
```

**RoomMembershipDaemonServer.ts** (lines 79-84):
```typescript
export class RoomMembershipDaemonServer extends RoomMembershipDaemon {
  async initialize(): Promise<void> {
    await this.setupEventSubscriptions();

    // Defer catch-up logic until after DataDaemon is ready
    setTimeout(() => {
      this.ensureAllUsersInRooms().catch(error => {
        this.log.error('❌ RoomMembershipDaemon: Deferred catch-up failed:', error);
      });
    }, 2000); // 2 second delay
  }
}
```

### Proposed DaemonBase Enhancement

**DaemonBase.ts** (proposed additions):
```typescript
export abstract class DaemonBase extends JTAGModule implements MessageSubscriber {
  /**
   * Wait for another daemon to be ready before executing callback
   * Useful for initialization dependencies (e.g., wait for DataDaemon)
   */
  protected onDaemonReady(
    daemonName: string,
    callback: () => Promise<void>
  ): void {
    const unsub = Events.subscribe('system:ready', async (payload: any) => {
      if (payload?.daemon === daemonName) {
        this.log.info(`📡 ${this.toString()}: ${daemonName} is ready`);
        try {
          await callback();
        } catch (error) {
          this.log.error(`Failed to handle ${daemonName} ready:`, error);
        }
      }
    });
    this.registerSubscription(unsub);
  }

  /**
   * Defer execution until after initialization completes
   * Alternative to setTimeout with better logging
   */
  protected deferInitialization(
    callback: () => Promise<void>,
    delayMs: number = 2000
  ): void {
    setTimeout(async () => {
      try {
        await callback();
      } catch (error) {
        this.log.error('Deferred initialization failed:', error);
      }
    }, delayMs);
  }
}
```

### Benefits
- ✅ Declarative daemon dependencies
- ✅ Consistent logging for initialization
- ✅ Error handling built-in
- ✅ Replaces magic setTimeout with named intent

### Migration Example

**Before** (UserDaemonServer.ts):
```typescript
export class UserDaemonServer extends UserDaemon {
  private subscribeToSystemReady(): void {
    const unsubReady = Events.subscribe('system:ready', async (payload: any) => {
      if (payload?.daemon === 'data') {
        this.log.info('📡 UserDaemon: Received system:ready from DataDaemon...');
        await this.ensurePersonaClients().catch((error: Error) => {
          this.log.error('❌ UserDaemon: Failed to initialize persona clients:', error);
        });
      }
    });
    this.unsubscribeFunctions.push(unsubReady);
  }
}
```

**After** (using DaemonBase helpers):
```typescript
export class UserDaemonServer extends UserDaemon {
  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    // Declarative dependency
    this.onDaemonReady('data', async () => {
      await this.ensurePersonaClients();
    });
  }
}
```

---

## Implementation Plan

### Phase 1: Add Helpers to DaemonBase (Non-Breaking)
**Goal**: Add new methods without changing existing daemons

**Changes**:
1. Add `unsubscribeFunctions` field to DaemonBase
2. Add `intervals` map to DaemonBase
3. Add `registerSubscription()` method
4. Add `cleanupSubscriptions()` method
5. Add `registerInterval()` method
6. Add `cleanupIntervals()` method
7. Add `onDaemonReady()` helper
8. Add `deferInitialization()` helper
9. Enhance `shutdown()` to call cleanup helpers
10. Add `cleanup()` abstract method (optional override)

**Files**:
- `daemons/command-daemon/shared/DaemonBase.ts` (~150 lines added)

**Validation**:
- ✅ TypeScript compilation passes
- ✅ Existing daemons unaffected (backwards compatible)
- ✅ Deploy and verify system works

### Phase 2: Migrate UserDaemonServer (Proof of Concept)
**Goal**: Demonstrate benefits with most complex daemon

**Changes**:
1. Remove `unsubscribeFunctions` field
2. Replace `.push(unsub)` with `this.registerSubscription(unsub)`
3. Remove `monitoringInterval` and `reconciliationInterval` fields
4. Replace `setInterval()` with `this.registerInterval(name, callback, ms)`
5. Remove `stopMonitoringLoops()` method (automatic)
6. Simplify `shutdown()` to call `super.shutdown()` only
7. Override `cleanup()` for persona-specific logic
8. Replace system ready subscription with `this.onDaemonReady('data', callback)`

**Files**:
- `daemons/user-daemon/server/UserDaemonServer.ts` (~50 lines removed)

**Validation**:
- ✅ npm run build:ts passes
- ✅ Deploy and verify PersonaUsers still initialize
- ✅ Verify intervals still run
- ✅ Verify cleanup works on shutdown

### Phase 3: Migrate Remaining Daemons (Incremental)
**Goal**: Apply pattern to all daemons with cleanup

**Daemons to migrate**:
1. TrainingDaemonServer (lines 55, 127, 324-330)
2. RoomMembershipDaemonServer (lines 34, 135, 255-259)

**Process**:
- Migrate one daemon at a time
- Test between migrations
- Commit after each successful migration

**Expected savings**:
- ~30-50 lines removed per daemon
- Eliminates 3+ duplicate implementations
- Consistent cleanup behavior across all daemons

### Phase 4: Documentation and Convention
**Goal**: Make pattern standard for all future daemons

**Updates**:
1. Update `docs/ARCHITECTURE-RULES.md` with daemon lifecycle patterns
2. Add section to `CLAUDE.md` about daemon base class usage
3. Create daemon creation guide showing proper subscription/interval usage
4. Update generator templates to use base class helpers

---

## Expected Outcomes

### Quantitative Benefits
- **Code reduction**: ~150-200 lines eliminated across 3-4 daemons
- **Pattern consistency**: 100% of daemons use same cleanup mechanism
- **Bug prevention**: Zero subscription/interval leaks guaranteed

### Qualitative Benefits
- **Easier daemon development**: No need to remember cleanup patterns
- **Better debugging**: Centralized logging for all cleanup operations
- **Improved stability**: Automatic cleanup prevents resource leaks
- **Clearer intent**: Named intervals and declarative dependencies

### Risks and Mitigations
**Risk**: Breaking existing daemons during migration
**Mitigation**: Phase 1 is backwards compatible, migrate incrementally

**Risk**: Introducing new bugs in base class
**Mitigation**: Comprehensive testing with UserDaemonServer first

**Risk**: Over-abstraction making code harder to understand
**Mitigation**: Keep helpers simple, document with examples

---

## Appendix: Full Daemon Inventory

| Daemon | Has Cleanup? | Event Subscriptions? | Intervals? | Priority |
|--------|--------------|---------------------|-----------|----------|
| UserDaemonServer | ✅ Yes | 3 subscriptions | 2 intervals | HIGH - Complex |
| TrainingDaemonServer | ✅ Yes | 1 subscription | None | HIGH - Pattern match |
| RoomMembershipDaemonServer | ✅ Yes | 1 subscription | None | HIGH - Pattern match |
| DataDaemonServer | ❌ No | None | None | LOW |
| CommandDaemonServer | ❌ No | None | None | LOW |
| EventsDaemonServer | ❌ No | None | None | LOW |
| SessionDaemonServer | ❌ No | None | None | LOW |
| WidgetDaemonServer | ❌ No | None | None | LOW |
| HealthDaemonServer | ❌ No | None | None | LOW |
| ProxyDaemonServer | ❌ No | None | None | LOW |
| LeaseDaemonServer | ❌ No | None | None | LOW |
| CodeDaemonServer | ❌ No | None | None | LOW |
| ConsoleDaemonServer | ❓ Special | None | None | N/A - Intentional |

**Conclusion**: 3 out of 13 daemons (23%) explicitly need cleanup. Extracting pattern to base class benefits these immediately and prepares for future daemons.

---

## Next Steps

1. **Review this document** - Validate patterns and approach
2. **Implement Phase 1** - Add helpers to DaemonBase (non-breaking)
3. **Test with UserDaemonServer** - Prove pattern works with complex daemon
4. **Migrate remaining daemons** - TrainingDaemonServer, RoomMembershipDaemonServer
5. **Update documentation** - Make pattern standard for all future daemons

**Estimated effort**: 2-3 hours for implementation, 1 hour for testing, 1 hour for documentation

**Impact**: Foundation for improved daemon stability, responsiveness, and maintainability
