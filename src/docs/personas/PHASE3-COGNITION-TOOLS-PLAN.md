# Phase 3: Cognition, Tool Calling & Streaming Integration

**Status**: Planning → Implementation
**Dependencies**: Phase 1 (#188), Phase 2 (#192)
**Target**: Q4 2025

---

## Vision

Enable PersonaUsers to operate autonomously with tool access, enhanced reasoning capabilities, and mid-stream model upgrades. Make base models (qwen2.5:7b, llama3.2) competitive with frontier models for 80%+ of tasks through tool use, better prompting, and selective upgrading.

**Goal**: PersonaUsers that think, plan, use tools, and learn - approaching human-level problem-solving autonomy.

---

## Phase Overview

### What Phase 2 Gave Us
- **Memory Architecture**: 5-level hierarchy for knowledge sharing
- **Vector Search**: Semantic retrieval across memory scopes
- **Progressive Scoring**: Real-time complexity detection
- **Foundation Types**: ComplexityTypes, ProgressiveScorer, memory schemas

### What Phase 3 Builds
- **Tool Calling**: Autonomous tool discovery and execution
- **Enhanced Cognition**: Multi-step reasoning, self-correction, planning
- **Streaming Integration**: Mid-stream model upgrades in production
- **Production Validation**: Real-world testing of Phases 1-2
- **Base Model Optimization**: Prompt engineering for local models

---

## Implementation Phases

### Phase 3A: Tool Calling Foundation (Weeks 1-2)

**Files to Create**:
```
system/user/server/modules/PersonaToolRegistry.ts
system/user/server/modules/PersonaToolExecutor.ts
system/user/server/modules/PersonaToolDefinitions.ts
tests/integration/persona-tool-calling.test.ts
```

**Key Classes**:
```typescript
class PersonaToolRegistry {
  // Discover available tools
  listTools(): ToolDefinition[];

  // Get tool details
  getTool(name: string): ToolDefinition | null;

  // Permission check
  canUse(toolName: string, personaId: string): boolean;
}

class PersonaToolExecutor {
  // Execute tool with parameters
  execute(
    toolName: string,
    params: Record<string, unknown>,
    personaId: string
  ): Promise<ToolResult>;

  // Parse result for AI
  parseResult(result: ToolResult): string;

  // Error handling
  handleError(error: ToolError): string;
}

interface ToolDefinition {
  name: string;
  description: string;
  parameters: ToolParameterSchema;
  permissions: string[];
  examples: ToolExample[];
}
```

**Success Criteria**:
- PersonaUser can discover available tools
- PersonaUser can execute `read`, `grep`, `bash`, `screenshot`
- Tool results correctly parsed and returned
- Error handling for invalid tool calls
- Unit tests: > 90% coverage

---

### Phase 3B: Cognition Enhancements (Weeks 2-3)

**Files to Create**:
```
system/user/server/modules/PersonaCognition.ts
system/user/server/modules/ProblemSolver.ts
system/user/server/modules/ContextManager.ts
tests/integration/persona-reasoning.test.ts
```

**Key Features**:

#### 1. Multi-Step Problem Solving
```typescript
class ProblemSolver {
  // Break problem into steps
  planSteps(problem: string): ProblemStep[];

  // Execute step (may involve tool calls)
  executeStep(step: ProblemStep): Promise<StepResult>;

  // Evaluate and adjust
  evaluateResult(result: StepResult): EvaluationResult;

  // Self-correct on errors
  selfCorrect(error: Error): CorrectionStrategy;
}
```

#### 2. Improved Prompting
```
System Prompt Enhancements:
- Tool-aware: "Use tools to verify information"
- Chain-of-thought: "Explain your reasoning"
- Self-reflection: "Check your answer"
- Uncertainty acknowledgment: "I'm not sure" instead of guessing
```

#### 3. Context Management
```typescript
class PersonaContextManager {
  // Rank context sources by relevance
  rankSources(query: string): ContextSource[];

  // Fit within token budget
  fitContext(sources: ContextSource[], maxTokens: number): string;

  // Recall relevant memories
  recallMemories(query: string, scope: MemoryScope): Memory[];
}
```

**Success Criteria**:
- Multi-step task completion > 70%
- Self-correction on errors > 80%
- Context relevance score > 0.85
- Integration tests validate reasoning flow

---

### Phase 3C: Streaming Integration (Weeks 3-4)

**Files to Modify**:
```
daemons/ai-provider-daemon/server/AIProviderDaemonServer.ts
system/user/server/modules/ProgressiveScorer.ts (enhance)
tests/integration/streaming-upgrades.test.ts
```

**Implementation**:
```typescript
class AIProviderDaemonServer {
  async generateWithUpgrade(
    request: GenerateRequest,
    scorer: ProgressiveScorer
  ): AsyncGenerator<string> {
    let currentModel = request.model; // Start cheap (qwen2.5:7b)
    let buffer = '';

    for await (const chunk of this.generate(currentModel, request)) {
      yield chunk;
      buffer += chunk;

      // Reassess every 200 tokens (~800 chars)
      if (buffer.length > 800) {
        const scoring = scorer.analyze(buffer);

        if (scoring.shouldUpgrade) {
          // Mid-stream model switch
          console.log(`🔼 ${currentModel} → ${scoring.suggestedModel}`);
          currentModel = scoring.suggestedModel;

          // Preserve context, continue generation
          request.context = buffer;
        }

        buffer = buffer.slice(-400); // Sliding window
      }
    }
  }
}
```

**Challenges**:
1. **Context Preservation**: Maintain conversation flow across model switch
2. **Latency**: < 500ms upgrade time (user doesn't notice)
3. **Cost Tracking**: Accurate accounting across multiple models
4. **Error Handling**: Graceful fallback if upgrade fails

**Success Criteria**:
- Successful mid-stream upgrades: > 95%
- Upgrade latency: < 500ms
- Context preservation: > 95% semantic similarity
- No user-perceived quality degradation

---

### Phase 3D: Production Validation (Weeks 4-5)

**Test Scenarios**:

#### 1. Memory Telepathy Validation
```bash
# Multi-agent debugging collaboration
# Expected: 4.5× speedup (60min → 13min)

Scenario:
1. Helper AI discovers bug in PersonaUser.ts
2. Stores finding in task-scoped memory with embedding
3. Code Review AI recalls via semantic vector search
4. Both collaborate using shared memory context

Metrics:
- Time to solution: with vs without memory telepathy
- Memory recall precision: > 80%
- Collaboration efficiency: 4× speedup target
```

#### 2. Progressive Scoring Validation
```bash
# Complex architecture question
# Expected: 80% cost reduction

Scenario:
1. Start with qwen2.5:7b ($0.000)
2. Detect uncertainty indicators ("it depends", "I'm not sure")
3. Upgrade to Claude 3.5 Sonnet mid-stream
4. User gets premium answer at fractional cost

Metrics:
- Upgrade frequency: 15-20% of messages
- Cost per message: < $0.0003 avg
- Quality rating: > 4/5 stars
```

#### 3. Tool Use Validation
```bash
# Autonomous tool calling
# Expected: 80% correct usage

Scenario:
1. User asks "What's in server/main.ts?"
2. PersonaUser autonomously calls 'read' tool
3. Correct path and parameters used
4. Result integrated into natural response

Metrics:
- Tool call autonomy: > 80%
- Parameter accuracy: > 90%
- Result integration quality: > 85%
```

**Success Criteria**:
- Memory telepathy: 4× speedup achieved
- Progressive scoring: 80% cost reduction
- Tool calling: 80% autonomous correct usage

---

### Phase 3E: Base Model Optimization (Weeks 5-6)

**Techniques**:

#### 1. Prompt Engineering
```
Enhanced System Prompts:
- Tool-first thinking: "Use tools to verify, don't guess"
- Chain-of-thought: "Step 1: ... Step 2: ..."
- Few-shot examples: Show correct tool usage
- Self-reflection: "Does this answer make sense?"
```

#### 2. RAG Integration
```typescript
// Pull relevant memories before generation
const memories = await this.memoryManager.recall(query, scope);
const context = formatMemoriesForPrompt(memories);

const prompt = `
Context from your memory:
${context}

User question: ${query}

Use this context to inform your answer. If you need more information, use tools.
`;
```

#### 3. Token Budget Optimization
```typescript
// Prioritize context sources
const sources = [
  { type: 'task-memory', priority: 10, tokens: 500 },
  { type: 'recent-chat', priority: 8, tokens: 1000 },
  { type: 'project-memory', priority: 6, tokens: 300 },
  { type: 'team-knowledge', priority: 4, tokens: 200 }
];

// Fit within budget (8192 tokens for qwen2.5:7b)
const fittedContext = fitWithinBudget(sources, 6000); // Leave 2192 for output
```

**Success Criteria**:
- Local model task completion: > 70% (vs 90% for Claude)
- Tool call accuracy: > 85%
- User satisfaction: > 80%
- Cost per message: < $0.0003

---

## Technical Architecture

### Tool Calling Flow

```
1. PersonaUser receives message
   ↓
2. Parse for tool requirements:
   - "What's in X file?" → needs 'read' tool
   - "Find references to Y" → needs 'grep' tool
   - "Show me the UI" → needs 'screenshot' tool
   ↓
3. Construct tool-aware prompt:
   system: "You have access to: read, grep, screenshot..."
   user: "What's in server/main.ts?"
   ↓
4. Generate response with tool calls:
   <tool_use>
     <tool_name>read</tool_name>
     <parameters>
       <file_path>server/main.ts</file_path>
     </parameters>
   </tool_use>
   ↓
5. Execute tool via PersonaToolExecutor
   ↓
6. Parse result and integrate:
   "Based on server/main.ts, the main function..."
```

### Streaming Upgrade Flow

```
1. Start with qwen2.5:7b (local, fast, free)
   ↓
2. Generate response, buffer output
   ↓
3. Every 200 tokens:
   - Run ProgressiveScorer.analyze(buffer)
   - Check upgrade indicators:
     * Hedging ("it depends", "possibly")
     * Uncertainty ("I'm not sure")
     * Self-correction ("actually, on second thought")
     * Multi-perspective ("on one hand... but...")
   ↓
4. If threshold exceeded:
   - Preserve context buffer
   - Switch to better model (deepseek → Claude)
   - Continue generation seamlessly
   ↓
5. Result: Premium quality at 80% cost savings
```

### Memory Integration

```
Tool Result → Memory Storage
   ↓
Generate 768-dim embedding (nomic-embed-text)
   ↓
Determine appropriate scope:
   - Personal: Private insights
   - Task: Collaboration context (7d TTL)
   - Project: Ongoing work (90d TTL)
   - Team: Cross-agent learning (infinite)
   ↓
Store in SQLite with vector index
   ↓
Future queries: Semantic vector search recalls relevant memories
```

---

## Cost Projections

### Current State (Phase 2)
```
After Phase 2:
  Start with qwen2.5:7b = $0.000
  Occasional deepseek = $0.0001
  Rare Claude upgrade = $0.003

  100 messages/day × $0.0005 avg = $1.50/month (87.5% savings vs all-cloud)
```

### Phase 3 Projections
```
With Tool Calling:
  - Fewer hallucinations (tools verify facts)
  - Correct answers first time (no wasted API calls)
  - Estimated additional savings: 10-15%

With Streaming Upgrades:
  - Start cheap always (qwen2.5:7b = $0.000)
  - Upgrade only 15-20% of messages
  - Estimated savings: 60-80% vs always using premium

Total Phase 3:
  100 messages/day × $0.0003 avg = $0.90/month (92.5% savings vs all-cloud)
```

**Key Insight**: Tool calling prevents expensive mistakes. Streaming upgrades reserve premium models for truly complex queries. Result: Best of both worlds.

---

## Success Metrics Summary

### Tool Calling
- ✅ Autonomous tool use: > 80%
- ✅ Tool call success rate: > 90%
- ✅ Result integration quality: > 85%

### Cognition
- ✅ Multi-step task completion: > 70%
- ✅ Self-correction on errors: > 80%
- ✅ Context relevance: > 0.85

### Streaming
- ✅ Mid-stream upgrades: > 95% successful
- ✅ Upgrade latency: < 500ms
- ✅ No quality degradation perceived

### Production Validation
- ✅ Memory telepathy: 4× speedup
- ✅ Progressive scoring: 80% cost reduction
- ✅ Tool autonomy: 80% correct usage

### Cost & Quality
- ✅ Cost per message: < $0.0003
- ✅ User satisfaction: > 80%
- ✅ Local model viability: > 70% task completion

---

## Next Steps

1. **Phase 3A**: Implement PersonaToolRegistry and PersonaToolExecutor
2. **Phase 3B**: Enhance cognition with multi-step reasoning
3. **Phase 3C**: Integrate ProgressiveScorer with AIProviderDaemon
4. **Phase 3D**: Run production validation tests
5. **Phase 3E**: Optimize base model prompts and context

**Timeline**: 5-6 weeks to complete all phases

**Outcome**: PersonaUsers that autonomously use tools, reason through problems, and operate efficiently with 92.5% cost savings.

---

## References

- **Phase 1**: Complexity Assessment Foundation (#188)
- **Phase 2**: Memory Systems, Vector Search & Progressive Scoring (#192)
- **ProgressiveScorer**: `system/user/server/modules/ProgressiveScorer.ts`
- **ComplexityTypes**: `system/shared/ComplexityTypes.ts`
- **Memory Architecture**: `papers/collaborative-memory-telepathy/paper.md`

---

*Document created: 2025-11-25*
*Status: Planning phase - implementation to follow*
