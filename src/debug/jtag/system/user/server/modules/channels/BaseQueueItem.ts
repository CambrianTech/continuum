/**
 * BaseQueueItem - Abstract base class for all inbox queue items
 *
 * Philosophy: Items control their own destiny. A VoiceQueueItem KNOWS it's urgent.
 * A ChatQueueItem KNOWS it consolidates per-room. Only the item class can really
 * know what to do with itself.
 *
 * The queue/channel is a generic container that delegates all behavioral decisions
 * to item polymorphism via this abstract class.
 *
 * Pattern: Template Method (protected hooks for aging, urgency, consolidation, kicking)
 * Subclasses override only what differs. Defaults handle the common case.
 */

import type { UUID } from '../../../../core/types/CrossPlatformUUID';
import { ActivityDomain } from '../cognitive-schedulers/ICognitiveScheduler';

// Re-export for subclass convenience
export { ActivityDomain };

/**
 * Construction parameters for BaseQueueItem.
 * Subclasses extend this with their own fields.
 */
export interface BaseQueueItemParams {
  id: UUID;
  timestamp: number;
  enqueuedAt?: number;
}

/**
 * Abstract base class for all queue items in the CNS channel system.
 *
 * Provides sensible defaults for RTOS aging, kick resistance, consolidation,
 * urgency, and sorting. Subclasses override only what differs from the default.
 *
 * Abstract (MUST implement):
 *   - itemType: string discriminator
 *   - domain: ActivityDomain for routing
 *   - basePriority: number (0.0-1.0)
 *
 * Protected hooks (CAN override):
 *   - agingBoostMs: time to reach max aging (default: 30s)
 *   - maxAgingBoost: maximum priority boost from aging (default: 0.5)
 *
 * Public behavioral (CAN override):
 *   - isUrgent: bypass scheduler (default: false)
 *   - canBeKicked: droppable under pressure (default: true)
 *   - kickResistance: lower = kicked first (default: effectivePriority)
 *   - shouldConsolidateWith(other): mergeable (default: false)
 *   - consolidateWith(others): merge logic (default: return self)
 *   - compareTo(other): sort order (default: effectivePriority desc)
 *   - routingDomain: which channel to route to (default: this.domain)
 */
export abstract class BaseQueueItem {
  // === Identity ===
  readonly id: UUID;
  readonly timestamp: number;
  enqueuedAt?: number;

  /** Discriminator string for runtime type identification (e.g., 'voice', 'chat', 'task') */
  abstract readonly itemType: string;

  /** Which CNS activity domain this item belongs to */
  abstract readonly domain: ActivityDomain;

  constructor(params: BaseQueueItemParams) {
    this.id = params.id;
    this.timestamp = params.timestamp;
    this.enqueuedAt = params.enqueuedAt;
  }

  // ═══════════════════════════════════════════════════════════════════
  // PRIORITY (Template Method Pattern)
  // ═══════════════════════════════════════════════════════════════════

  /** Base priority for this item (0.0-1.0). Subclasses define their own scale. */
  abstract get basePriority(): number;

  /**
   * Time in milliseconds for aging boost to reach maximum.
   * Override to change aging speed. Set very high to effectively disable.
   * Default: 30,000ms (30 seconds)
   */
  protected get agingBoostMs(): number { return 30_000; }

  /**
   * Maximum priority boost from queue aging.
   * Override to 0 to disable aging entirely (e.g., voice).
   * Default: 0.5
   */
  protected get maxAgingBoost(): number { return 0.5; }

  /**
   * Effective priority = basePriority + aging boost.
   * RTOS-style: items waiting longer get higher effective priority.
   * This prevents starvation - every item eventually gets serviced.
   *
   * Subclasses rarely override this; instead override agingBoostMs/maxAgingBoost.
   */
  get effectivePriority(): number {
    const waitMs = Date.now() - (this.enqueuedAt ?? this.timestamp);
    const boost = Math.min(
      this.maxAgingBoost,
      (waitMs / this.agingBoostMs) * this.maxAgingBoost
    );
    return Math.min(1.0, this.basePriority + boost);
  }

  // ═══════════════════════════════════════════════════════════════════
  // URGENCY
  // ═══════════════════════════════════════════════════════════════════

  /**
   * Is this item time-critical? Urgent items bypass the cognitive scheduler.
   * Default: false. Voice overrides to true. Chat overrides for mentions.
   */
  get isUrgent(): boolean { return false; }

  // ═══════════════════════════════════════════════════════════════════
  // CONSOLIDATION
  // ═══════════════════════════════════════════════════════════════════

  /**
   * Can this item be merged with another item in the same channel?
   * Items decide their own consolidation rules.
   *
   * Default: false (no consolidation).
   * Chat overrides to consolidate same-room messages.
   * Task overrides to consolidate related tasks.
   */
  shouldConsolidateWith(_other: BaseQueueItem): boolean { return false; }

  /**
   * Merge this item with compatible items. Returns the consolidated item.
   * Called only when shouldConsolidateWith() returned true for the group.
   *
   * Default: return self (no-op).
   * Chat overrides to merge messages into context + latest trigger.
   */
  consolidateWith(_others: BaseQueueItem[]): BaseQueueItem { return this; }

  // ═══════════════════════════════════════════════════════════════════
  // QUEUE MANAGEMENT (KICKING)
  // ═══════════════════════════════════════════════════════════════════

  /**
   * Can this item be dropped when the queue is at capacity?
   * Default: true. Voice overrides to false (never drop voice).
   * Task overrides to protect in-progress tasks.
   */
  get canBeKicked(): boolean { return true; }

  /**
   * Resistance to being kicked. Lower values are kicked first.
   * Default: effectivePriority (low priority items kicked first).
   * Voice overrides to Infinity (never kicked).
   */
  get kickResistance(): number { return this.effectivePriority; }

  // ═══════════════════════════════════════════════════════════════════
  // SORTING
  // ═══════════════════════════════════════════════════════════════════

  /**
   * Compare for queue ordering. Higher effectivePriority = serviced first.
   * Returns negative if this should come BEFORE other (higher priority).
   */
  compareTo(other: BaseQueueItem): number {
    return other.effectivePriority - this.effectivePriority;
  }

  // ═══════════════════════════════════════════════════════════════════
  // ROUTING
  // ═══════════════════════════════════════════════════════════════════

  /**
   * Which channel should this item be routed to?
   * Default: this.domain. Override for items that belong to a different
   * channel than their logical domain.
   */
  get routingDomain(): ActivityDomain { return this.domain; }
}
