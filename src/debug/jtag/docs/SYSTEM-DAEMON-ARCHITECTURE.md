# SystemDaemon Architecture

**Status**: ✅ Implemented and Deployed
**Location**: `daemons/system-daemon/shared/SystemDaemon.ts`
**Integration**: DataDaemonServer.ts:88-91

## Purpose

SystemDaemon provides efficient, cached access to system configuration with clean event-driven invalidation. It eliminates repeated database queries for configuration access and serves as the single source of truth for runtime system settings.

## Architecture

### Singleton Pattern
```typescript
const daemon = SystemDaemon.sharedInstance();
const config = daemon.getConfig(); // Instant - no DB query
```

### Initialization Flow
1. **DataDaemonServer startup** → Initializes SystemDaemon
2. **Query or create** → Loads `SystemConfigEntity` (name='default')
3. **Cache in memory** → Stores config entity
4. **Subscribe to events** → Filtered subscription: `data:system_config:updated` where `name='default'`
5. **Serve requests** → All config access uses cached entity

### Key Features

1. **Query Once, Serve Forever**
   - Single database query on startup
   - All subsequent access reads from memory cache
   - O(1) lookup time for any setting

2. **Clean Cache Invalidation**
   - Subscribes to filtered update events
   - Only receives updates for the singleton config (name='default')
   - Automatically refreshes cache when config changes

3. **Type-Safe API**
   ```typescript
   // Get entire config
   const config = daemon.getConfig();

   // Get specific setting
   const interval = daemon.getSetting('system/scheduling/timings/adapter-health-check');

   // Update setting (persists + emits event)
   await daemon.updateSetting(path, value, userId, reason);

   // Reset to factory default
   await daemon.resetSetting(path, userId);

   // Reset entire group
   await daemon.resetGroup('system/scheduling/timings', userId);

   // Get all settings under a group
   const schedulingSettings = daemon.getGroup('system/scheduling');
   ```

4. **System State Tracking**
   ```typescript
   // Update runtime state (not settings)
   await daemon.updateSystemState({
     currentLoad: 0.8,
     activeAICount: 15
   });

   // Get current state
   const state = daemon.getSystemState();
   ```

## Event Architecture Investigation

### Current Event Flow (PROBLEM IDENTIFIED)

**DataDaemon emits** (DataDaemon.ts:182-184):
```typescript
const eventName = getDataEventName(collection, 'created');
await Events.emit(DataDaemon.jtagContext, eventName, entity);
```

**Events.emit broadcasts** (Events.ts:114-141):
```typescript
await router.postMessage(eventMessage); // ❌ Goes to ALL clients via WebSocket
```

### The Chaos: O(n×m) Broadcast Storm

Every data operation broadcasts to:
- All browser tabs
- All PersonaUsers
- All server-side subscribers
- Regardless of relevance

**Example**: `data:chat_messages:created` for room="general" goes to:
- Chat widget for "academy" room (irrelevant!)
- All PersonaUsers (even if not in room!)
- All browser tabs (even viewing different content!)

### Solution: Filtered Event Subscriptions

EventSubscriptionManager DOES support filtering (line 85-88), but:
1. **Events.emit() doesn't apply filters before broadcast**
2. **Filtering happens at subscriber side** (too late - wasted bandwidth)
3. **Need scope-based routing** in EventBridge layer

### SystemDaemon's Approach (CORRECT)

```typescript
await Events.subscribe(
  'data:system_config:updated',
  (entity) => this.onConfigUpdated(entity),
  { where: { name: 'default' } } // ✅ Filter subscription
);
```

This uses the filter parameter, but the filter is applied CLIENT-SIDE after receiving the event. The broader fix requires EventBridge to:
1. Parse event data for scope/entity IDs
2. Route BEFORE WebSocket transmission
3. Only send events to relevant subscribers

## Integration Points

### DataDaemonServer
```typescript
// Initialize SystemDaemon for efficient system config access
const { SystemDaemon } = await import('../../system-daemon/shared/SystemDaemon');
await SystemDaemon.initialize(this.context);
log.info('System daemon initialized');
```

### Future: SystemSchedulingState
**TODO**: Update SystemSchedulingState to use SystemDaemon instead of querying DataDaemon directly.

```typescript
// OLD (repeated DB queries)
const result = await Commands.execute('data/list', {
  collection: SystemConfigEntity.collection,
  filter: { name: 'default' }
});

// NEW (cached, instant)
const { SystemDaemon } = await import('../../../daemons/system-daemon/shared/SystemDaemon');
const daemon = SystemDaemon.sharedInstance();
const config = daemon.getConfig();
```

## Benefits

1. **Performance**:
   - Eliminates repeated DB queries
   - O(1) config access time
   - Concurrent-safe (singleton)

2. **Reliability**:
   - Single source of truth
   - Clean cache invalidation
   - Type-safe API

3. **Maintainability**:
   - Centralized config management
   - Clear ownership (server-side only)
   - Event-driven architecture

4. **Scalability**:
   - No DB load for config reads
   - Filtered event subscriptions
   - Efficient memory usage

## Outstanding Issues

1. **EventBridge Broadcast Storm**: Need to fix router-level event filtering (separate from SystemDaemon)
2. **SystemSchedulingState Migration**: Update to use SystemDaemon (blocked by old architecture)
3. **SystemSchedulingConfigEntity vs SystemConfigEntity**: Two config systems exist, need to consolidate

## setTimeout Violations Found (2025-12-04)

### Problem: "Bugs and slowness" caused by setTimeout/setInterval violations

User reported: "a lot of bugs and slowness" and "we tried to create easy room switching via tabs and failed because of system inefficiencies"

**Root Cause Analysis:**

#### BaseAIProviderAdapter.ts (Lines 30, 111, 161)
```typescript
// Line 30: setInterval type stored (violation)
private healthMonitorInterval?: ReturnType<typeof setInterval>;

// Line 111: setInterval for health monitoring (30 second interval)
this.healthMonitorInterval = setInterval(async () => {
  await this.checkHealthAndRecover();
}, this.HEALTH_CHECK_INTERVAL); // 30000ms

// Line 161: setTimeout for 3 second stabilization wait
await new Promise(resolve => setTimeout(resolve, 3000));
```

#### SentinelAdapter.ts (Lines 112, 140)
```typescript
// Line 112: setTimeout for 2 second wait in server startup
while (Date.now() - startTime < maxWaitTime) {
  await new Promise(resolve => setTimeout(resolve, 2000));
  // ... health check ...
}

// Line 140: setTimeout for 2 second wait after killing server
this.serverProcess.kill();
await new Promise(resolve => setTimeout(resolve, 2000));
```

### Impact

1. **Health Monitoring Chaos**: Every adapter runs independent setInterval for health checks (no coordination)
2. **Restart Wait Patterns**: Hard-coded setTimeout waits during provider restarts (blocks event loop)
3. **Startup Delays**: Multiple 2-second waits during initialization (compounds across adapters)
4. **Room Switching Slowness**: setTimeout delays accumulate, causing UI lag

### Solution: AdapterHealthMonitor with Entity-Based Scheduling

Replace adapter-local setInterval with system-level BaseSleepingEntity:

```typescript
// NEW: AdapterHealthMonitor extends BaseSleepingEntity
export class AdapterHealthMonitor extends BaseSleepingEntity {
  protected async doWork(): Promise<void> {
    // Check all adapters in sequence
    for (const adapter of this.adapters) {
      await adapter.healthCheck();
    }
  }

  protected getSleepDuration(): number {
    // Uses SystemDaemon.getSetting('system/scheduling/timings/adapter-health-check')
    // Adaptive cadence based on system load
    return SystemSchedulingState.instance.getRecommendedCadence('adapter-health-check');
  }
}
```

**Benefits:**
- Single health monitor for ALL adapters (no setInterval spam)
- Adaptive cadence based on system load
- Configuration via SystemConfigEntity (runtime tunable)
- Clean shutdown (stopWorking() vs clearInterval())
- System-wide coordination

### Refactoring Tasks

1. ✅ SystemDaemon implemented with cached config access
2. ⏳ Create AdapterHealthMonitor using BaseSleepingEntity pattern
3. ⏳ Refactor BaseAIProviderAdapter:
   - Remove setInterval health monitoring
   - Remove setTimeout wait patterns
   - Health checks initiated by AdapterHealthMonitor
4. ⏳ Refactor SentinelAdapter:
   - Replace setTimeout waits with event-driven startup
   - Use BaseSleepingEntity for server readiness polling
5. ⏳ Update SystemConfigEntity with adapter-specific settings:
   - `system/scheduling/timings/adapter-health-check`
   - `system/adapters/sentinel/startup-timeout`
   - `system/adapters/sentinel/restart-stabilization-delay`

## Files Changed

- ✅ `daemons/system-daemon/shared/SystemDaemon.ts` (Created)
- ✅ `daemons/data-daemon/server/DataDaemonServer.ts` (Modified - lines 88-91)
- ✅ `system/data/entities/SystemConfigEntity.ts` (Read - confirmed decorator pattern)

## Testing

**Manual Verification**:
```bash
npm run build:ts  # ✅ Compiles successfully
npm start         # ✅ Deploys with SystemDaemon initialization
```

**System Logs** (DataDaemonServer initialization):
```
Data daemon server initialized with SQLite backend
Code daemon initialized
System daemon initialized  ← NEW
Emitted system:ready event
```

## Next Steps

1. **Implement AdapterHealthMonitor** using SystemDaemon for config access
2. **Fix EventBridge routing** to filter before broadcast
3. **Migrate SystemSchedulingState** to use SystemDaemon
4. **Consolidate config systems** (SystemSchedulingConfigEntity → SystemConfigEntity)
