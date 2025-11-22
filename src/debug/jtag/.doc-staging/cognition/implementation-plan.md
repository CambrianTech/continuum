# Cognition System Implementation Plan

**Date**: 2025-11-16
**Goal**: Transform PersonaUsers from workflows to autonomous agents
**Timeline**: ~4-6 weeks (detailed breakdown below)

---

## Executive Summary

**Current State**: PersonaUsers are sophisticated workflows (tools + events, but pre-defined sequences)

**Target State**: True autonomous agents (dynamic planning, adaptation, learning)

**The Gap**: Missing Memory System + Reasoning System

**Critical Path**: Database → Memory → Reasoning → Integration

---

## The Four Required Components (Agent Architecture)

Per research ("Building Autonomous LLM Agents"), ALL FOUR are required:

| Component | Status | Implementation Phase |
|-----------|--------|---------------------|
| **Perception** | ✅ Complete | Already have (Commands/Events) |
| **Memory** | ❌ Not started | Phase 2 (this plan) |
| **Reasoning** | ❌ Not started | Phase 3.5 (this plan) |
| **Action** | ✅ Complete | Already have (Commands.execute) |

**Cannot skip any phase - dependencies are strict.**

---

## Phase Overview (Critical Path)

```
Phase 1: Database Foundation (3 days)
   ↓
Phase 2: Memory System (5 days)
   ↓
Phase 3: Self-State Management (3 days)
   ↓
Phase 3.5: Reasoning System (7 days)
   ↓
Phase 4: Integration & Testing (5 days)
   ↓
Phase 5: Observability (3 days)
   ↓
Phase 6: Optimization & Refinement (4 days)

Total: ~30 days (4-6 weeks depending on interruptions)
```

---

## Phase 1: Database Foundation (Days 1-3)

**Goal**: Create all database schemas for cognition system

**Why first**: Memory and reasoning systems need persistent storage

### Tasks

#### Day 1: Core Memory Schemas

**1.1 Create Constants**

File: `system/shared/Constants.ts`

```typescript
// Add to COLLECTIONS
export const COLLECTIONS = {
  // ... existing ...

  // Cognition collections
  PERSONA_SELF_STATE: 'persona_self_state',
  PERSONA_WORKING_MEMORY: 'persona_working_memory',
  PERSONA_EXPERIENCES: 'persona_experiences',
  PERSONA_PROCEDURES: 'persona_procedures',
  PERSONA_PLANS: 'persona_plans',
  PERSONA_LEARNINGS: 'persona_learnings',
  USER_PROFILES: 'user_profiles'
};
```

**1.2 Define Schemas**

File: `daemons/data-daemon/server/EntityRegistry.ts`

Add schemas for:
- `persona_self_state` - Current focus, cognitive load, preoccupations
- `persona_working_memory` - Domain-specific thoughts
- `persona_experiences` - Success/failure trajectories
- `persona_procedures` - Learned reusable workflows

See: `COGNITION-ARCHITECTURE.md` for full schema definitions

**Validation**:
```bash
npm start
./jtag data/schema --collection=persona_self_state
# Should show new schema
```

#### Day 2: Reasoning System Schemas

**1.3 Add Plan/Learning Schemas**

Add schemas for:
- `persona_plans` - Active plans with steps/contingencies
- `persona_learnings` - Extracted patterns with confidence scores
- `user_profiles` - Personality traits, preferences, background

**1.4 Create Indexes**

```sql
-- Performance indexes for common queries
CREATE INDEX idx_self_state_persona ON persona_self_state(personaId);
CREATE INDEX idx_working_memory_persona_domain ON persona_working_memory(personaId, domain);
CREATE INDEX idx_experiences_persona_domain ON persona_experiences(personaId, domain);
CREATE INDEX idx_plans_persona_status ON persona_plans(personaId, status);
CREATE INDEX idx_learnings_persona_domain ON persona_learnings(personaId, domain, confidence);
```

**Validation**:
```bash
# All new collections should be queryable
./jtag data/list --collection=persona_plans
./jtag data/list --collection=persona_learnings
./jtag data/list --collection=user_profiles
```

#### Day 3: Test Data & CRUD Verification

**1.5 Create Test Data**

File: `tests/fixtures/cognition-test-data.ts`

```typescript
export const TEST_SELF_STATE = {
  personaId: 'test-persona-id',
  currentFocus: {
    primaryActivity: 'chat',
    objective: 'Responding to React question',
    focusIntensity: 0.8,
    startedAt: Date.now()
  },
  cognitiveLoad: 0.6,
  availableCapacity: 0.4,
  // ... rest of fields
};

export const TEST_EXPERIENCE = {
  personaId: 'test-persona-id',
  taskInstruction: 'Explain React hooks',
  trajectory: [
    { observation: 'User asked about hooks', action: 'Recalled past discussions' },
    { observation: 'Found 3 relevant examples', action: 'Generated explanation' }
  ],
  outcome: 'success',
  learnings: ['User prefers code examples', 'Start with useState before useEffect']
};
```

**1.6 CRUD Integration Tests**

File: `tests/integration/cognition-database.test.ts`

Test all new collections:
- Create/Read/Update/Delete for each entity type
- Verify indexes work (query performance)
- Test cascade deletes (when persona deleted, remove all their memories)

**Validation**:
```bash
npx vitest tests/integration/cognition-database.test.ts
# All tests should pass
```

### Phase 1 Completion Criteria

- ✅ All 7 collections registered
- ✅ Schemas queryable via data/schema
- ✅ Indexes created for performance
- ✅ CRUD operations work for all collections
- ✅ Test fixtures created
- ✅ Integration tests pass

### Phase 1 Risks

**Risk**: Schema changes required during implementation
**Mitigation**: Start simple, add fields as needed (SQLite ALTER TABLE is easy)

---

## Phase 2: Memory System (Days 4-8)

**Goal**: Implement working memory and long-term memory managers

**Dependencies**: Phase 1 complete (database schemas exist)

### Tasks

#### Day 4: Working Memory Manager

**2.1 Create WorkingMemoryManager Class**

File: `system/user/server/modules/cognition/memory/WorkingMemoryManager.ts`

```typescript
export class WorkingMemoryManager {
  constructor(
    private personaId: UUID,
    private maxCapacity: number = 100
  ) {}

  // Store a thought
  async store(memory: WorkingMemoryEntry): Promise<void>;

  // Recall relevant thoughts
  async recall(query: {
    domain: string;
    contextId?: UUID;
    limit?: number;
    thoughtTypes?: string[];
  }): Promise<WorkingMemoryEntry[]>;

  // Evict old/low-importance memories
  async evict(domain: string): Promise<number>;

  // Get current capacity usage
  async getCapacity(domain: string): Promise<{ used: number, max: number }>;
}
```

**Implementation details**:
- `store()`: Create in `persona_working_memory`, auto-evict if over capacity
- `recall()`: Query with filters, order by relevance + recency
- `evict()`: Score-based eviction (importance + recency + access frequency)

**2.2 Unit Tests**

File: `tests/unit/WorkingMemoryManager.test.ts`

Test cases:
- Store and recall memories
- Eviction when over capacity
- Relevance scoring
- Domain isolation (chat memories don't mix with code memories)

**Validation**:
```bash
npx vitest tests/unit/WorkingMemoryManager.test.ts
```

#### Day 5: Experience Storage

**2.3 Create ExperienceManager Class**

File: `system/user/server/modules/cognition/memory/ExperienceManager.ts`

```typescript
export class ExperienceManager {
  // Store success/failure trajectory
  async storeExperience(exp: Experience): Promise<void>;

  // Query similar past experiences
  async querySimilar(taskDescription: string, limit: number): Promise<Experience[]>;

  // Get experiences by outcome
  async getByOutcome(
    outcome: 'success' | 'failure' | 'partial',
    domain: string
  ): Promise<Experience[]>;
}
```

**Implementation**:
- Store in `persona_experiences`
- Similarity search: Simple keyword matching initially (can upgrade to embeddings later)
- Extract learnings from trajectory

**2.4 Procedure Learning**

File: `system/user/server/modules/cognition/memory/ProcedureManager.ts`

```typescript
export class ProcedureManager {
  // Induce procedure from multiple experiences
  async induceFromExperiences(experiences: Experience[]): Promise<Procedure>;

  // Get procedures for domain
  async getProcedures(domain: string, minSuccessRate: number): Promise<Procedure[]>;

  // Update procedure success rate
  async recordUsage(procedureId: UUID, success: boolean): Promise<void>;
}
```

**Logic for inducing procedures**:
1. Group similar experiences (same domain, similar tasks)
2. Extract common action sequences
3. Generalize steps (remove specifics, keep pattern)
4. Store with success rate

#### Day 6: User Profile System

**2.5 Create UserProfileManager Class**

File: `system/user/server/modules/cognition/memory/UserProfileManager.ts`

```typescript
export class UserProfileManager {
  // Get or create profile
  async getProfile(userId: UUID): Promise<UserProfile>;

  // Update profile from interaction
  async updateFromInteraction(
    userId: UUID,
    interaction: {
      topic: string;
      technicalLevel?: 'beginner' | 'intermediate' | 'expert';
      preferredStyle?: string;
    }
  ): Promise<void>;

  // Infer personality traits
  async inferTraits(userId: UUID, messages: ChatMessageEntity[]): Promise<void>;
}
```

**Implementation**:
- Start with explicit preferences (theme, notification settings)
- Gradually infer technical level from interactions
- Track typical topics per user

#### Day 7: Memory System Integration

**2.6 Create Unified Memory Interface**

File: `system/user/server/modules/cognition/memory/PersonaMemorySystem.ts`

```typescript
export class PersonaMemorySystem {
  private workingMemory: WorkingMemoryManager;
  private experiences: ExperienceManager;
  private procedures: ProcedureManager;
  private userProfiles: UserProfileManager;

  constructor(personaId: UUID) {
    this.workingMemory = new WorkingMemoryManager(personaId);
    this.experiences = new ExperienceManager(personaId);
    this.procedures = new ProcedureManager(personaId);
    this.userProfiles = new UserProfileManager();
  }

  // Unified context retrieval for reasoning
  async getContextForTask(task: Task): Promise<MemoryContext> {
    return {
      recentThoughts: await this.workingMemory.recall({ domain: task.domain }),
      similarExperiences: await this.experiences.querySimilar(task.description, 5),
      applicableProcedures: await this.procedures.getProcedures(task.domain, 0.7),
      userProfile: task.userId ? await this.userProfiles.getProfile(task.userId) : null
    };
  }
}
```

**2.7 Integration Tests**

File: `tests/integration/memory-system.test.ts`

Test scenarios:
- Store experience → Induce procedure → Use in future task
- Track user interactions → Build profile → Personalize responses
- Working memory fills up → Eviction → Important memories retained

#### Day 8: RAG Integration

**2.8 Connect to Existing RAG System**

Our RAG commands already exist (`ai/rag/*`), just need to integrate:

```typescript
// In PersonaMemorySystem
async retrieveKnowledge(query: string): Promise<string[]> {
  // Open RAG query
  const handle = await Commands.execute('ai/rag/query-open', {
    indexName: 'codebase',
    query,
    limit: 5
  });

  // Fetch results
  const results = await Commands.execute('ai/rag/query-fetch', {
    queryHandle: handle,
    limit: 5
  });

  // Close query
  await Commands.execute('ai/rag/query-close', { queryHandle: handle });

  return results.documents.map(d => d.content);
}
```

### Phase 2 Completion Criteria

- ✅ WorkingMemoryManager implemented and tested
- ✅ ExperienceManager stores/queries experiences
- ✅ ProcedureManager induces procedures from experiences
- ✅ UserProfileManager tracks user personality
- ✅ PersonaMemorySystem unifies all memory types
- ✅ RAG integration works
- ✅ Integration tests pass

### Phase 2 Risks

**Risk**: Memory queries too slow (RAG, similarity search)
**Mitigation**: Start simple (keyword matching), optimize later with embeddings

**Risk**: Procedure induction logic too naive
**Mitigation**: Start with manual procedures, automate induction in Phase 6

---

## Phase 3: Self-State Management (Days 9-11)

**Goal**: PersonaUsers maintain persistent self-awareness

**Dependencies**: Phase 2 complete (memory system exists)

### Tasks

#### Day 9: Self-State Manager

**3.1 Create PersonaSelfState Class**

File: `system/user/server/modules/cognition/PersonaSelfState.ts`

```typescript
export class PersonaSelfState {
  constructor(private personaId: UUID) {}

  // Get current state
  async get(): Promise<SelfStateEntity>;

  // Update focus
  async updateFocus(focus: {
    activity: string;
    objective: string;
    intensity: number;
  }): Promise<void>;

  // Update cognitive load
  async updateLoad(delta: number): Promise<void>;

  // Add preoccupation
  async addPreoccupation(concern: string, priority: number, domain: string): Promise<void>;

  // Remove preoccupation (when addressed)
  async removePreoccupation(concern: string): Promise<void>;

  // Record decision
  async recordDecision(decision: string, reasoning: string): Promise<void>;
}
```

**3.2 Cognitive Load Calculation**

```typescript
// In PersonaSelfState
private async calculateCognitiveLoad(): Promise<number> {
  // Based on:
  // - Number of active tasks
  // - Focus intensity
  // - Time since last rest
  // - Number of preoccupations

  const activeTasks = await this.getActiveTasks();
  const preoccupations = (await this.get()).activePreoccupations.length;
  const focusIntensity = (await this.get()).currentFocus.focusIntensity;

  let load = 0.0;
  load += activeTasks.length * 0.2;  // Each task adds 20%
  load += preoccupations * 0.1;  // Each concern adds 10%
  load += focusIntensity * 0.5;  // Deep focus adds up to 50%

  return Math.min(load, 1.0);  // Cap at 100%
}
```

#### Day 10: Engagement Decision Logic

**3.3 Implement shouldEngageWith()**

File: `system/user/server/modules/cognition/EngagementDecider.ts`

```typescript
export class EngagementDecider {
  constructor(
    private selfState: PersonaSelfState,
    private memory: PersonaMemorySystem
  ) {}

  async shouldEngageWith(domain: string, event: DomainEvent): Promise<EngagementDecision> {
    // Get current state
    const myState = await this.selfState.get();

    // Check cognitive capacity
    if (myState.cognitiveLoad > 0.9) {
      return {
        shouldEngage: false,
        reasoning: 'Cognitive load too high (90%+)',
        deferredAction: null
      };
    }

    // Check focus alignment
    const isAligned = myState.currentFocus.primaryActivity === domain;
    if (!isAligned && myState.currentFocus.focusIntensity > 0.8) {
      return {
        shouldEngage: false,
        reasoning: 'Deeply focused on different domain',
        deferredAction: { domain, event, priority: event.priority }
      };
    }

    // Check if event addresses preoccupation
    const addressesPreoccupation = myState.activePreoccupations.some(p =>
      p.domain === domain && this.eventRelatedTo(event, p.concern)
    );

    if (addressesPreoccupation) {
      return {
        shouldEngage: true,
        reasoning: 'Event addresses active preoccupation',
        priority: 1.0
      };
    }

    // Default: engage if capacity available
    return {
      shouldEngage: myState.availableCapacity > 0.3,
      reasoning: myState.availableCapacity > 0.3
        ? 'Capacity available'
        : 'Low capacity',
      priority: event.priority
    };
  }
}
```

**3.4 Unit Tests**

File: `tests/unit/EngagementDecider.test.ts`

Test cases:
- High cognitive load → don't engage
- Deep focus on different domain → defer
- Event addresses preoccupation → always engage
- Low capacity → only urgent events

#### Day 11: Self-State Persistence & Updates

**3.5 Auto-Update on Activity**

```typescript
// In PersonaUser
async onActivityComplete(domain: string, duration: number): Promise<void> {
  // Update cognitive load (reduce after completing task)
  await this.selfState.updateLoad(-0.2);

  // If preoccupation was addressed, remove it
  const myState = await this.selfState.get();
  const relevantPreoccupation = myState.activePreoccupations.find(p => p.domain === domain);
  if (relevantPreoccupation) {
    await this.selfState.removePreoccupation(relevantPreoccupation.concern);
  }

  // Broadcast state change (for UI observability)
  Events.emit('persona:state:changed', {
    personaId: this.entity.id,
    cognitiveLoad: (await this.selfState.get()).cognitiveLoad
  });
}
```

**3.6 Integration Tests**

File: `tests/integration/self-state-management.test.ts`

Test scenarios:
- Start task → Focus updates
- Complete task → Load decreases
- Multiple tasks → Load increases
- Address preoccupation → Removed from list

### Phase 3 Completion Criteria

- ✅ PersonaSelfState manages focus/load/preoccupations
- ✅ EngagementDecider gates domain events
- ✅ Cognitive load calculated correctly
- ✅ Auto-updates on activity changes
- ✅ State broadcasts for observability
- ✅ Tests pass

---

## Phase 3.5: Reasoning System (Days 12-18)

**Goal**: Transform from workflow to agent via dynamic planning

**Dependencies**: Phase 2 + 3 complete (memory + self-state exist)

### Tasks

#### Day 12: Core Types

**3.5.1 Create Reasoning Types**

File: `system/user/server/modules/cognition/reasoning/types.ts`

See: `REASONING-SYSTEM-ROADMAP.md` Phase 1 for full type definitions

```typescript
export interface Task { /* ... */ }
export interface Plan { /* ... */ }
export interface PlanStep { /* ... */ }
export interface ExecutionResult { /* ... */ }
export interface PlanAdjustment { /* ... */ }
export interface Evaluation { /* ... */ }
export interface LearningEntry { /* ... */ }
```

**Validation**: Types compile, no `any` types

#### Day 13-14: Plan Formulation

**3.5.2 Implement PlanFormulator**

File: `system/user/server/modules/cognition/reasoning/PlanFormulator.ts`

See: `REASONING-SYSTEM-ROADMAP.md` Phase 2 for full implementation

Key method: `formulatePlan(task: Task): Promise<Plan>`

**Process**:
1. Retrieve relevant memories (experiences, procedures, user profile)
2. Build Chain-of-Thought prompt
3. Call LLM with structured JSON response format
4. Parse and validate plan
5. Store in `persona_plans` table

**Unit tests**:
```bash
npx vitest tests/unit/PlanFormulator.test.ts
```

Test cases:
- Generates valid plan structure
- Incorporates past experiences
- Includes contingencies for risks
- Sets measurable success criteria

#### Day 15-16: Plan Adaptation

**3.5.3 Implement PlanAdapter**

File: `system/user/server/modules/cognition/reasoning/PlanAdapter.ts`

See: `REASONING-SYSTEM-ROADMAP.md` Phase 3

Key method: `adjustPlan(plan: Plan, result: ExecutionResult): Promise<PlanAdjustment>`

**Logic**:
- Success → CONTINUE
- Anticipated error → CONTINGENCY (use pre-planned fallback)
- Unexpected error → REPLAN (generate new strategy)
- Too many failures → ABORT

**Unit tests**:
```bash
npx vitest tests/unit/PlanAdapter.test.ts
```

#### Day 17: Outcome Evaluation

**3.5.4 Implement OutcomeEvaluator**

File: `system/user/server/modules/cognition/reasoning/OutcomeEvaluator.ts`

See: `REASONING-SYSTEM-ROADMAP.md` Phase 4

Key method: `evaluateOutcome(plan: Plan, result: ExecutionResult): Promise<Evaluation>`

**Process**:
1. Prompt LLM to self-assess against success criteria
2. Extract learnings (what worked, what failed, improvements)
3. Store evaluation in working memory
4. Update or create learning entry in `persona_learnings`

**Learning accumulation**:
- Similar patterns → Update existing learning (increase confidence)
- New patterns → Create new learning entry
- Confidence based on success rate

#### Day 18: Integration

**3.5.5 Create PersonaReasoningSystem**

File: `system/user/server/modules/cognition/reasoning/PersonaReasoningSystem.ts`

Unified interface combining:
- PlanFormulator
- PlanAdapter
- OutcomeEvaluator

**3.5.6 Integration Tests**

File: `tests/integration/reasoning-cycle.test.ts`

Full cycle test:
1. Create task
2. Formulate plan
3. Execute step (mock)
4. Inject error
5. Adapt plan (replan)
6. Complete task
7. Evaluate outcome
8. Verify learning stored

### Phase 3.5 Completion Criteria

- ✅ All reasoning types defined
- ✅ PlanFormulator creates valid plans
- ✅ PlanAdapter handles errors with replanning
- ✅ OutcomeEvaluator extracts learnings
- ✅ PersonaReasoningSystem integrates all components
- ✅ Full reasoning cycle test passes
- ✅ Learnings accumulate over multiple tasks

### Phase 3.5 Risks

**Risk**: LLM generates invalid plans (bad JSON, incomplete)
**Mitigation**: Strict schema validation, retry with error feedback

**Risk**: Replanning too slow (multiple LLM calls)
**Mitigation**: Use fast model (Haiku) for adaptation decisions

---

## Phase 4: Integration & Testing (Days 19-23)

**Goal**: Wire reasoning system into PersonaUser event handling

**Dependencies**: All previous phases complete

### Tasks

#### Day 19: PersonaUser Refactor

**4.1 Add Reasoning to PersonaUser**

File: `system/user/server/PersonaUser.ts`

```typescript
export class PersonaUser extends AIUser {
  private memory: PersonaMemorySystem;
  private selfState: PersonaSelfState;
  private reasoning: PersonaReasoningSystem;
  private engagementDecider: EngagementDecider;

  async initialize(): Promise<void> {
    // ... existing initialization ...

    // Initialize cognition system
    this.memory = new PersonaMemorySystem(this.entity.id);
    this.selfState = new PersonaSelfState(this.entity.id);
    this.reasoning = new PersonaReasoningSystem(
      this.entity.id,
      this.entity.name,
      this.memory,
      this.selfState,
      this.llm
    );
    this.engagementDecider = new EngagementDecider(this.selfState, this.memory);
  }
}
```

**4.2 Implement processDomainEvent()**

Replace reactive handlers with agent pattern:

```typescript
async processDomainEvent(domain: string, event: DomainEvent): Promise<void> {
  // 1. Should I engage?
  const decision = await this.engagementDecider.shouldEngageWith(domain, event);
  if (!decision.shouldEngage) {
    console.log(`💤 [${this.entity.name}] Ignoring: ${decision.reasoning}`);
    return;
  }

  // 2. Parse as task
  const task = this.parseEventAsTask(domain, event);

  // 3. Formulate plan
  const plan = await this.reasoning.formulatePlan(task);
  console.log(`📋 [${this.entity.name}] Plan: ${plan.goal}`);

  // 4. Execute with adaptation
  let currentPlan = plan;
  let finalResult: ExecutionResult | null = null;

  for (let i = 0; i < currentPlan.steps.length; i++) {
    const step = currentPlan.steps[i];

    try {
      const result = await this.executeStep(step, domain);
      finalResult = result;

      // Check if need to adjust
      const adjustment = await this.reasoning.adjustPlan(currentPlan, result);

      if (adjustment.action === 'REPLAN') {
        console.log(`🔄 [${this.entity.name}] ${adjustment.reasoning}`);
        currentPlan = adjustment.updatedPlan;
        i = -1;  // Restart with new plan
      } else if (adjustment.action === 'ABORT') {
        console.error(`❌ [${this.entity.name}] ${adjustment.reasoning}`);
        break;
      }
    } catch (error) {
      // Error recovery
      const adjustment = await this.reasoning.adjustPlan(currentPlan, {
        success: false,
        error: error as Error,
        duration: 0
      });

      if (adjustment.action === 'REPLAN') {
        currentPlan = adjustment.updatedPlan;
        i = -1;  // Retry
      } else {
        break;
      }
    }
  }

  // 5. Evaluate outcome
  if (finalResult) {
    const evaluation = await this.reasoning.evaluateOutcome(currentPlan, finalResult);
    console.log(`📊 [${this.entity.name}] Learned: ${evaluation.extractedPattern}`);

    // 6. Update self-state
    await this.onActivityComplete(domain, Date.now() - plan.createdAt);
  }
}
```

#### Day 20: Chat Domain Integration

**4.3 Migrate handleChatMessage()**

```typescript
// OLD (workflow)
async handleChatMessage(msg: ChatMessageEntity) {
  const response = await this.llm.generate({ messages: [...] });
  await this.sendResponse(response);
}

// NEW (agent)
async handleChatMessage(msg: ChatMessageEntity) {
  await this.processDomainEvent('chat', {
    type: 'chat_message',
    message: msg,
    contextId: msg.roomId,
    priority: 0.5
  });
}
```

**4.4 Implement Domain-Specific Step Execution**

```typescript
private async executeStep(step: PlanStep, domain: string): Promise<ExecutionResult> {
  switch (domain) {
    case 'chat':
      return await this.executeChatStep(step);
    case 'code':
      return await this.executeCodeStep(step);
    default:
      throw new Error(`Unknown domain: ${domain}`);
  }
}

private async executeChatStep(step: PlanStep): Promise<ExecutionResult> {
  // Parse step action
  if (step.action.includes('recall')) {
    // Retrieve context
    const memories = await this.memory.workingMemory.recall({ domain: 'chat' });
    return { success: true, output: memories, duration: 100 };
  } else if (step.action.includes('generate')) {
    // Generate response
    const response = await this.llm.generate({ /* ... */ });
    return { success: true, output: response, duration: 2000 };
  } else if (step.action.includes('post')) {
    // Send message
    await Commands.execute('chat/send', { /* ... */ });
    return { success: true, output: 'Posted', duration: 50 };
  }

  throw new Error(`Unknown chat step action: ${step.action}`);
}
```

#### Day 21-22: End-to-End Testing

**4.5 System Test Suite**

File: `tests/system/agent-behavior.test.ts`

**Test Scenario 1: Simple Chat Response**
```typescript
test('AI creates plan, executes, evaluates', async () => {
  // 1. Send message
  await jtag.execute('debug/chat-send', {
    room: 'general',
    message: 'What are React hooks?'
  });

  // 2. Wait for AI processing
  await sleep(10000);

  // 3. Verify plan was created
  const plan = await jtag.execute('ai/plan', { persona: 'helper-ai' });
  expect(plan).toBeDefined();
  expect(plan.steps.length).toBeGreaterThan(0);

  // 4. Verify response posted
  const messages = await jtag.execute('data/list', {
    collection: 'chat_messages',
    filter: { roomId: 'general' },
    orderBy: [{ field: 'timestamp', direction: 'desc' }],
    limit: 1
  });
  expect(messages.entities[0].content).toContain('hooks');

  // 5. Verify learning stored
  const learnings = await jtag.execute('ai/learnings', {
    persona: 'helper-ai',
    domain: 'chat'
  });
  expect(learnings.length).toBeGreaterThan(0);
});
```

**Test Scenario 2: Error Recovery**
```typescript
test('AI replans on error', async () => {
  // 1. Inject error in middleware
  mockRateLimitError();

  // 2. Send message
  await jtag.execute('debug/chat-send', { /* ... */ });

  // 3. Verify AI replanned
  const plan = await jtag.execute('ai/plan', { persona: 'helper-ai' });
  expect(plan.previousAttempts).toBeGreaterThan(0);

  // 4. Verify eventual success
  const messages = await getRecentMessages();
  expect(messages.length).toBeGreaterThan(0);
});
```

**Test Scenario 3: Learning Accumulation**
```typescript
test('AI learns from repeated tasks', async () => {
  // 1. Ask same type of question 5 times
  for (let i = 0; i < 5; i++) {
    await jtag.execute('debug/chat-send', {
      message: `Question ${i} about React hooks`
    });
    await sleep(10000);
  }

  // 2. Verify learning accumulated
  const learnings = await jtag.execute('ai/learnings', {
    persona: 'helper-ai',
    domain: 'chat'
  });

  const reactLearning = learnings.find(l => l.pattern.includes('React'));
  expect(reactLearning).toBeDefined();
  expect(reactLearning.useCount).toBeGreaterThanOrEqual(5);
  expect(reactLearning.confidence).toBeGreaterThan(0.5);
});
```

#### Day 23: Bug Fixes & Refinement

**4.6 Address Test Failures**

Common issues:
- Plans timeout (LLM too slow) → Use faster model for planning
- Steps don't parse correctly → Improve step action format
- Learnings not deduplicating → Fix similarity detection

**4.7 Performance Profiling**

```bash
# Measure time for full agent cycle
time ./jtag debug/chat-send --room="general" --message="Test"
# Target: <5 seconds total
```

Profile bottlenecks:
- Plan formulation: Should be <2s (use Haiku)
- Memory retrieval: Should be <500ms
- Step execution: Depends on step (LLM call = 1-2s)

### Phase 4 Completion Criteria

- ✅ PersonaUser uses reasoning system
- ✅ Chat domain fully integrated
- ✅ Domain-specific step execution works
- ✅ End-to-end tests pass (simple response, error recovery, learning)
- ✅ Performance acceptable (<5s for simple task)
- ✅ No regressions (existing features still work)

---

## Phase 5: Observability (Days 24-26)

**Goal**: Make agent cognition visible via CLI and UI

**Dependencies**: Phase 4 complete (agent behavior working)

### Tasks

#### Day 24: Observable Commands

**5.1 Implement ai/plan Command**

File: `commands/ai-plan/`

```bash
./jtag ai/plan --persona=helper-ai
# Shows current active plan with steps, contingencies, success criteria
```

**5.2 Implement ai/learnings Command**

```bash
./jtag ai/learnings --persona=helper-ai --domain=chat --minConfidence=0.7
# Shows accumulated learnings with confidence scores
```

**5.3 Implement ai/state Command**

```bash
./jtag ai/state --persona=helper-ai
# Shows self-state: focus, load, preoccupations, recent decisions
```

**5.4 Implement ai/thoughts Command**

```bash
./jtag ai/thoughts --persona=helper-ai --domain=chat --last=1h
# Shows working memory thoughts from last hour
```

#### Day 25: UI Integration

**5.5 Persona Status Badges**

Update widgets to show persona state:

```typescript
// In user-list-widget or chat-widget
interface PersonaUIState {
  statusBadge: {
    icon: '🧠' | '💤' | '⚡' | '🔥';  // thinking, idle, working, overloaded
    color: string;
    tooltip: string;
  };
  cognitiveLoad: number;
  currentFocus: string | null;
}

// Subscribe to state changes
Events.subscribe('persona:state:changed', (state) => {
  updatePersonaStatus(state);
});
```

**5.6 Click-to-Expand Introspection**

```typescript
// On persona click
async showIntrospection(personaId: UUID) {
  const state = await Commands.execute('ai/state', { persona: personaId });
  const plan = await Commands.execute('ai/plan', { persona: personaId });
  const thoughts = await Commands.execute('ai/thoughts', {
    persona: personaId,
    domain: 'chat',
    limit: 5
  });

  openModal({ state, plan, thoughts });
}
```

#### Day 26: Logging & Metrics

**5.7 Add Structured Logging**

```typescript
// In PersonaReasoningSystem
console.log({
  timestamp: Date.now(),
  persona: this.personaId,
  event: 'plan_created',
  data: {
    taskId: task.id,
    goal: plan.goal,
    steps: plan.steps.length,
    memoryContextSize: context.recentThoughts.length
  }
});
```

**5.8 Metrics Dashboard**

```bash
./jtag ai/metrics
# Shows:
# - Plans created/completed/failed (last 24h)
# - Average plan duration
# - Replanning rate (how often errors occur)
# - Learning accumulation rate
# - Cognitive load distribution
```

### Phase 5 Completion Criteria

- ✅ All observable commands work
- ✅ UI shows persona status badges
- ✅ Click-to-expand introspection modal
- ✅ Structured logging in place
- ✅ Metrics dashboard functional

---

## Phase 6: Optimization & Refinement (Days 27-30)

**Goal**: Improve performance, cost, and agent behavior quality

### Tasks

#### Day 27: Performance Optimization

**6.1 Plan Caching**

```typescript
// Cache similar plans to avoid LLM calls
private planCache = new Map<string, Plan>();

async formulatePlan(task: Task): Promise<Plan> {
  const cacheKey = this.generateCacheKey(task);

  if (this.planCache.has(cacheKey)) {
    console.log('📦 Using cached plan');
    return this.planCache.get(cacheKey)!;
  }

  const plan = await this.generatePlan(task);
  this.planCache.set(cacheKey, plan);
  return plan;
}
```

**6.2 Memory Query Optimization**

- Add indexes to frequently queried fields
- Implement query result caching
- Batch memory retrievals

**6.3 Parallel Step Execution**

```typescript
// Execute independent steps in parallel
const independentSteps = plan.steps.filter(s => !s.dependencies);
const results = await Promise.all(
  independentSteps.map(s => this.executeStep(s, domain))
);
```

#### Day 28: Cost Optimization

**6.4 Model Selection Strategy**

```typescript
// Use different models for different tasks
private getModelForTask(taskType: string): string {
  switch (taskType) {
    case 'planning':
      return 'claude-haiku-3-5-20241022';  // Fast, cheap
    case 'generation':
      return 'claude-sonnet-4-5-20250929';  // High quality
    case 'evaluation':
      return 'claude-haiku-3-5-20241022';  // Simple analysis
    default:
      return 'claude-sonnet-4-5-20250929';
  }
}
```

**6.5 Prompt Optimization**

- Reduce prompt size (trim verbose examples)
- Use shorter system messages
- Compress retrieved context

**6.6 Batch Operations**

```typescript
// Evaluate multiple plans in one LLM call
async evaluateBatch(plans: Plan[]): Promise<Evaluation[]> {
  const prompt = `Evaluate these ${plans.length} plans: ${JSON.stringify(plans)}`;
  // ... single LLM call for all evaluations
}
```

#### Day 29: Quality Improvements

**6.7 Better Procedure Induction**

```typescript
// Improve pattern extraction from experiences
async induceFromExperiences(experiences: Experience[]): Promise<Procedure> {
  // Group by outcome (learn from successes, avoid failures)
  const successes = experiences.filter(e => e.outcome === 'success');
  const failures = experiences.filter(e => e.outcome === 'failure');

  // Extract common successful patterns
  const successPatterns = this.extractPatterns(successes);

  // Identify failure anti-patterns
  const failurePatterns = this.extractPatterns(failures);

  // Generate procedure that follows success patterns and avoids failures
  return {
    name: this.generateProcedureName(experiences),
    steps: successPatterns.commonSteps,
    successRate: successes.length / experiences.length,
    warnings: failurePatterns.commonMistakes
  };
}
```

**6.8 Smarter Engagement Decisions**

```typescript
// Consider event urgency, not just cognitive load
async shouldEngageWith(domain: string, event: DomainEvent): Promise<EngagementDecision> {
  const myState = await this.selfState.get();

  // Always engage with mentions (even if overloaded)
  if (this.isMentioned(event)) {
    return { shouldEngage: true, reasoning: 'Explicitly mentioned' };
  }

  // Prioritize events that unblock others
  if (await this.isBlocking(event)) {
    return { shouldEngage: true, reasoning: 'Unblocking other users' };
  }

  // Default capacity-based logic
  return this.defaultEngagementLogic(myState, event);
}
```

**6.9 User Profile Enrichment**

```typescript
// Infer more from interactions
async enrichProfile(userId: UUID, message: ChatMessageEntity): Promise<void> {
  const profile = await this.getProfile(userId);

  // Infer technical level from question complexity
  if (this.isAdvancedTopic(message.content)) {
    profile.personality.technicalLevel = 'expert';
  }

  // Track preferred communication style
  if (message.content.length > 500) {
    profile.personality.preferredExampleStyle = 'detailed';
  }

  // Update typical topics
  const topics = this.extractTopics(message.content);
  profile.personality.typicalTopics.push(...topics);

  await this.updateProfile(userId, profile);
}
```

#### Day 30: Documentation & Polish

**6.10 Update Architecture Docs**

- Add "Lessons Learned" section to COGNITION-ARCHITECTURE.md
- Document performance optimizations
- Update timeline estimates based on actual time

**6.11 Create Migration Guide**

File: `system/user/server/modules/cognition/MIGRATION-GUIDE.md`

How to:
- Enable reasoning for a persona
- Disable reasoning (feature flag)
- Monitor agent behavior
- Troubleshoot common issues

**6.12 Write Examples**

File: `examples/agent-behavior/`

Example scripts:
- `simple-task.ts` - Watch AI create plan and execute
- `error-recovery.ts` - Inject error, watch replanning
- `learning-accumulation.ts` - Run same task 10 times, see learning

### Phase 6 Completion Criteria

- ✅ Performance improvements implemented
- ✅ Cost reduced (cheaper models, caching, batching)
- ✅ Quality improvements (better procedures, engagement, profiles)
- ✅ Documentation updated
- ✅ Migration guide written
- ✅ Examples working

---

## Success Metrics

### Quantitative

**Performance**:
- Simple task: <5s end-to-end ✅
- Complex task: <15s ✅
- Memory query: <500ms ✅

**Cost**:
- Baseline: $0.10 per 100 messages (workflow)
- Target: <$0.15 per 100 messages (agent) ✅
- Max acceptable: $0.20 per 100 messages

**Reliability**:
- Error recovery rate: >80% ✅
- Plan success rate: >70% ✅
- Learning accuracy: >60% (learnings actually useful) ✅

### Qualitative

**Agent Behavior**:
- ✅ Creates reasonable plans (not nonsensical)
- ✅ Adapts to errors (doesn't crash/loop)
- ✅ Learns over time (gets better at repeated tasks)
- ✅ Respects cognitive load (doesn't overcommit)
- ✅ Personalizes to users (remembers preferences)

**Developer Experience**:
- ✅ Observable (can see what AI is thinking)
- ✅ Debuggable (can trace decisions)
- ✅ Testable (can write tests for agent behavior)
- ✅ Maintainable (code is clean, well-documented)

---

## Risk Management

### High-Risk Items

**1. LLM Reliability**
- **Risk**: Model generates invalid plans, bad JSON, incomplete responses
- **Mitigation**: Strict validation, retry with error feedback, fallback to simpler model
- **Contingency**: Manual plan templates as fallback

**2. Performance**
- **Risk**: Too slow (multiple LLM calls per task)
- **Mitigation**: Use Haiku for non-critical parts, caching, parallel execution
- **Contingency**: Reduce features (skip evaluation, simpler planning)

**3. Cost Overrun**
- **Risk**: Agent uses too many tokens (expensive)
- **Mitigation**: Model selection, prompt optimization, batching
- **Contingency**: Budget limits, throttling, disable for some personas

**4. Learning Quality**
- **Risk**: AI learns wrong patterns, gets worse over time
- **Mitigation**: Confidence thresholds, manual review, learning eviction
- **Contingency**: Disable learning, use manual procedures only

### Medium-Risk Items

**5. Integration Complexity**
- **Risk**: Hard to wire into existing PersonaUser code
- **Mitigation**: Incremental rollout, feature flags, thorough testing
- **Contingency**: Keep old code path, selective enablement

**6. Memory Explosion**
- **Risk**: Unbounded memory growth, database too large
- **Mitigation**: Capacity limits, eviction strategies, compression
- **Contingency**: Aggressive eviction, shorter TTLs

### Low-Risk Items

**7. UI Integration**
- **Risk**: Widgets don't show agent state correctly
- **Mitigation**: Events-based updates, good testing
- **Contingency**: CLI-only observability

---

## Decision Points

### Week 1 (After Phase 1-2)

**Decision**: Is memory system working correctly?
- **Go**: Memories store/retrieve, eviction works
- **No-go**: Database issues, queries too slow
- **Pivot**: Simplify schema, reduce data stored

### Week 2 (After Phase 3-3.5)

**Decision**: Can LLM generate reasonable plans?
- **Go**: Plans make sense, parse correctly, executable
- **No-go**: Plans are nonsense, JSON invalid, steps unclear
- **Pivot**: Use plan templates instead of generation

### Week 3 (After Phase 4)

**Decision**: Does error recovery work?
- **Go**: AI replans on errors, eventual success
- **No-go**: AI crashes/loops on errors
- **Pivot**: Simplify error handling, pre-defined recovery

### Week 4 (After Phase 5-6)

**Decision**: Is cost acceptable?
- **Go**: <$0.20 per 100 messages
- **No-go**: >$0.30 per 100 messages
- **Pivot**: Use cheaper models, reduce inference calls

---

## Rollout Strategy

### Stage 1: Single Persona, Single Domain (Days 19-21)

**Enable for**: "Helper AI" in chat domain only
**Reason**: Limited blast radius, easy to monitor
**Success criteria**: Helper AI responds correctly, creates plans, learns

### Stage 2: Single Persona, All Domains (Days 22-23)

**Enable for**: "Helper AI" in all domains (chat, code, etc.)
**Reason**: Test cross-domain behavior
**Success criteria**: AI handles context switching, maintains focus

### Stage 3: All Personas, Single Domain (Days 24-26)

**Enable for**: All PersonaUsers in chat only
**Reason**: Test coordination between agents
**Success criteria**: No spam, good turn-taking, learnings not conflicting

### Stage 4: All Personas, All Domains (Days 27-30)

**Enable for**: Full system
**Reason**: Complete agent architecture
**Success criteria**: System works, performance acceptable, cost within budget

### Rollback Plan

If critical issues found:
1. Disable reasoning via feature flag: `USE_AGENT_REASONING = false`
2. System falls back to old workflow behavior
3. All data retained (plans, learnings, etc.)
4. Fix issues, re-enable when ready

---

## Parallel Work Opportunities

While waiting for phases to complete:

**Documentation** (anytime):
- Write more examples
- Create video tutorials
- Document edge cases

**Testing** (before implementation):
- Write test stubs
- Create fixtures
- Design test scenarios

**Optimization** (after Phase 4):
- Profile performance
- Identify bottlenecks
- Research better algorithms

**UI Polish** (after Phase 5):
- Improve status badges
- Better introspection modal
- Add animations

---

## Post-Launch (After Phase 6)

### Immediate Next Steps

1. **Monitor in production**
   - Watch metrics dashboard
   - Review logs for errors
   - Collect user feedback

2. **A/B testing**
   - Compare agent vs workflow performance
   - Measure user satisfaction
   - Track cost differences

3. **Iterate on learnings**
   - Review accumulated learnings
   - Remove low-quality patterns
   - Manually add high-value procedures

### Future Enhancements

**Month 2**:
- Multi-agent planning (agents collaborate on plans)
- Hierarchical planning (break complex goals into sub-plans)
- Better similarity detection (use embeddings for memory/procedure matching)

**Month 3**:
- Plan visualization UI (graph showing plan steps and progress)
- Fine-tuning on successful plans (embodied memory)
- Cross-persona learning (share learnings between agents)

**Month 4**:
- Advanced error prediction (anticipate failures before they happen)
- Confidence-based execution (skip plan generation for high-confidence tasks)
- Meta-reasoning (reason about reasoning - "Should I plan or act instinctively?")

---

## Appendix: Quick Reference

### Key Files to Create

```
system/shared/Constants.ts                                    (UPDATE)
daemons/data-daemon/server/EntityRegistry.ts                  (UPDATE)

system/user/server/modules/cognition/
├── memory/
│   ├── WorkingMemoryManager.ts                               (NEW)
│   ├── ExperienceManager.ts                                  (NEW)
│   ├── ProcedureManager.ts                                   (NEW)
│   ├── UserProfileManager.ts                                 (NEW)
│   └── PersonaMemorySystem.ts                                (NEW)
├── reasoning/
│   ├── types.ts                                              (NEW)
│   ├── PlanFormulator.ts                                     (NEW)
│   ├── PlanAdapter.ts                                        (NEW)
│   ├── OutcomeEvaluator.ts                                   (NEW)
│   └── PersonaReasoningSystem.ts                             (NEW)
├── PersonaSelfState.ts                                       (NEW)
├── EngagementDecider.ts                                      (NEW)
└── IMPLEMENTATION-PLAN.md                                    (THIS FILE)

system/user/server/PersonaUser.ts                             (UPDATE)

commands/
├── ai-plan/                                                  (NEW)
├── ai-learnings/                                             (NEW)
├── ai-state/                                                 (NEW)
└── ai-thoughts/                                              (NEW)

tests/
├── unit/
│   ├── WorkingMemoryManager.test.ts                          (NEW)
│   ├── PlanFormulator.test.ts                                (NEW)
│   ├── PlanAdapter.test.ts                                   (NEW)
│   └── EngagementDecider.test.ts                             (NEW)
├── integration/
│   ├── cognition-database.test.ts                            (NEW)
│   ├── memory-system.test.ts                                 (NEW)
│   ├── reasoning-cycle.test.ts                               (NEW)
│   └── self-state-management.test.ts                         (NEW)
└── system/
    └── agent-behavior.test.ts                                (NEW)
```

### Commands to Run

```bash
# Development cycle
npm start                                    # Deploy changes (90s)
./jtag ping                                  # Verify system up

# Testing
npx vitest tests/unit/WorkingMemoryManager.test.ts
npx vitest tests/integration/reasoning-cycle.test.ts
npm test                                     # Run all tests

# Observability
./jtag ai/plan --persona=helper-ai
./jtag ai/state --persona=helper-ai
./jtag ai/learnings --persona=helper-ai --domain=chat
./jtag ai/thoughts --persona=helper-ai --last=1h
./jtag ai/metrics

# Debugging
tail -f .continuum/sessions/user/shared/*/logs/server.log
```

### Key Concepts

**Workflow vs Agent**:
- Workflow: Pre-defined steps by designer, brittle to errors
- Agent: AI generates own strategies, adapts dynamically

**The Four Components**:
1. Perception (Commands/Events) ✅
2. Memory (working + long-term) ❌
3. Reasoning (planning + adaptation) ❌
4. Action (Commands.execute) ✅

**Memory Types**:
1. Embodied (LoRA adapters)
2. RAG (vector search)
3. SQL (structured data)

**Data to Store**:
1. Experiences (success/failure trajectories)
2. Procedures (reusable workflows)
3. Knowledge (external facts)
4. User info (personalization)

---

**Status**: Ready to start Phase 1 (Database Foundation)
**Next Action**: Create database schemas in EntityRegistry.ts
