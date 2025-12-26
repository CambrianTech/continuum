# Event Architecture - Comprehensive Guide

## Universal Event System Location
**File**: `/system/core/shared/Events.ts`
**Router Registry**: `/system/core/shared/RouterRegistry.ts`

## Event Naming Convention

### Data Events (CRUD Operations)
**Pattern**: `data:{collection}:{operation}`

**Collections**:
- `users` - User entities (human, agent, persona)
- `rooms` - Chat room entities
- `chat_messages` - Chat message entities
- `user_states` - User state entities
- `ContentType` - Content type entities
- `TrainingSession` - Training session entities

**Operations**:
- `created` - Entity created
- `updated` - Entity updated
- `deleted` - Entity deleted
- `truncated` - All entities in collection cleared

**Examples**:
```typescript
data:users:created
data:rooms:updated
data:chat_messages:deleted
data:users:truncated
```

### UI/Local Events (Browser-only)
**Pattern**: `{domain}:{action}`

**Current UI Events**:
- `room:selected` - User selected a different chat room
  - Payload: `{ roomId: string, roomName: string }`
  - Emitted by: RoomListWidget, RoomStateManager
  - Subscribed by: ChatWidget, RoomStateManager

### System Events
**Pattern**: `system:{event}`

**Future system events** (not yet implemented):
- `system:ready` - System initialization complete
- `system:shutdown` - System shutting down
- `system:error` - System-level error

---

## Event Emission Points

### 1. DataDaemon (Primary Data Layer)
**File**: `/daemons/data-daemon/shared/DataDaemon.ts`

**✅ Currently Emits Events**:
```typescript
// Instance methods with event emission:
async create<T>()    → data:{collection}:created
async update<T>()    → data:{collection}:updated
async delete()       → data:{collection}:deleted
async truncate()     → data:{collection}:truncated

// Static methods delegate to instance methods (events work automatically)
static async store<T>()   → calls instance create()
static async update<T>()  → calls instance update()
static async remove()     → calls instance delete()
static async truncate()   → calls instance truncate()
```

**❌ Missing Event Emission**:
```typescript
async clear()        → Should emit data:*:cleared
async clearAll()     → Should emit data:*:cleared + details
async batch()        → Should emit events for each operation in batch
```

**Design Decision**: Batch operations should emit individual CRUD events for each operation, not a single `batch:complete` event, so widgets stay in sync.

### 2. Widget Layer (UI Events)
**Files**:
- `/widgets/chat/room-list/RoomListWidget.ts`
- `/widgets/shared/RoomStateManager.ts`

**Emits**:
```typescript
Events.emit('room:selected', { roomId, roomName })
```

**Why browser-only**: Room selection is local UI state, doesn't need server persistence.

---

## Event Subscription Points

### 1. Widget Event Subscriptions (via EntityScrollerWidget)
**File**: `/widgets/shared/EntityScrollerWidget.ts`

**Pattern**: All entity-based widgets automatically subscribe via `createEntityCrudHandler()`

```typescript
// Automatic subscriptions for:
data:{collection}:created   → scroller.add(entity)
data:{collection}:updated   → scroller.update(id, entity)
data:{collection}:deleted   → scroller.remove(id)
data:{collection}:truncated → scroller.clear()
```

**Widgets using this**:
- `UserListWidget` → subscribes to `data:users:{created,updated,deleted,truncated}`
- `RoomListWidget` → subscribes to `data:rooms:{created,updated,deleted,truncated}`
- `ChatWidget` → subscribes to `data:chat_messages:{created,updated,deleted,truncated}`

### 2. Direct Widget Subscriptions
**Files**:
- `/widgets/chat/chat-widget/ChatWidget.ts`
- `/widgets/shared/RoomStateManager.ts`

```typescript
// ChatWidget subscribes to room selection
Events.subscribe('room:selected', (data) => this.loadMessagesForRoom(data.roomId))

// Also subscribes via EntityScrollerWidget for messages
Events.subscribe('data:ChatMessage:created', (msg) => ...)

// RoomStateManager subscribes to own events for state sync
Events.subscribe('room:selected', (room) => this.updateState(room))
```

### 3. Utility Subscriptions (createEntityCrudHandler)
**File**: `/commands/data/shared/DataEventUtils.ts`

```typescript
// Subscribes to ALL CRUD operations for a collection
subscribeToAllCrudEvents('User', (user, action) => {
  // action: 'created' | 'updated' | 'deleted' | 'truncated'
})

// Or specific operations
subscribeToSpecificCrudEvent('User', 'updated', (user) => ...)

// Or selected operations
subscribeToSelectedCrudEvents('User', ['created', 'deleted'], (user, action) => ...)
```

---

## Event Flow Architecture

### Server-Originated CRUD Events
```
┌─────────────────────────────────────────────────────┐
│ 1. Command Layer (data/create, data/update, etc.)  │
│    ↓                                                │
│ 2. DataDaemon instance method (create/update/del)  │
│    ├─→ Persist to SQLite                           │
│    └─→ Events.emit(context, eventName, entity)     │
│        ↓                                            │
│ 3. RouterRegistry.getForContext(context)           │
│    └─→ Router.postMessage() → EventBridge          │
│        ↓                                            │
│ 4. EventsDaemonServer broadcasts to all clients    │
│    ↓                                                │
│ 5. EventsDaemonBrowser receives event              │
│    ├─→ Triggers DOM event listeners                │
│    └─→ Calls Events.subscribe() callbacks          │
│        ↓                                            │
│ 6. Widget handlers receive event                   │
│    └─→ EntityScroller.add/update/remove/clear()    │
│        └─→ UI updates automatically                │
└─────────────────────────────────────────────────────┘
```

### Browser-Local UI Events
```
┌─────────────────────────────────────────────────────┐
│ 1. User interaction (click room in list)           │
│    ↓                                                │
│ 2. Widget emits: Events.emit('room:selected', data) │
│    ↓                                                │
│ 3. No router found (browser-local event)           │
│    └─→ Falls back to DOM event dispatch            │
│        ↓                                            │
│ 4. document.dispatchEvent(CustomEvent)             │
│    ↓                                                │
│ 5. Other widgets listen via Events.subscribe()     │
│    └─→ Triggers wildcard subscriptions             │
│    └─→ Triggers direct DOM listeners               │
│        ↓                                            │
│ 6. Widgets update UI based on new room selection   │
└─────────────────────────────────────────────────────┘
```

---

## Event Categories & Naming Strategy

### 1. CRUD Events (Server → All Clients)
**Must** go through server persistence first, then emit.

**Naming**: `data:{collection}:{operation}`
- Always originates from DataDaemon after successful DB operation
- Broadcasts to all connected clients
- Used for keeping UI in sync with database

### 2. UI State Events (Browser-local only)
**Never** persisted to database.

**Naming**: `{domain}:{action}`
- Examples: `room:selected`, `theme:changed`, `sidebar:toggled`
- Stays within browser (no server round-trip)
- Uses DOM events as fallback when no router available

### 3. System Events (Global scope)
**Both** server and browser.

**Naming**: `system:{event}`
- Examples: `system:ready`, `system:error`, `system:shutdown`
- Coordinated lifecycle events
- May or may not require server persistence depending on event

---

## Missing Event Emissions (TODO)

### DataDaemon Methods Without Events
```typescript
// Should these emit events?
async clear()        // Clears ALL data - should emit data:*:cleared?
async clearAll()     // Same but with reporting
async batch()        // Should emit individual CRUD events for each op?
```

**Design Question**: Should `batch()` emit individual events or a single `batch:complete` event?

**Recommendation**: Emit individual CRUD events so widgets stay in sync per-entity, not just "batch done".

---

## Event Subscription Patterns

### Pattern 1: Wildcard (Future Enhancement)
```typescript
Events.subscribe('data:*:created', (entity) => {
  // Any entity created in any collection
})

Events.subscribe('data:users:*', (user) => {
  // Any operation on users
})
```

### Pattern 2: Brace Expansion (Future Enhancement)
```typescript
Events.subscribe('data:users:{created,updated}', (user) => {
  // User created OR updated
})
```

### Pattern 3: Filtered Subscriptions (Future Enhancement)
```typescript
Events.subscribe('data:rooms', (room) => {
  // Only public rooms
}, { where: { public: true } })
```

**Current Status**: Patterns 1-3 are documented in Events.ts but not fully implemented. Basic subscriptions work today.

---

## Router Registry Pattern

### Why RouterRegistry?
Enables **automatic router discovery** from context without manual passing of router/commander objects.

**Before** (manual router passing):
```typescript
await Events.emit(eventName, data, this.context, this.commander)
```

**After** (automatic discovery):
```typescript
await Events.emit(eventName, data)  // Auto-discovers context & router
// OR
await Events.emit(this.context, eventName, data)  // Explicit context
```

### How It Works
```typescript
// During daemon initialization:
RouterRegistry.register(context, router)

// During Events.emit():
const router = RouterRegistry.getForContext(context)
if (router) {
  // Use EventBridge routing
} else if (isBrowser) {
  // Fall back to DOM-only events
} else {
  // Error: no router in server environment
}
```

---

## Type Safety

### Event Data Types
```typescript
// CRUD events use BaseEntity extensions
interface UserEntity extends BaseEntity {
  displayName: string;
  type: 'human' | 'agent' | 'persona';
  // ...
}

// UI events have custom payloads
interface RoomSelectedEvent {
  roomId: string;
  roomName: string;
}

// Events.emit() is generic
await Events.emit<UserEntity>('data:users:created', userEntity)
await Events.emit<RoomSelectedEvent>('room:selected', { roomId, roomName })
```

### Subscription Type Safety
```typescript
// Type-safe subscriptions
Events.subscribe<UserEntity>('data:users:created', (user) => {
  console.log(user.displayName)  // TypeScript knows the shape
})

Events.subscribe<RoomSelectedEvent>('room:selected', (data) => {
  console.log(data.roomId)  // Type-checked
})
```

---

## Best Practices

### ✅ DO:
- Emit CRUD events from DataDaemon layer only (single source of truth)
- Use auto-context form: `Events.emit('eventName', data)` when possible
- Subscribe to events in widget `connectedCallback()` or `setupEventSubscriptions()`
- Unsubscribe in widget `disconnectedCallback()` or cleanup methods
- Use TypeScript generics for type-safe event payloads
- Emit events AFTER successful database operations
- Use consistent naming: `data:{collection}:{operation}`

### ❌ DON'T:
- Emit CRUD events from command layer (DataDaemon handles it)
- Emit events before database operations complete
- Use `any` types for event payloads
- Subscribe without unsubscribing (memory leaks)
- Mix browser-local and server-persisted events (name clearly!)
- Use dynamic event names (hard to track subscriptions)

---

## Testing Event System

### Test Truncate Event (Example)
```bash
# 1. Start system
npm start

# 2. Trigger truncate
./jtag data/truncate --collection=users

# 3. Check logs for event emission
grep "DataDaemon: Emitted" .continuum/*/logs/server-console-log.log

# 4. Check logs for widget receiving event
grep "Clearing all entities" .continuum/*/logs/browser-console-log.log

# 5. Verify UI updated
./jtag interface/screenshot --querySelector="user-list-widget"
```

### Test CRUD Event Flow
```bash
# Create entity
./jtag data/create --collection=users --data='{"displayName":"Test","type":"human"}'

# Check server emitted
grep "DataDaemon: Emitted data:users:created" .continuum/*/logs/server-console-log.log

# Check browser received
grep "DataEventUtils: CRUD event received - users.created" .continuum/*/logs/browser-console-log.log

# Check widget updated
grep "EntityScroller created for users" .continuum/*/logs/browser-console-log.log
```

---

## Future Enhancements

### 1. Event History & Replay
Store events for offline sync, debugging, time-travel debugging.

### 2. Event Filtering at Router Level
Filter events before broadcasting to reduce network traffic.

### 3. Event Priority & Ordering
Guarantee event order for critical operations (e.g., delete before create).

### 4. Event Compression
Batch multiple events into single message for performance.

### 5. Event Acknowledgment
Confirm clients received/processed events for reliability.

### 6. Scoped Event Routing
Route events only to relevant contexts (per-room, per-user).

---

## Event Audit Checklist

- [x] DataDaemon.create() emits events
- [x] DataDaemon.update() emits events
- [x] DataDaemon.delete() emits events
- [x] DataDaemon.truncate() emits events
- [ ] DataDaemon.clear() emits events
- [ ] DataDaemon.clearAll() emits events
- [ ] DataDaemon.batch() emits events per operation
- [x] Widgets subscribe via EntityScrollerWidget
- [x] Router Registry auto-discovery works
- [x] Browser-local events fallback to DOM
- [x] Events.emit() has type safety
- [x] Events.subscribe() has type safety
- [ ] Wildcard subscriptions fully implemented
- [ ] Brace expansion subscriptions fully implemented
- [ ] Filtered subscriptions fully implemented
