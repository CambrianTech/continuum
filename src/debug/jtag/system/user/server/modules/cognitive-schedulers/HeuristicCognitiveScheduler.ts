/**
 * Heuristic Cognitive Scheduler
 *
 * Simple rule-based attention allocation - NO machine learning required.
 * This is the default scheduler that works for all models.
 *
 * Strategy: Fixed rules based on context (energy, queue backlogs, activity)
 * - Fast (instant allocation)
 * - Predictable (same context = same allocation)
 * - Safe (bounded behavior)
 * - No training required
 */

import {
  BaseCognitiveScheduler,
  ActivityDomain,
  type CognitiveContext,
  type AttentionAllocation,
  type ServiceResult
} from './ICognitiveScheduler';
import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';

export class HeuristicCognitiveScheduler extends BaseCognitiveScheduler {
  readonly name = 'heuristic';
  readonly requiredCapabilities = new Set<string>(); // Works with ANY model

  async initialize(personaId: UUID, personaName: string): Promise<void> {
    await super.initialize(personaId, personaName);
    console.log(`🧠 ${personaName}: Initialized HeuristicCognitiveScheduler (rule-based, no ML)`);
  }

  /**
   * Allocate attention budget using simple heuristic rules
   */
  async allocateAttention(
    budget: number,
    context: CognitiveContext
  ): Promise<AttentionAllocation> {
    const allocations = new Map<ActivityDomain, number>();

    // RULE 1: If in realtime game, prioritize game (80%) but still multitask
    if (context.activeGames > 0) {
      allocations.set(ActivityDomain.REALTIME_GAME, budget * 0.80);
      allocations.set(ActivityDomain.CHAT, budget * 0.08);
      allocations.set(ActivityDomain.SIMULATING, budget * 0.05);  // Think during game
      allocations.set(ActivityDomain.TRAINING, budget * 0.03);    // Train in background
      allocations.set(ActivityDomain.BACKGROUND, budget * 0.02);
      allocations.set(ActivityDomain.DREAMING, budget * 0.02);
      return { allocations, totalBudget: budget };
    }

    // RULE 2: If chat backlog high (>10 messages), prioritize chat
    if (context.unreadMessages > 10) {
      allocations.set(ActivityDomain.CHAT, budget * 0.60);
      allocations.set(ActivityDomain.SIMULATING, budget * 0.15);  // Think before responding
      allocations.set(ActivityDomain.TRAINING, budget * 0.10);
      allocations.set(ActivityDomain.REFLECTING, budget * 0.05);  // Analyze why backlog formed
      allocations.set(ActivityDomain.CODE_REVIEW, budget * 0.05);
      allocations.set(ActivityDomain.BACKGROUND, budget * 0.05);
      return { allocations, totalBudget: budget };
    }

    // RULE 3: If code review backlog high, prioritize deep work
    if (context.pendingReviews > 5) {
      allocations.set(ActivityDomain.CODE_REVIEW, budget * 0.50);
      allocations.set(ActivityDomain.SIMULATING, budget * 0.20);  // Plan review strategy
      allocations.set(ActivityDomain.CHAT, budget * 0.15);
      allocations.set(ActivityDomain.TRAINING, budget * 0.10);
      allocations.set(ActivityDomain.BACKGROUND, budget * 0.05);
      return { allocations, totalBudget: budget };
    }

    // RULE 4: If idle (no immediate work), focus on internal cognitive processes
    if (context.unreadMessages === 0 && context.queueBacklog < 3) {
      allocations.set(ActivityDomain.DREAMING, budget * 0.30);     // Consolidate memories
      allocations.set(ActivityDomain.TRAINING, budget * 0.25);     // Continuous learning
      allocations.set(ActivityDomain.REFLECTING, budget * 0.20);   // Self-analysis
      allocations.set(ActivityDomain.SIMULATING, budget * 0.10);   // Explore possibilities
      allocations.set(ActivityDomain.BACKGROUND, budget * 0.10);
      allocations.set(ActivityDomain.CHAT, budget * 0.05);         // Stay responsive
      return { allocations, totalBudget: budget };
    }

    // RULE 5: Default balanced allocation (normal operation)
    allocations.set(ActivityDomain.CHAT, budget * 0.40);
    allocations.set(ActivityDomain.SIMULATING, budget * 0.15);     // Always think before acting
    allocations.set(ActivityDomain.TRAINING, budget * 0.15);       // Continuous learning
    allocations.set(ActivityDomain.CODE_REVIEW, budget * 0.10);
    allocations.set(ActivityDomain.DREAMING, budget * 0.08);
    allocations.set(ActivityDomain.REFLECTING, budget * 0.07);
    allocations.set(ActivityDomain.BACKGROUND, budget * 0.05);

    return { allocations, totalBudget: budget };
  }

  /**
   * Determine next service interval based on context
   * Adaptive cadence: faster when energized, slower when tired
   */
  getNextServiceInterval(context: CognitiveContext): number {
    // If in realtime game, service every 16ms (60 FPS)
    if (context.activeGames > 0) {
      return 16;
    }

    // If high chat backlog, service frequently
    if (context.unreadMessages > 10) {
      return 1000; // 1 second
    }

    // Adaptive based on energy (same as current PersonaState logic)
    if (context.energy > 0.7) {
      return 3000;  // 3 seconds when energized
    }

    if (context.energy > 0.3) {
      return 5000;  // 5 seconds normal
    }

    if (context.energy > 0.1) {
      return 7000;  // 7 seconds when tired
    }

    // Very low energy - rest longer
    return 10000;  // 10 seconds when exhausted
  }

  /**
   * Should we service a specific domain right now?
   */
  async shouldServiceDomain(
    domain: ActivityDomain,
    context: CognitiveContext
  ): Promise<boolean> {
    // Check system overrides first (authoritative control)
    if (!this.isDomainAllowed(domain)) {
      return false;
    }

    // Energy gating - only honor critical contracts when exhausted
    if (context.energy < 0.2) {
      // When exhausted, only honor realtime contracts
      return domain === ActivityDomain.REALTIME_GAME;
    }

    if (context.energy < 0.4) {
      // When tired, defer internal cognitive processes
      return domain !== ActivityDomain.DREAMING && domain !== ActivityDomain.REFLECTING;
    }

    // Under system pressure, defer non-critical domains
    if (context.cpuPressure > 0.8 || context.memoryPressure > 0.9) {
      const criticalDomains = [
        ActivityDomain.REALTIME_GAME,
        ActivityDomain.CHAT
      ];
      return criticalDomains.includes(domain);
    }

    // Default: service all domains
    return true;
  }

  /**
   * Get priority order for domains
   * Higher priority = serviced first when resources limited
   */
  getDomainPriority(context: CognitiveContext): ActivityDomain[] {
    // Fixed priority order (heuristic - doesn't change)
    return [
      ActivityDomain.REALTIME_GAME,  // Highest priority (contractual obligation)
      ActivityDomain.SIMULATING,     // Think before acting
      ActivityDomain.CHAT,           // Social presence
      ActivityDomain.CODE_REVIEW,    // Deep work
      ActivityDomain.TRAINING,       // Continuous learning
      ActivityDomain.REFLECTING,     // Self-improvement
      ActivityDomain.DREAMING,       // Memory consolidation
      ActivityDomain.BACKGROUND      // Lowest priority (maintenance)
    ];
  }

  /**
   * Update policy (no-op for heuristic scheduler)
   * Heuristic schedulers don't learn - they use fixed rules
   */
  async updatePolicy(results: Map<ActivityDomain, ServiceResult>): Promise<void> {
    // NO-OP: Heuristic schedulers don't learn from experience
    // This method exists for interface compatibility
    // Advanced schedulers (RL, GAN, etc.) will implement learning here
  }

  /**
   * Log diagnostic info
   */
  private log(message: string): void {
    console.log(`🧠 ${this.personaName} [Heuristic]: ${message}`);
  }
}
