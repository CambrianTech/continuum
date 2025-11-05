# PersonaUser Recipe Logic Refactoring Plan

**Issue Identified**: 2025-10-14 23:30 UTC
**Priority**: Medium (deferred per Joel's directive)
**Status**: Documented for future implementation

---

## The Problem

**Location**: `system/user/server/PersonaUser.ts` lines 520-636

**Architectural Violation**: Recipe logic (prompt engineering, message formatting, context building) is embedded directly in PersonaUser class instead of being abstracted into a separate Recipe/Strategy pattern.

**Why This Matters**:
- Violates Single Responsibility Principle
- Makes PersonaUser harder to test and maintain
- Prevents easy experimentation with different prompt strategies
- Couples business logic (PersonaUser) with presentation logic (prompt engineering)
- Joel's directive: "Any other god objects or one off designs totally throws off the elegance and maintainability of the project"

---

## Current Problematic Code

### PersonaUser.ts:520-636 (shouldEvaluateMessage method)

```typescript
// Build RAG context for gating decision
const ragBuilder = new ChatRAGBuilder();
const ragContext = await ragBuilder.buildContext(
  message.roomId,
  this.id,
  {
    maxMessages: 10,
    maxMemories: 0,
    includeArtifacts: false,
    includeMemories: false,
    currentMessage: {
      role: 'user',
      content: message.content.text,
      name: message.senderName,
      timestamp: this.timestampToNumber(message.timestamp)
    }
  }
);

// 🚨 PROBLEM STARTS HERE - Hardcoded message building
const messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }> = [];

// System prompt from RAG builder
messages.push({
  role: 'system',
  content: fullRAGContext.identity.systemPrompt
});

// 🚨 Timestamp formatting logic embedded in PersonaUser
for (let i = 0; i < fullRAGContext.conversationHistory.length; i++) {
  const msg = fullRAGContext.conversationHistory[i];
  let timePrefix = '';
  if (msg.timestamp) {
    const date = new Date(msg.timestamp);
    const hours = date.getHours().toString().padStart(2, '0');
    const minutes = date.getMinutes().toString().padStart(2, '0');
    timePrefix = `[${hours}:${minutes}] `;
  }

  const formattedContent = msg.name
    ? `${timePrefix}${msg.name}: ${msg.content}`
    : `${timePrefix}${msg.content}`;

  messages.push({
    role: msg.role,
    content: formattedContent
  });
}

// 🚨 MASSIVE hardcoded identity reminder with prompt engineering
messages.push({
  role: 'system',
  content: `IDENTITY REMINDER: You are ${this.displayName}. You have a specific personality and communication style.

${this.profile?.description || 'Professional and helpful AI assistant.'}

${this.profile?.specialization ? `SPECIALIZATION: ${this.profile.specialization}` : ''}

CRITICAL TOPIC DETECTION PROTOCOL:

Step 1: Check for EXPLICIT TOPIC MARKERS in the most recent message
- "New topic:", "Different question:", "Changing subjects:", "Unrelated, but..."
- "Switching gears:", "Different context:", "Not related to the above, but..."
- If you see ANY of these phrases: STOP. Ignore ALL previous context. This is a NEW conversation.

Step 2: Extract HARD CONSTRAINTS from the most recent message
- Look for negative directives: "NOT", "DON'T", "WITHOUT", "NEVER", "AVOID", "NO"
- Example: "NOT triggering the app to foreground" = YOUR SOLUTION MUST NOT DO THIS
- Example: "WITHOUT using Python" = YOUR SOLUTION MUST NOT USE PYTHON
- These are ABSOLUTE REQUIREMENTS. Your answer MUST respect these constraints or you're wrong.

Step 3: Compare SUBJECT of most recent message to previous 2-3 messages
- If user was discussing "Worker Threads" but now asks about "ZSM authentication", that's a topic change
- If user was discussing eCommerce but now asks about authentication, that's a topic change
- Different technical domains = different topics

Step 4: Determine response strategy
- If EXPLICIT MARKER detected → Treat as brand new conversation, ignore all history
- If TOPIC CHANGED without marker → Acknowledge the shift, focus on NEW topic
- If SAME TOPIC → You can reference previous context
- If HARD CONSTRAINTS detected → Your solution MUST respect them or don't respond

Remember: Users expect you to adapt to topic changes naturally. Don't force continuity where it doesn't exist.`
});
```

**Problems with this code**:
1. **80+ lines of prompt engineering** embedded in PersonaUser
2. **Timestamp formatting logic** mixed with business logic
3. **Hardcoded topic detection protocol** can't be easily A/B tested
4. **No separation of concerns** - PersonaUser knows too much about prompts
5. **Makes testing difficult** - can't unit test prompt logic separately

---

## Proposed Architecture: Recipe Pattern

### Design Philosophy

Follow the existing adapter pattern Joel has established:
- Clean interface hiding implementation details
- Environment-agnostic abstractions in `/shared`
- Concrete implementations in `/server` or `/browser`
- Adapters can share code but hide complexity

### File Structure

```
system/conversation/recipe/
├── shared/
│   ├── BaseRecipe.ts              # Abstract base class
│   ├── RecipeTypes.ts             # Interface definitions
│   └── RecipeRegistry.ts          # Recipe selection logic
├── server/
│   ├── DefaultRecipe.ts           # Standard prompt recipe
│   ├── TopicShiftRecipe.ts        # Enhanced topic detection
│   ├── ConstraintAwareRecipe.ts   # Constraint extraction focus
│   └── ExperimentalRecipe.ts      # A/B testing new approaches
└── README.md                      # Recipe system documentation
```

### Core Interfaces

```typescript
// shared/RecipeTypes.ts

export interface RecipeContext {
  personaName: string;
  personaDescription?: string;
  personaSpecialization?: string;
  ragContext: RAGContext;
  currentMessage: ChatMessage;
  roomId: string;
}

export interface RecipeResult {
  messages: Array<{
    role: 'system' | 'user' | 'assistant';
    content: string;
  }>;
  metadata?: {
    recipeUsed: string;
    topicShiftDetected?: boolean;
    constraintsExtracted?: string[];
  };
}

export interface Recipe {
  readonly recipeId: string;
  readonly recipeName: string;
  readonly version: string;

  /**
   * Build message array for LLM consumption
   */
  buildMessages(context: RecipeContext): Promise<RecipeResult>;

  /**
   * Format single message with timestamp
   */
  formatMessage(message: ChatMessage): string;

  /**
   * Build identity reminder system message
   */
  buildIdentityReminder(context: RecipeContext): string;
}
```

### Base Implementation

```typescript
// shared/BaseRecipe.ts

export abstract class BaseRecipe implements Recipe {
  abstract readonly recipeId: string;
  abstract readonly recipeName: string;
  abstract readonly version: string;

  /**
   * Default message building - subclasses can override
   */
  async buildMessages(context: RecipeContext): Promise<RecipeResult> {
    const messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }> = [];

    // System prompt from RAG
    messages.push({
      role: 'system',
      content: context.ragContext.identity.systemPrompt
    });

    // Conversation history with formatting
    for (const msg of context.ragContext.conversationHistory) {
      messages.push({
        role: msg.role,
        content: this.formatMessage(msg)
      });
    }

    // Identity reminder (subclass-specific)
    messages.push({
      role: 'system',
      content: this.buildIdentityReminder(context)
    });

    return {
      messages,
      metadata: {
        recipeUsed: this.recipeId
      }
    };
  }

  /**
   * Standard timestamp formatting
   */
  formatMessage(message: ChatMessage): string {
    let timePrefix = '';
    if (message.timestamp) {
      const date = new Date(message.timestamp);
      const hours = date.getHours().toString().padStart(2, '0');
      const minutes = date.getMinutes().toString().padStart(2, '0');
      timePrefix = `[${hours}:${minutes}] `;
    }

    return message.name
      ? `${timePrefix}${message.name}: ${message.content}`
      : `${timePrefix}${message.content}`;
  }

  /**
   * Subclasses MUST implement this
   */
  abstract buildIdentityReminder(context: RecipeContext): string;
}
```

### Concrete Recipe Example

```typescript
// server/TopicShiftRecipe.ts

export class TopicShiftRecipe extends BaseRecipe {
  readonly recipeId = 'topic-shift-v1';
  readonly recipeName = 'Topic Shift Detection Recipe';
  readonly version = '1.0.0';

  buildIdentityReminder(context: RecipeContext): string {
    return `IDENTITY REMINDER: You are ${context.personaName}. You have a specific personality and communication style.

${context.personaDescription || 'Professional and helpful AI assistant.'}

${context.personaSpecialization ? `SPECIALIZATION: ${context.personaSpecialization}` : ''}

CRITICAL TOPIC DETECTION PROTOCOL:

Step 1: Check for EXPLICIT TOPIC MARKERS in the most recent message
- "New topic:", "Different question:", "Changing subjects:", "Unrelated, but..."
- "Switching gears:", "Different context:", "Not related to the above, but..."
- If you see ANY of these phrases: STOP. Ignore ALL previous context. This is a NEW conversation.

Step 2: Extract HARD CONSTRAINTS from the most recent message
- Look for negative directives: "NOT", "DON'T", "WITHOUT", "NEVER", "AVOID", "NO"
- Example: "NOT triggering the app to foreground" = YOUR SOLUTION MUST NOT DO THIS
- Example: "WITHOUT using Python" = YOUR SOLUTION MUST NOT USE PYTHON
- These are ABSOLUTE REQUIREMENTS. Your answer MUST respect these constraints or you're wrong.

Step 3: Compare SUBJECT of most recent message to previous 2-3 messages
- If user was discussing "Worker Threads" but now asks about "ZSM authentication", that's a topic change
- If user was discussing eCommerce but now asks about authentication, that's a topic change
- Different technical domains = different topics

Step 4: Determine response strategy
- If EXPLICIT MARKER detected → Treat as brand new conversation, ignore all history
- If TOPIC CHANGED without marker → Acknowledge the shift, focus on NEW topic
- If SAME TOPIC → You can reference previous context
- If HARD CONSTRAINTS detected → Your solution MUST respect them or don't respond

Remember: Users expect you to adapt to topic changes naturally. Don't force continuity where it doesn't exist.`;
  }
}
```

### Recipe Registry (Selection Logic)

```typescript
// shared/RecipeRegistry.ts

export class RecipeRegistry {
  private recipes = new Map<string, Recipe>();
  private defaultRecipeId: string;

  registerRecipe(recipe: Recipe): void {
    this.recipes.set(recipe.recipeId, recipe);
  }

  getRecipe(recipeId: string): Recipe | undefined {
    return this.recipes.get(recipeId);
  }

  /**
   * Select recipe based on persona configuration or context
   */
  selectRecipe(personaId: string, context?: RecipeContext): Recipe {
    // Future: Check persona preferences for recipe
    // Future: A/B testing logic
    // Future: Context-based selection (e.g., topic shift detected = use TopicShiftRecipe)

    // For now, use default
    return this.recipes.get(this.defaultRecipeId) || this.recipes.values().next().value;
  }

  listRecipes(): Recipe[] {
    return Array.from(this.recipes.values());
  }
}
```

---

## Refactored PersonaUser Usage

### Before (Current - 80+ lines in PersonaUser)

```typescript
// PersonaUser.ts:520-636
async shouldEvaluateMessage(message: ChatMessage, context: JTAGContext): Promise<boolean> {
  // ... RAG building ...

  // 🚨 80+ LINES OF PROMPT ENGINEERING HERE
  const messages = [];
  messages.push({ role: 'system', content: fullRAGContext.identity.systemPrompt });
  // ... timestamp formatting ...
  // ... identity reminder ...
  // ... topic detection protocol ...

  const response = await aiProvider.generateText({ messages, model: this.modelId });
  return this.parseGatingDecision(response.text);
}
```

### After (Clean Separation)

```typescript
// PersonaUser.ts (refactored)
async shouldEvaluateMessage(message: ChatMessage, context: JTAGContext): Promise<boolean> {
  // Build RAG context (same as before)
  const ragBuilder = new ChatRAGBuilder();
  const ragContext = await ragBuilder.buildContext(/* ... */);

  // ✅ DELEGATE to recipe
  const recipe = RecipeRegistry.sharedInstance().selectRecipe(this.id, {
    personaName: this.displayName,
    personaDescription: this.profile?.description,
    personaSpecialization: this.profile?.specialization,
    ragContext,
    currentMessage: message,
    roomId: message.roomId
  });

  const recipeResult = await recipe.buildMessages({
    personaName: this.displayName,
    personaDescription: this.profile?.description,
    personaSpecialization: this.profile?.specialization,
    ragContext,
    currentMessage: message,
    roomId: message.roomId
  });

  // Generate response (same as before)
  const response = await aiProvider.generateText({
    messages: recipeResult.messages,
    model: this.modelId,
    context
  });

  return this.parseGatingDecision(response.text);
}
```

**Benefits**:
- PersonaUser is now **15 lines** instead of 80+
- Recipe logic can be **unit tested** independently
- Easy to **A/B test** different prompt strategies
- Can **swap recipes** without touching PersonaUser
- Follows **existing adapter pattern** Joel established

---

## Migration Strategy

### Phase 1: Extract Current Logic (No Behavior Change)
1. Create `BaseRecipe` with current PersonaUser logic
2. Create `DefaultRecipe` that replicates current behavior exactly
3. Update PersonaUser to use DefaultRecipe
4. **Verify**: Run all existing tests, should pass with zero changes

### Phase 2: Create Alternative Recipes
1. `TopicShiftRecipe` - Enhanced topic detection (current implementation)
2. `ConstraintAwareRecipe` - Focus on constraint extraction
3. `MinimalContextRecipe` - Reduces context to 5 messages
4. `ExperimentalRecipe` - For testing new ideas

### Phase 3: Recipe Selection Logic
1. Add `preferredRecipe` to PersonaUser profile
2. Add A/B testing framework (% of users get ExperimentalRecipe)
3. Add context-based selection (detect topic shift → use TopicShiftRecipe)
4. Add performance metrics (which recipe gets best responses?)

### Phase 4: Recipe Marketplace (Future)
1. Allow users to create custom recipes
2. Share recipes across P2P mesh
3. Rate recipes based on effectiveness
4. AI citizens can evolve their own recipes

---

## Testing Strategy

### Unit Tests (New)

```typescript
// tests/unit/recipe/TopicShiftRecipe.test.ts

describe('TopicShiftRecipe', () => {
  it('should detect explicit topic markers', async () => {
    const recipe = new TopicShiftRecipe();
    const context = createMockContext({
      currentMessage: { content: 'New topic: tell me about ZSM' }
    });

    const result = await recipe.buildMessages(context);

    expect(result.metadata?.topicShiftDetected).toBe(true);
  });

  it('should extract hard constraints', async () => {
    const recipe = new TopicShiftRecipe();
    const context = createMockContext({
      currentMessage: { content: 'WITHOUT triggering the app to foreground' }
    });

    const result = await recipe.buildMessages(context);

    expect(result.metadata?.constraintsExtracted).toContain('WITHOUT triggering');
  });

  it('should format timestamps correctly', () => {
    const recipe = new TopicShiftRecipe();
    const message = {
      content: 'test',
      name: 'Joel',
      timestamp: new Date('2025-10-14T15:30:00Z')
    };

    const formatted = recipe.formatMessage(message);

    expect(formatted).toMatch(/\[\d{2}:\d{2}\] Joel: test/);
  });
});
```

### Integration Tests (Existing + New)

```bash
# Existing test should still pass
npm test -- worker-mock-evaluation.test.ts

# New test: Verify recipe swapping doesn't break responses
npm test -- recipe-selection.test.ts
```

---

## Documentation Requirements

### For AI Citizens
Create `system/conversation/recipe/README.md` explaining:
- What recipes are (prompt engineering strategies)
- How to create custom recipes
- How to test recipes before deploying
- Best practices for prompt engineering

### For Developers
Update `CLAUDE.md`:
- Recipe pattern architecture
- When to create new recipe vs modify existing
- How recipe selection works
- Testing recipe changes

---

## Success Metrics

### Before Refactoring
- PersonaUser.ts: 636 lines (80+ lines of prompt engineering)
- Prompt logic: Hardcoded, can't be A/B tested
- Testing: Integration tests only, can't unit test prompts
- Experimentation: Requires editing PersonaUser directly

### After Refactoring
- PersonaUser.ts: ~550 lines (15 lines of recipe delegation)
- Prompt logic: Separate Recipe classes, easily testable
- Testing: Unit tests for recipes + integration tests
- Experimentation: Create new recipe, register, test in isolation

### Quality Metrics
- ✅ Single Responsibility Principle restored
- ✅ Open/Closed Principle: Extend via new recipes, don't modify PersonaUser
- ✅ Testability: Recipe logic can be unit tested
- ✅ Maintainability: Prompt changes don't touch PersonaUser
- ✅ Follows existing adapter pattern established by Joel

---

## Timeline Estimate

- **Phase 1 (Extract Current Logic)**: 2-3 hours
  - Write BaseRecipe + DefaultRecipe
  - Update PersonaUser to use DefaultRecipe
  - Verify all tests pass

- **Phase 2 (Create Alternative Recipes)**: 1-2 hours
  - TopicShiftRecipe, ConstraintAwareRecipe, MinimalContextRecipe
  - Write unit tests for each recipe

- **Phase 3 (Recipe Selection Logic)**: 2-3 hours
  - RecipeRegistry with selection logic
  - A/B testing framework
  - Performance metrics

- **Phase 4 (Recipe Marketplace)**: Future enhancement

**Total Estimate**: 5-8 hours for Phases 1-3

---

## References

- **Similar AIDecisionService Issue**: `system/ai/server/AIDecisionService.ts:528-571` has same problem (prompt engineering embedded)
- **Existing Adapter Pattern**: `daemons/ai-provider-daemon/shared/BaseAIProviderAdapter.ts` - follow this architecture
- **Topic Detection Documentation**: `system/ai/TOPIC-DETECTION-ISSUE.md` - current prompt engineering approach

---

## Joel's Directive

> "you put persona recipe logic INSIDE personauser.ts, so add that to your list to fix. We will leave it for now."

**Status**: Documented for future implementation. Not urgent, but improves maintainability and follows project architecture principles.
