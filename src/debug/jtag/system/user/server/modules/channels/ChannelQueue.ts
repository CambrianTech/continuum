/**
 * ChannelQueue - Generic queue container that delegates all behavioral decisions to items
 *
 * This class has ZERO item-type-specific logic. It asks items:
 *   - How to sort? → item.compareTo()
 *   - Is this urgent? → item.isUrgent
 *   - Can this be dropped? → item.canBeKicked / item.kickResistance
 *   - Should items merge? → item.shouldConsolidateWith() / item.consolidateWith()
 *
 * One ChannelQueue instance per ActivityDomain. The CNS iterates over channels
 * in scheduler-determined priority order.
 *
 * Rust equivalent: struct ChannelQueue { items: Vec<Box<dyn QueueItem>> }
 */

import { EventEmitter } from 'events';
import type { ActivityDomain } from '../cognitive-schedulers/ICognitiveScheduler';
import { BaseQueueItem } from './BaseQueueItem';

export interface ChannelQueueConfig {
  /** Which activity domain this channel serves */
  domain: ActivityDomain;
  /** Maximum number of items before kicking begins */
  maxSize: number;
  /** Human-readable name for logging */
  name: string;
}

export class ChannelQueue {
  private items: BaseQueueItem[] = [];
  readonly domain: ActivityDomain;
  readonly name: string;
  private readonly maxSize: number;

  /**
   * Signal emitter for notifying upstream (PersonaInbox) that work is available.
   * External code sets this via setSignal() during wiring.
   */
  private signal: EventEmitter | null = null;

  constructor(config: ChannelQueueConfig) {
    this.domain = config.domain;
    this.name = config.name;
    this.maxSize = config.maxSize;
  }

  /**
   * Connect this channel's work signal to the inbox's signal emitter.
   * When items are enqueued, the signal wakes up the service loop.
   */
  setSignal(signal: EventEmitter): void {
    this.signal = signal;
  }

  // ═══════════════════════════════════════════════════════════════════
  // ENQUEUE — Items decide their own kick policy
  // ═══════════════════════════════════════════════════════════════════

  /**
   * Add item to this channel's queue.
   * Sorts by item.compareTo(). If over capacity, kicks items that allow it
   * (lowest kickResistance first).
   */
  enqueue(item: BaseQueueItem): void {
    item.enqueuedAt = Date.now();
    this.items.push(item);
    this.sort();

    // Capacity management: ASK ITEMS if they can be kicked
    while (this.items.length > this.maxSize) {
      const kickable = this.items
        .filter(i => i.canBeKicked)
        .sort((a, b) => a.kickResistance - b.kickResistance);

      if (kickable.length === 0) break; // Nothing can be kicked — queue stays oversized
      this.remove(kickable[0]);
    }

    // Signal that work is available (wakes service loop)
    if (this.signal) {
      this.signal.emit('work-available');
    }
  }

  // ═══════════════════════════════════════════════════════════════════
  // CONSOLIDATION — Items decide their own merge policy
  // ═══════════════════════════════════════════════════════════════════

  /**
   * Consolidate items in this channel.
   * Items decide: shouldConsolidateWith() determines groups, consolidateWith() merges.
   *
   * Called once per CNS service cycle before processing.
   * Voice: no-op (items return false for shouldConsolidateWith).
   * Chat: merges same-room messages into single work unit.
   * Task: groups related tasks by domain+context.
   */
  consolidate(): void {
    if (this.items.length <= 1) return; // Nothing to consolidate

    const result: BaseQueueItem[] = [];
    const consumed = new Set<string>();

    for (const item of this.items) {
      if (consumed.has(item.id)) continue;

      // Find all items that this item says it should consolidate with
      const group = this.items.filter(other =>
        other !== item
        && !consumed.has(other.id)
        && item.shouldConsolidateWith(other)
      );

      if (group.length > 0) {
        // Item decides how to merge
        const consolidated = item.consolidateWith(group);
        result.push(consolidated);
        for (const g of group) {
          consumed.add(g.id);
        }
      } else {
        result.push(item);
      }
      consumed.add(item.id);
    }

    this.items = result;
    this.sort();
  }

  // ═══════════════════════════════════════════════════════════════════
  // ACCESSORS — All delegate to item properties
  // ═══════════════════════════════════════════════════════════════════

  /** Any item in this channel reports itself as urgent */
  get hasUrgentWork(): boolean {
    return this.items.some(i => i.isUrgent);
  }

  /** Channel has any items at all */
  get hasWork(): boolean {
    return this.items.length > 0;
  }

  /** Number of items in this channel */
  get size(): number {
    return this.items.length;
  }

  /** Look at the highest-priority item without removing it */
  peek(): BaseQueueItem | undefined {
    // Re-sort before peeking (aging changes order over time)
    this.sort();
    return this.items[0];
  }

  /** Remove and return the highest-priority item */
  pop(): BaseQueueItem | undefined {
    this.sort();
    return this.items.shift();
  }

  /** Get all urgent items (for batch processing) */
  peekUrgent(): BaseQueueItem[] {
    return this.items.filter(i => i.isUrgent);
  }

  /** Get channel load as a fraction (0.0 = empty, 1.0 = at capacity) */
  get load(): number {
    return this.items.length / this.maxSize;
  }

  /** Clear all items (for testing/reset) */
  clear(): void {
    this.items = [];
  }

  // ═══════════════════════════════════════════════════════════════════
  // INTERNALS
  // ═══════════════════════════════════════════════════════════════════

  private sort(): void {
    this.items.sort((a, b) => a.compareTo(b));
  }

  private remove(item: BaseQueueItem): void {
    const idx = this.items.indexOf(item);
    if (idx !== -1) {
      this.items.splice(idx, 1);
    }
  }
}
