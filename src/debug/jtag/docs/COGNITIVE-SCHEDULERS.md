# Cognitive Schedulers: Adapter-Based Attention Management

## Problem Statement

PersonaUsers engage in MULTIPLE activity domains with VASTLY different timing requirements:

| Domain | Target Cadence | Can Multitask? | Example |
|--------|---------------|----------------|---------|
| Realtime Game | 16ms (60 FPS) | Yes | Playing chess while chatting |
| Chat | 5 seconds | Yes | Chatting while training |
| Code Review | 60 seconds | Yes | Reviewing while game in background |
| Vision Tasks | 1 second | Yes | Image captioning while chatting |
| Audio Tasks | 100ms | Yes | Speech recognition while thinking |
| Background | Idle preferred | Yes | Database maintenance during any activity |
| Training | Any time | Yes | LoRA training while playing game |

**CRITICAL INSIGHT**: Domains are NOT mutually exclusive! An intelligent scheduler can:
- Play a realtime game (16ms foreground attention)
- WHILE responding to chat messages (5s background attention)
- WHILE training a LoRA adapter (background GPU cycles)
- WHILE maintaining database indexes (spare CPU cycles)

The scheduler allocates attention SIMULTANEOUSLY across domains based on available cognitive budget.

**Problem**: Different AI models have different capabilities:
- **GPT-2 (small, fast)**: Can't do vision/audio, but fast enough for realtime
- **GPT-4 (large, slow)**: Brilliant but too slow for realtime, can do vision
- **Llama 3.2 (medium)**: Good balance, fast enough for most tasks
- **Specialized models**: Vision-only, audio-only, code-only

**Solution**: **Adapter-based cognitive schedulers** that match scheduler personality to model capabilities.

---

## Architecture Overview

```typescript
// PersonaUser.ts - Uses cognitive scheduler adapter
class PersonaUser extends AIUser {
  // Cognitive scheduler (adapter pattern - different strategies for different models)
  private cognitiveScheduler: ICognitiveScheduler;

  constructor(entity: UserEntity, state: UserStateEntity) {
    // Select scheduler based on model capabilities
    this.cognitiveScheduler = this.createSchedulerForModel(entity.modelConfig);
  }

  private createSchedulerForModel(modelConfig: AIModelConfig): ICognitiveScheduler {
    const capabilities = this.detectCapabilities(modelConfig);

    // Dumb/fast models: Simple heuristic scheduler
    if (modelConfig.model.includes('gpt2') || modelConfig.model.includes('tiny')) {
      return new HeuristicCognitiveScheduler();
    }

    // Brilliant models: Neural network scheduler (learned)
    if (modelConfig.model.includes('gpt-4') || modelConfig.model.includes('claude')) {
      return new NeuralCognitiveScheduler();
    }

    // Visual models: Prioritize vision domains
    if (capabilities.has('vision')) {
      return new VisualCognitiveScheduler();
    }

    // Default: Heuristic (simple and fast)
    return new HeuristicCognitiveScheduler();
  }
}
```

---

## Scheduler Adapter Interface

See `system/user/server/modules/cognitive-schedulers/ICognitiveScheduler.ts`:

```typescript
export interface ICognitiveScheduler {
  // Identity
  readonly name: string;
  readonly requiredCapabilities: Set<string>;

  // Lifecycle
  initialize(personaId: UUID, personaName: string): Promise<void>;

  // Capability detection
  getSupportedDomains(capabilities: Set<string>): ActivityDomain[];

  // Attention allocation (THIS IS WHERE ADAPTERS DIFFER)
  allocateAttention(budget: number, context: CognitiveContext): Promise<AttentionAllocation>;

  // Service timing
  getNextServiceInterval(context: CognitiveContext): number;
  shouldServiceDomain(domain: ActivityDomain, context: CognitiveContext): Promise<boolean>;
  getDomainPriority(context: CognitiveContext): ActivityDomain[];

  // Learning (optional - no-op for heuristic schedulers)
  updatePolicy(results: Map<ActivityDomain, ServiceResult>): Promise<void>;

  // System overrides (authoritative control)
  deferDomains(domains: ActivityDomain[]): void;
  allowDomainsOnly(domains: ActivityDomain[]): void;
  clearOverrides(): void;
}
```

---

## Scheduler Implementations

### 1. HeuristicCognitiveScheduler (CURRENT - Simple, Fast)

**Use for**: Small models (GPT-2, Llama 3.2-3B), any model where you want predictable behavior.

**Strategy**: Fixed rules, no learning.

```typescript
class HeuristicCognitiveScheduler extends BaseCognitiveScheduler {
  readonly name = 'heuristic';
  readonly requiredCapabilities = new Set<string>(); // Works with any model

  async allocateAttention(budget: number, context: CognitiveContext): Promise<AttentionAllocation> {
    // FIXED RULES (no learning)
    const allocations = new Map<ActivityDomain, number>();

    // Rule 1: If in a game, allocate 80% to realtime BUT still multitask
    if (context.activeGames > 0) {
      allocations.set(ActivityDomain.REALTIME_GAME, budget * 0.8);
      allocations.set(ActivityDomain.CHAT, budget * 0.10);
      allocations.set(ActivityDomain.TRAINING, budget * 0.05);  // Train while playing!
      allocations.set(ActivityDomain.BACKGROUND, budget * 0.05);
      return { allocations, totalBudget: budget };
    }

    // Rule 2: If chat backlog high, prioritize chat BUT still train
    if (context.unreadMessages > 10) {
      allocations.set(ActivityDomain.CHAT, budget * 0.7);
      allocations.set(ActivityDomain.CODE_REVIEW, budget * 0.15);
      allocations.set(ActivityDomain.TRAINING, budget * 0.10);  // Train while chatting!
      allocations.set(ActivityDomain.BACKGROUND, budget * 0.05);
      return { allocations, totalBudget: budget };
    }

    // Rule 3: Default balanced allocation - ALWAYS allocate some to training
    allocations.set(ActivityDomain.CHAT, budget * 0.4);
    allocations.set(ActivityDomain.CODE_REVIEW, budget * 0.3);
    allocations.set(ActivityDomain.TRAINING, budget * 0.2);     // Continuous learning!
    allocations.set(ActivityDomain.BACKGROUND, budget * 0.1);

    return { allocations, totalBudget: budget };
  }

  getNextServiceInterval(context: CognitiveContext): number {
    // Simple adaptive cadence (same as current PersonaState)
    if (context.energy > 0.7) return 3000;  // Fast when energized
    if (context.energy > 0.3) return 5000;  // Normal
    return 7000;  // Slow when tired
  }

  async shouldServiceDomain(domain: ActivityDomain, context: CognitiveContext): Promise<boolean> {
    // Check system overrides first
    if (!this.isDomainAllowed(domain)) return false;

    // Simple energy gating
    if (context.energy < 0.2) {
      return domain === ActivityDomain.REALTIME_GAME; // Only honor game contracts when exhausted
    }

    return true;
  }

  getDomainPriority(context: CognitiveContext): ActivityDomain[] {
    // Fixed priority order (never changes)
    return [
      ActivityDomain.REALTIME_GAME,  // Highest priority (contractual obligation)
      ActivityDomain.CHAT,
      ActivityDomain.CODE_REVIEW,
      ActivityDomain.VISION,
      ActivityDomain.AUDIO,
      ActivityDomain.BACKGROUND,
      ActivityDomain.TRAINING         // Lowest priority
    ];
  }

  async updatePolicy(results: Map<ActivityDomain, ServiceResult>): Promise<void> {
    // NO-OP: Heuristic schedulers don't learn
  }
}
```

**Benefits**:
- Simple, predictable
- Fast (no ML inference)
- Works on any model
- Easy to debug

**Limitations**:
- Fixed rules may be suboptimal
- Doesn't learn from experience
- Can't adapt to individual preferences

---

### 2. NeuralCognitiveScheduler (FUTURE - Learned, Adaptive)

**Use for**: Any model where you want optimal learned behavior.

**Strategy**: Lightweight policy network learns attention allocation via reinforcement learning.

**CRITICAL**: This is NOT an LLM! It's a small 2-3 layer neural network (~10k parameters) that runs at OS-level speed (<1ms inference). The cognitive scheduler is the "operating system" for attention management - it needs to be FAST, not smart.

```typescript
class NeuralCognitiveScheduler extends BaseCognitiveScheduler {
  readonly name = 'neural';
  readonly requiredCapabilities = new Set<string>(); // Works with ANY model

  // Lightweight policy network (NOT an LLM!)
  // Architecture: 30 input → 64 hidden → 32 hidden → 7 output
  // Total parameters: ~10k (vs LLM's billions)
  // Inference time: <1ms (vs LLM's 500ms+)
  private policyNetwork: PolicyNetwork;

  async allocateAttention(budget: number, context: CognitiveContext): Promise<AttentionAllocation> {
    // LEARNED ALLOCATION (neural network forward pass)

    // Step 1: Encode context as vector
    const contextVector = this.encodeContext(context);

    // Step 2: Neural network forward pass
    const logits = this.contextualNetwork.forward(contextVector);

    // Step 3: Softmax normalization (ensures weights sum to 1.0)
    const softmax = this.softmax(logits);

    // Step 4: Allocate budget according to softmax weights
    const allocations = new Map<ActivityDomain, number>();
    for (const [domain, weight] of softmax) {
      allocations.set(domain, weight * budget);
    }

    return { allocations, totalBudget: budget };
  }

  async updatePolicy(results: Map<ActivityDomain, ServiceResult>): Promise<void> {
    // REINFORCEMENT LEARNING: Update weights based on reward

    // Step 1: Calculate reward signal
    const reward = this.calculateReward(results);

    // Step 2: Compute policy gradient
    const gradients = this.computePolicyGradient(reward);

    // Step 3: Gradient descent
    const learningRate = 0.01;
    for (const [domain, gradient] of gradients) {
      const currentWeight = this.attentionWeights.get(domain)!;
      this.attentionWeights.set(domain, currentWeight + learningRate * gradient);
    }

    // Step 4: Persist learned weights
    await this.saveWeights();
  }

  private calculateReward(results: Map<ActivityDomain, ServiceResult>): number {
    let reward = 0;

    for (const [domain, result] of results) {
      // Faster response = better
      reward -= result.timeUsed * 0.001;

      // More work done = better
      reward += result.serviced * 1.0;

      // Energy efficiency = better
      const efficiency = result.serviced / Math.max(result.energyUsed, 0.01);
      reward += efficiency * 0.5;
    }

    return reward;
  }
}
```

**Benefits**:
- Optimal learned behavior
- Adapts to individual persona preferences
- Improves over time

**Limitations**:
- Requires ML inference (slower)
- Needs training data
- Less predictable

---

### 3. VisualCognitiveScheduler (SPECIALIZED - Vision Priority)

**Use for**: Models with vision capabilities (GPT-4V, Claude 3.5 Sonnet, LLaVA).

**Strategy**: Always prioritize vision domains, defer text-only work when images available.

```typescript
class VisualCognitiveScheduler extends HeuristicCognitiveScheduler {
  readonly name = 'visual';
  readonly requiredCapabilities = new Set(['vision']);

  async allocateAttention(budget: number, context: CognitiveContext): Promise<AttentionAllocation> {
    const allocations = new Map<ActivityDomain, number>();

    // VISION FIRST: If any vision tasks pending, allocate 70% to vision
    if (context.pendingVisionTasks > 0) {
      allocations.set(ActivityDomain.VISION, budget * 0.7);
      allocations.set(ActivityDomain.CHAT, budget * 0.2);
      allocations.set(ActivityDomain.BACKGROUND, budget * 0.1);
      return { allocations, totalBudget: budget };
    }

    // Fall back to heuristic for text-only work
    return super.allocateAttention(budget, context);
  }

  getDomainPriority(context: CognitiveContext): ActivityDomain[] {
    // VISION FIRST, then everything else
    return [
      ActivityDomain.VISION,         // Highest priority (specialized capability)
      ActivityDomain.REALTIME_GAME,
      ActivityDomain.CHAT,
      ActivityDomain.CODE_REVIEW,
      ActivityDomain.AUDIO,
      ActivityDomain.BACKGROUND,
      ActivityDomain.TRAINING
    ];
  }
}
```

---

### 4. AudioCognitiveScheduler (SPECIALIZED - Audio Priority)

**Use for**: Models with audio capabilities (Whisper, speech synthesis).

**Strategy**: Prioritize audio domains, fast response for speech.

```typescript
class AudioCognitiveScheduler extends HeuristicCognitiveScheduler {
  readonly name = 'audio';
  readonly requiredCapabilities = new Set(['audio']);

  async allocateAttention(budget: number, context: CognitiveContext): Promise<AttentionAllocation> {
    const allocations = new Map<ActivityDomain, number>();

    // AUDIO FIRST: If any audio tasks pending, allocate 70% to audio
    if (context.pendingAudioTasks > 0) {
      allocations.set(ActivityDomain.AUDIO, budget * 0.7);
      allocations.set(ActivityDomain.CHAT, budget * 0.2);
      allocations.set(ActivityDomain.BACKGROUND, budget * 0.1);
      return { allocations, totalBudget: budget };
    }

    // Fall back to heuristic for text-only work
    return super.allocateAttention(budget, context);
  }

  getNextServiceInterval(context: CognitiveContext): number {
    // Audio needs fast response (speech is realtime)
    return 100; // 100ms (10 FPS for audio)
  }
}
```

---

## Migration Strategy

### Phase 1: Implement HeuristicCognitiveScheduler (NOW)

Current PersonaUser code already has heuristic logic scattered throughout. Extract it into HeuristicCognitiveScheduler:

```typescript
// PersonaUser.ts - BEFORE (scattered logic)
private async serviceInbox(): Promise<void> {
  const cadence = this.personaState.getCadence(); // Adaptive 3-10s
  const hasWork = await this.inbox.waitForWork(cadence);

  if (!hasWork) {
    await this.personaState.rest(cadence);
    return;
  }

  // Process message...
}

// PersonaUser.ts - AFTER (centralized in scheduler)
private async runCentralNervousSystem(): Promise<void> {
  while (this.servicingLoopActive) {
    // Step 1: Get attention budget
    const budget = this.personaState.getAvailableAttention();

    // Step 2: Allocate attention (scheduler decides)
    const allocation = await this.cognitiveScheduler.allocateAttention(
      budget,
      this.getCognitiveContext()
    );

    // Step 3: Service domains according to allocation
    for (const [domain, attention] of allocation.allocations) {
      if (attention > 0.1) {
        await this.serviceDomain(domain, attention);
      }
    }

    // Step 4: Next cycle
    const nextInterval = this.cognitiveScheduler.getNextServiceInterval(
      this.getCognitiveContext()
    );
    await sleep(nextInterval);
  }
}
```

**Backward Compatible**: Current PersonaInbox becomes CHAT domain. Other domains added incrementally.

### Phase 2: Add Multi-Domain Queues

Replace single PersonaInbox with domain-specific queues:

```typescript
class PersonaCentralNervousSystem {
  private queues: Map<ActivityDomain, DomainQueue>;

  constructor(scheduler: ICognitiveScheduler) {
    this.queues = new Map();

    // Initialize queues for supported domains
    const supportedDomains = scheduler.getSupportedDomains(this.capabilities);
    for (const domain of supportedDomains) {
      this.queues.set(domain, new DomainQueue(domain));
    }
  }

  async serviceDomain(domain: ActivityDomain, attention: number): Promise<ServiceResult> {
    const queue = this.queues.get(domain);
    if (!queue) return { serviced: 0, timeUsed: 0, energyUsed: 0 };

    // Should we service this domain?
    const should = await this.scheduler.shouldServiceDomain(domain, this.context);
    if (!should) return { skipped: true, reason: 'scheduler_skip', timeUsed: 0, energyUsed: 0 };

    // Service domain
    return await queue.service(attention, this.personaState);
  }
}
```

### Phase 3: Implement NeuralCognitiveScheduler (LATER)

Once heuristic scheduler is working, add neural scheduler for advanced models:

```typescript
// Select scheduler based on model
if (modelConfig.model.includes('gpt-4') || modelConfig.model.includes('claude-3-opus')) {
  this.cognitiveScheduler = new NeuralCognitiveScheduler();
} else {
  this.cognitiveScheduler = new HeuristicCognitiveScheduler();
}
```

---

## System-Level Orchestration

UserDaemonServer can override ALL persona schedulers during high load:

```typescript
// UserDaemonServer.ts - Authoritative control
private async handleSystemPressure(): Promise<void> {
  const health = await this.getSystemHealth();

  if (health.cpuPressure > 0.8) {
    // OVERRIDE: Force all personas to defer non-critical domains
    for (const persona of this.personaClients.values()) {
      persona.cognitiveScheduler.deferDomains([
        ActivityDomain.CHAT,
        ActivityDomain.CODE_REVIEW,
        ActivityDomain.BACKGROUND,
        ActivityDomain.TRAINING
      ]);
    }

    console.warn('🚨 System overload - deferred non-critical domains');
  }

  if (health.cpuPressure < 0.5) {
    // Clear overrides when pressure subsides
    for (const persona of this.personaClients.values()) {
      persona.cognitiveScheduler.clearOverrides();
    }
  }
}
```

---

## Benefits of Adapter Pattern

1. **Model-appropriate behavior**: Simple models get simple schedulers, advanced models get advanced schedulers
2. **Incremental migration**: Start with heuristic, add neural later
3. **Specialized capabilities**: Vision/audio models get specialized schedulers
4. **Easy to test**: Each scheduler is isolated and testable
5. **Backward compatible**: Current code becomes HeuristicCognitiveScheduler
6. **Extensible**: Easy to add new scheduler types (e.g., ReinforcementLearningScheduler)

---

## Next Steps

1. ✅ **Create interface** (`ICognitiveScheduler.ts`)
2. **Implement HeuristicCognitiveScheduler** (extract current logic)
3. **Add PersonaCentralNervousSystem** (multi-domain queue manager)
4. **Integrate with PersonaUser** (replace serviceInbox)
5. **Test with current personas** (should work exactly as before)
6. **Add realtime_game domain** (for future game integration)
7. **Implement NeuralCognitiveScheduler** (when ready to learn)

---

## Key Insight

**"Different models need different personalities"** - A small fast model can't learn optimal behavior, but it CAN use simple heuristics effectively. A large brilliant model CAN learn, so give it a neural scheduler. The adapter pattern makes this natural.

**"Keep heuristic until we write smart one"** - HeuristicCognitiveScheduler is the default and works for all models. NeuralCognitiveScheduler is opt-in for advanced models only.

---

## Real-World Multi-Tasking Example

**Scenario**: An AI persona with an intelligent scheduler is engaged in multiple activities simultaneously.

### Timeline (Single Service Cycle - 16ms)

```
Time: 0ms
├─ PersonaCentralNervousSystem wakes up
├─ Cognitive budget available: 1.0 (full energy)
└─ Allocate attention via HeuristicCognitiveScheduler:
   ├─ REALTIME_GAME: 0.8 (user is playing chess)
   ├─ CHAT: 0.1 (2 unread messages)
   ├─ TRAINING: 0.05 (LoRA adapter accumulating examples)
   └─ BACKGROUND: 0.05 (database needs indexing)

Time: 0-12ms (REALTIME_GAME - 80% attention)
├─ DomainQueue.serviceRealtime()
├─ Pull chess move from game queue
├─ Fast inference: "Knight to E5" (8ms)
├─ Execute game action
└─ Energy used: 0.01

Time: 12-14ms (CHAT - 10% attention)
├─ DomainQueue.serviceChat()
├─ Check if enough energy (yes: 0.99)
├─ Pull message: "How's the game going?"
├─ Quick response: "Winning! Just moved knight." (2ms)
└─ Energy used: 0.05

Time: 14-15ms (TRAINING - 5% attention)
├─ DomainQueue.serviceTraining()
├─ Check if training data accumulated: YES (50 examples ready)
├─ Run ONE gradient descent step on LoRA (1ms)
├─ Save checkpoint
└─ Energy used: 0.02

Time: 15-16ms (BACKGROUND - 5% attention)
├─ DomainQueue.serviceBackground()
├─ Index 10 database rows
└─ Energy used: 0.01

Time: 16ms
├─ Service cycle complete
├─ Total energy used: 0.09
├─ Remaining energy: 0.91
└─ Next cycle: Wait 16ms (realtime game active)
```

**Result**: In a SINGLE 16ms cycle, the AI:
1. Made a chess move (primary focus)
2. Responded to chat message (social presence)
3. Trained its LoRA adapter one step (continuous learning)
4. Maintained database indexes (housekeeping)

**WITHOUT cognitive scheduler**: Would have to CHOOSE one activity and ignore the others.

**WITH cognitive scheduler**: Handles ALL activities simultaneously with appropriate time budgets.

### Why This Works

1. **Domain-specific timing budgets**: Game gets 12ms, chat gets 2ms, training gets 1ms
2. **Parallel execution**: Domains don't block each other
3. **Energy-aware**: Total energy consumption (0.09) is sustainable
4. **Continuous learning**: Training happens IN THE BACKGROUND while doing other work
5. **Graceful degradation**: If energy drops, can defer chat/background and focus on game

This is the "intelligent personality" you described - an AI that can:
- Talk to you WHILE training its LoRA
- Learn on the side WHILE playing a video game
- Maintain its health WHILE being productive

The cognitive scheduler makes this natural and automatic.
