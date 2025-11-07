/**
 * Deterministic Cognitive Scheduler
 *
 * Simplest possible scheduler - NO adaptation, NO learning, NO intelligence.
 * Fixed rules for basic models (GPT-2, tiny models, status bots).
 *
 * Strategy: If chat messages exist, service chat. Otherwise, do nothing.
 * - Instant allocation (no computation)
 * - Predictable (same every time)
 * - Zero overhead
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

export class DeterministicCognitiveScheduler extends BaseCognitiveScheduler {
  readonly name = 'deterministic';
  readonly requiredCapabilities = new Set<string>(); // Works with ANY model (even GPT-2)

  async initialize(personaId: UUID, personaName: string): Promise<void> {
    await super.initialize(personaId, personaName);
    console.log(`🧠 ${personaName}: Initialized DeterministicCognitiveScheduler (fixed rules, no adaptation)`);
  }

  /**
   * Fixed attention allocation - no intelligence needed
   */
  async allocateAttention(
    budget: number,
    context: CognitiveContext
  ): Promise<AttentionAllocation> {
    const allocations = new Map<ActivityDomain, number>();

    // RULE 1: If chat messages exist, allocate 100% to chat
    if (context.unreadMessages > 0) {
      allocations.set(ActivityDomain.CHAT, budget);
    } else {
      // RULE 2: If idle, allocate to background (maintenance only)
      allocations.set(ActivityDomain.BACKGROUND, budget);
    }

    return { allocations, totalBudget: budget };
  }

  /**
   * Fixed service interval - no adaptation
   */
  getNextServiceInterval(context: CognitiveContext): number {
    return 5000; // Fixed 5 second cadence
  }

  /**
   * Simple domain check - only service chat
   */
  async shouldServiceDomain(
    domain: ActivityDomain,
    context: CognitiveContext
  ): Promise<boolean> {
    // Check system overrides first
    if (!this.isDomainAllowed(domain)) {
      return false;
    }

    // Only service chat and background
    return domain === ActivityDomain.CHAT || domain === ActivityDomain.BACKGROUND;
  }

  /**
   * Fixed priority order
   */
  getDomainPriority(context: CognitiveContext): ActivityDomain[] {
    return [
      ActivityDomain.CHAT,        // Only priority
      ActivityDomain.BACKGROUND   // Maintenance if nothing else
    ];
  }

  /**
   * No learning - deterministic doesn't adapt
   */
  async updatePolicy(results: Map<ActivityDomain, ServiceResult>): Promise<void> {
    // NO-OP: Deterministic schedulers don't learn
    // Fixed rules forever
  }
}
