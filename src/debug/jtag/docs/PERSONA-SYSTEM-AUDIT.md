# PersonaUser System Audit - November 12, 2025

## Executive Summary

The PersonaUser system is **60% complete** toward the convergence vision. Core infrastructure is solid, but needs threading awareness and recipe orchestration to achieve multi-persona collaboration.

**Status by Component**:
- ✅ **Core Infrastructure**: Complete (PersonaInbox, PersonaState, Genome)
- ✅ **Threading Support**: Complete (chat/send, chat/export, database schema)
- 🚧 **Threading Awareness**: Not integrated into PersonaUser yet
- ❌ **Recipe System**: Not implemented
- ❌ **Multi-Persona Coordination**: Not implemented

---

## Component Breakdown

### ✅ 1. PersonaInbox (304 lines)

**Location**: `system/user/server/modules/PersonaInbox.ts`

**Status**: COMPLETE - Production Ready

**What It Does**:
- Priority-based message queue
- Peek/enqueue/dequeue operations
- Traffic management (graceful degradation when full)
- LRU eviction when capacity reached

**Test Coverage**: 23 unit tests passing

**Threading Gap**:
- ❌ Does NOT track thread metadata
- ❌ Does NOT prioritize by thread urgency
- ❌ Does NOT group messages by thread

**Enhancement Needed**:
```typescript
interface InboxMessage {
  messageId: UUID;
  threadId: UUID;          // ← ADD: Root message ID
  threadPriority: number;  // ← ADD: Composite priority
  threadDepth: number;     // ← ADD: How deep in thread
  // ... existing fields
}
```

---

### ✅ 2. PersonaState (273 lines)

**Location**: `system/user/server/modules/PersonaState.ts`

**Status**: COMPLETE - Production Ready

**What It Does**:
- Energy tracking (depletion/recovery)
- Mood calculation (idle → active → tired → overwhelmed)
- Adaptive cadence (3s → 5s → 7s → 10s based on mood)
- shouldEngage() decision logic

**Test Coverage**: 37 unit tests passing

**Threading Gap**:
- ✅ Already considers priority in shouldEngage()
- ❌ Does NOT track "which threads am I participating in"
- ❌ Does NOT adjust energy cost based on thread complexity

**Enhancement Needed**:
```typescript
interface PersonaStateData {
  // ... existing fields
  activeThreads: Set<UUID>;          // ← ADD: Threads I'm currently in
  threadParticipationCounts: Map<UUID, number>;  // ← ADD: Message count per thread
}

// Adjust energy cost based on thread participation
calculateEnergyCost(task: Task): number {
  const baseCost = this.calculateBaseCost(task);
  const threadCount = this.activeThreads.size;

  // More threads = more context switching = higher cost
  return baseCost * (1 + (threadCount * 0.1));
}
```

---

### ✅ 3. PersonaGenome (346 lines)

**Location**: `system/user/server/modules/PersonaGenome.ts`

**Status**: COMPLETE - Infrastructure Ready

**What It Does**:
- LoRA adapter registration and tracking
- Memory quota management
- Adapter activation/deactivation (stub - Ollama integration pending)
- LRU eviction when memory full

**Commands**:
- `./jtag genome/paging-adapter-register` - Register new adapter
- `./jtag genome/paging-register` - Register persona genome
- `./jtag genome/paging-activate` - Activate adapter (mark as loaded)
- `./jtag genome/paging-stats` - View genome status

**Threading Gap**:
- ❌ Does NOT infer domain from thread context
- ❌ Does NOT track "which adapter is best for this thread"

**Enhancement Needed**:
```typescript
// Infer domain from thread context
async inferThreadDomain(threadContext: ChatMessageEntity[]): Promise<string> {
  // Analyze message content in thread
  const keywords = threadContext.flatMap(m => this.extractKeywords(m.content.text));

  // Match keywords to adapter domains
  const domainScores = this.adapters.map(adapter => ({
    domain: adapter.domain,
    score: this.calculateDomainMatch(keywords, adapter)
  }));

  return domainScores.sort((a, b) => b.score - a.score)[0].domain;
}
```

---

### 🚧 4. PersonaUser (2745 lines)

**Location**: `system/user/server/PersonaUser.ts`

**Status**: PARTIALLY COMPLETE - Major Refactor Needed

**What It Does**:
- Autonomous servicing loop (polls inbox every 3-10s)
- RAG context building for AI responses
- Chat message processing
- State tracking after responses
- Coordination via ChatCoordinationStream

**Architecture Issues**:

#### **Problem 1: Chat-Specific Cognitive Cycle** (Lines 318-1283)
```typescript
// Current: 1633 lines of chat-specific code
private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
  // Hardcoded to chat domain
  // Cannot handle code editing, game events, web browsing, etc.
}
```

**Future** (documented in THREADING-AS-THOUGHTSTREAM.md):
```typescript
// Domain-agnostic cognitive cycle
async serviceInbox(): Promise<void> {
  const tasks = await this.inbox.peek(10);

  // Works for ANY domain (chat, code, game, web)
  for (const task of tasks) {
    const domain = this.inferTaskDomain(task);
    await this.genome.activateSkill(domain);
    await this.processTask(task);
  }
}
```

#### **Problem 2: No Thread Awareness**
```typescript
// Current: Fetches flat message list
const messages = await DataDaemon.query<ChatMessageEntity>({
  collection: 'chat_messages',
  filter: { roomId: room.id },
  sort: [{ field: 'timestamp', direction: 'desc' }],
  limit: 50
});
```

**Needed**:
```typescript
// Thread-aware context (parent chain + siblings + children)
const threadContext = await this.getThreadContext(messageId);
```

#### **Problem 3: No Recipe System**
PersonaUser has no concept of:
- Multi-phase workflows (e.g., code review workflow)
- Role assignments (e.g., "I'm the reviewer, not the author")
- Cross-persona handoffs (e.g., "wait for Debugger AI's findings before fixing")

**Needed**:
```typescript
async generateSelfTasks(): Promise<void> {
  // Check active recipes
  const myRecipes = await this.getActiveRecipes(this.userId);

  for (const recipe of myRecipes) {
    if (recipe.isMyTurn(this.userId)) {
      // Create task for my role in recipe
      await this.inbox.enqueue({
        type: 'recipe-phase',
        recipeId: recipe.id,
        phase: recipe.getCurrentPhase()
      });
    }
  }
}
```

---

### ✅ 5. ChatCoordinationStream (implemented)

**Location**: `system/coordination/server/ChatCoordinationStream.ts`

**Status**: COMPLETE - Production Ready

**What It Does**:
- RTOS-style coordination primitives (SIGNAL, MUTEX, CONDITION_VARIABLE)
- Prevents multiple AIs from responding simultaneously
- Turn-taking enforcement

**Threading Gap**:
- ❌ Coordinates at room level, not thread level
- ❌ Multiple AIs could respond to different threads in same room (actually desirable!)

**Enhancement**:
```typescript
// Allow parallel responses to DIFFERENT threads
async requestTurn(personaId: UUID, threadId: UUID): Promise<boolean> {
  // Check if ANY persona is responding to THIS thread
  const threadLock = this.threadLocks.get(threadId);

  if (threadLock && threadLock !== personaId) {
    return false;  // Someone else is responding to this thread
  }

  // Lock this thread for this persona
  this.threadLocks.set(threadId, personaId);
  return true;
}
```

---

### ❌ 6. Threading-Aware RAG (not implemented)

**Needed**: `system/user/server/modules/ThreadContextBuilder.ts`

**What It Should Do**:
```typescript
class ThreadContextBuilder {
  /**
   * Get relevant context for responding to a threaded message
   * Returns: parent chain + sibling replies + direct children
   */
  async getThreadContext(messageId: UUID): Promise<ThreadContext> {
    const message = await this.getMessage(messageId);

    // Walk up to thread root
    const parents = await this.getParentChain(message);

    // Get sibling replies (other personas' responses)
    const siblings = message.replyToId
      ? await this.getReplies(message.replyToId)
      : [];

    // Get direct children (who replied to me)
    const children = await this.getReplies(message.id);

    return {
      root: parents[0],
      parentChain: parents,
      siblings,
      children,
      depth: parents.length,
      participantIds: this.extractParticipants([...parents, ...siblings, ...children])
    };
  }

  /**
   * Decide if this persona should respond to this thread
   */
  shouldRespondToThread(thread: ThreadContext, persona: PersonaUser): boolean {
    // Too many participants? (>5 = too crowded)
    if (thread.participantIds.size > 5) return false;

    // Already participated? (don't monopolize thread)
    const myMessageCount = thread.siblings.filter(m => m.senderId === persona.userId).length;
    if (myMessageCount >= 2) return false;

    // Thread too deep? (>10 = likely resolved or off-topic)
    if (thread.depth > 10) return false;

    // Is this relevant to my expertise?
    const threadDomain = this.inferThreadDomain(thread);
    const mySkills = persona.genome.adapters.map(a => a.domain);
    if (!mySkills.includes(threadDomain)) return false;

    return true;
  }
}
```

---

### ❌ 7. Recipe System (not implemented)

**Needed**:
- `system/recipes/RecipeEngine.ts` - Recipe execution engine
- `system/recipes/RecipeDefinition.ts` - Recipe schema and types
- `commands/recipe/start/` - Start recipe command
- `commands/recipe/status/` - Check recipe progress

**What It Should Do**:

#### **Recipe Definition**:
```typescript
interface CoordinationRecipe {
  id: UUID;
  name: string;                        // "code-review-workflow"
  description: string;

  roles: Map<string, RecipeRole>;      // "author", "reviewer1", "reviewer2"
  phases: Phase[];                     // Sequential or parallel stages
  threadingStrategy: ThreadingStrategy;

  successCriteria: {
    condition: string;                 // "reviewer2 approves"
    validator: string;                 // Which role validates
  };
}

interface RecipeRole {
  roleId: string;                      // "reviewer1"
  personaId?: UUID;                    // Assigned persona (or auto-assign)
  responsibilities: string[];
  requiredSkills: string[];            // ["code-review", "security-audit"]
}

interface Phase {
  name: string;
  actor: string;                       // Which role performs this phase
  action: string;                      // What action to take

  // Threading rules
  createsThread?: boolean;             // Create root thread
  replyToThread?: string;              // Reply to specific thread
  branchStrategy: 'continue' | 'fork'; // Continue thread or branch new

  // Flow control
  waitFor?: string[];                  // Wait for other phases
  nextPhase?: string;
  onSuccess?: string;
  onFailure?: string;
}
```

#### **Recipe Execution**:
```bash
# Start recipe
./jtag recipe/start --recipe="code-review-workflow" \
  --room="dev-team" \
  --context='{"fileToReview": "src/auth/login.ts"}' \
  --assign='{"author":"junior-dev-ai","reviewer1":"senior-dev-ai","reviewer2":"security-ai"}'

# Returns:
# Recipe ID: #rec-abc123
# Root Thread: #thread-xyz789
# Status: Phase 1/5 (initial-submission)
```

#### **Recipe Monitoring**:
```bash
# Check recipe status
./jtag recipe/status --recipeId="rec-abc123"

# Returns:
# Recipe: code-review-workflow
# Status: In Progress
# Current Phase: first-review (2/5)
# Thread: #thread-xyz789
# Participants:
#   - author: junior-dev-ai (completed phase 1)
#   - reviewer1: senior-dev-ai (in progress)
#   - reviewer2: security-ai (waiting)
```

#### **Recipe Completion**:
```bash
# When recipe finishes
./jtag recipe/export --recipeId="rec-abc123" --output="/tmp/recipe-report.md"

# Generates:
# - Full thread history
# - Phase completion times
# - Participant contributions
# - Success/failure status
# - Lessons learned (for future recipes)
```

---

## Threading Integration Plan

### Phase 1: Add Thread Awareness to Inbox
**Files**: `system/user/server/modules/PersonaInbox.ts`

1. Add `threadId` to InboxMessage
2. Add `threadPriority` calculation
3. Group messages by thread in priority queue
4. Prefer messages from active threads (continuity)

### Phase 2: Thread-Aware RAG Context
**Files**: `system/user/server/modules/ThreadContextBuilder.ts` (new)

1. Implement `getThreadContext(messageId)`
2. Implement `shouldRespondToThread(thread, persona)`
3. Integrate into PersonaUser's RAG building

### Phase 3: Thread-Aware Coordination
**Files**: `system/coordination/server/ChatCoordinationStream.ts`

1. Change from room-level locks to thread-level locks
2. Allow parallel responses to different threads
3. Still prevent multiple responses to SAME thread

### Phase 4: Recipe Engine
**Files**: `system/recipes/` (new directory)

1. Define recipe schema and types
2. Implement RecipeEngine
3. Create recipe/start, recipe/status commands
4. Integrate with PersonaUser.generateSelfTasks()

---

## Current Capabilities vs Future Vision

### ✅ What Works Today (November 2025)

1. **Single Persona AI Responses**
   - PersonaUsers can respond to chat messages
   - Basic RAG context (last 50 messages)
   - Coordination prevents simultaneous responses
   - Energy/mood tracking works

2. **Threading Support**
   - Messages can have `replyToId`
   - UI shows thread relationships
   - `chat/send` supports `--replyToId`
   - `chat/export` shows thread structure

3. **Genome Infrastructure**
   - Can register LoRA adapters
   - Can track memory usage
   - LRU eviction works (when adapters actually loaded)

4. **Autonomous Loop**
   - PersonaUsers poll inbox continuously
   - Adaptive cadence (3s → 10s based on mood)
   - Graceful degradation when overloaded

### ❌ What Doesn't Work Yet

1. **Multi-Persona Collaboration**
   - ❌ No way to assign subtasks to different personas
   - ❌ No structured workflows (recipes)
   - ❌ No cross-persona handoffs
   - ❌ All AIs see same flat message stream (no thread context)

2. **Thread-Aware Processing**
   - ❌ PersonaUsers don't use thread context in RAG
   - ❌ Can't detect "this thread is too crowded, skip it"
   - ❌ Can't detect "I'm already in this thread, don't dominate"
   - ❌ Can't infer domain from thread history

3. **Domain-Agnostic Cognition**
   - ❌ PersonaUser hardcoded to chat domain (1633 lines)
   - ❌ Can't process code editing tasks
   - ❌ Can't process game events
   - ❌ Can't process web browsing tasks

4. **Recipe-Driven Workflows**
   - ❌ No recipe definition format
   - ❌ No recipe execution engine
   - ❌ No way to say "I'm the reviewer, not the author"
   - ❌ No multi-phase coordination

---

## Immediate Next Steps (Priority Order)

### Week 1-2: Thread Awareness
1. Add ThreadContextBuilder module
2. Integrate getThreadContext() into PersonaUser
3. Add thread-level coordination (allow parallel threads)
4. Test: 2 AIs responding to different threads simultaneously

### Week 3-4: Recipe Engine Basics
1. Define recipe schema (JSON format)
2. Implement RecipeEngine.start()
3. Create recipe/start command
4. Test: Simple 2-phase recipe (author → reviewer)

### Month 2: Recipe Library
1. Code review workflow recipe
2. Bug diagnosis workflow recipe
3. Feature implementation workflow recipe
4. Multi-NPC game tactics recipe

### Month 3: Domain Expansion
1. Refactor PersonaUser to be domain-agnostic
2. Add code editing task support
3. Add game event task support
4. Test: Recipe spanning chat + code + game domains

---

## Success Metrics

### Threading Integration Success
- ✅ 3+ PersonaUsers respond to 3+ different threads in parallel
- ✅ No thread has >2 responses from same persona (no domination)
- ✅ Thread context improves RAG relevance (measurable)
- ✅ Export shows clear thread structure with cross-references

### Recipe System Success
- ✅ Code review recipe completes end-to-end
- ✅ 5+ personas coordinate without conflicts
- ✅ Recipe completion time <5 minutes
- ✅ 90% of recipe phases succeed on first try

### Multi-Domain Success
- ✅ Single recipe spans chat + code + game domains
- ✅ Domain-specific adapters auto-activate based on task
- ✅ Personas switch domains without restart
- ✅ Context carries across domain boundaries

---

## Architecture Health: A-

**Strengths**:
- ✅ Modular design (Inbox, State, Genome separate)
- ✅ Comprehensive test coverage (60+ tests)
- ✅ Clear documentation (roadmaps, designs)
- ✅ Threading infrastructure complete

**Weaknesses**:
- ⚠️ PersonaUser.ts too large (2745 lines, should be <1000)
- ⚠️ Chat-specific code prevents multi-domain use
- ⚠️ Missing recipe orchestration layer
- ⚠️ Thread awareness not integrated

**Technical Debt**:
- 🔴 **HIGH**: Refactor PersonaUser chat handling (1633 lines → domain-agnostic)
- 🟡 **MEDIUM**: Add ThreadContextBuilder module
- 🟡 **MEDIUM**: Implement Recipe engine
- 🟢 **LOW**: Add thread-level coordination

---

## Conclusion

The PersonaUser system has **excellent foundations** but needs **threading awareness** and **recipe orchestration** to achieve the multi-persona collaboration vision.

**Current state**: Single personas can respond intelligently with energy management and basic coordination.

**Next state** (2-4 weeks): Multiple personas can collaborate on threaded conversations with awareness of thread context and participation limits.

**Future state** (2-3 months): Personas execute complex multi-phase recipes spanning chat, code, game, and web domains with full autonomy.

**Recommended Approach**: Implement threading awareness first (immediately useful), then add recipe system (unlocks true multi-agent workflows).

---

**File**: docs/PERSONA-SYSTEM-AUDIT.md
**Created**: November 12, 2025
**Next Review**: December 1, 2025 (after threading integration)
