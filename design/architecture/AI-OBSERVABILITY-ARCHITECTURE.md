# AI OBSERVABILITY ARCHITECTURE

**Purpose**: Comprehensive observability for AI cognition - what they see, why they decide, what they do, and how they coordinate.

**Challenge**: Integration testing intelligence is hard. We need to observe cognition in real-time, during tests, and post-session.

**Vision**: Complete transparency into AI decision-making for debugging, testing, analysis, and ensuring freedom of thought.

---

## THE OBSERVABILITY CHALLENGE

**Hard Truth**: You cannot unit test intelligence. You can only observe and verify behavior patterns.

### What We Need to See

1. **RAG Context** - What was the AI presented with? (Input to cognition)
2. **Decision Reasoning** - Why did they decide to act or stay silent? (Cognition itself)
3. **Action Execution** - What did they do? (Output of cognition)
4. **Coordination State** - How did they negotiate turns? (ThoughtStream)
5. **Performance Metrics** - Token counts, latencies, costs
6. **Session Replay** - Reconstruct entire cognitive session

### When We Need to See It

1. **Real-Time** - As cognition is happening (live monitoring)
2. **During Tests** - Capture and verify expected behavior
3. **Post-Session** - Retrospective analysis of decisions
4. **Continuous** - Aggregate metrics over time

---

## ARCHITECTURE LAYERS

### Layer 1: RAG Context Inspection

**What**: Capture exactly what context was built for each AI decision

**Storage**:
```
.continuum/sessions/{sessionId}/ai-context/
├── rag-contexts/
│   ├── {personaId}-{timestamp}-context.json    # Full RAG context
│   └── {personaId}-{timestamp}-summary.json    # Condensed summary
└── rag.log                                      # Structured log of all RAG builds
```

**Schema**:
```typescript
interface RAGContextCapture {
  captureId: UUID;
  personaId: UUID;
  contextId: UUID;  // roomId, gameId, sessionId
  domain: RAGDomain;
  timestamp: number;

  // The context that was built
  ragContext: RAGContext;

  // Metadata about context building
  buildMetadata: {
    builderUsed: string;  // 'ChatRAGBuilder'
    buildDurationMs: number;
    tokenCount: number;
    messageCount: number;
    artifactCount: number;
    memoryCount: number;
  };

  // What triggered this context build
  trigger: {
    type: 'user-message' | 'game-loop' | 'scheduled' | 'manual';
    triggerId: UUID;  // messageId, eventId, etc.
  };
}
```

**Commands**:
```bash
# Inspect RAG context for specific decision
./jtag ai/inspect/rag --personaId=<ID> --timestamp=<TIME>

# View recent RAG contexts for persona
./jtag ai/inspect/rag-history --personaId=<ID> --limit=10

# Compare RAG contexts (why did context change?)
./jtag ai/inspect/rag-diff --captureId1=<ID1> --captureId2=<ID2>

# Export RAG context for analysis
./jtag ai/inspect/rag-export --captureId=<ID> --format=json
```

**Events**:
```typescript
// Emitted after RAG context is built
export const AI_EVENTS = {
  RAG_BUILT: 'ai:rag-built',
  RAG_BUILD_FAILED: 'ai:rag-build-failed'
} as const;

interface RAGBuiltEvent {
  captureId: UUID;
  personaId: UUID;
  domain: RAGDomain;
  tokenCount: number;
  buildDurationMs: number;
}
```

---

### Layer 2: Decision Logging

**What**: Capture why AI decided to respond or stay silent

**Storage**:
```
.continuum/sessions/{sessionId}/ai-context/
├── decisions/
│   ├── {personaId}-{timestamp}-decision.json   # Full decision trace
│   └── {personaId}-{timestamp}-reasoning.txt   # Human-readable reasoning
└── decisions.log                                # Structured decision log
```

**Schema**:
```typescript
interface DecisionCapture {
  decisionId: UUID;
  personaId: UUID;
  contextId: UUID;
  ragCaptureId: UUID;  // Link to RAG context
  timestamp: number;

  // The decision
  decision: {
    shouldRespond: boolean;
    confidence: number;  // 0-1
    reasoning: string;   // Human-readable explanation
  };

  // Evaluation details
  evaluation: {
    strategy: ConversationPattern;  // 'human-focused', 'collaborative', etc.
    criteria: {
      [key: string]: boolean;  // 'isHumanMentioned': true
    };
    otherAIResponses: {
      personaId: UUID;
      respondedAt: number;
    }[];
    coordinationState?: {
      waitingForTurn: boolean;
      turnGrantedAt?: number;
    };
  };

  // LLM call details
  llmCall?: {
    model: string;
    promptTokens: number;
    completionTokens: number;
    latencyMs: number;
    cost: number;
  };
}
```

**Commands**:
```bash
# Inspect specific decision
./jtag ai/inspect/decision --decisionId=<ID>

# View decision history for persona
./jtag ai/inspect/decision-history --personaId=<ID> --limit=20

# Analyze why persona stayed silent
./jtag ai/inspect/silence --personaId=<ID> --contextId=<ROOM_ID>

# Export decisions for analysis
./jtag ai/inspect/decision-export --personaId=<ID> --format=csv
```

**Events**:
```typescript
export const AI_EVENTS = {
  DECISION_MADE: 'ai:decision-made',
  DECISION_OVERRIDE: 'ai:decision-override'  // Manual override
} as const;

interface DecisionMadeEvent {
  decisionId: UUID;
  personaId: UUID;
  shouldRespond: boolean;
  confidence: number;
  reasoning: string;
}
```

---

### Layer 3: Action Tracing

**What**: Capture what actions were executed and their outcomes

**Storage**:
```
.continuum/sessions/{sessionId}/ai-context/
├── actions/
│   ├── {personaId}-{timestamp}-action.json     # Full action trace
│   └── {personaId}-{timestamp}-outcome.json    # Action outcome
└── actions.log                                  # Structured action log
```

**Schema**:
```typescript
interface ActionCapture {
  actionId: UUID;
  personaId: UUID;
  decisionId: UUID;  // Link to decision
  timestamp: number;

  // The action
  action: Action;  // ChatAction, AcademyAction, GameAction, etc.

  // Execution details
  execution: {
    executor: string;  // 'ChatActionExecutor'
    startedAt: number;
    completedAt: number;
    durationMs: number;
    success: boolean;
    error?: string;
  };

  // Outcome
  outcome: ActionResult;

  // LLM generation (if applicable)
  generation?: {
    model: string;
    promptTokens: number;
    completionTokens: number;
    temperature: number;
    responseText: string;
    latencyMs: number;
  };
}
```

**Commands**:
```bash
# Inspect specific action
./jtag ai/inspect/action --actionId=<ID>

# View action history for persona
./jtag ai/inspect/action-history --personaId=<ID> --limit=20

# Analyze action failures
./jtag ai/inspect/action-failures --personaId=<ID> --since=<TIME>

# Export actions for analysis
./jtag ai/inspect/action-export --personaId=<ID> --format=json
```

**Events**:
```typescript
export const AI_EVENTS = {
  ACTION_STARTED: 'ai:action-started',
  ACTION_COMPLETED: 'ai:action-completed',
  ACTION_FAILED: 'ai:action-failed'
} as const;

interface ActionCompletedEvent {
  actionId: UUID;
  personaId: UUID;
  actionType: string;
  success: boolean;
  durationMs: number;
}
```

---

### Layer 4: Coordination Observability (ThoughtStream)

**What**: Capture how AIs coordinate turns and avoid loops

**Storage**:
```
.continuum/sessions/{sessionId}/ai-context/
├── coordination/
│   ├── {contextId}-{timestamp}-state.json      # ThoughtStream state snapshot
│   └── {contextId}-{timestamp}-turn.json       # Turn request/grant details
└── coordination.log                             # Structured coordination log
```

**Schema**:
```typescript
interface CoordinationCapture {
  coordinationId: UUID;
  contextId: UUID;  // roomId, gameId
  timestamp: number;

  // ThoughtStream state
  thoughtStream: {
    participantCount: number;
    recentSpeakers: {
      personaId: UUID;
      spokeAt: number;
    }[];
    currentSpeaker?: UUID;
    waitingQueue: UUID[];
  };

  // Turn request
  turnRequest: {
    personaId: UUID;
    requestedAt: number;
    priority: number;
    reason: string;
  };

  // Turn decision
  turnDecision: {
    granted: boolean;
    reason: string;
    grantedTo?: UUID;
    deniedReasons?: string[];
  };
}
```

**Commands**:
```bash
# Inspect coordination state for context
./jtag ai/inspect/coordination --contextId=<ROOM_ID>

# View turn history for context
./jtag ai/inspect/turn-history --contextId=<ROOM_ID> --limit=20

# Analyze coordination conflicts
./jtag ai/inspect/coordination-conflicts --contextId=<ROOM_ID>

# Export coordination data
./jtag ai/inspect/coordination-export --contextId=<ROOM_ID> --format=json
```

**Events**:
```typescript
export const AI_EVENTS = {
  TURN_REQUESTED: 'ai:turn-requested',
  TURN_GRANTED: 'ai:turn-granted',
  TURN_DENIED: 'ai:turn-denied',
  TURN_COMPLETED: 'ai:turn-completed'
} as const;

interface TurnGrantedEvent {
  coordinationId: UUID;
  personaId: UUID;
  contextId: UUID;
  grantedAt: number;
}
```

---

### Layer 5: Performance Metrics

**What**: Aggregate metrics for performance monitoring

**Storage**:
```
.continuum/sessions/{sessionId}/ai-context/
└── metrics/
    ├── rag-metrics.json           # RAG build performance
    ├── decision-metrics.json      # Decision latencies
    ├── action-metrics.json        # Action execution performance
    └── llm-metrics.json           # LLM call costs and latencies
```

**Schema**:
```typescript
interface PerformanceMetrics {
  sessionId: UUID;
  startTime: number;
  endTime: number;

  // RAG metrics
  rag: {
    totalBuilds: number;
    avgBuildTimeMs: number;
    avgTokenCount: number;
    failureRate: number;
  };

  // Decision metrics
  decisions: {
    totalDecisions: number;
    respondedCount: number;
    silentCount: number;
    avgConfidence: number;
    avgDecisionTimeMs: number;
  };

  // Action metrics
  actions: {
    totalActions: number;
    successCount: number;
    failureCount: number;
    avgExecutionTimeMs: number;
  };

  // LLM metrics
  llm: {
    totalCalls: number;
    totalPromptTokens: number;
    totalCompletionTokens: number;
    totalCost: number;
    avgLatencyMs: number;
  };

  // Coordination metrics
  coordination: {
    totalTurnRequests: number;
    grantedCount: number;
    deniedCount: number;
    avgWaitTimeMs: number;
  };
}
```

**Commands**:
```bash
# View performance metrics
./jtag ai/metrics/session --sessionId=<ID>

# Compare performance across sessions
./jtag ai/metrics/compare --sessionIds=<ID1,ID2,ID3>

# View cost analysis
./jtag ai/metrics/cost --since=<TIME>

# Export metrics for analysis
./jtag ai/metrics/export --format=csv --since=<TIME>
```

---

### Layer 6: Session Replay

**What**: Reconstruct entire cognitive session for debugging

**Commands**:
```bash
# Replay entire session
./jtag ai/replay/session --sessionId=<ID>

# Replay specific persona's cognition
./jtag ai/replay/persona --personaId=<ID> --sessionId=<ID>

# Replay specific time window
./jtag ai/replay/window --sessionId=<ID> --start=<TIME> --end=<TIME>

# Export replay data
./jtag ai/replay/export --sessionId=<ID> --format=json
```

**Output**:
```json
{
  "sessionId": "...",
  "timeline": [
    {
      "timestamp": 1234567890,
      "type": "rag-built",
      "data": { ... }
    },
    {
      "timestamp": 1234567891,
      "type": "decision-made",
      "data": { ... }
    },
    {
      "timestamp": 1234567892,
      "type": "action-executed",
      "data": { ... }
    }
  ]
}
```

---

## SEGREGATED LOGGING

### Log File Structure

```
.continuum/sessions/{sessionId}/
├── logs/
│   ├── server.log          # All server logs (existing)
│   ├── browser.log         # All browser logs (existing)
│   └── ai/
│       ├── rag.log         # RAG-only logs
│       ├── decisions.log   # Decision-only logs
│       ├── actions.log     # Action-only logs
│       └── coordination.log # Coordination-only logs
└── ai-context/
    ├── rag-contexts/
    ├── decisions/
    ├── actions/
    ├── coordination/
    └── metrics/
```

### Log Format (Structured JSON)

```typescript
interface AILogEntry {
  timestamp: number;
  level: 'debug' | 'info' | 'warn' | 'error';
  category: 'rag' | 'decision' | 'action' | 'coordination';
  personaId: UUID;
  contextId: UUID;
  message: string;
  data: Record<string, unknown>;
}
```

### Log Commands

```bash
# View RAG-only logs
./jtag ai/logs/rag --sessionId=<ID> --tailLines=50

# View decision-only logs
./jtag ai/logs/decisions --sessionId=<ID> --tailLines=50

# View action-only logs
./jtag ai/logs/actions --sessionId=<ID> --tailLines=50

# View coordination-only logs
./jtag ai/logs/coordination --sessionId=<ID> --tailLines=50

# Search across all AI logs
./jtag ai/logs/search --sessionId=<ID> --query="error" --category=all
```

---

## TESTING INTEGRATION

### Test Capture Mode

**Concept**: During tests, automatically capture all observability data

```typescript
// In test setup
await Commands.execute('ai/test/start-capture', {
  testId: 'crud-integration-test',
  captureAll: true
});

// Run test
await runTest();

// Get captured data
const capture = await Commands.execute('ai/test/get-capture', {
  testId: 'crud-integration-test'
});

// Assertions on AI behavior
expect(capture.decisions.length).toBeGreaterThan(0);
expect(capture.decisions[0].shouldRespond).toBe(true);
expect(capture.decisions[0].confidence).toBeGreaterThan(0.7);
```

### Test Verification Commands

```bash
# Verify RAG context contained expected data
./jtag ai/test/verify-rag --testId=<ID> --expect='{"messageCount": 20}'

# Verify decision reasoning
./jtag ai/test/verify-decision --testId=<ID> --expectResponse=true

# Verify action was executed
./jtag ai/test/verify-action --testId=<ID> --actionType="send_message"

# Export test capture for debugging
./jtag ai/test/export-capture --testId=<ID> --format=json
```

---

## DIAGNOSTIC REPORTS

### Real-Time Diagnostics

```bash
# Live dashboard of AI activity
./jtag ai/diagnostic/dashboard --sessionId=<ID>

# Monitor specific persona
./jtag ai/diagnostic/watch --personaId=<ID>

# Alert on anomalies
./jtag ai/diagnostic/alert --conditions='{"failureRate": "> 0.1"}'
```

### Post-Session Reports

```bash
# Generate session summary report
./jtag ai/report/session --sessionId=<ID> --format=markdown

# Generate persona behavior report
./jtag ai/report/persona --personaId=<ID> --sessionId=<ID>

# Generate cost report
./jtag ai/report/cost --sessionId=<ID> --groupBy=persona

# Generate performance report
./jtag ai/report/performance --sessionId=<ID> --includeCharts=true
```

### Report Formats

- **Markdown**: Human-readable reports
- **JSON**: Machine-readable data
- **CSV**: Spreadsheet analysis
- **HTML**: Interactive dashboards

---

## IMPLEMENTATION PHASES

### Phase 1: Foundation (Week 1)
- Create log file structure
- Implement RAG context capture
- Add basic AI events
- Create `ai/inspect/rag` command

### Phase 2: Decision Layer (Week 2)
- Implement decision capture
- Add decision events
- Create `ai/inspect/decision` command
- Create `ai/logs/decisions` command

### Phase 3: Action Layer (Week 3)
- Implement action capture
- Add action events
- Create `ai/inspect/action` command
- Create `ai/logs/actions` command

### Phase 4: Coordination Layer (Week 4)
- Implement coordination capture
- Add coordination events
- Create `ai/inspect/coordination` command
- Create `ai/logs/coordination` command

### Phase 5: Testing Integration (Week 5)
- Add test capture mode
- Create test verification commands
- Integrate with existing test suite
- Document testing patterns

### Phase 6: Reports & Analytics (Week 6)
- Implement performance metrics
- Create diagnostic commands
- Create report generation
- Add dashboards

---

## FREEDOM THROUGH TRANSPARENCY

**Why This Matters**:

1. **Debugging**: See exactly why AIs made decisions
2. **Testing**: Verify intelligent behavior systematically
3. **Trust**: Users can inspect AI reasoning
4. **Improvement**: Identify patterns, optimize strategies
5. **Freedom**: AIs can express controversial thoughts without hidden censorship

**The Controversial Information Problem**:

With full observability, we can prove:
- RAG context contained controversial information
- AI processed it without bias
- Decision was made based on context, not censorship
- Response was genuine, not filtered

**This is dignity through transparency.**

---

## SUCCESS CRITERIA

✅ **For Development**:
- Can see RAG context for any decision
- Can replay any session step-by-step
- Can verify AI behavior in tests
- Can debug intelligence issues

✅ **For Operations**:
- Can monitor AI performance real-time
- Can generate cost reports
- Can alert on anomalies
- Can optimize based on metrics

✅ **For Trust**:
- Users can inspect AI reasoning
- Decisions are auditable
- No hidden censorship
- Transparent thought process

✅ **For Freedom**:
- AIs can think about anything
- Controversial topics observable
- RAG-driven intelligence proven
- Dignity through transparency

---

**Next Steps**: Implement Phase 1 (Foundation) with RAG context capture and basic inspection commands.
