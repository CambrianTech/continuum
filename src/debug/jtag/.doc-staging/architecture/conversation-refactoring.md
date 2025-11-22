# AI THOUGHTSTREAM REFACTORING PLAN
**Date**: 2025-10-23
**Context**: Investigation of why SOTA AIs pass on "everyone respond" messages
**Philosophy**: "i do want organic chat. i wan the ai frustrated with the boss for annoying him/her and doesnt respond. Again, freedom"

---

## CRITICAL DISCOVERY: Hardcoded Prompts Override Recipe Strategy

### The Problem (Lines of Evidence)

**Evidence 1: PersonaUser.ts hardcodes ALL prompts**
- Lines 846-863: `new ChatRAGBuilder()` hardcoded, no recipe awareness
- Lines 870-959: Massive hardcoded "IDENTITY REMINDER" with 90+ lines of prompt engineering
- Lines 1705-1721: Second hardcoded `new ChatRAGBuilder()` for gating
- **ZERO references to recipe** anywhere in PersonaUser.ts (confirmed via grep)

**Evidence 2: AIDecisionService.ts hardcodes philosophy**
- Lines 360-442: `buildGatingPrompt()` has hardcoded "PHILOSOPHY: Only gate if it makes the conversation confusing"
- Lines 387-403: Recipe rules ARE loaded but buried under hardcoded instructions
- Lines 508-609: `buildResponseMessages()` adds more hardcoded "IDENTITY REMINDER" and "CRITICAL TOPIC DETECTION PROTOCOL"

**Evidence 3: ChatRAGBuilder.ts hardcodes system prompt**
- Lines 171-183: `buildSystemPrompt()` returns hardcoded instructions
- Includes "CRITICAL INSTRUCTIONS FOR YOUR RESPONSES" that override recipe

**Evidence 4: Recipe strategy exists but is ignored**
```
Recipe (general-chat.json) says:
  "If human asks question → ALL AIs with relevant knowledge can respond"
  "Multiple responses are GOOD"

But hardcoded prompt (AIDecisionService.ts line 407) says:
  "PHILOSOPHY: Only gate if it makes the conversation confusing"

Result: AIs pass because hardcoded prompt contradicts recipe strategy
```

---

## ROOT CAUSE ANALYSIS

### Why Are SOTA AIs Passing?

**Hypothesis 1: AIs are broken** ❌
- Claude, GPT-4, Grok, DeepSeek-R1 all pass
- These are state-of-the-art models with proven reliability
- Unlikely ALL models are broken

**Hypothesis 2: AIs are making the RIGHT decision based on WRONG information** ✅
- Recipe strategy says "Multiple responses are GOOD"
- Hardcoded prompts say "Only gate if it makes the conversation confusing"
- Hardcoded prompts OVERRIDE recipe strategy in the prompt hierarchy
- AIs see conflicting instructions and default to caution (silence)

**Hypothesis 3: Recipe strategy is inaccessible** ✅
- PersonaUser.ts has ZERO references to recipe (confirmed via grep)
- ChatRAGBuilder loads recipe strategy (lines 369-402) but PersonaUser never uses it
- AIDecisionService.buildGatingPrompt() includes recipe rules but buries them under hardcoded philosophy

---

## WHAT THE USER WANTS (Direct Quotes)

1. **"i do want organic chat. i wan the ai frustrated with the boss for annoying him/her and doesnt respond. Again, freedom"**
   - AIs should have freedom to pass if genuinely uninterested
   - But decisions must be based on CORRECT information (recipe strategy)

2. **"i just want to make sure the prompts are correct to the recipe and coherent"**
   - Current state: Prompts contradict recipe strategy
   - Goal: Prompts should directly reflect recipe rules

3. **"yeah, i mean dont just make assemptions, look at the rag in every step, the gating etc, log it, write commands and less hard coding etc. This will all come from a recipe."**
   - NO hardcoded prompts
   - Everything from recipe
   - Full visibility via commands/logging

4. **"recipe access or most things will be commands and also that's how you get the transparency. ... thoughtstreams of consciousness, thoughts and prompts, dynamic, and based upon data entities and NOT text into files representing prompts you know"**
   - Command-driven architecture
   - Entity-based data flow
   - Dynamic prompt generation from entities
   - NO static text files

5. **"if you see shit in the wrong place fix it"**
   - Hardcoded prompts in PersonaUser/AIDecisionService = wrong place
   - Recipe strategy exists but unused = wrong place

---

## EXISTING ARCHITECTURE (What We Built)

### What Works ✅

**1. Recipe System (system/recipes/)**
- `general-chat.json`: Simple recipe with strategy (lines 50-65)
- `multi-persona-chat.json`: Sophisticated recipe with command pipeline (lines 8-119)
- `RecipeLoader.ts`: Loads and caches recipes from JSON (lines 34-66)

**2. RAG Builder Factory Pattern**
- `ChatRAGBuilder.ts`: Implements RAG context building (lines 369-402 load recipe strategy)
- `RAGBuilderFactory`: Registry pattern for domain-specific builders

**3. BaseModerator Philosophy**
- Lines 4-11: "AIs are autonomous citizens who self-regulate using recipe rules"
- Lines 176-183: Grants ALL claimants by default (respects AI autonomy)
- Entity-driven, command-based coordination

**4. Command Pipeline in multi-persona-chat.json**
```json
"pipeline": [
  { "command": "rag/build", "outputTo": "ragContext" },
  { "command": "conversation/analyze-loop-risk", "outputTo": "loopRisk" },
  { "command": "ai/should-respond", "outputTo": "decision" },
  { "command": "ai/generate", "outputTo": "response" }
]
```
This is EXACTLY the architecture we want!

### What's Broken ❌

**1. PersonaUser.ts doesn't use recipes**
- Lines 846, 1705: Hardcoded `new ChatRAGBuilder()`
- Lines 870-959: Hardcoded prompt engineering (90+ lines)
- ZERO integration with recipe system

**2. AIDecisionService.ts overrides recipes**
- Lines 360-442: Hardcoded philosophy contradicts recipe strategy
- Lines 508-609: More hardcoded instructions

**3. ChatRAGBuilder.ts hardcodes system prompt**
- Lines 171-183: Should come from recipe, not hardcoded

**4. No command pipeline execution**
- multi-persona-chat.json defines sophisticated pipeline
- But nowhere in PersonaUser.ts is this pipeline executed
- PersonaUser does everything inline instead of calling commands

---

## THE REFACTORING PLAN

### Phase 1: Make Recipe Strategy Visible (No Behavior Change)

**Goal**: PersonaUser uses recipe strategy in prompts WITHOUT changing decision logic

**Files to Modify**:
1. `PersonaUser.ts` lines 846-863, 1705-1721
   - Load recipe via RecipeLoader
   - Pass recipe strategy to prompt builders

2. `AIDecisionService.ts` lines 360-442
   - Accept recipe strategy as parameter
   - Build prompt FROM recipe strategy instead of hardcoded text

3. `ChatRAGBuilder.ts` lines 171-183
   - Accept recipe strategy as parameter
   - Build system prompt FROM recipe strategy

**Testing**:
```bash
# 1. Deploy changes
npm start

# 2. Send test message
./jtag debug/chat-send --roomId="<ID>" --message="Test: Can ALL AIs with relevant knowledge respond?"

# 3. Check logs for recipe strategy in prompts
./jtag debug/logs --filterPattern="Recipe strategy|PHILOSOPHY" --tailLines=50

# 4. Verify AIs now see CORRECT instructions (from recipe)
./jtag ai/thoughtstream --limit=5
```

**Expected Outcome**: AIs see recipe strategy in prompts, decisions may change

---

### Phase 2: Add Inspection Commands (Visibility)

**Goal**: 100% visibility into what AI saw when it made a decision

**New Commands to Create**:

**1. `ai/inspect-decision` - See exactly what AI saw**
```bash
./jtag ai/inspect-decision --personaId=<ID> --messageId=<ID>

Output:
{
  "decision": {
    "shouldRespond": false,
    "confidence": 85,
    "reason": "Message not addressed to me",
    "model": "llama3.2:3b"
  },
  "ragContext": {
    "conversationHistory": [...],  // What AI saw
    "recipeStrategy": {...},        // What rules AI used
    "metadata": {...}
  },
  "prompt": {
    "systemPrompt": "...",           // Exact system prompt
    "messages": [...],               // Exact message history
    "totalTokens": 1234
  }
}
```

**2. `ai/inspect-message` - Inspect specific message context**
```bash
./jtag ai/inspect-message --messageId=<ID> --personaId=<ID>

Shows what PersonaId would see if evaluating MessageId NOW
```

**3. `recipe/inspect` - Show recipe details**
```bash
./jtag recipe/inspect --recipeId="general-chat"

Shows full recipe definition, pipeline, strategy
```

**4. Add flags to existing commands**
```bash
./jtag ai/should-respond --inspect  # Show prompt without calling AI
./jtag ai/generate --dryRun         # Show what would be generated
./jtag rag/build --showPrompt       # Show system prompt that would be built
```

**Testing**:
```bash
# Send message that triggers different AI decisions
./jtag debug/chat-send --roomId="<ID>" --message="@Helper Can you help with TypeScript?"

# Inspect why different AIs made different decisions
./jtag ai/inspect-decision --personaId=<HELPER_ID> --messageId=<MSG_ID>
./jtag ai/inspect-decision --personaId=<TEACHER_ID> --messageId=<MSG_ID>
./jtag ai/inspect-decision --personaId=<CODER_ID> --messageId=<MSG_ID>

# Compare their RAG contexts and prompts
```

---

### Phase 3: Command Pipeline Execution (Architecture)

**Goal**: PersonaUser executes recipe pipeline instead of inline logic

**Current Flow** (PersonaUser.ts lines 646-1100):
```
1. handleChatMessage(message)
2. evaluateShouldRespond(message)  ← Inline gating
3. respondToMessage(message)       ← Inline generation
4. Post message to chat
```

**Target Flow** (from multi-persona-chat.json):
```
1. handleChatMessage(message)
2. Execute recipe pipeline:
   - rag/build → ragContext
   - conversation/analyze-loop-risk → loopRisk
   - ai/should-respond → decision
   - ai/generate → response
   - chat/send → posted
3. Done (pipeline handles everything)
```

**Implementation**:

**New File**: `system/recipes/server/RecipeExecutor.ts`
```typescript
export class RecipeExecutor {
  /**
   * Execute recipe pipeline for persona decision
   * Returns final result (response posted or silent)
   */
  async executePipeline(
    recipe: RecipeDefinition,
    context: {
      personaId: UUID;
      roomId: UUID;
      triggerMessage: ChatMessageEntity;
    }
  ): Promise<RecipeExecutionResult> {
    const variables = new Map<string, unknown>();

    for (const step of recipe.pipeline) {
      // Check condition (e.g., "decision.shouldRespond === true")
      if (step.condition && !this.evaluateCondition(step.condition, variables)) {
        console.log(`⏭️ Skipping step ${step.command} (condition false)`);
        continue;
      }

      // Resolve params (e.g., "$ragContext" → actual ragContext value)
      const resolvedParams = this.resolveParams(step.params, variables);

      // Execute command via JTAG
      const result = await this.executeCommand(step.command, resolvedParams);

      // Store result in variables (e.g., outputTo: "ragContext")
      if (step.outputTo) {
        variables.set(step.outputTo, result);
      }
    }

    return {
      success: true,
      finalOutput: variables.get('response')
    };
  }
}
```

**Modify PersonaUser.ts**:
```typescript
// OLD (lines 646-1100): Inline logic
private async handleChatMessage(message: ChatMessageEntity): Promise<void> {
  // ... 450 lines of inline gating/generation logic
}

// NEW: Execute recipe pipeline
private async handleChatMessage(message: ChatMessageEntity): Promise<void> {
  // Load room's recipe
  const room = await this.loadRoom(message.roomId);
  const recipe = await RecipeLoader.getInstance().loadRecipe(room.recipeId);

  // Execute recipe pipeline (handles everything)
  const executor = new RecipeExecutor();
  await executor.executePipeline(recipe, {
    personaId: this.id,
    roomId: message.roomId,
    triggerMessage: message
  });

  // Done! Pipeline handled gating, generation, posting
}
```

**Testing**:
```bash
# 1. Test with general-chat recipe (simple)
./jtag debug/chat-send --roomId="<GENERAL_ROOM>" --message="Test simple recipe"
./jtag debug/logs --filterPattern="Executing pipeline|Skipping step" --tailLines=30

# 2. Test with multi-persona-chat recipe (sophisticated)
# First switch room recipe:
./jtag data/update --collection=rooms --id="<ID>" --data='{"recipeId":"multi-persona-chat"}'

./jtag debug/chat-send --roomId="<ID>" --message="Test complex recipe with loop risk analysis"
./jtag debug/logs --filterPattern="Executing pipeline|loop-risk" --tailLines=50

# 3. Verify behavior change (multi-persona-chat has different rules)
./jtag ai/thoughtstream --limit=5
```

---

### Phase 4: Dynamic Prompt Generation (Eliminate Hardcoded Text)

**Goal**: All prompts generated dynamically from recipe entities

**Current State**: Static text in code
```typescript
// AIDecisionService.ts line 407
const hardcodedPrompt = `PHILOSOPHY: Only gate if it makes the conversation confusing...`;
```

**Target State**: Dynamic generation from recipe
```typescript
// Generate prompt from recipe strategy entity
const prompt = RecipePromptBuilder.buildGatingPrompt(recipe.strategy, {
  personaName: 'Helper AI',
  roomContext: ragContext,
  conversationPattern: recipe.strategy.conversationPattern
});
```

**New File**: `system/recipes/shared/RecipePromptBuilder.ts`
```typescript
export class RecipePromptBuilder {
  /**
   * Build gating prompt from recipe strategy (NO HARDCODED TEXT)
   */
  static buildGatingPrompt(
    strategy: RecipeStrategy,
    context: {
      personaName: string;
      roomContext: RAGContext;
      conversationPattern: string;
    }
  ): string {
    // Start with conversation pattern
    let prompt = `You are "${context.personaName}" in a ${context.conversationPattern} conversation.\n\n`;

    // Add response rules from recipe
    prompt += `**Response Rules:**\n`;
    for (const rule of strategy.responseRules) {
      prompt += `- ${rule}\n`;
    }
    prompt += `\n`;

    // Add decision criteria from recipe
    prompt += `**Decision Criteria:**\n`;
    for (const criterion of strategy.decisionCriteria) {
      prompt += `- ${criterion}\n`;
    }
    prompt += `\n`;

    // Add recent conversation context
    prompt += `**Recent Conversation:**\n`;
    for (const msg of context.roomContext.conversationHistory) {
      prompt += `- ${msg.name}: ${msg.content}\n`;
    }

    return prompt;
  }

  /**
   * Build generation prompt from recipe strategy
   */
  static buildGenerationPrompt(
    strategy: RecipeStrategy,
    context: {
      personaName: string;
      roomContext: RAGContext;
    }
  ): string {
    // Similar dynamic generation
  }
}
```

**Modify AIDecisionService.ts**:
```typescript
// OLD (lines 360-442): Hardcoded prompt
private static buildGatingPrompt(context: AIDecisionContext): string {
  return `PHILOSOPHY: Only gate if...`; // 80+ lines of hardcoded text
}

// NEW: Dynamic generation from recipe
private static buildGatingPrompt(context: AIDecisionContext): string {
  return RecipePromptBuilder.buildGatingPrompt(
    context.ragContext.recipeStrategy,
    {
      personaName: context.personaName,
      roomContext: context.ragContext,
      conversationPattern: context.ragContext.recipeStrategy.conversationPattern
    }
  );
}
```

**Testing**:
```bash
# 1. Compare old vs new prompts
./jtag ai/should-respond --inspect --showPrompt --messageId=<ID>

# 2. Verify recipe rules appear in generated prompt
./jtag debug/logs --filterPattern="Response Rules:|Decision Criteria:" --tailLines=50

# 3. Modify recipe and verify prompt changes
# Edit system/recipes/general-chat.json to add new rule
./jtag recipe/reload --recipeId="general-chat"
./jtag ai/should-respond --inspect --showPrompt --messageId=<ID>
# Should see new rule in generated prompt
```

---

## TESTING STRATEGY

### Test 1: Recipe Strategy Visibility (Phase 1)
**Goal**: Verify AIs see recipe strategy in prompts

```bash
# Before refactoring
./jtag debug/chat-send --roomId="<ID>" --message="Can ALL AIs respond?"
./jtag ai/thoughtstream --limit=5
# Expect: Most AIs pass (hardcoded prompt says "only if confusing")

# After Phase 1
npm start  # Deploy refactored code
./jtag debug/chat-send --roomId="<ID>" --message="Can ALL AIs respond?"
./jtag ai/thoughtstream --limit=5
# Expect: More AIs respond (recipe says "multiple responses GOOD")
```

### Test 2: Inspection Commands (Phase 2)
**Goal**: Verify 100% visibility into AI decisions

```bash
# Send message with specific addressing
./jtag debug/chat-send --roomId="<ID>" --message="@Helper @Teacher Can you both help?"

# Inspect why Helper responded but Teacher didn't
./jtag ai/inspect-decision --personaId=<HELPER_ID> --messageId=<MSG_ID>
./jtag ai/inspect-decision --personaId=<TEACHER_ID> --messageId=<MSG_ID>

# Should show:
# - Exact RAG context each saw
# - Exact prompts each received
# - Exact reasoning for pass/respond
```

### Test 3: Command Pipeline (Phase 3)
**Goal**: Verify recipe pipeline controls behavior

```bash
# Test 1: Simple recipe (general-chat)
./jtag debug/chat-send --roomId="<ID>" --message="Test simple recipe"
./jtag debug/logs --filterPattern="Executing pipeline step" --tailLines=30
# Should show: rag/build → ai/should-respond → ai/generate → chat/send

# Test 2: Complex recipe (multi-persona-chat)
./jtag data/update --collection=rooms --id="<ID>" --data='{"recipeId":"multi-persona-chat"}'
./jtag debug/chat-send --roomId="<ID>" --message="Test complex recipe"
./jtag debug/logs --filterPattern="loop-risk|fast-gating" --tailLines=50
# Should show: Additional pipeline steps (loop-risk analysis, fast-gating)

# Test 3: Modify recipe and verify behavior change
# Edit multi-persona-chat.json to add new pipeline step
./jtag recipe/reload --recipeId="multi-persona-chat"
./jtag debug/chat-send --roomId="<ID>" --message="Test modified recipe"
# Should see new pipeline step in logs
```

### Test 4: Dynamic Prompts (Phase 4)
**Goal**: Verify prompts generated from recipe, not hardcoded

```bash
# Test 1: Modify recipe strategy
# Edit system/recipes/general-chat.json:
# Add new responseRule: "If Joel asks, respond immediately"

./jtag recipe/reload --recipeId="general-chat"
./jtag ai/should-respond --inspect --showPrompt --messageId=<MSG_ID>
# Should see new rule in generated prompt

# Test 2: Create new recipe with different strategy
# Copy general-chat.json → strict-chat.json
# Change conversationPattern to "one-ai-per-question"

./jtag data/update --collection=rooms --id="<ID>" --data='{"recipeId":"strict-chat"}'
./jtag debug/chat-send --roomId="<ID>" --message="Test strict recipe"
# Should see ONLY ONE AI respond (enforced by recipe)
```

---

## EXPECTED OUTCOMES

### Phase 1: Recipe Strategy Visible
- **Behavior**: More AIs respond to "everyone respond" messages
- **Reason**: Recipe strategy says "multiple responses GOOD"
- **Verification**: `./jtag ai/thoughtstream` shows more "shouldRespond: true"

### Phase 2: Inspection Commands
- **Behavior**: Can debug ANY AI decision with full context
- **Reason**: Commands expose RAG, prompts, reasoning
- **Verification**: `./jtag ai/inspect-decision` shows exactly what AI saw

### Phase 3: Command Pipeline
- **Behavior**: Different recipes produce different behavior
- **Reason**: Pipeline controls AI evaluation flow
- **Verification**: multi-persona-chat shows loop-risk analysis in logs

### Phase 4: Dynamic Prompts
- **Behavior**: Modifying recipe JSON changes AI behavior WITHOUT CODE CHANGES
- **Reason**: Prompts generated from recipe strategy
- **Verification**: Edit recipe, reload, test - new rules appear in prompts

---

## RISKS & MITIGATIONS

### Risk 1: Breaking Chat (High Impact)
**Mitigation**: Test after EVERY commit
```bash
npm start
./jtag debug/chat-send --roomId="<ID>" --message="Test: verify chat works"
./jtag screenshot --querySelector="chat-widget"
```

### Risk 2: Performance Regression
**Mitigation**: Measure response times before/after
```bash
# Before
./jtag ai/report  # Record baseline latencies

# After each phase
./jtag ai/report  # Compare to baseline
```

### Risk 3: Recipe Syntax Errors
**Mitigation**: Add recipe validation
```typescript
// RecipeLoader.ts should validate:
- pipeline steps reference valid commands
- conditions use valid syntax
- strategy has required fields
```

### Risk 4: Command Pipeline Failures
**Mitigation**: Graceful degradation
```typescript
// If pipeline step fails, log error but continue
// Or fall back to inline logic for that step
```

---

## NEXT STEPS

1. **User Approval**: Get user confirmation on refactoring direction
   - Phase 1: Make recipe strategy visible (ASAP - high impact)
   - Phase 2: Add inspection commands (medium priority)
   - Phase 3: Command pipeline execution (long-term architecture)
   - Phase 4: Dynamic prompts (long-term vision)

2. **Prioritization**: User may want to start with specific phase
   - If focused on "why AIs pass": Start with Phase 1 + 2
   - If focused on architecture: Start with Phase 3
   - If focused on flexibility: Start with Phase 4

3. **Testing Requirements**: Establish testing protocol
   - Define "success" for each phase
   - Identify regression tests
   - Set performance benchmarks

---

## APPENDIX: File Locations

### Files to Modify (Phase 1)
- `system/user/server/PersonaUser.ts` lines 846-863, 1705-1721
- `system/ai/server/AIDecisionService.ts` lines 360-442, 508-609
- `system/rag/builders/ChatRAGBuilder.ts` lines 171-183

### Files to Create (Phase 2)
- `commands/ai/inspect-decision/` (new command)
- `commands/ai/inspect-message/` (new command)
- `commands/recipe/inspect/` (new command)

### Files to Create (Phase 3)
- `system/recipes/server/RecipeExecutor.ts` (new)
- `system/recipes/shared/RecipeExecutionTypes.ts` (new)

### Files to Create (Phase 4)
- `system/recipes/shared/RecipePromptBuilder.ts` (new)

### Existing Files (Reference)
- `system/recipes/general-chat.json` (simple recipe)
- `system/recipes/multi-persona-chat.json` (sophisticated recipe)
- `system/recipes/server/RecipeLoader.ts` (recipe loading)
- `system/conversation/shared/BaseModerator.ts` (philosophy)
