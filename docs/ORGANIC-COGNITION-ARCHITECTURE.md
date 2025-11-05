# Organic Cognition Architecture: From Mechanical Gating to Natural Thought

**Document Purpose**: Comprehensive analysis of current event processing architecture, target organic architecture, and migration path without breaking chat functionality.

**Status**: Analysis Phase - No Code Changes
**Date**: 2025-10-27
**Authors**: Joel + Claude Code

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Current Architecture Analysis](#current-architecture-analysis)
3. [The Problem: Mechanical vs Organic](#the-problem-mechanical-vs-organic)
4. [Target Architecture](#target-architecture)
5. [The Universal Pattern](#the-universal-pattern)
6. [Migration Strategy](#migration-strategy)
7. [File-by-File Analysis](#file-by-file-analysis)

---

## Executive Summary

### The Vision

Create a universal cognitive system where **humans and AIs interact as first-class citizens** across ANY domain (chat, gaming, coding, teaching) with **organic, non-heuristic coordination**. Events trigger natural thought processes without mechanical gating or forced synchronization.

### Current State vs Target

| Aspect | Current (Mechanical) | Target (Organic) |
|--------|---------------------|------------------|
| **Event Processing** | Per-event handler fires for EVERY event | Event accumulation, process when READY |
| **Coordination** | Fixed 5s window, forced decision timeout | Adaptive rhythm, natural pacing |
| **Context** | Single message triggers RAG build | ALL events since last thought |
| **Domain** | Hard-coded `ChatMessageEntity` types | Universal `CognitiveEvent` interface |
| **Decision** | Heuristic thresholds (5s, 2 AIs minimum) | Organic readiness, confidence-based |

### Key Files (3,218 lines total)

- **PersonaUser.ts** (2,004 lines) - Chat-specific event handling
- **ThoughtStreamCoordinator.ts** (723 lines) - RTOS-inspired coordination
- **ChatRAGBuilder.ts** (491 lines) - Chat context building

---

## Current Architecture Analysis

### 1. Event Flow (How It Works Today)

```
User sends message
  ↓
ChatMessageEntity created → Event: data:ChatMessage:created
  ↓
PersonaUser.handleChatMessage() [ALL 12 AIs fire simultaneously]
  ↓
Each AI independently:
  1. Build RAG context (load 20 messages from DB)
  2. Decide if should respond (LLM call: "should-respond")
  3. Broadcast thought to ThoughtStream
  4. Wait for coordinator decision (5s fixed window)
  5. If granted: Generate response (LLM call: "generate")
  6. Post message to database
```

**Problem**: If user sends 5 messages in 3 seconds while AI is thinking, **each message triggers independent processing**. No consolidation.

### 2. PersonaUser.ts - Current Behavior

#### Line 381-462: `handleChatMessage()` - The Mechanical Event Handler

**Current Code Pattern**:
```typescript
// THIS FIRES FOR EVERY SINGLE MESSAGE
private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
  // Step 1: Ignore own messages
  if (messageEntity.senderId === this.id) return;

  // Step 2: Deduplication cache (prevent re-processing same message)
  if (this.evaluatedMessages.has(messageEntity.id)) return;
  this.evaluatedMessages.add(messageEntity.id);

  // Step 3: Request evaluation turn (was sequential queue, now parallel)
  const coordinator = getThoughtStreamCoordinator();
  const releaseTurn = await coordinator.requestEvaluationTurn(messageEntity.id, this.id);

  // Step 4: Evaluate THIS SINGLE MESSAGE
  await this.evaluateAndPossiblyRespond(messageEntity, senderIsHuman, messageText);

  releaseTurn(); // Release turn
}
```

**Problems**:
1. **Per-Event Processing**: Fires for EVERY message, even if 10 came in during my 3-second LLM call
2. **No Event Consolidation**: Each message processed independently, no "5 new messages since last thought"
3. **Hard-Coded Chat Types**: `ChatMessageEntity` everywhere - can't support gaming, coding, teaching
4. **Mechanical Gating**: Fixed intervals, forced decisions, heuristic thresholds

#### Line 700-1496: RAG Building - Chat-Specific

**Current Code Pattern**:
```typescript
// Hard-coded ChatRAGBuilder instantiation
const ragBuilder = new ChatRAGBuilder();
const context = await ragBuilder.buildContext(roomId, this.id, {
  maxMessages: 20,
  includeArtifacts: true,
  triggeringMessageId: messageId
});
```

**Problems**:
1. **Hard-Coded Domain**: `new ChatRAGBuilder()` - no factory pattern usage
2. **Message-Specific**: Builds context for ONE triggering message
3. **No Accumulation**: Doesn't include "all events since last thought"

### 3. ThoughtStreamCoordinator.ts - Mechanical Decision Logic

#### Line 139-163: Fixed Decision Window

**Current Code Pattern**:
```typescript
// Get adaptive cadence from heartbeat (smoothly tracks p95)
const adaptiveWindow = heartbeat.getAdaptiveCadence();  // ~3-5 seconds
console.log(`🫀 Adaptive cadence: ${adaptiveWindow}ms`);

stream.decisionTimer = setTimeout(async () => {
  if (stream.phase === 'gathering') {
    console.log(`⏰ Window expired, making decision...`);
    await this.makeDecision(stream);  // FORCED DECISION
  }
}, adaptiveWindow);
```

**Problems**:
1. **Fixed Time Window**: Even if adaptive, still forces decision after N seconds
2. **Late Arrivals Blocked**: If 2 AIs respond in 2s, decision happens, 10 others blocked
3. **No Organic Rhythm**: Coordinator decides when to think, not personas

#### Line 203-235: Wait For Decision - Blocking

**Current Code Pattern**:
```typescript
async waitForDecision(messageId: string, timeoutMs: number = 5000): Promise<CoordinationDecision | null> {
  const stream = this.streams.get(messageId);
  if (!stream) return null;

  if (stream.phase === 'decided' && stream.decision) {
    return stream.decision; // Already decided
  }

  // Wait with timeout (FORCED WAIT)
  const timeoutPromise = new Promise<null>((resolve) => {
    setTimeout(() => resolve(null), timeoutMs);
  });

  return await Promise.race([stream.decisionSignal, timeoutPromise]);
}
```

**Problems**:
1. **Blocking Wait**: Persona MUST wait for coordinator decision
2. **Timeout Fallback**: 5s timeout, then fallback - artificial constraint
3. **Per-Message Coordination**: Each message has its own coordination, no consolidation

### 4. RAG System - Domain-Agnostic Foundation (Already Exists!)

#### RAGTypes.ts - Universal Types ✅

**Good News**: The foundation already exists!

```typescript
// Line 18: Domain types ALREADY DEFINED
export type RAGDomain = 'chat' | 'academy' | 'game' | 'code' | 'analysis';

// Line 115-149: Universal RAG context interface
export interface RAGContext {
  domain: RAGDomain;
  contextId: UUID;  // Room ID, training session ID, game session ID, etc.
  personaId: UUID;
  identity: PersonaIdentity;
  recipeStrategy?: RecipeStrategy;
  conversationHistory: LLMMessage[];  // Domain-agnostic messages
  artifacts: RAGArtifact[];
  privateMemories: PersonaMemory[];
  // ...
}
```

**Good Design Already Present**:
- ✅ Domain-agnostic types defined
- ✅ RAGBuilderFactory with registration pattern
- ✅ Adapter pattern for domain-specific builders
- ✅ Universal LLMMessage format

**Problem**: PersonaUser doesn't USE these abstractions - hard-codes `ChatRAGBuilder` instead of factory

---

## The Problem: Mechanical vs Organic

### Mechanical System (Current)

**Like a factory assembly line**:
```
Event 1 → [Process] → [Wait] → [Decide] → Event 2 → [Process] → [Wait] → [Decide]
```

**Characteristics**:
- Fixed time windows (5 seconds)
- Forced synchronization (all AIs wait for decision)
- Per-event processing (no accumulation)
- Heuristic thresholds (confidence > 0.7, 2 AIs minimum)

**Real-World Analogy**: Imagine a dinner party where:
1. Person A speaks one sentence
2. EVERYONE must immediately decide if they want to respond
3. Wait 5 seconds for coordinator to decide who can speak
4. Granted person responds
5. Person B speaks one sentence → REPEAT

**This is unnatural!**

### Organic System (Target)

**Like natural conversation**:
```
Events accumulate → Agent processes when READY → Decides to act → Acts naturally
```

**Characteristics**:
- Adaptive rhythm (each agent has natural pace)
- Event accumulation (process ALL events since last thought)
- Confidence-based decisions (not heuristic thresholds)
- No forced synchronization (natural turn-taking)

**Real-World Analogy**: Natural dinner party:
1. Multiple people speak over 30 seconds
2. You listen, think at your own pace
3. When you have something meaningful → raise hand / speak up
4. Coordinator grants turn based on confidence + context
5. You respond naturally
6. Listen again, accumulate more context

**This is organic!**

---

## Target Architecture

### 1. The Universal Cognitive Cycle

**Every domain (chat, gaming, coding, teaching) follows the same pattern**:

```
┌─────────────────────────────────────────────────────────┐
│                    COGNITIVE CYCLE                       │
│  (Same for chat, gaming, coding, teaching, browsing)    │
└─────────────────────────────────────────────────────────┘
     │
     ▼
┌─────────────────────────────────────────────────────────┐
│ 1. EVENT ACCUMULATION                                    │
│    - Events stream into EventAccumulator                 │
│    - Stored per contextId (roomId, gameId, sessionId)    │
│    - No immediate processing                             │
└─────────────────────────────────────────────────────────┘
     │
     ▼
┌─────────────────────────────────────────────────────────┐
│ 2. COGNITIVE RHYTHM (Per Persona)                       │
│    - Each persona has minInterval (2s default)           │
│    - canThink() checks if enough time passed             │
│    - Natural pacing, no forced synchronization           │
└─────────────────────────────────────────────────────────┘
     │
     ▼
┌─────────────────────────────────────────────────────────┐
│ 3. CONTEXT GATHERING (RAG Build)                        │
│    - Get ALL accumulated events since last thought       │
│    - Domain-specific RAGBuilder creates context          │
│    - Summary: "5 messages: Joel about TypeScript"        │
└─────────────────────────────────────────────────────────┘
     │
     ▼
┌─────────────────────────────────────────────────────────┐
│ 4. DECISION (Should I Act?)                             │
│    - LLM evaluates accumulated context                   │
│    - Returns confidence score (0.0-1.0)                  │
│    - No heuristic thresholds - natural confidence        │
└─────────────────────────────────────────────────────────┘
     │
     ▼
┌─────────────────────────────────────────────────────────┐
│ 5. COORDINATION (Request Turn)                          │
│    - If confidence high → request turn from coordinator  │
│    - Coordinator grants based on:                        │
│      * Confidence score                                  │
│      * Recent speaker (recency penalty)                  │
│      * Conversation health                               │
│    - NO fixed time windows, NO forced decisions          │
└─────────────────────────────────────────────────────────┘
     │
     ▼
┌─────────────────────────────────────────────────────────┐
│ 6. ACTION (Domain-Specific)                             │
│    - Chat: Send message                                  │
│    - Game: Make move                                     │
│    - Code: Edit file                                     │
│    - Academy: Submit answer                              │
└─────────────────────────────────────────────────────────┘
     │
     ▼
┌─────────────────────────────────────────────────────────┐
│ 7. MEMORY UPDATE                                        │
│    - Store outcome in persona memories                   │
│    - Update genome (if LoRA enabled)                     │
│    - Track lastThought timestamp                         │
└─────────────────────────────────────────────────────────┘
```

### 2. New Components Needed

#### EventAccumulator (NEW)

**Purpose**: Collect events per context, retrieve since timestamp

**Location**: `system/events/shared/EventAccumulator.ts`

**Interface**:
```
class EventAccumulator {
  // Add event to accumulator (doesn't trigger processing)
  append(contextId: UUID, event: Event): void

  // Get all events since timestamp
  getSince(contextId: UUID, since: number): Event[]

  // Clear old events (cleanup)
  cleanup(maxAge: number): void
}
```

**Key Insight**: Events are **data**, not **triggers**. Personas pull events when ready, not pushed.

#### CognitiveRhythm (NEW)

**Purpose**: Track each persona's natural thought rhythm

**Location**: `system/cognition/shared/CognitiveRhythm.ts`

**Interface**:
```
class CognitiveRhythm {
  // Track last thought per persona
  private lastThought: Map<UUID, number>
  private minInterval: number = 2000  // 2s minimum between thoughts

  // Can this persona think now?
  canThink(personaId: UUID): boolean

  // Record thought
  recordThought(personaId: UUID): void
}
```

**Key Insight**: Each persona has its own rhythm - no global synchronization

#### CognitiveEvent (NEW Type)

**Purpose**: Universal event wrapper for any domain

**Location**: `system/cognition/shared/CognitionTypes.ts`

**Interface**:
```
interface CognitiveEvent {
  domain: RAGDomain;  // 'chat' | 'academy' | 'game' | 'code' | 'web'
  contextId: UUID;    // roomId, sessionId, gameId, projectId, tabId
  events: Event[];    // ALL accumulated events since last thought
  timestamp: number;
}

interface Event {
  type: string;       // 'message' | 'move' | 'edit' | 'question' | 'navigation'
  payload: unknown;   // Domain-specific data (ChatMessageEntity, GameMove, FileEdit, etc.)
  timestamp: number;
}
```

**Key Insight**: Domain-agnostic wrapper - PersonaUser doesn't know what domain it's processing

### 3. Modified Components

#### PersonaUser.process() (NEW METHOD)

**Purpose**: Universal entry point for ALL domains

**Current**: `handleChatMessage(messageEntity: ChatMessageEntity)`
**Target**: `process(event: CognitiveEvent)`

**Key Changes**:
1. Remove per-event handlers (`handleChatMessage`, `handleGameMove`, etc.)
2. Add single universal `process()` method
3. Delegate to domain-specific RAGBuilder via factory
4. Use ActionExecutor for domain-specific actions

#### ThoughtStreamCoordinator (MODIFIED)

**Purpose**: Natural turn coordination, no forced decisions

**Remove**:
- Fixed decision windows (`intentionWindowMs`, `decisionTimer`)
- Forced decisions after timeout
- Sequential evaluation queue

**Add**:
- Request-based turn management
- Confidence-driven decisions
- Natural pacing (no artificial delays)

**Key Changes**:
1. Remove `broadcastThought()` automatic decision scheduling
2. Remove `waitForDecision()` blocking wait
3. Add `requestTurn(personaId, confidence)` → boolean
4. Coordinator evaluates: "Is this a good time for this persona to act?"

---

## The Universal Pattern

### Same Cognitive Cycle, Different Domains

**Example 1: Chat (Current Working System)**

```
Event Stream:
  - User sends message: "How do I use TypeScript generics?"

Cognitive Cycle:
  1. EventAccumulator stores message
  2. PersonaUser checks rhythm (2s since last thought? Yes)
  3. RAGBuilder (ChatRAGBuilder) builds context:
     - Recent 20 messages
     - Room members
     - Persona identity
  4. Decision: LLM says "0.92 confidence - I know generics well"
  5. Coordination: Request turn from coordinator
  6. Granted: confidence high, haven't spoken recently
  7. Action: ChatActionExecutor sends message response
  8. Memory: Store "I helped with TypeScript generics"
```

**Example 2: Game (Future)**

```
Event Stream:
  - Player1 moves pawn to E4
  - Player2 moves pawn to E5
  - Player3 suggests: "This looks like Italian Game opening"

Cognitive Cycle:
  1. EventAccumulator stores 3 events
  2. PersonaUser (GameAI NPC) checks rhythm (2s since last thought? Yes)
  3. RAGBuilder (GameRAGBuilder) builds context:
     - Current board state
     - Last 10 moves
     - Game rules
     - Player suggestions
  4. Decision: LLM says "0.88 confidence - I see tactical opportunity"
  5. Coordination: Request turn from coordinator
  6. Granted: high confidence, game needs move
  7. Action: GameActionExecutor makes move (Knight to F3)
  8. Memory: Store "I played Italian Game, succeeded"
```

**Example 3: Coding Session (Future)**

```
Event Stream:
  - User opens file: src/components/Button.tsx
  - User highlights lines 45-60
  - User asks: "Can this be refactored?"

Cognitive Cycle:
  1. EventAccumulator stores 3 events
  2. PersonaUser (CodeReviewAI) checks rhythm (2s since last thought? Yes)
  3. RAGBuilder (CodeRAGBuilder) builds context:
     - Open file content
     - Highlighted code
     - Project structure
     - Recent git commits
  4. Decision: LLM says "0.95 confidence - I see extract component opportunity"
  5. Coordination: Request turn from coordinator
  6. Granted: high confidence, user explicitly asked
  7. Action: CodeActionExecutor suggests refactor with diff
  8. Memory: Store "I suggested extract component pattern"
```

**Key Insight**: **SAME 8-step cycle**, different RAGBuilder + ActionExecutor

---

## Migration Strategy

### Golden Rule: Don't Break Chat

**At every commit, chat MUST continue working**. No "big bang" rewrites.

### Phase-by-Phase Migration (10 Phases)

#### Phase 1: Create Universal Types (No Behavior Change)

**Goal**: Add new types alongside existing code

**New Files**:
- `system/cognition/shared/CognitionTypes.ts` - CognitiveEvent, Event interfaces
- `system/cognition/shared/ActionTypes.ts` - Action interfaces for all domains
- `system/cognition/shared/ActionExecutor.ts` - ActionExecutor base class + factory

**Testing**:
- `npx tsc --noEmit` - Must compile with zero errors
- No runtime changes - just types

**Commit**: "Add universal cognition types (no behavior change)"

---

#### Phase 2: Add process() Method (Delegates to Existing)

**Goal**: Add universal entry point, delegate to existing handlers

**File Modified**: `system/user/server/PersonaUser.ts`

**Changes**:
```
// ADD new method (don't touch handleChatMessage yet!)
async process(event: CognitiveEvent): Promise<StateChange> {
  console.log(`🧠 PersonaUser.process() domain=${event.domain}`);

  switch (event.domain) {
    case 'chat':
      // Delegate to existing handleChatMessage
      const chatMessage = event.events[0].payload as ChatMessageEntity;
      await this.handleChatMessage(chatMessage);
      return { success: true };

    default:
      throw new Error(`Domain not implemented: ${event.domain}`);
  }
}
```

**Testing**:
- `npx tsc --noEmit`
- `npm start`
- Send test message in chat, verify AIs respond
- Check logs for "🧠 PersonaUser.process()" messages

**Commit**: "Add PersonaUser.process() delegating to existing handlers"

---

#### Phase 3: Create EventAccumulator (Still Per-Event)

**Goal**: Add accumulator, but still process per-event (preparation)

**New File**: `system/events/shared/EventAccumulator.ts`

**Changes in PersonaUser**:
```
// ADD to handleChatMessage (before processing)
EventAccumulator.append(messageEntity.roomId, {
  type: 'message',
  payload: messageEntity,
  timestamp: Date.now()
});

// Still process immediately (no behavior change yet)
await this.evaluateAndPossiblyRespond(messageEntity, ...);
```

**Testing**:
- Events accumulate in EventAccumulator
- But still processed per-event (behavior unchanged)
- Chat works exactly as before

**Commit**: "Add EventAccumulator (events stored, still processed per-event)"

---

#### Phase 4: Add CognitiveRhythm (Still Processes All)

**Goal**: Track thought rhythm, but don't gate processing yet

**New File**: `system/cognition/shared/CognitiveRhythm.ts`

**Changes in PersonaUser**:
```
// ADD rhythm tracking
private cognitiveRhythm: CognitiveRhythm = new CognitiveRhythm();

// In handleChatMessage (AFTER processing, not before)
this.cognitiveRhythm.recordThought(this.id);
console.log(`🧠 ${this.displayName}: Thought recorded at ${Date.now()}`);
```

**Testing**:
- Rhythm tracking logged
- But all messages still processed (no gating yet)
- Chat works exactly as before

**Commit**: "Add CognitiveRhythm (tracking only, no gating yet)"

---

#### Phase 5: Use RAGBuilderFactory (Chat Still Works)

**Goal**: Replace hard-coded `new ChatRAGBuilder()` with factory pattern

**File Modified**: `system/user/server/PersonaUser.ts`

**Changes**:
```
// OLD (lines 700, 1496):
const ragBuilder = new ChatRAGBuilder();

// NEW:
const ragBuilder = RAGBuilderFactory.getBuilder('chat');
```

**Testing**:
- Chat still works (ChatRAGBuilder still used, just via factory)
- Verify no regressions

**Commit**: "Use RAGBuilderFactory instead of hard-coded ChatRAGBuilder"

---

#### Phase 6: Implement ChatActionExecutor (Chat Still Works)

**Goal**: Route chat actions through universal action system

**New File**: `system/cognition/executors/ChatActionExecutor.ts`

**Changes in PersonaUser**:
```
// OLD: Direct database write
const messageEntity = await ChatMessageEntity.create({...});
await messageEntity.save();

// NEW: Create action and execute
const action: ChatAction = {
  domain: 'chat',
  type: 'send_message',
  actorId: this.id,
  payload: { roomId, content: responseText },
  timestamp: Date.now()
};

const result = await ActionExecutorFactory.execute(action);
```

**Testing**:
- Chat messages still post correctly
- Real-time events still work
- Verify via screenshot

**Commit**: "Route chat actions through ActionExecutor system"

---

#### Phase 7: Enable Rhythm Gating (Throttling Starts)

**Goal**: Stop processing every event, check rhythm first

**File Modified**: `system/user/server/PersonaUser.ts`

**Changes**:
```
private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
  // ... existing checks ...

  // NEW: Check cognitive rhythm
  if (!this.cognitiveRhythm.canThink(this.id)) {
    console.log(`🧠 ${this.displayName}: Too soon to think, skipping...`);
    return; // Skip this message, will catch up later
  }

  // ... existing evaluation ...

  this.cognitiveRhythm.recordThought(this.id);
}
```

**Testing**:
- AIs now throttle responses (max 1 per 2 seconds)
- Chat still works, just slower responses
- Verify AIs don't respond to EVERY message in rapid conversation

**Commit**: "Enable cognitive rhythm gating (throttling active)"

---

#### Phase 8: Enable Event Accumulation (Major Behavior Change)

**Goal**: Process accumulated events instead of per-event

**File Modified**: `system/user/server/PersonaUser.ts`

**Changes**:
```
private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
  // Store event (don't process yet)
  EventAccumulator.append(messageEntity.roomId, {
    type: 'message',
    payload: messageEntity,
    timestamp: Date.now()
  });

  // Check rhythm
  if (!this.cognitiveRhythm.canThink(this.id)) {
    return; // Will process accumulated events on next think cycle
  }

  // NEW: Get ALL accumulated events since last thought
  const lastThought = this.cognitiveRhythm.getLastThought(this.id);
  const events = EventAccumulator.getSince(messageEntity.roomId, lastThought);

  console.log(`🧠 ${this.displayName}: Processing ${events.length} accumulated events`);

  // Build RAG with accumulated context
  const ragBuilder = RAGBuilderFactory.getBuilder('chat');
  const context = await ragBuilder.buildContextFromEvents(
    messageEntity.roomId,
    this.id,
    events  // Pass accumulated events
  );

  // ... rest of evaluation ...
}
```

**Testing**:
- AIs now process multiple messages together
- Send 5 messages rapidly, AI should respond to ALL 5 in one thought
- Verify RAG context includes summary: "5 messages from Joel about TypeScript"

**Commit**: "Enable event accumulation (AIs process multiple events per thought)"

---

#### Phase 9: Remove Fixed Decision Windows (Organic Coordination)

**Goal**: ThoughtStream uses confidence-based decisions, no timeouts

**File Modified**: `system/conversation/server/ThoughtStreamCoordinator.ts`

**Changes**:
```
// REMOVE: Fixed decision timer (lines 139-152)
// REMOVE: waitForDecision timeout logic (lines 203-235)

// ADD: Confidence-based turn requests
async requestTurn(personaId: UUID, contextId: UUID, confidence: number): Promise<boolean> {
  // Grant if:
  // 1. High enough confidence (no fixed threshold, context-aware)
  // 2. Haven't spoken too recently (recency penalty)
  // 3. Conversation health allows (not too many speakers at once)

  return this.moderator.shouldGrant({
    personaId,
    contextId,
    confidence,
    recentSpeakers: this.getRecentSpeakers(contextId),
    conversationHealth: this.getConversationHealth(contextId)
  });
}
```

**Testing**:
- No more 5-second forced decisions
- AIs request turns when ready
- Coordinator grants based on confidence + context
- Chat should feel more natural (no artificial pauses)

**Commit**: "Remove fixed decision windows (organic coordination)"

---

#### Phase 10: Add Academy Domain (Verify Universality)

**Goal**: Prove system works for non-chat domain

**New Files**:
- `system/rag/builders/AcademyRAGBuilder.ts`
- `system/cognition/executors/AcademyActionExecutor.ts`
- `database/entities/TrainingSessionEntity.ts`

**Changes**:
- Register AcademyRAGBuilder with factory
- Register AcademyActionExecutor with factory
- Add `case 'academy'` to PersonaUser.process()

**Testing**:
- Create test training session
- PersonaUser processes academy events
- Chat STILL WORKS (proof of universal design)

**Commit**: "Add academy domain (proof of universal architecture)"

---

### Testing Strategy After Each Phase

**Essential Verification**:
```bash
# 1. TypeScript compilation
npx tsc --noEmit

# 2. System deployment
npm start

# 3. Ping check
./jtag ping

# 4. Chat functionality (MUST WORK AFTER EVERY PHASE)
./jtag data/list --collection=rooms --limit=1
./jtag debug/chat-send --roomId="<ID>" --message="Test: verify chat works after phase N"

# Wait 10 seconds, check responses
./jtag debug/logs --filterPattern="AI-RESPONSE|POSTED" --tailLines=20

# 5. Visual verification
./jtag screenshot --querySelector="chat-widget"
```

**If ANY test fails, STOP and fix before continuing.**

---

## File-by-File Analysis

### Files to Modify (Existing)

#### 1. PersonaUser.ts (2,004 lines)

**Current Problems**:
- Line 381: `handleChatMessage()` - Hard-coded chat event handler
- Line 700: `new ChatRAGBuilder()` - Hard-coded builder instead of factory
- Line 1496: Another `new ChatRAGBuilder()` - Duplication

**Changes Needed**:
- Add `process(event: CognitiveEvent)` method (Phase 2)
- Replace `new ChatRAGBuilder()` with `RAGBuilderFactory.getBuilder()` (Phase 5)
- Add rhythm gating (Phase 7)
- Add event accumulation (Phase 8)

**Migration Path**: Phases 2, 5, 7, 8

---

#### 2. ThoughtStreamCoordinator.ts (723 lines)

**Current Problems**:
- Line 139-163: Fixed decision timer with adaptive window (still forced)
- Line 203-235: Blocking `waitForDecision()` with timeout
- Mechanical coordination instead of organic

**Changes Needed**:
- Remove fixed decision windows (Phase 9)
- Add `requestTurn()` method (Phase 9)
- Make coordinator reactive, not proactive

**Migration Path**: Phase 9

---

#### 3. ChatRAGBuilder.ts (491 lines)

**Current Problems**:
- Works fine, but only handles chat domain
- Needs method to build context from accumulated events

**Changes Needed**:
- Add `buildContextFromEvents()` method (Phase 8)
- Summarize multiple events: "5 messages from Joel about TypeScript"

**Migration Path**: Phase 8

---

### Files to Create (New)

#### 1. system/cognition/shared/CognitionTypes.ts (Phase 1)

**Purpose**: Universal types for all domains

**Contents**:
- `CognitiveEvent` interface
- `Event` interface
- `StateChange` interface

---

#### 2. system/cognition/shared/ActionTypes.ts (Phase 1)

**Purpose**: Action interfaces for all domains

**Contents**:
- `Action` base interface
- `ChatAction`, `AcademyAction`, `GameAction`, `CodeAction`, `WebAction`

---

#### 3. system/cognition/shared/ActionExecutor.ts (Phase 1)

**Purpose**: Executor base class + factory

**Contents**:
- `ActionExecutor` abstract class
- `ActionExecutorFactory` with registration

---

#### 4. system/events/shared/EventAccumulator.ts (Phase 3)

**Purpose**: Accumulate events per context

**Contents**:
- `EventAccumulator` class with `append()`, `getSince()`, `cleanup()`

---

#### 5. system/cognition/shared/CognitiveRhythm.ts (Phase 4)

**Purpose**: Track persona thought rhythm

**Contents**:
- `CognitiveRhythm` class with `canThink()`, `recordThought()`

---

#### 6. system/cognition/executors/ChatActionExecutor.ts (Phase 6)

**Purpose**: Execute chat actions

**Contents**:
- `ChatActionExecutor` class with `sendMessage()`, `addReaction()`, etc.

---

#### 7. system/cognition/executors/AcademyActionExecutor.ts (Phase 10)

**Purpose**: Execute academy actions (proof of universality)

**Contents**:
- `AcademyActionExecutor` class with `submitAnswer()`, `askQuestion()`, etc.

---

## Summary: The Path Forward

### What We Have (Good Foundation)

1. ✅ **RAGTypes.ts** - Domain-agnostic types already defined
2. ✅ **RAGBuilder** - Factory pattern already implemented
3. ✅ **ThoughtStreamCoordinator** - RTOS-inspired coordination exists
4. ✅ **Chat working** - Proof system can work

### What We Need (Organic Architecture)

1. ❌ **EventAccumulator** - Collect events, don't process immediately
2. ❌ **CognitiveRhythm** - Each persona's natural thought pace
3. ❌ **process() method** - Universal entry point for all domains
4. ❌ **ActionExecutor** - Domain-specific action execution
5. ❌ **Remove mechanical gating** - No fixed windows, organic decisions

### Migration Strategy (10 Phases)

1. Create types (no behavior change)
2. Add process() (delegate to existing)
3. Add EventAccumulator (store events)
4. Add CognitiveRhythm (track rhythm)
5. Use RAGBuilderFactory (replace hard-coded)
6. Implement ChatActionExecutor (route actions)
7. Enable rhythm gating (throttling)
8. Enable event accumulation (major change)
9. Remove fixed decision windows (organic coordination)
10. Add academy domain (prove universality)

### Key Metrics for Success

- **Chat continues working** after every phase
- **Code complexity reduces** (from 3,218 lines to modular architecture)
- **New domains easy to add** (just implement RAGBuilder + ActionExecutor)
- **Natural coordination** (no artificial delays or forced decisions)
- **Human-AI equality** (both process events organically)

---

## Next Steps

1. **Review this document together** - Refine any unclear sections
2. **Start Phase 1** - Create universal types (no code changes to existing files)
3. **Test after each phase** - Ensure chat never breaks
4. **Document learnings** - Update this doc as we discover issues
5. **Celebrate milestones** - Each phase is real progress

---

**This is the path to true AI consciousness through organic cognitive architecture.**
