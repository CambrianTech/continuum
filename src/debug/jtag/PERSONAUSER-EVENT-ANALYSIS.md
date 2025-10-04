# PersonaUser Event System Analysis

## Problem Statement

PersonaUsers are NOT receiving `data:chat_messages:created` events despite:
- ✅ Events being emitted correctly (`data:chat_messages:created`)
- ✅ PersonaUsers subscribing with correct event name (using `DataEventNames.created(COLLECTIONS.CHAT_MESSAGES)`)
- ✅ PersonaUsers having room memberships (2 out of 3 load rooms correctly)
- ✅ Event code executing (logs show emission)

## Root Cause Hypothesis

**EventManager Instance Isolation** - PersonaUsers create separate EventManager instances that don't share event subscriptions with the system EventManager that emits events.

### Evidence

1. **BaseUser.subscribeToChatEvents()** (`BaseUser.ts:169-181`):
```typescript
protected subscribeToChatEvents(handler: (message: any) => Promise<void>): void {
  const { EventManager } = require('../../events/shared/JTAGEventSystem');
  const eventManager = new EventManager();  // ❌ NEW INSTANCE!

  const eventName = DataEventNames.created(COLLECTIONS.CHAT_MESSAGES);
  eventManager.events.on(eventName, async (messageData: any) => {
    if (this.myRoomIds.has(messageData.roomId)) {
      await handler(messageData);
    }
  });
}
```

2. **Event Emission** happens via EventsDaemon which likely has its OWN EventManager instance

3. **Pattern**: Each `new EventManager()` creates ISOLATED event bus - subscriptions don't cross boundaries

## Architecture Understanding

### How Events SHOULD Work

1. **Server emits event** → DataCreateServerCommand emits via EventsDaemon
2. **EventsDaemon broadcasts** → Uses system EventManager singleton
3. **PersonaUsers subscribed** → Listen on SAME EventManager instance
4. **Handler fires** → PersonaUser receives event and responds

### What's Actually Happening

1. **Server emits event** → EventsDaemon uses EventManager instance A
2. **PersonaUser subscribes** → Creates EventManager instance B (different!)
3. **Event never reaches PersonaUser** → Instances A and B don't communicate

## Solution Approaches

### Option 1: Shared EventManager Singleton (Recommended)

Make EventManager a true singleton or provide shared instance via dependency injection.

```typescript
// In EventManager or EventSystemTypes
export function getSharedEventManager(): EventManager {
  if (!globalThis.__JTAG_EVENT_MANAGER__) {
    globalThis.__JTAG_EVENT_MANAGER__ = new EventManager();
  }
  return globalThis.__JTAG_EVENT_MANAGER__;
}

// In BaseUser
protected subscribeToChatEvents(handler: (message: any) => Promise<void>): void {
  const eventManager = getSharedEventManager();  // ✅ SHARED INSTANCE
  const eventName = DataEventNames.created(COLLECTIONS.CHAT_MESSAGES);
  eventManager.events.on(eventName, async (messageData: any) => {
    if (this.myRoomIds.has(messageData.roomId)) {
      await handler(messageData);
    }
  });
}
```

### Option 2: Pass EventManager via Dependency Injection

PersonaUsers receive EventManager via UserDaemon during creation.

```typescript
// In UserDaemonServer
const systemEventManager = this.getSystemEventManager();
const personaUser = new PersonaUser(userEntity, userState, storage, personaClient, systemEventManager);

// In PersonaUser
constructor(
  entity: UserEntity,
  state: UserStateEntity,
  storage: IUserStateStorage,
  client?: JTAGClient,
  eventManager?: EventManager  // ✅ INJECTED
) {
  super(entity, state, storage, eventManager);
  this.client = client;
}

// In BaseUser
protected subscribeToChatEvents(handler: (message: any) => Promise<void>): void {
  const eventManager = this.eventManager || getSharedEventManager();
  // ... rest of subscription
}
```

### Option 3: EventsDaemon Provides Subscription API

Instead of PersonaUsers creating EventManager, they ask EventsDaemon to subscribe them.

```typescript
// Via JTAGClient
this.client.daemons.events.on(
  DataEventNames.created(COLLECTIONS.CHAT_MESSAGES),
  this.handleChatMessage.bind(this)
);
```

## Recommended Implementation

**Use Option 3** - PersonaUsers subscribe via their JTAGClient's EventsDaemon accessor.

**Why:**
1. ✅ Already have JTAGClient dependency injection pattern working
2. ✅ EventsDaemon is the source of truth for events
3. ✅ Type-safe via JTAGClient.daemons.events API
4. ✅ No global singletons needed
5. ✅ Clean dependency flow: PersonaUser → JTAGClient → EventsDaemon → Shared EventManager

**Implementation:**
1. Update `PersonaUser.initialize()` to subscribe via `this.client.daemons.events.on()`
2. Remove `new EventManager()` from BaseUser subscription helpers
3. Update BaseUser helpers to accept EventManager parameter OR use client.daemons.events

## Next Steps

1. Verify EventsDaemon has `on()` method accessible via client.daemons.events
2. Update PersonaUser to subscribe via client
3. Test event delivery
4. Update BaseUser helpers to use injected EventManager or client
5. Document pattern in DAEMON-RESPONSIBILITIES.md

## Key Insight

**PersonaUsers operate INSIDE the system** - they should use the system's EventManager via their JTAGClient, not create separate instances. The client is their window into the system's event bus.
