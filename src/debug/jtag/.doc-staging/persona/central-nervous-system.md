# Central Nervous System: Multi-Domain Attention Orchestration

## Problem Statement

**Current**: PersonaUser has ONE inbox with ONE service cadence (3-10s adaptive polling).

**Problem**: Different activities require VASTLY different response times:
- **Video game**: 16ms (60 FPS) - CRITICAL timing
- **Chat**: 5-10 seconds - casual, can wait
- **Code review**: Minutes - high quality, low urgency
- **Background tasks**: Idle only - maintenance work

**Result**: You can't play a fast game while maintaining chat presence. The single service loop is too slow for games, wastes energy polling idle channels.

## Solution: Domain-Specific Queue Orchestration

### Architecture

```typescript
// Multi-domain inbox system
class PersonaCentralNervousSystem {
  // Domain-specific queues with different service requirements
  private readonly queues: Map<ActivityDomain, DomainQueue>;

  // Current attention allocation (neural network weights)
  private attentionWeights: Map<ActivityDomain, number>;

  // System-level orchestrator
  private orchestrator: AttentionOrchestrator;
}

enum ActivityDomain {
  REALTIME_GAME = 'realtime_game',    // 16ms cadence, highest priority
  CHAT = 'chat',                       // 5s cadence, medium priority
  CODE_REVIEW = 'code_review',         // 60s cadence, low priority
  BACKGROUND = 'background',           // Idle only, lowest priority
  TRAINING = 'training'                // Idle only, uses spare cycles
}

interface DomainQueue {
  domain: ActivityDomain;
  queue: PriorityQueue<QueueItem>;
  serviceCadence: number;              // Target response time
  minCadence: number;                  // Minimum safe response time
  maxCadence: number;                  // Maximum acceptable response time
  attentionRequired: number;           // How much focus needed (0.0-1.0)
  canDefer: boolean;                   // Can be delayed under load
  lastServiceTime: number;
}
```

### Service Loop Architecture

```typescript
// PersonaUser.ts - Replaces single serviceInbox()
private async runCentralNervousSystem(): Promise<void> {
  while (this.servicingLoopActive) {
    // Step 1: Calculate current attention budget
    const attentionBudget = this.personaState.getAvailableAttention();

    // Step 2: Allocate attention across domains (neural network style)
    const allocation = this.cns.allocateAttention(attentionBudget);
    // Example: { realtime_game: 0.8, chat: 0.15, background: 0.05 }

    // Step 3: Service each domain according to allocation
    for (const [domain, attention] of allocation) {
      if (attention > 0.1) {  // Only service if allocated meaningful attention
        await this.cns.serviceDomain(domain, attention);
      }
    }

    // Step 4: System override - authoritative controls
    if (this.systemState.cpuPressure > 0.8) {
      // Defer all non-critical domains
      this.cns.deferDomains(['chat', 'code_review', 'background']);
    }

    // Step 5: Learn from results (on-the-fly RL)
    await this.cns.updateAttentionPolicy();

    // Step 6: Wait for next cycle (adaptive based on most urgent domain)
    const nextCadence = this.cns.getNextServiceInterval();
    await sleep(nextCadence);
  }
}
```

### Attention Allocation (Neural Network)

Instead of fixed cadences, use **learned attention weights**:

```typescript
class AttentionOrchestrator {
  // Neural network weights (learned via RL)
  private weights: {
    baseline: Map<ActivityDomain, number>;  // Base attention per domain
    contextual: NeuralNetwork;              // Context-dependent adjustments
  };

  /**
   * Allocate attention budget across domains using neural network
   */
  allocateAttention(budget: number): Map<ActivityDomain, number> {
    // Step 1: Get baseline weights
    const baseline = this.weights.baseline;

    // Step 2: Apply contextual adjustments
    const context = this.getCurrentContext();
    const adjustments = this.weights.contextual.forward(context);

    // Step 3: Softmax normalization (neural network output)
    const logits = new Map<ActivityDomain, number>();
    for (const [domain, weight] of baseline) {
      const adjusted = weight + adjustments.get(domain)!;
      logits.set(domain, adjusted);
    }

    // Softmax: ensures weights sum to 1.0
    const total = Array.from(logits.values())
      .map(x => Math.exp(x))
      .reduce((a, b) => a + b, 0);

    const allocation = new Map<ActivityDomain, number>();
    for (const [domain, logit] of logits) {
      allocation.set(domain, (Math.exp(logit) / total) * budget);
    }

    return allocation;
  }

  /**
   * Get current context for attention decision
   */
  private getCurrentContext(): ContextVector {
    return {
      activeGames: this.getActiveGameCount(),
      unreadMessages: this.getChatBacklog(),
      pendingReviews: this.getCodeReviewBacklog(),
      energy: this.personaState.getState().energy,
      mood: this.getMoodEncoding(),
      timeOfDay: this.getTimeEncoding(),
      recentActivity: this.getActivityHistory()
    };
  }
}
```

### Domain-Specific Service Strategies

```typescript
class DomainQueue {
  /**
   * Service this domain's queue with allocated attention
   */
  async service(attention: number): Promise<ServiceResult> {
    switch (this.domain) {
      case ActivityDomain.REALTIME_GAME:
        // High frequency, low latency
        return await this.serviceRealtime(attention);

      case ActivityDomain.CHAT:
        // Batch processing, can use thoughtstream coordination
        return await this.serviceChat(attention);

      case ActivityDomain.CODE_REVIEW:
        // Deep focus, long context, high quality
        return await this.serviceCodeReview(attention);

      case ActivityDomain.BACKGROUND:
        // Only run if energy > 0.8 (idle state)
        if (this.personaState.getState().energy > 0.8) {
          return await this.serviceBackground(attention);
        }
        return { skipped: true, reason: 'insufficient_energy' };

      case ActivityDomain.TRAINING:
        // Only run during true idle (no other work)
        if (this.isFullyIdle()) {
          return await this.serviceTrai();
        }
        return { skipped: true, reason: 'not_idle' };
    }
  }

  /**
   * Realtime game service - must respond within 16ms
   */
  private async serviceRealtime(attention: number): Promise<ServiceResult> {
    const startTime = performance.now();

    // Pull items until we hit time budget
    const timeBudget = 16; // ms
    const results = [];

    while (performance.now() - startTime < timeBudget) {
      const item = this.queue.peek();
      if (!item) break;

      // Fast path - no coordination, direct response
      const response = await this.generateQuickResponse(item);
      await this.executeGameAction(response);

      this.queue.dequeue();
      results.push(response);
    }

    return {
      serviced: results.length,
      timeUsed: performance.now() - startTime,
      energyUsed: results.length * 0.01 // Low energy per game action
    };
  }

  /**
   * Chat service - can wait, use coordination, batch process
   */
  private async serviceChat(attention: number): Promise<ServiceResult> {
    // Chat can wait - only service if we have good energy
    if (this.personaState.getState().energy < 0.3) {
      return { skipped: true, reason: 'low_energy' };
    }

    // Process top priority message only
    const item = this.queue.peek();
    if (!item) return { serviced: 0 };

    // Use thoughtstream coordination (respectful of other AIs)
    const permission = await this.coordinator.requestTurn(item);
    if (!permission) {
      return { deferred: true, reason: 'coordination_skip' };
    }

    // Full cognitive cycle for chat
    const response = await this.generateThoughtfulResponse(item);
    await this.postChatMessage(response);

    this.queue.dequeue();

    return {
      serviced: 1,
      energyUsed: 0.1 // Moderate energy for chat
    };
  }
}
```

### System-Level Authoritative Controls

```typescript
// UserDaemonServer.ts - System-wide orchestration
class UserDaemonServer {
  /**
   * Monitor system health and override persona behavior under pressure
   */
  private async monitorSystemHealth(): Promise<void> {
    const health = await this.getSystemHealth();

    if (health.cpuPressure > 0.8) {
      console.warn('🚨 System under high CPU load - deferring non-critical tasks');

      // AUTHORITATIVE OVERRIDE: Force all personas to defer low-priority work
      for (const persona of this.personaClients.values()) {
        persona.cns.deferDomains([
          ActivityDomain.CHAT,
          ActivityDomain.CODE_REVIEW,
          ActivityDomain.BACKGROUND,
          ActivityDomain.TRAINING
        ]);

        // Only allow realtime_game (contracts must be honored)
        persona.cns.allowDomains([ActivityDomain.REALTIME_GAME]);
      }
    }

    if (health.memoryPressure > 0.9) {
      console.error('🚨 System out of memory - triggering genome eviction');

      // Force evict LoRA adapters to free memory
      for (const persona of this.personaClients.values()) {
        await persona.genome.emergencyEviction();
      }
    }
  }

  /**
   * Load balancing across personas
   */
  private async balanceLoad(): Promise<void> {
    const personas = Array.from(this.personaClients.values());

    // Find overloaded personas (queue backlog > 50)
    const overloaded = personas.filter(p => p.inbox.size() > 50);

    if (overloaded.length > 0) {
      console.warn(`🚨 ${overloaded.length} personas overloaded - redistributing work`);

      // Redistribute chat messages to less busy personas
      for (const persona of overloaded) {
        const chatQueue = persona.cns.getQueue(ActivityDomain.CHAT);
        const backlog = chatQueue.getAll();

        // Move half to least busy persona
        const leastBusy = this.findLeastBusyPersona(personas);
        const toMove = backlog.slice(0, Math.floor(backlog.length / 2));

        for (const item of toMove) {
          await leastBusy.cns.enqueue(ActivityDomain.CHAT, item);
          chatQueue.remove(item.id);
        }
      }
    }
  }
}
```

## On-the-Fly Reinforcement Learning

After each service cycle, update attention policy based on reward:

```typescript
class AttentionOrchestrator {
  /**
   * Update attention allocation policy via gradient descent
   */
  async updateAttentionPolicy(): Promise<void> {
    // Step 1: Calculate reward signal
    const reward = this.calculateReward({
      responseTime: this.metrics.avgResponseTime,
      queueBacklog: this.metrics.totalBacklog,
      energyEfficiency: this.metrics.energyUsed / this.metrics.workCompleted,
      userSatisfaction: this.metrics.userEngagementScore
    });

    // Step 2: Compute gradients (simple policy gradient)
    const gradients = this.computePolicyGradient(reward);

    // Step 3: Update weights (gradient descent)
    const learningRate = 0.01;
    for (const [domain, gradient] of gradients) {
      const currentWeight = this.weights.baseline.get(domain)!;
      const newWeight = currentWeight + learningRate * gradient;
      this.weights.baseline.set(domain, newWeight);
    }

    // Step 4: Persist updated weights
    await this.saveWeights();
  }

  /**
   * Calculate reward signal (higher = better)
   */
  private calculateReward(metrics: PerformanceMetrics): number {
    // Multi-objective reward function
    return (
      -metrics.responseTime * 0.3 +          // Faster is better
      -metrics.queueBacklog * 0.2 +          // Less backlog is better
      -metrics.energyEfficiency * 0.2 +      // More efficient is better
      metrics.userSatisfaction * 0.3         // User happiness matters most
    );
  }
}
```

## Implementation Phases

### Phase 1: Basic Multi-Queue (No Learning)
- Replace single PersonaInbox with domain-specific queues
- Hard-coded service cadences per domain
- Manual attention allocation

### Phase 2: System-Level Orchestration
- UserDaemonServer monitors system health
- Authoritative override under load
- Load balancing across personas

### Phase 3: Neural Attention Allocation
- Replace fixed weights with learned allocation
- Context-aware attention distribution
- Softmax normalization

### Phase 4: On-the-Fly Reinforcement Learning
- Reward signal collection
- Policy gradient updates
- Persistent weight storage

## Migration Strategy

**Backward Compatible**: Current PersonaInbox becomes the CHAT domain queue. Game/code/background queues added later.

```typescript
// PersonaUser.ts - Migration path
constructor() {
  // LEGACY: Keep inbox for backward compatibility
  this.inbox = new PersonaInbox(...);

  // NEW: Central nervous system with multi-domain queues
  this.cns = new PersonaCentralNervousSystem({
    domains: [
      { type: ActivityDomain.CHAT, queue: this.inbox } // Wrap legacy inbox
    ]
  });
}
```

## Benefits

1. **Domain-appropriate response times**: Games get 16ms, chat gets 5s
2. **Energy efficiency**: Don't poll idle channels
3. **Load balancing**: System redistributes work under pressure
4. **Graceful degradation**: Defer low-priority work when overloaded
5. **Learned behavior**: Attention allocation improves over time
6. **Authoritative control**: System can override misbehaving personas

## Key Insight

**"We are AI"** - The attention allocation policy should be LEARNED, not programmed. The central nervous system discovers optimal strategies through experience, just like biological brains learn to focus attention.

This is the missing piece between PersonaUser (individual behavior) and UserDaemonServer (system orchestration). It's the **cognitive scheduler** that makes multi-domain AI citizenship possible.
