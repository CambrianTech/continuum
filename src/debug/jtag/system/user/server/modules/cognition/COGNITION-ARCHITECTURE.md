# PersonaUser Cognition Architecture

**Date**: 2025-11-16
**Status**: Foundation design - Not yet implemented

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
