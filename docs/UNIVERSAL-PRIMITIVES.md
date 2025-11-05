# Universal Primitives: The E = mc² of Continuum

**Everything in this system is built on just TWO primitives.**

These primitives work **identically everywhere** - browser, server, CLI, and tests. They are type-safe, transparent (local or remote doesn't matter), and infinitely composable.

---

## Primitive 1: Commands (Request/Response)

**Concept**: Execute any command with full type safety, regardless of where you are or where the system is running.

### Basic Usage

```typescript
// Connect to system (local or remote - doesn't matter)
const client = await jtag.connect();

// Execute commands with type inference
const users = await client.commands['data/list']<DataListResult>({
  collection: 'users',
  filter: { isActive: true },
  limit: 10
});

const screenshot = await client.commands['screenshot']<ScreenshotResult>({
  querySelector: 'chat-widget',
  filename: 'debug.png'
});

const aiResponse = await client.commands['ai/generate']<AIGenerationResult>({
  model: 'phi3:mini',
  prompt: 'Explain TypeScript generics',
  maxTokens: 500
});
```

### How It Works

1. **Dynamic Discovery**: When client connects, server sends list of all available commands
2. **Type-Safe Interface**: Client builds typed command interface at runtime
3. **Transparent Routing**:
   - **Local connection**: Commands execute directly in same process
   - **Remote connection**: Commands route over WebSocket automatically
4. **Universal**: Same API works in browser widgets, server scripts, CLI tools, and tests

### Architecture

```
Client Side                          Server Side
───────────                         ───────────
client.commands['ping']({})         ┌──────────────┐
        ↓                           │CommandDaemon │
  Is connection local?              │  (router)    │
        ↓                           └──────────────┘
    ┌───┴────┐                            ↓
  YES      NO                       Find command
    ↓        ↓                            ↓
Local Call  WebSocket              ┌──────────────┐
    ↓        ↓                     │PingCommand   │
    └────────┴─────→ Execute ──→   │.execute()    │
                                   └──────────────┘
                                         ↓
                                   Return result
```

### Key Features

- **Type-Safe**: Full TypeScript inference for params and return types
- **Universal**: Works identically in browser, server, CLI, and tests
- **Dynamic**: Command list discovered at runtime (no hard-coding)
- **Transparent**: Local calls are direct, remote calls use WebSocket
- **Discoverable**: Use `client.commands['list']({})` to see all commands
- **Extensible**: Add new commands without modifying client code

### Common Commands

```typescript
// Data operations
await client.commands['data/list']({ collection: 'users' });
await client.commands['data/create']({ collection: 'users', data: newUser });
await client.commands['data/update']({ collection: 'users', id: userId, data: updates });
await client.commands['data/delete']({ collection: 'users', id: userId });

// System commands
await client.commands['ping']({});
await client.commands['list']({});  // List all commands
await client.commands['system/daemons']({});  // Show daemon status

// AI commands
await client.commands['ai/generate']({ model: 'phi3:mini', prompt: 'Hello' });
await client.commands['ai/should-respond']({ message, roomId, personaId });
await client.commands['ai/report']({});  // AI performance metrics

// Debug commands
await client.commands['screenshot']({ querySelector: 'body' });
await client.commands['debug/logs']({ tailLines: 50, includeErrorsOnly: true });
await client.commands['debug/widget-events']({ widgetSelector: 'chat-widget' });
```

---

## Primitive 2: Events (Publish/Subscribe)

**Concept**: Subscribe to events from anywhere, emit events from anywhere. Events are delivered both locally (same process) and remotely (via EventBridge over WebSocket).

### Basic Usage

```typescript
// Subscribe to events (from anywhere: browser, server, CLI, tests)
Events.subscribe('data:users:created', (user: UserEntity) => {
  console.log('New user created:', user.displayName);
});

// Wildcard subscriptions
Events.subscribe('data:*:created', (data: any) => {
  console.log('Something was created:', data);
});

// Emit events (from anywhere)
Events.emit('data:users:created', newUser);
```

### How It Works

1. **Local Subscriptions**: Stored in static class properties (process-local Maps)
2. **Local Delivery**: `Events.emit()` calls `checkWildcardSubscriptions()` directly
3. **Remote Delivery**: `Events.emit()` also sends event via EventBridge over WebSocket
4. **Remote Reception**: When event arrives via WebSocket, `handleTransportMessage()` calls `checkWildcardSubscriptions()`
5. **Universal**: Works everywhere after Events Ubiquity fix (2025-10-29)

### Architecture

```
Emitter Process                     Receiver Process
───────────────                    ─────────────────
Events.emit('data:users:created', user)
     ↓
 ┌───┴────────┐
 │ Trigger    │                    ┌──────────────┐
 │ Local      │                    │WebSocket     │
 │ Subscribers│                    │Transport     │
 └────────────┘                    └──────────────┘
     ↓                                    ↓
 ┌───┴────────┐                   ┌──────────────┐
 │ Send via   │ ──────────────→   │Receive Event │
 │EventBridge │    WebSocket       │Message       │
 └────────────┘                    └──────────────┘
                                         ↓
                                  handleTransportMessage()
                                         ↓
                               checkWildcardSubscriptions()
                                         ↓
                                  Trigger subscribers
```

### Event Naming Conventions

Events use a hierarchical naming pattern:

```
<domain>:<collection>:<operation>
```

Examples:
- `data:users:created` - User entity created
- `data:chat_messages:updated` - Chat message updated
- `system:daemon:started` - Daemon started
- `ai:thought:broadcasted` - AI broadcasted a thought
- `widget:mounted` - Widget mounted to DOM

**Wildcards**:
- `data:*:created` - Any entity created
- `data:users:*` - Any user operation
- `*` - All events (use sparingly!)

### Key Features

- **Universal**: Works everywhere (browser, server, CLI, tests)
- **Pattern Matching**: Supports wildcards (`data:*:created`)
- **Cross-Environment**: Events bridge across process boundaries automatically
- **Type-Safe**: Event payloads are strongly typed
- **Local + Remote**: Subscriptions work whether emitter is in same process or remote
- **No Central Hub**: Every process can emit and subscribe independently

### Common Events

```typescript
// Data events (from DataDaemon)
Events.subscribe('data:users:created', (user) => { /* ... */ });
Events.subscribe('data:users:updated', (user) => { /* ... */ });
Events.subscribe('data:users:deleted', (user) => { /* ... */ });
Events.subscribe('data:chat_messages:created', (message) => { /* ... */ });

// System events
Events.subscribe('system:ready', () => { /* ... */ });
Events.subscribe('system:daemon:started', (daemon) => { /* ... */ });
Events.subscribe('system:error', (error) => { /* ... */ });

// AI events (coordination)
Events.subscribe('ai:thought:broadcasted', (thought) => { /* ... */ });
Events.subscribe('ai:response:generated', (response) => { /* ... */ });

// Widget events (browser only)
Events.subscribe('widget:mounted', (widget) => { /* ... */ });
Events.subscribe('widget:unmounted', (widget) => { /* ... */ });
```

### Events Ubiquity Fix (2025-10-29)

Prior to this fix, `Events.subscribe()` only worked for **local** emissions - events from remote processes weren't triggering subscriptions.

**Problem**: When events arrived via WebSocket, the base `JTAGClient.handleTransportMessage()` just logged "Delegating event to server router" without calling `Events.checkWildcardSubscriptions()`.

**Solution**: Override `handleTransportMessage()` in both `JTAGClientServer` and `JTAGClientBrowser` to call `Events.checkWildcardSubscriptions()` when event messages arrive.

**Implementation**:
```typescript
// JTAGClientBrowser.ts and JTAGClientServer.ts
async handleTransportMessage(message: JTAGMessage): Promise<JTAGResponsePayload> {
  if (JTAGMessageTypes.isEvent(message)) {
    // Extract event name and data from EventBridgePayload
    const payload = message.payload as any;
    const eventName = payload?.eventName;
    const eventData = payload?.data;

    if (eventName && eventData !== undefined) {
      // Trigger local subscriptions
      Events.checkWildcardSubscriptions(eventName, eventData);
    }

    return { success: true, delegated: true, ... };
  }

  return super.handleTransportMessage(message);
}
```

**Result**: `Events.subscribe()` now works **universally** - local or remote doesn't matter.

---

## Why These Two Are Enough

With just commands and events, you can build **everything**:

### Chat System
```typescript
// Send message (command)
await client.commands['data/create']({
  collection: 'chat_messages',
  data: { roomId, content, senderId }
});

// Receive messages (event)
Events.subscribe('data:chat_messages:created', (message) => {
  displayMessage(message);
});
```

### AI Coordination
```typescript
// Generate AI response (command)
const response = await client.commands['ai/generate']({
  model: 'phi3:mini',
  prompt: context.prompt
});

// Coordinate via thoughts (event)
Events.subscribe('ai:thought:broadcasted', (thought) => {
  if (thought.type === 'claiming') {
    evaluateClaimPriority(thought);
  }
});
```

### Real-Time Updates
```typescript
// Mutate data (command)
await client.commands['data/update']({
  collection: 'users',
  id: userId,
  data: { status: 'online' }
});

// React to mutations (event)
Events.subscribe('data:users:updated', (user) => {
  updateUI(user);
});
```

### Cross-Environment Communication
```typescript
// Browser widget executes server command
const result = await client.commands['screenshot']({ querySelector: 'body' });

// Server receives browser event
Events.subscribe('widget:mounted', (widget) => {
  console.log('Widget mounted in browser:', widget.name);
});
```

### P2P Mesh Networking
```typescript
// Events bridge across network boundaries
Events.emit('p2p:message', { from: myNodeId, to: peerNodeId, data });

// Remote nodes receive via EventBridge + WebSocket
Events.subscribe('p2p:message', (message) => {
  if (message.to === myNodeId) {
    handlePeerMessage(message);
  }
});
```

---

## Best Practices

### Commands

1. **Always use type parameters**: `client.commands['data/list']<DataListResult>(...)`
2. **Check result.success**: Commands return `{ success: boolean, ... }`
3. **Handle errors gracefully**: Commands can fail (network, timeout, validation)
4. **Use dynamic discovery**: Call `client.commands['list']({})` to see available commands
5. **Keep commands idempotent**: Same params should produce same result

### Events

1. **Use specific event names**: `data:users:created` not `user-created`
2. **Type event data**: `Events.subscribe<UserEntity>('data:users:created', ...)`
3. **Unsubscribe when done**: Store unsubscribe function and call it
4. **Avoid wildcards in production**: `Events.subscribe('*', ...)` is expensive
5. **Don't block in handlers**: Event handlers should be fast (async work in background)

### Combining Commands & Events

```typescript
// ✅ Good: Command for mutation, event for notification
async function createUser(userData: Partial<UserEntity>) {
  // Mutate via command
  const result = await client.commands['data/create']({
    collection: 'users',
    data: userData
  });

  if (!result.success) {
    throw new Error('Failed to create user');
  }

  // System emits event automatically
  // Other parts of system react via subscriptions
}

// Subscribe once during initialization
Events.subscribe('data:users:created', (user) => {
  displayWelcomeMessage(user);
  sendEmailVerification(user);
  updateUserList();
});
```

---

## This is the E = mc²

These two primitives - **commands** and **events** - are the fundamental building blocks of the entire system.

They are:
- **Simple**: Easy to understand and use
- **Universal**: Work identically everywhere
- **Powerful**: Can build anything with just these two
- **Composable**: Combine in infinite ways
- **Type-Safe**: Full TypeScript inference
- **Transparent**: Local or remote doesn't matter

**Every feature in Continuum is built on these two primitives. Master them, and you master the system.**
