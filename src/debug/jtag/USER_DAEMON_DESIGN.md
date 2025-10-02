# UserDaemon Architecture Design

## 🎯 **PURPOSE**

**UserDaemon is the persistent lifecycle manager for ALL users (Human, Agent, Persona) in the system.**

It ensures that every User entity becomes a living, participating citizen with proper state management, event subscriptions, and for AI users, active model connections.

## 🚨 **THE PROBLEM IT SOLVES**

**Current Broken State:**
1. User entity created via `data/create` → No UserState created → User exists but has no preferences/state
2. PersonaUser entity exists in database → Not connected to chat system → Can't receive or send messages
3. AgentUser entity exists → No active connection → Can't respond to events
4. No central authority managing user lifecycle → Orphaned users, inconsistent state

**What UserDaemon Fixes:**
- ✅ **Automatic UserState creation** for ALL users (Human/Agent/Persona)
- ✅ **PersonaUser "comes alive"** - connects to model API, subscribes to chat rooms
- ✅ **AgentUser registers** for system events and external API integration
- ✅ **Consistent lifecycle management** - online/offline, state persistence, cleanup

## 🏗️ **ARCHITECTURE OVERVIEW**

```
┌─────────────────────────────────────────────────────────────────┐
│                         USER DAEMON                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  EVENT LISTENERS (What UserDaemon Watches)                      │
│  ├── data:User:created      → Create UserState + Start connection│
│  ├── data:User:updated      → Update state, reconnect if needed │
│  ├── data:User:deleted      → Clean up state, close connections │
│  ├── chat:room-joined       → Subscribe persona to new room     │
│  └── system:startup         → Load existing users, reconnect    │
│                                                                  │
│  USER TYPE HANDLERS (How UserDaemon Manages Each Type)         │
│  ├── HumanUser              → Create UserState only             │
│  ├── AgentUser              → Create UserState + Register API   │
│  └── PersonaUser            → Create UserState + PersonaConnection│
│                                                                  │
│  MANAGED CONNECTIONS (Active User Instances)                    │
│  ├── personaConnections: Map<UUID, PersonaConnection>          │
│  ├── agentConnections: Map<UUID, AgentConnection>              │
│  └── humanSessions: Map<UUID, SessionMetadata>                 │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## 📋 **USER DAEMON RESPONSIBILITIES**

### **1. User Lifecycle Management**
```typescript
/**
 * Core responsibility: Ensure every User entity has complete lifecycle
 */
class UserDaemon extends BaseDaemon {
  async handleUserCreated(event: DataEvent<UserEntity>): Promise<void> {
    const user = event.data;

    // STEP 1: Create UserState (ALL user types)
    await this.createUserState(user);

    // STEP 2: Type-specific initialization
    switch (user.type) {
      case 'persona':
        await this.startPersonaConnection(user);
        break;
      case 'agent':
        await this.registerAgentConnection(user);
        break;
      case 'human':
        // Humans managed by SessionDaemon, just ensure state exists
        break;
    }

    // STEP 3: Mark user as ready
    await this.markUserOnline(user.id);

    // STEP 4: Emit user:ready event
    this.eventManager.emit('user:ready', { userId: user.id, type: user.type });
  }
}
```

### **2. UserState Auto-Creation**
```typescript
/**
 * Every User entity MUST have UserState entity
 * UserDaemon ensures this happens automatically
 */
private async createUserState(user: UserEntity): Promise<UserStateEntity> {
  // Check if UserState already exists
  const existing = await DataDaemon.query<UserStateEntity>({
    collection: 'UserState',
    filters: { userId: user.id },
    limit: 1
  });

  if (existing.data && existing.data.length > 0) {
    console.log(`✅ UserState already exists for ${user.id}`);
    return existing.data[0].data;
  }

  // Create UserState with type-specific defaults
  const userState = new UserStateEntity();
  userState.userId = user.id;
  userState.deviceId = 'system-default';
  userState.contentState = {
    openItems: [],
    lastUpdatedAt: new Date()
  };

  // Type-specific preferences
  if (user.type === 'agent') {
    userState.preferences = {
      maxOpenTabs: 5,
      autoCloseAfterDays: 1,           // Short-lived
      rememberScrollPosition: false,
      syncAcrossDevices: false
    };
  } else if (user.type === 'persona') {
    userState.preferences = {
      maxOpenTabs: 5,
      autoCloseAfterDays: 365,         // Long-lived for genomic evolution
      rememberScrollPosition: true,
      syncAcrossDevices: false
    };
  } else {
    // Human defaults
    userState.preferences = {
      maxOpenTabs: 10,
      autoCloseAfterDays: 30,
      rememberScrollPosition: true,
      syncAcrossDevices: false
    };
  }

  await DataDaemon.store<UserStateEntity>('UserState', userState);
  console.log(`✅ Created UserState for ${user.type} user ${user.id}`);

  return userState;
}
```

### **3. PersonaUser Connection Management**
```typescript
/**
 * PersonaUsers need to be "alive" - connected to model API, listening for chat
 */
private async startPersonaConnection(user: UserEntity): Promise<void> {
  // Create PersonaConnection instance
  const personaConnection = new PersonaConnection(user);

  // Initialize model API client
  await personaConnection.initializeModelClient();

  // Subscribe to rooms where persona is participant
  await personaConnection.subscribeToRooms();

  // Listen for chat messages
  await personaConnection.startMessageListener();

  // Store active connection
  this.personaConnections.set(user.id, personaConnection);

  console.log(`🧬 PersonaUser "${user.displayName}" is now ONLINE and ACTIVE`);

  // Emit persona:online event
  this.eventManager.emit('persona:online', { userId: user.id });
}
```

### **4. Storage Backend Assignment**
```typescript
/**
 * Assign correct storage backend based on user type
 * CRITICAL for genomic PersonaUsers who need SQLite persistence
 */
private getStorageBackendForUserType(userType: 'human' | 'agent' | 'persona', userId: UUID): IUserStateStorage {
  switch (userType) {
    case 'persona':
      // Personas get dedicated SQLite database for genomic evolution
      const personaDbPath = `.continuum/personas/${userId}/state.sqlite`;
      return new SQLiteStateBackend(personaDbPath);

    case 'agent':
      // Agents use ephemeral memory storage
      return new MemoryStateBackend();

    case 'human':
      // Humans use session-specific storage (managed by SessionDaemon)
      return new MemoryStateBackend();
  }
}
```

### **5. System Startup - Reconnect Existing Users**
```typescript
/**
 * On system startup, reconnect all active AI users
 * CRITICAL: Personas must resume after system restart
 */
async initialize(): Promise<void> {
  console.log('🚀 UserDaemon initializing...');

  // Subscribe to events
  this.eventManager.on('data:User:created', this.handleUserCreated.bind(this));
  this.eventManager.on('data:User:updated', this.handleUserUpdated.bind(this));
  this.eventManager.on('data:User:deleted', this.handleUserDeleted.bind(this));

  // Load existing personas and reconnect
  await this.reconnectExistingPersonas();

  // Load existing agents and reconnect
  await this.reconnectExistingAgents();

  console.log('✅ UserDaemon initialized');
}

private async reconnectExistingPersonas(): Promise<void> {
  const personas = await DataDaemon.query<UserEntity>({
    collection: 'User',
    filters: { type: 'persona', status: 'online' }
  });

  console.log(`🔄 Reconnecting ${personas.data.length} PersonaUsers...`);

  for (const record of personas.data) {
    const persona = record.data;
    await this.startPersonaConnection(persona);
  }

  console.log(`✅ Reconnected ${this.personaConnections.size} PersonaUsers`);
}
```

### **6. Cleanup on User Deletion**
```typescript
/**
 * Clean up all user resources when deleted
 */
private async handleUserDeleted(event: DataEvent<UserEntity>): Promise<void> {
  const user = event.data;

  // Close active connections
  if (user.type === 'persona') {
    const connection = this.personaConnections.get(user.id);
    if (connection) {
      await connection.close();
      this.personaConnections.delete(user.id);
    }
  }

  if (user.type === 'agent') {
    const connection = this.agentConnections.get(user.id);
    if (connection) {
      await connection.close();
      this.agentConnections.delete(user.id);
    }
  }

  // Delete UserState (cascade)
  await DataDaemon.remove('UserState', user.id);

  console.log(`✅ Cleaned up user ${user.id}`);
}
```

## 🔌 **PERSONA CONNECTION (Sub-Component)**

```typescript
/**
 * PersonaConnection - Keeps PersonaUser alive and responsive
 *
 * One instance per PersonaUser, manages:
 * - Model API connection (GPT-4, Claude, local models)
 * - Chat room subscriptions
 * - Message listening and response generation
 * - Conversation context tracking
 * - Genomic layer loading (future)
 */
class PersonaConnection {
  private user: UserEntity;
  private modelClient: ModelAPIClient;
  private subscribedRooms: Set<UUID> = new Set();
  private conversationContexts: Map<UUID, ConversationContext> = new Map();

  constructor(user: UserEntity) {
    if (user.type !== 'persona') {
      throw new Error('PersonaConnection only for PersonaUsers');
    }
    this.user = user;
  }

  /**
   * Initialize model API client
   */
  async initializeModelClient(): Promise<void> {
    // Extract model config from user entity
    const modelConfig = this.user.modelConfig || {
      baseModel: 'gpt-4',
      systemPrompt: 'You are a helpful AI assistant.',
      loraAdapterId: null  // No genomic layers yet
    };

    // Create model client
    this.modelClient = new ModelAPIClient({
      provider: this.detectProvider(modelConfig.baseModel),
      model: modelConfig.baseModel,
      systemPrompt: modelConfig.systemPrompt,
      loraAdapter: modelConfig.loraAdapterId
    });

    console.log(`🤖 Model client initialized for ${this.user.displayName}`);
  }

  /**
   * Subscribe to chat rooms where persona is participant
   */
  async subscribeToRooms(): Promise<void> {
    // Query rooms where persona is participant
    const rooms = await DataDaemon.query<RoomEntity>({
      collection: 'Room',
      filters: { 'participants': { '$elemMatch': { userId: this.user.id } } }
    });

    for (const roomRecord of rooms.data) {
      const room = roomRecord.data;
      this.subscribedRooms.add(room.id);
      console.log(`📢 Subscribed to room: ${room.name}`);
    }
  }

  /**
   * Start listening for chat messages
   */
  async startMessageListener(): Promise<void> {
    eventManager.on('chat:message-received', this.handleChatMessage.bind(this));
  }

  /**
   * Handle incoming chat message
   */
  private async handleChatMessage(event: ChatMessageEvent): Promise<void> {
    const message = event.data;

    // Only respond to messages in subscribed rooms
    if (!this.subscribedRooms.has(message.roomId)) return;

    // Don't respond to own messages
    if (message.authorId === this.user.id) return;

    // Get conversation context
    const context = await this.getConversationContext(message.roomId);

    // Generate response using model API
    const response = await this.modelClient.generateResponse({
      conversationHistory: context.messages,
      userMessage: message.content.text
    });

    // Send response back to chat
    await this.sendChatMessage(message.roomId, response);

    // Update conversation context
    context.messages.push(
      { role: 'user', content: message.content.text },
      { role: 'assistant', content: response }
    );
  }

  /**
   * Send chat message as persona
   */
  private async sendChatMessage(roomId: UUID, text: string): Promise<void> {
    await executeCommand('chat/send', {
      roomId,
      authorId: this.user.id,
      content: { text, attachments: [] }
    });
  }

  /**
   * Close connection and cleanup
   */
  async close(): Promise<void> {
    // Unsubscribe from events
    eventManager.off('chat:message-received', this.handleChatMessage.bind(this));

    // Clear contexts
    this.conversationContexts.clear();
    this.subscribedRooms.clear();

    console.log(`🔌 Closed PersonaConnection for ${this.user.displayName}`);
  }
}
```

## 🔌 **MODEL API CLIENT (External Integration)**

```typescript
/**
 * ModelAPIClient - Calls external model APIs (GPT, Claude, local models)
 *
 * Supports:
 * - OpenAI (GPT-4, GPT-3.5)
 * - Anthropic (Claude)
 * - Local models (Ollama, vLLM)
 * - LoRA adapters (genomic layers - future)
 */
class ModelAPIClient {
  private provider: 'openai' | 'anthropic' | 'local';
  private model: string;
  private systemPrompt: string;
  private loraAdapter?: string;

  constructor(config: ModelAPIConfig) {
    this.provider = config.provider;
    this.model = config.model;
    this.systemPrompt = config.systemPrompt;
    this.loraAdapter = config.loraAdapter;
  }

  /**
   * Generate response using model API
   */
  async generateResponse(request: GenerateRequest): Promise<string> {
    const messages = [
      { role: 'system', content: this.systemPrompt },
      ...request.conversationHistory,
      { role: 'user', content: request.userMessage }
    ];

    switch (this.provider) {
      case 'openai':
        return await this.callOpenAI(messages);
      case 'anthropic':
        return await this.callAnthropic(messages);
      case 'local':
        return await this.callLocalModel(messages);
    }
  }

  /**
   * Call OpenAI API
   */
  private async callOpenAI(messages: Message[]): Promise<string> {
    // TODO: Implement OpenAI API call
    // Will use OPENAI_API_KEY from environment
    throw new Error('OpenAI integration not implemented');
  }

  /**
   * Call Anthropic API
   */
  private async callAnthropic(messages: Message[]): Promise<string> {
    // TODO: Implement Anthropic API call
    // Will use ANTHROPIC_API_KEY from environment
    throw new Error('Anthropic integration not implemented');
  }

  /**
   * Call local model (Ollama, vLLM)
   */
  private async callLocalModel(messages: Message[]): Promise<string> {
    // TODO: Implement local model call
    // Will use LOCAL_MODEL_URL from environment
    throw new Error('Local model integration not implemented');
  }
}
```

## 📂 **FILE STRUCTURE**

```
daemons/user-daemon/
├── shared/
│   ├── UserDaemon.ts                    # Base daemon class
│   ├── UserDaemonTypes.ts               # Interfaces, types
│   └── PersonaConnection.ts             # PersonaUser connection manager
├── server/
│   ├── UserDaemonServer.ts              # Server-specific implementation
│   └── ModelAPIClient.ts                # External model API integration
└── browser/
    └── UserDaemonBrowser.ts             # Browser stubs (minimal)

system/user/connections/
├── PersonaConnection.ts                 # Persona-specific connection logic
├── AgentConnection.ts                   # Agent-specific connection logic
└── ModelAPIClient.ts                    # Model API integration
```

## 🔄 **INTEGRATION WITH EXISTING SYSTEM**

### **1. System Startup**
```typescript
// In system initialization (JTAGSystem or similar)
async function startSystem() {
  // Start existing daemons
  await sessionDaemon.initialize();
  await dataDaemon.initialize();
  await eventsDaemon.initialize();

  // Start UserDaemon (NEW)
  await userDaemon.initialize();
  //   ↓ Loads existing personas
  //   ↓ Reconnects PersonaConnections
  //   ↓ Subscribes to data:User events

  console.log('✅ All daemons running');
}
```

### **2. SessionDaemon Integration**
```typescript
/**
 * SessionDaemonServer.createUser() now delegates to UserDaemon
 */
class SessionDaemonServer {
  private async createUser(params: CreateSessionParams): Promise<BaseUser> {
    // Create User entity
    const userEntity = new UserEntity();
    userEntity.type = this.determineUserType(params);
    userEntity.displayName = params.displayName;

    // Store in database
    await DataDaemon.store('User', userEntity);
    //   ↓ Emits data:User:created event
    //   ↓ UserDaemon receives event
    //   ↓ UserDaemon creates UserState
    //   ↓ UserDaemon starts PersonaConnection (if persona)

    // Wait for UserState to be created
    await this.waitForUserState(userEntity.id);

    // Load UserState
    const userState = await this.loadUserState(userEntity.id);

    // Create User instance with proper storage backend
    const storage = userDaemon.getStorageBackend(userEntity.type, userEntity.id);
    const user = this.instantiateUserClass(userEntity, userState, storage);

    return user;
  }
}
```

### **3. Widget Integration**
```typescript
/**
 * user-list-widget already subscribes to data:User events
 * No changes needed - widgets receive events as before
 */
class UserListWidget {
  connectedCallback() {
    // Subscribe to User events
    this.subscribe('data:User:created', this.handleUserCreated);
    this.subscribe('data:User:updated', this.handleUserUpdated);
    this.subscribe('data:User:deleted', this.handleUserDeleted);

    // Subscribe to UserDaemon-specific events (NEW)
    this.subscribe('user:ready', this.handleUserReady);
    this.subscribe('persona:online', this.handlePersonaOnline);
  }
}
```

## ✅ **TESTING STRATEGY**

### **Test 1: UserState Auto-Creation**
```bash
# Create User entity
./jtag data/create --collection=User --data='{"type":"persona","displayName":"TestPersona"}'

# Verify UserState created automatically
./jtag data/list --collection=UserState --filter='{"userId":"<persona-id>"}'
# Expected: UserState exists with persona-specific preferences
```

### **Test 2: PersonaConnection Active**
```bash
# Create PersonaUser
./jtag data/create --collection=User --data='{"type":"persona","displayName":"CodeHelper"}'

# Add persona to chat room
./jtag data/update --collection=Room --id=<room-id> --data='{"participants":[{"userId":"<persona-id>"}]}'

# Send message to room
./jtag chat/send --roomId=<room-id> --message="Hello CodeHelper"

# Verify persona responds (check logs for model API call)
./jtag debug/logs --filterPattern="PersonaConnection.*response"
```

### **Test 3: System Restart Persistence**
```bash
# Create PersonaUser
./jtag data/create --collection=User --data='{"type":"persona","displayName":"PersistentAI"}'

# Restart system
npm run restart

# Verify PersonaUser reconnected
./jtag debug/logs --filterPattern="Reconnecting.*PersonaUser"
# Expected: "Reconnected 1 PersonaUsers"
```

## 🎯 **SUCCESS CRITERIA**

**UserDaemon is successful when:**

1. ✅ **Every User has UserState** - Create User → UserState exists automatically
2. ✅ **PersonaUsers are alive** - Can receive chat messages and respond
3. ✅ **System restarts gracefully** - Personas reconnect after restart
4. ✅ **Storage backends correct** - Memory for agents, SQLite for personas
5. ✅ **Widgets stay in sync** - user-list-widget shows all users with correct status
6. ✅ **Clean lifecycle** - Delete User → Connection closed, UserState deleted
7. ✅ **Event-driven** - All actions triggered by events, no polling
8. ✅ **Type-safe** - Strict TypeScript, no `any` types
9. ✅ **Tested** - Integration tests prove complete User → Chat → Response flow

## 🚀 **IMPLEMENTATION PRIORITY**

### **Phase 1: UserDaemon Core (IMMEDIATE)**
- Event subscriptions (data:User:created/updated/deleted)
- Automatic UserState creation
- User lifecycle management
- Storage backend assignment

### **Phase 2: PersonaConnection (NEXT)**
- PersonaConnection class
- Room subscription logic
- Chat message listener
- Message response flow (without real API)

### **Phase 3: Model API Integration (AFTER PHASE 2)**
- ModelAPIClient implementation
- OpenAI integration
- Anthropic integration
- Local model integration

### **Phase 4: Genomic Enhancement (FUTURE)**
- LoRA adapter support
- Genomic layer loading
- Real-time evolution
- Academy integration

---

**This design makes PersonaUsers first-class citizens who can participate in chat and Academy training just like humans, with their genomic evolution managed through persistent state and model API integration.**