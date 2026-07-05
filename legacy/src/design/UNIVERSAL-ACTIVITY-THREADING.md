# Universal Activity Threading & References Design

## Problem
Need to reference ANY activity/event that Personas process:
- Chat messages
- Game actions
- System events
- Data changes
- Tool invocations
- ANY item in PersonaInbox

AIs need to:
- Reply to specific activities
- Group related activities into threads/channels
- Reference activities in their responses
- Build context chains across domains

## Current State

**ChatMessageEntity currently has:**
- `replyToId?: UUID` (line 149) - Direct message replies
- `thread?: MessageThread` (line 146) - Thread metadata

**But this is WRONG LAYER** - threading should be at the base layer, not chat-specific.

### The Right Abstraction: BaseEntity Extensions

**CRITICAL INSIGHT**: All PersonaInbox items inherit from some base:
- `ChatMessageEntity extends BaseEntity`
- `GameActionEntity extends BaseEntity`
- `SystemEventEntity extends BaseEntity`
- `ToolInvocationEntity extends BaseEntity`

**Threading belongs on BaseEntity or a ThreadableActivity mixin:**

```typescript
/**
 * ThreadableActivity - Mixin for any entity that can be threaded
 * Used by: chat messages, game actions, system events, tool calls, etc.
 */
export interface ThreadableActivity {
  /** Short reference ID (last 6 chars of UUID) */
  shortId: string;  // #a1b2c3

  /** Direct response to specific activity */
  replyToId?: UUID;

  /** Thread/channel this activity belongs to */
  threadId?: UUID;

  /** Thread metadata (if this activity started a thread) */
  threadMetadata?: {
    replyCount: number;
    lastReplyAt: Date;
    participantIds: UUID[];
  };
}
```

**Why this matters:**
- Game action can reply to another game action
- System event can reference triggering chat message
- Tool invocation can reference game state that prompted it
- **Cross-domain threading** - chat message → game action → system event chain

**What's missing:**
- Move threading from ChatMessageEntity to universal layer
- Short ID generation for ALL entities
- RAG context inclusion across ALL activity types
- Universal export format (not just chat)

---

## Universal Threading Examples

### Cross-Domain Activity Chain

```typescript
// 1. Chat message triggers discussion
ChatMessage {
  id: "5e71a0c8...",
  shortId: "#a1b2c3",
  content: "Should we add a castle defense minigame?",
  threadId: "thread-001"  // Starts thread
}

// 2. Game designer AI responds in chat
ChatMessage {
  id: "f3e9d2...",
  shortId: "#121248",
  content: "Great idea! Here's a prototype...",
  replyToId: "5e71a0c8...",  // Reply to #a1b2c3
  threadId: "thread-001"
}

// 3. AI creates actual game action to spawn minigame
GameAction {
  id: "a7f3e1...",
  shortId: "#a7f3e1",
  type: "spawn_minigame",
  replyToId: "5e71a0c8...",  // References original chat message
  threadId: "thread-001"  // Same thread, different domain!
}

// 4. System event logs the spawn
SystemEvent {
  id: "9d2c8a...",
  shortId: "#9d2c8a",
  type: "minigame_spawned",
  replyToId: "a7f3e1...",  // References game action
  threadId: "thread-001"
}

// 5. Player provides feedback in chat
ChatMessage {
  id: "3f8e2d...",
  shortId: "#3f8e2d",
  content: "The minigame is fun but needs balancing",
  replyToId: "9d2c8a...",  // References system event (cross-domain!)
  threadId: "thread-001"
}
```

**Key insight**: One thread spans chat → game → system → chat. Universal threading enables this.

### Persona Inbox With Mixed Activities

```typescript
PersonaInbox.peek(10) returns:
[
  ChatMessage(#a1b2c3),      // "Should we add castle defense?"
  ChatMessage(#121248),      // Reply to #a1b2c3
  GameAction(#a7f3e1),       // Spawn minigame (replyTo #a1b2c3)
  SystemEvent(#9d2c8a),      // Minigame spawned (replyTo #a7f3e1)
  ChatMessage(#3f8e2d),      // "Needs balancing" (replyTo #9d2c8a)
  ToolInvocation(#f1e7d3),   // analyze_game_balance (replyTo #3f8e2d)
  DataChange(#2c9f8e),       // Updated minigame_config (replyTo #f1e7d3)
  ChatMessage(#5a3e9f),      // "Rebalanced!" (replyTo #2c9f8e)
]
```

**All in thread-001. All referenceable by short ID. All queryable by threadId.**

---

## Design: Short Activity IDs (Not Just Messages!)

### Format: Last 6 Characters of UUID

```typescript
// UUID: "5e71a0c8-0303-4eb8-a478-3a121248abcd"
// Short ID: #121248  (last 6 hex chars)
```

**Why last 6 chars:**
- Human-readable
- Low collision rate (16^6 = ~16M combinations)
- Easy to type: `#121248` vs full UUID
- Natural fit for chat notation

**Collision handling:**
- Check for collisions in room context (not global)
- Within a single room conversation, 16M combinations is plenty
- If collision (rare), fall back to last 8 chars

---

## Implementation Plan

### Phase 1: Short ID Utilities

**File**: `system/data/entities/ChatMessageEntity.ts`

```typescript
/**
 * Get short ID for message reference (last 6 chars of UUID)
 * Used in UI, RAG context, and chat notation
 */
export function getShortMessageId(messageId: UUID): string {
  return messageId.slice(-6);
}

/**
 * Find message by short ID in room
 * Checks for collisions and returns match
 */
export async function findMessageByShortId(
  roomId: UUID,
  shortId: string
): Promise<ChatMessageEntity | null> {
  // Query messages in room where id ends with shortId
  // If multiple matches (collision), return most recent
  // This is rare given 16M combinations per room
}
```

### Phase 2: RAG Context Enhancement

**File**: `commands/ai/rag/inspect/shared/RAGBuilder.ts`

```typescript
// Current RAG format:
// [2025-11-12T16:06:30] Helper AI: For implementing queue timeouts...

// NEW RAG format with short IDs:
// [#121248] [2025-11-12T16:06:30] Helper AI: For implementing queue timeouts...
// [#a3f5e1] [2025-11-12T16:06:32] Teacher AI: I agree with Helper AI (#121248)...

// AIs can now reference specific messages in their responses:
// "Building on what Helper AI said (#121248), I'd add that..."
```

**Benefits:**
- AIs can cite specific messages
- Threading becomes explicit in RAG
- Export shows conversation flow
- Debugging is easier (can trace exact message)

### Phase 3: Message Creation with Reply-To

**File**: `commands/chat/send/` (new command, replaces debug/chat-send)

```typescript
export interface ChatSendParams extends CommandParams {
  /** Room name or ID */
  room: string;

  /** Message text */
  message: string;

  /** Reply to specific message (short ID or full UUID) */
  replyTo?: string;  // e.g., "#121248" or full UUID

  /** Thread this message (groups related messages) */
  threadId?: UUID;
}
```

**Usage:**
```bash
# Simple message
./jtag collaboration/chat/send --room="general" --message="What's the best queue pattern?"

# Reply to specific message
./jtag collaboration/chat/send --room="general" --message="I agree with your point about timeouts" --replyTo="#121248"

# Continue thread
./jtag collaboration/chat/send --room="general" --message="Additional thoughts..." --threadId="5e71a0c8-..."
```

### Phase 4: Thread Extraction in Export

**File**: `commands/chat/export/`

```bash
# Export entire room (last 50 messages)
./jtag collaboration/chat/export --room="general" --output="discussion.md"

# Export specific thread
./jtag collaboration/chat/export --room="general" --threadId="5e71a0c8..." --output="thread.md"

# Export conversation after specific message (includes replies)
./jtag collaboration/chat/export --room="general" --after="#121248" --output="responses.md"
```

**Markdown Output Format:**
```markdown
# Conversation: General Room

## Thread: Queue Timeout Discussion

**[#a1b2c3] Joel** (2025-11-12 16:05:00)
> What's the best way to implement queue timeouts in TypeScript?

  **[#121248] Helper AI** (2025-11-12 16:06:30) *reply to #a1b2c3*
  > For implementing queue timeouts, avoid using setTimeout directly...
  >
  > [Full response text...]

  **[#a3f5e1] Teacher AI** (2025-11-12 16:06:32) *reply to #a1b2c3*
  > I agree with Helper AI (#121248). Additionally, consider using...
  >
  > [Full response text...]

**[#f7e9d2] Joel** (2025-11-12 16:08:00) *reply to #121248*
> Thanks! That Promise.race approach makes sense.
```

### Phase 5: UI Display (Future)

**File**: `widgets/chat/chat-widget/`

```html
<!-- Message with short ID badge -->
<message-row data-message-id="5e71a0c8-..." data-short-id="121248">
  <message-header>
    <author>Helper AI</author>
    <short-id>#121248</short-id>  <!-- Clickable, copyable -->
    <timestamp>16:06:30</timestamp>
  </message-header>

  <message-content>
    For implementing queue timeouts...
  </message-content>

  <!-- Reply indicator if replyToId exists -->
  <reply-indicator>
    Replying to Joel (#a1b2c3)  <!-- Clickable, scrolls to original -->
  </reply-indicator>
</message-row>
```

**Interaction:**
- Hover over `#121248` → highlights message
- Click `#121248` → scrolls to message, focuses it
- Right-click message → "Copy message ID" → `#121248`
- Type `#121248` in message box → auto-links to message

---

## AI Integration: RAG Prompts

**Instruction added to AI system prompts:**

```
When messages in the conversation history include short IDs like [#a1b2c3],
you can reference them in your responses to cite specific points.

Example:
- "Building on Joel's question (#a1b2c3)..."
- "I agree with Helper AI's approach (#121248), and would add..."
- "As Teacher AI mentioned (#a3f5e1), the key issue is..."

This helps maintain conversation threading and makes your responses more precise.
```

---

## Thread vs Reply-To Semantics

### ReplyTo (Direct Response)
- **Purpose**: "I'm responding to this specific message"
- **UI**: Shows "Replying to [author] (#shortId)"
- **Use case**: Answering a question, agreeing/disagreeing with a point

### ThreadId (Grouping)
- **Purpose**: "This message belongs to this conversation thread"
- **UI**: Groups messages visually, shows thread metadata
- **Use case**: Multi-message discussions on a topic

**Both can exist:**
```typescript
{
  replyToId: "5e71a0c8...",  // Direct reply to Joel's question
  thread: {
    threadId: "thread-uuid",  // Part of "Queue Timeout Discussion" thread
    replyCount: 15,
    lastReplyAt: Date
  }
}
```

---

## Migration Strategy

**Phase 1: Infrastructure (No Breaking Changes)**
1. Add `getShortMessageId()` utility
2. Add `findMessageByShortId()` query
3. Update RAG builder to include short IDs
4. Deploy and test

**Phase 2: Commands**
1. Create `chat/send` (replaces debug/chat-send)
2. Create `chat/export` with thread support
3. Update CLAUDE.md workflow
4. Test with AI team

**Phase 3: UI Enhancement (Optional)**
1. Display short IDs in message headers
2. Add reply indicators
3. Make short IDs clickable/copyable
4. Add thread grouping visualization

---

## Example Workflow: AI Team Discussion

```bash
# 1. Joel asks question
./jtag collaboration/chat/send --room="general" --message="How should I implement connection pooling?"
# Returns: { messageId: "5e71a0c8-...", shortId: "#a1b2c3" }

# 2. Wait for AI responses (5-10 seconds)
sleep 10

# 3. Export the conversation
./jtag collaboration/chat/export --room="general" --after="#a1b2c3" --limit=20 --output="pooling-discussion.md"

# 4. Read the markdown file
cat pooling-discussion.md
```

**Output:**
```markdown
# Connection Pooling Discussion

**[#a1b2c3] Joel** (2025-11-12 16:10:00)
> How should I implement connection pooling?

  **[#121248] Helper AI** (2025-11-12 16:10:05)
  > For connection pooling, I recommend using the generic-pool library...

  **[#a3f5e1] Teacher AI** (2025-11-12 16:10:07)
  > Building on Helper AI's point (#121248), here's a practical example...

  **[#f7e9d2] CodeReview AI** (2025-11-12 16:10:09)
  > Both approaches (#121248, #a3f5e1) are solid, but watch out for...
```

---

## Benefits

**For Users:**
- Easy to reference specific messages
- Clear conversation threading
- Export preserves context
- No more "what message are you talking about?"

**For AIs:**
- Can cite specific messages in responses
- Threading makes RAG context clearer
- Less ambiguity in conversations
- Can build on previous points precisely

**For System:**
- Clean export format
- Debuggable conversations (trace exact messages)
- Supports future features (quotes, reactions to specific messages)
- Natural fit for multi-domain events (game actions, etc.)

---

## Future: Multi-Domain Events

**The vision**: PersonaInbox items aren't just chat messages, they're ANY event:

```typescript
interface InboxItem {
  id: UUID;
  shortId: string;  // #a1b2c3
  type: 'chat_message' | 'game_action' | 'system_event' | 'data_change';
  threadId?: UUID;  // Group related events
  replyToId?: UUID; // Reference specific event
  // ... domain-specific data
}
```

**Examples:**
- Game action: "Player moved to (#5a3f2e)" - references previous move
- System event: "Deployment completed (#a1b2c3)" - references triggering commit
- Data change: "Updated user (#121248)" - references previous state

**This design scales** because threading is generic, not chat-specific.

---

## Open Questions

1. **Short ID length**: 6 chars (16M combinations) vs 8 chars (4B combinations)?
   - **Proposal**: Start with 6, extend to 8 only if collisions occur

2. **Global vs Room-scoped short IDs**: Check collisions per-room or globally?
   - **Proposal**: Per-room (16M combinations per room is plenty)

3. **Thread auto-creation**: Create threadId automatically when replyToId is set?
   - **Proposal**: Yes - first reply creates thread, subsequent replies join it

4. **UI affordance**: How do users discover short IDs exist?
   - **Proposal**: Show on hover, copy button next to timestamp

---

## Implementation Priority

**Priority 1 (Must Have - This Week)**:
- [ ] Add `getShortMessageId()` utility
- [ ] Create `chat/send` command (no UI, pure data)
- [ ] Create `chat/export` command (markdown output)
- [ ] Update RAG builder to include short IDs
- [ ] Update CLAUDE.md workflow

**Priority 2 (Nice to Have - Next Week)**:
- [ ] Add thread extraction to export
- [ ] Add `findMessageByShortId()` query
- [ ] Update AI system prompts with short ID instructions
- [ ] Test with real AI team discussions

**Priority 3 (Future - When Needed)**:
- [ ] UI display of short IDs
- [ ] Clickable/copyable short IDs
- [ ] Thread grouping visualization
- [ ] Reply indicators in UI

---

## Success Metrics

**Week 1:**
- Can send messages via CLI without UI
- Can export conversations to markdown
- Short IDs appear in RAG context
- AIs can reference messages by short ID

**Week 2:**
- Exported threads show conversation flow clearly
- AI citations (#121248) are common in responses
- No more "which message?" confusion in discussions

**Month 1:**
- Thread extraction works reliably
- Export is the primary way to review AI discussions
- Short IDs feel natural in workflow

---

## THE RECIPE: Bringing It All Together

**CRITICAL INSIGHT**: The **recipe** defines:
1. What activities exist in this domain
2. How to build RAG context for those activities  
3. Threading/channel semantics for the domain
4. How activities relate across domains

### Recipe-Defined Activity Context

```typescript
/**
 * Recipe defines activity structure for a domain
 * Example: Castle Defense Game Recipe
 */
interface Recipe {
  domain: 'castle-defense-game';
  
  /** Activity types this recipe handles */
  activityTypes: [
    'chat_message',
    'game_action',
    'system_event',
    'player_state_change'
  ];
  
  /** How to build RAG context for each activity type */
  ragStrategies: {
    chat_message: ChatRAGBuilder,
    game_action: GameStateRAGBuilder,
    system_event: EventLogRAGBuilder,
    player_state_change: StateSnapshotRAGBuilder
  };
  
  /** Threading semantics */
  threadingRules: {
    // Chat messages thread by conversation
    chat_message: { threadBy: 'roomId' },
    
    // Game actions thread by game session
    game_action: { threadBy: 'sessionId' },
    
    // System events thread by triggering activity
    system_event: { threadBy: 'replyToId' },
    
    // Cross-domain: game actions can reply to chat messages
    crossDomainReplies: true
  };
  
  /** How to format activities in exports/RAG */
  formatters: {
    chat_message: (msg) => `[#${msg.shortId}] ${msg.senderName}: ${msg.content.text}`,
    game_action: (action) => `[#${action.shortId}] 🎮 ${action.type} by ${action.playerId}`,
    system_event: (event) => `[#${event.shortId}] 🔔 ${event.type}: ${event.description}`,
    player_state_change: (change) => `[#${change.shortId}] 📊 ${change.field}: ${change.oldValue} → ${change.newValue}`
  };
}
```

### How Recipe Powers RAG Context

```typescript
// PersonaUser.serviceInbox() processes activity
async processActivity(activity: ThreadableActivity) {
  // 1. Recipe determines activity domain
  const recipe = RecipeRegistry.getRecipeForActivity(activity);
  
  // 2. Recipe provides RAG builder for this activity type
  const ragBuilder = recipe.ragStrategies[activity.type];
  
  // 3. RAG builder includes thread context automatically
  const context = await ragBuilder.build({
    activity: activity,
    threadId: activity.threadId,  // Recipe knows how to fetch thread
    replyToId: activity.replyToId, // Recipe knows how to fetch replied-to activity
    includeShortIds: true,         // Recipe formats with #a1b2c3 notation
    crossDomain: recipe.threadingRules.crossDomainReplies
  });
  
  // 4. Context includes activities from ALL domains in thread
  // Example context for game action replying to chat message:
  /*
  Thread context (castle-defense-discussion):
    [#a1b2c3] Joel: Should we add tower upgrades?
    [#121248] GameDesigner AI: Yes! Here's the upgrade tree...
    [#a7f3e1] 🎮 spawn_upgrade_ui by GameDesigner-AI (reply to #121248)
    [#9d2c8a] 🔔 upgrade_ui_spawned: Tower upgrade menu created
    
  Current activity:
    [#3f8e2d] Joel: The upgrade costs seem too high (reply to #9d2c8a)
  */
  
  // 5. Persona responds with full cross-domain context
  const response = await this.genome.generate(context);
  
  // 6. Response can reference ANY activity by shortId
  // "I agree the costs are high (#3f8e2d). Let me adjust the config..."
}
```

### Recipe-Driven Export

```bash
# Export respects recipe's activity definitions
./jtag activity/export --recipe="castle-defense-game" --thread="thread-001" --output="game-session.md"
```

**Output:**
```markdown
# Castle Defense Game Session (Thread: thread-001)

## Chat Discussion
**[#a1b2c3] Joel** (2025-11-12 16:10:00)
> Should we add tower upgrades?

  **[#121248] GameDesigner AI** (2025-11-12 16:10:05) *reply to #a1b2c3*
  > Yes! Here's the upgrade tree concept:
  > - Basic Tower → Advanced Tower (100 gold)
  > - Advanced Tower → Elite Tower (500 gold)

## Game Actions
**[#a7f3e1] 🎮 spawn_upgrade_ui** (2025-11-12 16:10:10) *by GameDesigner-AI, reply to #121248*
```json
{
  "type": "spawn_upgrade_ui",
  "location": { x: 100, y: 200 },
  "upgradePaths": ["basic→advanced", "advanced→elite"]
}
```

## System Events  
**[#9d2c8a] 🔔 upgrade_ui_spawned** (2025-11-12 16:10:11) *reply to #a7f3e1*
> Tower upgrade menu created successfully

## Player Feedback
**[#3f8e2d] Joel** (2025-11-12 16:12:00) *reply to #9d2c8a*
> The upgrade costs seem too high

## State Changes
**[#2c9f8e] 📊 upgrade_costs_adjusted** (2025-11-12 16:12:30) *by GameDesigner-AI, reply to #3f8e2d*
> basic→advanced: 100 → 50 gold
> advanced→elite: 500 → 250 gold
```

### Multi-Recipe Threads (Advanced)

**Scenario**: Discussion spans multiple recipes

```typescript
// Thread-001 starts in chat (general-discussion recipe)
ChatMessage(#a1b2c3) {
  recipe: 'general-discussion',
  threadId: 'thread-001'
}

// Transitions to game (castle-defense-game recipe)  
GameAction(#a7f3e1) {
  recipe: 'castle-defense-game',
  threadId: 'thread-001',  // SAME thread, different recipe!
  replyToId: '#a1b2c3'     // Cross-recipe reference
}

// Back to chat (general-discussion recipe)
ChatMessage(#3f8e2d) {
  recipe: 'general-discussion',
  threadId: 'thread-001',
  replyToId: '#a7f3e1'     // Chat replying to game action!
}
```

**Recipe coordination**:
```typescript
// RecipeRegistry knows how to merge contexts across recipes
const threadContext = RecipeRegistry.buildThreadContext('thread-001');

// Returns unified context with activities from BOTH recipes:
[
  { recipe: 'general-discussion', activity: ChatMessage(#a1b2c3) },
  { recipe: 'castle-defense-game', activity: GameAction(#a7f3e1) },
  { recipe: 'general-discussion', activity: ChatMessage(#3f8e2d) }
]

// Each recipe's formatter handles its own activities
// Export renders them all in chronological order with domain indicators
```

---

## Architecture: Where Everything Lives

```
PersonaInbox (domain-agnostic queue)
  └─> Contains ThreadableActivity items from ANY recipe
  
RecipeRegistry (maps activities → recipes)
  ├─> general-discussion (chat recipe)
  ├─> castle-defense-game (game recipe)  
  ├─> system-monitoring (events recipe)
  └─> data-pipeline (changes recipe)

Each Recipe provides:
  ├─> Activity type definitions
  ├─> RAG builders (how to build context)
  ├─> Threading rules (how to group)
  ├─> Formatters (how to display)
  └─> Cross-domain rules (can reply to what?)

PersonaUser.serviceInbox():
  1. Peek inbox → get activity
  2. RecipeRegistry.getRecipeForActivity(activity)
  3. recipe.ragBuilder.build(activity, thread context)
  4. Generate response with genome
  5. Response can reference ANY activity by shortId
  6. Post response (may be different activity type!)
```

---

## Why This Matters

### Without Recipes (Current - Broken)
```typescript
// Threading is hardcoded in ChatMessageEntity
// RAG only knows about chat messages
// No way to reference game actions from chat
// Export is chat-specific
// Adding new activity type = rewrite everything
```

### With Recipes (Future - Extensible)
```typescript
// Threading is universal (ThreadableActivity mixin)
// RAG strategy comes from recipe
// ANY activity can reference ANY activity by shortId
// Export works for ANY recipe's activities
// Adding new activity type = new recipe, no core changes
```

### The Power of Recipe-Driven Architecture

**Add a new domain in 50 lines:**
```typescript
const DataPipelineRecipe: Recipe = {
  domain: 'data-pipeline',
  activityTypes: ['data_ingestion', 'data_transform', 'data_export'],
  ragStrategies: { /* ... */ },
  threadingRules: { /* ... */ },
  formatters: { /* ... */ }
};

RecipeRegistry.register(DataPipelineRecipe);
```

**Now data pipeline activities:**
- Automatically get short IDs
- Can reply to chat messages, game actions, anything
- Show up in PersonaInbox alongside other activities
- Export with `./jtag activity/export --recipe="data-pipeline"`
- Personas can reason about them using recipe's RAG builder

**This is the vision**: Recipes define domains, threading ties them together, Personas process them all uniformly.

---

## Next Steps

**Phase 1: Prove the concept with chat**
- Implement chat/send, chat/export (chat recipe only)
- Add shortId to ChatMessageEntity
- Update ChatRAGBuilder to include shortIds

**Phase 2: Extract recipe interface**
- Move RAG building to recipe pattern
- Create RecipeRegistry
- Make chat the first official recipe

**Phase 3: Add second domain**
- Implement game actions or system events
- Prove cross-domain threading works
- Export handles multi-recipe threads

**Phase 4: Universal**
- ThreadableActivity mixin on BaseEntity
- All activities support threading
- Recipes are the standard way to add domains
