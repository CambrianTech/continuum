# RECIPE SYSTEM REQUIREMENTS

**Based on Case Study Analysis**: Thronglets, Tarot Reading, and Real Chat System

This document identifies what the recipe system needs to support based on actual use cases, and what's missing from the current implementation.

---

## CURRENT RECIPE IMPLEMENTATION STATUS

### ✅ What Exists Today

```typescript
// system/recipes/general-chat.json
{
  "uniqueId": "general-chat",
  "name": "General Chat (Human-Focused)",
  "pipeline": [
    {
      "command": "rag/build",
      "params": { "maxMessages": 20 },
      "outputTo": "ragContext"
    },
    {
      "command": "ai/should-respond",
      "params": { "ragContext": "$ragContext", "strategy": "human-focused" },
      "outputTo": "decision"
    },
    {
      "command": "ai/generate",
      "params": { "ragContext": "$ragContext" },
      "condition": "decision.shouldRespond === true"
    }
  ]
}
```

**Loaded via**: `./jtag recipe/load --loadAll=true`

**Current Features**:
- ✅ Sequential pipeline execution
- ✅ Variable passing (`outputTo`, `$variableName`)
- ✅ Conditional execution (`condition`)
- ✅ JSON file storage
- ✅ Load/reload commands

---

## WHAT'S MISSING (FROM CASE STUDIES)

### 1. Trigger Types ❌

**Current**: Recipes don't have triggers - manually invoked only

**Needed** (from case studies):

```typescript
type RecipeTrigger =
  | { type: 'user-message', roomId?: string, filter?: MessageFilter }      // Chat, Tarot
  | { type: 'game-loop', frequency: number }                                // Thronglets
  | { type: 'event', eventPattern: string }                                 // Any event
  | { type: 'scheduled', cronPattern: string }                              // Periodic tasks
  | { type: 'manual', commandName: string };                                // Explicit invoke

// Example usage:
{
  "uniqueId": "thronglets-game-loop",
  "trigger": {
    "type": "game-loop",
    "frequency": 10  // Hz
  },
  "pipeline": [...]
}

{
  "uniqueId": "tarot-reading",
  "trigger": {
    "type": "user-message",
    "roomId": "tarot-reading-room"
  },
  "pipeline": [...]
}
```

**Why Critical**: Without triggers, recipes are just command sequences - not autonomous systems

---

### 2. Execution Modes ❌

**Current**: Sequential only

**Needed**:

```typescript
type RecipeExecutionMode =
  | 'sequential'     // One step at a time (Tarot dialogue)
  | 'parallel'       // All steps concurrently (Thronglets batch AI)
  | 'concurrent'     // Mix of parallel + sequential
  | 'streaming';     // Real-time output as steps complete

// Example:
{
  "executionMode": "parallel",
  "pipeline": [
    { "command": "ai/decide", "params": { "throngletId": "$thronglets[0]" } },
    { "command": "ai/decide", "params": { "throngletId": "$thronglets[1]" } },
    { "command": "ai/decide", "params": { "throngletId": "$thronglets[2]" } }
    // All execute simultaneously
  ]
}
```

**Why Critical**: Thronglets needs batch AI decisions (100+ at once), chat needs sequential dialogue

---

### 3. Loop Control ❌

**Current**: No loops - recipe runs once

**Needed**:

```typescript
interface LoopControl {
  type: 'continuous' | 'counted' | 'conditional';
  frequency?: number;           // Hz for continuous loops
  maxIterations?: number;       // Limit for counted loops
  stopCondition?: string;       // JS expression for conditional loops
  restartDelay?: number;        // Ms delay between iterations
}

// Example - Thronglets game loop:
{
  "uniqueId": "thronglets-game-loop",
  "loop": {
    "type": "continuous",
    "frequency": 10,
    "stopCondition": "gameState.ended === true"
  },
  "pipeline": [
    { "command": "game/tick" },
    { "command": "ai/decide-batch" },
    { "command": "game/apply-decisions" }
  ]
}
```

**Why Critical**: Games need continuous loops, chat needs single-run per message

---

### 4. Batch Operations ❌

**Current**: Commands execute on single entities

**Needed**:

```typescript
// Array iteration
{
  "command": "ai/decide",
  "forEach": "$thronglets",  // Iterate over array
  "params": {
    "throngletId": "$item.id",
    "context": "$item.context"
  },
  "outputTo": "decisions[]",  // Accumulate into array
  "parallelism": 10           // Max concurrent executions
}

// Batch command variant
{
  "command": "ai/decide-batch",
  "params": {
    "entities": "$thronglets",      // Pass entire array
    "ragContextBuilder": "game"
  },
  "outputTo": "decisions"
}
```

**Why Critical**: 100 Thronglets can't make decisions one-by-one - too slow

---

### 5. State Management ❌

**Current**: Variables lost after recipe execution

**Needed**:

```typescript
// Persistent recipe state
{
  "uniqueId": "tarot-reading",
  "state": {
    "drawnCards": [],
    "spreadType": null,
    "sessionState": "greeting"
  },
  "pipeline": [
    {
      "command": "tarot/draw-card",
      "outputTo": "newCard"
    },
    {
      "command": "state/update",
      "params": {
        "drawnCards": "$state.drawnCards.push($newCard)",
        "sessionState": "interpretation"
      }
    }
  ]
}

// Access in next execution:
// $state.drawnCards contains cards from previous runs
```

**Why Critical**: Multi-turn dialogues need conversation state, games need world state

---

### 6. Error Handling ❌

**Current**: Recipe fails completely on any error

**Needed**:

```typescript
{
  "command": "ai/generate",
  "errorHandling": {
    "retry": {
      "attempts": 3,
      "backoff": "exponential",  // 1s, 2s, 4s
      "retryableErrors": ["RATE_LIMIT", "TIMEOUT"]
    },
    "fallback": {
      "command": "chat/send-error-message",
      "params": { "message": "AI temporarily unavailable" }
    },
    "continue": true  // Don't stop pipeline on error
  }
}
```

**Why Critical**: AI calls can fail - need graceful degradation

---

### 7. Conditional Branching ❌

**Current**: Only simple `condition` on individual steps

**Needed**:

```typescript
// If-else branching
{
  "command": "conditional",
  "condition": "$userMessage.contains('help')",
  "then": [
    { "command": "help/show-menu" },
    { "command": "chat/send", "params": { "content": "$helpMenu" } }
  ],
  "else": [
    { "command": "ai/generate", "params": { "prompt": "$userMessage" } }
  ]
}

// Switch-case branching
{
  "command": "switch",
  "value": "$tarotSession.state",
  "cases": {
    "greeting": [
      { "command": "ai/generate", "params": { "responseType": "greeting" } }
    ],
    "spread-selection": [
      { "command": "tarot/suggest-spreads" }
    ],
    "card-interpretation": [
      { "command": "tarot/interpret-card", "params": { "card": "$drawnCard" } }
    ]
  }
}
```

**Why Critical**: Complex dialogues and games need branching logic

---

### 8. Sub-Recipes (Composition) ❌

**Current**: Monolithic pipelines

**Needed**:

```typescript
// Call another recipe as a step
{
  "command": "recipe/execute",
  "recipeId": "rag-build-standard",
  "params": {
    "maxMessages": 20,
    "includeParticipants": true
  },
  "outputTo": "ragContext"
}

// Reusable recipe modules
// system/recipes/modules/rag-build-standard.json
{
  "uniqueId": "rag-build-standard",
  "pipeline": [
    { "command": "data/list", "params": { "collection": "chat_messages" } },
    { "command": "data/list", "params": { "collection": "users" } },
    { "command": "rag/combine", "params": { "messages": "$messages", "users": "$users" } }
  ]
}
```

**Why Critical**: DRY principle - don't duplicate common patterns

---

### 9. Middleware/Hooks ❌

**Current**: No before/after hooks

**Needed**:

```typescript
{
  "uniqueId": "ai-generate-with-logging",
  "before": [
    { "command": "log/start", "params": { "operation": "ai-generate" } },
    { "command": "metrics/increment", "params": { "counter": "ai-requests" } }
  ],
  "pipeline": [
    { "command": "ai/generate", "params": { "prompt": "$prompt" } }
  ],
  "after": [
    { "command": "log/end", "params": { "operation": "ai-generate", "duration": "$duration" } },
    { "command": "metrics/record", "params": { "latency": "$duration" } }
  ],
  "onError": [
    { "command": "log/error", "params": { "error": "$error" } },
    { "command": "alert/send", "params": { "severity": "high" } }
  ]
}
```

**Why Critical**: Observability, debugging, metrics collection

---

### 10. Dynamic Parameters ❌

**Current**: Static parameter values only

**Needed**:

```typescript
// JavaScript expressions in parameters
{
  "command": "ai/generate",
  "params": {
    "temperature": "{{ $userEnergy > 0.5 ? 0.7 : 0.9 }}",  // Dynamic value
    "maxTokens": "{{ Math.min($contextLength * 2, 1000) }}",
    "systemPrompt": "{{ $templates[$recipeType] }}"
  }
}

// Function calls
{
  "command": "game/spawn",
  "params": {
    "position": "{{ randomPosition($worldBounds) }}",
    "genome": "{{ mutateGenome($parentGenome, 0.05) }}"
  }
}
```

**Why Critical**: Complex logic requires computed parameters

---

## PRIORITY FOR CHAT SYSTEM (Real-World Use Case)

### Must-Have for Chat (P0)
1. **Trigger Types** - `user-message` trigger to activate recipe on new messages
2. **State Management** - Track conversation state across messages
3. **Error Handling** - Graceful degradation when AI fails
4. **Sub-Recipes** - Reusable RAG building, AI generation patterns

### Should-Have for Chat (P1)
5. **Conditional Branching** - Different responses based on message content
6. **Execution Modes** - Parallel for multiple persona decisions
7. **Middleware/Hooks** - Logging, metrics, debugging

### Nice-to-Have for Chat (P2)
8. **Loop Control** - Not needed for chat (single-shot per message)
9. **Batch Operations** - Not needed unless many personas respond simultaneously
10. **Dynamic Parameters** - Helpful but can work around with commands

---

## PRIORITY FOR THRONGLETS (Game Use Case)

### Must-Have for Games (P0)
1. **Loop Control** - Continuous game loop at 10 Hz
2. **Batch Operations** - 100+ AI decisions simultaneously
3. **Execution Modes** - Parallel for performance
4. **State Management** - Persistent game world state

### Should-Have for Games (P1)
5. **Trigger Types** - `game-loop` trigger for autonomous execution
6. **Error Handling** - Individual entity failures shouldn't crash game
7. **Dynamic Parameters** - Computed game logic

### Nice-to-Have for Games (P2)
8. **Sub-Recipes** - Helpful for organizing complex game logic
9. **Conditional Branching** - Can be handled in commands
10. **Middleware/Hooks** - Useful for profiling, debugging

---

## IMPLEMENTATION ROADMAP

### Phase 1: Chat System Essentials (Week 1)
**Goal**: Get chat working with recipes

```typescript
// Minimum viable recipe system for chat
interface RecipeV1 {
  uniqueId: string;
  trigger: UserMessageTrigger;        // NEW
  state?: Record<string, any>;        // NEW
  pipeline: RecipeStep[];
  errorHandling?: ErrorHandlingConfig; // NEW
}
```

**New Commands Needed**:
- `recipe/execute` - Execute another recipe (sub-recipes)
- `state/get` - Read recipe state
- `state/update` - Modify recipe state
- `log/*` - Logging commands for debugging

**New Daemon Logic**:
- `RecipeEngine` - Executes recipes with trigger support
- `RecipeTriggerManager` - Listens for events, activates recipes
- `RecipeStateManager` - Persists state between executions

### Phase 2: Game System Essentials (Week 2-3)
**Goal**: Support real-time game loops

```typescript
interface RecipeV2 extends RecipeV1 {
  loop?: LoopControl;                 // NEW
  executionMode?: ExecutionMode;      // NEW
  batching?: BatchingConfig;          // NEW
}
```

**New Features**:
- Continuous loop execution
- Parallel command execution
- Array iteration (`forEach`)
- Batch command variants

### Phase 3: Advanced Features (Week 4+)
**Goal**: Production-ready robustness

```typescript
interface RecipeV3 extends RecipeV2 {
  before?: RecipeStep[];              // NEW
  after?: RecipeStep[];               // NEW
  onError?: RecipeStep[];             // NEW
  branches?: ConditionalBranch[];     // NEW
  dynamicParams?: boolean;            // NEW
}
```

**New Features**:
- Middleware hooks
- Conditional branching
- Dynamic parameter evaluation
- Advanced error handling

---

## DESIGN DECISIONS TO MAKE

### 1. Recipe Storage
**Question**: JSON files vs database?

**Current**: JSON files in `system/recipes/`

**Pros**:
- ✅ Easy to edit
- ✅ Version control friendly
- ✅ No DB dependency

**Cons**:
- ❌ No versioning
- ❌ No runtime editing
- ❌ No per-user recipes

**Recommendation**:
- Keep JSON for system recipes
- Add RecipeEntity for user-created recipes
- Support both sources

### 2. Variable Scoping
**Question**: How do variables flow through pipelines?

**Options**:
```typescript
// Option A: Global scope (current)
{
  "pipeline": [
    { "command": "cmd1", "outputTo": "var1" },
    { "command": "cmd2", "params": { "input": "$var1" } }  // Access anywhere
  ]
}

// Option B: Explicit passing
{
  "pipeline": [
    { "command": "cmd1", "outputTo": "var1" },
    { "command": "cmd2", "input": ["var1"], "params": { "input": "$var1" } }  // Declare dependencies
  ]
}

// Option C: Block scoping
{
  "pipeline": [
    {
      "block": "section1",
      "steps": [
        { "command": "cmd1", "outputTo": "var1" }  // Only visible in section1
      ]
    }
  ]
}
```

**Recommendation**: Start with Option A (simple), add Option B later for optimization

### 3. Error Recovery
**Question**: What happens when a step fails?

**Options**:
- **Fail-fast**: Stop entire recipe (current behavior)
- **Continue**: Skip failed step, continue pipeline
- **Retry**: Attempt step multiple times
- **Fallback**: Execute alternative command

**Recommendation**: Support all options via `errorHandling` config

### 4. State Persistence
**Question**: Where does recipe state live?

**Options**:
- **In-memory**: Fast, lost on restart
- **Database**: Persistent, slower
- **Hybrid**: Hot state in memory, periodic DB saves

**Recommendation**: Hybrid approach with `RecipeStateEntity`

### 5. Trigger Registration
**Question**: How do recipes register triggers?

**Options**:
```typescript
// Option A: On recipe load (automatic)
await Commands.execute('recipe/load', { recipeId: 'general-chat' });
// → Automatically registers user-message trigger

// Option B: Explicit activation
await Commands.execute('recipe/load', { recipeId: 'general-chat' });
await Commands.execute('recipe/activate', { recipeId: 'general-chat', roomId: 'general' });

// Option C: Always active (no registration)
// All recipes with triggers are always listening
```

**Recommendation**: Option B (explicit activation) for control + performance

---

## TESTING STRATEGY

### Unit Tests (Per Feature)
```typescript
// Test trigger activation
test('user-message trigger activates recipe', async () => {
  const recipe = loadRecipe('general-chat');
  await activateRecipe(recipe, { roomId: 'test' });

  await sendMessage('test', 'Hello');

  expect(recipeExecuted).toBe(true);
});

// Test state persistence
test('recipe state persists across executions', async () => {
  const recipe = loadRecipe('tarot-reading');

  await executeRecipe(recipe, { command: 'draw-card' });
  expect(recipe.state.drawnCards.length).toBe(1);

  await executeRecipe(recipe, { command: 'draw-card' });
  expect(recipe.state.drawnCards.length).toBe(2);
});

// Test error handling
test('recipe continues on error with continue=true', async () => {
  const recipe = {
    pipeline: [
      { command: 'cmd1' },
      { command: 'failing-cmd', errorHandling: { continue: true } },
      { command: 'cmd3' }
    ]
  };

  const result = await executeRecipe(recipe);

  expect(result.completedSteps).toBe(2);  // cmd1 and cmd3
});
```

### Integration Tests (Real Scenarios)
```typescript
// Test complete chat flow
test('chat recipe handles user message end-to-end', async () => {
  // 1. User sends message
  await Commands.execute('chat/send', {
    senderId: 'joel',
    roomId: 'general',
    content: 'What is recursion?'
  });

  // 2. Recipe triggers
  await waitForRecipeExecution('general-chat');

  // 3. AI responds
  const messages = await Commands.execute('data/list', {
    collection: 'chat_messages',
    filter: { roomId: 'general' },
    orderBy: [{ field: 'createdAt', direction: 'desc' }],
    limit: 1
  });

  expect(messages.items[0].senderType).toBe('ai');
  expect(messages.items[0].content).toContain('recursion');
});

// Test game loop performance
test('game loop maintains 10 Hz with 100 entities', async () => {
  const recipe = loadRecipe('thronglets-game-loop');

  const startTime = Date.now();
  const tickCount = 100;

  await runGameLoop(recipe, tickCount);

  const duration = Date.now() - startTime;
  const hz = tickCount / (duration / 1000);

  expect(hz).toBeGreaterThan(9);  // At least 9 Hz (close to target 10 Hz)
});
```

---

## NEXT STEPS (IMMEDIATE)

### For Real Chat Implementation:

1. **Create RecipeEngine** (`system/recipe/engine/RecipeEngine.ts`)
   - Execute recipe pipelines
   - Handle variable passing
   - Support conditional execution

2. **Create RecipeTriggerManager** (`system/recipe/triggers/RecipeTriggerManager.ts`)
   - Listen for events
   - Activate matching recipes
   - Support `user-message` trigger

3. **Create RecipeStateManager** (`system/recipe/state/RecipeStateManager.ts`)
   - Persist recipe state to database
   - Load state on recipe execution
   - Handle state updates

4. **Extend RecipeEntity** (`system/data/entities/RecipeEntity.ts`)
   - Add `trigger` field
   - Add `state` field
   - Add `executionMode` field

5. **Create recipe/activate Command**
   - Register recipe triggers
   - Associate with rooms/contexts
   - Enable/disable recipes

6. **Test with general-chat Recipe**
   - User sends message → recipe triggers
   - AI generates response → message sent
   - Verify end-to-end flow

---

## SUCCESS CRITERIA

### Chat System Ready When:
- ✅ User message triggers recipe automatically
- ✅ AI response appears in chat without manual command
- ✅ Multiple personas can coexist (different recipes per persona)
- ✅ Conversation state persists across messages
- ✅ Graceful error handling (AI fails → fallback message)

### Thronglets Ready When:
- ✅ Game loop runs continuously at 10 Hz
- ✅ 100+ Thronglet AI decisions execute in parallel
- ✅ Game state persists across recipe iterations
- ✅ Recipe can run for hours without memory leaks
- ✅ Individual entity errors don't crash game loop

---

## CONCLUSION

**Current Status**: Recipe system is 30% complete
- ✅ Have: Sequential pipelines, variable passing, conditional steps
- ❌ Missing: Triggers, loops, state, error handling, batching

**For Chat System**: Need triggers + state management + error handling (Phase 1)

**For Thronglets**: Need everything (Phase 1 + 2 + 3)

**Recommendation**: Build incrementally
1. Get chat working first (simpler use case, immediate value)
2. Learn from chat implementation
3. Extend for games (more complex requirements)

**Real-World Case Study**: Building chat is our actual case study - document everything we build for CHAT-SYSTEM-CASE-STUDY.md! 🚀
