# PersonaUser Operating System Architecture

## Complete OS Analogy - Continuum Persona Runtime

```
┌─────────────────────────────────────────────────────────────────┐
│                    CONTINUUM PERSONA OS                          │
├─────────────────────────────────────────────────────────────────┤
│  UserDaemon (Kernel)                                            │
│  ├─ Process Scheduler (PersonaScheduler)                        │
│  ├─ Memory Manager (ContextManager)                             │
│  ├─ Interrupt Controller (EventRouter)                          │
│  └─ IPC Manager (Events.emit/subscribe)                         │
├─────────────────────────────────────────────────────────────────┤
│  User Space Processes (PersonaUser instances)                   │
│  ├─ CodeAI (PID: persona-001)                                   │
│  │   ├─ Event Queue (priority queue)                            │
│  │   ├─ Context Cache (L1/L2)                                   │
│  │   ├─ Execution Pipeline (fetch/decode/execute)               │
│  │   └─ Private Memory (.continuum/personas/persona-001/)       │
│  │                                                               │
│  ├─ PlannerAI (PID: persona-002)                                │
│  └─ GeneralAI (PID: persona-003)                                │
├─────────────────────────────────────────────────────────────────┤
│  File System Layer                                              │
│  ├─ Per-Persona SQLite (process-local storage)                  │
│  ├─ Shared DataDaemon (global data layer)                       │
│  └─ Artifacts (RAG context files)                               │
├─────────────────────────────────────────────────────────────────┤
│  Hardware Abstraction Layer                                     │
│  ├─ LLM API (Claude/GPT - like GPU calls)                       │
│  ├─ Database (SQLite - like disk I/O)                           │
│  └─ WebSocket (browser clients - like network)                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 1. Kernel Layer (UserDaemon)

### Process Control Block (PCB) - Per Persona

```typescript
interface PersonaProcessControlBlock {
  // Process Identification
  pid: UUID;                           // Persona ID
  displayName: string;                 // Process name
  type: 'persona';                     // Process type

  // Process State
  state: 'ready' | 'running' | 'waiting' | 'suspended';
  currentRoom: UUID | null;            // Current execution context

  // CPU Scheduling Information
  priority: number;                    // Base scheduling priority
  cpuBurst: number;                    // Time spent processing events
  waitTime: number;                    // Time waiting in queue
  lastScheduled: Date;                 // Last time process ran

  // Memory Management
  baseAddress: string;                 // .continuum/personas/{pid}/
  contextCache: Map<UUID, ConversationContext>;  // Loaded contexts
  memoryLimit: number;                 // Max contexts in memory

  // I/O Status
  pendingIO: {
    llmCalls: number;                  // Waiting for LLM response
    databaseOps: number;               // Waiting for SQLite
  };

  // Accounting Information
  cpuTime: number;                     // Total CPU time used
  responseCount: number;               // Total responses posted
  startTime: Date;                     // Process start time

  // Inter-Process Communication
  eventQueue: PersonaEventQueue;       // Incoming events
  subscriptions: string[];             // Event subscriptions
}
```

### Process Scheduler

```typescript
class PersonaScheduler {
  private processes: Map<UUID, PersonaProcessControlBlock> = new Map();
  private readyQueue: PersonaProcessControlBlock[] = [];
  private runningProcess: UUID | null = null;
  private quantumMs: number = 100;     // Time slice per persona

  /**
   * Schedule next process (Round-robin with priority)
   */
  async schedule(): Promise<void> {
    // Get next ready process
    const nextProcess = this.selectNextProcess();
    if (!nextProcess) {
      await this.idle();
      return;
    }

    // Context switch if needed
    if (this.runningProcess !== nextProcess.pid) {
      await this.contextSwitch(this.runningProcess, nextProcess.pid);
    }

    // Run process for quantum
    await this.runProcess(nextProcess, this.quantumMs);
  }

  /**
   * Select next process to run (scheduling algorithm)
   */
  private selectNextProcess(): PersonaProcessControlBlock | null {
    // Sort ready queue by priority and wait time
    this.readyQueue.sort((a, b) => {
      // Higher priority first
      if (a.priority !== b.priority) {
        return a.priority - b.priority;
      }
      // Longer wait time first (prevent starvation)
      return b.waitTime - a.waitTime;
    });

    return this.readyQueue[0] || null;
  }

  /**
   * Context switch between processes
   */
  private async contextSwitch(fromPid: UUID | null, toPid: UUID): Promise<void> {
    const startTime = Date.now();

    // STEP 1: Save outgoing process state
    if (fromPid) {
      const fromProcess = this.processes.get(fromPid)!;
      fromProcess.state = 'ready';
      await this.saveProcessState(fromProcess);
    }

    // STEP 2: Load incoming process state
    const toProcess = this.processes.get(toPid)!;
    await this.loadProcessState(toProcess);
    toProcess.state = 'running';

    // STEP 3: Update scheduler state
    this.runningProcess = toPid;

    const switchTime = Date.now() - startTime;
    console.log(`🔄 Context switch: ${fromPid || 'idle'} → ${toPid} (${switchTime}ms)`);
  }
}
```

---

## 2. Interrupt System

### Hardware Interrupts (Event Types)

```typescript
enum InterruptType {
  // Hardware interrupts (highest priority)
  MENTION_IRQ = 0,           // @mention - immediate interrupt
  URGENT_MESSAGE_IRQ = 1,    // Urgent priority message

  // Software interrupts (lower priority)
  MESSAGE_SYSCALL = 2,       // Regular message received
  TIMER_IRQ = 3,             // Scheduled event (rate limit reset)
  CONTEXT_UPDATE = 4,        // Background context update

  // Exceptions
  RATE_LIMIT_EXCEPTION = 5,  // Rate limit exceeded
  LLM_TIMEOUT = 6,           // LLM API timeout
  MEMORY_FAULT = 7           // Context cache overflow
}

interface Interrupt {
  type: InterruptType;
  vector: number;            // Interrupt vector number
  priority: number;          // Interrupt priority
  data: any;                 // Interrupt data
  timestamp: Date;
  acknowledged: boolean;
}
```

### Interrupt Controller

```typescript
class PersonaInterruptController {
  private interruptVectorTable: Map<InterruptType, InterruptHandler> = new Map();
  private pendingInterrupts: Interrupt[] = [];
  private interruptsEnabled: boolean = true;
  private inInterruptHandler: boolean = false;

  /**
   * Raise interrupt (like CPU INT instruction)
   */
  raiseInterrupt(interrupt: Interrupt): void {
    if (!this.interruptsEnabled && interrupt.priority < 2) {
      // Only allow critical interrupts when disabled
      return;
    }

    // Add to pending interrupts
    this.pendingInterrupts.push(interrupt);
    this.pendingInterrupts.sort((a, b) => a.priority - b.priority);

    // If not already in handler, process immediately
    if (!this.inInterruptHandler) {
      this.processInterrupts();
    }
  }

  /**
   * Process pending interrupts
   */
  private async processInterrupts(): Promise<void> {
    while (this.pendingInterrupts.length > 0) {
      const interrupt = this.pendingInterrupts.shift()!;

      // Get handler for this interrupt type
      const handler = this.interruptVectorTable.get(interrupt.type);
      if (!handler) {
        console.error(`❌ No handler for interrupt type ${interrupt.type}`);
        continue;
      }

      // Execute interrupt handler
      this.inInterruptHandler = true;
      try {
        await handler(interrupt);
        interrupt.acknowledged = true;
      } catch (error) {
        console.error(`❌ Interrupt handler error:`, error);
      } finally {
        this.inInterruptHandler = false;
      }
    }
  }

  /**
   * Register interrupt handler
   */
  registerHandler(type: InterruptType, handler: InterruptHandler): void {
    this.interruptVectorTable.set(type, handler);
  }

  /**
   * Handle @mention interrupt (highest priority)
   */
  private async handleMentionInterrupt(interrupt: Interrupt): Promise<void> {
    const { personaId, message } = interrupt.data;

    console.log(`🔴 MENTION INTERRUPT: Persona ${personaId} mentioned in room ${message.roomId}`);

    // Get persona process
    const process = this.scheduler.getProcess(personaId);
    if (!process) return;

    // Force context switch to this room
    await this.scheduler.contextSwitch(
      this.scheduler.runningProcess,
      personaId
    );

    // Add high-priority event to process queue
    process.eventQueue.enqueue({
      type: 'mention',
      priority: EventPriority.CRITICAL,
      roomId: message.roomId,
      messageId: message.id,
      timestamp: new Date(),
      context: { senderType: 'human', messageText: message.content?.text || '' }
    });
  }
}
```

---

## 3. Memory Management

### Virtual Memory System (Per-Room Context)

```typescript
class PersonaMemoryManager {
  private pageSize: number = 20;          // Messages per "page"
  private maxResidentPages: number = 5;   // Max pages in memory (working set)

  // Virtual address space per persona
  private addressSpaces: Map<UUID, PersonaAddressSpace> = new Map();

  /**
   * Address space for a persona (like virtual memory)
   */
  interface PersonaAddressSpace {
    personaId: UUID;
    pageTable: Map<UUID, PageTableEntry>;  // Room ID → Page
    workingSet: Set<UUID>;                 // Currently loaded rooms
    freeMemory: number;                    // Available memory
  }

  /**
   * Page table entry (per room)
   */
  interface PageTableEntry {
    roomId: UUID;                // Virtual address (room ID)
    present: boolean;            // In memory?
    dirty: boolean;              // Modified since last save?
    accessed: Date;              // Last access time (for LRU)
    frameNumber: number;         // Physical memory location
    data: ConversationContext;   // Actual page data
  }

  /**
   * Load page (room context) into memory
   */
  async loadPage(personaId: UUID, roomId: UUID): Promise<ConversationContext> {
    const addressSpace = this.addressSpaces.get(personaId)!;
    const pageTable = addressSpace.pageTable;

    // Check if page already in memory
    const entry = pageTable.get(roomId);
    if (entry && entry.present) {
      // Page hit - update access time
      entry.accessed = new Date();
      console.log(`✅ Page HIT: Persona ${personaId}, Room ${roomId}`);
      return entry.data;
    }

    // Page fault - load from disk
    console.log(`💿 Page FAULT: Loading room ${roomId} from storage...`);

    // Check if working set is full
    if (addressSpace.workingSet.size >= this.maxResidentPages) {
      await this.evictPage(personaId);
    }

    // Load from SQLite
    const context = await this.loadFromDisk(personaId, roomId);

    // Add to page table
    pageTable.set(roomId, {
      roomId,
      present: true,
      dirty: false,
      accessed: new Date(),
      frameNumber: this.allocateFrame(),
      data: context
    });

    addressSpace.workingSet.add(roomId);
    return context;
  }

  /**
   * Evict page using LRU algorithm
   */
  private async evictPage(personaId: UUID): Promise<void> {
    const addressSpace = this.addressSpaces.get(personaId)!;

    // Find least recently used page
    let lruRoom: UUID | null = null;
    let lruTime: Date = new Date();

    for (const roomId of addressSpace.workingSet) {
      const entry = addressSpace.pageTable.get(roomId)!;
      if (entry.accessed < lruTime) {
        lruTime = entry.accessed;
        lruRoom = roomId;
      }
    }

    if (!lruRoom) return;

    const entry = addressSpace.pageTable.get(lruRoom)!;

    // Write back if dirty
    if (entry.dirty) {
      await this.writeToDisk(personaId, lruRoom, entry.data);
    }

    // Mark as not present
    entry.present = false;
    addressSpace.workingSet.delete(lruRoom);

    console.log(`🗑️  Page EVICTED: Persona ${personaId}, Room ${lruRoom} (LRU)`);
  }

  /**
   * Mark page as dirty (modified)
   */
  markDirty(personaId: UUID, roomId: UUID): void {
    const addressSpace = this.addressSpaces.get(personaId)!;
    const entry = addressSpace.pageTable.get(roomId);
    if (entry) {
      entry.dirty = true;
    }
  }
}
```

---

## 4. Inter-Process Communication (IPC)

### Message Passing Between Personas

```typescript
class PersonaIPC {
  /**
   * Send message to another persona (like Unix pipes or message queues)
   */
  async sendMessage(fromPid: UUID, toPid: UUID, message: IPCMessage): Promise<void> {
    const toProcess = this.scheduler.getProcess(toPid);
    if (!toProcess) {
      throw new Error(`Process ${toPid} not found`);
    }

    // Add to target's event queue
    toProcess.eventQueue.enqueue({
      type: 'ipc-message',
      priority: message.priority,
      timestamp: new Date(),
      context: {
        senderPid: fromPid,
        message: message.data
      }
    });

    console.log(`📨 IPC: ${fromPid} → ${toPid}`);
  }

  /**
   * Shared memory for collaboration (room context)
   */
  async createSharedMemory(roomId: UUID, personas: UUID[]): Promise<SharedMemory> {
    // Multiple personas can share read access to same room context
    const sharedContext = await this.memoryManager.loadPage(personas[0], roomId);

    return {
      roomId,
      readers: personas,
      context: sharedContext,
      lock: new PersonaMutex() // Prevent simultaneous writes
    };
  }

  /**
   * Semaphore for synchronization
   */
  class PersonaSemaphore {
    private count: number;
    private waitQueue: Array<{ resolve: () => void; personaId: UUID }> = [];

    constructor(initialCount: number) {
      this.count = initialCount;
    }

    /**
     * Wait (P operation / acquire)
     */
    async wait(personaId: UUID): Promise<void> {
      if (this.count > 0) {
        this.count--;
        return;
      }

      // Block until available
      return new Promise((resolve) => {
        this.waitQueue.push({ resolve, personaId });
        console.log(`⏸️  Persona ${personaId} blocked on semaphore`);
      });
    }

    /**
     * Signal (V operation / release)
     */
    signal(): void {
      if (this.waitQueue.length > 0) {
        const next = this.waitQueue.shift()!;
        console.log(`▶️  Persona ${next.personaId} unblocked`);
        next.resolve();
      } else {
        this.count++;
      }
    }
  }

  /**
   * Mutex for exclusive access (preventing AI-to-AI response loops)
   */
  class PersonaMutex extends PersonaSemaphore {
    constructor() {
      super(1); // Binary semaphore
    }

    async lock(personaId: UUID): Promise<void> {
      await this.wait(personaId);
    }

    unlock(): void {
      this.signal();
    }
  }
}
```

---

## 5. System Calls (Persona → Kernel)

### System Call Interface

```typescript
enum SystemCall {
  // Process management
  FORK_PERSONA,           // Create new persona
  EXIT_PERSONA,           // Terminate persona
  YIELD,                  // Give up CPU voluntarily

  // Memory management
  ALLOC_CONTEXT,          // Allocate room context
  FREE_CONTEXT,           // Free room context
  LOAD_RAG,               // Load RAG context
  SAVE_RAG,               // Save RAG context

  // I/O operations
  SEND_MESSAGE,           // Post message to room
  READ_MESSAGES,          // Read messages from room
  QUERY_DATABASE,         // Query DataDaemon

  // Inter-process communication
  SEND_IPC,               // Send to another persona
  RECV_IPC,               // Receive from another persona

  // LLM operations (expensive system calls)
  LLM_GENERATE,           // Generate response with LLM
  LLM_EMBED,              // Generate embeddings
}

class PersonaSystemCallHandler {
  /**
   * Handle system call from persona
   */
  async handleSyscall(syscall: SystemCall, args: any[]): Promise<any> {
    console.log(`⚙️  SYSCALL: ${SystemCall[syscall]}, args:`, args);

    switch (syscall) {
      case SystemCall.SEND_MESSAGE:
        return await this.syscall_sendMessage(args[0], args[1]);

      case SystemCall.LLM_GENERATE:
        return await this.syscall_llmGenerate(args[0], args[1]);

      case SystemCall.LOAD_RAG:
        return await this.syscall_loadRAG(args[0], args[1]);

      case SystemCall.YIELD:
        return await this.syscall_yield(args[0]);

      default:
        throw new Error(`Unknown syscall: ${syscall}`);
    }
  }

  /**
   * SYSCALL: Send message (blocking I/O)
   */
  private async syscall_sendMessage(personaId: UUID, message: ChatMessageEntity): Promise<void> {
    const process = this.scheduler.getProcess(personaId)!;

    // Block process (waiting for I/O)
    process.state = 'waiting';
    process.pendingIO.databaseOps++;

    try {
      // Execute via Commands API
      await Commands.execute(DATA_COMMANDS.CREATE, {
        collection: ChatMessageEntity.collection,
        backend: 'server',
        data: message
      });

      // Update process accounting
      process.responseCount++;

    } finally {
      // Unblock process
      process.pendingIO.databaseOps--;
      process.state = 'ready';
      this.scheduler.addToReadyQueue(process);
    }
  }

  /**
   * SYSCALL: LLM generate (very expensive, might timeout)
   */
  private async syscall_llmGenerate(personaId: UUID, context: LLMRequest): Promise<string> {
    const process = this.scheduler.getProcess(personaId)!;

    // Block process (waiting for external API)
    process.state = 'waiting';
    process.pendingIO.llmCalls++;

    console.log(`🤖 LLM API call: Persona ${personaId}, model: ${context.model}`);

    try {
      // Call LLM API (Claude/GPT)
      const response = await this.llmProvider.generate(context);
      return response.text;

    } catch (error) {
      // Raise timeout interrupt
      this.interruptController.raiseInterrupt({
        type: InterruptType.LLM_TIMEOUT,
        vector: 6,
        priority: 3,
        data: { personaId, error },
        timestamp: new Date(),
        acknowledged: false
      });
      throw error;

    } finally {
      process.pendingIO.llmCalls--;
      process.state = 'ready';
      this.scheduler.addToReadyQueue(process);
    }
  }

  /**
   * SYSCALL: Yield CPU (voluntary context switch)
   */
  private async syscall_yield(personaId: UUID): Promise<void> {
    const process = this.scheduler.getProcess(personaId)!;
    process.state = 'ready';

    // Force context switch to next process
    await this.scheduler.schedule();
  }
}
```

---

## 6. File System Layer

### Virtual File System for Persona Storage

```
/continuum/                           (root)
├── personas/                         (per-process private storage)
│   ├── persona-001/                  (like /proc/{pid})
│   │   ├── state.sqlite              (process state)
│   │   ├── rag_context/              (process memory)
│   │   │   ├── room-{uuid}.json
│   │   │   └── summaries/
│   │   ├── logs/                     (process logs)
│   │   │   └── debug.log
│   │   └── config.json               (process config)
│   │
│   ├── persona-002/
│   └── persona-003/
│
├── shared/                           (shared memory)
│   ├── room-contexts/                (multi-process access)
│   └── user-states/
│
└── system/                           (kernel space)
    ├── scheduler.log
    ├── interrupt.log
    └── memory.log
```

### File Descriptors for Personas

```typescript
interface PersonaFileDescriptor {
  fd: number;                  // File descriptor number
  path: string;                // File path
  mode: 'r' | 'w' | 'rw';      // Access mode
  position: number;            // Current read/write position
  personaId: UUID;             // Owning process
  openTime: Date;
}

class PersonaFileSystem {
  private fdTable: Map<number, PersonaFileDescriptor> = new Map();
  private nextFd: number = 3;  // 0=stdin, 1=stdout, 2=stderr

  /**
   * Open file (like Unix open())
   */
  open(personaId: UUID, path: string, mode: 'r' | 'w' | 'rw'): number {
    const fd = this.nextFd++;

    this.fdTable.set(fd, {
      fd,
      path,
      mode,
      position: 0,
      personaId,
      openTime: new Date()
    });

    console.log(`📂 OPEN: fd=${fd}, path=${path}, mode=${mode}`);
    return fd;
  }

  /**
   * Read from file descriptor
   */
  async read(fd: number, size: number): Promise<Buffer> {
    const descriptor = this.fdTable.get(fd);
    if (!descriptor) {
      throw new Error(`Bad file descriptor: ${fd}`);
    }

    // Read from file
    const data = await this.readFromStorage(descriptor.path, descriptor.position, size);
    descriptor.position += data.length;

    return data;
  }

  /**
   * Write to file descriptor
   */
  async write(fd: number, data: Buffer): Promise<number> {
    const descriptor = this.fdTable.get(fd);
    if (!descriptor || descriptor.mode === 'r') {
      throw new Error(`Bad file descriptor or not writable: ${fd}`);
    }

    // Write to file
    await this.writeToStorage(descriptor.path, descriptor.position, data);
    descriptor.position += data.length;

    return data.length;
  }

  /**
   * Close file descriptor
   */
  close(fd: number): void {
    this.fdTable.delete(fd);
    console.log(`📂 CLOSE: fd=${fd}`);
  }
}
```

---

## 7. Boot Sequence (System Initialization)

### Persona OS Boot Process

```typescript
class PersonaOS {
  /**
   * Boot sequence (like Linux boot)
   */
  async boot(): Promise<void> {
    console.log('🚀 Continuum Persona OS - Booting...');

    // PHASE 1: Hardware initialization
    await this.initializeHardware();

    // PHASE 2: Kernel initialization
    await this.initializeKernel();

    // PHASE 3: Load system daemons
    await this.loadSystemDaemons();

    // PHASE 4: Load user processes (personas)
    await this.loadPersonas();

    // PHASE 5: Start scheduler
    await this.startScheduler();

    console.log('✅ Persona OS ready - all processes loaded');
  }

  private async initializeKernel(): Promise<void> {
    console.log('⚙️  Initializing kernel...');

    // Initialize memory manager
    this.memoryManager = new PersonaMemoryManager();

    // Initialize interrupt controller
    this.interruptController = new PersonaInterruptController();
    this.registerInterruptHandlers();

    // Initialize process scheduler
    this.scheduler = new PersonaScheduler();

    // Initialize IPC
    this.ipc = new PersonaIPC();

    // Initialize file system
    this.fs = new PersonaFileSystem();

    console.log('✅ Kernel initialized');
  }

  private async loadPersonas(): Promise<void> {
    console.log('👥 Loading personas...');

    // Load all personas from database
    const personas = await DataDaemon.list<UserEntity>(COLLECTIONS.USERS, {
      filter: { type: 'persona' }
    });

    for (const personaEntity of personas) {
      // Create process control block
      const pcb = await this.createProcess(personaEntity);

      // Initialize persona (like exec())
      const persona = await PersonaUser.create(
        { ...personaEntity },
        this.context,
        this.router
      );

      // Add to scheduler
      this.scheduler.addProcess(pcb);

      console.log(`✅ Loaded persona: ${personaEntity.displayName} (PID: ${personaEntity.id})`);
    }
  }

  private async startScheduler(): Promise<void> {
    console.log('⏰ Starting scheduler...');

    // Main scheduling loop (runs forever)
    while (true) {
      await this.scheduler.schedule();

      // Small delay to prevent busy-wait
      await this.sleep(10);
    }
  }
}
```

---

## 8. Performance Metrics (Like `top` command)

### Persona Process Monitor

```typescript
interface PersonaProcessStats {
  pid: UUID;
  name: string;
  state: string;
  cpuUsage: number;      // Percentage
  memoryUsage: number;   // Contexts in memory
  ioWait: number;        // Time waiting for I/O
  uptime: number;        // Seconds since start
  responses: number;     // Total messages posted
  eventQueueSize: number;
}

class PersonaTop {
  /**
   * Display process stats (like Unix top)
   */
  displayStats(): void {
    console.log('┌─────────────────────────────────────────────────────────────┐');
    console.log('│                   PERSONA PROCESSES                          │');
    console.log('├─────────────────────────────────────────────────────────────┤');
    console.log('│ PID         NAME       STATE    CPU%   MEM  QUEUE  RESP     │');
    console.log('├─────────────────────────────────────────────────────────────┤');

    for (const [pid, process] of this.scheduler.processes) {
      const stats = this.getProcessStats(process);
      console.log(`│ ${stats.pid.slice(0,11)} ${stats.name.padEnd(10)} ${stats.state.padEnd(8)} ${stats.cpuUsage.toFixed(1)}%  ${stats.memoryUsage}    ${stats.eventQueueSize}      ${stats.responses}     │`);
    }

    console.log('└─────────────────────────────────────────────────────────────┘');
  }
}
```

---

## Summary: Why This Architecture Works

### 1. **Scalability**
- Each persona = independent process
- Kernel schedules fairly across all personas
- No hardcoded limits on persona count

### 2. **Isolation**
- Per-persona SQLite = private memory
- Virtual address spaces prevent context leakage
- Mutex prevents AI-to-AI loops

### 3. **Priority Handling**
- @mentions = interrupts (preempt current task)
- Keywords = normal priority
- Random engagement = background tasks

### 4. **Resource Management**
- Memory manager evicts cold contexts (LRU)
- Rate limiting = process quotas
- File descriptors track open resources

### 5. **Debugging**
- Clear kernel vs user space separation
- System logs for all operations
- Process stats like Unix `top`

### 6. **Extensibility**
- New personas = new processes (just boot them)
- New event types = new interrupt handlers
- New storage = new file system drivers

This is a **real operating system** for AI agents. Each PersonaUser is a process with its own memory, execution context, and scheduling priority - just like Linux processes!
