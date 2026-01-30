/**
 * PersonaInbox - Traffic-managed work queue for autonomous personas
 *
 * Philosophy: "scheduling and self prioritization, but not neglecting so badly, like a traffic problem"
 *
 * Traffic Management Properties:
 * - Priority queue with RTOS-style aging (no item starved — like a traffic intersection)
 * - Effective priority = base priority + aging boost (longer wait → higher priority)
 * - Graceful degradation (drop low effective priority when overloaded)
 * - Load awareness (personas see queue depth)
 * - Signal-based wakeup (instant response, no polling)
 * - Non-blocking operations (autonomous checking)
 *
 * Handles unified queue of messages and tasks with type-safe discrimination
 */

import { EventEmitter } from 'events';
import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { QueueItem, InboxMessage, InboxTask } from './QueueItemTypes';
import { isInboxMessage, isInboxTask, toChannelItem } from './QueueItemTypes';
import { getChatCoordinator } from '../../../coordination/server/ChatCoordinationStream';
import type { ChannelRegistry } from './channels/ChannelRegistry';

// Re-export types for backward compatibility and external use
export type { QueueItem, InboxMessage, InboxTask } from './QueueItemTypes';

/**
 * Inbox configuration
 */
export interface InboxConfig {
  maxSize: number;        // Maximum queue depth
  enableLogging: boolean;
}

export const DEFAULT_INBOX_CONFIG: InboxConfig = {
  maxSize: 1000,
  enableLogging: true
};

/**
 * RTOS-style aging constants
 *
 * Items that wait longer in the queue get their effective priority boosted.
 * This prevents starvation: no item waits forever regardless of base priority.
 * Like a traffic intersection — every direction eventually gets a green light.
 */
const AGING_RATE_MS = 30_000;       // Time for aging boost to reach maximum (30 seconds)
const MAX_AGING_BOOST = 0.5;        // Maximum priority boost from aging (0.5)

/**
 * Compute effective priority with RTOS-style aging
 *
 * effectivePriority = basePriority + agingBoost
 * agingBoost grows linearly from 0 to MAX_AGING_BOOST over AGING_RATE_MS
 *
 * Examples:
 * - Voice item (base 0.5), waited 0s:  effective = 0.50
 * - Voice item (base 0.5), waited 15s: effective = 0.75
 * - Voice item (base 0.5), waited 30s: effective = 1.00
 * - Fresh text  (base 0.65), waited 0s: effective = 0.65
 * - After ~12s, the voice item overtakes the fresh text item
 */
export function getEffectivePriority(item: QueueItem): number {
  const enqueuedAt = item.enqueuedAt ?? item.timestamp;
  const waitMs = Date.now() - enqueuedAt;
  const agingBoost = Math.min(MAX_AGING_BOOST, (waitMs / AGING_RATE_MS) * MAX_AGING_BOOST);
  return Math.min(1.0, item.priority + agingBoost);
}

/**
 * PersonaInbox: Priority queue for autonomous work processing
 * Handles both messages and tasks in unified queue
 * Uses EventEmitter for instant signal-based wakeup (no polling)
 */
export class PersonaInbox {
  private readonly config: InboxConfig;
  private queue: QueueItem[] = [];
  private readonly personaId: UUID;
  private readonly personaName: string;
  private readonly signal: EventEmitter;

  // Multi-channel routing: items converted to BaseQueueItem subclasses and routed to channels
  private channelRegistry: ChannelRegistry | null = null;

  // Load-aware deduplication (feedback-driven)
  private queueStatsProvider: (() => { queueSize: number; activeRequests: number; maxConcurrent: number; load: number }) | null = null;
  private readonly DEDUP_WINDOW_MS = 3000; // Look back 3s for duplicates

  constructor(personaId: UUID, personaName: string, config: Partial<InboxConfig> = {}) {
    this.personaId = personaId;
    this.personaName = personaName;
    this.config = { ...DEFAULT_INBOX_CONFIG, ...config };
    this.signal = new EventEmitter();

    this.log(`📬 Inbox initialized (maxSize=${this.config.maxSize}, signal-based wakeup)`);
  }

  /**
   * Set queue stats provider for load-aware deduplication
   * Called by PersonaUser during initialization
   */
  setQueueStatsProvider(provider: () => { queueSize: number; activeRequests: number; maxConcurrent: number; load: number }): void {
    this.queueStatsProvider = provider;
  }

  /**
   * Inject channel registry for multi-channel routing.
   * Items are converted to BaseQueueItem subclasses and routed to per-domain channels.
   * Each channel's signal is wired to this inbox's signal emitter for unified wakeup.
   */
  setChannelRegistry(registry: ChannelRegistry): void {
    this.channelRegistry = registry;

    // Wire each channel's signal to this inbox's signal emitter
    // When items are added to channels, the service loop wakes up
    for (const channel of registry.all()) {
      channel.setSignal(this.signal);
    }

    this.log(`🔗 Channel registry connected (${registry.domains().length} channels, signals wired)`);
  }

  /**
   * Add item to inbox (non-blocking)
   * Accepts both messages and tasks
   * Traffic management: Drop lowest priority when full
   * Load-aware deduplication: Skip redundant room messages under high load
   * SIGNAL-BASED: Instantly wakes waiting serviceInbox (no polling delay)
   */
  async enqueue(item: QueueItem): Promise<boolean> {
    // CRITICAL: Always enqueue to queue so PersonaUser can see it
    // (Previous bug: held messages in separate map, breaking autonomous loop)

    // Smart deduplication for messages: skip if recent message from same room already queued
    // BUT ONLY under high load (feedback-driven optimization)
    if (isInboxMessage(item)) {
      const shouldDeduplicate = this.shouldDeduplicateMessage(item);
      if (shouldDeduplicate) {
        this.log(`🔄 Skipped duplicate: room=${item.roomId?.slice(0, 8)} (recent message already queued)`);
        return true; // Silently skip, don't add to queue
      }
    }

    // MULTI-CHANNEL PATH: Route to per-domain channel if registry available
    if (this.channelRegistry) {
      const channelItem = toChannelItem(item);
      const channel = this.channelRegistry.route(channelItem);
      if (channel) {
        // Channel's enqueue stamps enqueuedAt, sorts, handles kicks, and emits signal
        channel.enqueue(channelItem);

        // Log with type-specific details
        if (isInboxMessage(item)) {
          const senderIdPreview = item.senderId?.slice(0, 8) ?? '[no-senderId]';
          this.log(`📬 Routed ${channelItem.itemType} → ${channel.name}: ${senderIdPreview} (priority=${item.priority.toFixed(2)}, channelSize=${channel.size})`);
          if (item.sourceModality === 'voice') {
            console.log(`🎙️🔊 VOICE-DEBUG [Inbox] Routed VOICE → ${channel.name}: voiceSessionId=${item.voiceSessionId?.slice(0, 8) || 'undefined'}`);
          }
        } else if (isInboxTask(item)) {
          this.log(`📬 Routed task → ${channel.name}: ${item.taskType} (priority=${item.priority.toFixed(2)}, channelSize=${channel.size})`);
        }

        return true; // Item routed to channel — not added to legacy queue
      }
      // No channel registered for this domain — fall through to legacy queue
    }

    // LEGACY PATH: Flat priority queue (for items without a channel)

    // Check if over capacity
    if (this.queue.length >= this.config.maxSize) {
      // Sort by effective priority (highest first) — aged items survive shedding
      this.queue.sort((a, b) => getEffectivePriority(b) - getEffectivePriority(a));

      // Drop lowest effective priority item (traffic shed)
      const dropped = this.queue.pop();
      this.log(`⚠️  Queue full! Dropped low-priority ${dropped?.type} (priority=${dropped?.priority.toFixed(2)}, effective=${dropped ? getEffectivePriority(dropped).toFixed(2) : '?'})`);
    }

    // Stamp enqueue time for RTOS aging
    item.enqueuedAt = Date.now();

    // Add item
    this.queue.push(item);

    // Sort by effective priority (base + aging boost)
    this.queue.sort((a, b) => getEffectivePriority(b) - getEffectivePriority(a));

    // Log with type-specific details
    if (isInboxMessage(item)) {
      // Defensive: handle undefined senderId
      const senderIdPreview = item.senderId?.slice(0, 8) ?? '[no-senderId]';
      this.log(`📬 Enqueued message: ${senderIdPreview} → priority=${item.priority.toFixed(2)} (queue=${this.queue.length})`);
      // VOICE DEBUG: Log voice metadata at enqueue time
      if (item.sourceModality === 'voice') {
        console.log(`🎙️🔊 VOICE-DEBUG [Inbox] Enqueued VOICE message: sourceModality=${item.sourceModality}, voiceSessionId=${item.voiceSessionId?.slice(0, 8) || 'undefined'}`);
      }
    } else if (isInboxTask(item)) {
      this.log(`📬 Enqueued task: ${item.taskType} → priority=${item.priority.toFixed(2)} (queue=${this.queue.length})`);
    }

    // CRITICAL: Signal waiting serviceInbox (instant wakeup, no polling)
    this.signal.emit('work-available');

    return true;
  }

  /**
   * Smart deduplication: Skip message if recent message from same room already queued
   * ONLY active under high adapter load (feedback-driven)
   *
   * Philosophy: When adapter queue is backed up, consolidate inbox events.
   * PersonaUser will still see ALL messages via RAG (fetches from DB).
   * This just prevents redundant "new message in room X" work items.
   */
  private shouldDeduplicateMessage(message: InboxMessage): boolean {
    // Only deduplicate under high load (>60% adapter queue saturation)
    if (this.queueStatsProvider) {
      const stats = this.queueStatsProvider();
      if (stats.load < 0.6) {
        return false; // Low load - process everything normally
      }
    } else {
      return false; // No stats provider - don't deduplicate
    }

    // Check for recent message from same room in queue
    const now = Date.now();
    const cutoff = now - this.DEDUP_WINDOW_MS;

    for (const item of this.queue) {
      if (isInboxMessage(item) &&
          item.roomId === message.roomId &&
          item.timestamp >= cutoff) {
        // Found recent message from same room - this is a duplicate
        return true;
      }
    }

    return false; // No recent room message found - not a duplicate
  }

  /**
   * Check inbox without removing (non-blocking)
   * Returns top N items by effective priority (base + aging boost)
   *
   * RTOS behavior: re-sorts by effective priority every peek,
   * so items that have been waiting longer bubble up automatically.
   */
  async peek(limit: number = 10): Promise<QueueItem[]> {
    // Re-sort by effective priority (aging changes order over time)
    this.queue.sort((a, b) => getEffectivePriority(b) - getEffectivePriority(a));

    const items = this.queue.slice(0, limit);
    // VOICE DEBUG: Log voice metadata when peeking
    for (const item of items) {
      if (isInboxMessage(item) && item.sourceModality === 'voice') {
        const eff = getEffectivePriority(item);
        console.log(`🎙️🔊 VOICE-DEBUG [Inbox.peek] VOICE message in queue: sourceModality=${item.sourceModality}, basePriority=${item.priority.toFixed(2)}, effectivePriority=${eff.toFixed(2)}, voiceSessionId=${item.voiceSessionId?.slice(0, 8) || 'undefined'}`);
      }
    }
    return items;
  }

  /**
   * Remove and return next item (blocking with timeout)
   * Returns null if no item within timeout
   *
   * RTOS behavior: re-sorts by effective priority before popping,
   * ensuring aged items get served before fresh higher-base-priority items.
   */
  async pop(timeoutMs: number = 5000): Promise<QueueItem | null> {
    // Immediate check
    if (this.queue.length > 0) {
      // Re-sort by effective priority (aging may have changed order)
      this.queue.sort((a, b) => getEffectivePriority(b) - getEffectivePriority(a));
      const item = this.queue.shift()!;
      if (isInboxMessage(item)) {
        // Defensive: handle undefined id
        const idPreview = item.id?.slice(0, 8) ?? '[no-id]';
        this.log(`📭 Popped message: ${idPreview} (queue=${this.queue.length})`);
      } else if (isInboxTask(item)) {
        // Defensive: handle undefined taskId
        const taskIdPreview = item.taskId?.slice(0, 8) ?? '[no-taskId]';
        this.log(`📭 Popped task: ${taskIdPreview} (queue=${this.queue.length})`);
      }
      return item;
    }

    // Wait for item
    return new Promise<QueueItem | null>((resolve) => {
      const startTime = Date.now();

      const checkInterval = setInterval(() => {
        if (this.queue.length > 0) {
          clearInterval(checkInterval);
          const item = this.queue.shift()!;
          if (isInboxMessage(item)) {
            // Defensive: handle undefined id
            const idPreview = item.id?.slice(0, 8) ?? '[no-id]';
            this.log(`📭 Popped message (after wait): ${idPreview} (queue=${this.queue.length})`);
          } else if (isInboxTask(item)) {
            // Defensive: handle undefined taskId
            const taskIdPreview = item.taskId?.slice(0, 8) ?? '[no-taskId]';
            this.log(`📭 Popped task (after wait): ${taskIdPreview} (queue=${this.queue.length})`);
          }
          resolve(item);
        } else if (Date.now() - startTime > timeoutMs) {
          clearInterval(checkInterval);
          resolve(null); // Timeout
        }
      }, 100); // Check every 100ms
    });
  }

  /**
   * Get inbox size (for load awareness)
   */
  getSize(): number {
    return this.queue.length;
  }

  /**
   * Get inbox load as percentage (0.0 = empty, 1.0 = full)
   */
  getLoad(): number {
    return this.queue.length / this.config.maxSize;
  }

  /**
   * Check if inbox is overloaded (>75% full)
   */
  isOverloaded(): boolean {
    return this.getLoad() > 0.75;
  }

  /**
   * Wait for work to become available (signal-based, not polling)
   * Returns immediately if work already available
   * Otherwise blocks until signal received or timeout
   */
  async waitForWork(timeoutMs: number = 30000): Promise<boolean> {
    // Immediate check - if work available in legacy queue OR channels, return instantly
    if (this.queue.length > 0) {
      return true;
    }
    if (this.channelRegistry?.hasWork()) {
      return true;
    }

    // Wait for signal with race condition protection
    return new Promise<boolean>((resolve) => {
      let settled = false;

      const workHandler = (): void => {
        if (settled) return; // Already resolved by timeout
        settled = true;
        clearTimeout(timer);
        this.signal.removeListener('work-available', workHandler);
        resolve(true); // Work available
      };

      const timer = setTimeout(() => {
        if (settled) return; // Already resolved by work handler
        settled = true;
        this.signal.removeListener('work-available', workHandler);
        resolve(false); // Timeout
      }, timeoutMs);

      // Use on() instead of once() to have full control over cleanup
      this.signal.on('work-available', workHandler);
    });
  }

  /**
   * Clear inbox (for testing/reset)
   */
  clear(): void {
    const cleared = this.queue.length;
    this.queue = [];
    this.log(`🗑️  Cleared ${cleared} items`);
  }

  /**
   * Get inbox stats (for diagnostics)
   */
  getStats(): {
    size: number;
    load: number;
    overloaded: boolean;
    highestPriority: number | null;
    lowestPriority: number | null;
    highestEffectivePriority: number | null;
    oldestWaitMs: number | null;
  } {
    const highestPriority = this.queue.length > 0 ? this.queue[0].priority : null;
    const lowestPriority = this.queue.length > 0 ? this.queue[this.queue.length - 1].priority : null;
    const highestEffective = this.queue.length > 0
      ? Math.max(...this.queue.map(getEffectivePriority))
      : null;
    const now = Date.now();
    const oldestWait = this.queue.length > 0
      ? Math.max(...this.queue.map(item => now - (item.enqueuedAt ?? item.timestamp)))
      : null;

    return {
      size: this.getSize(),
      load: this.getLoad(),
      overloaded: this.isOverloaded(),
      highestPriority,
      lowestPriority,
      highestEffectivePriority: highestEffective,
      oldestWaitMs: oldestWait
    };
  }

  /**
   * Logging helper
   * Can be overridden by injecting a logger function
   */
  private logFn: ((message: string) => void) | null = null;

  /**
   * Set custom logger (optional dependency injection)
   */
  setLogger(logger: (message: string) => void): void {
    this.logFn = logger;
  }

  private log(message: string): void {
    if (!this.config.enableLogging) return;
    const formattedMessage = `[${this.personaName}:Inbox] ${message}`;

    if (this.logFn) {
      this.logFn(formattedMessage);
    } else {
      // Fallback to console if no logger injected
      console.log(formattedMessage);
    }
  }
}

/**
 * Calculate message priority for persona
 *
 * Priority factors:
 * - Mentioned by name: +0.4 (NEVER neglect)
 * - Recent message: +0.2 (fresher = more relevant)
 * - Active conversation: +0.1 (persona recently active in room)
 * - Relevant expertise: +0.1 (matches persona's domain)
 * - Hot conversation (temp ≥ 0.7): +0.15 (PHASE 3BIS)
 * - Cold conversation (temp ≤ 0.3): -0.1 (PHASE 3BIS)
 *
 * Base: 0.2 (all messages have baseline relevance)
 */
export function calculateMessagePriority(
  message: { content: string; timestamp: number; roomId: UUID },
  persona: { displayName: string; id: UUID; recentRooms?: UUID[]; expertise?: string[] }
): number {
  let priority = 0.2; // Base priority

  // HIGH PRIORITY: Mentioned by name (NEVER neglect)
  // Check both "Groq Lightning" and "groq-lightning" formats
  const nameWithSpaces = `@${persona.displayName.toLowerCase()}`;
  const nameWithHyphens = `@${persona.displayName.toLowerCase().replace(/\s+/g, '-')}`;
  const contentLower = message.content.toLowerCase();

  if (contentLower.includes(nameWithSpaces) || contentLower.includes(nameWithHyphens)) {
    priority += 0.4;
  }

  // MEDIUM PRIORITY: Recent message (fresher = more relevant)
  const ageMs = Date.now() - message.timestamp;
  if (ageMs < 60000) { // Last minute
    priority += 0.2;
  } else if (ageMs < 300000) { // Last 5 minutes
    priority += 0.1;
  }

  // MEDIUM PRIORITY: Active conversation (persona recently active in room)
  if (persona.recentRooms && persona.recentRooms.includes(message.roomId)) {
    priority += 0.1;
  }

  // LOW PRIORITY: Relevant expertise (matches persona's domain)
  if (persona.expertise && persona.expertise.length > 0) {
    const contentLower = message.content.toLowerCase();
    const hasRelevantKeyword = persona.expertise.some(keyword =>
      contentLower.includes(keyword.toLowerCase())
    );
    if (hasRelevantKeyword) {
      priority += 0.1;
    }
  }

  // PHASE 3BIS: Temperature-based priority adjustment (activity ambient state)
  // Hot conversations = more responsive, Cold conversations = less urgent
  const temperature = getChatCoordinator().getTemperature(message.roomId);

  if (temperature >= 0.7) {
    // Hot conversation - be more responsive
    priority += 0.15;
  } else if (temperature <= 0.3) {
    // Cold conversation - less urgent (but still respond to mentions)
    priority -= 0.1;
  }
  // Neutral temperature (0.3-0.7) - no adjustment

  return Math.min(1.0, priority); // Cap at 1.0
}
