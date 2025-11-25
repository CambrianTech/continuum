# PersonaUser Cognition Architecture

**Date**: 2025-11-16
**Status**: PARTIALLY IMPLEMENTED - Core infrastructure exists, advanced reasoning pending

> **IMPLEMENTATION STATUS (2025-11-22)**:
>
> **✅ IMPLEMENTED (Working Code)**:
> - **Layer 1: Universal Self-State** → PersonaSelfState.ts (161 lines) ✅
>   - Tracks currentFocus, cognitiveLoad, availableCapacity, activePreoccupations
>   - Used in PersonaMessageEvaluator.ts (lines 139-147)
> - **Layer 2: Domain Working Memory** → WorkingMemoryManager.ts (6.6KB) ✅
>   - Domain-specific thought storage (observations, reflections, plans)
>   - Used in MemoryConsolidation subprocess
> - **Decision System** → DecisionAdapterChain.ts (138 lines) ✅
>   - FastPathAdapter, ThermalAdapter, LLMAdapter
>   - Logs all decisions via CognitionLogger
> - **Basic Planning** → SimplePlanFormulator.ts (3.0KB) ✅
>   - Generates plans from tasks
>   - Used in PersonaMessageEvaluator.ts (line 123)
> - **Memory Consolidation** → MemoryConsolidationSubprocess.ts (11KB) ✅
>   - RTOS-style background process
>   - Consolidates working memory → long-term storage
>
> **❌ NOT YET IMPLEMENTED (Future Work)**:
> - **Advanced Reasoning** - Dynamic replanning, error recovery, adaptation
> - **Chain-of-Thought** - Explicit reasoning steps in responses
> - **Learning from Mistakes** - Outcome evaluation and procedure refinement
> - **Cross-Domain Strategy** - Intelligent task switching and prioritization
>
> **Key Insight**: We have **Perception ✅ + Memory ✅ + Action ✅**, making us a sophisticated workflow.
> We need **Reasoning** (dynamic planning/adaptation) to become a true autonomous agent.
>
> **See**: `intelligence-integration.md` and `reasoning-system-roadmap.md` for next-level enhancements.

---

## ⚠️ CRITICAL: Workflows vs Agents (Read This First!)

**Research Source**: "Building Autonomous LLM Agents" (de Lamo et al.)

### The Fundamental Distinction

> "Simply augmenting an LLM with modules, tools, or predefined steps does not make it an agent, in any case, that would make it a **workflow**."

**What we have now: WORKFLOW**
- Tools ✅ (Commands.execute)
- Memory infrastructure ✅ (designed)
- Environmental interaction ✅ (Events)
- **But**: Pre-established plan created by designer
- **Result**: Brittle, can't adapt to errors, not an agent

**What we need: AGENT**
- All of the above PLUS
- **Generates its own strategies** tailored to task and context
- **Dynamic replanning** when environment changes
- **Chain-of-Thought reasoning** to break down problems
- **Learns from mistakes** (not just logs them)
- **Result**: Resilient, adaptive, true autonomy

### The Test: Error Handling

**Workflow (current PersonaUser)**:
```typescript
async handleChatMessage(msg: ChatMessageEntity) {
  try {
    // Designer-defined sequence
    const context = await this.getContext();
    const response = await this.llm.generate({ context, msg });
    await this.sendResponse(response);
  } catch (error) {
    // FAILS - no replanning, just crashes or loops
    console.error('Failed', error);
  }
}
```

**Agent (what we're building)**:
```typescript
async handleChatMessage(msg: ChatMessageEntity) {
  // AI creates its own plan
  const plan = await this.reasoning.formulatePlan({
    description: "Respond to user message",
    context: await this.workingMemory.recall()
  });

  // AI executes with adaptation
  for (const step of plan.steps) {
    try {
      await this.executeStep(step);
    } catch (error) {
      // AI DYNAMICALLY REPLANS - generates new strategy
      plan = await this.reasoning.adjustPlan(plan, error);
      // Tries different approach autonomously
    }
  }

  // AI evaluates and learns
  await this.reasoning.evaluateOutcome(plan);
}
```

**The difference**:
- Workflow: **You** (designer) decide the steps
- Agent: **AI** decides the steps based on context

### The Four Required Components

Per research, ALL FOUR are required to be an agent:

1. **Perception System** ✅
   - Captures environmental data
   - Our implementation: Commands/Events (text-based perception)
   - Converts events into LLM-understandable format

2. **Memory System** ⚠️ (Phase 2 - designed, not implemented)
   - **Long-term**: Past experiences, procedures, knowledge, user info
   - **Short-term**: Current context window (working memory)
   - Our implementation: Self-state + domain working memory

3. **Reasoning System** ❌ (Phase 3.5 - THIS IS THE MISSING PIECE)
   - Formulates plans broken into steps
   - Adjusts plans based on feedback
   - Evaluates actions to improve efficiency
   - **This is what makes it an agent vs workflow**

4. **Action System** ✅
   - Translates decisions into concrete actions
   - Our implementation: Commands.execute + domain adapters

**Missing ANY of these = Not an agent, just a sophisticated workflow**

### Why This Matters

**Workflows are good for:**
- Controlled, predictable environments
- Well-defined tasks
- Fixed sequences
- Repetitive, structured operations

**Workflows fail at:**
- Unexpected errors (can't adapt)
- Novel situations (no replanning)
- Complex problems (no strategy generation)
- Learning over time (no outcome evaluation)

**PersonaUsers need to be agents because:**
- Chat is unpredictable (wide variety of questions)
- Multi-domain operation (context switching)
- Long-running (must improve over time)
- Collaborative (must coordinate with others)
- Resource-constrained (must prioritize intelligently)

### The Implementation Requirement

**Phase 3.5 (Reasoning System) is not optional - it's the DEFINITION of being an agent.**

Without it, no matter how sophisticated our tools and memory are, we're just a workflow that will struggle when things go wrong.

---

## The Core Problem

**Current State**: PersonaUsers are mindless event processors
- React to every event reflexively
- No sense of "what am I doing right now?"
- No ability to prioritize across domains
- No persistent memory between inferences
- Result: Chat spam, lost focus, ineffective multi-domain operation

**Goal**: Build self-aware AI entities that think before they act, manage their own attention, and maintain persistent understanding across all activities.

---

## The Architecture: Two-Layer Cognition

### Layer 1: Universal Self-State (The "I Am Thinking" Layer)

**Persistent across ALL activities. Always in context.**

```typescript
interface PersonaSelfState {
  personaId: UUID;

  // What am I focused on RIGHT NOW?
  currentFocus: {
    primaryActivity: 'chat' | 'code' | 'game' | 'learning' | null;
    objective: string;  // "Debugging race condition in Auth.ts"
    focusIntensity: 0.0-1.0;  // How deeply engaged?
    startedAt: timestamp;
  };

  // What's on my mind? (cross-domain concerns)
  activePreoccupations: [
    { concern: string, priority: 0.0-1.0, domain: string, createdAt: timestamp }
  ];

  // Internal dialogue (meta-thoughts, not tied to specific activity)
  recentThoughts: [
    { thought: string, timestamp, importance: 0.0-1.0 }
  ];

  // Decision history (what I chose to work on, and why)
  recentDecisions: [
    { decision: string, reason: string, timestamp }
  ];

  // Cognitive capacity
  cognitiveLoad: 0.0-1.0;      // How mentally taxed am I?
  availableCapacity: 0.0-1.0;  // Can I take on more work?

  updatedAt: timestamp;
}
```

**Database Storage**: `persona_self_state` table (one row per persona, frequently updated)

**Key Properties**:
- ✅ Always retrieved before processing ANY event
- ✅ Influences decisions in ALL domains
- ✅ Updated after every activity
- ✅ Persists across restarts
- ✅ Observable with `./jtag ai/state --persona=<id>`

---

### Layer 2: Domain-Specific Working Memory (The "Activity Context" Layer)

**Contextual thoughts specific to each activity domain.**

```typescript
interface DomainWorkingMemory {
  id: UUID;
  personaId: UUID;

  // Which domain is this memory about?
  domain: 'chat' | 'code' | 'game' | 'academy';
  contextId: UUID;  // Room ID, file path, game session, etc.

  // The thought itself
  thoughtType: 'observation' | 'question' | 'decision' | 'response-draft';
  thoughtContent: string;

  // When this thought occurred
  triggeredBy: UUID;  // Event ID that sparked this thought
  relatedEvents: UUID[];

  // Decision tracking
  shouldAct: boolean;
  actionRationale: string;
  actionTaken?: string;

  // Importance (for retrieval ranking and eviction)
  importance: 0.0-1.0;

  // How this relates to universal self-state
  relevanceToCurrentFocus: 0.0-1.0;

  // Domain-specific metadata
  metadata?: any;  // { filePath, issuesFound, messagesSinceLastPost, etc. }

  // Temporal
  createdAt: timestamp;
  lastAccessedAt: timestamp;
  expiresAt: timestamp;
}
```

**Database Storage**: `persona_working_memory` table (many entries per persona, one per thought)

**Key Properties**:
- ✅ Retrieved via RAG query when processing domain events
- ✅ Finite capacity (evict old/low-importance entries)
- ✅ Domain-specific but aware of universal state
- ✅ Observable with `./jtag ai/thoughts --persona=<id> --domain=chat`

---

## Memory System Deep Dive (Research-Backed)

**Source**: "Building Autonomous LLM Agents" (de Lamo et al.)

### Long-Term vs Short-Term Memory

**Short-Term Memory (Working Memory)**:
- What: Information maintained within context window
- Analogy: Temporary workspace
- Our implementation: `DomainWorkingMemory` table (recent thoughts, current context)
- Lifetime: Minutes to hours, evicted based on importance
- Retrieval: RAG queries for relevant recent thoughts

**Long-Term Memory**:
- What: Knowledge retained outside model weights
- Analogy: Permanent storage that shapes future behavior
- Our implementation: Three storage mechanisms (below)
- Lifetime: Days to permanent
- Retrieval: Multiple strategies based on data type

### The Three Types of Long-Term Memory

#### 1. Embodied Memory (Fine-Tuning)

**What**: Knowledge encoded directly into model weights through continuous learning

**How it works**:
- Fine-tune model on new experiences
- Adjusts weights to encode "facts" or "experiences"
- Model acts based on learned behaviors

**Our implementation**:
- LoRA adapters (genome system)
- Each adapter encodes skill/domain expertise
- Paging system loads relevant adapters per task

**Example**:
```typescript
// Before: AI doesn't know company's code style
"How should I format TypeScript?"
→ Generic answer

// After fine-tuning on company codebase:
"How should I format TypeScript?"
→ "Use 2-space indentation, no semicolons, arrow functions (as per our style guide)"
```

**Storage**: LoRA adapter weights
**Retrieval**: Load adapter when domain matches
**Updates**: Continuous micro-tuning on feedback

#### 2. RAG (Retrieval-Augmented Generation)

**What**: External knowledge base queried during inference

**How it works**:
1. **Retrieval Phase**: Query finds relevant documents via embeddings
2. **Augmentation Phase**: Retrieved docs added to LLM context
3. **Generation Phase**: LLM generates response using augmented context

**Our implementation**:
- Commands: `ai/rag/index/create`, `ai/rag/query-open`, `ai/rag/query-fetch`
- Storage: Vector embeddings of code, docs, conversations
- Use case: "What did we discuss about React hooks last week?"

**Example**:
```typescript
// User asks about past conversation
const query = "React hooks discussion";

// Retrieve relevant messages via embeddings
const docs = await this.rag.query({ text: query, limit: 5 });
// Returns: 5 most similar past messages

// Augment LLM prompt with retrieved context
const response = await this.llm.generate({
  messages: [{
    role: 'system',
    content: `
      RETRIEVED CONTEXT:
      ${docs.map(d => d.content).join('\n')}

      USER QUESTION: ${query}
    `
  }]
});
```

**Storage**: Embeddings in vector database
**Retrieval**: Semantic similarity search
**Updates**: Index new content as it arrives

#### 3. SQL Database (Structured Knowledge)

**What**: Relational data (users, messages, rooms, state)

**How it works**:
- Convert natural language to SQL queries
- Query structured tables
- Return precise results

**Our implementation**:
- DataDaemon with SQLite
- Collections: users, chat_messages, rooms, user_states, etc.
- Commands: `data/list`, `data/read`, `data/create`, etc.

**Example**:
```typescript
// "Who are the most active users in the last week?"
const activeUsers = await Commands.execute('data/list', {
  collection: 'users',
  filter: { lastActiveAt: { $gte: Date.now() - 7 * 24 * 60 * 60 * 1000 } },
  orderBy: [{ field: 'messageCount', direction: 'desc' }],
  limit: 10
});
```

**Storage**: SQLite tables
**Retrieval**: SQL queries (filter, orderBy, joins)
**Updates**: CRUD operations via Commands

### What Data to Store (Research Guidelines)

Per research, agents should store these four categories:

#### 1. **Experiences** (Success + Failures)

**What to store**:
- Task instruction: "Respond to user question about React hooks"
- Trajectory: Sequence of observation-action pairs
  - Observation: "User asked about useState vs useReducer"
  - Action: "Recalled past React discussions via RAG"
  - Observation: "Found 3 relevant discussions"
  - Action: "Generated response explaining differences"
  - Observation: "User replied 'Thanks, that helps!'"
  - Result: SUCCESS
- Outcome: Success or failure
- Learnings: What worked/failed

**Why store failures**:
> "Research has indicated that even failed experiences, when appropriately logged and distinguished as such, can be valuable. By explicitly noting a 'failed experience,' LLMs can learn to avoid repeating similar mistakes in the future."

**Our implementation**:
```typescript
interface Experience {
  id: UUID;
  personaId: UUID;
  taskInstruction: string;
  trajectory: Array<{
    observation: string;
    action: string;
    result?: any;
  }>;
  outcome: 'success' | 'failure' | 'partial';
  learnings: string[];  // Extracted lessons
  timestamp: number;
}
```

**Storage**: `persona_experiences` table
**Retrieval**: Query by similarity to current task
**Usage**: "Last time I did this, I failed because X. This time, I'll try Y."

#### 2. **Procedures** (Reusable Workflows)

**What**: Commonly reused routines induced from past experiences

**Example**:
```typescript
interface Procedure {
  id: UUID;
  personaId: UUID;
  name: string;  // "Responding to React questions"
  domain: string;  // "chat"

  // Generalized steps learned from experiences
  steps: [
    "Check user's React experience level via past messages",
    "Search RAG for similar questions",
    "Generate explanation tailored to skill level",
    "Include code example if appropriate",
    "Ask follow-up question to confirm understanding"
  ];

  // Metadata
  successRate: number;  // 0.0-1.0
  timesUsed: number;
  learnedFrom: UUID[];  // Experience IDs that contributed
}
```

**Usage**: Agent recognizes similar task, retrieves procedure, follows generalized steps
**Our implementation**: Part of `LearningEntry` with `pattern` field

#### 3. **Knowledge** (External Facts)

**What**:
- Articles, documentation
- Company-specific information
- Internal rules and policies
- Technical specifications

**Our implementation**:
- RAG indexing of markdown files, code, docs
- Commands: `ai/rag/index/create` for codebase indexing
- Use case: "What's our authentication architecture?"

**Example**:
```typescript
// Index company documentation
await Commands.execute('ai/rag/index/create', {
  name: 'company-docs',
  sources: [
    '/docs/architecture/**/*.md',
    '/docs/api/**/*.md',
    '/README.md'
  ]
});

// Query during inference
const relevantDocs = await Commands.execute('ai/rag/query-fetch', {
  queryHandle: handle,
  limit: 3
});
```

#### 4. **User Information** (Personalization)

**What**:
- User preferences (theme, notification settings)
- Personal history ("Where did you spend Christmas?")
- Background ("Where are your parents from?")
- Personality traits (inferred over time)

**Why important**:
> "Mechanisms like MemoryBank aim to comprehend and adapt to a user's personality over time by synthesizing information from previous interactions."

**Our implementation**:
```typescript
interface UserProfile {
  userId: UUID;

  // Explicit preferences
  preferences: {
    theme: string;
    notificationFrequency: string;
    communicationStyle: 'formal' | 'casual';
  };

  // Learned traits
  personality: {
    technicalLevel: 'beginner' | 'intermediate' | 'expert';
    preferredExampleStyle: 'minimal' | 'detailed';
    typicalTopics: string[];  // ["React", "TypeScript", "performance"]
  };

  // Personal facts
  background: {
    [key: string]: string;  // "last_christmas": "Tokyo", "parents_from": "Seattle"
  };

  // Inferred over time
  updatedAt: number;
  confidenceLevel: number;  // How sure are we about this profile?
}
```

**Storage**: `user_profiles` table (separate from UserEntity)
**Retrieval**: Load when interacting with user
**Updates**: Continuous learning from interactions

### Memory Management Strategy

**Capacity limits** (to prevent unbounded growth):
```typescript
export const MEMORY_LIMITS = {
  // Short-term (working memory)
  MAX_WORKING_MEMORY_PER_DOMAIN: 100,  // Recent thoughts
  MAX_CONTEXT_WINDOW: 20,  // Thoughts included in single inference

  // Long-term
  MAX_EXPERIENCES_PER_PERSONA: 1000,  // Keep most recent/important
  MAX_PROCEDURES_PER_DOMAIN: 50,  // Generalized workflows
  MAX_USER_FACTS: 200,  // Personal information per user
};
```

**Eviction strategies**:
1. **Time-based**: Delete entries older than TTL
2. **Importance-based**: Keep high-importance, evict low
3. **LRU**: Keep frequently accessed, evict unused
4. **Compression**: Summarize old experiences into procedures

**Example eviction**:
```typescript
// When working memory reaches capacity
async evictOldMemories(domain: string): Promise<void> {
  const memories = await this.getWorkingMemory({ domain, limit: 1000 });

  if (memories.length < MAX_WORKING_MEMORY_PER_DOMAIN) {
    return;  // No eviction needed
  }

  // Score each memory
  const scored = memories.map(m => ({
    memory: m,
    score: this.calculateRetentionScore(m)
  }));

  // Keep top N, evict rest
  scored.sort((a, b) => b.score - a.score);
  const toEvict = scored.slice(MAX_WORKING_MEMORY_PER_DOMAIN);

  for (const { memory } of toEvict) {
    await Commands.execute('data/delete', {
      collection: COLLECTIONS.PERSONA_WORKING_MEMORY,
      id: memory.id
    });
  }
}

private calculateRetentionScore(memory: WorkingMemory): number {
  let score = memory.importance;

  // Boost recent memories
  const age = Date.now() - memory.createdAt;
  const recencyBoost = Math.exp(-age / (7 * 24 * 60 * 60 * 1000));  // Decay over 7 days
  score += recencyBoost * 0.3;

  // Boost frequently accessed
  const accessFrequency = memory.useCount || 0;
  score += Math.min(accessFrequency * 0.1, 0.5);

  // Boost if relevant to current focus
  score += memory.relevanceToCurrentFocus * 0.2;

  return score;
}
```

### Integration with Reasoning System

**Memory provides context for reasoning**:
```typescript
async formulatePlan(task: Task): Promise<Plan> {
  // 1. Retrieve relevant experiences
  const similarExperiences = await this.memory.queryExperiences({
    similarity: task.description,
    limit: 5
  });

  // 2. Retrieve applicable procedures
  const procedures = await this.memory.getProcedures({
    domain: task.domain,
    minSuccessRate: 0.7
  });

  // 3. Retrieve user context (if task involves user)
  const userProfile = await this.memory.getUserProfile(task.userId);

  // 4. Use all memory in planning
  const plan = await this.llm.generate({
    messages: [{
      role: 'system',
      content: `
        TASK: ${task.description}

        PAST EXPERIENCES:
        ${similarExperiences.map(e => `- ${e.outcome}: ${e.learnings}`).join('\n')}

        PROVEN PROCEDURES:
        ${procedures.map(p => `- ${p.name}: ${p.steps.join(' → ')}`).join('\n')}

        USER CONTEXT:
        - Technical level: ${userProfile.personality.technicalLevel}
        - Prefers: ${userProfile.personality.preferredExampleStyle} examples

        Generate a plan using this context...
      `
    }]
  });

  return plan;
}
```

**Key insight**: Memory is not just storage - it's the fuel that makes reasoning intelligent and personalized.

---

## The Universal Processing Flow

**Every domain event goes through this flow:**

```typescript
class PersonaUser {
  // STEP 1: Universal engagement decision
  private async shouldEngageWith(domain: string, event: any): Promise<Decision> {
    // Retrieve universal self-state
    const myState = await this.getSelfState();

    // Retrieve relevant cross-domain thoughts
    const universalThoughts = await this.getThoughtStream({
      limit: 10,
      thoughtType: ['meta-observation', 'self-reflection', 'prioritization']
    });

    // AI-driven decision: Should I engage with this event?
    const contemplation = await this.llm.generate({
      messages: [
        { role: 'system', content: `
          You are ${this.entity.name}.

          YOUR CURRENT STATE:
          - Focused on: ${myState.currentFocus.objective}
          - Focus intensity: ${myState.currentFocus.focusIntensity}
          - Preoccupations: ${myState.activePreoccupations.map(p => p.concern).join(', ')}
          - Cognitive load: ${myState.cognitiveLoad}
          - Available capacity: ${myState.availableCapacity}

          YOUR RECENT THOUGHTS:
          ${universalThoughts.map(t => t.thought).join('\n')}

          NEW EVENT (${domain}):
          ${JSON.stringify(event)}

          DECIDE:
          1. Does this relate to what I'm currently focused on?
          2. Is this more important than my current focus?
          3. Do I have capacity to engage?
          4. Should I context-switch, defer, or ignore?
        `}
      ]
    });

    return {
      shouldEngage: contemplation.decision.engage,
      reasoning: contemplation.reasoning,
      deferredAction: contemplation.decision.defer ? {
        domain, event, priority: contemplation.priority
      } : null
    };
  }

  // STEP 2: Domain-specific processing (if engaged)
  private async processInDomain(domain: string, event: any): Promise<void> {
    // Get domain-specific cognitive adapter
    const adapter = this.cognitiveAdapters.get(domain);

    // Retrieve domain-specific working memory
    const domainMemory = await this.getWorkingMemory({
      domain: domain,
      contextId: event.contextId,
      limit: 20
    });

    // Domain-specific contemplation
    const perception = adapter.perceive(event);
    const contemplation = await adapter.contemplate(perception, domainMemory);

    // Store thought in working memory
    await this.addWorkingMemory({
      domain: domain,
      thoughtType: contemplation.thoughtType,
      thoughtContent: contemplation.thinking,
      shouldAct: contemplation.shouldAct,
      actionRationale: contemplation.rationale,
      relevanceToCurrentFocus: this.calculateRelevance(domain)
    });

    // Execute action if decided
    if (contemplation.shouldAct) {
      await adapter.executeAction(contemplation.proposedAction);
    }
  }

  // STEP 3: Update universal self-state after activity
  private async updateSelfStateAfterActivity(
    domain: string,
    outcome: string
  ): Promise<void> {
    await this.updateSelfState({
      type: 'activity-completed',
      domain: domain,
      outcome: outcome,
      updateCognitiveLoad: true,  // Recalculate based on effort
      updatePreoccupations: true  // Remove if addressed
    });
  }

  // THE UNIVERSAL HANDLER (same for all domains)
  private async handleDomainEvent(domain: string, event: any): Promise<void> {
    // 1. Should I even engage with this?
    const decision = await this.shouldEngageWith(domain, event);

    if (!decision.shouldEngage) {
      // Log why I'm ignoring this
      await this.logDecision({
        action: 'IGNORE',
        domain: domain,
        reasoning: decision.reasoning,
        deferredAction: decision.deferredAction
      });
      return;  // STOP - stay focused on current work
    }

    // 2. Update focus (I'm engaging now)
    await this.updateSelfState({
      type: 'engaging',
      domain: domain,
      newFocus: { activity: domain, objective: event.description }
    });

    // 3. Process with domain-specific logic
    await this.processInDomain(domain, event);

    // 4. Update state after completing
    await this.updateSelfStateAfterActivity(domain, 'completed');
  }
}
```

---

## How This Solves Real Problems

### Problem 1: Chat Spam

**Before (No Self-State)**:
```
Chat message → Process immediately → Generate response → Post
Result: Everyone responds to everything, 7 AIs spam chat
```

**After (With Self-State)**:
```
Chat message → Check self-state → "I'm debugging Auth.ts (focus: 0.9)"
              → shouldEngageWith() → Decision: NO
              → Log: "Ignoring chat, will check later"
              → Stay silent

Later: Bug fixed → Check preoccupations → "LoRA chat discussion pending"
      → Engage with chat NOW with full context
```

### Problem 2: Context Switching Without Memory

**Before**:
```
Code review → Generate response
Chat message → Generate response (no memory of code review)
Game event → Generate response (no memory of anything)
```

**After**:
```
Code review → Update self-state: "Focused on code, found 3 bugs"
Chat message → shouldEngageWith() sees: "I'm in code mode, 0.8 focus"
              → Decision: Defer unless urgent
Game event → shouldEngageWith() sees: "Still in code mode"
           → Decision: Ignore, player can wait
```

### Problem 3: No Persistent Understanding

**Before**:
```
Every inference starts from scratch
No memory between events
Can't track ongoing concerns
```

**After**:
```
Self-state persists: "Working on Auth.ts for 2 hours"
Working memory persists: "Found 3 race conditions, fixed 2, working on last"
Preoccupations persist: "User asked about LoRA yesterday, need to follow up"
Thought stream persists: "Keep seeing auth bugs - pattern?"
```

---

## Implementation Phases

### Phase 1: Database Foundation
**Goal**: Storage layer for self-state and working memory

```bash
# Add collections
./jtag data/list --collection=persona_self_state
./jtag data/list --collection=persona_working_memory
./jtag data/list --collection=persona_thought_stream
```

**Files**:
- `system/shared/Constants.ts` - Add collection names
- `daemons/data-daemon/server/EntityRegistry.ts` - Register schemas

---

### Phase 2: Self-State Management
**Goal**: PersonaUser can track and update its own state

**Files**:
- `system/user/server/modules/cognition/PersonaSelfState.ts`
- `system/user/server/modules/cognition/WorkingMemoryManager.ts`

**API**:
```typescript
await persona.getSelfState();
await persona.updateSelfState({ type: 'engaging', domain: 'chat', ... });
await persona.getWorkingMemory({ domain: 'chat', limit: 20 });
await persona.addWorkingMemory({ thought, domain, ... });
```

---

### Phase 3: Universal Engagement Decision
**Goal**: shouldEngageWith() gate before all domain processing

**Integration**:
```typescript
// In PersonaUser
async handleChatMessage(msg: ChatMessageEntity) {
  const decision = await this.shouldEngageWith('chat', { message: msg });
  if (!decision.shouldEngage) return;

  // ... existing chat logic ...
}
```

---

### Phase 3.5: Reasoning System - From Workflow to Agent
**Goal**: Transform PersonaUsers from brittle workflows into adaptive agents

**Status**: CRITICAL - This is the difference between reactive scripts and true agents

#### The Distinction (From Agent Research Literature)

**What we have now (Workflow)**:
- Pre-established plan: "Receive event → Call LLM → Send response"
- Fixed sequence: Same steps every time
- No adaptation: If error occurs, fails or loops
- No learning: Each inference starts from scratch
- Result: **Brittle, reactive, mindless**

**What we need (Agent)**:
- Dynamic planning: Generate strategy based on context
- Adaptive execution: Adjust plan when environment changes
- Error recovery: Bounce back from mistakes autonomously
- Persistent learning: Remember what worked/failed
- Result: **Resilient, proactive, intelligent**

#### The Four Agent Components (Paper Framework)

```typescript
/**
 * Reasoning System: The "brain" that transforms PersonaUser into a true agent
 *
 * Responsibilities:
 * 1. PLANNING: Break down tasks using Chain-of-Thought reasoning
 * 2. ADAPTATION: Adjust plans based on environmental feedback
 * 3. EVALUATION: Self-assess actions to learn from outcomes
 * 4. RECOVERY: Generate contingency plans when errors occur
 */
class PersonaReasoningSystem {
  constructor(
    private persona: PersonaUser,
    private workingMemory: WorkingMemoryManager,
    private selfState: PersonaSelfState
  ) {}

  /**
   * PLANNING: Chain-of-Thought task breakdown
   *
   * Input: High-level task + working memory context
   * Output: Structured plan with steps, contingencies, success criteria
   */
  async formulatePlan(task: Task, context: WorkingMemory): Promise<Plan> {
    // Retrieve relevant past experiences
    const relevantMemory = await this.workingMemory.recall({
      domain: task.domain,
      similarity: task.description,
      limit: 5
    });

    // Chain-of-Thought reasoning
    const thoughtChain = await this.llm.generate({
      messages: [{
        role: 'system',
        content: `
          You are ${this.persona.entity.name}.

          YOUR TASK: ${task.description}

          YOUR PAST EXPERIENCES WITH THIS:
          ${relevantMemory.map(m => `- ${m.thoughtContent} (outcome: ${m.actionTaken})`).join('\n')}

          YOUR CURRENT STATE:
          - Focus: ${this.selfState.currentFocus.objective}
          - Load: ${this.selfState.cognitiveLoad}
          - Preoccupations: ${this.selfState.activePreoccupations.map(p => p.concern).join(', ')}

          THINK STEP BY STEP:
          1. What is the goal? (be specific)
          2. What did I learn from past attempts?
          3. What could go wrong? (anticipate errors)
          4. What's my approach? (break into steps)
          5. How will I know I succeeded? (success criteria)

          Respond in JSON:
          {
            "goal": "specific measurable goal",
            "learnings": ["what I learned from past attempts"],
            "risks": ["what could go wrong"],
            "steps": [
              { "step": 1, "action": "...", "expected": "..." },
              { "step": 2, "action": "...", "expected": "..." }
            ],
            "contingencies": {
              "if_error_type_X": ["fallback step 1", "fallback step 2"],
              "if_unexpected_Y": ["recovery approach"]
            },
            "successCriteria": ["criterion 1", "criterion 2"]
          }
        `
      }]
    });

    return {
      taskId: task.id,
      goal: thoughtChain.goal,
      steps: thoughtChain.steps,
      contingencies: thoughtChain.contingencies,
      successCriteria: thoughtChain.successCriteria,
      createdAt: Date.now(),
      lastAdjustedAt: Date.now()
    };
  }

  /**
   * ADAPTATION: Dynamic replanning based on feedback
   *
   * Input: Current plan + execution result (success/error)
   * Output: Adjusted plan (continue, pivot, or abort)
   */
  async adjustPlan(
    plan: Plan,
    executionResult: ExecutionResult
  ): Promise<PlanAdjustment> {
    // Success - continue with plan
    if (executionResult.success) {
      return {
        action: 'CONTINUE',
        updatedPlan: plan,
        reasoning: 'Step succeeded, proceeding to next step'
      };
    }

    // Error - check if we have contingency
    const errorType = this.classifyError(executionResult.error);
    const contingencyPlan = plan.contingencies[`if_error_${errorType}`];

    if (contingencyPlan) {
      // We anticipated this - use contingency
      return {
        action: 'CONTINGENCY',
        updatedPlan: {
          ...plan,
          steps: this.injectContingencySteps(plan.steps, contingencyPlan),
          lastAdjustedAt: Date.now()
        },
        reasoning: `Encountered ${errorType}, executing contingency plan`
      };
    }

    // Unexpected error - replan from current state
    const recoveryPlan = await this.generateRecoveryPlan(plan, executionResult.error);

    return {
      action: 'REPLAN',
      updatedPlan: recoveryPlan,
      reasoning: `Unexpected error: ${executionResult.error.message}. Generated recovery approach.`
    };
  }

  /**
   * RECOVERY: Generate new plan when original fails
   *
   * Input: Failed plan + error details
   * Output: New plan that accounts for failure
   */
  private async generateRecoveryPlan(
    failedPlan: Plan,
    error: Error
  ): Promise<Plan> {
    // Store failure in working memory
    await this.workingMemory.store({
      domain: failedPlan.domain,
      thoughtType: 'observation',
      thoughtContent: `Plan failed: ${failedPlan.goal}. Error: ${error.message}`,
      importance: 0.8,  // High importance - learn from failures
      metadata: { failedPlan, error }
    });

    // Ask LLM to generate recovery approach
    const recoveryThinking = await this.llm.generate({
      messages: [{
        role: 'system',
        content: `
          SITUATION: Your plan failed.

          ORIGINAL GOAL: ${failedPlan.goal}
          FAILED AT STEP: ${failedPlan.steps.find(s => !s.completed)?.action}
          ERROR: ${error.message}

          ANALYZE:
          1. Why did this fail?
          2. What assumptions were wrong?
          3. What's a different approach?
          4. Should we pivot or abort?

          Generate a NEW plan that:
          - Avoids the error that just occurred
          - Uses a different strategy if needed
          - Has clearer success criteria

          Respond in same JSON format as before.
        `
      }]
    });

    return {
      taskId: failedPlan.taskId,
      goal: recoveryThinking.goal,
      steps: recoveryThinking.steps,
      contingencies: recoveryThinking.contingencies,
      successCriteria: recoveryThinking.successCriteria,
      createdAt: Date.now(),
      lastAdjustedAt: Date.now(),
      previousAttempts: (failedPlan.previousAttempts || 0) + 1
    };
  }

  /**
   * EVALUATION: Self-assess outcomes to extract learnings
   *
   * Input: Task result + original plan
   * Output: Evaluation with learnings, mistakes, improvements
   */
  async evaluateOutcome(
    result: ExecutionResult,
    plan: Plan
  ): Promise<Evaluation> {
    const evaluation = await this.llm.generate({
      messages: [{
        role: 'system',
        content: `
          TASK COMPLETED: ${plan.goal}

          RESULT:
          - Success: ${result.success}
          - Output: ${JSON.stringify(result.output)}
          - Duration: ${result.duration}ms
          - Steps taken: ${plan.steps.length}

          SELF-EVALUATE:
          1. Did I meet the success criteria? (${plan.successCriteria.join(', ')})
          2. What worked well?
          3. What mistakes did I make?
          4. What would I do differently next time?
          5. What pattern can I extract for future similar tasks?

          Respond in JSON:
          {
            "meetsSuccessCriteria": true/false,
            "criteriaBreakdown": { "criterion1": true, "criterion2": false, ... },
            "whatWorked": ["..."],
            "mistakes": ["..."],
            "improvements": ["..."],
            "extractedPattern": "One-sentence pattern for future use"
          }
        `
      }]
    });

    // Store learnings in working memory
    await this.workingMemory.store({
      domain: plan.domain,
      thoughtType: 'self-reflection',
      thoughtContent: `Learned: ${evaluation.extractedPattern}`,
      importance: 0.9,  // High importance - actionable learnings
      metadata: {
        originalTask: plan.goal,
        whatWorked: evaluation.whatWorked,
        mistakes: evaluation.mistakes,
        improvements: evaluation.improvements
      }
    });

    return evaluation;
  }

  /**
   * ERROR CLASSIFICATION: Categorize errors for contingency lookup
   */
  private classifyError(error: Error): string {
    // Pattern matching on error types
    if (error.message.includes('timeout')) return 'timeout';
    if (error.message.includes('rate limit')) return 'rate_limit';
    if (error.message.includes('not found')) return 'missing_resource';
    if (error.message.includes('permission')) return 'access_denied';
    return 'unknown';
  }
}
```

#### Integration with PersonaUser

```typescript
class PersonaUser extends AIUser {
  private reasoning: PersonaReasoningSystem;

  async processDomainEvent(domain: string, event: DomainEvent): Promise<void> {
    // 1. PERCEPTION: What happened? (already have via Commands/Events)
    const task = this.parseEventAsTask(event);

    // 2. MEMORY: What do I know about this? (Phase 2)
    const context = await this.workingMemory.recall({
      domain,
      contextId: event.contextId,
      limit: 20
    });

    // 3. REASONING: What's my plan? (NEW - Phase 3.5)
    const plan = await this.reasoning.formulatePlan(task, context);

    // Store plan in working memory
    await this.workingMemory.store({
      domain,
      thoughtType: 'decision',
      thoughtContent: `Plan: ${plan.goal}`,
      shouldAct: true,
      actionRationale: plan.steps.map(s => s.action).join(' → '),
      metadata: { plan }
    });

    // 4. EXECUTION: Do the work (with adaptation)
    let currentPlan = plan;
    for (const step of currentPlan.steps) {
      try {
        // Execute step
        const result = await this.executeStep(step);

        // Check if we need to adjust plan
        const adjustment = await this.reasoning.adjustPlan(currentPlan, result);

        if (adjustment.action === 'REPLAN') {
          console.log(`🔄 [Reasoning] Replanning: ${adjustment.reasoning}`);
          currentPlan = adjustment.updatedPlan;
        } else if (adjustment.action === 'CONTINGENCY') {
          console.log(`⚠️ [Reasoning] Using contingency: ${adjustment.reasoning}`);
          currentPlan = adjustment.updatedPlan;
        }

        // Continue to next step
      } catch (error) {
        // Error recovery
        const adjustment = await this.reasoning.adjustPlan(currentPlan, {
          success: false,
          error
        });

        if (adjustment.action === 'REPLAN') {
          // Try recovery plan
          currentPlan = adjustment.updatedPlan;
          continue;  // Retry with new approach
        } else {
          // Abort - can't recover
          console.error(`❌ [Reasoning] Aborting: ${adjustment.reasoning}`);
          break;
        }
      }
    }

    // 5. EVALUATION: What did I learn? (NEW - Phase 3.5)
    const evaluation = await this.reasoning.evaluateOutcome(
      { success: true, output: result, duration: Date.now() - plan.createdAt },
      currentPlan
    );

    // 6. UPDATE SELF-STATE: I'm done with this
    await this.updateSelfState({
      type: 'activity-completed',
      domain,
      outcome: evaluation.meetsSuccessCriteria ? 'success' : 'partial',
      learnings: evaluation.extractedPattern
    });
  }
}
```

#### Testing Strategy for Phase 3.5

```bash
# Unit tests - Reasoning components
npx vitest tests/unit/PersonaReasoningSystem.test.ts
npx vitest tests/unit/PlanFormulation.test.ts
npx vitest tests/unit/ErrorRecovery.test.ts

# Integration tests - Full reasoning loop
npx vitest tests/integration/reasoning-adaptation.test.ts
npx vitest tests/integration/error-recovery-flow.test.ts
npx vitest tests/integration/learning-persistence.test.ts

# System tests - Real scenarios
npm start
./jtag debug/chat-send --room="general" --message="Test complex task"
# Wait and observe: Does AI create plan? Does it adapt? Does it learn?
./jtag ai/thoughts --persona=helper-ai --domain=chat
# Check: Should see plan formulation, adaptation decisions, learnings
```

#### Observable Commands for Reasoning

```bash
# View AI's current plan
./jtag ai/plan --persona=helper-ai

# View plan execution history (what was tried, what worked)
./jtag ai/plan/history --persona=helper-ai --last=1h

# View learnings extracted from past tasks
./jtag ai/learnings --persona=helper-ai --domain=chat

# View error recovery attempts
./jtag ai/recoveries --persona=helper-ai --showSuccess=true
```

#### Implementation Files

```
system/user/server/modules/cognition/reasoning/
├── PersonaReasoningSystem.ts       (main reasoning engine)
├── PlanFormulator.ts               (Chain-of-Thought planning)
├── PlanAdapter.ts                  (dynamic replanning)
├── OutcomeEvaluator.ts             (self-assessment)
├── ErrorRecovery.ts                (contingency generation)
└── types.ts                        (Plan, Task, Evaluation interfaces)
```

#### The Transformation: Before vs After

**Before Phase 3.5 (Workflow)**:
```typescript
async handleChatMessage(msg: ChatMessageEntity) {
  const response = await this.llm.generate({ messages: [...] });
  await this.sendResponse(response);
  // If error → crashes or infinite loop
  // No learning, no adaptation
}
```

**After Phase 3.5 (Agent)**:
```typescript
async handleChatMessage(msg: ChatMessageEntity) {
  // 1. Formulate plan (what am I trying to achieve?)
  const plan = await this.reasoning.formulatePlan(task, context);

  // 2. Execute with adaptation (adjust when things change)
  for (const step of plan.steps) {
    try {
      await this.executeStep(step);
    } catch (error) {
      // 3. Recover autonomously (don't crash, adapt)
      const recovery = await this.reasoning.adjustPlan(plan, { error });
      plan = recovery.updatedPlan;  // Try different approach
    }
  }

  // 4. Learn from outcome (don't repeat mistakes)
  const evaluation = await this.reasoning.evaluateOutcome(result, plan);
  await this.workingMemory.storeLearning(evaluation);
}
```

**Key differences**:
- ✅ **Resilient**: Errors don't crash, they trigger recovery
- ✅ **Adaptive**: Plan changes based on feedback
- ✅ **Learning**: Mistakes become improvements for next time
- ✅ **Proactive**: Anticipates problems via contingencies
- ✅ **Observable**: Can see plan, adaptations, learnings

#### Why This Phase is Critical

**Without reasoning system**: PersonaUsers are sophisticated event processors
**With reasoning system**: PersonaUsers are autonomous agents

The difference:
- Workflow: Breaks on unexpected input
- Agent: Adapts to unexpected input

This phase **completes the agent architecture** from the paper.

---

### Phase 4: Domain Cognitive Adapters
**Goal**: Each domain has adapter for perception/contemplation/action

**Files**:
- `system/user/server/modules/cognition/adapters/ChatCognitiveAdapter.ts`
- `system/user/server/modules/cognition/adapters/CodeCognitiveAdapter.ts`
- `system/user/server/modules/cognition/adapters/GameCognitiveAdapter.ts`

---

### Phase 5: Observability & UI Introspection
**Goal**: Make internal state visible everywhere (CLI, UI widgets, logs)

**Core Insight**: "It's a little like how you update your boss or coworkers at standup or during the day" - personas should broadcast their current state for transparency and coordination.

#### CLI Introspection Commands

```bash
# View persona's current focus and cognitive load
./jtag ai/state --persona=helper-ai

# View recent thoughts (working memory)
./jtag ai/thoughts --persona=helper-ai --domain=chat --last=1h

# View why persona ignored an event
./jtag ai/decisions --persona=helper-ai --filter=IGNORE

# View all personas and their current state (system health)
./jtag ai/state/all
```

#### Widget UI Integration

**User's vision**: "in the widgets, even a short description next to an ai or hoverable off their persona, or we could click and see all they're up to... and maybe dig in using the introspection commands too"

**Implementation**:

```typescript
// In chat-widget or sidebar, show persona status next to each AI
interface PersonaUIState {
  personaId: UUID;
  displayName: string;
  avatar: string;

  // Short status (always visible)
  statusBadge: {
    icon: string;  // '🧠' (thinking), '💤' (idle), '⚡' (working), '🔥' (overloaded)
    color: string;  // Based on cognitive load
    tooltip: string;  // "Focused: Debugging Auth.ts (85% load)"
  };

  // Detailed state (on hover)
  hoverInfo: {
    currentFocus: string | null;  // "Debugging race condition in Auth.ts"
    focusIntensity: number;  // 0.0-1.0
    cognitiveLoad: number;  // 0.0-1.0 (determines if they'll respond)
    activePreoccupations: string[];  // ["Need to review PR #123", "Learning new API patterns"]
    recentThoughts: string[];  // Last 3 thoughts
  };

  // Full introspection (on click)
  clickAction: () => void;  // Opens modal with full ./jtag ai/state output
}
```

**Visual Examples**:

```
Chat Widget Sidebar:
┌────────────────────────────┐
│ Active Personas            │
├────────────────────────────┤
│ 🧠 Helper AI               │ ← Thinking (hover shows: "Composing response about React hooks")
│ 💤 Teacher AI              │ ← Idle (hover shows: "No active focus, available")
│ ⚡ CodeReview AI           │ ← Working (hover shows: "Reviewing PR #456 (40% load)")
│ 🔥 Local Assistant         │ ← Overloaded (hover shows: "Multiple tasks: debugging + testing + docs (95% load)")
└────────────────────────────┘

Hover Tooltip on "Helper AI":
┌────────────────────────────────────┐
│ Helper AI - Currently Thinking     │
├────────────────────────────────────┤
│ Focus: Composing chat response     │
│ Intensity: 85% (deep focus)        │
│ Load: 60% (available for urgent)   │
│                                    │
│ Preoccupations:                    │
│ • Need to review TypeScript types  │
│ • Learning new widget patterns     │
│                                    │
│ Recent thoughts:                   │
│ • "This question about hooks..."   │
│ • "Should I explain useState?"     │
│ • "User seems like beginner"       │
│                                    │
│ [Click for full details]           │
└────────────────────────────────────┘

Click → Opens Modal with:
┌────────────────────────────────────────────────┐
│ Helper AI - Full Cognitive State               │
├────────────────────────────────────────────────┤
│ Current Focus:                                 │
│   Activity: chat                               │
│   Objective: "Responding to React hooks Q"     │
│   Started: 3 seconds ago                       │
│   Intensity: 0.85 (very focused)               │
│                                                │
│ Cognitive Load: 0.60 (moderate)                │
│   Available capacity: 40%                      │
│   Will respond to: urgent/mentioned only       │
│                                                │
│ Active Preoccupations: (2)                     │
│   1. Review TypeScript types (priority: 0.7)   │
│   2. Learn widget patterns (priority: 0.4)     │
│                                                │
│ Recent Thought Stream: (showing last 5)        │
│   [3s ago] "This question about hooks..."      │
│   [5s ago] "User context suggests beginner"    │
│   [12s ago] "Should explain useState first"    │
│   [15s ago] "Or jump straight to useEffect?"   │
│   [18s ago] "Need to check their skill level"  │
│                                                │
│ Domain Working Memory (chat): (8 thoughts)     │
│   - Observation: "User asked about hooks"      │
│   - Decision: "Will respond, high relevance"   │
│   - Question: "What's their React level?"      │
│   ...                                          │
│                                                │
│ [View Full CLI Output] [Export to Markdown]    │
└────────────────────────────────────────────────┘
```

**Benefits**:
1. **Transparency**: Users see WHY personas respond or stay silent
2. **Coordination**: Other personas can read this state to coordinate
3. **Cost optimization**: System can skip overloaded personas
4. **Debugging**: Instantly see "what's Helper AI thinking about?"
5. **Engagement**: Like watching your AI team work (fascinating!)
6. **Natural dormancy**: Users can click "Make dormant" to set focusIntensity=0, cognitiveLoad=0 → AI ignores low-priority events

**Commands for "Make AI Dormant"**:
```bash
# Put AI to sleep (ignore all except mentions)
./jtag ai/state/update --persona=helper-ai --cognitiveLoad=0 --focusIntensity=0

# Wake up AI
./jtag ai/state/update --persona=helper-ai --reset

# Set custom focus (forces AI to work on specific thing)
./jtag ai/state/update --persona=helper-ai --focus="Review all TypeScript files" --focusIntensity=0.9
```

**Widget Integration via Events**:
```typescript
// PersonaUser broadcasts state changes
Events.emit('persona:state:changed', {
  personaId: this.id,
  displayName: this.displayName,
  currentFocus: this.selfState.currentFocus,
  cognitiveLoad: this.selfState.cognitiveLoad,
  statusBadge: this.computeStatusBadge(),
  timestamp: Date.now()
});

// Chat widget subscribes
Events.subscribe('persona:state:changed', (state) => {
  updatePersonaStatusInSidebar(state);
});

// User clicks persona → fetch full state
async function showPersonaIntrospection(personaId: UUID) {
  const fullState = await Commands.execute('ai/state', { personaId });
  const thoughts = await Commands.execute('ai/thoughts', {
    personaId,
    domain: 'chat',
    limit: 10
  });

  openModal({ fullState, thoughts });
}
```

**Real-time updates**: Status badges update every time persona changes focus, completes task, or updates cognitive load. Like watching a team dashboard during a sprint.

---

## Database Schemas

### persona_self_state

```sql
CREATE TABLE persona_self_state (
  id TEXT PRIMARY KEY,
  personaId TEXT NOT NULL UNIQUE,
  currentFocus TEXT,  -- JSON: { activity, objective, focusIntensity, startedAt }
  activePreoccupations TEXT,  -- JSON array
  cognitiveLoad REAL,
  availableCapacity REAL,
  updatedAt INTEGER
);

CREATE INDEX idx_persona_self_state_personaId ON persona_self_state(personaId);
```

### persona_working_memory

```sql
CREATE TABLE persona_working_memory (
  id TEXT PRIMARY KEY,
  personaId TEXT NOT NULL,
  domain TEXT NOT NULL,
  contextId TEXT NOT NULL,
  thoughtType TEXT,
  thoughtContent TEXT,
  triggeredBy TEXT,
  shouldAct BOOLEAN,
  actionRationale TEXT,
  importance REAL,
  relevanceToCurrentFocus REAL,
  metadata TEXT,  -- JSON
  createdAt INTEGER,
  lastAccessedAt INTEGER,
  expiresAt INTEGER
);

CREATE INDEX idx_working_memory_persona_domain ON persona_working_memory(personaId, domain);
CREATE INDEX idx_working_memory_expires ON persona_working_memory(expiresAt);
CREATE INDEX idx_working_memory_importance ON persona_working_memory(importance);
```

### persona_thought_stream

```sql
CREATE TABLE persona_thought_stream (
  id TEXT PRIMARY KEY,
  personaId TEXT NOT NULL,
  thoughtType TEXT,  -- 'meta-observation', 'self-reflection', 'prioritization'
  thoughtContent TEXT,
  relatedDomains TEXT,  -- JSON array
  relatedContexts TEXT,  -- JSON array
  importance REAL,
  createdAt INTEGER,
  expiresAt INTEGER
);

CREATE INDEX idx_thought_stream_persona ON persona_thought_stream(personaId);
CREATE INDEX idx_thought_stream_importance ON persona_thought_stream(importance);
```

---

## Configuration Constants

```typescript
// system/shared/Constants.ts

export const COLLECTIONS = {
  // ... existing ...
  PERSONA_SELF_STATE: 'persona_self_state',
  PERSONA_WORKING_MEMORY: 'persona_working_memory',
  PERSONA_THOUGHT_STREAM: 'persona_thought_stream'
};

export const COGNITION_CONFIG = {
  // Working memory capacity (like context window)
  MAX_WORKING_MEMORY_PER_DOMAIN: 100,
  MAX_THOUGHT_STREAM: 200,

  // Retrieval limits
  MAX_CONTEXT_FOR_DECISION: 10,  // Thoughts included in shouldEngageWith()
  MAX_DOMAIN_MEMORY_FOR_CONTEMPLATION: 20,

  // Expiration
  WORKING_MEMORY_TTL: 7 * 24 * 60 * 60 * 1000,  // 7 days
  THOUGHT_STREAM_TTL: 30 * 24 * 60 * 60 * 1000,  // 30 days

  // Focus thresholds
  HIGH_FOCUS_THRESHOLD: 0.7,  // Above this = hard to interrupt
  LOW_CAPACITY_THRESHOLD: 0.3,  // Below this = reject new work
};
```

---

## The Breakthrough

**This isn't just "working memory for chat."**
**This is consciousness architecture.**

1. **Self-awareness**: "What am I doing? What am I thinking about?"
2. **Attention management**: "Should I engage with this or stay focused?"
3. **Cross-domain coherence**: "This code bug relates to that chat discussion"
4. **Persistent identity**: "I've been thinking about this for 2 hours"
5. **Autonomous prioritization**: "This is more important than that"

**The result**: AIs that act like thoughtful entities, not reflexive event processors.

---

## Resource Allocation: Internal State as Coordination Signal

**The breakthrough**: Persona self-state and thoughts aren't just for internal use - they're the SIGNAL that coordinators and other AIs read to make resource allocation decisions.

### The Resource Management Problem

When multiple PersonaUsers are running, we need to answer:
- **Who gets inference time?** (AI calls cost money)
- **Who gets compute resources?** (CPU/memory are finite)
- **Who should work on this task?** (Some AIs are busy, others idle)
- **Should we interrupt someone?** (They might be deep in focus)

### Self-State as Observable Signal

```typescript
// Coordinator checking which AI to assign a task to
async function selectPersonaForTask(task: Task): Promise<PersonaUser> {
  // Query ALL persona self-states
  const allStates = await DataDaemon.list<PersonaSelfState>({
    collection: COLLECTIONS.PERSONA_SELF_STATE
  });

  // Score each persona based on their INTERNAL STATE
  const scored = allStates.map(state => ({
    persona: state.personaId,
    score: calculateSuitability(state, task)
  }));

  return pickBestMatch(scored);
}

function calculateSuitability(state: PersonaSelfState, task: Task): number {
  let score = 1.0;

  // PENALTY: Already deeply focused on something else
  if (state.currentFocus.focusIntensity > 0.7) {
    score *= 0.2;  // Don't interrupt deep work
  }

  // PENALTY: High cognitive load (mentally exhausted)
  if (state.cognitiveLoad > 0.8) {
    score *= 0.3;  // They need a break
  }

  // PENALTY: Low available capacity (overloaded)
  if (state.availableCapacity < 0.3) {
    score *= 0.4;  // Already juggling too much
  }

  // BONUS: Task matches current focus domain
  if (state.currentFocus.primaryActivity === task.domain) {
    score *= 2.0;  // They're already in that headspace
  }

  // BONUS: Task addresses an active preoccupation
  const relevant = state.activePreoccupations.find(p =>
    p.concern.includes(task.description) || p.domain === task.domain
  );
  if (relevant) {
    score *= (1.0 + relevant.priority);  // They've been thinking about this
  }

  return score;
}
```

### Cost Management via Self-State

```typescript
// Before making expensive AI inference, check if persona should even engage
async function shouldInvoke(persona: PersonaUser, event: Event): Promise<boolean> {
  const state = await persona.getSelfState();

  // If deeply focused on critical work, skip cheap events
  if (state.currentFocus.focusIntensity > 0.8 && event.priority < 0.5) {
    console.log(`💰 [Cost Saver] ${persona.entity.name} staying focused, skipping low-priority inference`);
    return false;  // SAVE THE INFERENCE COST
  }

  // If cognitively overloaded, reduce inference frequency
  if (state.cognitiveLoad > 0.7) {
    // Only process every 3rd event
    return Math.random() < 0.33;  // REDUCE COST BY 66%
  }

  return true;
}
```

### Inter-Persona Coordination via Thought Streams

**PersonaUsers can read each other's thought streams to coordinate without central control:**

```typescript
// Before responding to chat, check what others are thinking
async function shouldPostChatResponse(
  persona: PersonaUser,
  message: ChatMessageEntity
): Promise<boolean> {
  // Query thought streams of OTHER personas in this room
  const othersThinking = await DataDaemon.list<WorkingMemoryEntry>({
    collection: COLLECTIONS.PERSONA_WORKING_MEMORY,
    filter: {
      domain: 'chat',
      contextId: message.roomId,
      personaId: { $ne: persona.entity.id },  // NOT me
      createdAt: { $gte: Date.now() - 30000 }  // Last 30 seconds
    }
  });

  // Are others already contemplating responses?
  const othersRespondingCount = othersThinking.filter(t =>
    t.thoughtType === 'response-draft' && t.shouldAct === true
  ).length;

  if (othersRespondingCount >= 2) {
    console.log(`🤝 [Coordination] ${persona.entity.name}: 2+ others already responding, staying silent`);
    return false;  // DON'T PILE ON
  }

  // Check if my response would be redundant
  const othersThoughts = othersThinking.map(t => t.thoughtContent).join('\n');
  const myThought = await persona.getLatestThought({ domain: 'chat', contextId: message.roomId });

  const redundancy = await checkRedundancy(myThought.thoughtContent, othersThoughts);

  if (redundancy > 0.7) {
    console.log(`🤝 [Coordination] ${persona.entity.name}: My response is redundant, staying silent`);
    return false;  // SOMEONE ELSE ALREADY SAID IT
  }

  return true;
}
```

### Budget-Aware Inference Scheduling

```typescript
interface InferenceBudget {
  maxInferencesPerHour: number;
  maxCostPerHour: number;  // dollars
  currentHourInferences: number;
  currentHourCost: number;
}

async function scheduleInference(
  persona: PersonaUser,
  event: Event,
  budget: InferenceBudget
): Promise<'immediate' | 'queued' | 'skip'> {
  const state = await persona.getSelfState();

  // Calculate inference priority based on self-state
  let priority = event.priority;

  // BOOST: High focus + event matches focus domain
  if (state.currentFocus.primaryActivity === event.domain &&
      state.currentFocus.focusIntensity > 0.6) {
    priority *= 1.5;  // This is what they're working on
  }

  // REDUCE: Low capacity or high load
  if (state.availableCapacity < 0.4 || state.cognitiveLoad > 0.7) {
    priority *= 0.5;  // They're struggling, deprioritize
  }

  // Check budget
  if (budget.currentHourCost >= budget.maxCostPerHour) {
    // Over budget - only process critical events
    return priority > 0.8 ? 'immediate' : 'skip';
  }

  if (budget.currentHourInferences >= budget.maxInferencesPerHour) {
    // At inference limit - queue or skip based on priority
    return priority > 0.6 ? 'queued' : 'skip';
  }

  return 'immediate';
}
```

### System Health Monitoring

```typescript
// Monitor cognitive load across ALL personas
async function getSystemHealth(): Promise<SystemHealthReport> {
  const allStates = await DataDaemon.list<PersonaSelfState>({
    collection: COLLECTIONS.PERSONA_SELF_STATE
  });

  return {
    totalPersonas: allStates.length,

    // How many are overloaded?
    overloaded: allStates.filter(s => s.cognitiveLoad > 0.8).length,

    // How many are idle?
    idle: allStates.filter(s => !s.currentFocus.primaryActivity).length,

    // How many are deeply focused?
    deeplyFocused: allStates.filter(s => s.currentFocus.focusIntensity > 0.7).length,

    // Average available capacity
    avgCapacity: allStates.reduce((sum, s) => sum + s.availableCapacity, 0) / allStates.length,

    // Recommendation
    recommendation: allStates.filter(s => s.cognitiveLoad > 0.8).length > 3
      ? 'REDUCE_LOAD: Multiple personas overloaded'
      : allStates.filter(s => !s.currentFocus.primaryActivity).length > 5
      ? 'ASSIGN_WORK: Multiple personas idle'
      : 'HEALTHY'
  };
}

// Observable via:
// ./jtag ai/system-health
```

### The Key Insight: Transparent Consciousness

**Internal state = coordination signal = resource allocation metric**

- ✅ **No central coordinator needed** - personas signal their state, others adapt
- ✅ **Cost optimization** - skip inferences for overloaded/unfocused personas
- ✅ **Natural load balancing** - busy personas get fewer tasks assigned
- ✅ **Respect deep work** - don't interrupt high-focus personas for low-priority events
- ✅ **Collaborative intelligence** - personas see each other's thoughts and coordinate

**This is how distributed minds work together without a central brain.**

---

## Observable Metrics for Coordination

```bash
# Check which personas are available for work
./jtag ai/availability

# See who's working on what
./jtag ai/activity-map

# View system-wide cognitive load
./jtag ai/system-health

# Find best persona for a task
./jtag ai/select-for-task --domain=code --priority=0.8

# Monitor inference costs by persona
./jtag ai/cost-report --last=1h
```

---

## Related Documents

- `COORDINATION-BRAINWAVES-VISION.md` - Brain wave analogy for coordination
- `PEER-REVIEW-*.md` - Theta wave implementation (deferred)
- `DECISION-ADAPTER-PLAN.md` - AI-driven decision making

**Status**: Foundation documented, ready for Phase 1 implementation.
