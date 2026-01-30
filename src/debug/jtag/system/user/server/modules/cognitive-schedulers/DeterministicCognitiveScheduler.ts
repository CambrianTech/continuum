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
  getNextServiceInterval(_context: CognitiveContext): number {
    return 5000; // Fixed 5 second cadence
  }

  /**
   * Deterministic scheduler: only external interactive + maintenance domains.
   * No internal cognitive domains (dreaming, reflecting, etc.) for simple models.
   * Inherits base class defaults for domain ordering (time-critical first).
   */
  private static readonly ALLOWED_DOMAINS = new Set([
    ActivityDomain.AUDIO,
    ActivityDomain.CHAT,
    ActivityDomain.BACKGROUND,
  ]);

  async shouldServiceDomain(
    domain: ActivityDomain,
    _context: CognitiveContext
  ): Promise<boolean> {
    if (!this.isDomainAllowed(domain)) {
      return false;
    }
    return DeterministicCognitiveScheduler.ALLOWED_DOMAINS.has(domain);
  }

  // getDomainPriority() inherited from BaseCognitiveScheduler
  // Returns ALL domains in priority order — shouldServiceDomain gates which are active

  /**
   * No learning - deterministic doesn't adapt
   */
  async updatePolicy(_results: Map<ActivityDomain, ServiceResult>): Promise<void> {
    // NO-OP: Deterministic schedulers don't learn
    // Fixed rules forever
  }
}
