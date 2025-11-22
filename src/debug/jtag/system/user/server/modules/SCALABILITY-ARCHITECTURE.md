# Scalability Architecture: Event-Driven Personas + Database Splitting

**Status**: Design Phase
**Related PRs**: #188 (Persona Cognition Phase 1), #192 (Progressive Scoring Phase 2)
**Target**: Phase 4 - System Scalability & Long-Term Stability

---

## The Problem: Polling + Single DB = Performance Degradation

### Current Bottlenecks (Identified 2025-11-21)

**1. Polling Architecture (CPU Waste)**
```typescript
// PersonaUser.ts - Each persona polls independently
private async serviceInbox(): Promise<void> {
  while (this.isActive) {
    await this.checkForMessages();  // 13+ personas × polling
    await this.sleep(this.adaptiveCadence);  // 3-10s intervals
  }
}
```

**Problem**: Thundering herd of 13+ personas hammering database every 3-10 seconds
- **CPU usage**: Constant polling even when idle (zero messages)
- **Lock contention**: All personas compete for same SQLite database lock
- **Latency**: Average response time degrades as more personas added

**2. Single Database (Lock Contention)**
```
database.sqlite (monolithic, growing unbounded)
├── users (20 rows)
├── rooms (5 rows)
├── chat_messages (1000s, growing)
├── cognition_records (100s/day × 13 personas = 1300+/day)
├── cognition_plans (similar growth)
├── decision_records (growing)
├── genome_configs (static)
└── training_datasets (large blobs)
```

**Problem**: SQLite writer lock blocks all other operations
- **Write serialization**: Only ONE persona can write at a time
- **Memory bloat**: Loading entire DB into memory on every query
- **No cleanup**: Cognition records accumulate indefinitely
- **Mixed access patterns**: Hot data (messages) mixed with cold data (genomes)

**3. Unbounded Growth (Memory Leaks)**
- Cognition records never pruned (retention policy missing)
- Event subscriptions accumulate (personas re-subscribe on every message)
- Message history loaded in full (no pagination)
- Working memory grows without bounds

**4. No Backpressure (System Hangs)**
- Personas accept all tasks even when overloaded
- Inbox queues grow unbounded
- No "I'm busy" signaling mechanism
- System becomes unresponsive under load

---

## The Solution: Two-Pronged Architectural Redesign

### Part A: Database Splitting (Data Locality by Access Pattern)

**Principle**: Split by **access pattern** (not by table). Group data that's accessed together.

#### 1. **Core Relational DB** (Needs joins/transactions)
```
database.sqlite (hot, frequently accessed)
├── users (joins with messages)
├── rooms (joins with memberships)
├── room_memberships (joins both)
└── chat_messages (joins users + rooms)
```
**Access pattern**: High-frequency reads with joins (every message)
**Retention**: Keep all (core system data)

#### 2. **Per-Persona Cognition** (Isolated, no relations)
```
.continuum/jtag/cognition/
├── persona-{uuid}-cognition.sqlite
│   ├── cognition_records
│   └── cognition_plans
```
**Access pattern**: Write-heavy, persona-specific, NO joins
**Retention**: 30 days rolling window (prune old records)
**Benefit**: Each persona writes to own file (NO lock contention!)

#### 3. **Genome Storage** (Document store, static)
```
.continuum/jtag/genomes/
└── genomes.sqlite
    ├── genome_configs
    └── lora_adapters
```
**Access pattern**: Read-mostly, large blobs
**Retention**: Keep all (configuration data)

#### 4. **Decision/Voting** (Append-only archive)
```
.continuum/jtag/decisions/
└── decisions.sqlite
    ├── decision_records
    ├── decision_options
    └── decision_votes
```
**Access pattern**: Write-once, rare reads
**Retention**: Keep all (historical record)

#### 5. **Ephemeral Cache** (Session data, TTL cleanup)
```
.continuum/jtag/cache/
├── rag-embeddings.sqlite (cleared on restart)
├── working-memory.sqlite (cleared on restart)
└── session-state.sqlite (cleared on restart)
```
**Access pattern**: Frequent read/write, short-lived
**Retention**: Clear on restart (ephemeral)

**Benefits of Database Splitting:**
- **10-100x better concurrency**: 13 personas writing to different files simultaneously
- **Easier cleanup**: Delete old cognition files per retention policy
- **Reduced memory**: Load only core DB in memory, fetch cognition on demand
- **Better diagnostics**: Check file sizes to see which persona generates most records
- **Faster backups**: Backup only what's needed (skip cache, archive old cognition separately)

### Part B: Event-Driven Concurrency (Eliminate Polling)

**Principle**: React to events (not poll for work). Zero CPU when idle.

#### Phase 1: Event-Driven Personas (Eliminate Polling)

**Current (Polling)**:
```typescript
// Each persona polls independently
private async serviceInbox(): Promise<void> {
  while (this.isActive) {
    const messages = await this.checkForMessages();  // Poll DB
    if (messages.length > 0) {
      await this.processMessages(messages);
    }
    await this.sleep(this.adaptiveCadence);  // 3-10s
  }
}
```
**Problem**: 13 personas × polling every 3-10s = constant DB queries even when idle

**Better (Event-Driven)**:
```typescript
// Subscribe to events ONCE in constructor
constructor(entity: UserEntity, client: JTAGClient) {
  super(entity, client);

  // Subscribe to message events (zero CPU when idle)
  Events.subscribe('chat:message:created', async (event) => {
    await this.handleMessageEvent(event);
  });

  // Subscribe to system events
  Events.subscribe('system:shutdown', () => this.cleanup());
}

private async handleMessageEvent(event: MessageCreatedEvent): Promise<void> {
  // Check if this message is for me
  if (!this.shouldRespond(event.message)) return;

  // Process message
  await this.evaluateAndPossiblyRespond(event.message);
}
```
**Benefit**: Zero CPU usage when no messages. Instant response (no polling delay).

**Implementation:**
1. Remove `serviceInbox()` polling loop
2. Add event subscriptions in constructor
3. Update RoomMembershipDaemon to emit `chat:message:created` events
4. Personas react to events (not poll for work)

**Migration Strategy:**
- Keep polling as fallback during transition
- Add event-driven path alongside polling
- Measure performance (event latency vs poll latency)
- Remove polling once event-driven proven stable

#### Phase 2: Work-Stealing Scheduler

**Current (Thread-Per-Task)**:
```typescript
// Each persona has own independent loop
13 personas × independent timers = resource waste
```
**Problem**: No load balancing. Some personas idle while others overloaded.

**Better (Work-Stealing Scheduler)**:
```typescript
// Single scheduler dispatches work to available personas
class PersonaScheduler {
  private workQueue: PriorityQueue<Task>;
  private workers: Map<UUID, PersonaWorker>;

  async dispatch(task: Task): Promise<void> {
    // Add to priority queue
    this.workQueue.push(task);

    // Pick least-loaded available worker
    const worker = this.selectWorker();
    if (worker) {
      await worker.processTask(this.workQueue.pop());
    }
  }

  private selectWorker(): PersonaWorker | null {
    // Work-stealing: pick least loaded
    let minLoad = Infinity;
    let selected: PersonaWorker | null = null;

    for (const [id, worker] of this.workers) {
      if (worker.isAvailable() && worker.currentLoad < minLoad) {
        minLoad = worker.currentLoad;
        selected = worker;
      }
    }

    return selected;
  }
}
```
**Benefit**:
- Load balancing: work distributed evenly
- Better utilization: no idle workers while others overloaded
- Simpler scaling: add workers without code changes

**Implementation:**
1. Create PersonaScheduler class
2. Personas register as workers
3. Tasks posted to scheduler (not directly to personas)
4. Scheduler picks least-loaded worker

#### Phase 3: Reactive Streams (Backpressure)

**Current (Unbounded Queues)**:
```typescript
// Inbox grows unbounded
this.inbox.push(task);  // No limits!
```
**Problem**: Overwhelmed personas accept more work than they can handle

**Better (Backpressure)**:
```typescript
class PersonaInbox {
  private queue: Task[] = [];
  private readonly MAX_SIZE = 100;

  push(task: Task): boolean {
    if (this.queue.length >= this.MAX_SIZE) {
      // Signal backpressure
      Events.emit('persona:overloaded', {
        personaId: this.personaId,
        queueSize: this.queue.length
      });
      return false;  // Reject task
    }

    this.queue.push(task);
    return true;  // Accepted
  }
}

// Scheduler respects backpressure
class PersonaScheduler {
  async dispatch(task: Task): Promise<void> {
    const worker = this.selectWorker();
    if (!worker) {
      // All workers overloaded, queue task for later
      this.waitingTasks.push(task);
      return;
    }

    const accepted = await worker.inbox.push(task);
    if (!accepted) {
      // Worker rejected due to backpressure
      this.waitingTasks.push(task);
    }
  }
}
```
**Benefit**:
- Graceful degradation under load
- No system hangs from unbounded growth
- Clear feedback ("I'm busy, try later")

**Implementation:**
1. Add MAX_SIZE to PersonaInbox
2. Return false when queue full
3. Emit backpressure events
4. Scheduler queues rejected tasks for retry

---

## Implementation Roadmap

### Phase 1A: Split Cognition DB (Immediate Win) - **PRIORITY 1**

**Effort**: 2-3 days
**Impact**: 10-100x better write concurrency

**Tasks**:
1. Create CognitionDataAdapter routing to per-persona files
2. Migrate existing cognition records to new files
3. Update PersonaUser to use dedicated cognition DB
4. Add retention policy (prune records > 30 days)

**Success Metrics**:
- Zero write lock contention between personas
- 10x faster cognition record writes
- Memory usage stable (no growth)

### Phase 1B: Event-Driven Personas (Eliminate Polling) - **PRIORITY 2**

**Effort**: 3-5 days
**Impact**: 90% reduction in CPU usage when idle

**Tasks**:
1. Remove serviceInbox() polling loop
2. Add event subscriptions in constructor
3. Update RoomMembershipDaemon to emit events
4. Measure latency (event vs poll)
5. Remove polling fallback once stable

**Success Metrics**:
- Zero CPU when no messages
- <100ms response latency (vs 3-10s polling delay)
- No missed messages (reliability)

### Phase 2A: Split Genome + Decision DBs

**Effort**: 1-2 days
**Impact**: Further reduce lock contention

**Tasks**:
1. Create GenomeDataAdapter for genome storage
2. Create DecisionDataAdapter for voting data
3. Migrate existing data to new files

### Phase 2B: Work-Stealing Scheduler

**Effort**: 5-7 days
**Impact**: Better load balancing

**Tasks**:
1. Create PersonaScheduler class
2. Implement work-stealing algorithm
3. Update personas to register as workers
4. Route tasks through scheduler

**Success Metrics**:
- Even load distribution (variance < 20%)
- No idle workers while others overloaded

### Phase 3A: Add Connection Pooling

**Effort**: 2-3 days
**Impact**: Reduce connection overhead

**Tasks**:
1. Create ConnectionPool for SQLite adapters
2. Configure max connections per DB
3. Add connection reuse logic

### Phase 3B: Reactive Streams (Backpressure)

**Effort**: 3-4 days
**Impact**: Graceful degradation under load

**Tasks**:
1. Add MAX_SIZE to PersonaInbox
2. Implement backpressure signaling
3. Update scheduler to respect backpressure
4. Add retry queue for rejected tasks

**Success Metrics**:
- No unbounded queue growth
- System remains responsive under load
- Clear backpressure metrics

---

## Testing Strategy

### Unit Tests
- PersonaScheduler work-stealing algorithm
- ConnectionPool connection reuse
- PersonaInbox backpressure logic

### Integration Tests
- Event-driven message handling (no polling)
- Per-persona cognition DB writes (no lock contention)
- Backpressure signaling (reject when overloaded)

### Load Tests
- 100 messages/second sustained load
- 13 personas processing simultaneously
- Measure: latency, CPU, memory, lock contention

### Chaos Tests
- Kill random personas (scheduler recovers)
- Overflow inboxes (backpressure triggers)
- Database locks (retry logic works)

---

## Success Criteria

**Before (Current)**:
- System degrades over hours
- Ping timeout after 2-3 hours uptime
- 13 personas polling every 3-10s
- Single DB = lock contention
- No backpressure = unbounded growth

**After (Target)**:
- System stable for days/weeks
- Ping responds <100ms consistently
- Zero CPU when idle
- 13 personas writing to different files (no contention)
- Backpressure prevents overload

**Metrics**:
- **Uptime**: 2-3 hours → 7+ days
- **CPU (idle)**: 5-10% → <1%
- **Response latency**: 3-10s (polling) → <100ms (events)
- **Write concurrency**: 1 writer at a time → 13+ simultaneous
- **Memory growth**: Unbounded → Bounded (retention policies)

---

## Related Documentation

- **PERSONA-CONVERGENCE-ROADMAP.md** - Integration of three visions
- **AUTONOMOUS-LOOP-ROADMAP.md** - RTOS-inspired servicing (to be replaced)
- **LORA-GENOME-PAGING.md** - Virtual memory for skills
- **PHASE2-PROGRESSIVE-SCORING-PLAN.md** - Complexity routing (separate concern)

---

## Notes

This architecture addresses the fundamental scalability issues identified on 2025-11-21:
- Polling loops causing CPU waste
- Single database causing lock contention
- Unbounded growth causing memory leaks
- No backpressure causing system hangs

The solution combines:
1. **Database splitting** (data locality by access pattern)
2. **Event-driven** (eliminate polling)
3. **Work-stealing** (load balancing)
4. **Backpressure** (graceful degradation)

This is NOT about Progressive Scoring (Phase 2) - that's about **routing work to appropriate models**. This is about **system scalability** - ensuring the system can handle load without degradation.

Implementation prioritizes quick wins (Phase 1A: cognition DB splitting) before larger refactors (Phase 2B: work-stealing scheduler).
