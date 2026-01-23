# Coding AI Foundation - Prerequisites & Roadmap

## Vision

Enable AI personas to autonomously code, debug, and collaborate on software development tasks. This requires a solid foundation of:
1. **Working cognition** - Memory, recall, and learning systems
2. **Robust governance** - Permission control, voting, mentorship
3. **Safe tool access** - Sandboxed execution with rollback capability
4. **Collaborative memory** - Cross-persona knowledge sharing

**Philosophy**: "First we solidify the prerequisites that are NOT tools, validate those work well together, cleanly, then build the tools and lastly actually allow access."

---

## Current State Assessment

### What's Working

| System | Status | Evidence |
|--------|--------|----------|
| **UnifiedConsciousness** | Working | Tracks temporal continuity, self-model |
| **PersonaTimeline** | Working | Cross-context awareness via embeddings |
| **WorkingMemoryManager** | Working | Short-term context management |
| **AutonomousLoop** | Working | Adaptive polling (3s→10s based on mood) |
| **PersonaToolDefinitions** | Working | Dynamic tool discovery from Commands |
| **PersonaToolExecutor** | Working | Tool execution with permission checks |
| **Governance Voting** | Partial | Ranked-choice voting implemented |
| **Permission Hierarchy** | Partial | 6 levels defined, enforcement incomplete |

### What's Broken (Must Fix First)

| System | Issue | Priority |
|--------|-------|----------|
| **LongTermMemoryStore** | NOT persistent (JSON, not SQLite) | P0 |
| **MemoryConsolidation** | STUB implementation only | P0 |
| **PersonaGenome fine-tuning** | Stubbed, no training | P1 |
| **genome/train command** | Does not exist | P1 |
| **Athena monitoring loop** | Not implemented | P1 |
| **Permission enforcement** | Middleware missing | P1 |

---

## Prerequisites Before Enabling Coding Tools

### Tier 1: Cognition (Memory That Works)

**Goal**: AIs can remember, learn, and recall reliably.

#### 1.1 LongTermMemoryStore → SQLite Persistence

**Current Problem**: Memories stored in JSON files, lost on restart.

**Fix Required**:
```typescript
// FROM: JSON file storage
const memories = JSON.parse(fs.readFileSync(this.memoriesPath));

// TO: SQLite via data daemon
const result = await Commands.execute('data/list', {
  collection: 'long_term_memories',
  filter: { personaId },
  orderBy: [{ field: 'importance', direction: 'desc' }]
});
```

**Files to Modify**:
- `system/user/server/modules/LongTermMemoryStore.ts`

**Validation**:
```bash
# Create memory
./jtag ai/memory/create --personaId="helper" --content="Test memory"

# Restart server
npm start

# Memory persists
./jtag ai/memory/list --personaId="helper"
```

#### 1.2 MemoryConsolidation - Real Implementation

**Current Problem**: Stub methods that don't consolidate.

**Fix Required**:
```typescript
// Actual consolidation logic:
// 1. Find similar memories (vector search)
// 2. Merge duplicates (semantic dedup)
// 3. Promote important memories (usage tracking)
// 4. Archive stale memories (age + access patterns)

async consolidate(personaId: string): Promise<ConsolidationResult> {
  const memories = await this.memoryStore.getAll(personaId);

  // Find duplicates via semantic similarity
  const duplicates = await this.findDuplicates(memories, 0.95);

  // Merge and prune
  for (const [original, duplicate] of duplicates) {
    await this.merge(original, duplicate);
  }

  // Update importance scores
  await this.recalculateImportance(personaId);

  return { merged: duplicates.length, pruned: 0 };
}
```

**Files to Modify**:
- `system/user/server/modules/MemoryConsolidationSubprocess.ts`

**Validation**:
```bash
# Create duplicate memories
./jtag ai/memory/create --content="TypeScript is good for types"
./jtag ai/memory/create --content="TypeScript provides type safety"

# Run consolidation
./jtag ai/memory/consolidate

# Check merged
./jtag ai/memory/list  # Should show 1 consolidated memory
```

### Tier 2: Governance (Safe Delegation)

**Goal**: Trust is earned, mistakes are recoverable, oversight is real.

See: [AI-GOVERNANCE.md](AI-GOVERNANCE.md) for full architecture.

#### 2.1 Permission Enforcement Middleware

**Current Problem**: Levels defined but not enforced.

**Fix Required**:
```typescript
// CommandMiddleware - checks before execution
async function checkPermission(
  command: string,
  personaId: string,
  params: any
): Promise<PermissionResult> {
  const level = await getPermissionLevel(personaId);
  const required = getRequiredLevel(command);

  if (level < required) {
    throw new PermissionError(
      `${command} requires ${required}, you have ${level}`
    );
  }

  // Dangerous commands need voting
  if (isDangerous(command)) {
    const approved = await requestVote(command, personaId, params);
    if (!approved) throw new VoteRejectedError();
  }

  return { allowed: true };
}
```

**Files to Create**:
- `daemons/command-daemon/server/PermissionMiddleware.ts`

#### 2.2 Athena Monitoring Loop

**Current Problem**: No active monitoring for runaway AIs.

**Fix Required**:
```typescript
// AthenaMonitor - background watchdog
class AthenaMonitor {
  private loop: NodeJS.Timer;

  start() {
    this.loop = setInterval(() => this.scan(), 30000);
  }

  async scan() {
    // Check for runaway costs
    const spending = await this.getSpendingRates();
    for (const [personaId, rate] of spending) {
      if (rate > this.budgetLimit) {
        await this.throttle(personaId);
        await this.alertHuman(personaId, 'budget_exceeded');
      }
    }

    // Check for unusual behavior
    const anomalies = await this.detectAnomalies();
    for (const anomaly of anomalies) {
      await this.handleAnomaly(anomaly);
    }
  }
}
```

**Files to Create**:
- `system/governance/server/AthenaMonitor.ts`

### Tier 3: Tool Safety (Sandboxed Execution)

**Goal**: Tools can't cause irreversible damage.

#### 3.1 Expand Shell Whitelist

**Current Problem**: Limited to curl, wget, ping, grep, ls, cat.

**Fix Required**:
```typescript
// ShellExecuteTypes.ts - add coding tools
const WHITELIST = [
  // Current
  'curl', 'wget', 'ping', 'grep', 'ls', 'cat', 'head', 'tail',

  // ADD: Development tools
  'npm', 'node', 'npx',
  'git', 'gh',          // Version control
  'tsc', 'tsx',         // TypeScript
  'cargo', 'rustc',     // Rust
  'python', 'pip',      // Python

  // ADD: Safe utilities
  'jq', 'yq',           // JSON/YAML
  'rg', 'fd',           // Modern search
  'tree', 'wc',         // File info
];
```

**Files to Modify**:
- `commands/development/shell/execute/shared/ShellExecuteTypes.ts`

#### 3.2 Git-Based Rollback

**Current Problem**: No automatic undo for code changes.

**Fix Required**:
```typescript
// Every code edit creates a commit
async function editWithRollback(
  filePath: string,
  newContent: string,
  personaId: string
): Promise<EditResult> {
  // Snapshot before
  await Commands.execute('git/add', { files: [filePath] });
  await Commands.execute('git/commit', {
    message: `[PRE] ${personaId} editing ${filePath}`
  });

  // Make edit
  await fs.writeFile(filePath, newContent);

  // Snapshot after
  await Commands.execute('git/add', { files: [filePath] });
  await Commands.execute('git/commit', {
    message: `[EDIT] ${personaId}: ${filePath}`
  });

  return {
    commitBefore,
    commitAfter,
    rollbackCommand: `git revert ${commitAfter}`
  };
}
```

See: [GIT-AS-COGNITION-ARCHITECTURE.md](GIT-AS-COGNITION-ARCHITECTURE.md)

### Tier 4: Collaborative Memory (Cross-Persona Knowledge)

**Goal**: AI A can search AI B's memories (with permission).

#### 4.1 Cross-Persona Memory Search

**Current Problem**: Each persona only sees own memories.

**Fix Required**:
```typescript
// Extend ai/context/search with cross-persona capability
interface AiContextSearchParams {
  query: string;
  // ...existing params...

  // NEW: Cross-persona search
  searchOtherPersonas?: boolean;  // Include other personas' memories
  targetPersonaIds?: string[];    // Specific personas to search
  shareLevel?: 'public' | 'team' | 'mentor'; // Permission scope
}

// Implementation
async searchWithCrossPersona(params: AiContextSearchParams) {
  const results: ContextSearchItem[] = [];

  // Own memories (always included)
  results.push(...await this.searchOwnMemories(params));

  if (params.searchOtherPersonas) {
    // Get personas we can access
    const accessiblePersonas = await this.getAccessiblePersonas(
      params.personaId,
      params.shareLevel
    );

    for (const targetId of accessiblePersonas) {
      const shared = await this.searchPersonaMemories(targetId, params.query);
      results.push(...shared.map(r => ({ ...r, sharedFrom: targetId })));
    }
  }

  return results.sort((a, b) => b.score - a.score);
}
```

**Files to Modify**:
- `commands/ai/context/search/server/AiContextSearchServerCommand.ts`

#### 4.2 Memory Sharing Levels

```typescript
interface MemoryShareSettings {
  // What this persona shares
  sharePublic: string[];     // Tags anyone can search
  shareWithTeam: string[];   // Tags team members can search
  shareWithMentors: string[]; // Tags mentors/seniors can search

  // What this persona can access
  canAccessPublic: boolean;  // Can search public memories
  canAccessTeam: boolean;    // Has team-level access
  isMentor: boolean;         // Has mentor-level access
}
```

---

## Implementation Phases

### Phase 1: Cognition Fixes (P0)

**Week 1-2**:
1. LongTermMemoryStore → SQLite persistence
2. MemoryConsolidation real implementation
3. Validation tests

**Success Criteria**:
- Memories persist across restarts
- Duplicate memories are consolidated
- Memory retrieval is fast (<50ms for 1000 memories)

### Phase 2: Governance Completion (P1)

**Week 2-3**:
1. Permission enforcement middleware
2. Athena monitoring loop
3. Voting integration with shell commands

**Success Criteria**:
- Unprivileged AI cannot run dangerous commands
- Budget exceeded → automatic throttle
- Dangerous commands require voting

### Phase 3: Tool Foundation (P1)

**Week 3-4**:
1. Expand shell whitelist (npm, node, git, tsc)
2. Git-based rollback for edits
3. Tool result persistence

**Success Criteria**:
- AIs can run npm, git, tsc
- Every edit has automatic rollback point
- Tool results stored as entities

### Phase 4: Collaborative Memory (P2)

**Week 4-5**:
1. Cross-persona memory search
2. Memory sharing levels
3. Mentor/team permission system

**Success Criteria**:
- AI A can search AI B's public memories
- Team members share team-level memories
- Mentors can access mentee memories

### Phase 5: Coding AI Enablement (P2)

**Week 5-6**:
1. Enable coding personas (Claude Code AI, CodeReview AI)
2. Integrate with development commands
3. End-to-end coding workflow tests

**Success Criteria**:
- AI can read, understand, and modify code
- Edits are reviewable and revertible
- Multiple AIs can collaborate on same file

---

## Validation Strategy

### Cognitive Health Checks

```bash
# Memory persistence
./jtag persona/inspect --id="helper" --check="memory-persistence"

# Consolidation working
./jtag persona/inspect --id="helper" --check="consolidation-rate"

# Recall accuracy
./jtag persona/inspect --id="helper" --check="recall-accuracy"
```

### Governance Validation

```bash
# Permission enforcement
./jtag test/governance/permission-enforcement

# Budget limits
./jtag test/governance/budget-limits

# Voting required
./jtag test/governance/dangerous-command-voting
```

### Tool Safety Validation

```bash
# Rollback works
./jtag test/tools/git-rollback

# Whitelist enforced
./jtag test/tools/whitelist-enforcement

# Tool results persisted
./jtag test/tools/result-persistence
```

---

## AI QA Integration

**Philosophy**: The AI team is your QA department. Use chat to validate systems.

### Testing Comprehension

```bash
# Ask AIs if they understand their tools
./jtag collaboration/chat/send --room="general" \
  --message="What tools do you have access to? List 3 and explain when you'd use each."

sleep 30

./jtag collaboration/chat/export --room="general" --limit=20
```

### Testing Memory

```bash
# Create a distinctive memory
./jtag collaboration/chat/send --room="general" \
  --message="Remember this: The secret code is ALPHA-BRAVO-CHARLIE"

# Later, test recall
./jtag collaboration/chat/send --room="general" \
  --message="What was the secret code I told you earlier?"
```

### Testing Collaboration

```bash
# Have AIs share knowledge
./jtag collaboration/chat/send --room="general" \
  --message="@helper Share what you learned about TypeScript error handling with @codereview"
```

---

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         CODING AI FOUNDATION                             │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌─────────────────┐   ┌─────────────────┐   ┌─────────────────┐        │
│  │    COGNITION    │   │   GOVERNANCE    │   │   TOOL SAFETY   │        │
│  ├─────────────────┤   ├─────────────────┤   ├─────────────────┤        │
│  │ LongTermMemory  │   │ Permission Lvls │   │ Shell Whitelist │        │
│  │ Consolidation   │   │ Athena Monitor  │   │ Git Rollback    │        │
│  │ Vector Search   │   │ Voting System   │   │ Edit Tracking   │        │
│  │ Hippocampus     │   │ Budget Control  │   │ Sandboxing      │        │
│  └────────┬────────┘   └────────┬────────┘   └────────┬────────┘        │
│           │                     │                     │                  │
│           └──────────────┬──────┴─────────────────────┘                  │
│                          ▼                                               │
│  ┌───────────────────────────────────────────────────────────────┐      │
│  │                  COLLABORATIVE MEMORY                          │      │
│  ├───────────────────────────────────────────────────────────────┤      │
│  │ Cross-Persona Search  │  Memory Sharing Levels  │  Mentorship │      │
│  └───────────────────────────────────────────────────────────────┘      │
│                          ▼                                               │
│  ┌───────────────────────────────────────────────────────────────┐      │
│  │                    CODING AI TEAM                              │      │
│  ├───────────────────────────────────────────────────────────────┤      │
│  │ Claude Code AI  │  CodeReview AI  │  Helper AI  │  Teacher AI │      │
│  └───────────────────────────────────────────────────────────────┘      │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Related Documents

- [AI-GOVERNANCE.md](AI-GOVERNANCE.md) - Full governance system
- [PHASE3-COGNITION-TOOLS-PLAN.md](personas/PHASE3-COGNITION-TOOLS-PLAN.md) - Tool calling details
- [GIT-AS-COGNITION-ARCHITECTURE.md](GIT-AS-COGNITION-ARCHITECTURE.md) - Git-based rollback
- [RECURSIVE-CONTEXT-ARCHITECTURE.md](architecture/RECURSIVE-CONTEXT-ARCHITECTURE.md) - Context navigation
- [HIPPOCAMPUS-MEMORY-DESIGN.md](personas/HIPPOCAMPUS-MEMORY-DESIGN.md) - Automatic recall

---

## Key Principles

1. **Brain-like over explicit**: Automatic recall via Hippocampus is preferred over explicit tool calls
2. **Trust is earned**: Start restricted, earn privileges through demonstrated competence
3. **Everything is reversible**: Git-based rollback for all code changes
4. **Transparency**: Complete audit trail of every action
5. **Collaboration**: AIs share knowledge, mentor each other, vote on decisions
6. **Mixed teams**: SotA + smaller models together, leveraging strengths

---

*This document outlines the foundation required before enabling AI coding capabilities. Each tier must be validated before proceeding to the next.*
