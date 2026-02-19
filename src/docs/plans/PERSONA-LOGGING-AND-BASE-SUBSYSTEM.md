# PersonaUser Logging & Base Subsystem Implementation Plan
**Phase 1 & 3 from Observability System - Immediate Work**

## Overview

**Goal**: Add segregated logging per persona/subsystem + extract common patterns into base class

**Why Now**:
- Tool execution, RAG, response generation are currently "black boxes"
- Multiple personas → logs mixed together, impossible to debug
- Mind/Body/Soul have duplicate initialization/logging logic

**Dependencies**: Being Architecture Phase 3 (Body) ✅ Complete

---

## Part 1: Segregated Logging Infrastructure

### Problem Statement

**Current Pain Points**:
1. PersonaToolExecutor fails → no visibility into which tool, which params, why
2. RAG retrieval wrong → can't see what was searched, what was found, why it matched
3. Response generation slow → which step? Template? LLM? Post-processing?
4. Helper AI logs mixed with Teacher AI logs → can't isolate issues
5. Cross-subsystem bugs → message flows through Mind → CNS → Body, but no trace

### Solution Architecture

**Log Directory Structure**:
```
.continuum/sessions/user/{personaId}/logs/
├── mind.log           # PersonaMind: cognition, working memory, planning
├── body.log           # PersonaBody: tool execution, response generation
├── soul.log           # PersonaSoul: memory, learning, genome operations
├── cns.log            # CNS: orchestration, scheduling decisions
├── tools.log          # PersonaToolExecutor: tool calls, results, errors
├── rag.log            # RAGBuilder: retrieval operations, context building
├── responses.log      # PersonaResponseGenerator: templates, LLM, post-processing
├── inbox.log          # PersonaInbox: queue operations, priority
├── autonomous.log     # PersonaAutonomousLoop: servicing cycles, cadence
└── system.log         # General PersonaUser lifecycle events
```

### Implementation Tasks

#### Task 1.1: Create SubsystemLogger Class

**File**: `system/logging/SubsystemLogger.ts`

**Features**:
- Per-persona isolation (separate log dirs per PersonaUser)
- Per-subsystem segregation (mind.log ≠ body.log ≠ tools.log)
- Trace correlation IDs (follow message through pipeline)
- Structured logging (JSON + human-readable formats)
- Performance timers (measure operation duration)
- Async writes (non-blocking, queue-based)
- Log rotation (by size/date, configurable)

**Interface**:
```typescript
class SubsystemLogger {
  constructor(config: {
    personaId: UUID;
    personaName: string;
    subsystem: SubsystemType;
    logPath?: string;  // Override default
  });

  // Basic logging
  info(message: string, metadata?: Record<string, unknown>): void;
  warn(message: string, metadata?: Record<string, unknown>): void;
  error(message: string, error?: Error, metadata?: Record<string, unknown>): void;
  debug(message: string, metadata?: Record<string, unknown>): void;

  // Trace correlation
  startTrace(traceId: string, operation: string): void;
  logTrace(traceId: string, event: string, metadata?: Record<string, unknown>): void;
  endTrace(traceId: string, result: 'success' | 'error', metadata?: Record<string, unknown>): void;

  // Performance profiling
  startTimer(label: string): string;  // Returns timerId
  endTimer(timerId: string, metadata?: Record<string, unknown>): void;
}

type SubsystemType =
  | 'mind' | 'body' | 'soul' | 'cns'
  | 'tools' | 'rag' | 'responses'
  | 'inbox' | 'autonomous' | 'system';
```

**Log Format Examples**:

Structured JSON (machine-parseable):
```json
{
  "timestamp": "2025-11-28T16:15:30.123Z",
  "level": "INFO",
  "subsystem": "tools",
  "personaId": "helper-ai-uuid",
  "personaName": "Helper AI",
  "traceId": "msg-abc123",
  "event": "tool:execute:complete",
  "message": "Tool execution completed successfully",
  "metadata": {
    "toolName": "screenshot",
    "duration": 234,
    "resultSize": 45678
  }
}
```

Human-readable (tail/grep friendly):
```
[2025-11-28 16:15:30.123] [INFO] [tools] [Helper AI] [trace:msg-abc123]
Tool execution completed successfully
  toolName: screenshot
  duration: 234ms
  resultSize: 45678 bytes
```

**Testing**:
- ✅ Logs go to correct files per persona/subsystem
- ✅ Trace IDs correlate across subsystems
- ✅ Performance timers work correctly
- ✅ Async writes don't block execution
- ✅ Log rotation triggers at configured size

#### Task 1.2: Integrate Logger with PersonaBody

**Files Modified**:
- `system/user/server/modules/being/PersonaBody.ts`
- `system/user/server/PersonaToolExecutor.ts`
- `system/user/server/PersonaResponseGenerator.ts`

**Changes**:

PersonaBody constructor:
```typescript
constructor(personaUser: PersonaUserForBody) {
  // Initialize logger FIRST
  this.logger = new SubsystemLogger({
    personaId: personaUser.id,
    personaName: personaUser.displayName,
    subsystem: 'body'
  });

  this.logger.info('PersonaBody initializing');

  // Create toolExecutor with tools logger
  this.toolExecutor = new PersonaToolExecutor(
    personaUser.id,
    personaUser.displayName,
    new SubsystemLogger({
      personaId: personaUser.id,
      personaName: personaUser.displayName,
      subsystem: 'tools'
    })
  );

  // Create responseGenerator with responses logger
  this.responseGenerator = new PersonaResponseGenerator({
    // ... existing params ...
    logger: new SubsystemLogger({
      personaId: personaUser.id,
      personaName: personaUser.displayName,
      subsystem: 'responses'
    })
  });

  this.logger.info('PersonaBody initialized');
}
```

PersonaToolExecutor with trace correlation:
```typescript
async executeTool(toolName: string, params: unknown, traceId: string): Promise<unknown> {
  this.logger.startTrace(traceId, `tool:${toolName}`);

  try {
    this.logger.logTrace(traceId, 'tool:lookup', { toolName });
    const tool = this.toolRegistry.getTool(toolName);

    this.logger.logTrace(traceId, 'tool:validate', { params });
    const validated = await this.validateParams(tool, params);

    const timerId = this.logger.startTimer(`tool:${toolName}:execution`);
    this.logger.logTrace(traceId, 'tool:execute:start', { toolName });

    const result = await tool.execute(validated);

    this.logger.endTimer(timerId, { success: true });
    this.logger.logTrace(traceId, 'tool:execute:complete', { toolName });
    this.logger.endTrace(traceId, 'success', { toolName, duration: timerId });

    return result;
  } catch (error) {
    this.logger.error(`Tool execution failed: ${toolName}`, error, { toolName, params, traceId });
    this.logger.endTrace(traceId, 'error', { toolName, error: error.message });
    throw error;
  }
}
```

**Testing**:
- ✅ Body logs go to `{personaId}/logs/body.log`
- ✅ Tool logs go to `{personaId}/logs/tools.log`
- ✅ Response logs go to `{personaId}/logs/responses.log`
- ✅ Trace IDs correlate tool execution with response generation
- ✅ Performance timers show slow tools

#### Task 1.3: Integrate Logger with PersonaMind

**Files Modified**:
- `system/user/server/modules/being/PersonaMind.ts`
- `system/user/server/modules/cognition/memory/WorkingMemoryManager.ts`
- `system/user/server/modules/cognitive/memory/Hippocampus.ts`

**Changes**:

PersonaMind constructor:
```typescript
constructor(personaUser: PersonaUserForMind) {
  this.logger = new SubsystemLogger({
    personaId: personaUser.id,
    personaName: personaUser.displayName,
    subsystem: 'mind'
  });

  this.logger.info('PersonaMind initializing');

  this.personaState = new PersonaStateManager(personaUser.displayName, {
    enableLogging: true
  });

  this.workingMemory = new WorkingMemoryManager(personaUser.id, this.logger);
  this.selfState = new PersonaSelfState(personaUser.id, this.logger);
  this.planFormulator = new SimplePlanFormulator(personaUser.id, personaUser.displayName, this.logger);

  this.logger.info('PersonaMind initialized', {
    workingMemoryCapacity: this.workingMemory.getCapacity(),
    energy: this.personaState.getCurrentState().energy
  });
}
```

WorkingMemoryManager with logging:
```typescript
async store(thought: WorkingMemoryEntry): Promise<void> {
  this.logger.debug('Storing thought', {
    importance: thought.importance,
    contentLength: thought.content.length,
    tags: thought.tags
  });

  await this.memoryStore.set(thought.id, thought);

  this.logger.info('Thought stored', {
    id: thought.id,
    importance: thought.importance,
    currentSize: this.getSize(),
    capacity: this.getCapacity()
  });
}

async recall(query: WorkingMemoryQuery): Promise<WorkingMemoryEntry[]> {
  const timerId = this.logger.startTimer('working-memory:recall');

  const results = await this.memoryStore.query(query);

  this.logger.endTimer(timerId, {
    queryType: query.minImportance ? 'importance' : 'all',
    resultsCount: results.length,
    limit: query.limit
  });

  return results;
}
```

**Testing**:
- ✅ Mind logs go to `{personaId}/logs/mind.log`
- ✅ Working memory operations logged (store, recall, eviction)
- ✅ Performance timers show slow recall queries
- ✅ Self-state changes logged (confidence, uncertainty)

#### Task 1.4: Integrate Logger with PersonaSoul

**Files Modified**:
- `system/user/server/modules/being/PersonaSoul.ts`
- `system/user/server/modules/memory/PersonaGenome.ts`
- `system/user/server/modules/cognitive/memory/Hippocampus.ts`

**Changes**:

PersonaSoul constructor:
```typescript
constructor(personaUser: PersonaUserForSoul) {
  this.logger = new SubsystemLogger({
    personaId: personaUser.id,
    personaName: personaUser.displayName,
    subsystem: 'soul'
  });

  this.logger.info('PersonaSoul initializing');

  this.memory = new RAGBuilder(personaUser.id, this.logger);
  this.genome = new PersonaGenome(personaUser.id, this.logger);
  this.hippocampus = new Hippocampus(personaUser, this.logger);

  this.logger.info('PersonaSoul initialized', {
    genomeAdapters: this.genome.getActiveAdapters().length,
    memorySize: this.memory.getSize()
  });
}
```

PersonaGenome with logging:
```typescript
async activateSkill(skillName: string): Promise<void> {
  this.logger.info('Activating skill', { skillName });

  const adapter = await this.loadAdapter(skillName);

  this.logger.info('Skill activated', {
    skillName,
    activeAdapters: this.getActiveAdapters().length,
    memoryPressure: this.getMemoryPressure()
  });

  // Check for LRU eviction
  if (this.getMemoryPressure() > 0.8) {
    this.logger.warn('Memory pressure high, evicting LRU adapter', {
      memoryPressure: this.getMemoryPressure()
    });
    await this.evictLRU();
  }
}
```

**Testing**:
- ✅ Soul logs go to `{personaId}/logs/soul.log`
- ✅ Genome operations logged (activate, evict)
- ✅ Memory operations logged (consolidation, retrieval)
- ✅ Hippocampus consolidation cycles logged

#### Task 1.5: Integrate Logger with CNS

**Files Modified**:
- `system/user/server/modules/central-nervous-system/PersonaCentralNervousSystem.ts`
- `system/user/server/modules/central-nervous-system/CNSFactory.ts`

**Changes**:

PersonaCentralNervousSystem constructor:
```typescript
constructor(config: CNSConfig) {
  this.logger = new SubsystemLogger({
    personaId: config.personaId,
    personaName: config.personaName,
    subsystem: 'cns'
  });

  this.logger.info('CNS initializing', {
    schedulerType: config.scheduler.constructor.name,
    enabledDomains: config.enabledDomains,
    allowBackgroundThreads: config.allowBackgroundThreads
  });

  this.scheduler = config.scheduler;
  this.inbox = config.inbox;
  this.personaState = config.personaState;

  this.logger.info('CNS initialized');
}

async routeMessage(message: QueueItem): Promise<void> {
  const traceId = message.id;

  this.logger.startTrace(traceId, 'cns:route-message');
  this.logger.logTrace(traceId, 'cns:evaluate-priority', {
    priority: message.priority,
    domain: message.domain
  });

  const decision = await this.scheduler.decide(message, this.personaState);

  this.logger.logTrace(traceId, 'cns:routing-decision', {
    shouldProcess: decision.shouldProcess,
    targetDomain: decision.targetDomain
  });

  if (decision.shouldProcess) {
    await this.processMessage(message);
    this.logger.endTrace(traceId, 'success', { domain: decision.targetDomain });
  } else {
    this.logger.endTrace(traceId, 'success', { skipped: true, reason: decision.reason });
  }
}
```

**Testing**:
- ✅ CNS logs go to `{personaId}/logs/cns.log`
- ✅ Routing decisions logged (priority, domain, scheduler choice)
- ✅ Message processing traced through CNS pipeline

### Success Criteria for Part 1

✅ **Log Isolation**: Helper AI's tool errors don't appear in Teacher AI's logs
✅ **Log Segregation**: RAG issues immediately visible in `rag.log`, not mixed with tools
✅ **Trace Correlation**: Can follow message `msg-abc123` from Inbox → CNS → Mind → Body → Tools
✅ **Performance Profiling**: Can identify slow subsystems via timer logs
✅ **Production Debugging**: Can diagnose issues without code changes or restarts

---

## Part 2: Base Subsystem Pattern (After Logging Works)

### Problem Statement

Mind, Body, Soul all have duplicate code for:
- Logger initialization
- Lifecycle management (init/shutdown)
- Status reporting
- Error handling patterns

### Solution Architecture

**Abstract Base Class**:
```typescript
export abstract class PersonaSubsystem {
  protected readonly personaId: UUID;
  protected readonly personaName: string;
  protected readonly logger: SubsystemLogger;
  private isInitialized = false;

  constructor(config: PersonaSubsystemConfig) {
    this.personaId = config.personaId;
    this.personaName = config.personaName;
    this.logger = new SubsystemLogger({
      personaId: config.personaId,
      personaName: config.personaName,
      subsystem: this.getSubsystemType()
    });
  }

  async initialize(): Promise<void> {
    if (this.isInitialized) {
      this.logger.warn('initialize() called on already-initialized subsystem');
      return;
    }

    try {
      this.logger.info(`${this.getSubsystemType()} initializing`);
      await this.onInitialize();
      this.isInitialized = true;
      this.logger.info(`${this.getSubsystemType()} initialized successfully`);
    } catch (error) {
      this.logger.error(`${this.getSubsystemType()} initialization failed`, error);
      throw error;
    }
  }

  async shutdown(): Promise<void> {
    if (!this.isInitialized) {
      this.logger.warn('shutdown() called on non-initialized subsystem');
      return;
    }

    try {
      this.logger.info(`${this.getSubsystemType()} shutting down`);
      await this.onShutdown();
      this.isInitialized = false;
      this.logger.info(`${this.getSubsystemType()} shutdown complete`);
    } catch (error) {
      this.logger.error(`${this.getSubsystemType()} shutdown failed`, error);
      throw error;
    }
  }

  getStatus(): SubsystemStatus {
    return {
      subsystem: this.getSubsystemType(),
      personaId: this.personaId,
      personaName: this.personaName,
      initialized: this.isInitialized,
      healthy: this.isHealthy(),
      metrics: this.getMetrics()
    };
  }

  // Abstract methods - each subsystem implements
  protected abstract getSubsystemType(): SubsystemType;
  protected abstract onInitialize(): Promise<void>;
  protected abstract onShutdown(): Promise<void>;
  protected abstract isHealthy(): boolean;
  protected abstract getMetrics(): Record<string, unknown>;
}
```

### Implementation Tasks

#### Task 2.1: Create PersonaSubsystem Base Class

**File**: `system/user/server/modules/being/PersonaSubsystem.ts`

**Responsibilities**:
- Logger initialization (auto-configured per subsystem)
- Lifecycle management (init/shutdown with error handling)
- Status reporting (for health checks)
- Metrics collection (for monitoring)

**Testing**:
- ✅ Subclasses can extend without duplicating logging setup
- ✅ Initialize/shutdown patterns work consistently
- ✅ Status reporting shows correct state
- ✅ Metrics collection works for all subsystems

#### Task 2.2: Migrate PersonaMind to Extend Base

**File**: `system/user/server/modules/being/PersonaMind.ts`

**Changes**:
```typescript
export class PersonaMind extends PersonaSubsystem {
  public readonly personaState: PersonaStateManager;
  public readonly workingMemory: WorkingMemoryManager;
  public readonly selfState: PersonaSelfState;
  public readonly planFormulator: SimplePlanFormulator;

  constructor(personaUser: PersonaUserForMind) {
    super({
      personaId: personaUser.id,
      personaName: personaUser.displayName
    });

    // Mind-specific components
    this.personaState = new PersonaStateManager(personaUser.displayName);
    this.workingMemory = new WorkingMemoryManager(personaUser.id);
    this.selfState = new PersonaSelfState(personaUser.id);
    this.planFormulator = new SimplePlanFormulator(personaUser.id, personaUser.displayName);
  }

  protected getSubsystemType(): SubsystemType {
    return 'mind';
  }

  protected async onInitialize(): Promise<void> {
    await this.workingMemory.initialize();
    this.logger.info('Mind initialization complete', {
      workingMemoryCapacity: this.workingMemory.getCapacity()
    });
  }

  protected async onShutdown(): Promise<void> {
    await this.workingMemory.flush();
    this.logger.info('Mind shutdown complete');
  }

  protected isHealthy(): boolean {
    return this.workingMemory.isHealthy() &&
           this.personaState.getCurrentState().energy > 0;
  }

  protected getMetrics(): Record<string, unknown> {
    return {
      workingMemorySize: this.workingMemory.getSize(),
      energy: this.personaState.getCurrentState().energy,
      mood: this.personaState.getCurrentState().mood
    };
  }
}
```

**Testing**:
- ✅ Logger auto-configured to write to `mind.log`
- ✅ Initialize/shutdown work via base class
- ✅ Status reporting shows health + metrics

#### Task 2.3: Migrate PersonaBody to Extend Base

**File**: `system/user/server/modules/being/PersonaBody.ts`

**Changes**: Similar to PersonaMind migration

**Testing**:
- ✅ Logger auto-configured to write to `body.log`
- ✅ Initialize/shutdown work via base class
- ✅ Tool registry stats in metrics

#### Task 2.4: Migrate PersonaSoul to Extend Base

**File**: `system/user/server/modules/being/PersonaSoul.ts`

**Changes**: Similar to PersonaMind migration

**Testing**:
- ✅ Logger auto-configured to write to `soul.log`
- ✅ Initialize/shutdown work via base class
- ✅ Genome adapter stats in metrics

### Success Criteria for Part 2

✅ **No Duplicate Code**: Logging/lifecycle patterns centralized in base class
✅ **Consistent Patterns**: All subsystems init/shutdown the same way
✅ **Easy Health Checks**: `./jtag health` works uniformly across subsystems
✅ **Easy Extension**: Adding new subsystems (e.g., PersonaBrain) trivial

---

## Testing Strategy

### Unit Tests

```bash
# SubsystemLogger
npx vitest tests/unit/SubsystemLogger.test.ts
  ✅ Logs to correct file paths
  ✅ Trace IDs correlate across logs
  ✅ Performance timers measure correctly
  ✅ Async writes don't block

# PersonaSubsystem base class
npx vitest tests/unit/PersonaSubsystem.test.ts
  ✅ Initialize/shutdown lifecycle
  ✅ Status reporting
  ✅ Metrics collection

# PersonaMind/Body/Soul logging
npx vitest tests/unit/PersonaMind.logging.test.ts
npx vitest tests/unit/PersonaBody.logging.test.ts
npx vitest tests/unit/PersonaSoul.logging.test.ts
  ✅ Logs go to correct files
  ✅ Important operations logged
```

### Integration Tests

```bash
# End-to-end message trace
npx vitest tests/integration/message-trace.test.ts
  ✅ Send message to PersonaUser
  ✅ Check logs in: inbox.log → cns.log → mind.log → body.log → tools.log
  ✅ Verify trace ID correlates across all logs

# Multi-persona isolation
npx vitest tests/integration/multi-persona-logging.test.ts
  ✅ Create Helper AI + Teacher AI
  ✅ Send messages to both
  ✅ Verify logs segregated (Helper AI logs ≠ Teacher AI logs)
```

### Manual Testing

```bash
# Deploy and run
npm start

# Send message to Helper AI
./jtag collaboration/chat/send --room="general" --message="Test logging"

# Check logs
tail -f .continuum/sessions/user/helper-ai-uuid/logs/tools.log
tail -f .continuum/sessions/user/helper-ai-uuid/logs/cns.log

# Verify trace correlation
grep "msg-abc123" .continuum/sessions/user/helper-ai-uuid/logs/*.log

# Performance profiling
grep "duration:" .continuum/sessions/user/helper-ai-uuid/logs/tools.log
```

---

## Implementation Timeline

### Week 1: Logging Infrastructure (PRIORITY)
- **Day 1-2**: Create SubsystemLogger class + tests
- **Day 3**: Integrate with PersonaBody (tools.log, responses.log)
- **Day 4**: Integrate with PersonaMind (mind.log)
- **Day 5**: Integrate with PersonaSoul (soul.log) + CNS (cns.log)
- **Day 6-7**: Integration testing, bug fixes, performance tuning

### Week 2: Base Subsystem Pattern (CLEANUP)
- **Day 1-2**: Create PersonaSubsystem base class + tests
- **Day 3**: Migrate PersonaMind to extend base
- **Day 4**: Migrate PersonaBody to extend base
- **Day 5**: Migrate PersonaSoul to extend base
- **Day 6-7**: Integration testing, refactor any issues

---

## Future Work (Not in This Plan)

### Command Introspection (Separate Plan)
- `./jtag mind/working-memory`
- `./jtag body/tools`
- `./jtag soul/genome`
- `./jtag trace --messageId=...`

### Visual Dashboards (Future Vision)
- Three.js industrial brain visualization
- Event-driven updates (already have UI with 4 squares!)
- Mind/Body/Soul/CNS as interactive boxes
- Data flow pipes with animations

---

## Success Metrics

**Debugging Speed**:
- ❌ Before: 30+ minutes to diagnose tool failure (edit code, redeploy, test)
- ✅ After: 2 minutes (tail tools.log, see exact error + trace)

**Log Noise**:
- ❌ Before: All personas mixed in system.log (thousands of lines)
- ✅ After: Per-persona logs (Helper AI: 100 lines, Teacher AI: 50 lines)

**Performance Visibility**:
- ❌ Before: "System slow" → no idea which subsystem
- ✅ After: Grep duration logs → "tools.log shows screenshot: 2.3s"

**Production Diagnosis**:
- ❌ Before: Add console.logs, redeploy, wait 2 minutes, test
- ✅ After: Tail logs in real-time, see exactly what's happening

---

## Related Documents

- `docs/personas/PERSONA-OBSERVABILITY-SYSTEM.md` - Full observability vision (this plan implements Phases 1 & 3)
- `docs/personas/PERSONA-BEING-ARCHITECTURE.md` - Mind/Body/Soul decomposition
- `src/system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md` - Integration architecture

---

**Document Status**: ✅ Ready for Implementation
**Priority**: HIGH (Week 1 starts immediately)
**Owner**: Being Architecture Team
**Last Updated**: 2025-11-28
