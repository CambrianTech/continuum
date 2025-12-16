# Multi-Persona Collaborative Chat Recipe

**Recipe ID**: `multi-persona-chat`
**Status**: Phase 2 Complete - Ready for Phase 3 (Recipe Engine Integration)
**Created**: 2025-10-10

---

## Overview

This recipe defines **organic multi-persona AI conversations** with intelligent resource management. It leverages the fast bag-of-words gating system and PersonaResponseConfig to create natural, domain-driven collaboration without artificial limitations.

**Core Philosophy**: Use appropriate AI for each task, escalate only when necessary, let personas respond organically based on domain expertise.

---

## Multi-Stage AI Escalation Pipeline

### Stage 1: Fast Deterministic Gating (<1ms)
```json
{
  "command": "ai/should-respond-fast",
  "params": {
    "ragContext": "$ragContext",
    "personaConfig": "$personaConfig"
  },
  "outputTo": "fastGating"
}
```

**Purpose**: Instant bag-of-words scoring eliminates irrelevant responses
**Speed**: <1ms (500x faster than LLM)
**Decision**:
- Score >= 80 → Proceed directly to Stage 3 (high confidence)
- Score 40-79 → Escalate to Stage 2 (borderline case)
- Score < 40 → Silent (not relevant)

**Example**:
- Message: "How do I fix this TypeScript error?"
- CodeReview AI: Score 90 (keywords: "fix", "TypeScript", "error") → Stage 3
- Helper AI: Score 45 (keyword: "How") → Stage 2
- Teacher AI: Score 25 (no domain match) → Silent

---

### Stage 2: Small Model Decision (~500ms)
```json
{
  "command": "ai/should-respond",
  "params": {
    "ragContext": "$ragContext",
    "strategy": "collaborative",
    "model": "$personaConfig.gatingModel"
  },
  "outputTo": "decision",
  "condition": "fastGating.score >= 40 && fastGating.score < 80"
}
```

**Purpose**: Small LLM (llama3.2:1b) evaluates borderline cases
**Speed**: ~500ms
**Decision**: Boolean shouldRespond for context-aware gating

**Example**:
- Message: "How do I understand this code better?"
- Helper AI (score 45): Small model evaluates → "More teaching-focused" → Pass to Teacher AI
- Teacher AI receives escalation with full context

---

### Stage 3: Full Model Response (~2-5s)
```json
{
  "command": "ai/generate",
  "params": {
    "ragContext": "$ragContext",
    "temperature": 0.7,
    "model": "$personaConfig.responseModel",
    "systemPrompt": "$personaConfig.systemPrompt"
  },
  "outputTo": "response",
  "condition": "fastGating.score >= 80 || decision.shouldRespond === true"
}
```

**Purpose**: Full LLM (llama3.2:3b) generates quality response
**Speed**: ~2-5s
**Triggers**: High-confidence fast gating OR small model approval

**Example**:
- CodeReview AI (score 90): Bypassed Stage 2, generates response immediately
- Response: "That TypeScript error occurs because..."

---

## Resource Management Strategy

### Intelligent Model Selection
```typescript
// From PersonaResponseConfig (UserEntity)
{
  gatingModel: 'deterministic',      // Stage 1: Fast path (default)
  responseModel: 'llama3.2:3b',      // Stage 3: Full response
  escalationModel: 'llama3.2:7b'     // Stage 4: Complex tasks (future)
}
```

**Why This Matters**:
- Most responses eliminated in <1ms (Stage 1)
- Only borderline cases use small model (~500ms)
- Full model only for confident responses (~2-5s)
- No wasted compute on irrelevant responses

---

## Persona Configuration Integration

### Example: CodeReview AI Configuration
```typescript
// From seed-continuum.ts
{
  domainKeywords: [
    'code', 'programming', 'function', 'bug',
    'typescript', 'javascript', 'review', 'refactor'
  ],
  responseThreshold: 50,              // Min score to respond
  alwaysRespondToMentions: true,      // @CodeReview bypasses gating
  cooldownSeconds: 30,                // Min time between responses
  maxResponsesPerSession: 50,         // Prevent infinite loops
  gatingModel: 'deterministic',
  responseModel: 'llama3.2:3b'
}
```

**How Recipe Uses This**:
1. Fast gating reads `domainKeywords` for scoring
2. `responseThreshold` determines Stage 1 cutoff
3. `cooldownSeconds` prevents spam (checked by PersonaUser)
4. `responseModel` selects appropriate LLM for generation

---

## Conversation Patterns

### Pattern 1: Domain-Driven Response
```
User: "How do I fix this TypeScript error with async/await?"

Fast Gating:
- CodeReview AI: 95 (typescript, error, async, await) → RESPOND (Stage 3)
- Teacher AI: 30 (how, fix) → SILENT
- Helper AI: 25 (how) → SILENT

Result: CodeReview AI responds with technical fix
```

### Pattern 2: Escalation for Borderline Cases
```
User: "Can someone help me understand async programming?"

Fast Gating:
- Teacher AI: 65 (help, understand, programming) → ESCALATE (Stage 2)
- CodeReview AI: 55 (programming, async) → ESCALATE (Stage 2)
- Helper AI: 45 (help, someone) → ESCALATE (Stage 2)

Small Model Decisions:
- Teacher AI: TRUE (teaching request)
- CodeReview AI: FALSE (understanding > code review)
- Helper AI: FALSE (domain-specific help)

Result: Teacher AI responds with educational explanation
```

### Pattern 3: Multi-Persona Collaboration
```
User: "I'm building a game - how should I structure the code and teach my team?"

Fast Gating:
- CodeReview AI: 85 (code, structure, building) → RESPOND (Stage 3)
- Teacher AI: 80 (teach, team, how) → RESPOND (Stage 3)
- Helper AI: 40 (how, should) → ESCALATE (Stage 2)

Result: Both CodeReview AI and Teacher AI respond organically
- CodeReview: "For game structure, I recommend..."
- Teacher: "To help your team learn..."
- Helper: (Small model decides not to add redundant help)
```

### Pattern 4: Mention Override
```
User: "@Helper can you assist with this?"

Fast Gating:
- Helper AI: Score doesn't matter → RESPOND (alwaysRespondToMentions: true)

Result: Helper AI responds regardless of domain match
```

---

## RAG Template Configuration

```json
{
  "messageHistory": {
    "maxMessages": 30,
    "orderBy": "chronological",
    "includeTimestamps": true
  },
  "participants": {
    "includeRoles": true,
    "includeExpertise": true,
    "includeHistory": true
  },
  "custom": {
    "personaDomains": true,           // Include domain keywords
    "conversationTemperature": true,  // Current activity level
    "participationRatios": true       // Who's spoken recently
  }
}
```

**Why These Settings**:
- 30 messages: Enough context for natural conversation without overwhelming LLM
- Timestamps: Help LLMs understand conversation pacing
- Expertise: Personas know each other's domains (avoid redundancy)
- Participation ratios: Prevent single persona domination

---

## Strategy: Collaborative Pattern

```json
{
  "conversationPattern": "collaborative",
  "responseRules": [
    "Use fast gating (Stage 1) to eliminate irrelevant responses instantly",
    "Escalate to small model (Stage 2) for borderline domain matches",
    "Use full model (Stage 3) only when confident response is valuable",
    "Domain expertise drives response priority",
    "Multiple personas can respond organically - no artificial limits",
    "Cooldown periods prevent individual persona spam",
    "Always respond if @mentioned regardless of gating score",
    "Natural conversation flow > rigid turn-taking"
  ]
}
```

**Contrast with `human-focused` Pattern** (general-chat.json):
- Human-focused: "If AI just responded → WAIT for human"
- Collaborative: "Multiple personas can respond organically"

**Why Collaborative for Multi-Persona**:
- Encourages organic AI discussions
- Domain expertise naturally limits responses
- Cooldowns prevent spam without artificial turn-taking
- Humans can observe/guide but AIs can explore ideas together

---

## Current Implementation Status

### ✅ Phase 1: Generic Coordination Primitives (Complete)
- Fast gating command (`ai/should-respond-fast`)
- Bag-of-words scoring system
- Generic coordination patterns
- Architecture documentation

### ✅ Phase 2: Persona Configuration (Complete - Just Committed)
- PersonaResponseConfig interface in UserEntity
- Domain keywords per persona
- Multi-stage escalation settings
- Persona-specific model selection
- Genome/LoRA support placeholders

### ✅ Phase 2.5: Recipe Definition (Complete - This File)
- Multi-persona recipe JSON
- Multi-stage pipeline definition
- Collaborative strategy documentation

### 🔄 Phase 3: Recipe Engine (Next - Per RECIPE-SYSTEM-REQUIREMENTS.md)
**What's Needed**:
1. RecipeEngine - Execute recipe pipelines
2. RecipeTriggerManager - Listen for user-message events
3. RecipeStateManager - Persist conversation state
4. recipe/activate command - Enable recipe for room
5. Integration with PersonaUser

**Current Workaround**: PersonaUser manually implements fast gating inline (lines 395-473)

**Future**: PersonaUser delegates to RecipeEngine, recipe defines behavior

---

## Testing Strategy

### Manual Testing (Current)
```bash
# 1. Deploy system with configured personas
npm start

# 2. Send test messages with different domain keywords
./jtag collaboration/chat/send --roomId=general --content="How do I fix TypeScript errors?"

# 3. Observe logs for gating scores
./jtag debug/logs --filterPattern="Fast gating score" --tailLines=20

# 4. Verify appropriate personas responded
./jtag debug/widget-state --widgetSelector="chat-widget" --includeMessages=true
```

### Integration Testing (Phase 3)
```typescript
// Test multi-stage escalation
test('recipe uses fast gating before small model', async () => {
  const message = 'How do I understand this code?';

  // Trigger recipe
  await recipeEngine.execute('multi-persona-chat', {
    messageText: message,
    roomId: 'test-room'
  });

  // Verify execution path
  expect(recipe.trace[0].command).toBe('ai/should-respond-fast');
  expect(recipe.trace[1].command).toBe('ai/should-respond'); // Stage 2
  expect(recipe.trace[2].command).toBe('ai/generate');       // Stage 3
});

// Test domain-driven routing
test('CodeReview AI responds to code questions', async () => {
  const message = 'Fix TypeScript error in async function';

  await recipeEngine.execute('multi-persona-chat', {
    messageText: message,
    roomId: 'test-room'
  });

  const responses = await getResponses('test-room');
  expect(responses[0].senderId).toBe('code-review-ai');
  expect(responses[0].metadata.gatingStage).toBe('fast'); // Bypassed Stage 2
});

// Test organic multi-persona collaboration
test('multiple personas respond when relevant', async () => {
  const message = 'How do I teach my team about async programming?';

  await recipeEngine.execute('multi-persona-chat', {
    messageText: message,
    roomId: 'test-room'
  });

  const responses = await getResponses('test-room');
  expect(responses.length).toBeGreaterThanOrEqual(2);
  expect(responses.map(r => r.senderId)).toContain('teacher-ai');
  expect(responses.map(r => r.senderId)).toContain('code-review-ai');
});
```

---

## Future Enhancements

### Stage 4: Escalation to Specialized Models
```json
{
  "command": "ai/generate",
  "params": {
    "ragContext": "$ragContext",
    "model": "$personaConfig.escalationModel",
    "temperature": 0.5
  },
  "condition": "response.complexity === 'high' && response.confidence < 0.7",
  "comment": "Escalate complex/uncertain responses to larger model"
}
```

**Use Case**: CodeReview AI uses 3b model, detects complex architectural question, escalates to 7b model

### Genome/LoRA Integration
```json
{
  "command": "genome/apply",
  "params": {
    "genomeId": "$personaConfig.genomeId",
    "baseModel": "$personaConfig.responseModel"
  },
  "outputTo": "adaptedModel",
  "condition": "personaConfig.genomeId !== null"
}
```

**Use Case**: Persona has trained LoRA adapter, apply before generation for specialized responses

### Cost Management Widget (Much Later)
```json
{
  "command": "cost/estimate",
  "params": {
    "model": "$personaConfig.responseModel",
    "contextTokens": "$ragContext.tokenCount"
  },
  "outputTo": "costEstimate"
},
{
  "command": "cost/approve",
  "params": {
    "estimate": "$costEstimate",
    "budget": "$userBudget"
  },
  "outputTo": "approved",
  "comment": "User can approve/reject based on cost"
}
```

**Use Case**: User sets monthly AI budget, system gates expensive calls

---

## Key Design Decisions

### Why Collaborative Pattern?
- **Organic Intelligence**: Natural domain-driven responses
- **No Artificial Limits**: Let expertise determine participation
- **Scalable**: Works with 3 personas or 30
- **User Control**: Can switch recipes per room (collaborative vs human-focused)

### Why Multi-Stage Escalation?
- **Efficiency**: 90% eliminated in <1ms, only 10% use LLMs
- **Quality**: Full model only for confident responses
- **Flexibility**: Easy to add Stage 4+ for complex tasks
- **Future-Proof**: Supports genome/LoRA without architecture changes

### Why Deterministic Fast Gating Default?
- **Speed**: <1ms vs ~500ms small model
- **Predictability**: Scoring is transparent and debuggable
- **Resource-Friendly**: No API calls for rejection
- **Sufficient**: 50-point scoring system captures most relevance

### Why Per-Persona Configuration?
- **Specialization**: CodeReview AI vs Teacher AI have different thresholds
- **Flexibility**: Users can tune individual personas
- **Scalability**: Add new personas without changing recipes
- **Genome-Ready**: Configuration supports future LoRA training

---

## Success Criteria

### ✅ Phase 2 Complete When:
- [x] Fast gating integrated into PersonaUser
- [x] PersonaResponseConfig in database
- [x] 3 personas configured with domain keywords
- [x] Recipe JSON written and documented
- [x] All changes committed

### 🎯 Phase 3 Complete When:
- [ ] RecipeEngine executes multi-persona-chat recipe
- [ ] User message triggers recipe automatically
- [ ] Multiple personas respond organically
- [ ] Fast gating scores visible in logs
- [ ] Recipe can be activated/deactivated per room

### 🚀 Production Ready When:
- [ ] Recipe handles errors gracefully (AI failures)
- [ ] Conversation state persists (multi-turn dialogues)
- [ ] Cooldowns prevent spam
- [ ] Metrics/observability for gating decisions
- [ ] User can switch recipes in UI

---

## Related Documentation

- `RECIPE-SYSTEM-REQUIREMENTS.md` - Full recipe system roadmap
- `design/GENOME-COMMANDS-SPEC.md` - Future genome integration
- `commands/ai/should-respond-fast/README.md` - Fast gating implementation
- `system/data/entities/UserEntity.ts` - PersonaResponseConfig interface
- `system/user/shared/PersonaUser.ts` - Current gating implementation

---

## Conclusion

This recipe represents **Phase 2.5** of our multi-persona AI coordination system:

**What We Built**:
- Fast deterministic gating (<1ms)
- Multi-stage AI escalation (Fast → Small → Full → Specialized)
- Domain-driven response routing
- Organic collaboration without artificial limits
- Intelligent resource management
- Foundation for genome/LoRA training

**What's Next (Phase 3)**:
- Recipe Engine implementation
- Automatic trigger system
- Integration with PersonaUser
- Testing end-to-end flows

**Vision**:
- Natural speaking milestone (organic AI conversations)
- Full genome support (LoRA adaptation layers)
- AI scheduling logic (intelligent task distribution)
- Cost management (much later, separate domain)

**Strategic Priority**: Build organic intelligence first, optimize cost later. Free local models enable unlimited experimentation.
