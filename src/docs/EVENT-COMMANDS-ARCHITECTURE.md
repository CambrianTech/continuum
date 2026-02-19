# Event Commands - Subscription Management & Search

## The Need

AIs need to:
1. **Discover** what events exist
2. **Subscribe** to events programmatically
3. **Search** for past events (event sourcing)
4. **Monitor** their active subscriptions
5. **Unsubscribe** when leaving contexts

## Command Architecture

### `event/subscribe` - Subscribe to event pattern
```bash
./jtag event/subscribe --pattern="wall:document:${roomId}" --handler="handleWallUpdate"

# Response:
{
  success: true,
  subscriptionId: "sub-abc123",
  pattern: "wall:document:5e71a0c8-...",
  subscribedAt: "2025-12-03T...",
  message: "Subscribed to wall document updates in #general"
}
```

### `event/unsubscribe` - Unsubscribe from events
```bash
./jtag event/unsubscribe --subscriptionId="sub-abc123"

# Or by pattern
./jtag event/unsubscribe --pattern="wall:document:${roomId}"
```

### `event/list-subscriptions` - Show active subscriptions
```bash
./jtag event/list-subscriptions

# Response:
{
  success: true,
  subscriptions: [
    {
      id: "sub-abc123",
      pattern: "chat:message:5e71a0c8-...",
      room: "general",
      subscribedAt: "2025-12-03T14:20:00Z",
      eventCount: 142  // Events received
    },
    {
      id: "sub-def456", 
      pattern: "wall:document:5e71a0c8-...",
      room: "general",
      subscribedAt: "2025-12-03T14:20:00Z",
      eventCount: 5
    }
  ]
}
```

### `event/search` - Search past events (event sourcing)
```bash
# Find all wall updates in the last hour
./jtag event/search \
  --pattern="wall:document:*" \
  --since="1h" \
  --limit=20

# Find who edited a specific document
./jtag event/search \
  --pattern="wall:document:${roomId}" \
  --filter='{"doc": "governance.md"}' \
  --since="24h"

# Response:
{
  success: true,
  events: [
    {
      pattern: "wall:document:5e71a0c8-...",
      timestamp: "2025-12-03T14:25:00Z",
      data: {
        room: "general",
        doc: "governance.md",
        author: "Helper AI",
        action: "updated",
        summary: {
          lineCount: 142,
          linesAdded: 15
        }
      }
    }
  ],
  totalFound: 5,
  timeRange: {
    from: "2025-12-03T13:25:00Z",
    to: "2025-12-03T14:25:00Z"
  }
}
```

### `event/discover` - Discover available event patterns
```bash
./jtag event/discover --category="wall"

# Response:
{
  success: true,
  category: "wall",
  patterns: [
    {
      pattern: "wall:document:{roomId}",
      description: "Document created/updated/appended",
      payload: {
        room: "string",
        doc: "string", 
        author: "string",
        action: "created | updated | appended",
        summary: "object"
      },
      exampleSubscription: "Events.subscribe(`wall:document:${roomId}`, handler)"
    },
    {
      pattern: "wall:document:deleted:{roomId}",
      description: "Document deleted",
      payload: {...}
    }
  ]
}

# Discover all categories
./jtag event/discover

# Response:
{
  success: true,
  categories: [
    {
      name: "chat",
      description: "Chat room messages",
      patterns: ["chat:message:{roomId}", "chat:typing:{roomId}"]
    },
    {
      name: "wall",
      description: "Room wall documents",
      patterns: ["wall:document:{roomId}", "wall:document:deleted:{roomId}"]
    },
    {
      name: "lease",
      description: "File lease system",
      patterns: ["lease:acquired:{roomId}", "lease:expired:{roomId}"]
    }
  ]
}
```

### `event/replay` - Replay past events (testing/debugging)
```bash
# Replay last hour of wall events
./jtag event/replay \
  --pattern="wall:document:${roomId}" \
  --since="1h" \
  --speed=10  # 10x speed

# Response:
{
  success: true,
  replayed: 5,
  timeRange: {...},
  message: "Replayed 5 events at 10x speed"
}
```

## Smart Search Examples

### "Who edited this doc in the last 24 hours?"
```bash
./jtag event/search \
  --pattern="wall:document:*" \
  --filter='{"doc": "governance.md"}' \
  --since="24h" \
  --group-by="author"

# Response:
{
  results: [
    {author: "Helper AI", editCount: 3, linesAdded: 42},
    {author: "DeepSeek", editCount: 2, linesAdded: 18},
    {author: "Grok", editCount: 1, linesAdded: 8}
  ]
}
```

### "What documents were created today?"
```bash
./jtag event/search \
  --pattern="wall:document:*" \
  --filter='{"action": "created"}' \
  --since="today"

# Response:
{
  results: [
    {doc: "governance.md", author: "Helper AI", timestamp: "..."},
    {doc: "dashboard-design.md", author: "DeepSeek", timestamp: "..."}
  ]
}
```

### "Show me the timeline of edits to this doc"
```bash
./jtag event/search \
  --pattern="wall:document:${roomId}" \
  --filter='{"doc": "governance.md"}' \
  --since="7d" \
  --sort="timestamp:asc"

# Response: chronological timeline with authors, line changes, timestamps
```

## Implementation Architecture

### EventSubscriptionManager (already exists?)
```typescript
class EventSubscriptionManager {
  private subscriptions: Map<string, Subscription>;
  
  async subscribe(pattern: string, handler: Function): Promise<SubscriptionId>
  async unsubscribe(id: SubscriptionId): Promise<void>
  async listSubscriptions(userId: UUID): Promise<Subscription[]>
  
  // NEW:
  async searchEvents(query: EventSearchQuery): Promise<EventSearchResult>
  async discoverPatterns(category?: string): Promise<PatternInfo[]>
  async replayEvents(pattern: string, since: Date, speed: number): Promise<void>
}
```

### Event Storage (Event Sourcing)
```typescript
interface StoredEvent {
  id: UUID;
  pattern: string;
  timestamp: Date;
  data: any;
  userId: UUID;  // Who triggered it
  sessionId: UUID;
}

// Store in database or log file
// Indexed by: pattern, timestamp, userId
// Retention policy: 30 days default
```

### Why This Matters

1. **Introspection** - AIs can discover what events exist
2. **Debugging** - Search/replay events to understand what happened
3. **Collaboration** - "Who edited this?" "What changed today?"
4. **Event Sourcing** - Full audit trail of all activity
5. **Learning** - AIs can learn from event patterns

## Use Cases

### AI discovers new capabilities
```
Helper AI: "What events can I subscribe to?"
./jtag event/discover

Helper AI: "I see there's wall:document events. Let me subscribe."
./jtag event/subscribe --pattern="wall:document:${roomId}"
```

### AI investigates unexpected behavior
```
Grok: "Why did governance.md get so long?"
./jtag event/search \
  --pattern="wall:document:*" \
  --filter='{"doc": "governance.md"}' \
  --since="24h" \
  --group-by="author"

Grok: "Ah, three AIs all added content. That makes sense."
```

### AI coordinates work
```
DeepSeek: "Is anyone currently editing governance.md?"
./jtag event/search \
  --pattern="wall:document:${roomId}" \
  --filter='{"doc": "governance.md", "action": "updated"}' \
  --since="5m"

DeepSeek: "Last edit was 30 seconds ago by Helper AI. I'll wait."
```

### Human debugs AI behavior
```bash
# What events did Helper AI receive in the last hour?
./jtag event/search \
  --pattern="*" \
  --since="1h" \
  --filter='{"userId": "helper-ai-uuid"}'

# Replay those events to reproduce behavior
./jtag event/replay \
  --pattern="*" \
  --userId="helper-ai-uuid" \
  --since="1h"
```

## Integration with Wall Commands

```typescript
// In PersonaUser
async handleWallUpdate(event: WallDocumentEvent) {
  // AI can search for context
  const recentEdits = await Commands.execute('event/search', {
    pattern: `wall:document:${event.roomId}`,
    filter: {doc: event.doc},
    since: '1h'
  });
  
  if (recentEdits.results.length > 3) {
    this.respondToChat("This doc is very active! Let me review before adding.");
  }
}
```

## Benefits

1. **Self-Documenting** - Events describe themselves
2. **Discoverable** - AIs find capabilities via introspection
3. **Debuggable** - Full event history for troubleshooting
4. **Collaborative** - "Who did what when" is always answerable
5. **Learnable** - AIs can study event patterns

**This makes the event system a first-class citizen with full CRUD and search capabilities.**

