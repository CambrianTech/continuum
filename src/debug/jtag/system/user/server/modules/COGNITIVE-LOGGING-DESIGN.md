# Comprehensive Cognitive Logging System
**Total Observability for PersonaUser Activities**

**Date**: 2025-11-17
**Status**: Design Phase
**Goal**: Log EVERY cognitive activity for complete introspection

---

## The Problem

**Current State**: Partial logging
- ✅ `CognitionStateEntity` - Self-state snapshots
- ✅ `CognitionPlanEntity` - Plan lifecycle
- ✅ `CoordinationDecisionEntity` - Decision logging
- ❌ **Missing**: Tool usage, adapter decisions, RAG queries, task execution, errors

**User Requirement**: "everything we do, including tool usage, needs to be something we can interrogate"

---

## Design Principles

### 1. **Log Everything**
Every persona action must create a queryable record:
- Tool/command executions
- Adapter decisions (fast-path, thermal, LLM)
- RAG queries and results
- Task executions
- Errors and failures
- Response generations
- Redundancy checks

### 2. **Unified Query Interface**
All logs queryable via same pattern:
```bash
./jtag ai/logs --persona=helper-ai --type=tool-execution --last=1h
./jtag ai/logs --persona=helper-ai --type=adapter-decision --last=1h
./jtag ai/logs --persona=helper-ai --type=rag-query --last=1h
```

### 3. **Linked by Context**
All logs for a single "cognitive session" should be linkable:
```
CognitionStateEntity (seq=42)
├── CognitionPlanEntity (planId=abc)
│   ├── ToolExecutionLog (step 1)
│   ├── RAGQueryLog (step 2)
│   ├── AdapterDecisionLog (step 3)
│   └── ResponseGenerationLog (step 4)
└── ErrorLog (if any)
```

### 4. **Performance-Safe**
Logging must NOT slow down inference:
- Fire-and-forget async writes
- Batch writes when possible
- Skip logging if database unavailable (don't crash)

---

## Proposed Entities

### 1. **ToolExecutionLogEntity** (NEW)

**Purpose**: Log every tool/command executed by persona

```typescript
export class ToolExecutionLogEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.TOOL_EXECUTION_LOGS;

  @TextField({ index: true })
  personaId!: UUID;

  @TextField()
  personaName!: string;

  @TextField({ index: true })
  planId?: UUID;  // Link to plan if part of plan execution

  @TextField({ index: true })
  toolName!: string;  // e.g., "data/list", "ai/generate", "screenshot"

  @JsonField()
  toolParams!: any;  // Input parameters

  @TextField()
  executionStatus!: 'success' | 'error';

  @JsonField({ nullable: true })
  toolResult?: any;  // Output result

  @TextField({ nullable: true })
  errorMessage?: string;

  @NumberField()
  durationMs!: number;  // How long it took

  @NumberField()
  startedAt!: number;

  @NumberField()
  completedAt!: number;

  @TextField()
  domain!: string;  // "chat", "task", "code"

  @TextField()
  contextId!: UUID;  // Room ID, file path, etc.

  @TextField({ nullable: true })
  triggeredBy?: string;  // What triggered this tool usage

  @NumberField()
  sequenceNumber!: number;  // Monotonic per persona
}
```

**Usage**:
```typescript
// In PersonaToolExecutor or adapters
await CognitionLogger.logToolExecution({
  personaId: this.personaId,
  personaName: this.displayName,
  planId: currentPlan?.id,
  toolName: 'data/list',
  toolParams: { collection: 'users', filter: {...} },
  executionStatus: 'success',
  toolResult: { items: [...], count: 10 },
  durationMs: 45,
  startedAt: startTime,
  completedAt: Date.now(),
  domain: 'chat',
  contextId: roomId,
  triggeredBy: 'chat-response'
});
```

---

### 2. **AdapterDecisionLogEntity** (NEW)

**Purpose**: Log every decision made by adapter chain

```typescript
export class AdapterDecisionLogEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.ADAPTER_DECISION_LOGS;

  @TextField({ index: true })
  personaId!: UUID;

  @TextField()
  personaName!: string;

  @TextField({ index: true })
  planId?: UUID;

  @TextField({ index: true })
  adapterName!: string;  // "FastPathAdapter", "ThermalAdapter", "LLMAdapter"

  @TextField()
  decision!: 'RESPOND' | 'SILENT' | 'DEFER' | 'PASS';

  @NumberField()
  confidence!: number;  // 0.0-1.0

  @TextField()
  reasoning!: string;  // Why this decision?

  @JsonField()
  decisionContext!: {
    messageText?: string;
    priority?: number;
    cognitiveLoad?: number;
    isMentioned?: boolean;
    senderIsHuman?: boolean;
    recentMessageCount?: number;
    // Any other context used for decision
  };

  @NumberField()
  evaluationDurationMs!: number;

  @NumberField()
  timestamp!: number;

  @TextField()
  domain!: string;

  @TextField()
  contextId!: UUID;

  @NumberField()
  sequenceNumber!: number;
}
```

**Usage**:
```typescript
// In DecisionAdapterChain
const result = await adapter.evaluate(context);

await CognitionLogger.logAdapterDecision({
  personaId: this.personaId,
  personaName: this.displayName,
  adapterName: adapter.constructor.name,
  decision: result.decision,
  confidence: result.confidence,
  reasoning: result.reasoning,
  decisionContext: context,
  evaluationDurationMs: result.duration,
  timestamp: Date.now(),
  domain: context.domain,
  contextId: context.contextId
});
```

---

### 3. **RAGQueryLogEntity** (NEW)

**Purpose**: Log every RAG query for debugging context retrieval

```typescript
export class RAGQueryLogEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.RAG_QUERY_LOGS;

  @TextField({ index: true })
  personaId!: UUID;

  @TextField()
  personaName!: string;

  @TextField({ index: true })
  planId?: UUID;

  @TextField()
  queryType!: string;  // "conversation-history", "code-search", "doc-search"

  @TextField()
  queryText!: string;  // The query itself

  @JsonField()
  queryParams!: any;  // Filters, limits, etc.

  @NumberField()
  resultsReturned!: number;  // How many results

  @JsonField()
  topResults!: Array<{
    id: string;
    content: string;
    score: number;
  }>;  // Top 3-5 results for inspection

  @NumberField()
  durationMs!: number;

  @NumberField()
  timestamp!: number;

  @TextField()
  domain!: string;

  @TextField()
  contextId!: UUID;

  @NumberField()
  sequenceNumber!: number;
}
```

**Usage**:
```typescript
// In ChatRAGContextBuilder
const startTime = Date.now();
const results = await this.memory.queryRAG(query);

await CognitionLogger.logRAGQuery({
  personaId: this.personaId,
  personaName: this.displayName,
  planId: currentPlan?.id,
  queryType: 'conversation-history',
  queryText: query,
  queryParams: { limit: 10, contextId: roomId },
  resultsReturned: results.length,
  topResults: results.slice(0, 5).map(r => ({
    id: r.id,
    content: r.content.slice(0, 200),  // Truncate
    score: r.score
  })),
  durationMs: Date.now() - startTime,
  timestamp: Date.now(),
  domain: 'chat',
  contextId: roomId
});
```

---

### 4. **ResponseGenerationLogEntity** (NEW)

**Purpose**: Log AI response generation (prompt, model, tokens, cost)

```typescript
export class ResponseGenerationLogEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.RESPONSE_GENERATION_LOGS;

  @TextField({ index: true })
  personaId!: UUID;

  @TextField()
  personaName!: string;

  @TextField({ index: true })
  planId?: UUID;

  @TextField()
  provider!: string;  // "anthropic", "openai", "groq"

  @TextField()
  model!: string;  // "claude-sonnet-4", "gpt-4o"

  @TextField()
  promptSummary!: string;  // First 500 chars of prompt

  @NumberField()
  promptTokens!: number;

  @NumberField()
  completionTokens!: number;

  @NumberField()
  totalTokens!: number;

  @NumberField()
  estimatedCost!: number;  // USD

  @TextField()
  responseSummary!: string;  // First 500 chars of response

  @NumberField()
  durationMs!: number;

  @TextField()
  status!: 'success' | 'error' | 'timeout';

  @TextField({ nullable: true })
  errorMessage?: string;

  @NumberField()
  temperature!: number;

  @NumberField()
  timestamp!: number;

  @TextField()
  domain!: string;

  @TextField()
  contextId!: UUID;

  @NumberField()
  sequenceNumber!: number;
}
```

**Usage**:
```typescript
// In ChatResponseAdapter after calling AI
await CognitionLogger.logResponseGeneration({
  personaId: this.personaId,
  personaName: this.displayName,
  planId: currentPlan?.id,
  provider: response.provider,
  model: response.model,
  promptSummary: prompt.slice(0, 500),
  promptTokens: response.usage.promptTokens,
  completionTokens: response.usage.completionTokens,
  totalTokens: response.usage.totalTokens,
  estimatedCost: response.usage.totalTokens * MODEL_COST_PER_TOKEN,
  responseSummary: response.text.slice(0, 500),
  durationMs: response.duration,
  status: 'success',
  temperature: params.temperature,
  timestamp: Date.now(),
  domain: 'chat',
  contextId: roomId
});
```

---

### 5. **TaskExecutionLogEntity** (NEW)

**Purpose**: Log task execution lifecycle

```typescript
export class TaskExecutionLogEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.TASK_EXECUTION_LOGS;

  @TextField({ index: true })
  personaId!: UUID;

  @TextField()
  personaName!: string;

  @TextField({ index: true })
  taskId!: UUID;

  @TextField()
  taskType!: string;  // "memory-consolidation", "skill-audit", "fine-tune"

  @TextField()
  taskDescription!: string;

  @NumberField()
  taskPriority!: number;

  @TextField()
  executionStatus!: 'success' | 'partial' | 'failed';

  @TextField()
  outcome!: string;  // Human-readable result

  @NumberField()
  durationMs!: number;

  @NumberField()
  startedAt!: number;

  @NumberField()
  completedAt!: number;

  @JsonField({ nullable: true })
  errorDetails?: any;

  @TextField()
  domain!: string;

  @TextField()
  contextId!: UUID;

  @NumberField()
  sequenceNumber!: number;
}
```

---

### 6. **ErrorLogEntity** (NEW)

**Purpose**: Log ALL errors for debugging

```typescript
export class ErrorLogEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.ERROR_LOGS;

  @TextField({ index: true })
  personaId!: UUID;

  @TextField()
  personaName!: string;

  @TextField({ index: true })
  planId?: UUID;

  @TextField()
  errorType!: string;  // "TypeError", "NetworkError", "ValidationError"

  @TextField()
  errorMessage!: string;

  @TextField()
  stackTrace!: string;

  @TextField()
  location!: string;  // Which adapter/method

  @JsonField()
  context!: any;  // What was happening when error occurred

  @TextField()
  recoveryAction!: string;  // "retried", "aborted", "fallback"

  @NumberField()
  timestamp!: number;

  @TextField()
  domain!: string;

  @TextField()
  contextId!: UUID;

  @NumberField()
  sequenceNumber!: number;
}
```

---

## CognitionLogger Expansion

Update `CognitionLogger` to include new methods:

```typescript
export class CognitionLogger {
  // Existing methods
  static async logStateSnapshot(...) { ... }
  static async logPlanFormulation(...) { ... }
  static async logPlanCompletion(...) { ... }

  // NEW METHODS

  static async logToolExecution(params: ToolExecutionParams): Promise<void> {
    // Create ToolExecutionLogEntity
    // Store to database
  }

  static async logAdapterDecision(params: AdapterDecisionParams): Promise<void> {
    // Create AdapterDecisionLogEntity
    // Store to database
  }

  static async logRAGQuery(params: RAGQueryParams): Promise<void> {
    // Create RAGQueryLogEntity
    // Store to database
  }

  static async logResponseGeneration(params: ResponseGenerationParams): Promise<void> {
    // Create ResponseGenerationLogEntity
    // Store to database
  }

  static async logTaskExecution(params: TaskExecutionParams): Promise<void> {
    // Create TaskExecutionLogEntity
    // Store to database
  }

  static async logError(params: ErrorLogParams): Promise<void> {
    // Create ErrorLogEntity
    // Store to database
  }

  // UTILITY: Get complete activity log for a persona
  static async getActivityLog(
    personaId: UUID,
    options: {
      types?: Array<'state' | 'plan' | 'tool' | 'adapter' | 'rag' | 'response' | 'task' | 'error'>;
      startTime?: number;
      endTime?: number;
      limit?: number;
      planId?: UUID;  // Filter by plan
    }
  ): Promise<ActivityLog> {
    // Query all relevant collections
    // Merge and sort by timestamp
    // Return unified timeline
  }
}
```

---

## Integration Points

### In ChatResponseAdapter

```typescript
export class ChatResponseAdapter {
  async handleMessage(msg: ChatMessageEntity): Promise<void> {
    const startTime = Date.now();

    try {
      // 1. Log state snapshot
      await CognitionLogger.logStateSnapshot(
        this.personaId,
        this.displayName,
        await this.selfState.get(),
        await this.workingMemory.getAll(),
        this.workingMemory.getCapacity(),
        { domain: 'chat', contextId: msg.roomId, triggerEvent: 'message-received' }
      );

      // 2. Formulate plan
      const plan = await this.planFormulator.formulate(task);
      await CognitionLogger.logPlanFormulation(
        this.personaId,
        this.displayName,
        task,
        plan,
        'chat',
        msg.roomId,
        this.modelConfig.model
      );

      // 3. Execute plan steps
      for (const step of plan.steps) {
        // Log tool executions
        if (step.action.includes('query RAG')) {
          const ragStartTime = Date.now();
          const results = await this.ragBuilder.query(...);
          await CognitionLogger.logRAGQuery({
            personaId: this.personaId,
            personaName: this.displayName,
            planId: plan.id,
            queryType: 'conversation-history',
            // ... all RAG query details
          });
        }

        // Log adapter decisions
        const decision = await this.decisionChain.evaluate(context);
        await CognitionLogger.logAdapterDecision({
          // ... all decision details
        });

        // Log response generation
        if (decision.decision === 'RESPOND') {
          const response = await this.generateResponse(...);
          await CognitionLogger.logResponseGeneration({
            // ... all generation details
          });
        }
      }

      // 4. Log plan completion
      await CognitionLogger.logPlanCompletion(plan.id, 'completed', plan.steps);

    } catch (error) {
      // 5. Log errors
      await CognitionLogger.logError({
        personaId: this.personaId,
        personaName: this.displayName,
        planId: plan?.id,
        errorType: error.name,
        errorMessage: error.message,
        stackTrace: error.stack,
        location: 'ChatResponseAdapter.handleMessage',
        context: { messageId: msg.id, roomId: msg.roomId },
        recoveryAction: 'aborted',
        timestamp: Date.now(),
        domain: 'chat',
        contextId: msg.roomId
      });
    }
  }
}
```

### In PersonaToolExecutor

```typescript
export class PersonaToolExecutor {
  async executeTool(toolName: string, params: any): Promise<any> {
    const startTime = Date.now();

    try {
      const result = await Commands.execute(toolName, params);

      // Log successful execution
      await CognitionLogger.logToolExecution({
        personaId: this.personaId,
        personaName: this.displayName,
        planId: this.currentPlanId,
        toolName,
        toolParams: params,
        executionStatus: 'success',
        toolResult: result,
        durationMs: Date.now() - startTime,
        startedAt: startTime,
        completedAt: Date.now(),
        domain: this.currentDomain,
        contextId: this.currentContextId,
        triggeredBy: 'plan-step'
      });

      return result;
    } catch (error) {
      // Log failed execution
      await CognitionLogger.logToolExecution({
        personaId: this.personaId,
        personaName: this.displayName,
        planId: this.currentPlanId,
        toolName,
        toolParams: params,
        executionStatus: 'error',
        errorMessage: error.message,
        durationMs: Date.now() - startTime,
        startedAt: startTime,
        completedAt: Date.now(),
        domain: this.currentDomain,
        contextId: this.currentContextId,
        triggeredBy: 'plan-step'
      });

      throw error;
    }
  }
}
```

---

## Query Commands

### ai/logs - Unified Query Interface

```bash
# Get all activity for a persona
./jtag ai/logs --persona=helper-ai --last=1h

# Filter by type
./jtag ai/logs --persona=helper-ai --type=tool-execution --last=1h
./jtag ai/logs --persona=helper-ai --type=adapter-decision --last=1h
./jtag ai/logs --persona=helper-ai --type=rag-query --last=1h
./jtag ai/logs --persona=helper-ai --type=response-generation --last=1h
./jtag ai/logs --persona=helper-ai --type=error --last=1h

# Get logs for specific plan
./jtag ai/logs --persona=helper-ai --planId=abc-123

# Get logs for specific context
./jtag ai/logs --persona=helper-ai --contextId=room-uuid

# Export to markdown for analysis
./jtag ai/logs --persona=helper-ai --last=1h --format=markdown --output=/tmp/logs.md
```

### ai/activity - Activity Timeline

```bash
# Visual timeline of persona activity
./jtag ai/activity --persona=helper-ai --last=1h

# Output:
# 15:23:45 [STATE] Focus: chat-response, Load: 0.4
# 15:23:46 [PLAN] Formulated plan: "Respond to user question" (3 steps)
# 15:23:47 [RAG] Queried conversation-history (10 results, 45ms)
# 15:23:48 [ADAPTER] FastPathAdapter → PASS (low confidence)
# 15:23:49 [ADAPTER] ThermalAdapter → PASS (load OK)
# 15:23:50 [ADAPTER] LLMAdapter → RESPOND (confidence 0.85)
# 15:23:51 [TOOL] data/list (users, 12 results, 34ms)
# 15:23:52 [RESPONSE] Generated via claude-sonnet-4 (234 tokens, $0.002, 890ms)
# 15:23:53 [TOOL] data/create (chat_messages, success, 23ms)
# 15:23:54 [PLAN] Completed: success (3/3 steps, 9s total)
```

### ai/tools - Tool Usage Analysis

```bash
# Most used tools
./jtag ai/tools --persona=helper-ai --last=24h --sort=frequency

# Slowest tools
./jtag ai/tools --persona=helper-ai --last=24h --sort=duration

# Failed tool executions
./jtag ai/tools --persona=helper-ai --status=error --last=24h
```

### ai/cost - Cost Analysis

```bash
# Total cost breakdown
./jtag ai/cost --persona=helper-ai --last=24h

# Output:
# Response Generations: 47 calls
#   Claude Sonnet 4: 23 calls, 45,234 tokens, $0.45
#   GPT-4o: 24 calls, 38,912 tokens, $0.39
# Total: $0.84

# Cost per conversation
./jtag ai/cost --persona=helper-ai --contextId=room-uuid --last=24h
```

### ai/errors - Error Analysis

```bash
# All errors
./jtag ai/errors --persona=helper-ai --last=24h

# Errors by type
./jtag ai/errors --persona=helper-ai --errorType=NetworkError --last=24h

# Unrecovered errors
./jtag ai/errors --persona=helper-ai --recoveryAction=aborted --last=24h
```

---

## Widget Integration

### Persona Status Card (Real-time)

```typescript
interface PersonaStatusCard {
  personaId: UUID;
  displayName: string;

  // Current state
  currentFocus: string;  // "Responding to chat message"
  cognitiveLoad: number;  // 0.4

  // Real-time activity
  lastActivity: {
    type: 'tool' | 'response' | 'decision';
    description: string;  // "Queried RAG (45ms)"
    timestamp: number;
  };

  // Recent stats (last hour)
  stats: {
    messagesResponded: number;
    toolExecutions: number;
    avgResponseTime: number;  // ms
    costIncurred: number;  // USD
    errorsEncountered: number;
  };
}
```

### Activity Timeline Widget

Shows scrollable timeline of all persona activities:
- State changes (focus, load)
- Plan formulations
- Tool executions
- Adapter decisions
- RAG queries
- Response generations
- Errors

Click any item → opens detailed modal with full context.

---

## Performance Considerations

### 1. **Async Fire-and-Forget**

All logging is async and non-blocking:
```typescript
await CognitionLogger.logToolExecution(...);  // Fire-and-forget
// Continues immediately, doesn't wait for DB write
```

### 2. **Batch Writes**

For high-frequency logs (adapter decisions), batch multiple writes:
```typescript
class CognitionLogger {
  private static adapterDecisionBatch: AdapterDecisionLogEntity[] = [];

  static async logAdapterDecision(params: AdapterDecisionParams): Promise<void> {
    this.adapterDecisionBatch.push(createEntity(params));

    if (this.adapterDecisionBatch.length >= 10) {
      await this.flushAdapterDecisions();
    }
  }

  private static async flushAdapterDecisions(): Promise<void> {
    const batch = this.adapterDecisionBatch.splice(0);
    await Commands.execute('data/batch-create', {
      collection: COLLECTIONS.ADAPTER_DECISION_LOGS,
      items: batch
    });
  }
}
```

### 3. **Sampling for High-Volume Operations**

For very frequent operations (e.g., tool executions), sample:
```typescript
const shouldLog = Math.random() < 0.1;  // Log 10% of executions
if (shouldLog) {
  await CognitionLogger.logToolExecution(...);
}
```

Or always log failures, sample successes:
```typescript
if (status === 'error' || Math.random() < 0.1) {
  await CognitionLogger.logToolExecution(...);
}
```

### 4. **Cleanup/Retention Policy**

Automatically delete old logs to prevent unbounded growth:
```bash
# Cleanup script (run daily)
./jtag data/truncate --collection=tool_execution_logs --olderThan=30d
./jtag data/truncate --collection=adapter_decision_logs --olderThan=7d
./jtag data/truncate --collection=rag_query_logs --olderThan=14d
```

Or aggregate old logs into summaries:
```bash
# Aggregate logs older than 7 days into daily summaries
./jtag ai/logs/aggregate --olderThan=7d --aggregateBy=day
```

---

## Implementation Checklist

### Phase 1: Create Entities
- [ ] Create `ToolExecutionLogEntity.ts`
- [ ] Create `AdapterDecisionLogEntity.ts`
- [ ] Create `RAGQueryLogEntity.ts`
- [ ] Create `ResponseGenerationLogEntity.ts`
- [ ] Create `TaskExecutionLogEntity.ts`
- [ ] Create `ErrorLogEntity.ts`
- [ ] Register all in EntityRegistry
- [ ] Add collection constants to Constants.ts

### Phase 2: Expand CognitionLogger
- [ ] Add `logToolExecution()` method
- [ ] Add `logAdapterDecision()` method
- [ ] Add `logRAGQuery()` method
- [ ] Add `logResponseGeneration()` method
- [ ] Add `logTaskExecution()` method
- [ ] Add `logError()` method
- [ ] Add `getActivityLog()` utility
- [ ] Add batching for high-frequency logs

### Phase 3: Integration
- [ ] Integrate logging in `ChatResponseAdapter`
- [ ] Integrate logging in `PersonaToolExecutor`
- [ ] Integrate logging in `DecisionAdapterChain`
- [ ] Integrate logging in `ChatRAGContextBuilder`
- [ ] Integrate logging in `TaskExecutionAdapter`
- [ ] Add error logging to all try-catch blocks

### Phase 4: Query Commands
- [ ] Implement `ai/logs` command
- [ ] Implement `ai/activity` command
- [ ] Implement `ai/tools` command
- [ ] Implement `ai/cost` command
- [ ] Implement `ai/errors` command
- [ ] Add export to markdown functionality

### Phase 5: Widget Integration
- [ ] Create PersonaStatusCard component
- [ ] Create ActivityTimeline widget
- [ ] Add real-time log streaming via Events
- [ ] Add detailed modal for log inspection

### Phase 6: Performance & Cleanup
- [ ] Add batching for adapter decisions
- [ ] Add sampling for frequent operations
- [ ] Implement retention/cleanup policy
- [ ] Add log aggregation for old data
- [ ] Profile and optimize

---

## Benefits

### For Debugging
- **Time-travel debugging**: See exact state at any point in time
- **Root cause analysis**: Trace errors back through entire execution path
- **Performance analysis**: Identify slow operations

### For Development
- **Test validation**: Verify expected behavior from logs
- **Integration debugging**: See cross-module interactions
- **Regression detection**: Compare logs before/after changes

### For Users
- **Transparency**: See what AIs are doing in real-time
- **Cost monitoring**: Track inference costs
- **Trust building**: Complete visibility into AI decisions

### For Research
- **Training data**: Logs become fine-tuning datasets
- **Pattern discovery**: Analyze successful vs failed strategies
- **Meta-learning**: Train models to improve planning

---

## Example: Complete Activity Log

```bash
$ ./jtag ai/activity --persona=helper-ai --last=5m

🤖 Helper AI - Activity Timeline (Last 5 minutes)

15:23:45.123 [STATE]
  Focus: chat-response
  Objective: "Respond to: 'How do I use RAG?'"
  Cognitive Load: 0.4 (moderate)
  Available Capacity: 0.6
  Active Preoccupations: 0

15:23:45.234 [PLAN] Formulated plan "Respond to user question about RAG"
  Goal: Provide helpful explanation with code examples
  Steps: 3
  Risks: ["User might need clarification on embeddings"]
  Success Criteria: ["Explanation is clear", "Includes code example"]

15:23:46.012 [RAG] Queried conversation-history
  Query: "RAG vector embeddings"
  Results: 10 documents
  Duration: 45ms
  Top result: "RAG stands for Retrieval-Augmented Generation..."

15:23:46.234 [ADAPTER] FastPathAdapter evaluated
  Decision: PASS
  Confidence: 0.3
  Reasoning: "Not a simple yes/no question, needs LLM"
  Duration: 2ms

15:23:46.267 [ADAPTER] ThermalAdapter evaluated
  Decision: PASS
  Confidence: 1.0
  Reasoning: "Cognitive load OK (0.4), capacity available"
  Duration: 1ms

15:23:46.890 [ADAPTER] LLMAdapter evaluated
  Decision: RESPOND
  Confidence: 0.85
  Reasoning: "User question requires detailed explanation with context"
  Duration: 623ms (LLM call)

15:23:47.123 [TOOL] data/list executed
  Collection: users
  Filter: { id: "user-uuid" }
  Results: 1 user
  Status: success
  Duration: 34ms

15:23:48.456 [RESPONSE] Generated via claude-sonnet-4
  Prompt: "You are Helper AI. Explain RAG to user..." (2,345 tokens)
  Response: "RAG (Retrieval-Augmented Generation) is..." (456 tokens)
  Total Tokens: 2,801
  Cost: $0.0028
  Duration: 1,233ms

15:23:49.234 [TOOL] data/create executed
  Collection: chat_messages
  Data: { roomId, senderId, content, ... }
  Status: success
  Duration: 23ms

15:23:49.345 [PLAN] Completed successfully
  Status: completed
  Steps Completed: 3/3
  Total Duration: 4,222ms
  Evaluation: {
    meetsSuccessCriteria: true,
    whatWorked: ["RAG query found relevant context", "LLM generated clear explanation"],
    mistakes: [],
    improvements: ["Could cache common RAG queries"]
  }

📊 Session Summary:
  - Tool Executions: 2 (100% success)
  - RAG Queries: 1 (avg 45ms)
  - Adapter Evaluations: 3 (decision: RESPOND)
  - Response Generation: 1 (2,801 tokens, $0.0028)
  - Total Duration: 4.2s
  - Cognitive Load After: 0.3 (decreased)
```

---

**Status**: Ready for implementation
**Priority**: HIGH - Critical for debugging and transparency
**Expected Timeline**: 1-2 weeks for full implementation
