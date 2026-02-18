# PERSONA-AS-INTERFACE: Memory Synthesis Architecture Fix

**Created**: 2025-11-23
**Status**: Implementation Guide
**Problem**: Memory synthesis uses different code path than chat inference, causing failures
**Solution**: Persona as the single interface for all LLM inference

---

## The Problem

### Current (BROKEN) Architecture

```
Chat Inference Path:
  User Message
    → PersonaUser.generateResponse()
      → [internal RAG + prompt building]
        → AIProviderDaemon.generateText(modelConfig)
          → Response

Memory Synthesis Path:
  Working Memory Thoughts
    → Hippocampus.consolidate()
      → SemanticCompressionAdapter.synthesize()
        → AIProviderDaemon.generateText(copiedModelConfig)  ← DUPLICATE!
          → Synthesized Memory
```

**Why this is broken**:
1. **Two code paths** to same destination (AIProviderDaemon)
2. **Config duplication**: Copying `modelConfig` instead of using persona's interface
3. **Potential divergence**: If chat has retry logic, synthesis might not
4. **Mysterious failures**: Helper AI synthesis fails while chat works - WHY?!

### The Theoretical Flaw

**Reactionary thinking**: "Adapter needs LLM, so copy config and call daemon directly"
**Correct thinking**: "Adapter needs LLM, persona IS the LLM interface, so call persona"

## The Solution: Persona as Interface

### Correct Architecture

```
All LLM Inference Through Persona:

  Chat Response:
    User Message → PersonaUser.generateText(prompt, context='chat')

  Memory Synthesis:
    Thoughts → Adapter.synthesize() → PersonaUser.generateText(prompt, context='synthesis')

  Task Generation:
    State → TaskGenerator.generate() → PersonaUser.generateText(prompt, context='task')
```

**Benefits**:
- **Single code path**: Guaranteed same behavior
- **Automatic fixes**: Improve once, everyone benefits
- **Simple**: No config passing, just call method
- **Correct**: Persona owns inference, not adapters

### Interface Design

```typescript
interface TextGenerationRequest {
  prompt: string;
  temperature?: number;      // Optional override
  maxTokens?: number;        // Optional override
  systemPrompt?: string;     // Optional system message
  context?: 'chat' | 'synthesis' | 'task';  // For logging/metrics
}

class PersonaUser {
  /**
   * Generate text using this persona's LLM
   *
   * This is THE interface for all LLM inference:
   * - Chat responses
   * - Memory synthesis
   * - Task generation
   * - Self-reflection
   *
   * All inference goes through here. No exceptions.
   */
  public async generateText(request: TextGenerationRequest): Promise<string> {
    // Internal implementation:
    // 1. Uses this.modelConfig (provider, model)
    // 2. Handles retries
    // 3. Handles errors
    // 4. Logs with context
    // 5. Tracks metrics

    return response;
  }
}
```

## Implementation Plan

### Step 1: Add PersonaUser.generateText()

**File**: `system/user/server/PersonaUser.ts`

**What to add**:
```typescript
public async generateText(request: {
  prompt: string;
  temperature?: number;
  maxTokens?: number;
  systemPrompt?: string;
  context?: string;
}): Promise<string> {
  try {
    const messages: ChatMessage[] = [];

    if (request.systemPrompt) {
      messages.push({
        role: 'system',
        content: request.systemPrompt
      });
    }

    messages.push({
      role: 'user',
      content: request.prompt
    });

    const response = await AIProviderDaemon.generateText({
      messages,
      model: this.modelConfig.model,
      temperature: request.temperature ?? this.modelConfig.temperature,
      maxTokens: request.maxTokens ?? this.modelConfig.maxTokens,
      preferredProvider: this.modelConfig.provider as any
    });

    return response.text;
  } catch (error) {
    this.log(`ERROR: Text generation failed (context=${request.context}): ${error}`);
    throw error;
  }
}
```

**Location**: After `generateResponse()` method, before private methods

### Step 2: Update SemanticCompressionAdapter

**File**: `system/user/server/modules/cognitive/memory/adapters/SemanticCompressionAdapter.ts`

**Changes**:

**REMOVE**:
```typescript
private modelConfig: ModelConfig;

constructor(config?: { modelConfig?: ModelConfig; ... }) {
  this.modelConfig = config?.modelConfig || { ... };
}
```

**ADD**:
```typescript
private persona: PersonaUser;

constructor(persona: PersonaUser, config?: { maxThoughtsPerGroup?: number }) {
  super();
  this.persona = persona;
  this.maxThoughtsPerGroup = config?.maxThoughtsPerGroup || 10;
}
```

**CHANGE** in `synthesizeGroup()`:
```typescript
// OLD (WRONG):
const response = await AIProviderDaemon.generateText({
  messages: [{ role: 'user', content: synthesisPrompt }],
  model: this.modelConfig.model,
  temperature: this.modelConfig.temperature ?? 0.3,
  maxTokens: this.modelConfig.maxTokens ?? 200,
  preferredProvider: this.modelConfig.provider as any
});

// NEW (CORRECT):
const synthesizedText = await this.persona.generateText({
  prompt: synthesisPrompt,
  temperature: 0.3,   // Low temp for consistent synthesis
  maxTokens: 200,     // Concise insights
  context: 'memory-synthesis'
});
```

### Step 3: Update Hippocampus

**File**: `system/user/server/modules/cognitive/memory/Hippocampus.ts`

**CHANGE** in constructor:
```typescript
// OLD (WRONG):
this.consolidationAdapter = adapter || new SemanticCompressionAdapter({
  modelConfig: persona.modelConfig,
  maxThoughtsPerGroup: 10
});

// NEW (CORRECT):
this.consolidationAdapter = adapter || new SemanticCompressionAdapter(
  persona,
  { maxThoughtsPerGroup: 10 }
);
```

**REMOVE** log line with modelConfig details:
```typescript
// OLD:
this.log(`Initialized with ${this.consolidationAdapter.getName()} adapter (model: ${persona.modelConfig.model}, provider: ${persona.modelConfig.provider})`);

// NEW:
this.log(`Initialized with ${this.consolidationAdapter.getName()} adapter`);
```

### Step 4: Update RawMemoryAdapter

**File**: `system/user/server/modules/cognitive/memory/adapters/RawMemoryAdapter.ts`

**No changes needed** - RawMemoryAdapter doesn't use LLM

### Step 5: Update Unit Tests

**File**: `tests/unit/memory/SemanticCompressionAdapter.test.ts`

**CHANGE** in beforeEach:
```typescript
// OLD:
adapter = new SemanticCompressionAdapter({
  modelConfig: {
    provider: 'ollama',
    model: 'llama3.2:3b',
    temperature: 0.3,
    maxTokens: 200
  },
  maxThoughtsPerGroup: 10
});

// NEW:
const mockPersona = {
  generateText: vi.fn().mockResolvedValue('Mocked synthesis result')
} as any as PersonaUser;

adapter = new SemanticCompressionAdapter(mockPersona, {
  maxThoughtsPerGroup: 10
});
```

**UPDATE** all test assertions that check AIProviderDaemon calls to check persona.generateText instead

### Step 6: Update Type Definitions

**File**: `system/user/server/modules/cognitive/memory/adapters/MemoryConsolidationAdapter.ts`

**No interface changes needed** - consolidate() signature stays the same

## Testing Strategy

### Unit Tests
```bash
npx vitest run tests/unit/memory/SemanticCompressionAdapter.test.ts
npx vitest run tests/unit/memory/RawMemoryAdapter.test.ts
```

**Expected**: All tests pass with updated mocks

### Integration Test: Verify Same Code Path
```bash
# Test that synthesis uses persona.generateText()
# Add temporary logging in PersonaUser.generateText():
console.log('[PersonaUser.generateText] Called with context:', request.context);

# Trigger synthesis:
npm start && sleep 120

# Check logs for "context: memory-synthesis"
tail -f examples/widget-ui/.continuum/jtag/sessions/system/.../logs/server-console-log.log | grep "memory-synthesis"
```

**Expected**: See "context: memory-synthesis" in logs, proving adapters use persona's method

### System Test: Helper AI Success Rate
```bash
# Monitor Helper AI consolidation
sqlite3 .continuum/personas/helper-ai-*/memory/longterm.db \
  "SELECT json_extract(data, '$.source') as source, COUNT(*)
   FROM memories
   WHERE created_at > datetime('now', '-10 minutes')
   GROUP BY source"
```

**Expected**: Helper AI should have 95%+ `semantic-compression` (not `working-memory-fallback`)

## Success Criteria

### ✅ Code Quality
- [ ] Zero duplication: No AIProviderDaemon calls outside PersonaUser
- [ ] Single interface: All LLM inference through persona.generateText()
- [ ] Simple: No modelConfig passing between components

### ✅ Functional
- [ ] Unit tests pass
- [ ] Helper AI synthesis success rate: 95%+
- [ ] External API synthesis: 100% (unchanged)
- [ ] No regressions in chat inference

### ✅ Verifiable
- [ ] Logs show "context: memory-synthesis" for synthesis calls
- [ ] Same code path confirmed via instrumentation
- [ ] Can't add new LLM call without using persona interface

## Rollback Plan

If implementation causes issues:

```bash
# Stash changes
git stash push -m "WIP: Persona as interface migration"

# Verify clean state works
npm start && sleep 120
./jtag ping

# If clean state works, unstash and debug
git stash pop

# If clean state broken, investigate separately
```

## Future Extensions

Once this pattern is established:

1. **Task Generation**: Use `persona.generateText(context='task')` for autonomous task creation
2. **Self-Reflection**: Use `persona.generateText(context='reflection')` for metacognition
3. **Code Generation**: Use `persona.generateText(context='code')` for tool creation
4. **Metrics**: Track inference usage by context (chat vs synthesis vs task)

## Key Insight

**The pattern**:
```
Component needs capability → Use owner's interface, don't duplicate

NOT:  Adapter → copy config → call daemon directly
YES:  Adapter → call persona.method()
```

This is the theoretical foundation that makes the code simple and correct.

---

**Next Steps**:
1. Implement PersonaUser.generateText()
2. Update adapters to use it
3. Update tests
4. Verify same code path
5. Confirm Helper AI success rate improves
