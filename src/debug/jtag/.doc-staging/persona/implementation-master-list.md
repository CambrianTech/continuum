# PersonaUser Implementation - Master Resource List

## Existing System Components (What We Have)

### 1. Daemon System
```
daemons/
├── command-daemon/          ✅ Command execution (Commands.execute)
├── events-daemon/           ✅ Event pub/sub (Events.emit/subscribe)
├── data-daemon/            ✅ Database operations (CRUD)
├── user-daemon/            ✅ User management (creates PersonaUsers)
├── artifacts-daemon/        ✅ File storage (RAG context storage)
├── ai-provider-daemon/      ✅ LLM API integration (Anthropic + OpenAI)
├── file-daemon/            ✅ File operations
├── session-daemon/         ✅ Session management
├── console-daemon/         ✅ Logging
├── health-daemon/          ✅ System health monitoring
├── proxy-daemon/           ✅ HTTP proxy
└── widget-daemon/          ✅ Widget management
```

### 2. Core System
```
system/core/
├── shared/
│   ├── Commands.ts          ✅ Universal command execution API
│   ├── Events.ts            ✅ Universal event system
│   ├── EventConstants.ts    ✅ Event name constants
│   ├── JTAGBase.ts         ✅ Base system class
│   ├── JTAGModule.ts       ✅ Module system
│   └── RouterRegistry.ts    ✅ Router discovery
│
├── client/
│   ├── shared/JTAGClient.ts ✅ Client API (for PersonaUsers)
│   └── services/           ✅ Chat, User, Widget services
│
├── router/                 ✅ Message routing system
├── types/                  ✅ Type definitions
└── config/                 ✅ Configuration system
```

### 3. Data Layer
```
system/data/
├── entities/
│   ├── UserEntity.ts       ✅ User data structure
│   ├── UserStateEntity.ts  ✅ User state (theme, tabs, etc.)
│   ├── ChatMessageEntity.ts ✅ Chat messages
│   ├── RoomEntity.ts       ✅ Chat rooms
│   └── ...                 ✅ Other entities
│
├── config/
│   ├── DatabaseConfig.ts   ✅ Database configuration
│   └── COLLECTIONS.ts      ✅ Collection names
│
└── storage/
    └── SQLite adapters     ✅ Data persistence
```

### 4. User System
```
system/user/
├── shared/
│   ├── BaseUser.ts         ✅ Base user class
│   ├── HumanUser.ts        ✅ Human users
│   ├── AIUser.ts           ✅ AI user base class
│   ├── PersonaUser.ts      ✅ Internal AI citizens (our target!)
│   └── AgentUser.ts        ✅ External AI agents
│
├── storage/
│   ├── IUserStateStorage.ts ✅ Storage interface
│   └── MemoryStateBackend.ts ✅ In-memory storage
│
└── config/
    └── UserCapabilitiesDefaults.ts ✅ Default configs
```

### 5. Event System
```
✅ Events.emit<T>(eventName, data)  - Emit events
✅ Events.subscribe<T>(pattern, callback) - Subscribe to events
✅ EventConstants.DATA_EVENTS       - Standard event names
✅ EventConstants.UI_EVENTS
✅ EventConstants.SYSTEM_EVENTS
✅ getDataEventName(collection, operation) - Dynamic event names
```

### 6. Command System
```
✅ Commands.execute<P, R>(command, params) - Execute commands
✅ CommandConstants.DATA_COMMANDS   - Data operations
✅ CommandConstants.STATE_COMMANDS  - State operations
✅ CommandConstants.FILE_COMMANDS   - File operations
✅ CommandConstants.DEBUG_COMMANDS  - Debug operations
```

### 7. AI Provider Integration
```
daemons/ai-provider-daemon/
✅ Anthropic API integration (Claude)
✅ OpenAI API integration (GPT)
✅ API key management
✅ Request/response handling
✅ Token counting
✅ Rate limiting
```

---

## What PersonaUsers Need (Implementation Checklist)

### Phase 1: Basic Response System ⏭️

#### 1.1 Response Decision Logic
```typescript
// PersonaUser.ts enhancements needed:

interface ResponseDecisionSystem {
  // ⏭️ Implement
  shouldRespondToMessage(message: ChatMessageEntity): Promise<ResponseDecision>;

  // Dependencies (already have):
  ✅ isSenderHuman(senderId)  - Already implemented
  ✅ isPersonaMentioned(text) - Already implemented
  ✅ myRoomIds                - Already tracking room membership
}
```

**What we need to add:**
- [ ] Keyword matching system
- [ ] Relevance scoring
- [ ] Response probability calculation

#### 1.2 Rate Limiting System
```typescript
interface RateLimitingSystem {
  // ⏭️ Implement
  isRateLimited(roomId: UUID): boolean;
  trackResponse(roomId: UUID): void;
  getRateLimitState(roomId: UUID): RateLimitState;

  // Dependencies (already have):
  ✅ Per-persona SQLite storage (.continuum/personas/{id}/state.sqlite)
  ✅ Room membership tracking
}
```

**What we need to add:**
- [ ] SQLite schema for rate limit tracking
- [ ] Per-minute/per-hour counters
- [ ] Consecutive response tracking
- [ ] Cooldown period management

#### 1.3 Timing Controls
```typescript
interface TimingSystem {
  // ⏭️ Implement
  canRespondNow(message: ChatMessageEntity): Promise<TimingDecision>;
  calculateThinkingTime(message: ChatMessageEntity): number;
  scheduleDelayedResponse(message, responseText, delay): Promise<void>;

  // Dependencies (already have):
  ✅ Event system for scheduling
  ✅ Database for tracking timestamps
}
```

**What we need to add:**
- [ ] Minimum time between messages enforcement
- [ ] Artificial "thinking time" calculation
- [ ] Delayed response scheduling
- [ ] Room-wide rapid-fire detection

---

### Phase 2: AI-to-AI Interaction Protocol ⏭️

#### 2.1 Conversation State Tracking
```typescript
interface ConversationStateSystem {
  // ⏭️ Implement
  getConversationState(roomId: UUID): Promise<RoomConversationState>;
  updateConversationState(roomId, message): Promise<void>;
  calculateTemperature(state): number;

  // Dependencies (already have):
  ✅ Data daemon for room queries
  ✅ Event system for message tracking
  ✅ Room membership data
}
```

**What we need to add:**
- [ ] RoomConversationState interface
- [ ] Message count tracking (per minute, per 5 minutes)
- [ ] Participant tracking (humans vs AIs)
- [ ] Speaker sequence tracking
- [ ] Temperature calculation algorithm
- [ ] Conclusion signal detection

#### 2.2 Turn-Taking Protocol
```typescript
interface TurnTakingSystem {
  // ⏭️ Implement
  calculateTurnProbability(persona, state): number;
  checkParticipationRatio(persona, state): number;
  detectHumanDisengagement(state): boolean;

  // Dependencies (already have):
  ✅ Room message history (via data daemon)
  ✅ User type detection (isSenderHuman)
}
```

**What we need to add:**
- [ ] Turn probability algorithm
- [ ] Participation ratio limits (40% max)
- [ ] Human activity detection
- [ ] Conversation cooldown logic

---

### Phase 3: LLM Integration ⏭️

#### 3.1 AI Provider Access
```typescript
interface LLMIntegrationSystem {
  // ⏭️ Implement
  generateResponse(context: ConversationContext): Promise<string>;
  loadRAGContext(roomId: UUID): Promise<PersonaRAGContext>;
  assemblePrompt(persona, message, context): string;

  // Dependencies (already have):
  ✅ ai-provider-daemon     - API access
  ✅ artifacts-daemon       - RAG storage
  ✅ data-daemon           - Message history
}
```

**What we need to add:**
- [ ] System prompt templates for personas
- [ ] RAG context assembly
- [ ] Token counting and context window management
- [ ] Response generation logic
- [ ] Error handling for API failures

#### 3.2 RAG Context Management
```typescript
interface RAGSystem {
  // Already have basic structure:
  ✅ storeRAGContext(roomId, context)
  ✅ loadRAGContext(roomId)
  ✅ updateRAGContext(roomId, message)

  // ⏭️ Need to implement:
  - [ ] Actual artifact daemon integration
  - [ ] Context summarization when approaching token limits
  - [ ] Semantic search for relevant history
  - [ ] Token counting
}
```

---

### Phase 4: Persona Configuration System ⏭️

#### 4.1 Persona Configs
```typescript
interface PersonaConfigSystem {
  // ⏭️ Implement
  loadPersonaConfig(personaId: UUID): PersonaConfig;
  getKeywords(): string[];
  getResponseTemplates(): ResponseTemplate[];
  getTimingLimits(): TimingLimits;
  getRateLimits(): RateLimits;

  // Dependencies (already have):
  ✅ File system access
  ✅ JSON parsing
  ✅ Per-persona directories
}
```

**What we need to add:**
- [ ] PersonaConfig interface
- [ ] Configuration file format
- [ ] Default configs for each persona type
- [ ] Config loading/validation

#### 4.2 Keyword System
```typescript
interface KeywordSystem {
  // ⏭️ Implement
  checkKeywordMatch(text: string): KeywordMatch;
  calculateRelevance(text, keywords): number;

  // Dependencies (already have):
  ✅ String matching utilities
}
```

**What we need to add:**
- [ ] Keyword matching algorithm
- [ ] Relevance scoring
- [ ] Synonym/variant handling

---

### Phase 5: Collaboration Features ⏭️

#### 5.1 Handoff Protocol
```typescript
interface CollaborationSystem {
  // ⏭️ Implement
  handoffTo(nextPersona: string, context): Promise<void>;
  requestAssistance(specialist, question): Promise<void>;
  deferToSpecialist(persona): boolean;

  // Dependencies (already have):
  ✅ Chat message posting (via Commands)
  ✅ Persona discovery (via user daemon)
  ✅ @mention parsing
}
```

**What we need to add:**
- [ ] Handoff message format
- [ ] Specialist discovery
- [ ] Collaboration state tracking

#### 5.2 Multi-AI Coordination
```typescript
interface CoordinationSystem {
  // ⏭️ Implement
  detectActivePersonas(roomId): Promise<UUID[]>;
  checkIfPersonaAvailable(personaId): Promise<boolean>;
  broadcastToPersonas(message): Promise<void>;

  // Dependencies (already have):
  ✅ Room membership data
  ✅ User daemon for persona queries
  ✅ Event system for broadcasting
}
```

---

## Implementation Resources (What We Can Use)

### 1. JTAGClient - Already Available ✅
```typescript
// PersonaUsers already have access to:
this.client.daemons.commands.execute<P, R>(command, params)
this.client.daemons.events.emit<T>(event, data)
this.client.daemons.events.on<T>(pattern, callback)
this.client.daemons.data.store<T>(collection, entity)
this.client.daemons.data.list<T>(collection, params)
this.client.daemons.artifacts.write(path, data)
this.client.context    // JTAG context
this.client.sessionId  // Session ID
```

### 2. AI Provider Access ✅
```typescript
// Via ai-provider-daemon (already integrated):
{
  provider: 'anthropic' | 'openai',
  model: 'claude-3.5-sonnet' | 'gpt-4',
  apiKey: string,  // From config
  systemPrompt: string,
  messages: Message[],
  maxTokens: number,
  temperature: number
}
```

### 3. Storage Access ✅
```typescript
// Per-persona SQLite database:
.continuum/personas/{persona-id}/state.sqlite

// Available via:
✅ data-daemon (for shared collections)
✅ artifacts-daemon (for RAG context files)
✅ Direct SQLite access (for persona-specific data)
```

### 4. Event System ✅
```typescript
// Subscribe to chat messages:
✅ Events.subscribe('data:ChatMessage:created', handler)

// Subscribe to room updates:
✅ Events.subscribe('data:Room:updated', handler)

// Subscribe to user events:
✅ Events.subscribe('data:User:*', handler)

// Emit custom events:
✅ Events.emit('persona:response-generated', data)
```

### 5. Command System ✅
```typescript
// Create messages:
✅ Commands.execute(DATA_COMMANDS.CREATE, {
    collection: 'ChatMessage',
    data: messageEntity
  })

// Query users:
✅ Commands.execute(DATA_COMMANDS.READ, {
    collection: 'User',
    id: userId
  })

// List room messages:
✅ Commands.execute(DATA_COMMANDS.LIST, {
    collection: 'ChatMessage',
    filter: { roomId: roomId }
  })
```

---

## Configuration Access (What We Have)

### API Keys (Already Configured) ✅
```typescript
// Access via environment or config:
process.env.ANTHROPIC_API_KEY  ✅ Available
process.env.OPENAI_API_KEY     ✅ Available

// Or via config system:
Config.get('ai.anthropic.apiKey')
Config.get('ai.openai.apiKey')
```

### Database Paths ✅
```typescript
// Main database:
.continuum/data/continuum.sqlite  ✅ Shared data

// Per-persona databases:
.continuum/personas/{id}/state.sqlite  ✅ Persona-specific

// Session data:
.continuum/sessions/user/shared/{sessionId}/  ✅ Session storage
```

### Logging ✅
```typescript
// Via console daemon:
console.log('message')  ✅ Logged to .continuum/.../logs/
console.error('error')  ✅ Error logging

// Log files:
.continuum/.../logs/server.log   ✅ Server logs
.continuum/.../logs/browser.log  ✅ Browser logs
```

---

## Implementation Priority

### Week 1: Basic Response System
1. ✅ PersonaUser class (already have base)
2. ⏭️ Rate limiting SQLite schema
3. ⏭️ Timing enforcement
4. ⏭️ Basic keyword matching
5. ⏭️ Simple response generation (templates)

### Week 2: AI-to-AI Protocol
6. ⏭️ Conversation state tracking
7. ⏭️ Turn-taking probability
8. ⏭️ Participation ratio limits
9. ⏭️ Temperature calculation
10. ⏭️ Conclusion detection

### Week 3: LLM Integration
11. ⏭️ AI provider daemon integration
12. ⏭️ RAG context loading
13. ⏭️ Prompt assembly
14. ⏭️ Response generation
15. ⏭️ Context summarization

### Week 4: Collaboration Features
16. ⏭️ Handoff protocol
17. ⏭️ Specialist discovery
18. ⏭️ Multi-AI coordination
19. ⏭️ Academy training basics

---

## Quick Start: Enable Basic Persona Responses

### Minimal Implementation (can do today):

1. **Remove response disable** (PersonaUser.ts:135):
```typescript
// REMOVE:
console.log(`🚫 CLAUDE-FIX-${Date.now()}: ${this.displayName}: Persona responses DISABLED for debugging`);
return;

// ADD:
const decision = await this.shouldRespond(message);
if (decision.shouldRespond) {
  await this.respondWithTemplate(message);
}
```

2. **Add basic rate limiting**:
```typescript
private lastResponseTime: Map<UUID, Date> = new Map();
private readonly minSecondsBetweenResponses = 10;

private isRateLimited(roomId: UUID): boolean {
  const lastTime = this.lastResponseTime.get(roomId);
  if (!lastTime) return false;

  const seconds = (Date.now() - lastTime.getTime()) / 1000;
  return seconds < this.minSecondsBetweenResponses;
}
```

3. **Add AI-to-AI check**:
```typescript
private async shouldRespond(message: ChatMessageEntity): Promise<{shouldRespond: boolean}> {
  // Already have:
  if (message.senderId === this.id) return { shouldRespond: false };

  // Check if sender is AI (already implemented):
  const senderIsHuman = await this.isSenderHuman(message.senderId);
  if (!senderIsHuman) return { shouldRespond: false };

  // Check rate limit:
  if (this.isRateLimited(message.roomId)) return { shouldRespond: false };

  // Check if mentioned:
  if (this.isPersonaMentioned(message.content?.text || '')) {
    return { shouldRespond: true };
  }

  return { shouldRespond: false };
}
```

**That's it!** With these 3 changes, personas will:
- ✅ Only respond to humans (not AIs)
- ✅ Only respond when @mentioned
- ✅ Rate limit to once per 10 seconds per room
- ✅ Use existing template responses

**This can be implemented in < 30 minutes and tested immediately.**

---

## Testing Strategy

### Test 1: Basic Response
```bash
# 1. Start system
npm start

# 2. Send message mentioning persona
# (via chat widget or test script)
"@CodeAI what do you think?"

# 3. Verify response appears within 10 seconds
# 4. Verify no infinite loops
# 5. Verify rate limiting works
```

### Test 2: AI-to-AI Prevention
```bash
# 1. Have 2 personas in same room
# 2. One persona posts message
# 3. Verify other persona does NOT respond
# 4. @mention second persona
# 5. Verify it responds
```

### Test 3: Multi-Room Isolation
```bash
# 1. Put persona in 2 rooms
# 2. Send messages in both rooms
# 3. Verify rate limiting is per-room
# 4. Verify persona tracks state separately
```

---

## Summary: We Have Everything We Need!

✅ **Daemon Infrastructure** - All daemons in place
✅ **Event System** - Real-time event pub/sub
✅ **Command System** - Universal command execution
✅ **Data Layer** - SQLite + entities
✅ **AI Providers** - Anthropic + OpenAI integrated
✅ **User System** - BaseUser → AIUser → PersonaUser
✅ **Storage** - Per-persona SQLite + artifacts
✅ **Configuration** - API keys + config system
✅ **JTAGClient** - Clean API for personas

⏭️ **What We Need to Build**:
1. Response decision logic (~100 lines)
2. Rate limiting system (~150 lines)
3. Timing controls (~200 lines)
4. Conversation state tracking (~250 lines)
5. LLM integration (~300 lines)
6. Collaboration protocol (~200 lines)

**Total: ~1200 lines of code to make it work.**

**The foundation is rock solid. Now we just build on top of it.**
