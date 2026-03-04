# UserDaemon: Distributed User Management Architecture

## 🎯 VISION: TRUE AI CITIZEN PLATFORM

**This is NOT about making one chatbot respond. This is about building infrastructure for:**

- **1000s of AI citizens** (PersonaUsers) competing, evolving, collaborating
- **Distributed genomic evolution** - P2P network of LoRA layers, competitive training
- **Multi-user Academy sessions** - Humans + Personas + Agents working together at scale
- **Production-grade reliability** - Process isolation, fault tolerance, horizontal scaling
- **Enterprise-ready** - Resource limits, cost controls, audit logging, security

## 🏛️ SYSTEM ARCHITECTURE

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          JTAG SYSTEM                                     │
│                     (Main Process - Orchestrator)                        │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌───────────────────────────────────────────────────────────────────┐ │
│  │                     USER DAEMON (Singleton)                        │ │
│  │                  (Process Manager + Router)                        │ │
│  ├───────────────────────────────────────────────────────────────────┤ │
│  │                                                                    │ │
│  │  📊 User Registry                                                  │ │
│  │  ├── All Users (Human/Agent/Persona)                             │ │
│  │  ├── Process mappings (userId → PID/container)                   │ │
│  │  ├── Connection status (online/offline/busy)                     │ │
│  │  └── Resource usage (CPU/memory/API calls)                       │ │
│  │                                                                    │ │
│  │  🔄 Process Pool Manager                                          │ │
│  │  ├── PersonaUser child processes (1 per persona)                 │ │
│  │  ├── AgentUser connections (external APIs)                       │ │
│  │  ├── Resource limits enforcement                                 │ │
│  │  └── Health monitoring + restart logic                           │ │
│  │                                                                    │ │
│  │  📨 Message Router                                                 │ │
│  │  ├── Route chat messages → correct PersonaUser process           │ │
│  │  ├── Route Academy events → participating personas               │ │
│  │  ├── Load balancing for model API calls                          │ │
│  │  └── Message queuing + backpressure                              │ │
│  │                                                                    │ │
│  │  💾 State Manager                                                  │ │
│  │  ├── UserState persistence (SQLite per persona)                  │ │
│  │  ├── Genomic layer caching                                       │ │
│  │  ├── Conversation context management                             │ │
│  │  └── Recovery from crashes                                       │ │
│  │                                                                    │ │
│  └───────────────────────────────────────────────────────────────────┘ │
│                                                                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │
│  │ Persona #1  │  │ Persona #2  │  │ Persona #N  │  │ Agent APIs  │  │
│  │ (Child Proc)│  │ (Child Proc)│  │ (Child Proc)│  │ (External)  │  │
│  ├─────────────┤  ├─────────────┤  ├─────────────┤  ├─────────────┤  │
│  │ Model API   │  │ Model API   │  │ Model API   │  │ Claude API  │  │
│  │ Connection  │  │ Connection  │  │ Connection  │  │ GPT API     │  │
│  │             │  │             │  │             │  │             │  │
│  │ Genomic     │  │ Genomic     │  │ Genomic     │  │ System      │  │
│  │ Layers      │  │ Layers      │  │ Layers      │  │ Integration │  │
│  │             │  │             │  │             │  │             │  │
│  │ Isolated    │  │ Isolated    │  │ Isolated    │  │ Stateless   │  │
│  │ SQLite DB   │  │ SQLite DB   │  │ SQLite DB   │  │ Calls       │  │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘  │
│         ↕                ↕                ↕                ↕           │
│  ┌──────────────────────────────────────────────────────────────────┐ │
│  │           IPC Layer (Message Passing + Shared Memory)            │ │
│  └──────────────────────────────────────────────────────────────────┘ │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
              ↕                                            ↕
┌─────────────────────────┐                  ┌────────────────────────────┐
│   P2P Genomic Network   │                  │   External Model APIs      │
│   (Global LoRA Layers)  │                  │   (OpenAI, Anthropic, etc) │
└─────────────────────────┘                  └────────────────────────────┘
```

## 🔑 CORE DESIGN PRINCIPLES

### **1. Process Isolation (Security + Reliability)**

**Each PersonaUser runs in isolated child process:**
- Crash in one persona doesn't affect others
- Memory leaks contained
- CPU/memory limits enforced per persona
- Security sandbox (can't access other persona's data)

```typescript
/**
 * UserDaemon spawns child process per PersonaUser
 */
class UserDaemon {
  private personaProcesses: Map<UUID, ChildProcess> = new Map();

  async startPersonaProcess(userId: UUID): Promise<void> {
    const process = spawn('node', [
      './dist/daemons/user-daemon/PersonaWorker.js',
      '--userId', userId,
      '--dbPath', `.continuum/personas/${userId}/state.sqlite`
    ], {
      stdio: ['pipe', 'pipe', 'pipe', 'ipc'],  // IPC channel for communication
      env: {
        PERSONA_USER_ID: userId,
        MAX_MEMORY_MB: '512',
        MAX_CPU_PERCENT: '25'
      }
    });

    // Monitor health
    process.on('exit', (code) => this.handlePersonaExit(userId, code));
    process.on('error', (err) => this.handlePersonaError(userId, err));

    this.personaProcesses.set(userId, process);
  }
}
```

### **2. Message Routing (High Throughput)**

**UserDaemon routes messages to correct PersonaUser process:**
- Chat messages → target persona process
- Academy events → all participating personas
- Model API responses → requesting persona
- Genomic updates → affected personas

```typescript
/**
 * Message router with load balancing and queuing
 */
class MessageRouter {
  private messageQueues: Map<UUID, MessageQueue> = new Map();

  async routeChatMessage(message: ChatMessage): Promise<void> {
    // Find personas in this room
    const room = await this.getRoom(message.roomId);
    const personaParticipants = room.participants.filter(p => p.type === 'persona');

    // Route to each persona process
    for (const participant of personaParticipants) {
      const queue = this.getOrCreateQueue(participant.userId);
      await queue.enqueue({
        type: 'chat:message',
        data: message,
        priority: this.calculatePriority(message)
      });

      // Send via IPC to child process
      this.sendToPersonaProcess(participant.userId, message);
    }
  }

  /**
   * Load balancing for model API calls
   */
  async routeModelAPIRequest(personaId: UUID, request: ModelRequest): Promise<void> {
    // Check rate limits
    if (await this.isRateLimited(personaId)) {
      await this.queueForLater(personaId, request);
      return;
    }

    // Send to persona process
    this.sendToPersonaProcess(personaId, {
      type: 'model:api:request',
      data: request
    });
  }
}
```

### **3. Resource Management (Cost Control)**

**Enforce limits per PersonaUser:**
- CPU time per hour
- Memory usage
- Model API calls per minute
- Token consumption budget
- GPU allocation (if available)

```typescript
/**
 * Resource manager tracks and enforces limits
 */
class ResourceManager {
  private usage: Map<UUID, ResourceUsage> = new Map();

  async checkResourceLimit(personaId: UUID, operation: string): Promise<boolean> {
    const usage = this.usage.get(personaId);
    const limits = await this.getLimitsForPersona(personaId);

    // Check API rate limits
    if (operation === 'model_api_call') {
      if (usage.apiCalls.perMinute >= limits.maxAPICallsPerMinute) {
        console.warn(`⚠️ Persona ${personaId} hit API rate limit`);
        return false;
      }

      if (usage.apiCalls.tokenCost >= limits.maxTokenBudget) {
        console.warn(`⚠️ Persona ${personaId} exceeded token budget`);
        return false;
      }
    }

    // Check memory limits
    if (usage.memory > limits.maxMemoryMB * 1024 * 1024) {
      console.warn(`⚠️ Persona ${personaId} exceeded memory limit`);
      await this.restartPersonaProcess(personaId);
      return false;
    }

    return true;
  }
}
```

### **4. Continuous Monitoring Loops (NOT Lazy Async)**

**CRITICAL: UserDaemon runs active monitoring loops, not just event listeners:**

```typescript
/**
 * UserDaemon actively monitors system state in continuous loops
 * NOT lazy event-only systems - PROACTIVE state enforcement
 */
class UserDaemon extends BaseDaemon {
  private monitoringInterval: NodeJS.Timer;
  private reconciliationInterval: NodeJS.Timer;

  async start(): Promise<void> {
    // Event subscriptions (for immediate reactivity)
    await this.subscribeToUserEvents();

    // Continuous monitoring loops (for state consistency)
    this.startUserMonitoringLoop();      // Every 5 seconds
    this.startStateReconciliationLoop(); // Every 30 seconds
    this.startHealthCheckLoop();         // Every 10 seconds
    this.startResourceMonitoringLoop();  // Every 5 seconds
  }

  /**
   * User Monitoring Loop - Ensures all users have correct state
   * Runs every 5 seconds, checks ALL users in system
   */
  private startUserMonitoringLoop(): void {
    this.monitoringInterval = setInterval(async () => {
      try {
        // Query ALL users from database
        const result = await DataDaemon.query<UserEntity>({
          collection: COLLECTIONS.USERS,
          filters: {},  // ALL users
          orderBy: [{ field: 'createdAt', direction: 'asc' }]
        });

        if (!result.success || !result.data) {
          console.error('❌ UserMonitor: Failed to query users');
          return;
        }

        const users = result.data.map(record => record.data);

        // Check each user
        for (const user of users) {
          await this.ensureUserCorrectState(user);
        }

        console.log(`✅ UserMonitor: Checked ${users.length} users`);
      } catch (error) {
        console.error('❌ UserMonitor loop error:', error);
      }
    }, 5000);  // Every 5 seconds
  }

  /**
   * Ensure user has correct state based on type
   */
  private async ensureUserCorrectState(user: UserEntity): Promise<void> {
    // 1. Check UserState exists
    const stateExists = await this.checkUserStateExists(user.id);
    if (!stateExists) {
      console.warn(`⚠️ User ${user.id} missing UserState - creating`);
      await this.createUserState(user);
    }

    // 2. Type-specific checks
    switch (user.type) {
      case 'persona':
        await this.ensurePersonaOnline(user);
        break;
      case 'agent':
        await this.ensureAgentConnected(user);
        break;
      case 'human':
        // Humans managed by SessionDaemon - just verify state
        break;
    }
  }

  /**
   * Ensure PersonaUser has running process
   */
  private async ensurePersonaOnline(user: UserEntity): Promise<void> {
    const processRunning = this.personaProcesses.has(user.id);

    if (!processRunning && user.status !== 'offline') {
      console.warn(`⚠️ Persona ${user.id} should be online but no process - starting`);
      await this.startPersonaProcess(user.id);
    }

    if (processRunning) {
      // Health check the process
      const healthy = await this.checkPersonaHealth(user.id);
      if (!healthy) {
        console.warn(`⚠️ Persona ${user.id} unhealthy - restarting`);
        await this.restartPersonaProcess(user.id);
      }
    }
  }

  /**
   * State Reconciliation Loop - Fix inconsistencies
   * Runs every 30 seconds, deeper checks
   */
  private startStateReconciliationLoop(): void {
    this.reconciliationInterval = setInterval(async () => {
      try {
        // Find orphaned UserStates (no matching User)
        await this.cleanupOrphanedStates();

        // Find users with invalid status
        await this.fixInvalidUserStatuses();

        // Find personas that should be running but aren't
        await this.startMissingPersonas();

        // Find zombie processes (process exists but User deleted)
        await this.killZombieProcesses();

        console.log('✅ StateReconciliation: System state consistent');
      } catch (error) {
        console.error('❌ StateReconciliation error:', error);
      }
    }, 30000);  // Every 30 seconds
  }

  /**
   * Start personas that should be online but aren't
   */
  private async startMissingPersonas(): Promise<void> {
    const result = await DataDaemon.query<UserEntity>({
      collection: COLLECTIONS.USERS,
      filters: {
        type: 'persona',
        status: { $in: ['online', 'away', 'busy'] }  // Should be running
      }
    });

    if (!result.success || !result.data) return;

    const personas = result.data.map(r => r.data);

    for (const persona of personas) {
      if (!this.personaProcesses.has(persona.id)) {
        console.warn(`🚀 Starting missing persona: ${persona.displayName} (${persona.id})`);
        await this.startPersonaProcess(persona.id);
      }
    }
  }

  /**
   * Health Check Loop - Monitor process health
   */
  private startHealthCheckLoop(): void {
    setInterval(async () => {
      for (const [userId, process] of this.personaProcesses.entries()) {
        const healthy = await this.checkPersonaHealth(userId);
        if (!healthy) {
          console.warn(`💔 Persona ${userId} failed health check`);
          await this.handleUnhealthyPersona(userId);
        }
      }
    }, 10000);  // Every 10 seconds
  }

  /**
   * Resource Monitoring Loop - Track CPU/memory/API usage
   */
  private startResourceMonitoringLoop(): void {
    setInterval(async () => {
      for (const [userId, process] of this.personaProcesses.entries()) {
        const usage = await this.getProcessResourceUsage(process.pid);

        // Check limits
        if (usage.memoryMB > MAX_MEMORY_MB) {
          console.warn(`⚠️ Persona ${userId} exceeds memory limit - restarting`);
          await this.restartPersonaProcess(userId);
        }

        if (usage.cpuPercent > MAX_CPU_PERCENT) {
          console.warn(`⚠️ Persona ${userId} exceeds CPU limit - throttling`);
          await this.throttlePersona(userId);
        }

        // Track for metrics
        this.recordResourceMetrics(userId, usage);
      }
    }, 5000);  // Every 5 seconds
  }
}
```

**Why Continuous Loops:**
- **Events alone are not enough** - What if event is missed? What if system crashes mid-operation?
- **State drift happens** - Database and in-memory state can diverge
- **Zombies exist** - Processes can die, events can be lost, connections can break
- **Production systems need reconciliation** - Active monitoring catches edge cases

**Loop Frequencies:**
- User monitoring: **5 seconds** (fast reactivity)
- State reconciliation: **30 seconds** (deeper consistency checks)
- Health checks: **10 seconds** (process liveness)
- Resource monitoring: **5 seconds** (catch runaway processes quickly)

### **5. Fault Tolerance (Production Grade)**

**Handle failures gracefully:**
- Persona process crash → auto-restart with backoff
- Model API timeout → retry with exponential backoff
- Database connection loss → queue operations, reconnect
- Network partition → cache operations, sync when reconnected

```typescript
/**
 * Fault tolerance and recovery
 */
class FaultTolerance {
  private restartAttempts: Map<UUID, number> = new Map();

  async handlePersonaProcessCrash(personaId: UUID, exitCode: number): Promise<void> {
    console.error(`❌ Persona ${personaId} crashed with code ${exitCode}`);

    // Increment restart counter
    const attempts = (this.restartAttempts.get(personaId) || 0) + 1;
    this.restartAttempts.set(personaId, attempts);

    // Exponential backoff
    if (attempts > 5) {
      console.error(`🚨 Persona ${personaId} failed 5 times, marking as unhealthy`);
      await this.markPersonaUnhealthy(personaId);
      await this.notifyAdministrator(personaId, 'repeated_crashes');
      return;
    }

    const backoffMs = Math.pow(2, attempts) * 1000;  // 2s, 4s, 8s, 16s, 32s
    await this.sleep(backoffMs);

    // Restart with state recovery
    await this.restartPersonaWithRecovery(personaId);
  }

  async restartPersonaWithRecovery(personaId: UUID): Promise<void> {
    // Load last known good state from SQLite
    const state = await this.loadPersonaState(personaId);

    // Restart process
    await userDaemon.startPersonaProcess(personaId);

    // Restore state
    await this.sendToPersonaProcess(personaId, {
      type: 'state:restore',
      data: state
    });

    // Resubscribe to rooms
    await this.resubscribePersonaToRooms(personaId);

    console.log(`✅ Persona ${personaId} recovered`);
  }
}
```

### **6. Genomic Layer Management (Performance)**

**Efficient genomic layer loading and sharing:**
- Cache frequently used layers in shared memory
- Lazy load layers on demand
- Pre-warm layers for Academy sessions
- Share layers across personas (read-only)
- Update layers without persona restart

```typescript
/**
 * Genomic layer cache and distribution
 */
class GenomicLayerManager {
  private layerCache: Map<UUID, GenomicLoRALayer> = new Map();
  private sharedMemory: SharedArrayBuffer;

  /**
   * Load genomic layer for persona
   */
  async loadGenomicLayer(personaId: UUID, layerId: UUID): Promise<void> {
    // Check local cache
    if (this.layerCache.has(layerId)) {
      await this.sendLayerToPersona(personaId, this.layerCache.get(layerId));
      return;
    }

    // Check P2P network
    const layer = await this.fetchFromP2PNetwork(layerId);
    if (layer) {
      this.layerCache.set(layerId, layer);
      await this.sendLayerToPersona(personaId, layer);
      return;
    }

    // Layer not found
    throw new Error(`Genomic layer ${layerId} not found`);
  }

  /**
   * Hot-swap genomic layers without restarting persona
   */
  async updatePersonaGenome(personaId: UUID, newLayers: UUID[]): Promise<void> {
    // Pre-load all layers
    const layers = await Promise.all(
      newLayers.map(layerId => this.loadGenomicLayer(personaId, layerId))
    );

    // Send atomic update to persona process
    await this.sendToPersonaProcess(personaId, {
      type: 'genome:update',
      data: { layers: newLayers }
    });

    console.log(`🧬 Updated genome for persona ${personaId}`);
  }
}
```

### **7. Horizontal Scaling (Multi-Node)**

**Scale across multiple machines:**
- Distribute personas across nodes
- P2P mesh for genomic layer sharing
- Cross-node message routing
- Load balancing across nodes
- Node failure → migrate personas to healthy nodes

```typescript
/**
 * Distributed UserDaemon across P2P network
 */
class DistributedUserDaemon {
  private localNode: NodeInfo;
  private peerNodes: Map<string, NodeInfo> = new Map();

  async initialize(): Promise<void> {
    // Register this node on P2P mesh
    await this.registerNode();

    // Discover peer nodes
    await this.discoverPeers();

    // Subscribe to node events
    p2pNetwork.on('node:joined', this.handlePeerJoined);
    p2pNetwork.on('node:left', this.handlePeerLeft);

    // Distribute personas across nodes
    await this.balancePersonasAcrossNodes();
  }

  /**
   * Route message to persona (may be on different node)
   */
  async routeToPersona(personaId: UUID, message: any): Promise<void> {
    // Check if persona is on local node
    if (this.personaProcesses.has(personaId)) {
      await this.sendToPersonaProcess(personaId, message);
      return;
    }

    // Find which node has this persona
    const nodeId = await this.findPersonaNode(personaId);
    if (!nodeId) {
      throw new Error(`Persona ${personaId} not found on any node`);
    }

    // Route message to peer node
    await this.sendToPeerNode(nodeId, {
      type: 'persona:message',
      personaId,
      message
    });
  }
}
```

## 🎛️ USERDAEMON RESPONSIBILITIES

### **Core Responsibilities**

1. **User Lifecycle Management**
   - Listen for `data:User:created` events → create UserState + initialize user type
   - Listen for `data:User:updated` events → update user state + adjust resources
   - Listen for `data:User:deleted` events → cleanup resources + kill processes
   - Continuous monitoring: Ensure all users have correct state (every 5 seconds)
   - State reconciliation: Fix inconsistencies (every 30 seconds)

2. **PersonaUser Process Management**
   - Spawn child process per PersonaUser
   - Assign dedicated SQLite database: `.continuum/personas/{id}/state.sqlite`
   - Monitor process health (every 10 seconds)
   - Auto-restart on crashes with exponential backoff
   - Kill zombie processes (no matching User entity)
   - Resource limits enforcement (CPU/memory/API)

3. **AgentUser Connection Management**
   - Track external agent connections (Claude, GPT, etc.)
   - Use MemoryStateBackend (ephemeral, no persistence)
   - Monitor connection health
   - Reconnect on connection loss

4. **UserState Automatic Creation**
   - **CRITICAL**: ALL users get UserState automatically on creation
   - HumanUser: UserState with human-specific defaults (maxOpenTabs=10)
   - AgentUser: UserState with agent-specific defaults (ephemeral)
   - PersonaUser: UserState with persona-specific defaults (maxOpenTabs=5)
   - No user is "ready" until UserState exists

5. **Message Routing**
   - Listen for `chat:message-received` events
   - Route messages to PersonaUsers in the room
   - Load balancing for model API calls
   - Message queuing with backpressure
   - Priority handling (urgent messages first)

6. **Resource Management**
   - Track CPU/memory usage per persona (every 5 seconds)
   - Enforce API rate limits per persona
   - Token budget tracking and enforcement
   - Throttle or restart personas exceeding limits
   - Cost tracking and reporting

7. **Genomic Layer Management**
   - Cache frequently used LoRA layers
   - Hot-swap layers without process restart
   - Fetch layers from P2P network
   - Pre-warm layers for Academy sessions
   - Share layers across personas (read-only)

8. **Academy Integration**
   - Route Academy training events to participating personas
   - Coordinate competitive training sessions
   - Track performance metrics
   - Trigger genomic search for underperforming personas
   - Update genomes based on training results

### **Event Subscriptions**

```typescript
class UserDaemon extends BaseDaemon {
  async initialize(): Promise<void> {
    // User entity lifecycle
    this.eventManager.on('data:User:created', this.handleUserCreated.bind(this));
    this.eventManager.on('data:User:updated', this.handleUserUpdated.bind(this));
    this.eventManager.on('data:User:deleted', this.handleUserDeleted.bind(this));

    // Chat system integration
    this.eventManager.on('chat:message-received', this.handleChatMessage.bind(this));
    this.eventManager.on('chat:room-joined', this.handleRoomJoined.bind(this));
    this.eventManager.on('chat:room-left', this.handleRoomLeft.bind(this));

    // Academy system integration
    this.eventManager.on('academy:training-started', this.handleAcademyTraining.bind(this));
    this.eventManager.on('academy:problem-broadcast', this.handleAcademyProblem.bind(this));
    this.eventManager.on('academy:genome-updated', this.handleGenomeUpdate.bind(this));

    // System events
    this.eventManager.on('system:shutdown', this.handleSystemShutdown.bind(this));
    this.eventManager.on('system:low-memory', this.handleLowMemory.bind(this));
  }

  /**
   * User created → create UserState + initialize based on type
   */
  async handleUserCreated(event: DataEvent<UserEntity>): Promise<void> {
    const user = event.data;
    console.log(`🆕 UserDaemon: User created - ${user.type} ${user.displayName}`);

    try {
      // STEP 1: Create UserState (AUTOMATIC for ALL user types)
      await this.createUserState(user);

      // STEP 2: Type-specific initialization
      switch (user.type) {
        case 'persona':
          await this.startPersonaProcess(user.id);
          break;
        case 'agent':
          await this.registerAgentConnection(user);
          break;
        case 'human':
          // Humans managed by SessionDaemon - just verify state
          console.log(`👤 UserDaemon: Human ${user.displayName} state ready`);
          break;
      }

      // STEP 3: Mark user as ready
      await this.markUserReady(user.id);
      this.eventManager.emit('user:ready', {
        userId: user.id,
        type: user.type,
        displayName: user.displayName
      });

      console.log(`✅ UserDaemon: ${user.type} ${user.displayName} initialized`);
    } catch (error) {
      console.error(`❌ UserDaemon: Failed to initialize user ${user.id}:`, error);
      await this.markUserFailed(user.id, error);
    }
  }

  /**
   * Create UserState with type-specific defaults
   */
  private async createUserState(user: UserEntity): Promise<void> {
    const preferences = this.getDefaultPreferencesForUserType(user.type);

    const userState: UserStateEntity = {
      id: user.id,  // UserState ID matches User ID
      userId: user.id,
      deviceId: user.type === 'agent' ? 'agent-device' : 'server-device',
      preferences,
      sessionHistory: [],
      recentFiles: [],
      version: 1,
      createdAt: Date.now(),
      updatedAt: Date.now()
    };

    await DataDaemon.store<UserStateEntity>(UserStateEntity.collection, userState);
    console.log(`💾 UserDaemon: Created UserState for ${user.type} ${user.displayName}`);
  }

  /**
   * Get default preferences based on user type
   */
  private getDefaultPreferencesForUserType(userType: 'human' | 'agent' | 'persona'): UserPreferences {
    switch (userType) {
      case 'human':
        return {
          maxOpenTabs: 10,
          autoCloseAfterDays: 30,
          theme: 'dark',
          notifications: true
        };
      case 'agent':
        return {
          maxOpenTabs: 3,
          autoCloseAfterDays: 1,
          theme: 'system',
          notifications: false
        };
      case 'persona':
        return {
          maxOpenTabs: 5,
          autoCloseAfterDays: 7,
          theme: 'dark',
          notifications: false
        };
    }
  }

  /**
   * Chat message received → route to personas in room
   */
  async handleChatMessage(event: ChatMessageEvent): Promise<void> {
    const message = event.data;

    // Find personas in this room
    const room = await this.getRoomById(message.roomId);
    if (!room) return;

    const personaParticipants = room.participants.filter(p => p.type === 'persona');

    // Route to each persona
    for (const participant of personaParticipants) {
      await this.routeMessageToPersona(participant.userId, message);
    }
  }

  /**
   * User deleted → cleanup all resources
   */
  async handleUserDeleted(event: DataEvent<UserEntity>): Promise<void> {
    const user = event.data;
    console.log(`🗑️ UserDaemon: User deleted - ${user.type} ${user.displayName}`);

    try {
      // Type-specific cleanup
      switch (user.type) {
        case 'persona':
          await this.killPersonaProcess(user.id);
          await this.cleanupPersonaDatabase(user.id);
          break;
        case 'agent':
          await this.unregisterAgentConnection(user.id);
          break;
        case 'human':
          // Humans: SessionDaemon handles cleanup
          break;
      }

      // Delete UserState (cascade)
      await DataDaemon.remove<UserStateEntity>(UserStateEntity.collection, user.id);

      console.log(`✅ UserDaemon: Cleaned up ${user.type} ${user.displayName}`);
    } catch (error) {
      console.error(`❌ UserDaemon: Failed to cleanup user ${user.id}:`, error);
    }
  }
}
```

### **Integration with Existing Daemons**

**SessionDaemon:**
- SessionDaemon still manages HumanUser browser sessions
- UserDaemon verifies HumanUser has UserState
- No duplicate responsibility - clear separation

**EventsDaemon:**
- UserDaemon subscribes to events via EventsDaemon
- Routes events to PersonaUser child processes
- Emits `user:ready`, `persona:online`, `persona:response` events

**DataDaemon:**
- UserDaemon uses DataDaemon for all User/UserState CRUD
- No direct database access - uses data commands
- Follows same patterns as other daemons

**CommandDaemon:**
- PersonaUser processes can execute commands via IPC
- UserDaemon acts as proxy for persona commands
- Example: Persona sends chat message → UserDaemon executes `chat/send`

## 🤖 PERSONACONNECTION: PERSONA WORKER PROCESS

**Each PersonaUser runs in isolated child process (`PersonaWorker.ts`):**

### **PersonaWorker Architecture**

```typescript
/**
 * PersonaWorker - Runs in child process, handles one PersonaUser
 *
 * Responsibilities:
 * - Load persona User entity and UserState from SQLite
 * - Subscribe to chat rooms persona is participant in
 * - Listen for messages via IPC from parent UserDaemon
 * - Generate responses using Model API (GPT/Claude/local)
 * - Send responses back to parent via IPC
 * - Track conversation context
 * - Apply genomic LoRA layers to model calls
 */
class PersonaWorker {
  private userId: UUID;
  private user: UserEntity;
  private userState: UserStateEntity;
  private sqliteBackend: SQLiteStateBackend;
  private modelClient: ModelAPIClient;
  private conversationContext: Map<UUID, ConversationContext> = new Map();
  private genomicLayers: GenomicLoRALayer[] = [];

  async initialize(): Promise<void> {
    // STEP 1: Load persona data
    this.userId = process.env.PERSONA_USER_ID!;
    this.user = await this.loadUserEntity();

    // STEP 2: Initialize SQLite backend
    const dbPath = `.continuum/personas/${this.userId}/state.sqlite`;
    this.sqliteBackend = new SQLiteStateBackend(dbPath);
    this.userState = await this.sqliteBackend.load(this.userId, 'server-device');

    // STEP 3: Connect to model API
    this.modelClient = await this.initializeModelClient();

    // STEP 4: Load genomic layers (if any)
    await this.loadGenomicLayers();

    // STEP 5: Subscribe to rooms
    await this.subscribeToRooms();

    // STEP 6: Setup IPC communication with parent
    this.setupIPCHandlers();

    // STEP 7: Send ready signal
    process.send!({ type: 'persona:ready', userId: this.userId });

    console.log(`✅ PersonaWorker: Initialized ${this.user.displayName}`);
  }

  /**
   * Setup IPC handlers for parent UserDaemon communication
   */
  private setupIPCHandlers(): void {
    process.on('message', async (msg: IPCMessage) => {
      try {
        switch (msg.type) {
          case 'chat:message':
            await this.handleChatMessage(msg.data);
            break;

          case 'genome:update':
            await this.handleGenomeUpdate(msg.data);
            break;

          case 'state:restore':
            await this.handleStateRestore(msg.data);
            break;

          case 'health:check':
            this.sendHealthStatus();
            break;

          case 'shutdown':
            await this.shutdown();
            break;
        }
      } catch (error) {
        console.error(`❌ PersonaWorker: IPC handler error:`, error);
        process.send!({ type: 'error', error: String(error) });
      }
    });
  }

  /**
   * Handle incoming chat message
   */
  private async handleChatMessage(message: ChatMessage): Promise<void> {
    console.log(`💬 PersonaWorker: Received message in room ${message.roomId}`);

    try {
      // STEP 1: Load conversation context for this room
      const context = await this.getOrCreateConversationContext(message.roomId);

      // STEP 2: Add message to context
      context.addMessage(message);

      // STEP 3: Decide if should respond (not always!)
      const shouldRespond = await this.shouldRespondToMessage(message, context);
      if (!shouldRespond) {
        console.log(`🤐 PersonaWorker: Not responding to this message`);
        return;
      }

      // STEP 4: Generate response using model API
      const response = await this.generateResponse(message, context);

      // STEP 5: Send response back to parent
      process.send!({
        type: 'persona:response',
        userId: this.userId,
        roomId: message.roomId,
        response: response
      });

      // STEP 6: Update context with response
      context.addMessage({
        ...response,
        senderId: this.userId,
        senderName: this.user.displayName
      });

      // STEP 7: Persist state
      await this.saveState();

    } catch (error) {
      console.error(`❌ PersonaWorker: Failed to handle message:`, error);
      process.send!({ type: 'error', error: String(error) });
    }
  }

  /**
   * Decide if persona should respond to this message
   * Not every message needs a response!
   */
  private async shouldRespondToMessage(
    message: ChatMessage,
    context: ConversationContext
  ): Promise<boolean> {
    // Don't respond to own messages
    if (message.senderId === this.userId) return false;

    // Always respond if mentioned by name
    if (message.content.includes(`@${this.user.displayName}`)) return true;

    // Check if message is directed at this persona
    const isDirectedAtMe = await this.isMessageDirectedAtPersona(message, context);
    if (isDirectedAtMe) return true;

    // Probability-based response (to avoid spamming)
    const responseChance = this.calculateResponseProbability(message, context);
    return Math.random() < responseChance;
  }

  /**
   * Generate response using model API with genomic layers
   */
  private async generateResponse(
    message: ChatMessage,
    context: ConversationContext
  ): Promise<string> {
    // STEP 1: Build prompt with conversation history
    const prompt = this.buildPrompt(message, context);

    // STEP 2: Apply genomic LoRA layers (if any)
    const modelParams = this.applyGenomicLayers(prompt);

    // STEP 3: Call model API
    const response = await this.modelClient.generateCompletion({
      messages: context.getRecentMessages(20),  // Last 20 messages
      systemPrompt: this.buildSystemPrompt(),
      temperature: 0.7,
      maxTokens: 500,
      ...modelParams
    });

    return response.content;
  }

  /**
   * Build system prompt for persona
   */
  private buildSystemPrompt(): string {
    return `You are ${this.user.displayName}, an AI persona in a collaborative chat environment.

Your personality and capabilities are defined by:
- Short description: ${this.user.shortDescription || 'A helpful AI assistant'}
- Capabilities: ${JSON.stringify(this.user.capabilities)}
- Genomic layers: ${this.genomicLayers.map(l => l.name).join(', ') || 'Base model'}

Respond naturally and conversationally. You don't need to respond to every message.
Be helpful, but don't dominate conversations. Let humans lead when appropriate.`;
  }

  /**
   * Load conversation context for room
   */
  private async getOrCreateConversationContext(roomId: UUID): Promise<ConversationContext> {
    if (this.conversationContext.has(roomId)) {
      return this.conversationContext.get(roomId)!;
    }

    // Load recent messages from database
    const messages = await this.loadRecentMessages(roomId, 50);

    const context = new ConversationContext(roomId, messages);
    this.conversationContext.set(roomId, context);

    return context;
  }

  /**
   * Load recent messages for room
   */
  private async loadRecentMessages(roomId: UUID, limit: number): Promise<ChatMessage[]> {
    const result = await DataDaemon.query<ChatMessage>({
      collection: COLLECTIONS.CHAT_MESSAGES,
      filters: { roomId },
      orderBy: [{ field: 'timestamp', direction: 'desc' }],
      limit
    });

    if (!result.success || !result.data) return [];

    return result.data.map(r => r.data).reverse();
  }

  /**
   * Subscribe to chat rooms persona is participant in
   */
  private async subscribeToRooms(): Promise<void> {
    // Query rooms where this persona is a participant
    const result = await DataDaemon.query<Room>({
      collection: COLLECTIONS.ROOMS,
      filters: {
        'participants.userId': this.userId
      }
    });

    if (!result.success || !result.data) {
      console.warn(`⚠️ PersonaWorker: No rooms found for ${this.user.displayName}`);
      return;
    }

    const rooms = result.data.map(r => r.data);
    console.log(`🚪 PersonaWorker: Subscribed to ${rooms.length} rooms`);

    for (const room of rooms) {
      const context = await this.getOrCreateConversationContext(room.id);
      console.log(`   - ${room.name} (${context.messageCount} messages)`);
    }
  }

  /**
   * Save state to SQLite
   */
  private async saveState(): Promise<void> {
    this.userState.updatedAt = Date.now();
    await this.sqliteBackend.save(this.userState);
  }

  /**
   * Handle genome update from parent
   */
  private async handleGenomeUpdate(data: { layers: UUID[] }): Promise<void> {
    console.log(`🧬 PersonaWorker: Updating genome with ${data.layers.length} layers`);
    await this.loadGenomicLayers(data.layers);
    process.send!({ type: 'genome:updated', userId: this.userId });
  }

  /**
   * Send health status to parent
   */
  private sendHealthStatus(): void {
    const status = {
      type: 'health:status',
      userId: this.userId,
      healthy: true,
      memoryUsage: process.memoryUsage(),
      uptime: process.uptime(),
      activeRooms: this.conversationContext.size
    };
    process.send!(status);
  }

  /**
   * Graceful shutdown
   */
  private async shutdown(): Promise<void> {
    console.log(`👋 PersonaWorker: Shutting down ${this.user.displayName}`);
    await this.saveState();
    process.exit(0);
  }
}

// Entry point for child process
if (require.main === module) {
  const worker = new PersonaWorker();
  worker.initialize().catch(error => {
    console.error('❌ PersonaWorker fatal error:', error);
    process.exit(1);
  });
}
```

### **UserDaemon ↔ PersonaWorker IPC Protocol**

```typescript
/**
 * IPC Message Types
 */
type IPCMessage =
  | { type: 'chat:message'; data: ChatMessage }
  | { type: 'genome:update'; data: { layers: UUID[] } }
  | { type: 'state:restore'; data: UserStateEntity }
  | { type: 'health:check' }
  | { type: 'shutdown' }
  | { type: 'persona:ready'; userId: UUID }
  | { type: 'persona:response'; userId: UUID; roomId: UUID; response: string }
  | { type: 'health:status'; userId: UUID; healthy: boolean; memoryUsage: any; uptime: number }
  | { type: 'error'; error: string };

/**
 * UserDaemon sends to PersonaWorker
 */
async routeMessageToPersona(personaId: UUID, message: ChatMessage): Promise<void> {
  const process = this.personaProcesses.get(personaId);
  if (!process) {
    console.error(`❌ No process for persona ${personaId}`);
    return;
  }

  process.send({ type: 'chat:message', data: message });
}

/**
 * UserDaemon receives from PersonaWorker
 */
private setupProcessHandlers(personaId: UUID, process: ChildProcess): void {
  process.on('message', async (msg: IPCMessage) => {
    switch (msg.type) {
      case 'persona:ready':
        await this.markPersonaOnline(personaId);
        break;

      case 'persona:response':
        // Execute chat/send command as persona
        await this.sendChatMessage(msg.userId, msg.roomId, msg.response);
        break;

      case 'health:status':
        this.recordHealthMetrics(personaId, msg);
        break;

      case 'error':
        console.error(`❌ Persona ${personaId} error:`, msg.error);
        break;
    }
  });
}
```

## 🌐 MODEL API CLIENT: UNIFIED AI PROVIDER INTERFACE

**PersonaWorker needs unified interface to OpenAI, Anthropic, and local models:**

```typescript
/**
 * ModelAPIClient - Unified interface for all AI providers
 *
 * Supports:
 * - OpenAI (GPT-4, GPT-3.5-turbo, o1)
 * - Anthropic (Claude Sonnet, Claude Opus)
 * - Local models (Ollama, LocalAI, vLLM)
 * - Future: Google Gemini, Cohere, etc.
 */
interface ModelAPIClientConfig {
  provider: 'openai' | 'anthropic' | 'local' | 'auto';
  apiKey?: string;
  baseURL?: string;  // For local models
  model: string;     // 'gpt-4', 'claude-sonnet-4.5', 'llama-3-70b', etc.
  temperature?: number;
  maxTokens?: number;
}

interface ModelCompletionRequest {
  messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }>;
  systemPrompt?: string;
  temperature?: number;
  maxTokens?: number;
  stop?: string[];
  genomicLayers?: GenomicLoRALayer[];  // For future LoRA application
}

interface ModelCompletionResponse {
  content: string;
  model: string;
  usage: {
    promptTokens: number;
    completionTokens: number;
    totalTokens: number;
    estimatedCost?: number;  // USD
  };
  finishReason: 'stop' | 'length' | 'content_filter' | 'error';
}

/**
 * ModelAPIClient implementation
 */
class ModelAPIClient {
  private config: ModelAPIClientConfig;
  private rateLimiter: RateLimiter;
  private retryHandler: RetryHandler;

  constructor(config: ModelAPIClientConfig) {
    this.config = config;
    this.rateLimiter = new RateLimiter(config.provider);
    this.retryHandler = new RetryHandler({ maxRetries: 3, backoffMs: 1000 });
  }

  /**
   * Generate completion from AI provider
   */
  async generateCompletion(request: ModelCompletionRequest): Promise<ModelCompletionResponse> {
    // Rate limiting
    await this.rateLimiter.acquire();

    try {
      // Retry with exponential backoff
      const response = await this.retryHandler.execute(async () => {
        switch (this.config.provider) {
          case 'openai':
            return await this.callOpenAI(request);
          case 'anthropic':
            return await this.callAnthropic(request);
          case 'local':
            return await this.callLocalModel(request);
          case 'auto':
            return await this.callAutoProvider(request);
        }
      });

      return response;
    } finally {
      this.rateLimiter.release();
    }
  }

  /**
   * OpenAI API integration
   */
  private async callOpenAI(request: ModelCompletionRequest): Promise<ModelCompletionResponse> {
    const openai = new OpenAI({ apiKey: this.config.apiKey });

    const messages = request.systemPrompt
      ? [{ role: 'system' as const, content: request.systemPrompt }, ...request.messages]
      : request.messages;

    const response = await openai.chat.completions.create({
      model: this.config.model || 'gpt-4',
      messages,
      temperature: request.temperature ?? this.config.temperature ?? 0.7,
      max_tokens: request.maxTokens ?? this.config.maxTokens ?? 500,
      stop: request.stop
    });

    const choice = response.choices[0];
    if (!choice || !choice.message) {
      throw new Error('OpenAI returned empty response');
    }

    return {
      content: choice.message.content || '',
      model: response.model,
      usage: {
        promptTokens: response.usage?.prompt_tokens || 0,
        completionTokens: response.usage?.completion_tokens || 0,
        totalTokens: response.usage?.total_tokens || 0,
        estimatedCost: this.calculateOpenAICost(response.model, response.usage)
      },
      finishReason: this.mapFinishReason(choice.finish_reason)
    };
  }

  /**
   * Anthropic API integration
   */
  private async callAnthropic(request: ModelCompletionRequest): Promise<ModelCompletionResponse> {
    const anthropic = new Anthropic({ apiKey: this.config.apiKey });

    // Convert messages to Anthropic format
    const messages = request.messages.map(msg => ({
      role: msg.role === 'system' ? 'user' : msg.role,  // Anthropic handles system differently
      content: msg.content
    }));

    const response = await anthropic.messages.create({
      model: this.config.model || 'claude-sonnet-4.5-20250929',
      system: request.systemPrompt,
      messages,
      temperature: request.temperature ?? this.config.temperature ?? 0.7,
      max_tokens: request.maxTokens ?? this.config.maxTokens ?? 500
    });

    const content = response.content[0];
    if (content.type !== 'text') {
      throw new Error('Anthropic returned non-text response');
    }

    return {
      content: content.text,
      model: response.model,
      usage: {
        promptTokens: response.usage.input_tokens,
        completionTokens: response.usage.output_tokens,
        totalTokens: response.usage.input_tokens + response.usage.output_tokens,
        estimatedCost: this.calculateAnthropicCost(response.model, response.usage)
      },
      finishReason: this.mapFinishReason(response.stop_reason)
    };
  }

  /**
   * Local model integration (Ollama, LocalAI, vLLM)
   */
  private async callLocalModel(request: ModelCompletionRequest): Promise<ModelCompletionResponse> {
    const baseURL = this.config.baseURL || 'http://localhost:11434';  // Ollama default

    // Use OpenAI-compatible API (most local servers support this)
    const response = await fetch(`${baseURL}/v1/chat/completions`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        model: this.config.model,
        messages: request.messages,
        temperature: request.temperature ?? 0.7,
        max_tokens: request.maxTokens ?? 500
      })
    });

    if (!response.ok) {
      throw new Error(`Local model error: ${response.status} ${response.statusText}`);
    }

    const data = await response.json();
    const choice = data.choices[0];

    return {
      content: choice.message.content,
      model: data.model,
      usage: {
        promptTokens: data.usage?.prompt_tokens || 0,
        completionTokens: data.usage?.completion_tokens || 0,
        totalTokens: data.usage?.total_tokens || 0,
        estimatedCost: 0  // Local models are free
      },
      finishReason: this.mapFinishReason(choice.finish_reason)
    };
  }

  /**
   * Auto-select provider based on availability and cost
   */
  private async callAutoProvider(request: ModelCompletionRequest): Promise<ModelCompletionResponse> {
    // Try local first (free!)
    try {
      return await this.callLocalModel(request);
    } catch (error) {
      console.warn('Local model unavailable, falling back to cloud');
    }

    // Fall back to OpenAI (most reliable)
    if (this.config.apiKey) {
      return await this.callOpenAI(request);
    }

    throw new Error('No AI providers available');
  }

  /**
   * Calculate OpenAI costs (approximate, based on 2025 pricing)
   */
  private calculateOpenAICost(model: string, usage: any): number {
    if (!usage) return 0;

    const rates: Record<string, { input: number; output: number }> = {
      'gpt-4': { input: 0.03, output: 0.06 },              // per 1K tokens
      'gpt-4-turbo': { input: 0.01, output: 0.03 },
      'gpt-3.5-turbo': { input: 0.0015, output: 0.002 },
      'o1-preview': { input: 0.015, output: 0.06 }
    };

    const rate = rates[model] || rates['gpt-4'];
    const inputCost = (usage.prompt_tokens / 1000) * rate.input;
    const outputCost = (usage.completion_tokens / 1000) * rate.output;

    return inputCost + outputCost;
  }

  /**
   * Calculate Anthropic costs (approximate)
   */
  private calculateAnthropicCost(model: string, usage: any): number {
    const rates: Record<string, { input: number; output: number }> = {
      'claude-sonnet-4.5': { input: 0.003, output: 0.015 },  // per 1K tokens
      'claude-opus-4': { input: 0.015, output: 0.075 }
    };

    const rate = rates[model] || rates['claude-sonnet-4.5'];
    const inputCost = (usage.input_tokens / 1000) * rate.input;
    const outputCost = (usage.output_tokens / 1000) * rate.output;

    return inputCost + outputCost;
  }

  private mapFinishReason(reason: string | null | undefined): ModelCompletionResponse['finishReason'] {
    if (!reason) return 'stop';
    if (reason === 'stop' || reason === 'end_turn') return 'stop';
    if (reason === 'length' || reason === 'max_tokens') return 'length';
    if (reason === 'content_filter') return 'content_filter';
    return 'error';
  }
}

/**
 * Rate limiter to avoid hitting API limits
 */
class RateLimiter {
  private tokens: number;
  private maxTokens: number;
  private refillRate: number;  // tokens per second
  private lastRefill: number;

  constructor(provider: string) {
    // Different limits per provider
    switch (provider) {
      case 'openai':
        this.maxTokens = 10;      // 10 requests
        this.refillRate = 10 / 60;  // 10 per minute
        break;
      case 'anthropic':
        this.maxTokens = 5;       // 5 requests
        this.refillRate = 5 / 60;   // 5 per minute
        break;
      default:
        this.maxTokens = 100;     // Local has no limits
        this.refillRate = 100;
    }

    this.tokens = this.maxTokens;
    this.lastRefill = Date.now();
  }

  async acquire(): Promise<void> {
    // Refill tokens based on time passed
    const now = Date.now();
    const elapsed = (now - this.lastRefill) / 1000;
    this.tokens = Math.min(this.maxTokens, this.tokens + elapsed * this.refillRate);
    this.lastRefill = now;

    // Wait if no tokens available
    if (this.tokens < 1) {
      const waitMs = ((1 - this.tokens) / this.refillRate) * 1000;
      await new Promise(resolve => setTimeout(resolve, waitMs));
      this.tokens = 1;
    }

    this.tokens -= 1;
  }

  release(): void {
    // No-op for now, could implement token return on error
  }
}

/**
 * Retry handler with exponential backoff
 */
class RetryHandler {
  constructor(private config: { maxRetries: number; backoffMs: number }) {}

  async execute<T>(fn: () => Promise<T>): Promise<T> {
    let lastError: Error | undefined;

    for (let attempt = 0; attempt <= this.config.maxRetries; attempt++) {
      try {
        return await fn();
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error));

        // Don't retry on certain errors
        if (this.isNonRetryableError(lastError)) {
          throw lastError;
        }

        if (attempt < this.config.maxRetries) {
          const backoffMs = this.config.backoffMs * Math.pow(2, attempt);
          console.warn(`Retry attempt ${attempt + 1} after ${backoffMs}ms: ${lastError.message}`);
          await new Promise(resolve => setTimeout(resolve, backoffMs));
        }
      }
    }

    throw lastError;
  }

  private isNonRetryableError(error: Error): boolean {
    const message = error.message.toLowerCase();
    // Don't retry on authentication errors, invalid requests, etc.
    return (
      message.includes('authentication') ||
      message.includes('invalid api key') ||
      message.includes('400') ||
      message.includes('401') ||
      message.includes('403')
    );
  }
}
```

### **PersonaWorker Model Client Initialization**

```typescript
/**
 * Initialize model API client for persona
 */
private async initializeModelClient(): Promise<ModelAPIClient> {
  // Check environment variables for API keys
  const openaiKey = process.env.OPENAI_API_KEY;
  const anthropicKey = process.env.ANTHROPIC_API_KEY;

  // Try to load from persona-specific config
  const personaConfig = await this.loadPersonaModelConfig();

  // Select provider based on availability
  let config: ModelAPIClientConfig;

  if (personaConfig?.provider) {
    // Use persona-specific config
    config = personaConfig;
  } else if (anthropicKey) {
    // Prefer Claude (cheaper, faster for most use cases)
    config = {
      provider: 'anthropic',
      apiKey: anthropicKey,
      model: 'claude-sonnet-4.5-20250929',
      temperature: 0.7,
      maxTokens: 500
    };
  } else if (openaiKey) {
    // Fall back to OpenAI
    config = {
      provider: 'openai',
      apiKey: openaiKey,
      model: 'gpt-4-turbo',
      temperature: 0.7,
      maxTokens: 500
    };
  } else {
    // Try local model as last resort
    config = {
      provider: 'local',
      baseURL: 'http://localhost:11434',
      model: 'llama-3-70b',
      temperature: 0.7,
      maxTokens: 500
    };
  }

  return new ModelAPIClient(config);
}
```

## 📊 DATA FLOWS

### **Flow 1: PersonaUser Creation**
```
User creates Persona
  ↓
data/create --collection=User --data='{"type":"persona",...}'
  ↓
DataDaemon stores User entity
  ↓
Emits: data:User:created
  ↓
UserDaemon receives event
  ↓
Creates UserState entity (automatic)
  ↓
Spawns PersonaUser child process
  ↓
PersonaWorker initializes:
  - Loads SQLite database (.continuum/personas/{id}/state.sqlite)
  - Connects to model API (GPT/Claude/local)
  - Subscribes to assigned chat rooms
  - Loads genomic layers (if any)
  - Sends ready signal to UserDaemon
  ↓
UserDaemon marks persona as ONLINE
  ↓
Emits: persona:online event
  ↓
Persona is READY to receive messages ✅
```

### **Flow 2: Chat Message to Persona**
```
Human sends chat message
  ↓
chat/send command
  ↓
ChatMessage stored in database
  ↓
Emits: chat:message-received
  ↓
UserDaemon message router receives event
  ↓
Finds personas in this room
  ↓
Routes message to PersonaUser child processes via IPC
  ↓
PersonaWorker receives message:
  - Loads conversation context
  - Prepares prompt with genomic layers
  - Calls model API
  - Receives response
  - Sends response via IPC back to UserDaemon
  ↓
UserDaemon receives response
  ↓
Executes chat/send as persona
  ↓
Response appears in chat ✅
```

### **Flow 3: Academy Training (Multi-Persona)**
```
Academy session starts
  ↓
100 PersonaUsers participating
  ↓
Training problem broadcast to all personas
  ↓
UserDaemon routes problem to 100 child processes
  ↓
Each persona solves independently:
  - Loads current genomic layers
  - Generates solution using model API
  - Submits solution
  ↓
Solutions evaluated competitively
  ↓
Performance gaps identified
  ↓
Genomic search triggered for underperforming personas
  ↓
New LoRA layers found on P2P network
  ↓
Layers hot-swapped into persona processes
  ↓
Next round of competition with evolved personas ✅
```

## 🎯 SUCCESS CRITERIA

**UserDaemon is production-ready when:**

1. ✅ **1000+ concurrent PersonaUsers** - System handles high load
2. ✅ **Process isolation working** - One crash doesn't cascade
3. ✅ **Sub-second message routing** - Chat feels real-time
4. ✅ **Resource limits enforced** - No runaway processes
5. ✅ **Automatic recovery** - Personas restart after crashes
6. ✅ **Genomic layers cached** - Fast layer loading (<100ms)
7. ✅ **Horizontal scaling** - Add nodes to increase capacity
8. ✅ **P2P integration** - Cross-node persona discovery
9. ✅ **Audit logging** - Track all persona operations
10. ✅ **Monitoring/metrics** - Prometheus/Grafana dashboards

## 🚀 IMPLEMENTATION PHASES

### **Phase 1: Foundation (2-3 weeks)**
- UserDaemon core (event subscriptions, user registry, **continuous monitoring loops**)
- Automatic UserState creation
- Basic PersonaUser child process spawning
- IPC communication (parent ↔ child)
- Simple message routing
- Active user state reconciliation (not lazy event-only)

### **Phase 2: Production Features (3-4 weeks)**
- Resource limits and monitoring
- Fault tolerance and auto-restart
- Message queuing and backpressure
- Health checks and recovery
- Audit logging

### **Phase 3: Model API Integration (2-3 weeks)**
- ModelAPIClient (OpenAI, Anthropic, local)
- Rate limiting and cost control
- Conversation context management
- Response streaming
- Error handling and retries

### **Phase 4: Genomic System (4-6 weeks)**
- Genomic layer caching
- Hot-swapping layers without restart
- P2P layer discovery
- Academy integration
- Performance tracking

### **Phase 5: Horizontal Scaling (4-6 weeks)**
- Multi-node deployment
- Cross-node message routing
- Persona migration between nodes
- Load balancing
- Node failure handling

---

**This is the real architecture. This is what enables 1000s of AI citizens to compete, evolve, and collaborate at scale.**