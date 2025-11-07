# Central Nervous System Implementation Guide

## Overview

The PersonaCentralNervousSystem (CNS) is a **thin orchestration layer** that coordinates existing PersonaUser modules to enable multi-domain attention management. It does NOT replace existing code - it orchestrates it.

## Design Principles

1. **Capability-driven, not intelligence-driven**: Select CNS complexity based on model capabilities, not arbitrary intelligence thresholds
2. **Minimal changes**: CNS wraps existing modules (PersonaInbox, PersonaState, PersonaGenome, Scheduler)
3. **Backward compatible**: Existing chat functionality continues working unchanged
4. **Adaptive complexity**: Simple models get deterministic CNS, advanced models get neural CNS with background threads
5. **Fast first**: Start with working heuristic scheduler, add learning later

## Architecture

```
PersonaUser
  └── PersonaCentralNervousSystem (orchestrator)
      ├── ICognitiveScheduler (adapter - which domains to service)
      │   ├── DeterministicCognitiveScheduler (simple models)
      │   ├── HeuristicCognitiveScheduler (mid-tier models)
      │   └── NeuralCognitiveScheduler (frontier models - future)
      ├── PersonaInbox (existing - signal-based queue)
      ├── PersonaState (existing - energy/mood tracking)
      └── PersonaGenome (existing - LoRA adapter management)
```

## Implementation Phases

### Phase 1: Basic CNS Orchestration (No Multi-Domain Yet)

**Goal**: Replace PersonaUser.serviceInbox() with CNS.serviceCycle() - functionally identical

```typescript
// BEFORE: PersonaUser.ts
async serviceInbox(): Promise<void> {
  const cadence = this.personaState.getCadence();
  const hasWork = await this.inbox.waitForWork(cadence);
  if (!hasWork) {
    await this.personaState.rest(cadence);
    return;
  }
  const candidates = await this.inbox.peek(1);
  await this.handleChatMessage(candidates[0]);
}

// AFTER: PersonaUser.ts
async serviceInbox(): Promise<void> {
  await this.cns.serviceCycle();
}

// NEW: PersonaCentralNervousSystem.ts
class PersonaCentralNervousSystem {
  async serviceCycle(): Promise<void> {
    // Delegate to existing modules (same behavior)
    const cadence = this.personaState.getCadence();
    const hasWork = await this.inbox.waitForWork(cadence);

    if (!hasWork) {
      await this.personaState.rest(cadence);
      return;
    }

    // Service chat domain (only domain for now)
    await this.serviceChatDomain();
  }

  private async serviceChatDomain(): Promise<void> {
    const candidates = await this.inbox.peek(1);
    if (candidates.length > 0) {
      await this.personaUser.handleChatMessage(candidates[0]);
    }
  }
}
```

**Test**: All personas should respond exactly as before (no behavior change)

### Phase 2: Add Scheduler Adapter

**Goal**: Integrate HeuristicCognitiveScheduler (doesn't change behavior yet, just structure)

```typescript
class PersonaCentralNervousSystem {
  constructor(
    private scheduler: ICognitiveScheduler,
    private inbox: PersonaInbox,
    private personaState: PersonaState,
    private personaUser: PersonaUser
  ) {}

  async serviceCycle(): Promise<void> {
    const cadence = this.personaState.getCadence();
    const hasWork = await this.inbox.waitForWork(cadence);

    if (!hasWork) {
      await this.personaState.rest(cadence);
      return;
    }

    // NEW: Ask scheduler which domain to service
    const context = this.buildCognitiveContext();
    const shouldServiceChat = await this.scheduler.shouldServiceDomain(
      ActivityDomain.CHAT,
      context
    );

    if (shouldServiceChat) {
      await this.serviceChatDomain();
    }
  }

  private buildCognitiveContext(): CognitiveContext {
    const state = this.personaState.getState();
    return {
      energy: state.energy,
      mood: state.mood,
      activeGames: 0,  // Not implemented yet
      unreadMessages: this.inbox.size(),
      pendingReviews: 0,
      backgroundTasksPending: 0,
      avgResponseTime: 0,
      queueBacklog: this.inbox.size(),
      cpuPressure: 0,
      memoryPressure: 0,
      modelCapabilities: new Set(['text'])
    };
  }
}
```

**Test**: Personas still respond exactly as before (scheduler always returns true for chat)

### Phase 3: Capability-Based CNS Factory

**Goal**: Different personas get different CNS configurations based on capabilities

```typescript
class CNSFactory {
  static create(persona: PersonaUser): PersonaCentralNervousSystem {
    const capabilities = persona.entity.capabilities || {};

    let scheduler: ICognitiveScheduler;
    let enabledDomains: ActivityDomain[];
    let allowBackgroundThreads: boolean;

    // Select CNS complexity based on capabilities
    if (this.hasAdvancedCognition(capabilities)) {
      // Frontier models: Full neural CNS (future)
      scheduler = new HeuristicCognitiveScheduler(); // For now, use heuristic
      enabledDomains = [
        ActivityDomain.CHAT,
        ActivityDomain.CODE_REVIEW,
        ActivityDomain.TRAINING
      ];
      allowBackgroundThreads = true;

    } else if (this.hasModerateReasoning(capabilities)) {
      // Mid-tier: Heuristic with limited domains
      scheduler = new HeuristicCognitiveScheduler();
      enabledDomains = [
        ActivityDomain.CHAT,
        ActivityDomain.TRAINING
      ];
      allowBackgroundThreads = true;

    } else {
      // Simple models: Deterministic, chat only
      scheduler = new DeterministicCognitiveScheduler();
      enabledDomains = [ActivityDomain.CHAT];
      allowBackgroundThreads = false;
    }

    return new PersonaCentralNervousSystem({
      scheduler,
      inbox: persona.inbox,
      personaState: persona.personaState,
      genome: persona.genome,
      personaUser: persona,
      enabledDomains,
      allowBackgroundThreads
    });
  }

  private static hasAdvancedCognition(capabilities: any): boolean {
    return !!(
      capabilities['advanced-reasoning'] ||
      capabilities['meta-cognition'] ||
      capabilities['long-context']
    );
  }

  private static hasModerateReasoning(capabilities: any): boolean {
    return !!(
      capabilities['moderate-reasoning'] ||
      capabilities['pattern-recognition']
    );
  }
}

// Usage in PersonaUser constructor
this.cns = CNSFactory.create(this);
```

**Test**: Personas still work identically (all domains service chat for now)

### Phase 4: Multi-Domain Queue Management (Future)

**Goal**: Actually support multiple domains beyond chat

This requires:
1. Multiple domain-specific queues in PersonaInbox (or separate inboxes per domain)
2. Scheduler attention allocation actually routing to different domains
3. Background thread spawning for internal cognitive processes

**NOT IMPLEMENTED IN PHASE 1-3** - chat continues working, infrastructure ready for expansion

## File Structure

```
system/user/server/modules/
├── central-nervous-system/
│   ├── PersonaCentralNervousSystem.ts    (orchestrator)
│   ├── CNSFactory.ts                     (capability-based factory)
│   └── CNSTypes.ts                       (shared types)
├── cognitive-schedulers/
│   ├── ICognitiveScheduler.ts            (already exists)
│   ├── HeuristicCognitiveScheduler.ts    (already exists)
│   ├── DeterministicCognitiveScheduler.ts (new - simple)
│   └── NeuralCognitiveScheduler.ts       (future)
└── CENTRAL-NERVOUS-SYSTEM-IMPLEMENTATION.md (this doc)
```

## Key Interfaces

### PersonaCentralNervousSystem

```typescript
interface CNSConfig {
  scheduler: ICognitiveScheduler;
  inbox: PersonaInbox;
  personaState: PersonaState;
  genome: PersonaGenome;
  personaUser: PersonaUser;
  enabledDomains: ActivityDomain[];
  allowBackgroundThreads: boolean;
  maxBackgroundThreads?: number;
}

class PersonaCentralNervousSystem {
  constructor(config: CNSConfig);

  /**
   * Single service cycle (replaces PersonaUser.serviceInbox)
   */
  async serviceCycle(): Promise<void>;

  /**
   * Build context for scheduler decisions
   */
  private buildCognitiveContext(): CognitiveContext;

  /**
   * Service chat domain (delegates to PersonaUser)
   */
  private async serviceChatDomain(): Promise<void>;

  /**
   * Spawn background thread (future)
   */
  private spawnBackgroundThread(type: string): void;
}
```

### DeterministicCognitiveScheduler (New)

```typescript
class DeterministicCognitiveScheduler implements ICognitiveScheduler {
  readonly name = 'deterministic';
  readonly requiredCapabilities = new Set<string>();

  async allocateAttention(budget: number, context: CognitiveContext): Promise<AttentionAllocation> {
    // Fixed allocation: 100% to chat if messages exist, else background
    const allocations = new Map<ActivityDomain, number>();

    if (context.unreadMessages > 0) {
      allocations.set(ActivityDomain.CHAT, budget);
    } else {
      allocations.set(ActivityDomain.BACKGROUND, budget);
    }

    return { allocations, totalBudget: budget };
  }

  async shouldServiceDomain(domain: ActivityDomain, context: CognitiveContext): Promise<boolean> {
    // Simple: only service chat
    return domain === ActivityDomain.CHAT;
  }

  getDomainPriority(context: CognitiveContext): ActivityDomain[] {
    return [ActivityDomain.CHAT];
  }

  getNextServiceInterval(context: CognitiveContext): number {
    return 5000; // Fixed 5s cadence
  }

  async updatePolicy(results: Map<ActivityDomain, ServiceResult>): Promise<void> {
    // No-op: deterministic doesn't learn
  }
}
```

## Implementation Strategy

### Step 1: Create Files (30 min)
1. `PersonaCentralNervousSystem.ts` - Basic orchestrator
2. `CNSFactory.ts` - Capability-based factory
3. `DeterministicCognitiveScheduler.ts` - Simple scheduler
4. `CNSTypes.ts` - Shared types

### Step 2: Integrate into PersonaUser (15 min)
1. Add `private cns: PersonaCentralNervousSystem`
2. Initialize in constructor: `this.cns = CNSFactory.create(this)`
3. Replace `serviceInbox()` body with: `await this.cns.serviceCycle()`

### Step 3: Test (30 min)
1. `npm start` - Deploy
2. Send test message
3. Verify all personas respond identically to before
4. Check logs for CNS initialization messages

### Step 4: Document Capabilities (15 min)
Add capability detection to UserEntity or seed script:
```typescript
// scripts/seed-continuum.ts
{
  displayName: 'Claude Code',
  capabilities: { 'advanced-reasoning': true, 'long-context': true }
},
{
  displayName: 'GPT-2 Bot',
  capabilities: { 'template-responses': true }
}
```

## Success Criteria

**Phase 1-3 Success = Zero Behavior Change**
- All personas respond to chat messages exactly as before
- Energy system continues working
- Signal-based wakeup continues working
- Autonomous loop continues working
- Logs show CNS initialization with appropriate scheduler

**Future Phases**: Multi-domain support, background threads, neural schedulers

## Migration Notes

**Backward Compatible**: Current PersonaInbox becomes the CHAT domain. Future domains (games, code review, training) are additive.

**No Breaking Changes**: Existing handleChatMessage() logic unchanged, just called through CNS.

**Incremental**: Can ship Phase 1-3 with zero user-visible changes, then add domains later.

## Key Insight

**CNS is NOT a rewrite** - it's a thin coordinator over existing fast modules. The autonomous loop, signal-based wakeup, energy management, and inbox prioritization all stay exactly as they are. CNS just adds the capability to service multiple domains beyond chat in the future.
