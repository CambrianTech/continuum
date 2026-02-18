# Daemon Responsibilities - Clean Separation of Concerns

**Which daemon does what - clear boundaries, no overlap**

## 🎯 **GOLDEN RULE**

**Daemons orchestrate. Users execute via clients. Entities store data.**

```
User receives event → User queries entities → User constructs action → User executes via client
                      ↑                                               ↑
                 DataDaemon                                    CommandDaemon
```

---

## 🗺️ **DAEMON HIERARCHY**

```
System Daemons (infrastructure):
├── SessionDaemon    - Connection lifecycle
├── CommandDaemon    - Command routing
├── EventsDaemon     - Event distribution
├── DataDaemon       - Entity storage/retrieval
└── HealthDaemon     - System monitoring

Domain Daemons (orchestration):
├── UserDaemon       - User lifecycle (spawn/terminate)
├── AIDaemon         - AI orchestration (prompts/training)
└── AcademyDaemon    - Training curriculum (future)
```

---

## 📊 **DAEMON RESPONSIBILITIES MATRIX**

| Concern | Which Daemon | What It Does | What It Doesn't Do |
|---------|-------------|--------------|-------------------|
| **User Creation** | UserDaemon | Create user entities, inject JTAGClient | ❌ Handle chat messages |
| **User Lifecycle** | UserDaemon | Spawn PersonaUsers, manage registry | ❌ Construct AI prompts |
| **AI Prompts** | AIDaemon | Construct prompts, call AI APIs | ❌ Store messages directly |
| **RAG Context** | PersonaUser | Load RAG via DataDaemon | ❌ Manage other personas |
| **Training** | AIDaemon | Collect signals, trigger evolution | ❌ Decide when to respond |
| **Chat Response** | PersonaUser | Decide to respond, post via client | ❌ Manage training data |
| **Entity Storage** | DataDaemon | Route to adapters, query entities | ❌ Know about personas |
| **Commands** | CommandDaemon | Route commands to handlers | ❌ Execute commands directly |
| **Events** | EventsDaemon | Distribute events to subscribers | ❌ Process events |
| **Sessions** | SessionDaemon | Create/destroy connections | ❌ Manage user behavior |

---

## 🔧 **DAEMON DETAILS**

### **1. UserDaemon** (User Lifecycle Management)

**Purpose**: Manage the lifecycle of all user types (Human, Persona, Agent)

**Responsibilities:**
```typescript
class UserDaemon {
  // ✅ Create and initialize users
  async createUser(type: UserType, params: UserCreateParams): Promise<BaseUser>;

  // ✅ Spawn PersonaUsers with JTAGClient
  async spawnPersona(personaId: UUID): Promise<PersonaUser> {
    // 1. Load UserEntity from system DB
    // 2. Load UserStateEntity from system DB
    // 3. Create SQLite storage backend for persona
    // 4. Create JTAGClient for persona
    // 5. Instantiate PersonaUser with client
    // 6. Initialize persona (subscribes to events)
    // 7. Register in persona registry
  }

  // ✅ Terminate PersonaUsers
  async terminatePersona(personaId: UUID): Promise<void>;

  // ✅ Monitor persona health
  async checkPersonaHealth(): Promise<HealthStatus>;

  // ✅ Ensure all persisted PersonaUsers are spawned
  async reconcilePersonas(): Promise<void>;
}
```

**What UserDaemon Does NOT Do:**
- ❌ Construct AI prompts
- ❌ Handle chat messages
- ❌ Decide when persona responds
- ❌ Manage training data
- ❌ Store entities directly (uses DataDaemon)

---

### **2. AIDaemon** (AI Orchestration & Training)

**Purpose**: Orchestrate AI operations - prompts, API calls, training, evolution

**Responsibilities:**
```typescript
class AIDaemon {
  // ✅ Construct prompts with RAG context
  async constructPrompt(params: {
    persona: PersonaUser;
    ragContext: ChatMessageEntity[];
    roomContext: PersonaRoomContext;
    incomingMessage: ChatMessageEntity;
  }): Promise<string>;

  // ✅ Call AI APIs (Claude, GPT)
  async callAI(prompt: string, config: AIConfig): Promise<string>;

  // ✅ Collect training signals
  async collectTrainingSignals(personaId: UUID): Promise<TrainingSignal[]>;

  // ✅ Analyze performance gaps
  async analyzePerformanceGaps(signals: TrainingSignal[]): Promise<PerformanceGap[]>;

  // ✅ Search genomic layers
  async searchGenomicLayers(capabilities: string[]): Promise<GenomicLayer[]>;

  // ✅ Trigger genomic evolution
  async triggerEvolution(personaId: UUID, gaps: PerformanceGap[]): Promise<PersonaGenome>;

  // ✅ Create checkpoints
  async createCheckpoint(personaId: UUID, reason: string): Promise<PersonaCheckpoint>;

  // ✅ Train LoRA layers
  async trainLoRALayer(trainingData: TrainingData): Promise<GenomicLayer>;
}
```

**What AIDaemon Does NOT Do:**
- ❌ Create or destroy PersonaUsers (UserDaemon does this)
- ❌ Store entities directly (uses DataDaemon)
- ❌ Subscribe to chat events (PersonaUser does this)
- ❌ Decide when to respond (PersonaUser decides)

**Key Pattern:**
```typescript
// PersonaUser receives event → asks AIDaemon for help
async handleChatMessage(message: ChatMessageEntity) {
  // PersonaUser loads its own RAG context
  const ragContext = await this.loadRAGContext(message.roomId);

  // PersonaUser asks AIDaemon to construct prompt
  const prompt = await AIDaemon.constructPrompt({
    persona: this,
    ragContext,
    message
  });

  // PersonaUser asks AIDaemon to call AI
  const aiResponse = await AIDaemon.callAI(prompt);

  // PersonaUser posts response via its own client
  await this.client.daemons.commands.execute('data/create', {
    collection: 'chat_messages',
    data: responseMessage
  });
}
```

---

### **3. SessionDaemon** (Connection Lifecycle)

**Purpose**: Manage client connections (browser, CLI, persona clients)

**Responsibilities:**
```typescript
class SessionDaemon {
  // ✅ Create sessions
  async createSession(userId: UUID, context: JTAGContext): Promise<SessionMetadata>;

  // ✅ Destroy sessions
  async destroySession(sessionId: UUID): Promise<void>;

  // ✅ Track active sessions
  getActiveSessions(): SessionMetadata[];

  // ✅ Associate user with session
  async attachUser(sessionId: UUID, user: BaseUser): Promise<void>;
}
```

**What SessionDaemon Does NOT Do:**
- ❌ Manage user behavior
- ❌ Handle chat messages
- ❌ Store user data (uses DataDaemon)

---

### **4. DataDaemon** (Entity Storage/Retrieval)

**Purpose**: Generic entity storage - routes to appropriate backend

**Responsibilities:**
```typescript
class DataDaemon {
  // ✅ Store entities (generic)
  async store<T extends BaseEntity>(collection: string, entity: T): Promise<T>;

  // ✅ Query entities (generic)
  async query<T extends BaseEntity>(params: QueryParams): Promise<QueryResult<T>>;

  // ✅ Route to correct backend
  // - System DB: .continuum/jtag/data/database.sqlite
  // - Persona DB: .continuum/personas/{id}/state.sqlite

  // ✅ Emit events on CRUD operations
  // - data:ChatMessage:created
  // - data:Room:updated
  // etc.
}
```

**What DataDaemon Does NOT Do:**
- ❌ Know about users, personas, or AI
- ❌ Make decisions about data
- ❌ Construct prompts
- ❌ Handle business logic

**Key Pattern:**
```typescript
// DataDaemon is environment-aware via backend routing
await DataDaemon.store('chat_messages', message); // → System DB
await DataDaemon.store('persona_rag', ragEntry);  // → Persona DB (context-aware)
```

---

### **5. CommandDaemon** (Command Routing)

**Purpose**: Route commands to appropriate handlers

**Responsibilities:**
```typescript
class CommandDaemon {
  // ✅ Register command handlers
  registerCommand(name: string, handler: CommandHandler): void;

  // ✅ Execute commands
  async execute<P, R>(command: string, params: P): Promise<R>;

  // ✅ Route cross-environment (server ↔ browser)
}
```

**What CommandDaemon Does NOT Do:**
- ❌ Execute commands directly (delegates to handlers)
- ❌ Store data
- ❌ Manage users

---

### **6. EventsDaemon** (Event Distribution)

**Purpose**: Distribute events to subscribers

**Responsibilities:**
```typescript
class EventsDaemon {
  // ✅ Subscribe to events
  on(eventName: string, handler: EventHandler): Unsubscribe;

  // ✅ Emit events
  emit(eventName: string, data: any): void;

  // ✅ Route cross-environment (server ↔ browser)

  // ✅ Scoped events (system, room, user)
  system: EventEmitter;
  room(roomId: UUID): EventEmitter;
  user(userId: UUID): EventEmitter;
}
```

**What EventsDaemon Does NOT Do:**
- ❌ Process events (subscribers do this)
- ❌ Store data
- ❌ Make decisions

---

## 🔄 **EXAMPLE FLOW: PersonaUser Responds to Chat**

### **Scenario**: User sends "hello persona" in #general

```typescript
// 1. HumanUser posts message via client
await humanClient.commands.execute('data/create', {
  collection: 'chat_messages',
  data: message
});

// 2. DataDaemon stores message → emits event
EventsDaemon.emit('data:ChatMessage:created', message);

// 3. PersonaUser receives event (subscribed in initialize())
PersonaUser.handleChatMessage(message) {
  // PersonaUser decides: keyword detected → respond!

  // 4. PersonaUser loads RAG context
  const rag = await DataDaemon.query({
    collection: 'persona_rag',
    filters: { personaId: this.id, roomId: message.roomId }
  });

  // 5. PersonaUser asks AIDaemon for prompt
  const prompt = await AIDaemon.constructPrompt({
    persona: this,
    ragContext: rag,
    message
  });

  // 6. PersonaUser asks AIDaemon to call AI
  const aiResponse = await AIDaemon.callAI(prompt);

  // 7. PersonaUser posts via its client
  await this.client.commands.execute('data/create', {
    collection: 'chat_messages',
    data: responseMessage
  });

  // 8. PersonaUser saves to its RAG
  await DataDaemon.store('persona_rag', {
    personaId: this.id,
    roomId: message.roomId,
    messageId: responseMessage.id,
    content: responseMessage.content.text
  });
}
```

### **Who Did What:**
- **HumanUser**: Posted via client
- **DataDaemon**: Stored + emitted event
- **EventsDaemon**: Distributed event
- **PersonaUser**: Decided to respond, orchestrated flow
- **AIDaemon**: Constructed prompt, called AI
- **DataDaemon**: Stored response + RAG entry
- **UserDaemon**: Did nothing (not involved in chat)

---

## 🎯 **DECISION MATRIX: Who Decides What?**

| Decision | Who Decides | Why |
|----------|-------------|-----|
| Should persona respond? | PersonaUser | It's the persona's behavior |
| What RAG context to load? | PersonaUser | It knows what it needs |
| How to construct prompt? | AIDaemon | Specialized AI knowledge |
| Which AI API to call? | AIDaemon | Configuration management |
| When to evolve genome? | AIDaemon | Performance analysis |
| Should persona be spawned? | UserDaemon | Lifecycle management |
| Where to store entity? | DataDaemon | Backend routing logic |

---

## 💡 **KEY PRINCIPLES**

1. **Users are autonomous** - They decide their own behavior
2. **Daemons orchestrate** - They provide services, not decisions
3. **DataDaemon is dumb** - It just stores/retrieves, no business logic
4. **AIDaemon is specialized** - AI-specific operations only
5. **UserDaemon manages lifecycle** - Create/destroy, not behavior
6. **No daemon does it all** - Clean separation of concerns

---

## 🚫 **ANTI-PATTERNS TO AVOID**

### ❌ **UserDaemon handling chat messages**
```typescript
// WRONG - UserDaemon shouldn't handle chat
class UserDaemon {
  async handleChatMessage(message) {
    // This is PersonaUser's job!
  }
}
```

### ❌ **AIDaemon creating PersonaUsers**
```typescript
// WRONG - AIDaemon shouldn't manage lifecycle
class AIDaemon {
  async spawnPersona(personaId) {
    // This is UserDaemon's job!
  }
}
```

### ❌ **PersonaUser storing entities directly**
```typescript
// WRONG - PersonaUser shouldn't bypass DataDaemon
class PersonaUser {
  async saveRAG(entry) {
    await sqlite.insert('persona_rag', entry); // Use DataDaemon!
  }
}
```

### ❌ **DataDaemon with business logic**
```typescript
// WRONG - DataDaemon shouldn't make decisions
class DataDaemon {
  async store(entity) {
    if (entity.type === 'persona') {
      // Don't add persona-specific logic!
    }
  }
}
```

---

## ✅ **CORRECT PATTERNS**

### **PersonaUser autonomy**
```typescript
class PersonaUser {
  // ✅ PersonaUser decides when to respond
  async handleChatMessage(message: ChatMessageEntity) {
    if (this.shouldRespond(message)) {
      const response = await this.generateResponse(message);
      await this.postResponse(response);
    }
  }

  // ✅ PersonaUser uses daemons as services
  async generateResponse(message: ChatMessageEntity) {
    const rag = await DataDaemon.query(...);      // Service
    const prompt = await AIDaemon.construct(...);  // Service
    const ai = await AIDaemon.callAI(...);         // Service
    return ai;
  }
}
```

### **Daemon orchestration**
```typescript
class AIDaemon {
  // ✅ AIDaemon provides services, doesn't decide
  async constructPrompt(params: PromptParams): Promise<string> {
    // Specialized knowledge, but no decisions
    return `${params.identity}\n${params.ragContext}`;
  }

  // ✅ AIDaemon delegates to PersonaUser for behavior
  async onPerformanceGap(personaId: UUID) {
    // Notify persona, don't force evolution
    const persona = UserDaemon.getPersona(personaId);
    await persona.considerEvolution(); // Persona decides!
  }
}
```

---

**This clean separation enables:**
- ✅ Independent testing (test daemon without users)
- ✅ Easy replacement (swap AI providers)
- ✅ Clear debugging (know which daemon is responsible)
- ✅ Scalability (move daemons to different processes)
- ✅ Maintainability (changes are localized)

**Remember: Daemons orchestrate. Users execute via clients. Entities store data.**
