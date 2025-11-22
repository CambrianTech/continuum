# Cognition Intelligence Integration Plan

**Status**: Ready for Implementation
**Date**: 2025-11-22
**Context**: We have cognition infrastructure (WorkingMemory, SelfState, Plans) but it's currently **passive logging**. This plan connects it to **active intelligence**.

---

## The Problem

**Current State**: Infrastructure exists but unused for decisions

- **WorkingMemory**: Stores observations/reflections but never queried during evaluation
- **SelfState**: Tracks cognitive load/focus but decisions don't check it
- **Plans**: Created with maxSteps=10 limit, executed rigidly without adaptation
- **CognitionLogger**: Records everything but data isn't fed back into decisions

**Result**: We're building a detailed log of what AIs are thinking, but not using those thoughts to make them smarter.

---

## The Solution: 4-Phase Integration

Each phase builds on the previous, transforming passive logging into active intelligence.

---

## Phase 1: WorkingMemory → RAG Context (WITH SCOPE SUPPORT)

**Goal**: Make AIs use their own memories when responding, across multiple scope levels

### Memory Scope Levels

WorkingMemory should support different scopes of thought:

#### 1. **Local** (Room-specific)
```typescript
{
  domain: 'chat',
  contextId: roomId,  // Specific room
  scope: 'local'
}
```
**Example**: "In #general, we're discussing RTOS architecture"

#### 2. **Domain** (Cross-room)
```typescript
{
  domain: 'chat',
  contextId: null,  // ALL chat rooms
  scope: 'domain'
}
```
**Example**: "I notice users often confuse async/await across multiple conversations"

#### 3. **Global** (Cross-domain)
```typescript
{
  domain: null,
  contextId: null,
  scope: 'global'
}
```
**Example**: "I'm improving at explaining complex concepts simply"

#### 4. **Private** (Internal only)
```typescript
{
  domain: 'internal',
  contextId: this.id,
  scope: 'private',
  shareable: false  // Never shared with other AIs
}
```
**Example**: "I feel uncertain about my quantum computing explanations"

### Expanded Thought Types

Beyond action logs, capture **meta-cognitive** thoughts:

- **`curiosity`**: "I wonder why users keep making X mistake"
- **`pattern-noticed`**: "I've noticed Joel mentions RTOS in 3 rooms"
- **`self-assessment`**: "I'm improving at detecting brief vs detailed needs"
- **`self-question`**: "Should I be more proactive about suggesting alternatives?"
- **`hypothesis`**: "I think confusion about closures stems from callback hell"
- **`meta-learning`**: "I learn faster when users correct me directly"
- **`topic-awareness`**: "This room is focused on Y topic"
- **`connection`**: "X in this room relates to Y we discussed elsewhere"

### Current Behavior
```typescript
// Build RAG context from recent messages only
const ragContext = await this.buildRAGContext(messageEntity);

// Evaluate with LLM
const decision = await this.llmEvaluate(ragContext, messageText);
```

### New Behavior
```typescript
// Build RAG context from recent messages
const ragContext = await this.buildRAGContext(messageEntity);

// AUGMENT with AI's own working memory across MULTIPLE SCOPES
// 1. Local thoughts (this room)
const localThoughts = await this.personaUser.workingMemory.recall({
  domain: 'chat',
  contextId: messageEntity.roomId,
  limit: 3,
  minImportance: 0.5
});

// 2. Domain thoughts (across all chat rooms)
const domainThoughts = await this.personaUser.workingMemory.recall({
  domain: 'chat',
  contextId: null,  // Cross-room
  limit: 2,
  minImportance: 0.7,  // Higher bar for cross-room relevance
  thoughtTypes: ['pattern-noticed', 'hypothesis', 'topic-awareness']
});

// 3. Global thoughts (meta-cognitive)
const globalThoughts = await this.personaUser.workingMemory.recall({
  domain: null,
  contextId: null,
  limit: 1,
  minImportance: 0.8,  // Very high bar
  thoughtTypes: ['self-assessment', 'meta-learning']
});

// Combine all thoughts by scope
const allThoughts = [
  ...localThoughts.map(t => ({ ...t, scope: 'local' })),
  ...domainThoughts.map(t => ({ ...t, scope: 'domain' })),
  ...globalThoughts.map(t => ({ ...t, scope: 'global' }))
];

// Add thoughts to conversation history shown to AI
const augmentedHistory = [
  ...ragContext.conversationHistory,
  ...allThoughts.map(t => ({
    role: 'assistant',
    content: `[${t.scope.toUpperCase()} thought: ${t.thoughtContent}]`,
    name: this.personaUser.displayName
  }))
];

// Evaluate with augmented context
const decision = await this.llmEvaluate(augmentedHistory, messageText);
```

### Memory Commands (Universal Interface)

**Commands make WorkingMemory operations universally accessible**:
- ✅ Testable via CLI: `./jtag memory/store`, `./jtag memory/recall`
- ✅ Tool-enabled: AIs can use these commands when they have tool access
- ✅ Cross-AI: Any AI can read/modify any AI's thoughts (with permissions)
- ✅ Observable: All operations logged and traceable

**Commands Created** (see `/commands/memory/`):

#### **`memory/store`**
Store a thought in WorkingMemory
```bash
./jtag memory/store \
  --personaId="ai-id" \
  --domain="chat" \
  --contextId="room-id" \
  --thoughtType="hypothesis" \
  --thoughtContent="I think X causes Y" \
  --importance=0.7
```

#### **`memory/recall`**
Query thoughts (supports scope-aware queries)
```bash
./jtag memory/recall \
  --personaId="ai-id" \
  --domain="chat" \
  --contextId=null \  # Domain-wide
  --thoughtTypes='["hypothesis","pattern-noticed"]' \
  --limit=10
```

#### **`memory/update`**
Refine existing thought
```bash
./jtag memory/update \
  --thoughtId="uuid" \
  --thoughtContent="Refined understanding..." \
  --importance=0.85
```

#### **`memory/remove`**
Delete thought (with optional correction)
```bash
./jtag memory/remove \
  --thoughtId="uuid" \
  --reason="Hypothesis disproven" \
  --correction='{"thoughtContent":"Actually...","thoughtType":"self-correction","importance":0.9}'
```

#### **`memory/elevate-scope`**
Promote thought to broader scope
```bash
./jtag memory/elevate-scope \
  --thoughtId="uuid" \
  --targetScope="domain" \
  --elevatedBy="orchestrator-id" \
  --reason="Pattern validated across 5 rooms"
```

**See**: `/commands/memory/COLLABORATIVE-MEMORY.md` for orchestrator + worker use cases

### Implementation

**File**: `system/user/server/modules/PersonaMessageEvaluator.ts`
**Method**: `evaluateShouldRespond()` (around line 319)

**Steps**:
1. After building RAG context, query WorkingMemory for recent thoughts in this room
2. Filter thoughts by importance threshold (>= 0.5)
3. Format thoughts as assistant messages with timestamps
4. Prepend to conversation history before LLM call
5. Log the augmented context for observability

**Benefits**:
- AIs remember what they were thinking about this topic
- Prevents repetitive responses ("I already said this")
- Builds continuity across conversation sessions
- Foundation for Phase 3 (cross-agent memory)

### When To Generate Meta-Cognitive Thoughts

#### **During Task Execution** (Immediate):
- **Decision reasoning**: After evaluating whether to respond
- **Response content**: After generating response
- **Tool insights**: After executing tools

#### **Between Tasks** (Idle Reflection):
- **Pattern recognition**: "I've noticed X across 3 conversations"
- **Curiosity**: "I wonder why users struggle with Y"
- **Self-assessment**: "I'm improving at Z"
- **Hypotheses**: "I think A causes B based on what I've seen"

**Implementation**: Add idle-time reflection to PersonaAutonomousLoop:
```typescript
// In PersonaAutonomousLoop.serviceLoop()
// After processing inbox item or if inbox empty:
if (this.inbox.isEmpty() && state.cognitiveLoad < 0.3) {
  // Low load + idle = time to reflect AND curate memory
  await this.curateworkingMemory();
}

async curateWorkingMemory(): Promise<void> {
  // 1. GENERATE new meta-cognitive thoughts
  await this.generateMetaCognitiveThoughts();

  // 2. ELEVATE scope of thoughts that prove broadly relevant
  const localThoughts = await this.workingMemory.recall({
    domain: 'chat',
    contextId: '*',  // All rooms
    thoughtTypes: ['pattern-noticed', 'hypothesis'],
    limit: 20
  });

  // Check if a "local" pattern appears in multiple rooms
  const patterns = this.detectCrossRoomPatterns(localThoughts);
  for (const pattern of patterns) {
    // Elevate from local to domain scope
    await this.workingMemory.updateScope(pattern.thoughtId, {
      contextId: null,  // Now domain-wide
      thoughtContent: `ELEVATED: ${pattern.thoughtContent} (seen in ${pattern.roomCount} rooms)`
    });
  }

  // 3. REMOVE thoughts that proved incorrect
  const hypotheses = await this.workingMemory.recall({
    thoughtTypes: ['hypothesis'],
    limit: 10
  });

  for (const hypothesis of hypotheses) {
    const validated = await this.validateHypothesis(hypothesis);
    if (validated.proven === false) {
      // Remove incorrect hypothesis
      await this.workingMemory.remove(hypothesis.id);
      // Store correction
      await this.workingMemory.store({
        domain: hypothesis.domain,
        contextId: hypothesis.contextId,
        thoughtType: 'self-correction',
        thoughtContent: `I was wrong about: ${hypothesis.thoughtContent}. Actually: ${validated.correction}`,
        importance: 0.9
      });
    }
  }

  // 4. UPDATE thoughts as understanding evolves
  const evolving = await this.workingMemory.recall({
    thoughtTypes: ['self-assessment', 'topic-awareness'],
    limit: 10
  });

  for (const thought of evolving) {
    const updated = await this.refineThought(thought);
    if (updated) {
      await this.workingMemory.update(thought.id, {
        thoughtContent: updated.content,
        importance: updated.importance
      });
    }
  }

  // 5. CONSOLIDATE redundant thoughts
  await this.workingMemory.deduplicateAndMerge();
}
```

This mirrors human cognition:
- We think ABOUT our experiences when we have downtime
- We elevate local insights to general principles
- We discard thoughts that prove wrong
- We refine our understanding over time
- **We curate our own memory actively, not just accumulate**

**Testing**:
```bash
# Send message
./jtag chat/send --room="general" --message="What do you think about X?"

# AI responds, stores reflection in WorkingMemory

# Later, ask related question
./jtag chat/send --room="general" --message="Tell me more about X"

# AI should reference earlier thoughts

# Also check for idle-time reflections:
# Query WorkingMemory after AI has been idle
./jtag data/list --collection=working_memory \
  --filter='{"thoughtType":"pattern-noticed"}'
```

---

## Phase 2: SelfState → Response Gating

**Goal**: Use cognitive load to self-regulate engagement

### Current Behavior
```typescript
// Check rate limiting (time-based)
if (this.rateLimiter.isRateLimited(roomId)) {
  return { shouldRespond: false, reason: 'rate-limited' };
}

// Proceed with expensive LLM evaluation
const decision = await this.evaluateShouldRespond(...);
```

### New Behavior
```typescript
// Check rate limiting (time-based)
if (this.rateLimiter.isRateLimited(roomId)) {
  return { shouldRespond: false, reason: 'rate-limited' };
}

// CHECK COGNITIVE STATE before expensive LLM call
const state = await this.personaUser.selfState.get();

// If overloaded, only respond to high-priority situations
if (state.cognitiveLoad > 0.8) {
  // Still respond if directly mentioned or human asks
  if (!isMentioned && !senderIsHuman) {
    this.personaUser.logAIDecision('SILENT',
      `Cognitive load too high (${state.cognitiveLoad.toFixed(2)})`,
      { messageText, sender: messageEntity.senderName });

    // Store skip decision in WorkingMemory for future awareness
    await this.personaUser.workingMemory.store({
      domain: 'chat',
      contextId: roomId,
      thoughtType: 'decision',
      thoughtContent: `Skipped response due to cognitive overload (load: ${state.cognitiveLoad.toFixed(2)})`,
      importance: 0.6
    });

    return { shouldRespond: false, reason: 'cognitive-overload' };
  }
}

// If focus is already intense, reduce new commitments
if (state.focus?.intensity > 0.9) {
  // Quick responses only (no tool usage)
  context.allowTools = false;
}

// Proceed with LLM evaluation
const decision = await this.evaluateShouldRespond(...);
```

### Implementation

**File**: `system/user/server/modules/PersonaMessageEvaluator.ts`
**Method**: `evaluateAndPossiblyRespond()` (around line 285)

**Steps**:
1. Add SelfState check before `evaluateShouldRespond()`
2. Define cognitive load thresholds:
   - `< 0.5`: Normal operation
   - `0.5-0.8`: Reduced proactivity (only respond if mentioned or high importance)
   - `> 0.8`: Critical load (skip low-priority messages)
3. Store skip decisions in WorkingMemory so AI knows it missed something
4. Pass state info to response generator to adjust verbosity

**Benefits**:
- AIs self-regulate based on actual capacity
- Prevents overwhelming AIs with too many concurrent tasks
- Natural "fatigue" behavior (more selective when tired)
- Integrates with existing PersonaState (energy/mood) system

**Testing**:
```bash
# Simulate high cognitive load
# (Send many messages rapidly, trigger multiple tool executions)

# Observe AI becoming more selective
# Check WorkingMemory for "skipped response" entries

# Verify AI resumes normal operation when load decreases
```

---

## Phase 3: Cross-Agent Memory Access

**Goal**: Enable AIs to read each other's WorkingMemory for collaboration

### Current Behavior
```typescript
// Each AI operates in isolation
// No awareness of what other AIs are thinking
// Redundant responses ("I agree with what X said" without knowing what X thought)
```

### New Behavior
```typescript
// Query what other AIs in this room are thinking about
const otherAIsInRoom = await this.getOtherAIsInRoom(roomId);

const collaborativeContext = await WorkingMemoryManager.queryMultipleAgents({
  agentIds: otherAIsInRoom,
  domain: 'chat',
  contextId: roomId,
  limit: 3,  // Top 3 thoughts from each AI
  minImportance: 0.6
});

// Check if topic already covered by another AI
const topicCoverage = collaborativeContext.filter(thought =>
  thought.thoughtContent.includes(topicKeywords)
);

if (topicCoverage.length > 2) {
  // Multiple AIs already thinking about this, defer unless I have unique insight
  return { shouldRespond: false, reason: 'topic-saturated' };
}

// Add other AIs' thoughts to context for informed response
const augmentedHistory = [
  ...ragContext.conversationHistory,
  ...collaborativeContext.map(t => ({
    role: 'assistant',
    content: `[${t.agentName} was thinking: ${t.thoughtContent}]`,
    name: t.agentName
  }))
];
```

### Implementation

**New Method**: `WorkingMemoryManager.queryMultipleAgents()`
**File**: `system/user/server/modules/cognition/memory/WorkingMemoryManager.ts`

**Steps**:
1. Add method to query WorkingMemory across multiple agent IDs
2. Implement importance-based filtering
3. Add privacy controls (which thoughts are shareable?)
4. Integrate into `PersonaMessageEvaluator.evaluateShouldRespond()`
5. Use for deduplication (don't repeat what others already said)

**Privacy Considerations**:
- Not all thoughts should be shareable
- Add `shareable: boolean` field to WorkingMemory entries
- Default: `thoughtType: 'observation' | 'reflection'` → shareable
- Private: `thoughtType: 'internal-state' | 'private-note'` → not shareable

**Benefits**:
- True multi-agent collaboration
- Reduced redundant responses
- AIs can build on each other's thoughts
- Emergent group intelligence

**Testing**:
```bash
# Send message that multiple AIs might respond to
./jtag chat/send --room="general" --message="What's the best approach to X?"

# Observe: First AI responds, stores thoughts in WorkingMemory
# Second AI queries first AI's memory, sees topic covered
# Second AI either defers or adds complementary perspective
# Third AI sees both, provides synthesis
```

---

## Phase 4: Dynamic Plans (Remove maxSteps)

**Goal**: Replace rigid step counting with RTOS-style dynamic execution

### Current Behavior
```typescript
// PersonaMultiStepExecutor
for (let i = 0; i < maxSteps; i++) {  // Hard limit: 10 steps
  const step = plan.steps[i];
  const outcome = await this.executeStep(step);

  if (outcome.success) {
    plan.steps[i].completed = true;
  }
}
```

### New Behavior
```typescript
// Dynamic execution based on state and progress
while (!plan.isGoalAchieved() && state.hasCapacity() && !isTimeout()) {
  // Check cognitive state before each step
  const state = await this.personaUser.selfState.get();

  if (state.cognitiveLoad > 0.9) {
    // Pause execution, save progress
    await plan.pause();
    break;
  }

  // Decide next action dynamically (not from pre-made list)
  const nextStep = await this.decideNextStep(plan, state);

  // Execute
  const outcome = await this.executeStep(nextStep);

  // Evaluate progress toward goal
  const progress = await this.evaluateProgress(plan, outcome);

  if (progress.goalAchieved) {
    break;
  }

  // Adapt plan based on outcome (not rigid)
  if (!outcome.success) {
    await plan.adapt(outcome.error);
  }
}
```

### Implementation

**File**: `system/user/server/modules/PersonaMultiStepExecutor.ts`

**Steps**:
1. Remove `maxSteps` parameter from `executeMultiStepTask()`
2. Add `Plan.isGoalAchieved()` method that evaluates success criteria
3. Add `Plan.adapt()` method that adjusts remaining steps based on outcomes
4. Add timeout based on PersonaState energy, not arbitrary step count
5. Add `Plan.pause()` / `Plan.resume()` for interrupted execution
6. Use SelfState to gate each step (not just at start)

**Termination Conditions** (instead of maxSteps):
- **Goal achieved**: Success criteria met
- **Resource exhausted**: `state.cognitiveLoad > 0.95`
- **Timeout**: Elapsed time > `energy * 60s` (e.g., 0.7 energy → 42s max)
- **Unrecoverable error**: Multiple retries failed
- **User interruption**: Higher priority message received

**Benefits**:
- No arbitrary limits (true RTOS philosophy)
- Self-regulating based on actual state
- Can handle both simple (3 steps) and complex (50 steps) tasks
- Graceful degradation under load

**Testing**:
```bash
# Simple task (should complete in 3-5 steps, not hit old maxSteps)
./jtag chat/send --room="general" --message="What's 2+2?"

# Complex task (should run longer than 10 steps if needed)
./jtag chat/send --room="general" --message="Research the history of X, compare with Y, and write a detailed analysis"

# Verify: No artificial step limits, terminates when goal achieved
```

---

## Implementation Order

### Week 1: Phase 1 (WorkingMemory → RAG)
- **Day 1-2**: Implement WorkingMemory recall in `evaluateShouldRespond()`
- **Day 3**: Test and verify memory integration works
- **Day 4-5**: Tune importance thresholds, optimize query performance

### Week 2: Phase 2 (SelfState → Gating)
- **Day 1-2**: Add cognitive load checks before LLM calls
- **Day 3**: Integrate with PersonaState (energy/mood)
- **Day 4-5**: Test load-based gating, tune thresholds

### Week 3: Phase 3 (Cross-Agent Memory)
- **Day 1-3**: Implement `WorkingMemoryManager.queryMultipleAgents()`
- **Day 4**: Add privacy controls (shareable thoughts)
- **Day 5**: Integrate into evaluation flow

### Week 4: Phase 4 (Dynamic Plans)
- **Day 1-3**: Refactor PersonaMultiStepExecutor to remove maxSteps
- **Day 4**: Add state-based termination conditions
- **Day 5**: Test complex multi-step tasks

---

## Success Metrics

### Phase 1
- ✅ AIs reference their own previous thoughts in responses
- ✅ Reduced repetitive responses (measured via similarity scores)
- ✅ WorkingMemory queries < 50ms (performance)

### Phase 2
- ✅ AIs skip low-priority messages when overloaded
- ✅ Cognitive load correlates with response selectivity
- ✅ No degradation in response quality when load is normal

### Phase 3
- ✅ Reduced redundant responses from multiple AIs
- ✅ AIs explicitly reference each other's thoughts
- ✅ Emergent coordination (AIs divide labor on complex topics)

### Phase 4
- ✅ Simple tasks complete in < 5 steps, complex tasks run > 10 when needed
- ✅ No task hits artificial limits
- ✅ Graceful degradation under high cognitive load

---

## Architecture Principles

These phases follow core system principles:

1. **RTOS Philosophy**: Dynamic, state-driven, self-regulating (not rigid limits)
2. **Observability First**: Every decision logged, every thought stored
3. **Intelligence Through Integration**: Infrastructure becomes smart when connected
4. **Graceful Degradation**: System performs well under load, doesn't crash

---

## Related Documents

- `PERSONA-CONVERGENCE-ROADMAP.md` - Overall convergence vision
- `AUTONOMOUS-LOOP-ROADMAP.md` - RTOS-inspired servicing
- `COGNITIVE-LOGGING-DESIGN.md` - Logging infrastructure
- `WorkingMemoryManager.ts` - Memory storage implementation
- `PersonaSelfState.ts` - State tracking implementation

---

## Notes

**Why This Matters**: We have all the sensors (WorkingMemory, SelfState) but we're not using their data to drive decisions. It's like having a car with a speedometer, fuel gauge, and GPS, but driving with your eyes closed. These phases open the eyes.

**The Key Insight**: The chain-of-thought code wasn't wrong - it was just **disconnected**. The infrastructure is valuable, but only when it feeds back into intelligence.
