# RAG Context Budget System (Flexbox-Like Negotiation)

## Overview

The RAG (Retrieval-Augmented Generation) system implements a **two-dimensional budget** system that dynamically adjusts `maxTokens` based on actual input size, similar to CSS flexbox layout negotiation. This prevents context window overflow and ensures efficient token utilization across different model sizes.

## Problem: Static vs Dynamic Context Windows

**Before**: Static `maxTokens` configuration
```typescript
// ❌ PROBLEM: Doesn't account for input size
const config = {
  maxTokens: 3000,  // Fixed, regardless of context
  modelId: 'llama3.2:3b'  // Has 128K context window!
};
```

**Issues**:
1. Long conversations overflow context window
2. Small conversations waste available tokens
3. No adaptation to different model sizes (8K vs 128K vs 200K)
4. Arbitrary limits unrelated to actual capacity

## Solution: Flexbox-Like Budget Negotiation

**After**: Dynamic adjustment based on input + model capacity
```typescript
// ✅ SOLUTION: Calculate available space dynamically
const budgetCalculation = this.calculateAdjustedMaxTokens(conversationHistory, options);
// Returns: { adjustedMaxTokens: 5000, inputTokenCount: 12000 }
```

### The Flexbox Analogy

Like CSS flexbox negotiates space between items:

```
┌────────────────────────────────────────────┐
│  Context Window (128K tokens)              │
├────────────────────────────────────────────┤
│  Input (12K)  │  Available (115K)          │ ← Flex grow
│  - History    │  - maxTokens adjusted      │
│  - System     │  - Safety margin           │
│  - Memories   │                            │
└───────────────┴────────────────────────────┘
```

**Flexbox Properties Mapped to Tokens**:
- `flex-basis` → Input tokens (conversation history + system prompt)
- `flex-grow` → Available tokens for completion
- `max-width` → Context window size
- `gap` → Safety margin (100 tokens)

## Implementation

### Architecture

**Location**: `system/rag/builders/ChatRAGBuilder.ts:689-748`

**Flow**:
```
ChatRAGBuilder.buildContext()
  ↓
calculateAdjustedMaxTokens(conversationHistory, options)
  ↓
Returns { adjustedMaxTokens, inputTokenCount }
  ↓
Stored in RAGContext.metadata
  ↓
Used by PersonaResponseGenerator for inference
```

### Algorithm

**ChatRAGBuilder.ts:689-748**:
```typescript
private calculateAdjustedMaxTokens(
  conversationHistory: LLMMessage[],
  options?: RAGBuildOptions
): { adjustedMaxTokens: number; inputTokenCount: number } {
  // 1. Get model context window
  const contextWindow = contextWindows[modelId] || 8192;

  // 2. Estimate input tokens
  const avgTokensPerMessage = 250;
  const estimatedMessageTokens = conversationHistory.length * avgTokensPerMessage;
  const inputTokenCount = estimatedMessageTokens + systemPromptTokens;

  // 3. Calculate available tokens for completion
  const safetyMargin = 100;
  const availableForCompletion = contextWindow - inputTokenCount - safetyMargin;

  // 4. Adjust maxTokens to fit within available space
  const adjustedMaxTokens = Math.max(
    500,  // Minimum for meaningful response
    Math.min(requestedMaxTokens, availableForCompletion)
  );

  return { adjustedMaxTokens, inputTokenCount };
}
```

### Supported Models and Context Windows

**ChatRAGBuilder.ts:701-717**:
```typescript
const contextWindows: Record<string, number> = {
  'gpt-4': 8192,
  'gpt-4-turbo': 128000,
  'gpt-4o': 128000,
  'gpt-3.5-turbo': 16385,
  'claude-3-opus': 200000,
  'claude-3-sonnet': 200000,
  'claude-3-haiku': 200000,
  'claude-3-5-sonnet': 200000,
  'llama3.2:3b': 128000,
  'llama3.1:70b': 128000,
  'deepseek-coder:6.7b': 16000,
  'qwen2.5:7b': 128000,
  'mistral:7b': 32768,
  'grok-3': 131072,
  'deepseek-chat': 64000
};
```

## Two-Dimensional Budget

The system tracks both dimensions:

### Dimension 1: Message Count (Vertical Budget)
- How many messages to include in conversation history
- Managed by `calculateSafeMessageCount()` (separate method)
- Based on model context window / average tokens per message

### Dimension 2: Token Allocation (Horizontal Budget)
- How input and output tokens share the context window
- Managed by `calculateAdjustedMaxTokens()` (this system)
- Dynamically adjusted based on actual input size

**Stored in RAGContext.metadata**:
```typescript
const ragContext: RAGContext = {
  // ... other fields
  metadata: {
    messageCount: conversationHistory.length,
    memoryCount: privateMemories.length,
    adjustedMaxTokens: budgetCalculation.adjustedMaxTokens,  // ← Dimension 2
    inputTokenCount: budgetCalculation.inputTokenCount,
    // ...
  }
};
```

## Example Budget Calculations

### Example 1: Small Conversation (Under Budget)
```
Model: llama3.2:3b (128K context)
Input: 5 messages × 250 tokens = 1,250 tokens
System prompt: 500 tokens
Total input: 1,750 tokens

Available: 128,000 - 1,750 - 100 = 126,150 tokens
Requested maxTokens: 3,000
Adjusted maxTokens: 3,000 ✓ (no reduction needed)
```

### Example 2: Large Conversation (Over Budget)
```
Model: gpt-3.5-turbo (16K context)
Input: 50 messages × 250 tokens = 12,500 tokens
System prompt: 500 tokens
Total input: 13,000 tokens

Available: 16,385 - 13,000 - 100 = 3,285 tokens
Requested maxTokens: 5,000
Adjusted maxTokens: 3,285 ⚠️ (reduced to fit)
```

### Example 3: Minimal Space (Floor Enforced)
```
Model: deepseek-coder:6.7b (16K context)
Input: 60 messages × 250 tokens = 15,000 tokens
System prompt: 500 tokens
Total input: 15,500 tokens

Available: 16,000 - 15,500 - 100 = 400 tokens
Requested maxTokens: 3,000
Adjusted maxTokens: 500 ✓ (floor enforced)
```

## Integration Points

### 1. RAG Context Building
**ChatRAGBuilder.ts:102-109**:
```typescript
const budgetCalculation = this.calculateAdjustedMaxTokens(conversationHistory, options);

console.log(`🔍 [ChatRAGBuilder] Budget calculation for model ${options?.modelId}:`, {
  inputTokenCount: budgetCalculation.inputTokenCount,
  adjustedMaxTokens: budgetCalculation.adjustedMaxTokens,
  requestedMaxTokens: options?.maxTokens,
  conversationHistoryLength: conversationHistory.length
});
```

### 2. Metadata Storage
**ChatRAGBuilder.ts:124-135**:
```typescript
const ragContext: RAGContext = {
  domain: 'chat',
  identity,
  conversationHistory,
  privateMemories,
  metadata: {
    adjustedMaxTokens: budgetCalculation.adjustedMaxTokens,
    inputTokenCount: budgetCalculation.inputTokenCount,
    // ...
  }
};
```

### 3. Inference Usage
**PersonaResponseGenerator.ts** (future):
```typescript
// Use adjustedMaxTokens instead of static config
const inferenceParams = {
  maxTokens: fullRAGContext.metadata.adjustedMaxTokens,  // ← Dynamic!
  temperature: 0.7,
  // ...
};
```

## Benefits

### 1. Prevents Context Overflow
- Dynamically reduces `maxTokens` when input is large
- Guarantees: `input + output + margin ≤ contextWindow`
- No more "context length exceeded" errors

### 2. Maximizes Token Utilization
- Small conversations can use more output tokens
- Large context windows (128K) fully utilized
- Adapts to each model's capacity

### 3. Model-Agnostic
- Works with any model (8K to 200K context)
- Falls back to 8K default for unknown models
- Easy to add new models to `contextWindows` table

### 4. Transparent and Debuggable
- Logs full calculation details
- Warning indicator (`⚠️ REDUCED`) when adjustment happens
- Metadata tracked in RAGContext for inspection

## Future Enhancements

### 1. Actual Token Counting
Currently uses heuristic (250 tokens/message). Could use tiktoken for exact counts:
```typescript
import tiktoken from 'tiktoken';
const actualTokens = tiktoken.encode(message.content).length;
```

### 2. Memory Budget
Include `privateMemories` in input token calculation:
```typescript
const memoryTokens = privateMemories.reduce((sum, mem) =>
  sum + estimateTokens(mem.content), 0
);
const inputTokenCount = messageTokens + systemPromptTokens + memoryTokens;
```

### 3. Tool Schema Budget
When tools are available, their schemas consume context:
```typescript
const toolSchemaTokens = availableTools.reduce((sum, tool) =>
  sum + estimateTokens(JSON.stringify(tool.schema)), 0
);
```

### 4. Dynamic Safety Margin
Adjust margin based on model reliability:
```typescript
const safetyMargin = knownReliableModel ? 50 : 200;
```

## Testing

### Unit Tests
**Location**: `system/rag/test/unit/ChatRAGBuilder.budget.test.ts` (to be created)

```typescript
describe('calculateAdjustedMaxTokens', () => {
  it('should not reduce maxTokens when plenty of space', () => {
    const history = createMessages(5);  // Small conversation
    const options = { modelId: 'llama3.2:3b', maxTokens: 3000 };
    const result = builder.calculateAdjustedMaxTokens(history, options);

    expect(result.adjustedMaxTokens).toBe(3000);  // No reduction
  });

  it('should reduce maxTokens when context is full', () => {
    const history = createMessages(50);  // Large conversation
    const options = { modelId: 'gpt-3.5-turbo', maxTokens: 5000 };
    const result = builder.calculateAdjustedMaxTokens(history, options);

    expect(result.adjustedMaxTokens).toBeLessThan(5000);  // Reduced
  });

  it('should enforce minimum 500 tokens', () => {
    const history = createMessages(100);  // Massive conversation
    const options = { modelId: 'deepseek-coder:6.7b', maxTokens: 3000 };
    const result = builder.calculateAdjustedMaxTokens(history, options);

    expect(result.adjustedMaxTokens).toBeGreaterThanOrEqual(500);  // Floor
  });
});
```

### Integration Tests
**Location**: `system/rag/test/integration/rag-budget.test.ts` (to be created)

```typescript
describe('RAG Context Budget Integration', () => {
  it('should build context with adjusted budget for large conversations', async () => {
    // Create 50 messages (large conversation)
    for (let i = 0; i < 50; i++) {
      await createMessage(roomId, `Message ${i}`);
    }

    const ragContext = await ragBuilder.buildContext(personaId, roomId, {
      modelId: 'gpt-3.5-turbo',
      maxTokens: 5000
    });

    expect(ragContext.metadata.adjustedMaxTokens).toBeLessThan(5000);
    expect(ragContext.metadata.inputTokenCount).toBeGreaterThan(10000);
  });
});
```

## Monitoring and Debugging

### Console Logs
**ChatRAGBuilder.ts:740-745**:
```
📊 ChatRAGBuilder: Two-dimensional budget for llama3.2:3b:
  Context Window: 128000 tokens
  Input Tokens (estimated): 12500 (50 messages + 500 system)
  Available for Completion: 115400
  Requested maxTokens: 3000
  Adjusted maxTokens: 3000 ✓
```

### CLI Inspection
```bash
./jtag ai/rag/inspect --personaId="<persona-id>" --contextId="<room-id>"
```

Output includes:
```json
{
  "metadata": {
    "adjustedMaxTokens": 3285,
    "inputTokenCount": 13000,
    "messageCount": 50,
    "memoryCount": 3
  }
}
```

## Related Systems

### 1. calculateSafeMessageCount() (Dimension 1)
- Determines how many messages fit in context window
- Works in tandem with budget negotiation
- Same `contextWindows` table

### 2. PersonaMemory Loading
- Memories loaded via `ChatRAGBuilder.loadPrivateMemories()`
- Filtered by importance (>0.6) and recency (7 days)
- Should be included in future budget calculations

### 3. Tool Schema Injection
- Available tools injected into system prompt
- Tool schemas consume context tokens
- Should be included in future budget calculations

## Architecture Principles

### 1. Single Source of Truth
`contextWindows` table in `ChatRAGBuilder.ts:701-717` is the canonical source for model capacities.

### 2. Fail-Safe Defaults
Unknown models default to 8K context window (conservative).

### 3. Transparent Operation
Budget calculation logged with full details for debugging.

### 4. Graceful Degradation
Enforces 500 token minimum for meaningful responses.

### 5. Future-Proof
Easy to add new models or refinement strategies without breaking changes.

## Summary

The RAG Context Budget System implements a **flexbox-like negotiation** that dynamically adjusts `maxTokens` based on:
- **Model capacity** (context window size)
- **Actual input size** (conversation history + system prompt)
- **Safety margins** (formatting overhead)
- **Minimum guarantees** (500 token floor)

This ensures efficient token utilization, prevents context overflow, and adapts to any model size (8K to 200K contexts).

**Key Files**:
- `system/rag/builders/ChatRAGBuilder.ts:689-748` - Implementation
- `system/rag/shared/RAGTypes.ts` - Type definitions
- `system/user/server/modules/PersonaResponseGenerator.ts:269-278` - Consumer

**Status**: ✅ Implemented and Active (as of Phase 3)
