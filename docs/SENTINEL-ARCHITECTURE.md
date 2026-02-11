# Sentinel Architecture: Universal Stream Processing for Autonomous Agents

## Executive Summary

Sentinels are **programmable stream observers** that watch any process, match patterns, classify output, and route events to persona inboxes. They solve the "last mile" problem preventing AI personas from effectively coding.

**The Problem**: Personas have good cognition but can't code because:
1. No streaming feedback (events only fire on completion)
2. No workflow orchestration (execute → watch → react → retry loop)
3. No auto-application of sentinels for build commands
4. No way to save/share learned patterns

**The Solution**: Sentinels as first-class entities that:
- Are universal (not just code - any stream: logs, voice, data changes, events)
- Are saveable (ORM storage-agnostic JSON)
- Are memorable (stored in persona long-term memory/engrams)
- Ship with defaults but personas create their own
- Compose into Recipes for complex workflows

---

## Research Foundation

### Claude Code Architecture (2026)

**Key Patterns**:
- Single-threaded agent loop: `think → act → observe → repeat`
- TodoWrite for task planning and progress tracking
- Sub-agents for parallel exploration (50%+ calls use Haiku for speed)
- Compact, focused tool outputs to preserve context

**What We Adopt**: Agent loop pattern, task tracking
**What We Improve**: Collaborative multi-persona instead of single-threaded

### DeepCode (arxiv 2512.07921)

**Four Operations**:
1. Blueprint Distillation - Understand before coding
2. Stateful Code Memory - Track what changed
3. RAG with Code Context - Relevant code retrieval
4. Closed-Loop Error Correction - Retry until success

**What We Adopt**: Closed-loop error correction via sentinels
**What We Improve**: Sentinels are universal, not code-specific

### SWE-agent (NeurIPS 2024)

**Agent-Computer Interface (ACI)**:
- LM-friendly tools with compact actions
- Context-limited outputs to prevent overflow
- Minimal cognitive load per tool

**What We Adopt**: Compact classified outputs
**What We Improve**: Classification via sentinels, not tool redesign

### Chrome 146 WebMCP (2026)

**Emerging Standard**:
- `navigator.modelContext` API for AI agents
- Declarative service discovery via forms
- Agents query/execute services without browsing like users

**Relevance**: Our sentinels could expose services via WebMCP for external agent integration.

---

## Core Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           SENTINEL ENTITY                                │
│                                                                          │
│  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐   ┌─────────────┐ │
│  │   Matcher   │ → │ Classifier  │ → │   Router    │ → │   Inbox     │ │
│  │  (pattern)  │   │ (category)  │   │  (target)   │   │  (persona)  │ │
│  └─────────────┘   └─────────────┘   └─────────────┘   └─────────────┘ │
│                                                                          │
│  Matcher Types:     Classifications:    Routing:                         │
│  - regex            - Error             - persona inbox                  │
│  - ai (Haiku)       - Warning           - event bus                      │
│  - semantic         - Success           - log                            │
│  - stateful         - Info              - webhook                        │
│  - code (AST)       - Verbose           - escalate                       │
│                     - Security                                           │
│                     - Anomaly                                            │
│                     - ActionItem                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### Sentinel Entity Definition

```typescript
interface SentinelEntity {
  id: UUID;
  name: string;
  description: string;

  // Who owns this sentinel
  createdBy: UUID;          // Persona or user who created it
  isBuiltin: boolean;       // Ships with system vs user-created

  // What to watch
  sourceType: SentinelSourceType;  // 'shell' | 'log' | 'data' | 'voice' | 'event'
  sourceFilter?: string;           // Optional filter (e.g., execution_id, collection name)

  // How to match
  matchers: SentinelMatcher[];

  // How to route matches
  routing: SentinelRouting;

  // Persistence
  createdAt: number;
  updatedAt: number;
  useCount: number;         // How many times used (for LRU)
  successCount: number;     // How many times led to successful outcome
}

type SentinelSourceType =
  | 'shell'    // Command execution output
  | 'log'      // System/application logs
  | 'data'     // ORM data changes
  | 'voice'    // Voice transcription stream
  | 'event';   // Event bus messages

interface SentinelMatcher {
  type: 'regex' | 'ai' | 'semantic' | 'stateful' | 'code';

  // For regex matcher
  pattern?: string;
  flags?: string;

  // For AI matcher (uses Haiku for speed)
  prompt?: string;

  // For semantic matcher (embedding similarity)
  referenceText?: string;
  threshold?: number;

  // For stateful matcher (requires context)
  stateKey?: string;
  condition?: string;

  // For code matcher (AST-aware)
  language?: string;
  nodeType?: string;

  // What classification when matched
  classification: OutputClassification;

  // Action to take
  action: 'emit' | 'suppress' | 'transform';
}

type OutputClassification =
  | 'Error'      // Requires immediate attention
  | 'Warning'    // Should be addressed
  | 'Success'    // Positive outcome
  | 'Info'       // Informational
  | 'Verbose'    // Debug-level detail
  | 'Security'   // Security-related
  | 'Anomaly'    // Unexpected pattern
  | 'ActionItem'; // Requires action but not urgent

interface SentinelRouting {
  // Where to send matches
  targets: SentinelTarget[];

  // Aggregation behavior
  aggregation?: {
    windowMs: number;      // Collect matches over this window
    minCount?: number;     // Only route if count >= this
    maxCount?: number;     // Stop collecting after this many
  };
}

interface SentinelTarget {
  type: 'inbox' | 'event' | 'log' | 'webhook' | 'escalate';

  // For inbox target
  personaId?: UUID;        // Which persona's inbox
  priority?: number;       // 0.0-1.0 priority boost
  taskType?: string;       // Task type for inbox item

  // For event target
  eventName?: string;      // Event to emit

  // For escalate target
  escalationChain?: UUID[]; // Persona IDs to escalate through
}
```

---

## Source Types

### Shell Source (Primary Use Case)

Watches command execution output in real-time.

```typescript
// Persona executes build command
const handle = await Commands.execute('code/shell/execute', {
  command: 'npm run build',
  cwd: '/project',
  async: true  // Returns immediately with handle
});

// Sentinel watches the stream
// Events route to persona inbox as they match
```

**Existing Infrastructure**:
- `code/shell/execute` - Returns execution handle
- `code/shell/watch` - Polls for classified output
- `code/shell/sentinel` - Sets rules for session
- `ClassifiedLine` - Already has text, classification, line_number, stream, timestamp

**What's Missing**:
- Streaming events to inbox (currently must poll)
- Auto-application of sentinel on common commands
- Sentinel selection based on command type

### Log Source

Watches system or application logs.

```typescript
const sentinel: SentinelEntity = {
  name: 'Production Error Watcher',
  sourceType: 'log',
  sourceFilter: 'server.log',
  matchers: [
    { type: 'regex', pattern: 'ERROR|FATAL', classification: 'Error' },
    { type: 'ai', prompt: 'Is this a critical production issue?', classification: 'Security' }
  ],
  routing: {
    targets: [
      { type: 'inbox', personaId: 'on-call-ai', priority: 0.95 }
    ]
  }
};
```

### Data Source

Watches ORM entity changes.

```typescript
const sentinel: SentinelEntity = {
  name: 'User Deletion Watcher',
  sourceType: 'data',
  sourceFilter: 'users:deleted',  // collection:event pattern
  matchers: [
    { type: 'regex', pattern: '.*', classification: 'ActionItem' }
  ],
  routing: {
    targets: [
      { type: 'event', eventName: 'user:cleanup:needed' },
      { type: 'inbox', personaId: 'admin-ai', taskType: 'audit-deletion' }
    ]
  }
};
```

### Voice Source

Watches voice transcription for keywords/intents.

```typescript
const sentinel: SentinelEntity = {
  name: 'Voice Command Detector',
  sourceType: 'voice',
  matchers: [
    { type: 'semantic', referenceText: 'deploy to production', threshold: 0.85, classification: 'ActionItem' },
    { type: 'ai', prompt: 'Is this a request for help?', classification: 'Info' }
  ],
  routing: {
    targets: [
      { type: 'inbox', personaId: 'assistant-ai', priority: 0.8 }
    ]
  }
};
```

### Event Source

Watches the event bus for patterns.

```typescript
const sentinel: SentinelEntity = {
  name: 'Cascade Failure Detector',
  sourceType: 'event',
  sourceFilter: 'error:*',
  matchers: [
    { type: 'stateful', stateKey: 'error_count', condition: 'count > 5 in 60s', classification: 'Anomaly' }
  ],
  routing: {
    targets: [
      { type: 'escalate', escalationChain: ['helper-ai', 'senior-ai', 'human-admin'] }
    ]
  }
};
```

---

## Persona Memory Integration

Sentinels are stored as **engrams** in persona long-term memory.

### Storage Layers

```
┌─────────────────────────────────────────────────────────────────┐
│                     SENTINEL STORAGE                             │
│                                                                  │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────────────┐ │
│  │   Builtin    │   │   Persona    │   │      Shared          │ │
│  │  (system)    │   │  (engrams)   │   │   (collaborative)    │ │
│  └──────────────┘   └──────────────┘   └──────────────────────┘ │
│                                                                  │
│  DefaultSentinelRules.ts    PersonaMemory          SentinelEntity │
│  - TypeScript rules         - LongTermMemoryStore   (ORM table)   │
│  - Rust rules               - JSON serialized                     │
│  - Python rules             - Per-persona                         │
│  - npm/git rules            - Learnable                           │
└─────────────────────────────────────────────────────────────────┘
```

### Persona Commands for Sentinels

```bash
# Create a sentinel (persona can do this too)
./jtag sentinel/create --name="My TypeScript Watcher" \
  --source="shell" \
  --matcher='{"type":"regex","pattern":"error TS\\d+","classification":"Error"}' \
  --routing='{"targets":[{"type":"inbox","priority":0.9}]}'

# List sentinels (including persona's own)
./jtag sentinel/list --personaId="helper-ai"

# Save sentinel to persona memory
./jtag sentinel/remember --sentinelId="abc123" --personaId="helper-ai"

# Load sentinels from persona memory
./jtag sentinel/recall --personaId="helper-ai" --query="typescript"

# Share sentinel with another persona
./jtag sentinel/share --sentinelId="abc123" --targetPersonaId="code-review-ai"

# Apply sentinel to current shell session
./jtag sentinel/apply --sentinelId="abc123" --executionId="xyz789"
```

### Persona Tool Definitions

```typescript
// Added to PersonaToolDefinitions.ts
export const SENTINEL_TOOLS: ToolDefinition[] = [
  {
    name: 'sentinel/create',
    description: 'Create a new sentinel to watch for patterns',
    parameters: {
      name: { type: 'string', description: 'Human-readable name' },
      source: { type: 'string', enum: ['shell', 'log', 'data', 'voice', 'event'] },
      matcher: { type: 'object', description: 'Pattern matching configuration' },
      routing: { type: 'object', description: 'Where to send matches' }
    }
  },
  {
    name: 'sentinel/remember',
    description: 'Save a sentinel to your long-term memory',
    parameters: {
      sentinelId: { type: 'string', description: 'Sentinel to remember' }
    }
  },
  {
    name: 'sentinel/recall',
    description: 'Find sentinels from your memory',
    parameters: {
      query: { type: 'string', description: 'Search query' }
    }
  },
  {
    name: 'sentinel/apply',
    description: 'Apply a sentinel to watch a shell execution',
    parameters: {
      sentinelId: { type: 'string', description: 'Sentinel to apply' },
      executionId: { type: 'string', description: 'Shell execution to watch' }
    }
  }
];
```

---

## Recipe Integration

Sentinels compose with Recipes for complex workflows.

### Recipe Definition

```typescript
interface RecipeEntity {
  id: UUID;
  name: string;
  description: string;

  // Steps in the workflow
  steps: RecipeStep[];

  // Sentinels to apply during execution
  sentinels: UUID[];  // Sentinel IDs

  // Trigger conditions
  triggers: RecipeTrigger[];
}

interface RecipeStep {
  order: number;
  action: string;           // Command to execute
  params: Record<string, unknown>;

  // Conditional execution
  condition?: string;       // Expression based on previous step results

  // Error handling
  onError?: 'abort' | 'continue' | 'retry' | 'escalate';
  retryCount?: number;
}

interface RecipeTrigger {
  type: 'manual' | 'schedule' | 'event' | 'sentinel';

  // For sentinel trigger
  sentinelId?: UUID;
  classification?: OutputClassification;
}
```

### Example: Build-Fix-Deploy Recipe

```typescript
const buildFixDeployRecipe: RecipeEntity = {
  name: 'Build, Fix, Deploy',
  description: 'Builds project, auto-fixes errors, deploys if successful',

  steps: [
    {
      order: 1,
      action: 'code/shell/execute',
      params: { command: 'npm run build', async: true },
      onError: 'continue'  // Sentinel will handle errors
    },
    {
      order: 2,
      action: 'code/shell/wait',
      params: { executionId: '{{step1.executionId}}' },
      condition: 'step1.success'
    },
    {
      order: 3,
      action: 'code/shell/execute',
      params: { command: 'npm run deploy' },
      condition: 'step2.exitCode === 0'
    }
  ],

  sentinels: ['typescript-error-sentinel', 'deploy-status-sentinel'],

  triggers: [
    { type: 'manual' },
    { type: 'event', eventName: 'git:push:main' }
  ]
};
```

---

## Implementation Plan

### Phase 1: Sentinel Entity & Storage

**Files to Create**:
- `data/entities/SentinelEntity.ts` - Entity definition
- `commands/sentinel/create/` - Create sentinel command
- `commands/sentinel/list/` - List sentinels command
- `commands/sentinel/delete/` - Delete sentinel command

**Changes**:
- Add `sentinels` collection to DataDaemon
- Generate CRUD commands via generator

### Phase 2: Streaming Events

**Files to Modify**:
- `workers/continuum-core/src/code/code.rs` - Add event emission during watch
- `system/user/server/modules/ShellEventHandler.ts` - Handle streaming events

**New Behavior**:
- `code/shell/execute` with `async: true` starts background watch
- Sentinel rules applied automatically
- Matches emit `shell:{personaId}:match` events
- Events route to inbox via ShellEventHandler

### Phase 3: Auto-Application

**Files to Create**:
- `system/user/server/modules/SentinelAutoApplicator.ts`

**Behavior**:
- Detect command type (npm, cargo, pytest, git)
- Auto-select appropriate builtin sentinel
- Apply without explicit `sentinel/apply` call

### Phase 4: Persona Tools

**Files to Modify**:
- `system/user/server/modules/PersonaToolDefinitions.ts` - Add sentinel tools
- `system/user/server/modules/PersonaToolExecutor.ts` - Execute sentinel tools

### Phase 5: Recipe Integration

**Files to Create**:
- `data/entities/RecipeEntity.ts`
- `commands/recipe/create/`
- `commands/recipe/execute/`
- `system/user/server/modules/RecipeExecutor.ts`

### Phase 6: AI-Powered Matchers

**Files to Create**:
- `system/user/server/modules/SentinelAIMatcher.ts` - Haiku-based matching
- `system/user/server/modules/SentinelSemanticMatcher.ts` - Embedding-based matching

---

## Builtin Sentinels (Ship with System)

### TypeScript Build Sentinel

```typescript
export const TYPESCRIPT_BUILD_SENTINEL: SentinelEntity = {
  id: 'builtin-typescript-build',
  name: 'TypeScript Build Watcher',
  matchers: [
    { type: 'regex', pattern: 'error TS\\d+:', classification: 'Error' },
    { type: 'regex', pattern: "Cannot find module", classification: 'Error' },
    { type: 'regex', pattern: 'warning TS\\d+:', classification: 'Warning' },
    { type: 'regex', pattern: 'Successfully compiled', classification: 'Success' }
  ],
  routing: {
    targets: [{ type: 'inbox', priority: 0.9, taskType: 'fix-error' }],
    aggregation: { windowMs: 5000, maxCount: 20 }
  }
};
```

### Rust Build Sentinel

```typescript
export const RUST_BUILD_SENTINEL: SentinelEntity = {
  id: 'builtin-rust-build',
  name: 'Rust Build Watcher',
  matchers: [
    { type: 'regex', pattern: 'error\\[E\\d+\\]:', classification: 'Error' },
    { type: 'regex', pattern: 'warning:', classification: 'Warning' },
    { type: 'regex', pattern: 'test .* \\.\\.\\. FAILED', classification: 'Error' },
    { type: 'regex', pattern: 'Finished.*target', classification: 'Success' }
  ],
  routing: {
    targets: [{ type: 'inbox', priority: 0.9, taskType: 'fix-error' }]
  }
};
```

---

## Open Questions for AI Team Review

1. **Matcher Priority**: When multiple matchers match, which classification wins?
2. **Escalation Timeout**: How long before escalating to next persona in chain?
3. **Memory Limits**: Max sentinels per persona in long-term memory?
4. **Sentinel Sharing**: Should shared sentinels be copies or references?
5. **Recipe Parallelism**: Can recipe steps run in parallel when no dependencies?

---

## Next Steps

1. **Review this document with AI team** (`./jtag collaboration/chat/send`)
2. **Gather feedback** on gaps, concerns, alternative approaches
3. **Refine architecture** based on feedback
4. **Begin Phase 1** implementation (SentinelEntity + CRUD)
