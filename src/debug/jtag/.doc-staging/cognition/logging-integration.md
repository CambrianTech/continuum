# Cognitive Logging Integration Plan

## Vision: Complete Observability for Agent Development

To build true agents (per the LLM Agent paper), we need **complete observability** into:
- Perception (what the agent sees)
- Reasoning (how it thinks)
- Memory (what it remembers)
- Action (what it does)

The logging infrastructure provides 3 collections:
1. **adapter_decision_logs** - Decision-making pipeline
2. **tool_execution_logs** - Action execution
3. **response_generation_logs** - AI generation

## Current Coverage (as of 2025-11-17)

### ✅ Well-Logged Modules

**DecisionAdapterChain** (system/user/server/modules/cognition/DecisionAdapterChain.ts)
- Logs: RESPOND/SILENT/PASS decisions from each adapter
- Includes: confidence, reason, duration, context metadata
- Collection: `adapter_decision_logs`
- Quality: ⭐⭐⭐⭐⭐ (Excellent)

**PersonaToolExecutor** (system/user/server/modules/PersonaToolExecutor.ts)
- Logs: Every tool call (success/error)
- Includes: tool name, parameters, result, duration
- Collection: `tool_execution_logs`
- Quality: ⭐⭐⭐⭐⭐ (Excellent)

**PersonaResponseGenerator** (system/user/server/modules/PersonaResponseGenerator.ts)
- Logs: AI generation attempts
- Includes: provider, model, tokens, duration, status
- Collection: `response_generation_logs`
- Quality: ⭐⭐⭐⭐⭐ (Excellent)

### ❌ Missing Logging (CRITICAL GAPS)

## Priority 1: Reasoning & Planning Modules

These are **critical for agent development** - without logging here, we can't debug planning failures or understand why agents make poor decisions.

### SimplePlanFormulator
**Location**: `system/user/server/modules/cognition/reasoning/SimplePlanFormulator.ts`

**Missing Logs**:
1. Plan creation (when a plan is formulated)
2. Step execution (as each step runs)
3. Re-planning (when plan needs adjustment)
4. Plan completion/failure

**Proposed Logging**:
```typescript
// When plan is created
await CognitionLogger.logPlanCreation(
  personaId,
  personaName,
  task,
  plan.steps,
  plan.estimatedDuration,
  'chat',
  contextId
);

// When step executes
await CognitionLogger.logPlanStepExecution(
  personaId,
  personaName,
  stepIndex,
  step.action,
  result.success ? 'success' : 'failed',
  duration,
  'chat',
  contextId,
  { stepResult: result.data }
);

// When re-planning
await CognitionLogger.logPlanReplan(
  personaId,
  personaName,
  originalPlan,
  newPlan,
  reason,
  'chat',
  contextId
);
```

**New Entity Needed**: `PlanExecutionLogEntity`
- Fields: personaId, planId, task, steps[], currentStep, status, duration, outcomes[]

### PersonaSelfState
**Location**: `system/user/server/modules/cognition/PersonaSelfState.ts`

**Missing Logs**:
1. Goal updates
2. Belief changes
3. Capability assessments
4. Self-reflection results

**Proposed Logging**:
```typescript
// When beliefs update
await CognitionLogger.logBeliefUpdate(
  personaId,
  personaName,
  beliefKey,
  oldValue,
  newValue,
  evidence,
  'chat',
  contextId
);

// When self-reflecting
await CognitionLogger.logSelfReflection(
  personaId,
  personaName,
  reflectionType, // 'goal-check', 'performance-review', 'capability-assessment'
  insights,
  actionsTaken,
  'chat',
  contextId
);
```

**New Entity Needed**: `SelfStateLogEntity`
- Fields: personaId, stateType, keyValues{}, changeReason, timestamp

### WorkingMemoryManager
**Location**: `system/user/server/modules/cognition/memory/WorkingMemoryManager.ts`

**Missing Logs**:
1. Memory storage operations
2. Memory retrieval operations
3. Memory eviction (when cache is full)
4. Memory consolidation (moving to long-term)

**Proposed Logging**:
```typescript
// When storing memory
await CognitionLogger.logMemoryOperation(
  personaId,
  personaName,
  'store',
  memoryKey,
  memorySize,
  importance,
  'chat',
  contextId
);

// When retrieving memory
await CognitionLogger.logMemoryOperation(
  personaId,
  personaName,
  'retrieve',
  queryKey,
  retrievedCount,
  relevanceScore,
  'chat',
  contextId
);

// When evicting memory (LRU)
await CognitionLogger.logMemoryOperation(
  personaId,
  personaName,
  'evict',
  evictedKey,
  evictedSize,
  reason,
  'chat',
  contextId
);
```

**New Entity Needed**: `MemoryOperationLogEntity`
- Fields: personaId, operation, key, size, metadata{}, timestamp

## Priority 2: Decision Adapter Internal Reasoning

While DecisionAdapterChain logs the *output* of adapters, we need to log their *internal reasoning* for debugging.

### FastPathAdapter
**Missing**: Why it chose RESPOND/SILENT (mention detection, keyword matching)

### ThermalAdapter
**Missing**: Temperature calculation, threshold comparison, cooling rate

### LLMAdapter
**Missing**: LLM prompt sent, response received, parsing logic

**Proposed**: Each adapter should log its reasoning process before returning decision:

```typescript
// Inside adapter.evaluate()
const reasoning = {
  inputSignals: { isMentioned, temperature, keywords },
  calculations: { score: 0.8, threshold: 0.5 },
  intermediateSteps: ['detected @mention', 'high priority message'],
  finalDecision: 'RESPOND'
};

await CognitionLogger.logAdapterReasoning(
  context.personaId,
  context.personaDisplayName,
  this.name,
  reasoning,
  duration,
  'chat',
  contextId
);

return { shouldRespond: true, confidence: 0.8, reason: '...' };
```

**New Entity Needed**: `AdapterReasoningLogEntity`
- Fields: personaId, adapterName, reasoning{}, duration, timestamp

## Priority 3: Peer Review & Collaboration

### PeerReviewManager
**Missing**: Peer review requests, responses, consensus building

### ProposalRatingAdapter
**Missing**: Proposal submissions, rating calculations, selection logic

**Proposed Logging**:
```typescript
// When requesting peer review
await CognitionLogger.logPeerReview(
  personaId,
  personaName,
  'request',
  proposal,
  reviewers,
  'chat',
  contextId
);

// When providing peer review
await CognitionLogger.logPeerReview(
  personaId,
  personaName,
  'provide',
  proposal,
  rating,
  feedback,
  'chat',
  contextId
);
```

**New Entity Needed**: `PeerReviewLogEntity`
- Fields: personaId, action, proposalId, reviewers[], ratings[], consensus, timestamp

## Implementation Strategy

### Phase 1: Extend CognitionLogger (1 hour)
Add new methods to CognitionLogger:
- `logPlanCreation()`
- `logPlanStepExecution()`
- `logPlanReplan()`
- `logBeliefUpdate()`
- `logSelfReflection()`
- `logMemoryOperation()`
- `logAdapterReasoning()`
- `logPeerReview()`

### Phase 2: Create New Entities (2 hours)
- PlanExecutionLogEntity
- SelfStateLogEntity
- MemoryOperationLogEntity
- AdapterReasoningLogEntity
- PeerReviewLogEntity

Register in EntityRegistry, add to COLLECTIONS constant.

### Phase 3: Integrate into Modules (3 hours)
Update each module to call logging methods at critical points.

### Phase 4: Create Query Tools (1 hour)
CLI commands for interrogating cognitive logs:
```bash
./jtag ai/cognition/plans --persona="helper-ai" --limit=10
./jtag ai/cognition/beliefs --persona="helper-ai" --changed-after="2025-11-17"
./jtag ai/cognition/memory --operation="evict" --limit=20
./jtag ai/cognition/adapter-reasoning --adapter="FastPathAdapter" --limit=10
```

### Phase 5: Visualization (2 hours)
Create cognitive dashboards showing:
- Plan execution timelines
- Belief evolution over time
- Memory usage patterns
- Adapter decision distributions

## Benefits for Agent Development

With complete logging:

1. **Debug Planning Failures**
   - See exactly which step failed
   - Understand why re-planning was triggered
   - Identify bottlenecks in execution

2. **Understand Decision-Making**
   - See full reasoning chain from perception → decision → action
   - Identify which adapters are most/least effective
   - Tune adapter priorities based on data

3. **Optimize Memory Usage**
   - Track which memories are most accessed
   - Identify inefficient eviction patterns
   - Tune cache sizes based on usage

4. **Improve Collaboration**
   - See how personas interact via peer review
   - Identify consensus patterns
   - Optimize review workflows

5. **Agent Benchmarking**
   - Compare agent performance across tasks
   - Identify which agents are best at planning vs execution
   - Train better models from logged data

## Connection to Agent Paper

From "Building Autonomous LLM Agents" paper:

> "Agents are designed to act according to the feedback from its environment. Rather than relying on a pre-set plan, agents generate their own strategies tailored to the task and context."

**Our logging enables this** by capturing:
- Environment feedback (tool results, memory retrievals)
- Strategy generation (plan creation, re-planning)
- Task adaptation (belief updates, self-reflection)

Without this logging, we're blind to agent cognition. With it, we can:
- Debug agent failures
- Tune agent parameters
- Train better models
- Build true autonomous agents

## Next Steps

1. Get approval for new entities
2. Implement Phase 1 (extend CognitionLogger)
3. Implement Phase 2 (create entities)
4. Roll out Phase 3 module by module (start with SimplePlanFormulator)
5. Build query tools as needed
6. Create visualizations for agent developers

**Goal**: Complete cognitive observability by 2025-11-20.
