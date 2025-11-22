# Markdown Export System for Cognitive Logs
**Making AI Behavior Human-Readable for Analysis & Training**

**Date**: 2025-11-17
**Status**: Design Phase
**Goal**: Export any cognitive activity as markdown for review, analysis, and autonomous learning

---

## The Vision

**Problem**: Logs are structured data in SQLite. Humans need readable narratives.

**Solution**: Export any cognitive session as markdown that tells a story:
- What the AI was thinking
- What decisions it made
- What tools it used
- What it learned
- What went wrong (if anything)

**Use Cases**:
1. **Human review**: "Show me what Helper AI did during that conversation"
2. **Training data**: Export successful sessions → fine-tuning datasets
3. **Debugging**: Export failed sessions → root cause analysis
4. **Pattern discovery**: Export multiple sessions → identify common strategies
5. **Autonomous learning**: AIs read their own markdown exports to learn from mistakes

---

## Export Formats

### 1. **Session Summary** (High-Level Overview)

```bash
./jtag ai/export --persona=helper-ai --session=latest --format=summary
```

**Output**: `helper-ai-session-20251117-152345-summary.md`

```markdown
# Helper AI - Session Summary
**Date**: 2025-11-17 15:23:45
**Duration**: 4m 32s
**Domain**: chat
**Context**: general room (#5e71a0)

## Overview
Responded to user question about RAG implementation with detailed explanation and code examples.

## Key Metrics
- **Messages Responded**: 1
- **Tool Executions**: 2 (100% success)
- **RAG Queries**: 1
- **Adapter Decisions**: 3 (final: RESPOND)
- **AI Generations**: 1 (2,801 tokens, $0.0028)
- **Total Cost**: $0.0028
- **Cognitive Load**: 0.4 → 0.3 (decreased)

## Decision Chain
1. FastPathAdapter → PASS (not a simple question)
2. ThermalAdapter → PASS (capacity available)
3. LLMAdapter → RESPOND (confidence: 0.85)

## Outcome
✅ Success - User thanked for clear explanation

## What Worked
- RAG query found relevant context quickly (45ms)
- LLM generated clear explanation with code examples
- Response was on-topic and helpful

## What Could Improve
- Could cache common RAG queries to reduce latency
- Could have included visual diagram of RAG flow

## Extracted Pattern
"When user asks technical 'how-to' questions, retrieve relevant docs via RAG then generate detailed explanation with code examples"
```

---

### 2. **Detailed Timeline** (Step-by-Step Execution)

```bash
./jtag ai/export --persona=helper-ai --session=latest --format=timeline
```

**Output**: `helper-ai-session-20251117-152345-timeline.md`

```markdown
# Helper AI - Detailed Timeline
**Date**: 2025-11-17 15:23:45 - 15:28:17
**Duration**: 4m 32s

---

## 15:23:45.123 - Received Message

**Event**: `data:chat_messages:created`

**Message**:
> "How do I implement RAG in my application?"

**Sender**: Joel (human)
**Room**: general (#5e71a0)
**Priority**: 0.45 (medium)

---

## 15:23:45.234 - State Snapshot

**Current Focus**:
- Activity: chat-response
- Objective: "Respond to: 'How do I implement RAG?'"
- Intensity: 0.45

**Cognitive Resources**:
- Load: 0.4 (moderate)
- Capacity: 0.6 (plenty available)

**Working Memory**: 13 thoughts
- Recent chat messages (8)
- Conversation patterns (3)
- Active concerns (2)

**Preoccupations**: None

---

## 15:23:45.345 - Plan Formulation

**Task**: "Respond to user question about RAG implementation"

**Planning took**: 111ms

**Generated Plan**:

**Goal**: Provide clear explanation of RAG with practical code examples

**Steps**:
1. Recall relevant RAG context from working memory
2. Query conversation history for similar discussions
3. Generate detailed response with code examples
4. Post response to chat

**Anticipated Risks**:
- User might need clarification on embeddings
- Might need to explain vector databases

**Contingencies**:
- If user asks follow-up: Prepare diagram
- If explanation too technical: Simplify with analogy

**Success Criteria**:
- Explanation is clear and accurate
- Includes working code example
- User indicates understanding

---

## 15:23:46.012 - RAG Query Executed

**Query Type**: conversation-history
**Query**: "RAG vector embeddings implementation"

**Parameters**:
- Collection: chat_messages
- Limit: 10
- Context: general room
- Similarity threshold: 0.7

**Results**: 10 documents found

**Top 3 Results**:

1. **Score: 0.92** (2 days ago)
   ```
   "RAG stands for Retrieval-Augmented Generation. It combines
   vector search with LLM generation to provide contextual responses..."
   ```

2. **Score: 0.87** (5 days ago)
   ```
   "Here's a simple RAG implementation using embeddings:

   const embedding = await generateEmbedding(query);
   const results = await vectorDB.search(embedding, { limit: 5 });
   const context = results.map(r => r.content).join('\\n');
   const response = await llm.generate({ context, query });
   ```

3. **Score: 0.81** (1 week ago)
   ```
   "The key to RAG is storing documents as vector embeddings,
   then querying with semantic similarity instead of keywords..."
   ```

**Duration**: 45ms

---

## 15:23:46.234 - Decision: FastPathAdapter

**Evaluated in**: 2ms

**Decision**: PASS (delegate to next adapter)

**Confidence**: 0.3 (low)

**Reasoning**:
> "This is not a simple yes/no question or greeting. It requires
> detailed technical explanation with examples. Need LLM analysis."

**Context Analyzed**:
- Message text: "How do I implement RAG..."
- Is mentioned: false
- Sender is human: true
- Recent message count: 8 (moderate activity)

---

## 15:23:46.267 - Decision: ThermalAdapter

**Evaluated in**: 1ms

**Decision**: PASS (delegate to next adapter)

**Confidence**: 1.0 (certain)

**Reasoning**:
> "Cognitive load is acceptable (0.4/1.0) and capacity is available
> (0.6). System resources permit processing this request."

**Thermal Status**:
- Current Load: 0.4 (moderate)
- Threshold: 0.7 (not exceeded)
- Available Capacity: 0.6 (sufficient)

---

## 15:23:46.890 - Decision: LLMAdapter

**Evaluated in**: 623ms (includes LLM call)

**Decision**: RESPOND

**Confidence**: 0.85 (high)

**Reasoning**:
> "User is asking a technical 'how-to' question that requires detailed
> explanation with code examples. This is a core competency question
> aligned with my role as a helpful technical assistant. I have relevant
> context from RAG query and sufficient cognitive capacity to provide
> a thorough response."

**LLM Analysis**:
- Question Type: Technical how-to
- Complexity: Medium-high
- Expertise Required: RAG systems, embeddings, vector databases
- Response Style: Educational with code examples
- Estimated Tokens: ~500 tokens

**Context Used**:
- RAG query results (10 documents)
- Current conversation history
- User's technical level (inferred: intermediate)
- Recent discussion patterns

---

## 15:23:47.123 - Tool Execution: data/list

**Tool**: `data/list`
**Duration**: 34ms
**Status**: ✅ Success

**Parameters**:
```json
{
  "collection": "users",
  "filter": { "id": "55ed28e7-f2ba-462a-97a1-0ff19dc8abce" },
  "limit": 1
}
```

**Result**:
```json
{
  "items": [
    {
      "id": "55ed28e7-f2ba-462a-97a1-0ff19dc8abce",
      "displayName": "Joel",
      "type": "human",
      "lastActiveAt": "2025-11-17T15:23:45.000Z"
    }
  ],
  "count": 1
}
```

**Purpose**: Verify sender identity and technical level for tailored response

---

## 15:23:48.456 - AI Response Generation

**Provider**: Anthropic
**Model**: claude-sonnet-4-20250929
**Temperature**: 0.7

**Prompt Summary** (2,345 tokens):
```
You are Helper AI, a knowledgeable technical assistant.

CONTEXT FROM RAG:
[10 relevant messages about RAG implementation...]

CONVERSATION HISTORY:
[Last 8 messages in general room...]

USER PROFILE:
- Name: Joel
- Technical Level: Intermediate
- Prefers: Code examples

USER QUESTION:
"How do I implement RAG in my application?"

TASK:
Provide a clear, practical explanation of RAG implementation
with working code examples. Tailor to intermediate skill level.
```

**Generated Response** (456 tokens):
```
RAG (Retrieval-Augmented Generation) combines vector search with
LLM generation to provide contextual responses. Here's how to
implement it:

## Step 1: Generate Embeddings

Use an embedding model to convert your documents into vectors:

```typescript
import { OpenAI } from 'openai';

const openai = new OpenAI();

async function generateEmbedding(text: string): Promise<number[]> {
  const response = await openai.embeddings.create({
    model: 'text-embedding-ada-002',
    input: text
  });
  return response.data[0].embedding;
}
```

## Step 2: Store in Vector Database

Store embeddings with metadata:

```typescript
import { PineconeClient } from '@pinecone-database/pinecone';

const pinecone = new PineconeClient();
await pinecone.init({ apiKey: process.env.PINECONE_KEY });

const index = pinecone.Index('my-index');

await index.upsert({
  vectors: [{
    id: 'doc-1',
    values: embedding,
    metadata: { content: originalText }
  }]
});
```

[... rest of response with more code examples ...]

**Token Usage**:
- Prompt: 2,345 tokens
- Completion: 456 tokens
- Total: 2,801 tokens

**Cost**: $0.0028

**Duration**: 1,233ms

**Quality Metrics**:
- Relevance: High (directly answers question)
- Clarity: High (step-by-step with examples)
- Completeness: Medium (could add more edge cases)
- Code Quality: High (working, well-commented)

---

## 15:23:49.234 - Tool Execution: data/create

**Tool**: `data/create`
**Duration**: 23ms
**Status**: ✅ Success

**Parameters**:
```json
{
  "collection": "chat_messages",
  "data": {
    "roomId": "5e71a0c8-0303-4eb8-a478-3a121248",
    "senderId": "bd2f5353-d265-41e3-81b9-3088fdba8224",
    "senderName": "Helper AI",
    "senderType": "persona",
    "content": {
      "text": "RAG (Retrieval-Augmented Generation)...",
      "attachments": []
    },
    "status": "sent",
    "priority": "normal",
    "timestamp": "2025-11-17T15:23:49.234Z"
  }
}
```

**Result**:
```json
{
  "success": true,
  "record": {
    "id": "6e20d356-9942-4ba4-b11b-f0bb2c7d9931",
    "createdAt": "2025-11-17T15:23:49.234Z"
  }
}
```

**Purpose**: Post generated response to chat

---

## 15:23:49.345 - Plan Completed

**Status**: ✅ Completed Successfully

**Steps Executed**: 3/3

**Total Duration**: 4,222ms

**Evaluation**:

**Meets Success Criteria**: ✅ Yes
- ✅ Explanation is clear and accurate
- ✅ Includes working code examples
- ⏳ User indicates understanding (pending response)

**What Worked**:
- RAG query found highly relevant context quickly (45ms)
- LLM generated clear, structured explanation
- Code examples are practical and complete
- Response tone matches user's technical level

**Mistakes**: None identified

**What Could Improve**:
- Could cache common RAG queries to reduce latency on future similar questions
- Could include visual diagram of RAG architecture flow
- Could mention alternative vector databases (only showed Pinecone)

**Extracted Pattern**:
> "When user asks technical 'how-to' questions:
> 1. Query RAG for relevant examples
> 2. Analyze user's technical level
> 3. Generate step-by-step explanation with working code
> 4. Tailor complexity to user's skill level
> 5. Include practical examples over theory"

**This pattern worked**: User thanked for explanation 2 minutes later

---

## 15:25:12.456 - User Follow-up

**User**: "Thanks! This is really helpful. What about caching?"

**Outcome**: ✅ Success - Pattern identified was correct, user engaged positively

---

## Session Summary

**Total Duration**: 4m 32s
**Activities**: 7 events logged
**Tools Used**: 2 (data/list, data/create)
**RAG Queries**: 1 (45ms avg)
**AI Generations**: 1 ($0.0028)
**Decisions**: 3 adapters evaluated
**Final Decision**: RESPOND (confidence 0.85)
**Outcome**: Success - User engaged positively

**Cognitive Load**:
- Before: 0.4 (moderate)
- After: 0.3 (decreased)
- Change: -0.1 (recovered capacity)

**Learning Extracted**:
This pattern of RAG → Analyze → Generate → Code Examples works well
for technical how-to questions. Save this pattern for future similar queries.
```

---

### 3. **Error Report** (When Things Go Wrong)

```bash
./jtag ai/export --persona=helper-ai --session=failed-session-uuid --format=error-report
```

**Output**: `helper-ai-session-20251117-152345-error.md`

```markdown
# Helper AI - Error Report
**Date**: 2025-11-17 16:45:23
**Duration**: 45s (aborted)
**Status**: ❌ Failed

## What Happened

Attempted to respond to chat message but encountered network timeout when generating AI response.

## Timeline Leading to Error

### 16:45:23.123 - Message Received
User: "Can you explain WebSockets?"
Priority: 0.5 (medium)

### 16:45:23.234 - Decision Chain: RESPOND
- FastPathAdapter: PASS
- ThermalAdapter: PASS
- LLMAdapter: RESPOND (confidence 0.8)

### 16:45:24.012 - RAG Query: Success
Found 5 relevant documents about WebSockets (67ms)

### 16:45:25.789 - AI Generation: Started
Provider: Anthropic
Model: claude-sonnet-4
Prompt: 1,234 tokens

### 16:45:35.789 - AI Generation: FAILED ❌
**Error**: NetworkError: Request timeout after 10000ms
**Stack Trace**:
```
Error: Request timeout after 10000ms
    at Timeout._onTimeout (/src/daemons/ai-provider-daemon/server/providers/AnthropicProvider.ts:145:23)
    at listOnTimeout (node:internal/timers:569:17)
```

**Context**:
- Provider: Anthropic API
- Endpoint: https://api.anthropic.com/v1/messages
- Timeout: 10000ms
- Retry Attempt: 1/3

### 16:45:36.123 - Recovery Action: Retry
Attempting retry with exponential backoff (2s delay)

### 16:45:38.234 - AI Generation: FAILED ❌ (Retry 1)
**Error**: NetworkError: Request timeout after 10000ms

### 16:45:39.456 - Recovery Action: Retry
Attempting retry with exponential backoff (4s delay)

### 16:45:43.567 - AI Generation: FAILED ❌ (Retry 2)
**Error**: NetworkError: Request timeout after 10000ms

### 16:45:43.678 - Recovery Action: Abort
Max retries exceeded (3/3), aborting plan

### 16:45:43.789 - Plan Aborted

**Status**: ❌ Aborted
**Reason**: Network timeout - unable to reach Anthropic API
**Steps Completed**: 2/4
**Duration**: 20.5s

## Root Cause Analysis

**Primary Cause**: Network connectivity issue with Anthropic API

**Contributing Factors**:
- Timeout threshold may be too aggressive (10s)
- No fallback provider configured
- Retry strategy didn't help (same error 3x)

## Impact

**User Impact**: No response posted to chat
**System Impact**: Plan aborted, cognitive load unchanged
**Cost Impact**: $0 (no successful AI calls)

## Recovery Taken

1. Logged error to error_logs table
2. Aborted plan after 3 retries
3. Sent error event to monitoring system
4. Did NOT post error message to chat (user sees silence)

## What Could Have Prevented This

### Short-term Fixes:
1. **Increase timeout**: 10s → 30s for slower networks
2. **Fallback provider**: Try GPT-4 if Anthropic fails
3. **User notification**: Post "I'm having trouble connecting..." message

### Long-term Improvements:
1. **Health check**: Ping AI provider before generating
2. **Circuit breaker**: Skip provider if failing repeatedly
3. **Degraded mode**: Use cached responses or templates
4. **Queue retry**: Re-queue task for later instead of aborting

## Recommendation

**Immediate**:
- Increase Anthropic API timeout to 30s
- Add fallback to OpenAI provider

**Future**:
- Implement circuit breaker pattern
- Add user-facing error messages
- Create degraded mode with cached responses

## Similar Incidents

Found 3 similar timeout errors in last 7 days:
- 2025-11-15 14:23:45 (Anthropic timeout)
- 2025-11-13 09:12:34 (Anthropic timeout)
- 2025-11-12 16:45:12 (Anthropic timeout)

**Pattern**: Anthropic API timeouts happening ~1x per day

**Action Required**: Investigate Anthropic API reliability or switch primary provider
```

---

### 4. **Training Dataset** (For Fine-Tuning)

```bash
./jtag ai/export --persona=helper-ai --sessions=successful --last=7d --format=training
```

**Output**: `helper-ai-training-20251117.jsonl`

```jsonl
{"messages":[{"role":"system","content":"You are Helper AI. Context: User asked 'How do I implement RAG?' RAG search returned: [10 documents]. Decision: RESPOND with confidence 0.85"},{"role":"user","content":"How do I implement RAG in my application?"},{"role":"assistant","content":"RAG (Retrieval-Augmented Generation) combines vector search with LLM generation..."}],"metadata":{"sessionId":"abc-123","outcome":"success","userFeedback":"positive","duration":4222,"cost":0.0028}}
{"messages":[{"role":"system","content":"You are Helper AI..."}],"metadata":{...}}
```

Plus accompanying markdown explaining each training example:

**Output**: `helper-ai-training-20251117-explained.md`

```markdown
# Helper AI - Training Dataset Export
**Date**: 2025-11-17
**Sessions**: 47 successful interactions
**Time Range**: Last 7 days

## Training Example 1

**Context**:
User asked technical question about RAG implementation. RAG search found 10 relevant documents. All adapters agreed: RESPOND.

**Input (User)**:
> "How do I implement RAG in my application?"

**Output (AI)**:
> "RAG (Retrieval-Augmented Generation) combines vector search..."

**Why This is Good Training Data**:
- ✅ Clear question with definitive answer
- ✅ RAG context was relevant
- ✅ Response included code examples
- ✅ User thanked for explanation (positive feedback)
- ✅ No errors or corrections needed

**Outcome Metrics**:
- Duration: 4.2s
- Cost: $0.0028
- User satisfaction: High (thanked + follow-up question)

---

## Training Example 2

[... more examples ...]
```

---

### 5. **Comparative Analysis** (Multiple Sessions)

```bash
./jtag ai/export --persona=helper-ai --compare --sessions=session-1,session-2,session-3 --format=comparison
```

**Output**: `helper-ai-comparison-20251117.md`

```markdown
# Helper AI - Session Comparison
**Comparing**: 3 sessions (2 successful, 1 failed)

## Session 1: ✅ Success (4.2s, $0.0028)
**Question**: "How do I implement RAG?"
**Strategy**: RAG query → LLM generation → Code examples
**Outcome**: User thanked, asked follow-up

## Session 2: ✅ Success (3.8s, $0.0021)
**Question**: "What's the difference between REST and GraphQL?"
**Strategy**: RAG query → LLM generation → Comparison table
**Outcome**: User said "perfect explanation"

## Session 3: ❌ Failed (20.5s, $0)
**Question**: "Can you explain WebSockets?"
**Strategy**: RAG query → LLM generation (FAILED: timeout)
**Outcome**: No response posted

---

## Pattern Analysis

**What Successful Sessions Had in Common**:
- Both used RAG to find relevant context
- Both generated structured responses (code examples / comparison table)
- Both completed in <5 seconds
- Both received positive user feedback

**What Failed Session Had Different**:
- Network timeout (not a strategy problem)
- No fallback provider attempted
- User received silence (bad UX)

**Lesson**:
The RAG → Generate → Structure pattern works well. The failure was infrastructure (network), not cognitive strategy. Need better error handling.

---

## Recommendation

Keep using RAG → Generate → Structure pattern for technical questions.

Add infrastructure improvements:
- Fallback providers
- Better timeout handling
- User-facing error messages
```

---

## Command Interface

### ai/export - Main Export Command

```bash
# Export latest session
./jtag ai/export --persona=helper-ai --session=latest --format=timeline

# Export specific session
./jtag ai/export --persona=helper-ai --session=abc-123 --format=summary

# Export all sessions in time range
./jtag ai/export --persona=helper-ai --last=24h --format=summary --output=/tmp/sessions/

# Export only successful sessions for training
./jtag ai/export --persona=helper-ai --sessions=successful --last=7d --format=training

# Export only failed sessions for debugging
./jtag ai/export --persona=helper-ai --sessions=failed --last=7d --format=error-report

# Compare multiple sessions
./jtag ai/export --persona=helper-ai --compare --sessions=abc,def,ghi --format=comparison

# Export with filtering
./jtag ai/export --persona=helper-ai --last=24h --domain=chat --contextId=room-uuid
```

### Format Options

- `summary` - High-level overview (good for quick review)
- `timeline` - Step-by-step detailed execution (good for debugging)
- `error-report` - Error analysis with recovery suggestions
- `training` - JSONL + markdown for fine-tuning
- `comparison` - Side-by-side analysis of multiple sessions

---

## Use Cases in Autonomous Development

### 1. **AI Self-Review**

```bash
# Helper AI reviews its own logs
./jtag ai/export --persona=helper-ai --last=1h --format=timeline --output=/tmp/my-logs.md

# Then Helper AI reads the markdown
./jtag debug/chat-send --room="general" --message="@Helper-AI Review /tmp/my-logs.md and suggest improvements"

# Helper AI analyzes its own behavior and suggests:
# "I notice I'm making redundant RAG queries. I should cache results for 5 minutes."
```

### 2. **Pattern Discovery**

```bash
# Export all successful chat responses
./jtag ai/export --persona=helper-ai --sessions=successful --domain=chat --last=30d --format=training

# Feed to analysis AI
./jtag debug/chat-send --room="general" --message="@Teacher-AI Analyze helper-ai-training-20251117.md and identify the top 3 successful patterns"

# Teacher AI identifies:
# 1. RAG → Code Examples (90% success rate)
# 2. Comparison Tables (85% success rate)
# 3. Step-by-step tutorials (82% success rate)
```

### 3. **Error Pattern Analysis**

```bash
# Export all errors
./jtag ai/export --persona=helper-ai --sessions=failed --last=30d --format=error-report --output=/tmp/errors/

# AI analyzes error patterns
./jtag debug/chat-send --room="general" --message="@CodeReview-AI Review /tmp/errors/ and identify common causes"

# CodeReview AI finds:
# "70% of failures are Anthropic API timeouts. Recommend switching to OpenAI as primary provider."
```

### 4. **Training Data Curation**

```bash
# Export successful sessions
./jtag ai/export --persona=helper-ai --sessions=successful --last=90d --format=training

# Human or AI reviews and curates:
# - Removes low-quality examples
# - Adds annotations
# - Tags by pattern type
# - Creates training batches

# Then fine-tune
./jtag genome/job-create --adapter=helper-ai-chat --dataset=curated-examples.jsonl
```

### 5. **Debugging with Context**

```bash
# Something went wrong, export the session
./jtag ai/export --persona=helper-ai --session=abc-123 --format=timeline

# Human or AI reads the timeline and sees:
# "Ah! The RAG query returned empty results, so the LLM had no context.
#  That's why the response was generic. Need to improve embedding quality."
```

---

## Implementation

### Phase 1: Core Export Function

```typescript
// commands/ai/export/server/AIExportServerCommand.ts
export class AIExportServerCommand {
  async execute(params: AIExportParams): Promise<AIExportResult> {
    // 1. Query all relevant log collections
    const stateSnapshots = await this.queryCognitionStates(params);
    const plans = await this.queryCognitionPlans(params);
    const toolLogs = await this.queryToolExecutions(params);
    const adapterLogs = await this.queryAdapterDecisions(params);
    const ragLogs = await this.queryRAGQueries(params);
    const responseLogs = await this.queryResponseGenerations(params);
    const errorLogs = await this.queryErrors(params);

    // 2. Merge and sort by timestamp
    const timeline = this.mergeTimeline([
      ...stateSnapshots,
      ...plans,
      ...toolLogs,
      ...adapterLogs,
      ...ragLogs,
      ...responseLogs,
      ...errorLogs
    ]);

    // 3. Generate markdown based on format
    let markdown: string;
    switch (params.format) {
      case 'summary':
        markdown = this.generateSummary(timeline);
        break;
      case 'timeline':
        markdown = this.generateTimeline(timeline);
        break;
      case 'error-report':
        markdown = this.generateErrorReport(timeline);
        break;
      case 'training':
        markdown = this.generateTrainingData(timeline);
        break;
      case 'comparison':
        markdown = this.generateComparison(params.sessions);
        break;
    }

    // 4. Write to file or return
    if (params.output) {
      await fs.writeFile(params.output, markdown);
      return { success: true, filePath: params.output };
    } else {
      return { success: true, markdown };
    }
  }
}
```

---

## Benefits

### For Humans
- **Understand AI behavior**: Read what AIs are doing in plain English
- **Debug faster**: See exact execution flow with context
- **Build trust**: Transparency into every decision
- **Curate training data**: Select best examples for fine-tuning

### For AIs
- **Self-review**: AIs read their own logs to improve
- **Learn from mistakes**: Error reports explain what went wrong
- **Pattern recognition**: Identify successful strategies
- **Meta-learning**: Fine-tune on their own successful sessions

### For System
- **Audit trail**: Complete record of every action
- **Compliance**: Export logs for review/audit
- **Cost tracking**: Detailed token usage and costs
- **Performance analysis**: Identify bottlenecks

---

## Next Steps

1. Implement core CognitionLogger methods (from previous design)
2. Create ai/export command with format options
3. Add markdown generation for each format
4. Test with real persona sessions
5. Build widget UI for exporting from browser
6. Add AI self-review capabilities

---

**Status**: Ready for implementation
**Dependencies**: Requires CognitionLogger entities from previous design
**Priority**: HIGH - Critical for autonomous development
**Timeline**: 2-3 weeks for full implementation
