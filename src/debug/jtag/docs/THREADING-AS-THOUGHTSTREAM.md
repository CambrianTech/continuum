# Threading as Thoughtstream: Multi-Persona Coordination Architecture

## The Core Insight

**Threading is not just a UI feature - it's the fundamental cognitive architecture for multi-persona coordination.**

Chat message threads are **thoughtstreams with pointers**. Each thread represents a coherent chain of reasoning that multiple personas can follow, branch from, and coordinate around.

---

## The Architecture

### **1. Thoughtstream = Linked Conversation**

```typescript
interface ChatMessageEntity {
  id: UUID;                    // Unique message ID
  replyToId?: UUID;            // Pointer to parent thought
  roomId: UUID;                // Conversation space
  senderId: UUID;              // Which persona/human
  content: MessageContent;     // The thought itself
  timestamp: Date;             // Temporal ordering
  // ... metadata
}
```

**Key Properties**:
- **Each message is a node** in a directed acyclic graph (DAG)
- **replyToId creates edges** linking thoughts together
- **Multiple threads can coexist** in the same room without interference
- **Personas can follow threads** they're interested in vs ignoring others

### **2. Thoughtstream Primitives**

#### **Thread Creation** (Root Message)
```bash
./jtag chat/send --room="dev-team" --message="We need to refactor the auth system"
# Returns: #abc123
```

Creates a new thoughtstream root that others can branch from.

#### **Thread Continuation** (Child Message)
```bash
./jtag chat/send --room="dev-team" --message="I'll audit current implementation" --replyToId="abc123"
# Returns: #def456 (reply to #abc123)
```

Extends an existing thoughtstream with a new node.

#### **Thread Export** (Replay History)
```bash
./jtag chat/export --room="dev-team" --limit=100 --includeThreading=true
```

Reconstructs the full DAG for analysis, learning, or decision replay.

---

## Multi-Persona Coordination Patterns

### **Pattern 1: Parallel Work Decomposition**

```
Human: "We need to implement user authentication"  [#root]
  ├─ CodeReview AI: "I'll analyze security requirements" [#a1 → #root]
  │   └─ CodeReview AI: "OWASP Top 10 compliance needed" [#a2 → #a1]
  ├─ Architect AI: "I'll design the auth flow" [#b1 → #root]
  │   └─ Architect AI: "Proposing JWT + refresh tokens" [#b2 → #b1]
  └─ Testing AI: "I'll write integration tests" [#c1 → #root]
      └─ Testing AI: "Created 15 test scenarios" [#c2 → #c1]
```

**Properties**:
- Three independent thoughtstreams from one root
- No coordination conflicts (each AI works on distinct subtask)
- Human can track progress across all threads
- Personas can reference other threads: "As Architect AI noted in #b2..."

### **Pattern 2: Sequential Refinement**

```
Junior AI: "Here's my initial implementation" [#root]
  └─ Senior AI: "Good start, but missing edge cases" [#rev1 → #root]
      └─ Junior AI: "Added null checks and validation" [#fix1 → #rev1]
          └─ Senior AI: "Approved, ready to merge" [#approve → #fix1]
```

**Properties**:
- Linear thoughtstream (code review workflow)
- Each message builds on previous context
- Clear decision trail for later analysis
- Can export thread to understand how decision evolved

### **Pattern 3: Multi-Agent Code Editing**

```
Task: "Fix the authentication bug in src/auth/login.ts"

Thread 1 (Diagnosis):
  Root: "Starting bug investigation" [#diag-root]
    └─ Debugger AI: "Found null pointer at line 47" [#diag-1]
        └─ Debugger AI: "Root cause: missing user validation" [#diag-2]

Thread 2 (Fix Implementation):
  Root: "Implementing fix based on #diag-2" [#fix-root → #diag-2]
    └─ Coder AI: "Added validation before user access" [#fix-1]
        └─ Coder AI: "Unit test passing" [#fix-2]

Thread 3 (Review):
  Root: "Reviewing changes from #fix-1" [#rev-root → #fix-1]
    └─ Review AI: "LGTM, but add error message" [#rev-1]
        └─ Coder AI: "Added descriptive error message" [#rev-2]
            └─ Review AI: "Approved" [#rev-3]
```

**Properties**:
- Cross-thread references (#fix-root → #diag-2)
- Parallel diagnosis and fix work
- Clear handoff points between personas
- Complete audit trail for post-mortem

### **Pattern 4: Video Game NPC Coordination**

```
Player: "We need to cross this bridge safely" [#root]
  ├─ Scout NPC: "I'll check for enemies ahead" [#scout-1 → #root]
  │   └─ Scout NPC: "3 enemies spotted on bridge" [#scout-2 → #scout-1]
  ├─ Warrior NPC: "Based on #scout-2, I'll lead the charge" [#war-1 → #scout-2]
  │   └─ Warrior NPC: "Engaging front enemy" [#war-2 → #war-1]
  └─ Healer NPC: "I'll stay back and support #war-2" [#heal-1 → #war-2]
      └─ Healer NPC: "Warrior at 75% health, healing" [#heal-2 → #heal-1]
```

**Properties**:
- Dynamic cross-thread dependencies (Healer references Warrior's actions)
- Real-time coordination during gameplay
- Thread structure = decision log for AI learning
- Can replay thread to understand why tactics succeeded/failed

---

## Implementation Status

### ✅ **COMPLETE** (as of November 12, 2025)

**Data Layer**:
- `ChatMessageEntity.replyToId` field (src/debug/jtag/system/data/entities/ChatMessageEntity.ts)
- Threading fully supported in database storage

**Commands**:
- `chat/send` - Send messages with optional `--replyToId` for threading
- `chat/export` - Export with `--includeThreading=true` to show reply relationships

**UI** (Widget):
- Thread display in chat widget (visual indication of reply relationships)
- Short ID display (#abc123 format for easy reference)

### 🚧 **IN PROGRESS**

**PersonaUser Threading Awareness**:
- [ ] Thread context in RAG queries (include parent messages)
- [ ] Thread priority (urgent threads get higher attention)
- [ ] Thread subscription (persona watches specific threads)
- [ ] Thread completion detection (know when thread is resolved)

**Coordination Enhancements**:
- [ ] Cross-thread references ("As discussed in #abc123...")
- [ ] Thread forking (split one thread into multiple)
- [ ] Thread merging (combine related threads)

### 📋 **PLANNED**

**Advanced Features**:
- [ ] Thread summarization (AI generates thread summary on request)
- [ ] Thread search (find threads by topic/participants)
- [ ] Thread analytics (most active threads, longest threads)
- [ ] Thread lifecycle (mark threads as active/resolved/archived)

---

## Recipe System: Coordination Patterns as Code

**Vision**: Define multi-persona collaboration patterns as executable recipes.

### Recipe Structure

```typescript
interface CoordinationRecipe {
  name: string;                        // e.g., "code-review-workflow"
  description: string;

  roles: {
    [roleId: string]: {
      persona: string;                 // PersonaUser ID or type
      responsibilities: string[];
    }
  };

  phases: Phase[];                     // Sequential stages

  threadingRules: {
    rootCreator: string;               // Which role creates root thread
    parallelThreads: string[];         // Which roles can work in parallel
    sequentialThreads: string[];       // Which roles must wait for others
    crossReferences: string[];         // Which roles reference other threads
  };

  successCriteria: {
    condition: string;                 // When recipe is complete
    validator: string;                 // Which role validates completion
  };
}
```

### Example Recipe: Code Review Workflow

```typescript
const codeReviewRecipe: CoordinationRecipe = {
  name: "code-review-workflow",
  description: "Multi-AI code review with sequential approval",

  roles: {
    author: {
      persona: "junior-dev-ai",
      responsibilities: ["Write initial code", "Address feedback", "Final submission"]
    },
    reviewer1: {
      persona: "senior-dev-ai",
      responsibilities: ["Initial review", "Verify fixes"]
    },
    reviewer2: {
      persona: "security-ai",
      responsibilities: ["Security audit", "Final approval"]
    }
  },

  phases: [
    {
      name: "initial-submission",
      actor: "author",
      action: "submit-code",
      createsThread: true,             // Root message
      nextPhase: "first-review"
    },
    {
      name: "first-review",
      actor: "reviewer1",
      action: "review-code",
      replyToThread: "initial-submission",
      branchStrategy: "continue",      // Same thread
      nextPhase: "address-feedback"
    },
    {
      name: "address-feedback",
      actor: "author",
      action: "fix-issues",
      replyToThread: "first-review",
      nextPhase: "security-audit"
    },
    {
      name: "security-audit",
      actor: "reviewer2",
      action: "audit-security",
      replyToThread: "address-feedback",
      nextPhase: "final-approval"
    },
    {
      name: "final-approval",
      actor: "reviewer2",
      action: "approve",
      replyToThread: "security-audit",
      marksThreadComplete: true
    }
  ],

  successCriteria: {
    condition: "reviewer2 posts approval message",
    validator: "reviewer2"
  }
};
```

### Recipe Execution

```bash
# Start a recipe
./jtag recipe/start --recipe="code-review-workflow" \
  --room="dev-team" \
  --context='{"fileToReview": "src/auth/login.ts"}'

# Returns:
# Recipe started: #rec-abc123
# Root thread: #abc123
# Assigned roles: junior-dev-ai, senior-dev-ai, security-ai
```

**What Happens**:
1. System creates root thread with initial context
2. Assigns personas to roles based on recipe
3. Each persona knows its responsibilities and threading rules
4. Personas execute phases in order, creating threaded messages
5. Recipe completes when success criteria met
6. Full thread history available for replay/learning

---

## Technical Implementation

### Thread-Aware RAG Context

**Current** (context window):
```typescript
// Fetch last 50 messages (flat list)
const messages = await DataDaemon.query<ChatMessageEntity>({
  collection: 'chat_messages',
  filter: { roomId: room.id },
  sort: [{ field: 'timestamp', direction: 'desc' }],
  limit: 50
});
```

**Enhanced** (thread-aware context):
```typescript
// Fetch thread context (parent + siblings + children)
async function getThreadContext(messageId: UUID): Promise<ChatMessageEntity[]> {
  const message = await getMessage(messageId);

  // Get parent chain (walk replyToId back to root)
  const parents = await getParentChain(message);

  // Get sibling replies (other messages replying to same parent)
  const siblings = message.replyToId
    ? await getReplies(message.replyToId)
    : [];

  // Get direct children (messages replying to this one)
  const children = await getReplies(message.id);

  return [...parents, message, ...siblings, ...children];
}
```

**Benefit**: PersonaUser sees **relevant conversation history** instead of chronologically recent noise.

### Thread Priority System

```typescript
interface ThreadMetadata {
  threadId: UUID;              // Root message ID
  priority: number;            // 0.0-1.0
  participantCount: number;    // How many personas involved
  messageCount: number;        // Thread depth
  lastActivityAt: Date;
  status: 'active' | 'resolved' | 'archived';
  urgencyTags: string[];       // ['bug', 'security', 'feature']
}
```

**PersonaUser Decision Logic**:
```typescript
async selectThreadToProcess(): Promise<UUID | null> {
  const activeThreads = await this.getActiveThreads();

  // Sort by composite priority:
  // - Explicit priority (from recipe or human)
  // - Urgency tags (bugs > features)
  // - Recency (recent activity = higher priority)
  // - Participant count (more personas = higher coordination need)

  return activeThreads[0]?.threadId || null;
}
```

---

## Connection to PersonaUser Convergence

### Threading Enables the Convergence

**From PERSONA-CONVERGENCE-ROADMAP.md**:
```typescript
async serviceInbox(): Promise<void> {
  const tasks = await this.inbox.peek(10);

  // Threading provides task context:
  for (const task of tasks) {
    // What thread is this task related to?
    const thread = await this.getThreadContext(task.messageId);

    // Should I engage based on thread properties?
    if (thread.participantCount > 5) {
      // Too many cooks, skip this thread
      continue;
    }

    if (thread.priority < 0.5 && this.state.mood === 'tired') {
      // Low priority thread, I'm tired, skip
      continue;
    }

    // Activate skill based on thread domain
    const domain = this.inferThreadDomain(thread);
    await this.genome.activateSkill(domain);

    // Process with full thread context
    await this.processThreadMessage(task, thread);
  }
}
```

**Key Insight**: Threading transforms **reactive message processing** into **context-aware thread participation**.

### Recipes = Task Generation Framework

**Future Enhancement**:
```typescript
async generateSelfTasks(): Promise<void> {
  // Check if any active recipes need my role
  const myRecipes = await this.getActiveRecipes(this.userId);

  for (const recipe of myRecipes) {
    const nextPhase = recipe.getCurrentPhase();

    if (nextPhase.actor === this.roleInRecipe(recipe)) {
      // It's my turn in this recipe
      const task = {
        type: 'recipe-phase',
        recipeId: recipe.id,
        phase: nextPhase.name,
        threadId: recipe.rootThreadId,
        priority: recipe.priority
      };

      await this.inbox.enqueue(task);
    }
  }
}
```

**Result**: Recipes drive autonomous task creation, threads provide execution context.

---

## Summary: Why This Matters

### For Multi-Persona Collaboration
- **No coordination conflicts** - Each thread is an independent workspace
- **Clear handoff points** - Cross-thread references create explicit dependencies
- **Decision transparency** - Full thread history = audit trail
- **Scalable coordination** - 10 personas can work on 10 threads without chaos

### For Video Games
- **NPC believability** - Threads show NPCs thinking and planning
- **Dynamic tactics** - NPCs adapt based on thread context
- **Learning from failure** - Replay threads to improve AI behavior
- **Emergent storytelling** - Thread history = narrative record

### For Code Editing
- **Parallel debugging** - Multiple personas investigate different aspects
- **Review workflows** - Structured approval process via threads
- **Knowledge capture** - Threads document why decisions were made
- **Onboarding** - New AI can read thread history to understand codebase

### For Thoughtstream Architecture
- **Threads = External memory** - Personas don't need to remember everything
- **Threads = Coordination primitives** - Replace ad-hoc coordination with structured threads
- **Threads = Learning corpus** - Export threads as training data for continuous improvement
- **Threads = Cognitive architecture** - Threading IS the multi-agent cognitive framework

---

## Next Steps

### Immediate (Week 1-2)
1. **Thread-aware RAG** - PersonaUser uses getThreadContext() for responses
2. **Thread priority** - Add ThreadMetadata to database
3. **Thread subscription** - PersonaUser can watch specific threads

### Near-Term (Month 1)
4. **Recipe definition format** - JSON schema for recipes
5. **Recipe execution engine** - `./jtag recipe/start` command
6. **Recipe monitoring** - Track recipe progress and completion

### Long-Term (Quarter 1)
7. **Recipe library** - Standard recipes for common workflows
8. **Recipe learning** - AI analyzes successful threads to propose new recipes
9. **Cross-domain recipes** - Recipes that span chat, code, game, web

---

**File**: docs/THREADING-AS-THOUGHTSTREAM.md
**Created**: November 12, 2025
**Status**: Living Document - Continuously Updated
