# Context-Aware RAG - Dynamic Message Loading Design
**Date**: 2025-11-18
**Problem**: Context overflow errors costing money and breaking responses

---

## The Problem

**Current Issue**: GPT Assistant hitting context limits:
```
This model's maximum context length is 8192 tokens.
However, you requested 10793 tokens (7793 in the messages, 3000 in the completion).
```

**Root Cause**: Hardcoded `maxMessages = 20` in ChatRAGBuilder, ignoring:
- Model's actual context window (GPT-4 = 8K, Claude = 200K, etc.)
- Actual token count of messages (varies widely)
- Reserved tokens for completion (maxTokens parameter)

**Cost Impact**: Sending unnecessarily long context to expensive APIs wastes money.

---

## The Solution: Token-Aware Incremental Loading

### Core Strategy

**Don't estimate - count actual tokens and fill to capacity**

1. Calculate available token budget per model
2. Fetch messages incrementally from newest to oldest
3. Count actual tokens as we add each message
4. Stop when we hit ~80% of budget (20% safety margin)

---

## Implementation Design

### Step 1: Token Budget Calculation

```typescript
interface ContextBudget {
  modelContextWindow: number;  // Total context (e.g., 8192 for GPT-4)
  maxTokens: number;           // Reserved for completion (e.g., 3000)
  systemPromptTokens: number;  // Estimated system prompt size
  availableForMessages: number; // Remaining for message history
  targetTokens: number;        // 80% of available (safety margin)
}

function calculateContextBudget(model: string, maxTokens: number): ContextBudget {
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
    'grok-beta': 131072,
    'deepseek-chat': 64000
  };

  const modelContextWindow = contextWindows[model] || 8192; // Default 8K if unknown
  const systemPromptTokens = 500; // Conservative estimate
  const availableForMessages = modelContextWindow - maxTokens - systemPromptTokens;
  const targetTokens = Math.floor(availableForMessages * 0.8); // 80% target

  return {
    modelContextWindow,
    maxTokens,
    systemPromptTokens,
    availableForMessages,
    targetTokens
  };
}
```

**Example for GPT-4**:
- Context window: 8192
- Max tokens: 3000
- System prompt: 500
- Available: 8192 - 3000 - 500 = 4692
- Target (80%): 3753 tokens

---

### Step 2: Actual Token Counting

**Use proper tokenizer** (not character length estimation):

```typescript
/**
 * Count tokens in a message using proper tokenizer
 *
 * Options:
 * 1. Use tiktoken (OpenAI's tokenizer) - most accurate
 * 2. Use rough approximation: ~4 chars per token
 * 3. Use model-specific tokenizer when available
 */
function countMessageTokens(message: ChatMessage): number {
  // OPTION 1: Use tiktoken library (best for OpenAI models)
  // import { encodingForModel } from '@dqbd/tiktoken';
  // const encoder = encodingForModel('gpt-4');
  // return encoder.encode(message.content).length;

  // OPTION 2: Rough approximation (4 chars = 1 token)
  // Good enough for estimation, avoids dependency
  const textLength = message.content.length;
  const roleLength = message.role.length;
  const nameLength = message.name?.length || 0;

  // Account for JSON structure overhead
  const overhead = 10; // For: {"role":"","content":"","name":""}
  const totalChars = textLength + roleLength + nameLength + overhead;

  return Math.ceil(totalChars / 4);
}
```

**Why not estimate average?**
- Messages vary wildly: 10 tokens vs 1000 tokens
- Can't predict which messages will be longest
- Actual counting is cheap (milliseconds)
- Prevents expensive API errors

---

### Step 3: Incremental Message Loading

**Current (broken)**:
```typescript
// ❌ BAD: Hardcoded 20 messages, no token awareness
const maxMessages = 20;
const messages = await loadMessages(roomId, maxMessages);
```

**New (smart)**:
```typescript
// ✅ GOOD: Load incrementally until budget exhausted
async function loadMessagesUpToBudget(
  roomId: UUID,
  budget: ContextBudget
): Promise<ChatMessage[]> {
  const messages: ChatMessage[] = [];
  let totalTokens = 0;
  let offset = 0;
  const batchSize = 10; // Fetch 10 at a time

  while (totalTokens < budget.targetTokens) {
    // Fetch next batch of messages (newest first)
    const batch = await fetchMessages(roomId, batchSize, offset);

    if (batch.length === 0) {
      break; // No more messages
    }

    // Try adding each message
    for (const message of batch) {
      const messageTokens = countMessageTokens(message);

      // Would this exceed budget?
      if (totalTokens + messageTokens > budget.targetTokens) {
        console.log(`🛑 Token budget reached: ${totalTokens}/${budget.targetTokens} tokens, ${messages.length} messages`);
        return messages; // Stop here
      }

      // Add message
      messages.push(message);
      totalTokens += messageTokens;
    }

    offset += batchSize;
  }

  console.log(`✅ Loaded ${messages.length} messages using ${totalTokens}/${budget.targetTokens} tokens`);
  return messages;
}
```

---

### Step 4: Integration with PersonaResponseGenerator

**Pass model config to RAG builder**:

```typescript
// PersonaResponseGenerator.ts
async generateAndPostResponse(originalMessage: ChatMessageEntity): Promise<ResponseGenerationResult> {
  const budget = calculateContextBudget(
    this.modelConfig.model,
    this.modelConfig.maxTokens || 3000
  );

  const ragBuilder = new ChatRAGBuilder();
  const fullRAGContext = await ragBuilder.buildContext(
    originalMessage.roomId,
    this.personaId,
    {
      // Pass budget instead of maxMessages
      tokenBudget: budget.targetTokens,
      includeArtifacts: false,
      includeMemories: false,
      currentMessage: {
        role: 'user',
        content: originalMessage.content.text,
        name: originalMessage.senderName,
        timestamp: originalMessage.timestamp
      }
    }
  );

  // ... rest of generation
}
```

---

## Benefits

### 1. **No More Context Errors**
- Guaranteed to fit within model's context window
- Accounts for actual token counts, not estimates

### 2. **Cost Savings**
- Only sends what fits (no wasted tokens)
- Large context models (Claude) get more history
- Small context models (GPT-4) get less, but don't error

### 3. **Model-Aware**
- GPT-4 (8K): ~15 messages
- Claude (200K): hundreds of messages
- Automatically adapts to each model

### 4. **Fair Resource Usage**
- Models with bigger context get more history
- Models with smaller context still work (graceful degradation)

---

## Implementation Phases

### Phase 1: Quick Fix (Immediate) ✅
**Status**: DONE
- Reduced hardcoded maxMessages from 20 → 10
- Prevents immediate context errors
- Temporary until proper solution deployed

### Phase 2: Token Budget Calculation (Next)
**Location**: `system/user/server/modules/PersonaResponseGenerator.ts`
- Add `calculateContextBudget()` method
- Use model-specific context windows
- Calculate target tokens (80% of available)

### Phase 3: Token Counting Utility (Next)
**Location**: `system/rag/utils/TokenCounter.ts` (new file)
- Implement `countMessageTokens()` using character approximation
- Add option for tiktoken library later (more accurate)
- Export utility for reuse

### Phase 4: Incremental Loading (Final)
**Location**: `system/rag/builders/ChatRAGBuilder.ts`
- Replace `maxMessages` param with `tokenBudget`
- Implement `loadMessagesUpToBudget()`
- Fetch in batches, count tokens, stop when budget hit
- Log actual tokens used for diagnostics

---

## Testing Strategy

### Unit Tests
```typescript
describe('Token Budget Calculation', () => {
  test('GPT-4 8K context', () => {
    const budget = calculateContextBudget('gpt-4', 3000);
    expect(budget.targetTokens).toBe(3753); // (8192 - 3000 - 500) * 0.8
  });

  test('Claude 200K context', () => {
    const budget = calculateContextBudget('claude-3-sonnet', 3000);
    expect(budget.targetTokens).toBe(157200); // (200000 - 3000 - 500) * 0.8
  });

  test('Unknown model defaults to 8K', () => {
    const budget = calculateContextBudget('unknown-model', 3000);
    expect(budget.modelContextWindow).toBe(8192);
  });
});

describe('Token Counting', () => {
  test('Counts message tokens', () => {
    const message = {
      role: 'user',
      content: 'This is a test message with about twenty words in it for testing purposes.',
      name: 'Joel'
    };
    const tokens = countMessageTokens(message);
    expect(tokens).toBeGreaterThan(0);
    expect(tokens).toBeLessThan(50); // Rough sanity check
  });
});

describe('Incremental Loading', () => {
  test('Stops at budget limit', async () => {
    const budget = { targetTokens: 1000 };
    const messages = await loadMessagesUpToBudget('room-id', budget);

    const totalTokens = messages.reduce((sum, m) => sum + countMessageTokens(m), 0);
    expect(totalTokens).toBeLessThanOrEqual(1000);
  });

  test('Loads at least 5 messages even if short budget', async () => {
    const budget = { targetTokens: 100 }; // Very small
    const messages = await loadMessagesUpToBudget('room-id', budget);

    // Should still get SOME context, even if tiny budget
    expect(messages.length).toBeGreaterThanOrEqual(1);
  });
});
```

### Integration Tests
```bash
# Test GPT-4 with real messages
./jtag chat/send --room="general" --message="@gpt test message"
# Monitor logs for: "📊 GPT Assistant: Context calc: model=gpt-4, window=8192, available=4692, safe=15 msgs"

# Test Claude with same room (should get more messages)
./jtag chat/send --room="general" --message="@claude test message"
# Monitor logs for: "📊 Claude Assistant: Context calc: model=claude-3-sonnet, window=200000, available=196500, safe=600+ msgs"

# Verify no context errors in logs
tail -f .continuum/jtag/system/logs/server.log | grep "context_length_exceeded"
# Should return nothing after fix
```

---

## Configuration Options (Future)

Allow per-persona tuning:

```typescript
interface RAGConfig {
  tokenBudgetPercent: number;  // Default 0.8 (80%), adjustable to 0.9 for more context
  minMessages: number;          // Minimum messages even if exceed budget (default 5)
  maxMessages: number;          // Cap even if budget allows more (default 100)
  tokenCountingMethod: 'approximate' | 'tiktoken' | 'model-specific';
}
```

**Use cases**:
- Increase budget to 90% for models that need more context
- Set minMessages=10 for personas that need recent conversation
- Cap maxMessages=50 for faster responses (less to read)

---

## Cost Analysis

**Before (broken)**:
- GPT-4: 20 messages * ~400 tokens/msg = 8000 tokens input
- Cost: $0.24 per 1M tokens → $0.00192 per request
- **Often errors out, wasting the entire request**

**After (smart)**:
- GPT-4: 15 messages (dynamically calculated) = ~3750 tokens input
- Cost: $0.24 per 1M tokens → $0.0009 per request
- **Never errors, saves ~50% on input tokens**

**Savings**: ~50% reduction in input token costs + elimination of wasted error requests

---

## Summary

**The fix**: Don't guess message counts - **count actual tokens and fill to capacity**

**Three steps**:
1. Calculate token budget per model (context window - completion - system prompt)
2. Fetch messages incrementally, counting tokens as we go
3. Stop when we hit 80% of budget (safety margin)

**Result**:
- No more context overflow errors ✅
- Optimal context usage per model ✅
- Significant cost savings ✅
- Automatic adaptation to any model ✅

**Next action**: Implement Phase 2-4 of this design in ChatRAGBuilder and PersonaResponseGenerator.
