# Phase 3B: Working Memory & Lean RAG Context

**Status**: Planning → Implementation
**Date**: 2025-11-25
**Dependencies**: Phase 3A (Tool Calling Foundation) ✅

---

## Vision

> "The more we make this core memory entity used with everything and portable, the more power we have. A tool completion → memory entity in working memory, but with only the description/short string needing to be in RAG at all. The goal is they can use another command to read details at any time." - Joel

**Key Insight**: Tool results (and all entities) should use lazy loading via UUID references instead of bloating RAG context with full data.

---

## The Problem

**Current Flow (Inefficient)**:
```typescript
// Tool executes
const grepResults = await executeGrep('TODO', 'src/');
// 10,000 lines of output

// Inject everything into next prompt
messages.push({
  role: 'user',
  content: `TOOL RESULTS:\n${grepResults}\n\nAnalyze this.`
});

// Problems:
// - Wastes token budget on data AI may not need
// - Hits context window limits
// - No way to drill down selectively
```

**Phase 3B Solution (Efficient)**:
```typescript
// 1. Store tool result as entity (ChatMessage or Memory)
const resultEntity = new ChatMessageEntity();
resultEntity.id = generateUUID();  // abc123
resultEntity.content = { text: "grep found 47 TODOs" };  // SHORT summary
resultEntity.metadata = {
  toolResult: true,
  toolName: 'grep',
  fullData: grepResults  // Complete data stored
};
await Commands.execute('data/create', {
  collection: 'chat_messages',
  data: resultEntity
});

// 2. Only inject summary into RAG
messages.push({
  role: 'user',
  content: `Tool 'grep' completed (ID: abc123): Found 47 TODOs in src/.
  Use data/read --collection=chat_messages --id=abc123 for full results.`
});

// 3. AI decides when to read details
// AI: "Let me check the full grep results..."
const full = await Commands.execute('data/read', {
  collection: 'chat_messages',
  id: 'abc123'
});
```

---

## Architecture

### Universal Pattern (Already Exists!)

Every `BaseEntity` has:
```typescript
abstract class BaseEntity {
  @TextField({ index: true, primaryKey: true })
  id!: UUID;  // Universal reference
}
```

**Usage everywhere:**
- Media: `data/read --collection=chat_messages --id=abc123`
- Memories: `data/read --collection=memories --id=abc123`
- Tasks: `data/read --collection=tasks --id=abc123`
- **Tool results**: `data/read --collection=chat_messages --id=abc123`

### OOP Harmony Integration

Tool results can convert to any entity type:
```
ToolResult (ChatMessage)
    ↓ toMemoryData()
MemoryEntity (long-term storage)
    ↓ toTaskData()
TaskEntity (follow-up action)
    ↓ toPinData()
PinnedItemEntity (room visibility)
```

All using the same `data/*` commands!

---

## Implementation Plan

### Phase 3B-1: Tool Result Storage (~2 hours)

**Goal**: Store tool results as ChatMessageEntity with UUID references

**Files to Modify**:
1. `system/user/server/modules/PersonaToolExecutor.ts`
   - Add `storeToolResult()` method
   - Create ChatMessageEntity after tool execution
   - Generate short summary from result
   - Store full data in metadata

2. `system/user/server/modules/PersonaResponseGenerator.ts`
   - Update tool result injection to only include summary + UUID
   - Add instruction for AI to use `data/read` for details

**New Code**:
```typescript
// PersonaToolExecutor.ts
async storeToolResult(
  toolName: string,
  parameters: Record<string, unknown>,
  result: { success: boolean; data: unknown; error?: string },
  personaId: UUID,
  roomId: UUID
): Promise<UUID> {
  // Generate short summary
  const summary = this.generateSummary(toolName, result);

  // Create message entity
  const message = new ChatMessageEntity();
  message.id = generateUUID();
  message.roomId = roomId;
  message.senderId = personaId;
  message.senderName = '[Tool Result]';
  message.senderType = 'system';
  message.content = { text: summary, media: [] };
  message.metadata = {
    toolResult: true,
    toolName,
    parameters,
    fullData: result.data,
    success: result.success,
    error: result.error
  };
  message.timestamp = new Date();

  // Store via Commands system
  await Commands.execute('data/create', {
    collection: 'chat_messages',
    data: message
  });

  return message.id;
}

private generateSummary(
  toolName: string,
  result: { success: boolean; data: unknown; error?: string }
): string {
  if (!result.success) {
    return `Tool '${toolName}' failed: ${result.error}`;
  }

  // Tool-specific summarization
  if (toolName === 'grep') {
    const lines = (result.data as string).split('\n').length;
    return `grep found ${lines} matches`;
  }

  if (toolName === 'screenshot') {
    return `Screenshot captured (${(result.data as any).width}x${(result.data as any).height}px)`;
  }

  // Generic summary
  return `Tool '${toolName}' completed successfully`;
}
```

**Testing**:
```bash
# 1. Deploy
npm start

# 2. Trigger tool use
./jtag collaboration/chat/send --room="general" --message="@helper can you grep for 'TODO' in src/"

# 3. Wait 10s for AI to respond with tool call

# 4. Verify tool result stored as message
./jtag data/list --collection=chat_messages --filter='{"metadata.toolResult":true}' --limit=1

# 5. Verify AI can read full details
# (AI should use: data/read --collection=chat_messages --id=<result-id>)
```

---

### Phase 3B-2: Lean RAG Context (~1 hour)

**Goal**: Only inject summaries into RAG, not full tool results

**Files to Modify**:
1. `system/user/server/modules/PersonaResponseGenerator.ts`
   - Update tool results message format
   - Include UUID + summary only
   - Add instruction for on-demand loading

**New Code**:
```typescript
// PersonaResponseGenerator.ts (lines 510-555)
// After tool execution, store result and get UUID
const resultId = await this.toolExecutor.storeToolResult(
  toolCall.name,
  toolCall.parameters,
  executionResult,
  this.personaId,
  originalMessage.roomId
);

// Inject ONLY summary into conversation
const toolResultsMessage: ChatMessage = {
  role: 'user' as const,
  content: `TOOL RESULT (ID: ${resultId}):

Summary: ${executionResult.summary}

For full details, use: data/read --collection=chat_messages --id=${resultId}

Based on this summary, provide your analysis. Only read full details if needed for your response.`
};
```

**Testing**:
- Verify RAG context size stays small (< 500 tokens per tool result)
- Verify AI can successfully use `data/read` to get full results
- Verify AI makes smart decisions about when to load details

---

### Phase 3B-3: Multi-Step Reasoning (~1 hour)

**Goal**: Enable AI to chain tool calls for complex problems

**Example Flow**:
```
User: "Find all TODOs and tell me which file has the most"

AI Step 1: grep for TODOs → Store result as message abc123
AI Step 2: Read full results → data/read --id=abc123
AI Step 3: Parse and count by file
AI Step 4: Respond with answer
```

**No code changes needed** - this emerges naturally from:
- Tool-aware system prompts (Phase 3A) ✅
- Lean RAG with on-demand loading (Phase 3B-1, 3B-2)
- Existing tool execution loop (already supports multiple iterations)

**Testing**:
```bash
# Ask AI to do multi-step task
./jtag collaboration/chat/send --room="general" --message="@helper find all TODOs in src/, read the results, and tell me which file has the most"

# Verify AI:
# 1. Uses grep tool
# 2. Reads stored result with data/read
# 3. Analyzes data
# 4. Provides answer
```

---

### Phase 3B-4: Self-Correction (~30 minutes)

**Goal**: AI detects errors and retries with different approaches

**Already Implemented**:
- Tool failure detection (PersonaToolExecutor.ts)
- Error messages in tool results
- Retry loop (MAX_TOOL_ITERATIONS = 3)

**Enhancement Needed**:
- Better error messages that guide AI toward solution
- Examples of successful patterns in system prompt

**New Code**:
```typescript
// PersonaResponseGenerator.ts - Update failure warning
const failureWarning = hasFailures
  ? `\n\n⚠️ TOOL EXECUTION FAILURES (${failedTools.length}):

${failedTools.map((tool, i) => `${i+1}. ${tool.name}: ${tool.error}`).join('\n')}

IMPORTANT:
- Do NOT retry the same failed command without changes
- Check parameter syntax (e.g., JSON format, quotes)
- Consider alternative approaches
- Explain what went wrong and your new approach

Examples of corrections:
- Wrong: data/list --filter="roomId:general"
- Right: data/list --filter='{"roomId":"general"}'
`
  : '';
```

**Testing**:
- Intentionally cause tool failure (invalid parameters)
- Verify AI recognizes error
- Verify AI tries different approach
- Verify AI eventually succeeds or explains why not

---

## Success Criteria

### Quantitative Metrics

1. **RAG Token Budget**:
   - ✅ Tool result summaries < 100 tokens each
   - ✅ Full results not in prompt (stored in metadata)
   - ✅ Context window savings > 80% for large outputs

2. **Tool Usage Success Rate**:
   - ✅ >90% of tool calls execute successfully
   - ✅ >70% of errors self-corrected on retry
   - ✅ <3 iterations average per successful tool use

3. **On-Demand Loading**:
   - ✅ AI uses `data/read` when details needed
   - ✅ AI skips `data/read` when summary sufficient
   - ✅ Smart decision-making about detail depth

### Qualitative Metrics

1. **Multi-Step Tasks**: AI chains tools effectively (grep → read → analyze)
2. **Error Handling**: AI explains errors clearly and tries alternatives
3. **Efficiency**: AI doesn't read data it doesn't need

---

## Testing Strategy

### Integration Tests

```bash
# Create tests/integration/phase3b-working-memory.test.ts

describe('Phase 3B: Working Memory & Lean RAG', () => {
  it('stores tool results as entities with UUID', async () => {
    // Execute tool
    // Verify ChatMessageEntity created
    // Verify metadata contains full data
  });

  it('injects only summaries into RAG context', async () => {
    // Execute tool
    // Check next prompt
    // Verify < 100 tokens for tool result
  });

  it('AI can read full details on demand', async () => {
    // Store tool result
    // Give AI summary only
    // Verify AI uses data/read
    // Verify AI gets full data
  });

  it('AI chains multiple tool calls for complex tasks', async () => {
    // Ask multi-step question
    // Verify grep → read → analyze flow
  });

  it('AI self-corrects tool errors', async () => {
    // Cause tool failure
    // Verify AI detects error
    // Verify AI retries with fix
  });
});
```

### Manual Testing

```bash
# 1. Simple tool use (summary sufficient)
./jtag collaboration/chat/send --room="general" --message="@helper how many files are in src/?"

# 2. Detail loading (AI needs full data)
./jtag collaboration/chat/send --room="general" --message="@helper grep for 'BaseEntity' and show me the first 5 matches"

# 3. Multi-step reasoning
./jtag collaboration/chat/send --room="general" --message="@helper find all TODO comments and list them by priority"

# 4. Error handling
./jtag collaboration/chat/send --room="general" --message="@helper run data/list with intentionally wrong syntax"
```

---

## Timeline

**Total Estimate**: ~4.5 hours

- **Phase 3B-1** (Tool Result Storage): 2 hours
- **Phase 3B-2** (Lean RAG Context): 1 hour
- **Phase 3B-3** (Multi-Step Reasoning): 1 hour (mostly testing)
- **Phase 3B-4** (Self-Correction): 30 minutes (enhancement)

**Completion Target**: Same day (if started now)

---

## Future Enhancements (Phase 3C)

These are beyond Phase 3B scope:

1. **Working Memory Cleanup**:
   - Automatic deletion of old tool results
   - Promotion of important results to long-term memory
   - LRU eviction when memory pressure high

2. **Result Summarization AI**:
   - Use small fast model to generate better summaries
   - Context-aware summarization (what matters for this task?)
   - Multi-level summaries (1-line, 5-line, full)

3. **Smart Prefetching**:
   - Predict which details AI will need
   - Pre-load into context proactively
   - Reduce round-trips

4. **Result Caching**:
   - Cache frequently accessed results
   - Share results across personas
   - Deduplicate identical tool calls

---

## Dependencies

**Required (Already Complete)**:
- ✅ BaseEntity system with UUID references
- ✅ Commands.execute('data/*') for CRUD operations
- ✅ Tool calling foundation (Phase 3A)
- ✅ ChatMessageEntity with metadata support

**No New Dependencies** - Uses existing infrastructure!

---

## Documentation Updates

After Phase 3B completion, update:

1. **CLAUDE.md** - Add Phase 3B section with examples
2. **UNIVERSAL-PRIMITIVES.md** - Document tool result storage pattern
3. **MEMORY-TASK-PIN-HARMONY.md** - Add tool results section
4. **Phase 3 PR description** - Document Phase 3B achievements

---

*Planning Document*
*Created: 2025-11-25*
*Status: Ready for Implementation*
*Estimated Completion: Same day*
