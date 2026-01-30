/**
 * ChannelRegistry - Routes queue items to per-domain ChannelQueues
 *
 * The registry doesn't know item types — it routes by item.routingDomain.
 * Each ActivityDomain has at most one ChannelQueue.
 *
 * Pattern: follows AdapterProviderRegistry (singleton registry, dynamic lookup)
 *
 * Rust equivalent: struct ChannelRegistry { channels: HashMap<ActivityDomain, ChannelQueue> }
 */

import type { ActivityDomain } from '../cognitive-schedulers/ICognitiveScheduler';
import type { BaseQueueItem } from './BaseQueueItem';
import { ChannelQueue } from './ChannelQueue';

export class ChannelRegistry {
  private readonly channels: Map<ActivityDomain, ChannelQueue> = new Map();

  /**
   * Register a channel queue for a domain.
   * One queue per domain. Re-registration replaces.
   */
  register(domain: ActivityDomain, queue: ChannelQueue): void {
    this.channels.set(domain, queue);
  }

  /**
   * Route an item to its channel based on item.routingDomain.
   * Returns undefined if no channel registered for that domain.
   */
  route(item: BaseQueueItem): ChannelQueue | undefined {
    return this.channels.get(item.routingDomain);
  }

  /**
   * Get channel by domain.
   */
  get(domain: ActivityDomain): ChannelQueue | undefined {
    return this.channels.get(domain);
  }

  /**
   * Get all registered channels (for iteration in service cycle).
   */
  all(): ChannelQueue[] {
    return Array.from(this.channels.values());
  }

  /**
   * Get all registered domains.
   */
  domains(): ActivityDomain[] {
    return Array.from(this.channels.keys());
  }

  /**
   * Does ANY channel have urgent work?
   */
  hasUrgentWork(): boolean {
    for (const channel of this.channels.values()) {
      if (channel.hasUrgentWork) return true;
    }
    return false;
  }

  /**
   * Does ANY channel have work?
   */
  hasWork(): boolean {
    for (const channel of this.channels.values()) {
      if (channel.hasWork) return true;
    }
    return false;
  }

  /**
   * Total items across all channels.
   */
  totalSize(): number {
    let total = 0;
    for (const channel of this.channels.values()) {
      total += channel.size;
    }
    return total;
  }

  /**
   * Clear all channels (for testing/reset).
   */
  clearAll(): void {
    for (const channel of this.channels.values()) {
      channel.clear();
    }
  }
}
