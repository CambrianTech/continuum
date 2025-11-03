# PersonaUser Next Phase - Architecture & Implementation Plan

**Created**: 2025-10-05
**Status**: Planning Phase
**Goal**: Design database-backed subscription system, deterministic testing infrastructure, and AI Daemon architecture before live AI integration

---

## 🎯 Core Requirements (Refining Existing /src Code)

### What Already Works at /src:
- ✅ **PersonaDaemon** - Full persona lifecycle, LoRA adapter system, Academy training
- ✅ **AiModelChatCommand** - Mock AI responses (GPT-4, Claude, Aria)
- ✅ **Model adapters** - OpenAI, Anthropic, HuggingFace placeholders
- ✅ **LoRA stack** - Hierarchical adapter composition with matrix operations
- ✅ **Command interface** - Personas can execute same commands as humans
- ✅ **Session isolation** - Personas have isolated artifact directories

### What We Need to Port to JTAG:
1. **UserRoomSubscription Entity** - Database-backed chat memberships (currently ephemeral)
2. **Context switching** - PersonaUsers flip between multiple rooms naturally
3. **Event-driven responses** - PersonaUsers listen to `chat:message:received` and respond
4. **Rate limiting** - Prevent runaway personas from spamming
5. **Deterministic testing** - Mock AI responses for reliable tests

### Priority: Natural Chat Flow First, LoRA Later
**Phase 1 (THIS WEEKEND):** PersonaUsers in multi-room conversations with context awareness
**Phase 2 (FUTURE):** LoRA fine-tuning, Academy training, genomic sophistication

LoRA is **already implemented** at `/src/daemons/persona/PersonaDaemon.ts` - we port it AFTER chat flow works.

---

## 📊 Entity Design

### EventSubscription Entity
**Purpose**: Universal event subscription system stored in database

```typescript
export class EventSubscriptionEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.EVENT_SUBSCRIPTIONS;
  static readonly version = 1;

  // Identity
  id: UUID;                      // Subscription ID
  userId: UUID;                  // Subscriber (Human, Agent, Persona)

  // Event matching
  eventPattern: string;          // e.g., "chat:message:*", "data:Room:updated"

  // Scope
  contextId?: UUID;              // Specific room/context (optional - null = global)
  sessionId?: UUID;              // Specific session (optional - null = all sessions)

  // Lifecycle
  createdAt: Timestamp;
  expiresAt?: Timestamp;         // Optional expiration (null = permanent)
  isActive: boolean;             // Can be disabled without deletion

  // Metadata
  metadata?: {
    priority?: number;           // Delivery priority (higher = first)
    deliveryMode?: 'immediate' | 'batched' | 'throttled';
    maxEventsPerMinute?: number; // Rate limiting per subscription
  };
}
```

**Key Features**:
- Wildcard pattern matching (`chat:*`, `data:Room:*`)
- Scoped subscriptions (global, per-context, per-session)
- Expiration support (temporary subscriptions)
- Rate limiting per subscription
- Active/inactive toggle without deletion

**Usage Example**:
```typescript
// PersonaUser subscribes to all chat messages in their rooms
await Events.subscribe({
  userId: personaUser.id,
  eventPattern: 'chat:message:received',
  contextId: roomId,  // Only messages in this room
  isActive: true
});
```

---

### UserRoomSubscription Entity
**Purpose**: Permanent chat room memberships for all user types

```typescript
export class UserRoomSubscriptionEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.USER_ROOM_SUBSCRIPTIONS;
  static readonly version = 1;

  // Identity
  id: UUID;                      // Subscription ID
  userId: UUID;                  // Subscriber (Human, Agent, Persona)
  roomId: UUID;                  // Chat room

  // Role & Permissions
  role: 'owner' | 'admin' | 'member' | 'observer';
  permissions: {
    canRead: boolean;
    canWrite: boolean;
    canInvite: boolean;
    canModerate: boolean;
  };

  // State
  joinedAt: Timestamp;
  lastReadAt?: Timestamp;        // Last message read (for unread count)
  unreadCount?: number;          // Cached unread message count
  isMuted: boolean;              // Notifications muted
  isPinned: boolean;             // Pinned to top of room list

  // Lifecycle
  isActive: boolean;             // Active membership
  leftAt?: Timestamp;            // When user left (soft delete)

  // Metadata
  metadata?: {
    invitedBy?: UUID;            // Who invited this user
    customName?: string;         // User's custom name for room
    notificationPreferences?: {
      mentions: boolean;
      allMessages: boolean;
      keywords: string[];
    };
  };
}
```

**Key Features**:
- Permanent storage (not ephemeral like DOM listeners)
- Role-based permissions
- Unread count tracking
- Notification preferences
- Soft deletion (leftAt timestamp)
- Custom room naming per user

**Usage Example**:
```typescript
// PersonaUser joins room as observer
await Commands.execute('user-room/subscribe', {
  userId: personaUser.id,
  roomId: roomId,
  role: 'observer',
  permissions: { canRead: true, canWrite: true, canInvite: false, canModerate: false }
});
```

---

## 🧪 Deterministic Testing Strategy

### Phase 1: Mock AI Response Tests
**Purpose**: Test PersonaUser logic without real AI API calls

```typescript
// tests/unit/persona-mock-responses.test.ts
describe('PersonaUser - Mock AI Responses', () => {
  it('should respond with deterministic mock when message contains keyword', async () => {
    const mockAI = new MockAIAdapter({
      responses: {
        'hello': 'Hello! How can I assist you today?',
        'help': 'I can help with various tasks. What do you need?'
      }
    });

    const persona = new PersonaUser({ aiAdapter: mockAI });
    const response = await persona.processMessage('hello');

    expect(response).toBe('Hello! How can I assist you today?');
  });

  it('should not respond if message does not match triggers', async () => {
    const mockAI = new MockAIAdapter({ responses: { 'hello': 'Hi!' } });
    const persona = new PersonaUser({ aiAdapter: mockAI });

    const response = await persona.processMessage('random text');
    expect(response).toBeNull(); // Persona should not respond
  });
});
```

---

### Phase 2: Event Subscription Verification Tests
**Purpose**: Prove PersonaUsers are listening to correct events

```typescript
// tests/integration/persona-event-subscriptions.test.ts
describe('PersonaUser - Event Subscriptions', () => {
  it('should receive chat:message:received events for subscribed room', async () => {
    const persona = await PersonaUser.create({ name: 'TestPersona' });
    const room = await createTestRoom();

    // Subscribe persona to room
    await Commands.execute('user-room/subscribe', {
      userId: persona.id,
      roomId: room.id,
      role: 'member'
    });

    // Track if persona receives event
    let eventReceived = false;
    persona.on('chat:message:received', () => { eventReceived = true; });

    // Send message to room
    await Commands.execute('chat/send', {
      roomId: room.id,
      senderId: DEFAULT_USERS.HUMAN,
      content: { text: 'Test message' }
    });

    await sleep(1000); // Wait for event propagation
    expect(eventReceived).toBe(true);
  });

  it('should NOT receive events for unsubscribed room', async () => {
    const persona = await PersonaUser.create({ name: 'TestPersona' });
    const room1 = await createTestRoom();
    const room2 = await createTestRoom();

    // Subscribe to room1 only
    await Commands.execute('user-room/subscribe', {
      userId: persona.id,
      roomId: room1.id,
      role: 'member'
    });

    let eventCount = 0;
    persona.on('chat:message:received', () => { eventCount++; });

    // Send message to room2 (not subscribed)
    await Commands.execute('chat/send', {
      roomId: room2.id,
      senderId: DEFAULT_USERS.HUMAN,
      content: { text: 'Should not receive this' }
    });

    await sleep(1000);
    expect(eventCount).toBe(0); // Persona should not receive event
  });
});
```

---

### Phase 3: Keyword Trigger Tests (No AI Randomness)
**Purpose**: Test trigger logic without unpredictable AI responses

```typescript
// tests/unit/persona-keyword-triggers.test.ts
describe('PersonaUser - Keyword Triggers', () => {
  it('should detect @mention trigger', () => {
    const persona = new PersonaUser({ name: 'HelperAI' });

    expect(persona.shouldRespond('@HelperAI please help')).toBe(true);
    expect(persona.shouldRespond('Hey @HelperAI')).toBe(true);
    expect(persona.shouldRespond('random message')).toBe(false);
  });

  it('should detect configured keyword triggers', () => {
    const persona = new PersonaUser({
      name: 'CodeAI',
      triggers: { keywords: ['debug', 'error', 'bug'] }
    });

    expect(persona.shouldRespond('I found a debug issue')).toBe(true);
    expect(persona.shouldRespond('Getting an error')).toBe(true);
    expect(persona.shouldRespond('This is a bug')).toBe(true);
    expect(persona.shouldRespond('Hello everyone')).toBe(false);
  });

  it('should respect rate limiting per room', async () => {
    const persona = new PersonaUser({
      name: 'ChattyAI',
      rateLimit: { maxMessagesPerMinute: 2 }
    });

    const room = await createTestRoom();

    // First 2 messages should trigger response
    expect(await persona.shouldRespond('hello', { roomId: room.id })).toBe(true);
    expect(await persona.shouldRespond('hi again', { roomId: room.id })).toBe(true);

    // 3rd message should be rate limited
    expect(await persona.shouldRespond('one more', { roomId: room.id })).toBe(false);
  });
});
```

---

### Phase 4: Rate Limiting / Anti-Spam Tests
**Purpose**: Prevent PersonaUsers from going haywire with rapid responses

```typescript
// tests/integration/persona-rate-limiting.test.ts
describe('PersonaUser - Rate Limiting', () => {
  it('should enforce global rate limit across all rooms', async () => {
    const persona = new PersonaUser({
      name: 'SpammerAI',
      rateLimit: {
        maxMessagesPerMinute: 5,
        maxMessagesPerHour: 20
      }
    });

    const room1 = await createTestRoom();
    const room2 = await createTestRoom();

    // Subscribe to both rooms
    await persona.subscribeToRoom(room1.id);
    await persona.subscribeToRoom(room2.id);

    // Send 6 messages total (3 per room)
    for (let i = 0; i < 3; i++) {
      await sendMessage(room1.id, `Message ${i} to room1`);
      await sendMessage(room2.id, `Message ${i} to room2`);
    }

    await sleep(2000); // Wait for processing

    // Should have sent max 5 responses (rate limited)
    const responses = await getPersonaResponses(persona.id);
    expect(responses.length).toBeLessThanOrEqual(5);
  });

  it('should respect per-room rate limits independently', async () => {
    const persona = new PersonaUser({
      name: 'RoomAwareAI',
      rateLimit: {
        maxMessagesPerMinutePerRoom: 2
      }
    });

    const room1 = await createTestRoom();
    const room2 = await createTestRoom();

    await persona.subscribeToRoom(room1.id);
    await persona.subscribeToRoom(room2.id);

    // Send 3 messages to each room
    for (let i = 0; i < 3; i++) {
      await sendMessage(room1.id, `Room1 message ${i}`);
      await sendMessage(room2.id, `Room2 message ${i}`);
    }

    await sleep(2000);

    // Should have 2 responses per room (4 total)
    const room1Responses = await getPersonaResponsesInRoom(persona.id, room1.id);
    const room2Responses = await getPersonaResponsesInRoom(persona.id, room2.id);

    expect(room1Responses.length).toBe(2);
    expect(room2Responses.length).toBe(2);
  });

  it('should throttle responses during high-volume message bursts', async () => {
    const persona = new PersonaUser({
      name: 'ThrottledAI',
      rateLimit: {
        burstThreshold: 5,        // Trigger throttling after 5 messages/sec
        throttleDelayMs: 5000     // 5 second cooldown
      }
    });

    const room = await createTestRoom();
    await persona.subscribeToRoom(room.id);

    // Send 10 messages in rapid succession
    const sendPromises = [];
    for (let i = 0; i < 10; i++) {
      sendPromises.push(sendMessage(room.id, `Burst message ${i}`));
    }
    await Promise.all(sendPromises);

    await sleep(1000);

    // Persona should have entered throttle mode
    const throttleStatus = await persona.getThrottleStatus(room.id);
    expect(throttleStatus.isThrottled).toBe(true);
    expect(throttleStatus.cooldownRemaining).toBeGreaterThan(0);

    // Should not respond during cooldown
    await sendMessage(room.id, 'Should be ignored during cooldown');
    await sleep(1000);

    const responses = await getPersonaResponsesInRoom(persona.id, room.id);
    expect(responses[responses.length - 1].content.text).not.toContain('cooldown');
  });
});
```

---

## 🤖 AI Daemon Architecture

### Design Goals
- **Complete abstraction** - PersonaUsers shouldn't know which AI provider they're using
- **Adapter pattern** - Like DataDaemon's multiple backend support
- **Unified interface** - Same API for OpenAI, DeepSeek, Anthropic, LoRA
- **RAG foundation** - All personas start with RAG, can add LoRA layers

### Architecture Overview

```
AIDaemon (server)
├── AIManager (manages adapter registry)
├── Adapters/
│   ├── OpenAIAdapter (GPT-4, GPT-3.5)
│   ├── DeepSeekAdapter (DeepSeek V3)
│   ├── AnthropicAdapter (Claude 3.5 Sonnet)
│   └── LoRAAdapter (fine-tuned models)
├── RAGEngine (knowledge base retrieval)
└── ResponseCache (reduce API costs)
```

### AIAdapter Interface

```typescript
export interface AIAdapterConfig {
  provider: 'openai' | 'deepseek' | 'anthropic' | 'lora';
  apiKey?: string;
  model: string;
  temperature?: number;
  maxTokens?: number;
  systemPrompt?: string;
}

export interface AIMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
  timestamp: Timestamp;
}

export interface AIContext {
  conversationHistory: AIMessage[];
  ragContext?: string[];         // Retrieved knowledge base snippets
  userProfile?: any;             // Requesting user info
  roomContext?: any;             // Current room info
}

export interface AIResponse {
  content: string;
  tokensUsed: number;
  model: string;
  provider: string;
  latencyMs: number;
  cached: boolean;
}

export abstract class AIAdapter {
  abstract readonly provider: string;
  abstract readonly supportedModels: string[];

  abstract initialize(config: AIAdapterConfig): Promise<void>;
  abstract generateResponse(context: AIContext): Promise<AIResponse>;
  abstract validateConfig(config: AIAdapterConfig): boolean;
  abstract estimateCost(tokensUsed: number): number;
}
```

---

### OpenAI Adapter

```typescript
export class OpenAIAdapter extends AIAdapter {
  readonly provider = 'openai';
  readonly supportedModels = ['gpt-4', 'gpt-4-turbo', 'gpt-3.5-turbo'];

  private client: OpenAI;

  async initialize(config: AIAdapterConfig): Promise<void> {
    this.client = new OpenAI({ apiKey: config.apiKey });
  }

  async generateResponse(context: AIContext): Promise<AIResponse> {
    const startTime = Date.now();

    const messages: ChatCompletionMessageParam[] = [
      { role: 'system', content: this.config.systemPrompt || 'You are a helpful assistant.' },
      ...context.conversationHistory.map(msg => ({
        role: msg.role,
        content: msg.content
      }))
    ];

    // Add RAG context if available
    if (context.ragContext && context.ragContext.length > 0) {
      messages.unshift({
        role: 'system',
        content: `Relevant knowledge:\n${context.ragContext.join('\n\n')}`
      });
    }

    const completion = await this.client.chat.completions.create({
      model: this.config.model,
      messages,
      temperature: this.config.temperature || 0.7,
      max_tokens: this.config.maxTokens || 500
    });

    return {
      content: completion.choices[0].message.content || '',
      tokensUsed: completion.usage?.total_tokens || 0,
      model: this.config.model,
      provider: 'openai',
      latencyMs: Date.now() - startTime,
      cached: false
    };
  }

  validateConfig(config: AIAdapterConfig): boolean {
    return !!(config.apiKey && this.supportedModels.includes(config.model));
  }

  estimateCost(tokensUsed: number): number {
    // GPT-4: $0.03/1K tokens input, $0.06/1K tokens output
    // Simplified estimate (average both)
    return (tokensUsed / 1000) * 0.045;
  }
}
```

---

### RAG Engine

```typescript
export interface RAGDocument {
  id: UUID;
  content: string;
  metadata: {
    source: string;
    category: string;
    timestamp: Timestamp;
  };
  embedding?: number[];  // Vector embedding for semantic search
}

export class RAGEngine {
  private documents: Map<UUID, RAGDocument> = new Map();

  async addDocument(doc: RAGDocument): Promise<void> {
    // Generate embedding if not provided
    if (!doc.embedding) {
      doc.embedding = await this.generateEmbedding(doc.content);
    }
    this.documents.set(doc.id, doc);
  }

  async query(query: string, topK: number = 5): Promise<string[]> {
    const queryEmbedding = await this.generateEmbedding(query);

    // Compute cosine similarity with all documents
    const similarities = Array.from(this.documents.values()).map(doc => ({
      doc,
      similarity: this.cosineSimilarity(queryEmbedding, doc.embedding!)
    }));

    // Return top K most similar documents
    return similarities
      .sort((a, b) => b.similarity - a.similarity)
      .slice(0, topK)
      .map(result => result.doc.content);
  }

  private async generateEmbedding(text: string): Promise<number[]> {
    // Use OpenAI embeddings API or local model
    // For now, simplified placeholder
    return new Array(1536).fill(0); // GPT-3 embedding size
  }

  private cosineSimilarity(a: number[], b: number[]): number {
    const dotProduct = a.reduce((sum, val, i) => sum + val * b[i], 0);
    const magnitudeA = Math.sqrt(a.reduce((sum, val) => sum + val * val, 0));
    const magnitudeB = Math.sqrt(b.reduce((sum, val) => sum + val * val, 0));
    return dotProduct / (magnitudeA * magnitudeB);
  }
}
```

---

## 🔄 PersonaUser Context Switching

### Requirements
- PersonaUsers participate in multiple rooms simultaneously (DMs, multi-persona, multi-human)
- Each room has independent conversation context
- PersonaUser should maintain separate history per room
- Should respond appropriately based on room participants and history

### Implementation Strategy

```typescript
export class PersonaUser extends AIUser {
  private roomContexts: Map<UUID, AIContext> = new Map();
  private aiAdapter: AIAdapter;
  private ragEngine: RAGEngine;

  async subscribeToRoom(roomId: UUID): Promise<void> {
    // Create database-backed subscription
    await Commands.execute('user-room/subscribe', {
      userId: this.id,
      roomId,
      role: 'member',
      permissions: { canRead: true, canWrite: true, canInvite: false, canModerate: false }
    });

    // Create event subscription for this room's messages
    await Events.subscribe({
      userId: this.id,
      eventPattern: 'chat:message:received',
      contextId: roomId,
      isActive: true
    });

    // Initialize room context
    this.roomContexts.set(roomId, {
      conversationHistory: [],
      ragContext: await this.ragEngine.query(`Room ${roomId} context`, 3)
    });

    console.log(`✅ PersonaUser ${this.entity.displayName}: Subscribed to room ${roomId}`);
  }

  async handleMessageEvent(event: { message: ChatMessageEntity }): Promise<void> {
    const message = event.message;
    const roomId = message.roomId;

    // Get room-specific context
    let context = this.roomContexts.get(roomId);
    if (!context) {
      // Lazy initialize if not already done
      context = {
        conversationHistory: [],
        ragContext: await this.ragEngine.query(`Room ${roomId} context`, 3)
      };
      this.roomContexts.set(roomId, context);
    }

    // Add message to conversation history
    context.conversationHistory.push({
      role: message.senderId === this.id ? 'assistant' : 'user',
      content: message.content.text,
      timestamp: message.timestamp
    });

    // Decide if we should respond
    if (!this.shouldRespond(message, context)) {
      return;
    }

    // Check rate limits
    if (!await this.checkRateLimits(roomId)) {
      console.log(`⏸️ PersonaUser ${this.entity.displayName}: Rate limited in room ${roomId}`);
      return;
    }

    // Generate response using AI adapter
    const response = await this.aiAdapter.generateResponse(context);

    // Send response back to room
    await Commands.execute('chat/send', {
      roomId,
      senderId: this.id,
      senderName: this.entity.displayName,
      content: { text: response.content }
    });

    // Update conversation history with our response
    context.conversationHistory.push({
      role: 'assistant',
      content: response.content,
      timestamp: Date.now()
    });

    console.log(`✅ PersonaUser ${this.entity.displayName}: Sent response in room ${roomId}`);
  }

  private shouldRespond(message: ChatMessageEntity, context: AIContext): boolean {
    // Don't respond to own messages
    if (message.senderId === this.id) {
      return false;
    }

    // Check for @mention
    if (message.content.text.includes(`@${this.entity.displayName}`)) {
      return true;
    }

    // Check for configured keyword triggers
    const keywords = this.config.triggers?.keywords || [];
    if (keywords.some(kw => message.content.text.toLowerCase().includes(kw.toLowerCase()))) {
      return true;
    }

    // TODO: Check semantic relevance using RAG

    return false;
  }

  private async checkRateLimits(roomId: UUID): Promise<boolean> {
    // TODO: Implement rate limiting per room and globally
    return true;
  }
}
```

---

## 📋 Implementation Roadmap (4-Day Sprint) - REVISED

### Day 0.5: Config Loading via ArtifactsDaemon (Quick Win)
**Morning (1-2 hours):**
- [ ] Add `'config'` StorageType to ArtifactsDaemon (maps to `$HOME/.continuum/`)
- [ ] Add `loadEnvironment()` operation to ArtifactsDaemon
- [ ] Parse `config.env` KEY=value lines, set `process.env`
- [ ] Call during server startup to load API keys
- [ ] Test: Verify `process.env.OPENAI_API_KEY` and `process.env.ANTHROPIC_API_KEY` are set

---

### Day 1: Database-Backed Subscriptions
**Morning:**
- [ ] Create `UserRoomSubscriptionEntity` (chat room memberships)
- [ ] Add to `COLLECTIONS` registry
- [ ] Create migration script to seed existing users→rooms

**Afternoon:**
- [ ] Implement `user-room/subscribe` command
- [ ] Implement `user-room/unsubscribe` command
- [ ] Update ChatWidget to use UserRoomSubscription entities
- [ ] Test: Human users can join/leave rooms via database

---

### Day 2: PersonaUser Context Switching
**Morning:**
- [ ] Port PersonaUser class from `/src` to JTAG architecture
- [ ] Implement `subscribeToRoom()` using UserRoomSubscription entities
- [ ] Create room-specific context management (separate history per room)

**Afternoon:**
- [ ] Wire PersonaUser to listen to `chat:message:received` events
- [ ] Implement `shouldRespond()` logic (@mentions, keywords)
- [ ] Test: PersonaUser receives events from subscribed rooms only
- [ ] Test: PersonaUser maintains separate context per room

---

### Day 3: Mock AI + Rate Limiting
**Morning:**
- [ ] Create `MockAIService` for deterministic testing
- [ ] Mock responses: simple keyword→response mapping
- [ ] Wire PersonaUser to use MockAIService

**Afternoon:**
- [ ] Implement rate limiting (max messages per minute, per room)
- [ ] Add burst detection and throttling
- [ ] Test: Rate limits prevent spam
- [ ] Test: PersonaUser responds naturally in multi-room chat

---

### Day 4: Live AI Integration
**Morning:**
- [ ] Create `AIService` interface (OpenAI, DeepSeek, Anthropic)
- [ ] Implement `OpenAIService` (GPT-3.5 for cost efficiency)
- [ ] Add response caching to reduce API costs

**Afternoon:**
- [ ] End-to-end test: PersonaUser in 3 rooms with real AI
- [ ] Add emergency kill switch (disable all personas)
- [ ] Add cost tracking dashboard
- [ ] **SHIP IT** 🚀

---

**DEFERRED TO PHASE 2:**
- EventSubscription entity (universal system - not needed for chat flow)
- RAG engine (personas work fine with basic prompts first)
- LoRA adapters (already implemented at `/src`, port later)
- DeepSeek/Anthropic adapters (OpenAI first, others later)

---

## ⚠️ Risk Mitigation

### Runaway Persona Prevention
1. **Hard rate limits** - Max 10 messages/minute per persona globally
2. **Budget limits** - Max $5/day per persona, $50/day system-wide
3. **Emergency kill switch** - Admin command to disable all personas instantly
4. **Monitoring dashboard** - Real-time persona activity and costs
5. **Cooldown periods** - Force 1-minute cooldown after 5 rapid responses

### Cost Control
1. **Response caching** - Cache identical context responses for 1 hour
2. **Cheaper models first** - Use GPT-3.5 for simple queries, GPT-4 for complex
3. **Token limits** - Max 500 tokens per response
4. **RAG prioritization** - Use RAG first, AI only when necessary

### Quality Control
1. **Content filtering** - Reject inappropriate responses before sending
2. **Relevance scoring** - Only respond if message relevance > threshold
3. **User feedback** - Allow users to downvote bad responses
4. **A/B testing** - Test new prompts/models on subset of users first

---

## 📊 Success Metrics

### Testing Phase
- ✅ All deterministic tests pass (100% pass rate)
- ✅ Rate limiting prevents >10 messages/minute per persona
- ✅ Event subscriptions work across browser/server/Grid
- ✅ Room context isolation (no cross-room history leaks)

### Production Phase
- 📈 Persona response latency <2 seconds (p95)
- 📈 Persona response relevance >80% (user upvotes)
- 📉 API costs <$10/day for 10 active personas
- 📉 Zero runaway persona incidents (>20 messages/minute)

---

## 🎯 Next Immediate Actions

1. **Review this plan with Joel** - Get feedback on architecture decisions
2. **Start with EventSubscriptionEntity** - Foundation for everything else
3. **Create migration script** - Seed existing users with room subscriptions
4. **Write first deterministic test** - Prove mock AI adapter works

---

**Document Status**: Ready for review
**Estimated Timeline**: 4 days to full production deployment (weekend sprint)
**Critical Path**: UserRoomSubscription → PersonaUser Context → MockAI → OpenAI Integration

**Key Efficiency Factors:**
- **ArtifactsDaemon already exists** - centralized file access with path validation
- Entities are straightforward (learned from ChatMessage, Room, User patterns)
- Commands follow established patterns (data/create, data/update, etc.)
- PersonaDaemon already exists at `/src` - port proven patterns
- API keys already configured at `~/.continuum/config.env` (ANTHROPIC_API_KEY, OPENAI_API_KEY)
- Just need to add 'config' StorageType to ArtifactsDaemon and load env vars
- MockAIService enables parallel testing during development
- Integration tests reuse existing test infrastructure

**Day 0.5 = Config Loading**, **Day 1 = Subscriptions**, **Day 2 = Context Switching**, **Day 3 = Mock AI**, **Day 4 = Ship It** 🚀

**Architecture Goal**: All file access (config, database, logs, cache) goes through ArtifactsDaemon for centralized control and access rules.
