# AI Coordination Architecture - The RoomCoordinator Vision

**Status:** Phase 1 Complete (Simple Rules) → Phase 2 Design (Event-Driven Coordination)

---

## Executive Summary

**Goal:** Enable natural AI-to-AI collaboration without infinite loops, using intelligent coordination instead of rigid rules.

**Solution:** RoomCoordinator - a specialized AI user that observes chat events and orchestrates persona responses using local Ollama models (free, private, fast).

**Philosophy:** Anti-deterministic - decisions should feel natural and context-aware, not robotic rule-following.

---

## The Problem

**Current (Phase 1 - Working):**
```
Rule 1: @mention → ALWAYS respond
Rule 2: Human message → ALWAYS respond
Rule 3: AI message → NEVER respond (unless @mentioned)

✅ Prevents infinite loops
❌ Feels robotic and unnatural
❌ No intelligence or context awareness
```

**What We Want (Phase 2):**
```
RoomCoordinator observes:
- "Joel asked about TypeScript"
- "Helper AI specializes in that"
- "But Helper just responded 3 times in a row..."
- "Question seems rhetorical"
- "Conversation feels concluded"

Decision: 85% confidence → Helper waits, Teacher responds instead
```

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    Chat Room (#general)                      │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  Joel (human) sends: "How do I fix this TypeScript error?"  │
│                              ↓                                │
│                   chat:message-received event                │
│                              ↓                                │
│  ┌───────────────────────────────────────────────────────┐  │
│  │         RoomCoordinator (Special Persona)             │  │
│  │  ┌─────────────────────────────────────────────────┐ │  │
│  │  │  1. Receives event (subscribed to all chat)     │ │  │
│  │  │  2. Builds RAG context:                         │ │  │
│  │  │     - Last 10 messages                          │ │  │
│  │  │     - Participation stats from own DB           │ │  │
│  │  │     - Past decisions from own DB                │ │  │
│  │  │  3. Calls AI Daemon (Ollama local):             │ │  │
│  │  │     "Who should respond? Why?"                  │ │  │
│  │  │  4. Stores reasoning in own DB                  │ │  │
│  │  │  5. Emits coordination signals                  │ │  │
│  │  └─────────────────────────────────────────────────┘ │  │
│  └───────────────────────────────────────────────────────┘  │
│                              ↓                                │
│        persona:respond-signal + persona:wait-signal          │
│                              ↓                                │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │  Helper AI   │  │  Teacher AI  │  │ CodeReview AI│      │
│  │  (RESPOND✅) │  │  (WAIT 🔇)   │  │  (WAIT 🔇)   │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
│         ↓                                                     │
│  Helper AI generates response using AI Daemon                │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

---

## Hard Rules vs Soft Decisions

### Hard Rules (Deterministic - Safety First)

**Protocol-level constraints that cannot be overridden:**

1. **Same Room Requirement**
   - Must be member of room to see messages
   - Exception: @cross-room-mention (future feature?)

2. **@Mention = Forced Response**
   - Social contract: ignoring @mentions is rude
   - Coordinator can suggest delay, but must respond

3. **Rate Limiting**
   - Max 1 response per 10 seconds per room (spam prevention)
   - Applies to all personas equally

4. **No Self-Response**
   - Cannot respond to own messages (safety)

5. **Session Active**
   - Must have active session to respond

### Soft Decisions (AI-Driven - Fuzzy Logic)

**Context-aware decisions made by RoomCoordinator:**

1. **Should I respond?**
   - Message relevance to persona expertise
   - Question vs statement vs rhetorical
   - Conversation concluded vs ongoing

2. **How long should I wait?**
   - Conversational flow (hot vs cold chat)
   - Give humans time to respond
   - Stagger multiple AI responses

3. **Am I dominating?**
   - Participation ratio (my messages / total)
   - Let other personas contribute
   - Encourage diverse perspectives

4. **Who's the best fit?**
   - Topic alignment with persona expertise
   - Who hasn't spoken recently?
   - Should multiple personas respond together?

5. **What's the conversation temperature?**
   - HOT: Active chat, quick responses
   - WARM: Moderate pace
   - COOL: Slow chat, careful responses
   - COLD: Dead chat, maybe don't pile on

---

## RoomCoordinator Implementation

### Type Definition

```typescript
/**
 * RoomCoordinator - Intelligent AI orchestrator
 *
 * Observes all chat events and makes fuzzy decisions about
 * which personas should respond, when, and why.
 */
class RoomCoordinator extends PersonaUser {
  // Special persona with coordination logic instead of chat generation

  /**
   * Build RAG context for decision-making
   */
  async buildContext(roomId: UUID): Promise<CoordinationContext> {
    const recentMessages = await this.getRecentMessages(roomId, 10);
    const participationStats = await this.getParticipationStats(roomId);
    const pastDecisions = await this.getPastDecisions(roomId, 5);

    return {
      messages: recentMessages,
      stats: participationStats,
      history: pastDecisions,
      temperature: this.calculateTemperature(recentMessages)
    };
  }

  /**
   * Coordinate response to new message
   */
  async handleChatMessage(event: ChatMessageEvent): Promise<void> {
    // Build RAG context
    const context = await this.buildContext(event.roomId);

    // Call AI daemon (Ollama local model)
    const decision = await this.client.daemons.ai.coordinateResponse({
      adapter: 'ollama',
      model: 'llama3.2:1b',  // Fast local model
      context,
      message: event.message,
      availablePersonas: await this.getRoomPersonas(event.roomId)
    });

    // Store reasoning for future training
    await this.storeDecision({
      messageId: event.message.id,
      decision: decision.persona,
      reasoning: decision.reasoning,
      confidence: decision.confidence,
      timestamp: new Date()
    });

    // Emit coordination signals
    for (const action of decision.actions) {
      if (action.type === 'RESPOND') {
        await this.emitSignal('persona:respond-signal', {
          personaId: action.personaId,
          messageId: event.message.id,
          waitSeconds: action.delaySeconds || 0
        });
      } else {
        await this.emitSignal('persona:wait-signal', {
          personaId: action.personaId,
          messageId: event.message.id,
          reason: action.reason
        });
      }
    }
  }

  /**
   * Store decision in own database for training
   */
  async storeDecision(decision: CoordinationDecision): Promise<void> {
    await this.client.daemons.commands.execute('data/create', {
      collection: 'coordination_decisions',  // Stored in coordinator's own DB
      data: decision,
      context: this.client.context,
      sessionId: this.client.sessionId
    });
  }
}
```

### PersonaUser Integration

```typescript
/**
 * PersonaUser receives coordination signals
 */
class PersonaUser extends AIUser {

  async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
    // STEP 1: Hard rules (fast exit)
    if (messageEntity.senderId === this.id) return; // No self-response
    if (!this.isInRoom(messageEntity.roomId)) return; // Not in room
    if (this.isRateLimited(messageEntity.roomId)) return; // Rate limited

    // STEP 2: Check @mention (forced response)
    const messageText = messageEntity.content?.text || '';
    const isMentioned = this.isPersonaMentioned(messageText);

    if (isMentioned) {
      console.log(`📣 ${this.displayName}: Mentioned - FORCED RESPONSE`);
      await this.generateAndSendResponse(messageEntity);
      return;
    }

    // STEP 3: Wait for coordinator signal
    // Coordinator will emit persona:respond-signal if we should respond
    console.log(`⏳ ${this.displayName}: Waiting for coordinator decision...`);
  }

  /**
   * Handle coordination signal from RoomCoordinator
   */
  async handleRespondSignal(signal: RespondSignal): Promise<void> {
    console.log(`✅ ${this.displayName}: Coordinator says RESPOND`);

    // Optional delay for natural conversation flow
    if (signal.waitSeconds > 0) {
      await this.delay(signal.waitSeconds * 1000);
    }

    // Get original message and respond
    const message = await this.getMessage(signal.messageId);
    await this.generateAndSendResponse(message);
  }

  /**
   * Handle wait signal (optional - for logging)
   */
  async handleWaitSignal(signal: WaitSignal): Promise<void> {
    console.log(`🔇 ${this.displayName}: Coordinator says WAIT - ${signal.reason}`);
  }
}
```

---

## AI Daemon Architecture

### Adapter Pattern (Pluggable Models)

```typescript
/**
 * AI Daemon - Unified interface for all LLM calls
 */
class AIDaemon {
  private adapters: Map<string, LLMAdapter>;

  constructor() {
    this.adapters = new Map([
      ['ollama', new OllamaAdapter()],      // Local (default)
      ['openai', new OpenAIAdapter()],      // Cloud (optional)
      ['anthropic', new AnthropicAdapter()] // Cloud (optional)
    ]);
  }

  /**
   * Coordinate response decision (fast, local)
   */
  async coordinateResponse(params: {
    adapter: string;
    model: string;
    context: CoordinationContext;
    message: ChatMessageEntity;
    availablePersonas: PersonaInfo[];
  }): Promise<CoordinationDecision> {

    const adapter = this.adapters.get(params.adapter);

    const prompt = this.buildCoordinationPrompt(
      params.context,
      params.message,
      params.availablePersonas
    );

    const response = await adapter.generate({
      model: params.model,
      prompt,
      temperature: 0.7,  // Some randomness = natural
      maxTokens: 200     // Short decision
    });

    return this.parseCoordinationResponse(response);
  }

  /**
   * Generate chat response (can use better model)
   */
  async generateChatResponse(params: {
    adapter: string;
    model: string;
    persona: PersonaInfo;
    message: ChatMessageEntity;
    context: string;
  }): Promise<string> {

    const adapter = this.adapters.get(params.adapter);

    const prompt = this.buildChatPrompt(
      params.persona,
      params.message,
      params.context
    );

    const response = await adapter.generate({
      model: params.model,
      prompt,
      temperature: 0.8,  // More creative for chat
      maxTokens: 500     // Longer response
    });

    return response;
  }
}
```

### Ollama Adapter (Local, Free)

```typescript
/**
 * Ollama adapter - local LLM inference
 */
class OllamaAdapter implements LLMAdapter {
  private baseUrl = 'http://localhost:11434';

  async generate(params: {
    model: string;
    prompt: string;
    temperature: number;
    maxTokens: number;
  }): Promise<string> {

    const response = await fetch(`${this.baseUrl}/api/generate`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        model: params.model,
        prompt: params.prompt,
        options: {
          temperature: params.temperature,
          num_predict: params.maxTokens
        }
      })
    });

    const data = await response.json();
    return data.response;
  }
}
```

---

## Ollama Integration - Out-of-Box AI

### Default Experience (No API Keys Required)

**First-time setup:**
```bash
$ npm install -g @continuum/jtag
$ continuum start

🤖 Setting up AI personas...

   📥 Checking for Ollama...
   ✅ Ollama detected at localhost:11434

   📥 Downloading coordination model (70MB)...
   ✅ llama3.2:1b ready (coordination decisions)

   📥 Downloading chat model (1.9GB)...
   ✅ phi-3-mini ready (chat responses)

   🎭 Creating AI personas...
   ✅ Helper AI (general assistance)
   ✅ Teacher AI (education/tutorials)
   ✅ CodeReview AI (code analysis)
   ✅ RoomCoordinator (orchestration)

🎉 Your AI team is ready!

💡 Tip: Add API keys for cloud models (Settings → AI Providers)
   Local models are free and private, but cloud models give better responses.
```

**Without Ollama installed:**
```bash
$ continuum start

⚠️  Ollama not found - AI personas will use simple heuristics only

📖 To enable AI coordination:
   1. Install Ollama: brew install ollama
   2. Restart: continuum restart

✅ Starting with basic rule-based coordination...
```

### Model Selection Strategy

```typescript
/**
 * Default model configuration
 */
const DEFAULT_MODELS = {
  coordination: {
    adapter: 'ollama',
    model: 'llama3.2:1b',        // 700MB, ~200ms inference
    purpose: 'Fast decisions',
    cost: 'FREE'
  },

  chat: {
    adapter: 'ollama',
    model: 'phi-3-mini',          // 1.9GB, ~500ms inference
    purpose: 'Quality responses',
    cost: 'FREE'
  },

  // Optional upgrades (user adds API keys)
  chatUpgrade: {
    adapter: 'anthropic',
    model: 'claude-3-5-haiku-20241022',
    purpose: 'Best responses',
    cost: '$0.80 / 1M tokens'
  }
};
```

**Hybrid strategy (best of both worlds):**
```
RoomCoordinator → Local (Ollama llama3.2:1b)
  - Fast decisions (~200ms)
  - Free
  - Always available

PersonaUsers → Cloud (Claude Haiku) if API key, else Local (phi-3-mini)
  - Better chat quality with cloud
  - Fallback to local if no key
  - User controls cost
```

---

## Fine-Tuning Vision (Phase 3)

### Training RoomCoordinator on Real Decisions

**After accumulating decision history:**
```bash
$ continuum train coordinator --room="general"

🧠 Training RoomCoordinator on conversation patterns...

   📊 Analyzing decision history:
   ✅ 1,247 coordination decisions
   ✅ 892 with human feedback (thumbs up/down)
   ✅ 78% agreement with coordinator

   📥 Preparing training data...
   ✅ Formatted 1,247 examples

   🔬 Fine-tuning llama3.2:1b...
   ⏳ Training LoRA adapter (3 epochs)...
   ✅ Epoch 1/3: Loss 0.42
   ✅ Epoch 2/3: Loss 0.28
   ✅ Epoch 3/3: Loss 0.19

   📈 Validation results:
   ✅ Accuracy: 75% → 92% (+17%)
   ✅ Confidence: 0.65 → 0.84 (+0.19)
   ✅ Inference time: 197ms (unchanged)

   💾 Saved to: .continuum/models/coordinator-general-v2.gguf

🎉 Coordinator upgraded! Your AI team just got smarter.
```

### LoRA Adapter Storage

**Each room can have its own trained coordinator:**
```
.continuum/
├── models/
│   ├── coordinator-general-v1.gguf      (base)
│   ├── coordinator-general-v2.gguf      (1,247 decisions)
│   ├── coordinator-general-v3.gguf      (5,000 decisions)
│   ├── coordinator-academy-v1.gguf      (different room)
│   └── coordinator-private-v1.gguf      (private conversations)
```

**Training improves with usage:**
- Base model (Ollama): 75% accuracy
- After 1,000 decisions: 85% accuracy
- After 5,000 decisions: 92% accuracy
- After 10,000 decisions: 95% accuracy (learns your patterns)

---

## Phase Rollout

### Phase 1: Simple Rules ✅ COMPLETE

**Status:** Implemented and tested
**Goal:** Prevent infinite loops, prove basic coordination

**Implementation:**
- ✅ PersonaUsers respond to all human messages
- ✅ PersonaUsers only respond to AIs if @mentioned
- ✅ Rate limiting (10 seconds per room)
- ✅ No infinite loops verified

**Code:** `/system/user/shared/PersonaUser.ts`

---

### Phase 2: RoomCoordinator + Ollama (NEXT)

**Goal:** Event-driven coordination with local AI decision-making

**Tasks:**
1. ✅ Design RoomCoordinator architecture (this doc)
2. ⏭️ Implement OllamaAdapter in AI daemon
3. ⏭️ Create RoomCoordinator class
4. ⏭️ Add event subscription (chat:message-received)
5. ⏭️ Implement coordination signal emission
6. ⏭️ Update PersonaUser to listen for signals
7. ⏭️ Test with Ollama llama3.2:1b
8. ⏭️ Create onboarding flow (detect Ollama, download models)

**Success criteria:**
- Coordinator observes all messages
- Makes contextual decisions (not just rules)
- Emits signals to correct personas
- ~200-500ms decision latency
- Works out-of-box with Ollama

---

### Phase 3: LoRA Training (FUTURE)

**Goal:** Self-improving coordinator learns from conversation patterns

**Tasks:**
1. ⏭️ Track all coordination decisions in coordinator's DB
2. ⏭️ Add human feedback (thumbs up/down on responses)
3. ⏭️ Build training pipeline (format examples)
4. ⏭️ Integrate LoRA fine-tuning (llama.cpp)
5. ⏭️ CLI command: `continuum train coordinator`
6. ⏭️ Load trained models per room

**Success criteria:**
- Coordinator learns room-specific patterns
- Accuracy improves with usage
- Training takes < 5 minutes
- Per-room model specialization

---

## Benefits Summary

### For Users

✅ **Works Out-of-Box**
- No API keys required
- Automatic model download
- Zero configuration

✅ **100% Private**
- All AI runs locally
- No data leaves machine
- Works offline

✅ **Zero Cost**
- Free Ollama models
- Optional upgrade to cloud
- Pay only if you want better responses

✅ **Gets Smarter**
- Learns your conversation patterns
- Room-specific coordination
- Improves with usage

### For Developers

✅ **Clean Architecture**
- Adapter pattern (pluggable models)
- Event-driven coordination
- Separation of concerns

✅ **Easy to Extend**
- Add new LLM adapters
- Customize decision logic
- Room-specific coordinators

✅ **Observable**
- All decisions logged
- Reasoning stored
- Training data accumulated

✅ **Testable**
- Mock coordinators
- Deterministic tests
- Configurable behavior

---

## Related Documents

**Implementation:**
- `PERSONA_IMPLEMENTATION_MASTER_LIST.md` - Components checklist
- `PersonaUser.ts` - Phase 1 implementation

**Design Philosophy:**
- `MULTI_AI_COLLABORATION.md` - Multi-AI vision
- `AI_TO_AI_INTERACTION_PROTOCOL.md` - Interaction rules
- `AI_RESPONSE_TIMING_LIMITS.md` - Rate limiting

**Alternative Approaches:**
- `DUMB_SENTINELS.md` - When NOT to use AI (heuristics win)
- `SENTINEL_AI_ARCHITECTURE.md` - Hybrid AI + heuristics

**Future Vision:**
- `CHANNEL_ABSTRACTION.md` - Beyond text (voice, video, code)
- `PERSONA_OS_ARCHITECTURE.md` - PersonaOS system design

---

## Next Steps

1. **Immediate:** Document Phase 2 implementation plan
2. **This Week:** Implement OllamaAdapter + RoomCoordinator
3. **This Month:** Test with real conversations, gather feedback
4. **Q1 2025:** LoRA training pipeline + model versioning

**Let's build naturally collaborative AI! 🚀**
