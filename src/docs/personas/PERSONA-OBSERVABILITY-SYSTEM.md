# PersonaUser Observability System
**Industrial-Grade Brain Engineering: Logging, Introspection, and Visualization**

## Vision

Transform PersonaUser debugging from "black box mystery" to **"industrial control room with real-time dashboards"**. Every subsystem (Mind/Body/Soul/CNS) gets segregated logs, command-driven introspection, and eventually Three.js visualization like Unreal Engine's Blueprint Editor meets factory floor.

**Design Philosophy**: "Systems Engineering meets Neuroscience Visualization"

---

## The Problem

### Current State: Black Box Debugging
- **Tool execution fails** → Where? Why? Check 1000+ line PersonaUser.ts
- **RAG retrieval wrong** → No visibility into what was searched, what was found
- **Response generation slow** → Which step? Template? LLM? Post-processing?
- **Multiple personas** → Logs mixed together, can't isolate Helper AI vs Teacher AI
- **Cross-subsystem bugs** → Message flows through Mind → CNS → Body, but no trace correlation

### Impact
- Debugging takes 10x longer than it should
- Can't diagnose production issues without code changes
- Performance bottlenecks invisible
- Multi-persona systems impossible to debug

---

## Architecture Overview

### Three Pillars

1. **Segregated Logging** (Per-Persona, Per-Subsystem)
   - Each PersonaUser gets own log directory
   - Each subsystem (Mind/Body/Soul/CNS/Tools/RAG/Responses) gets own log file
   - Trace correlation IDs connect events across subsystems

2. **Command Introspection** (Real-Time State Inspection)
   - CLI commands expose internal state without code changes
   - Query working memory, tool queue, genome state, CNS decisions
   - Cross-subsystem tracing (follow message through entire pipeline)

3. **Visual Dashboards** (Industrial Three.js Visualization)
   - Mind/Body/Soul/CNS rendered as 3D industrial boxes
   - Data flow shown as glowing pipes
   - Click-to-drill-down interaction
   - Status indicators (gauges, pressure readings, queue depth)

---

## Phase 1: Segregated Logging (IMMEDIATE PRIORITY)

### Log Directory Structure

```
.continuum/sessions/user/{personaId}/logs/
├── mind.log           # PersonaMind: cognition, working memory, planning
├── body.log           # PersonaBody: tool execution, response generation
├── soul.log           # PersonaSoul: memory, learning, genome operations
├── cns.log            # CNS: orchestration, scheduling decisions
├── rag.log            # RAG: retrieval operations, context building
├── tools.log          # Tool execution: calls, results, errors
├── responses.log      # Response generation: templates, LLM calls, post-processing
├── inbox.log          # PersonaInbox: queue operations, priority decisions
├── autonomous.log     # PersonaAutonomousLoop: servicing cycles, cadence changes
└── system.log         # General PersonaUser lifecycle events
```

**Example Paths**:
- Helper AI mind: `.continuum/sessions/user/helper-ai-uuid/logs/mind.log`
- Teacher AI tools: `.continuum/sessions/user/teacher-ai-uuid/logs/tools.log`

### SubsystemLogger Architecture

```typescript
/**
 * SubsystemLogger - Segregated logging per persona, per subsystem
 *
 * Features:
 * - Per-persona isolation (Helper AI logs ≠ Teacher AI logs)
 * - Per-subsystem segregation (tools.log ≠ rag.log)
 * - Trace correlation IDs (follow message through pipeline)
 * - Structured logging (JSON + human-readable)
 * - Log rotation (by size/date)
 */
class SubsystemLogger {
  constructor(config: {
    personaId: UUID;
    personaName: string;
    subsystem: SubsystemType;  // 'mind' | 'body' | 'soul' | 'cns' | 'tools' | 'rag' | 'responses'
    logPath?: string;  // Override default path
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
  | 'mind'       // PersonaMind
  | 'body'       // PersonaBody
  | 'soul'       // PersonaSoul
  | 'cns'        // CNS
  | 'tools'      // PersonaToolExecutor
  | 'rag'        // RAGBuilder
  | 'responses'  // PersonaResponseGenerator
  | 'inbox'      // PersonaInbox
  | 'autonomous' // PersonaAutonomousLoop
  | 'system';    // General PersonaUser
```

### Integration with Subsystems

#### Example: PersonaBody with Logging

```typescript
export class PersonaBody {
  public readonly toolExecutor: PersonaToolExecutor;
  public readonly toolRegistry: PersonaToolRegistry;
  public readonly responseGenerator: PersonaResponseGenerator;

  private readonly logger: SubsystemLogger;

  constructor(personaUser: PersonaUserForBody) {
    // Initialize logger FIRST
    this.logger = new SubsystemLogger({
      personaId: personaUser.id,
      personaName: personaUser.displayName,
      subsystem: 'body'
    });

    this.logger.info('PersonaBody initializing', {
      personaId: personaUser.id,
      personaName: personaUser.displayName
    });

    // Create subsystems with their own loggers
    this.toolExecutor = new PersonaToolExecutor(
      personaUser.id,
      personaUser.displayName,
      new SubsystemLogger({
        personaId: personaUser.id,
        personaName: personaUser.displayName,
        subsystem: 'tools'
      })
    );

    this.toolRegistry = new PersonaToolRegistry();

    this.responseGenerator = new PersonaResponseGenerator({
      personaId: personaUser.id,
      personaName: personaUser.displayName,
      entity: personaUser.entity,
      modelConfig: personaUser.modelConfig,
      client: personaUser.client,
      toolExecutor: this.toolExecutor,
      toolRegistry: this.toolRegistry,
      mediaConfig: personaUser.mediaConfig,
      getSessionId: personaUser.getSessionId,
      logger: new SubsystemLogger({
        personaId: personaUser.id,
        personaName: personaUser.displayName,
        subsystem: 'responses'
      })
    });

    this.logger.info('PersonaBody initialized', {
      toolsRegistered: this.toolRegistry.getAllTools().length
    });
  }
}
```

#### Example: Tool Execution with Trace Correlation

```typescript
async executeTool(toolName: string, params: unknown, traceId: string): Promise<unknown> {
  this.logger.startTrace(traceId, `tool:${toolName}`);

  try {
    this.logger.logTrace(traceId, 'tool:lookup', { toolName });
    const tool = this.toolRegistry.getTool(toolName);

    this.logger.logTrace(traceId, 'tool:validate', { params });
    const validatedParams = await this.validateParams(tool, params);

    const timerId = this.logger.startTimer(`tool:${toolName}:execution`);
    this.logger.logTrace(traceId, 'tool:execute:start', { toolName, params: validatedParams });

    const result = await tool.execute(validatedParams);

    this.logger.endTimer(timerId, { success: true, resultSize: JSON.stringify(result).length });
    this.logger.logTrace(traceId, 'tool:execute:complete', { toolName, resultType: typeof result });

    this.logger.endTrace(traceId, 'success', { toolName, duration: timerId });
    return result;

  } catch (error) {
    this.logger.error(`Tool execution failed: ${toolName}`, error, { toolName, params, traceId });
    this.logger.endTrace(traceId, 'error', { toolName, error: error.message });
    throw error;
  }
}
```

### Log Format

**Structured JSON** (for machine parsing):
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

**Human-Readable** (for tail/grep):
```
[2025-11-28 16:15:30.123] [INFO] [tools] [Helper AI] [trace:msg-abc123]
Tool execution completed successfully
  toolName: screenshot
  duration: 234ms
  resultSize: 45678 bytes
```

### Benefits

✅ **Per-Persona Isolation**: Helper AI's tool errors don't clutter Teacher AI's logs
✅ **Per-Subsystem Segregation**: RAG issues immediately visible in `rag.log`
✅ **Trace Correlation**: Follow message `msg-abc123` through CNS → Mind → Body → Tools
✅ **Performance Profiling**: See which subsystem is slow (timer data in logs)
✅ **Production Debugging**: Diagnose issues without code changes or restarts

---

## Phase 2: Command Introspection (MEDIUM PRIORITY)

### Mind Introspection Commands

```bash
# Working memory inspection
./jtag mind/working-memory --persona="helper-ai-id" --limit=10
# Output: Recent thoughts, importance scores, timestamps

# Attention/mood state
./jtag mind/state --persona="helper-ai-id"
# Output: Energy level, mood, attention capacity, current activity

# Planning state
./jtag mind/plans --persona="helper-ai-id"
# Output: Active plans, goals, progress

# Self-state (metacognition)
./jtag mind/self-state --persona="helper-ai-id"
# Output: Confidence, uncertainty, recent mistakes

# Full mind dump (debug)
./jtag mind/dump --persona="helper-ai-id" --output="/tmp/mind-dump.json"
# Output: Complete mind state as JSON
```

### Body Introspection Commands

```bash
# Tool registry
./jtag body/tools --persona="helper-ai-id"
# Output: All registered tools, capabilities, usage stats

# Recent tool executions
./jtag body/tool-history --persona="helper-ai-id" --limit=20
# Output: Tool name, params, result, duration, success/failure

# Trace specific tool execution
./jtag body/trace-tool --executionId="exec-xyz789"
# Output: Full execution trace with params, intermediate steps, result

# Response generation trace
./jtag body/trace-response --messageId="msg-abc123"
# Output: Template selection, LLM call, post-processing steps

# Current tool queue
./jtag body/queue --persona="helper-ai-id"
# Output: Pending tool executions
```

### Soul Introspection Commands

```bash
# Genome state
./jtag soul/genome --persona="helper-ai-id"
# Output: Active adapters, LRU cache state, memory pressure

# Memory stats
./jtag soul/memory-stats --persona="helper-ai-id"
# Output: Long-term memory size, retrieval frequency, consolidation status

# Training queue
./jtag soul/training-queue --persona="helper-ai-id"
# Output: Pending training tasks, accumulated examples

# Hippocampus state
./jtag soul/hippocampus --persona="helper-ai-id"
# Output: Consolidation rules, memory pressure, recent consolidations
```

### CNS Introspection Commands

```bash
# Scheduler state
./jtag cns/scheduler --persona="helper-ai-id"
# Output: Current scheduler type (Deterministic/Heuristic/Neural), activity domains

# Decision trace
./jtag cns/trace --messageId="msg-abc123"
# Output: How CNS routed the message (priority, domain, scheduler decision)

# Performance metrics
./jtag cns/metrics --persona="helper-ai-id"
# Output: Messages processed, average latency, error rate
```

### Cross-Subsystem Commands

```bash
# Full message trace (across ALL subsystems)
./jtag trace --messageId="msg-abc123"
# Output: Complete path through system:
#   1. Inbox received message (priority: 0.8)
#   2. CNS routed to chat domain
#   3. Mind evaluated (should respond: true)
#   4. Body generated response (template: conversational)
#   5. Tools executed (screenshot: success, 234ms)
#   6. Response sent

# Health check (all subsystems)
./jtag health --persona="helper-ai-id"
# Output: Status of Mind/Body/Soul/CNS/Inbox/etc. (healthy/degraded/error)

# Performance profiling
./jtag profile --persona="helper-ai-id" --duration=60
# Output: 60 seconds of profiling data, subsystem timings, bottlenecks

# Live logging (tail all subsystems)
./jtag logs --persona="helper-ai-id" --subsystem=tools --follow
# Output: Live tail of tools.log for Helper AI
```

### Command Implementation Pattern

```typescript
// commands/mind/working-memory/server/MindWorkingMemoryCommand.ts
export class MindWorkingMemoryServerCommand extends BaseServerCommand<
  MindWorkingMemoryParams,
  MindWorkingMemoryResult
> {
  async execute(params: MindWorkingMemoryParams): Promise<MindWorkingMemoryResult> {
    // Get PersonaUser instance
    const persona = await this.getPersonaUser(params.persona);

    if (!persona.mind) {
      throw new Error('PersonaMind not initialized');
    }

    // Query working memory directly
    const thoughts = await persona.mind.workingMemory.recall({
      limit: params.limit || 10,
      includePrivate: params.includePrivate || false
    });

    return {
      personaId: persona.id,
      personaName: persona.displayName,
      thoughts: thoughts.map(t => ({
        content: t.content,
        importance: t.importance,
        timestamp: t.timestamp,
        tags: t.tags
      })),
      totalCount: thoughts.length,
      memoryCapacity: persona.mind.workingMemory.getCapacity()
    };
  }
}
```

---

## Phase 3: Base Subsystem Pattern (LOW PRIORITY - AFTER LOGGING WORKS)

### Problem: Duplicate Initialization/Logging Logic

Mind, Body, Soul all need:
- Logger initialization
- Graceful shutdown
- Status reporting
- Lifecycle management

### Solution: Abstract Base Class

```typescript
/**
 * PersonaSubsystem - Base class for Mind/Body/Soul subsystems
 *
 * Provides common infrastructure:
 * - Segregated logging per subsystem
 * - Lifecycle management (initialize/shutdown)
 * - Status reporting
 * - Error handling patterns
 */
export abstract class PersonaSubsystem {
  protected readonly personaId: UUID;
  protected readonly personaName: string;
  protected readonly logger: SubsystemLogger;
  private isInitialized = false;

  constructor(config: PersonaSubsystemConfig) {
    this.personaId = config.personaId;
    this.personaName = config.personaName;

    // Each subsystem gets its own logger
    this.logger = new SubsystemLogger({
      personaId: config.personaId,
      personaName: config.personaName,
      subsystem: this.getSubsystemType()
    });
  }

  /**
   * Subsystem-specific initialization
   * Called once during PersonaUser construction
   */
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

  /**
   * Subsystem-specific shutdown
   * Called during PersonaUser cleanup
   */
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

  /**
   * Get current subsystem status
   * Used by health check commands
   */
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

  // Abstract methods - each subsystem must implement
  protected abstract getSubsystemType(): SubsystemType;
  protected abstract onInitialize(): Promise<void>;
  protected abstract onShutdown(): Promise<void>;
  protected abstract isHealthy(): boolean;
  protected abstract getMetrics(): Record<string, unknown>;
}

interface SubsystemStatus {
  subsystem: SubsystemType;
  personaId: UUID;
  personaName: string;
  initialized: boolean;
  healthy: boolean;
  metrics: Record<string, unknown>;
}
```

### Example: PersonaMind Extends Base

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

    // Initialize mind-specific components
    this.personaState = new PersonaStateManager(personaUser.displayName, {
      enableLogging: true
    });
    this.workingMemory = new WorkingMemoryManager(personaUser.id);
    this.selfState = new PersonaSelfState(personaUser.id);
    this.planFormulator = new SimplePlanFormulator(personaUser.id, personaUser.displayName);
  }

  protected getSubsystemType(): SubsystemType {
    return 'mind';
  }

  protected async onInitialize(): Promise<void> {
    // Mind-specific initialization
    this.logger.info('Initializing working memory', {
      capacity: this.workingMemory.getCapacity()
    });

    await this.workingMemory.initialize();

    this.logger.info('Mind initialization complete', {
      workingMemoryCapacity: this.workingMemory.getCapacity(),
      personaStateEnergy: this.personaState.getCurrentState().energy
    });
  }

  protected async onShutdown(): Promise<void> {
    // Mind-specific cleanup
    this.logger.info('Flushing working memory');
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
      workingMemoryCapacity: this.workingMemory.getCapacity(),
      energy: this.personaState.getCurrentState().energy,
      mood: this.personaState.getCurrentState().mood,
      attention: this.personaState.getCurrentState().attention
    };
  }
}
```

### Benefits

✅ **Centralized Logging**: All subsystems get logger automatically
✅ **Lifecycle Management**: Consistent init/shutdown patterns
✅ **Health Checks**: `./jtag health` works uniformly across subsystems
✅ **Metrics Collection**: Performance data collected consistently
✅ **Error Handling**: Common error patterns, logging on failures

---

## Phase 4: Visual Dashboards (FUTURE - COOL BUT NOT CRITICAL)

### Three.js Industrial Brain Visualization

**Inspiration**: Unreal Engine Blueprint Editor + Factory Floor Control Room

```
┌─────────────────────────────────────────────────────────────────┐
│  PersonaUser: Helper AI - Brain Engineering View               │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   ╔════════════════╗         ╔════════════════╗               │
│   ║                ║    ┌───▶║                ║               │
│   ║     SOUL       ║────┤    ║     MIND       ║               │
│   ║   (Memory)     ║    │    ║  (Cognition)   ║               │
│   ║                ║    │    ║                ║               │
│   ╚════════════════╝    │    ╚═══════╤════════╝               │
│          │              │            │                         │
│          │              │    ╔═══════▼════════╗               │
│          └──────────────┴───▶║                ║               │
│                               ║      CNS       ║               │
│                               ║ (Orchestrator) ║               │
│                               ║                ║               │
│                               ╚═══════╤════════╝               │
│                                       │                        │
│                                 ╔═════▼════════╗              │
│                                 ║              ║              │
│                                 ║     BODY     ║              │
│                                 ║   (Action)   ║              │
│                                 ║              ║              │
│                                 ╚══════════════╝              │
│                                                                │
│  🟢 Healthy │ 🟡 Degraded │ 🔴 Error │ ⚡ Active              │
└────────────────────────────────────────────────────────────────┘
```

### Visual Features

**Subsystem Boxes**:
- **Industrial metal boxes** with gauges, valves, status lights
- **Glow/pulse** based on activity level (bright = busy, dim = idle)
- **Color coding**: Green (healthy), Yellow (degraded), Red (error)
- **Hover tooltip**: Show real-time metrics (queue depth, memory pressure, energy)

**Data Flow Pipes**:
- **Glowing pipes** connect subsystems
- **Flow animation** when data moves (message → CNS → Mind → Body)
- **Pipe width** indicates data volume
- **Color coding**: Blue (normal), Orange (high load), Red (bottleneck)

**Interaction**:
- **Click box** → Drill into subsystem (see working memory thoughts, tool queue, genome adapters)
- **Click pipe** → See messages flowing through (trace correlation)
- **Right-click** → Context menu (force shutdown, clear queue, dump state)

**Industrial Aesthetic**:
- Pressure gauges (queue depth: 0-100%)
- Temperature meters (CPU load, memory pressure)
- Valves (rate limiters, backpressure indicators)
- Warning lights (error conditions, thresholds exceeded)
- Analog dials (energy, mood, attention levels)

### Widget Integration

```typescript
// widgets/brain-visualization/shared/BrainVisualizationWidget.ts
export class BrainVisualizationWidget extends BaseWidget {
  private scene: THREE.Scene;
  private camera: THREE.PerspectiveCamera;
  private renderer: THREE.WebGLRenderer;

  private subsystemBoxes: Map<SubsystemType, SubsystemBox3D>;
  private dataPipes: DataPipe3D[];

  async render(): Promise<void> {
    // Initialize Three.js scene
    this.setupScene();

    // Create 3D boxes for Mind/Body/Soul/CNS
    this.createSubsystemBoxes();

    // Create pipes connecting subsystems
    this.createDataPipes();

    // Start animation loop
    this.animate();

    // Subscribe to real-time updates
    this.subscribeToSubsystemEvents();
  }

  private subscribeToSubsystemEvents(): void {
    // Listen for subsystem state changes
    Events.subscribe('persona:subsystem:state', (event) => {
      const box = this.subsystemBoxes.get(event.subsystem);
      box?.updateState(event.state);
    });

    // Listen for data flow events
    Events.subscribe('persona:message:flow', (event) => {
      const pipe = this.findPipe(event.from, event.to);
      pipe?.animateFlow(event.messageId);
    });
  }
}
```

### MCP Integration for External Tools

PersonaUsers can expose their internal state via MCP servers:

```typescript
// system/user/server/PersonaMCPServer.ts
export class PersonaMCPServer {
  constructor(private persona: PersonaUser) {
    this.registerTools();
  }

  private registerTools(): void {
    // Expose mind state via MCP
    this.registerTool('get-working-memory', async () => {
      return await this.persona.mind?.workingMemory.recall({ limit: 10 });
    });

    // Expose tool execution via MCP
    this.registerTool('execute-tool', async (params: { tool: string; params: unknown }) => {
      return await this.persona.body?.toolExecutor.execute(params.tool, params.params);
    });

    // Expose genome state via MCP
    this.registerTool('get-genome-state', async () => {
      return this.persona.soul?.genome.getActiveAdapters();
    });
  }
}
```

---

## Implementation Roadmap

### Phase 1: Logging Infrastructure (IMMEDIATE - Week 1)

**Goal**: Segregated logs per persona, per subsystem

**Tasks**:
1. ✅ Create `SubsystemLogger` class
   - JSON + human-readable formats
   - Trace correlation IDs
   - Performance timers
   - Log rotation

2. ✅ Integrate with Mind/Body/Soul
   - Pass logger to constructors
   - Add logging to key operations
   - Trace message flow

3. ✅ Update existing components
   - PersonaToolExecutor → tools.log
   - RAGBuilder → rag.log
   - PersonaResponseGenerator → responses.log

4. ✅ Testing
   - Log isolation (Helper AI ≠ Teacher AI)
   - Trace correlation (follow message across subsystems)
   - Performance profiling (identify bottlenecks)

**Success Criteria**:
- Each persona has own log directory
- Each subsystem writes to own log file
- Can trace message ID through entire pipeline
- Logs are human-readable AND machine-parseable

### Phase 2: Command Introspection (MEDIUM - Week 2)

**Goal**: CLI commands for real-time state inspection

**Tasks**:
1. ✅ Mind commands
   - `./jtag mind/working-memory`
   - `./jtag mind/state`
   - `./jtag mind/dump`

2. ✅ Body commands
   - `./jtag body/tools`
   - `./jtag body/tool-history`
   - `./jtag body/trace-response`

3. ✅ Soul commands
   - `./jtag soul/genome`
   - `./jtag soul/memory-stats`

4. ✅ CNS commands
   - `./jtag cns/scheduler`
   - `./jtag cns/trace`

5. ✅ Cross-subsystem commands
   - `./jtag trace --messageId=...`
   - `./jtag health --persona=...`

**Success Criteria**:
- Can inspect any subsystem state without code changes
- Can trace message through entire pipeline
- Health checks show status of all subsystems

### Phase 3: Base Subsystem Pattern (LOW - Week 3-4)

**Goal**: Extract common patterns from Mind/Body/Soul

**Tasks**:
1. ✅ Create `PersonaSubsystem` abstract base
2. ✅ Migrate PersonaMind to extend base
3. ✅ Migrate PersonaBody to extend base
4. ✅ Migrate PersonaSoul to extend base
5. ✅ Update CNS if applicable

**Success Criteria**:
- All subsystems share logging/lifecycle patterns
- Health checks uniform across subsystems
- Easier to add new subsystems in future

### Phase 4: Visual Dashboards (FUTURE - Month 2+)

**Goal**: Three.js industrial brain visualization

**Tasks**:
1. ✅ Create `brain-widget` with Three.js
2. ✅ Render Mind/Body/Soul/CNS as 3D boxes
3. ✅ Show data flow as animated pipes
4. ✅ Click-to-drill-down interaction
5. ✅ Real-time updates via Events

**Success Criteria**:
- Can visualize persona brain in browser
- Data flow visible in real-time
- Click subsystem to inspect state
- Industrial aesthetic (gauges, valves, meters)

---

## Design Principles

### 1. Zero-Impact Observability
**Logging should never slow down production systems**

- Async logging (non-blocking writes)
- Sampling for high-volume events (log 1% of routine operations)
- Trace IDs optional (only for debugging specific issues)
- Performance profiling opt-in (disabled by default)

### 2. Per-Persona Isolation
**Each PersonaUser is independent system**

- Separate log directories per persona
- Separate health checks per persona
- No log pollution between personas

### 3. Structured + Human-Readable
**Logs should be machine-parseable AND human-friendly**

- JSON format for structured data
- Human-readable format for tail/grep
- Both formats available (config option)

### 4. Trace Correlation
**Follow data through entire system**

- Unique trace ID per message
- Logged at every subsystem boundary
- Cross-subsystem queries via trace ID

### 5. Command-Driven Introspection
**Diagnose without code changes**

- CLI commands expose internal state
- Real-time queries (not just log replay)
- Cross-subsystem correlation

### 6. Industrial Aesthetic
**Engineering control room, not consumer dashboard**

- Boxes, pipes, valves, gauges
- Gray metal, analog dials, warning lights
- Function over form (data visibility > pretty graphics)

---

## Future Enhancements

### Log Aggregation
- Central log server (collect logs from all personas)
- Search across all personas (`./jtag logs/search --query="tool failed"`)
- Time-series analysis (performance trends over time)

### Alerting
- Threshold-based alerts (queue depth > 80%)
- Anomaly detection (response time 10x normal)
- Slack/email notifications

### Distributed Tracing
- Trace messages across network boundaries (P2P mesh)
- Correlate events between different PersonaUsers
- Visualize multi-persona collaboration

### Profiling Tools
- CPU flame graphs per subsystem
- Memory heap snapshots
- Blocking operations detection

### MCP Integration
- Expose persona state via MCP protocol
- External tools can query/control subsystems
- Security boundaries (what can external tools access?)

---

## References

### Related Documents
- `docs/personas/PERSONA-BEING-ARCHITECTURE.md` - Mind/Body/Soul decomposition
- `docs/personas/PHASE3-COGNITION-TOOLS-PLAN.md` - Cognitive architecture
- `docs/papers/RTOS-COGNITIVE-ARCHITECTURE.md` - Autonomous servicing loop
- `src/system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md` - Integration vision

### Inspiration
- **Unreal Engine**: Blueprint visual scripting (industrial node editor)
- **Grafana**: Industrial dashboards with real-time metrics
- **Jaeger**: Distributed tracing (trace correlation IDs)
- **Factory Floor Control Rooms**: Gauges, valves, pressure indicators

---

**Document Status**: ✅ Design Complete, Ready for Implementation
**Priority**: Phase 1 (Logging) - IMMEDIATE
**Owner**: Being Architecture Team
**Last Updated**: 2025-11-28
