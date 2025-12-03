/**
 * PersonaInbox - Traffic-managed work queue for autonomous personas
 *
 * Philosophy: "scheduling and self prioritization, but not neglecting so badly, like a traffic problem"
 *
 * Traffic Management Properties:
 * - Priority-based queue (high priority never starved)
 * - Graceful degradation (drop low priority when overloaded)
 * - Load awareness (personas see queue depth)
 * - Signal-based wakeup (instant response, no polling)
 * - Non-blocking operations (autonomous checking)
 *
 * Handles unified queue of messages and tasks with type-safe discrimination
 */

import { EventEmitter } from 'events';
import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { QueueItem, InboxMessage, InboxTask } from './QueueItemTypes';
import { isInboxMessage, isInboxTask } from './QueueItemTypes';
import { getChatCoordinator } from '../../../coordination/server/ChatCoordinationStream';

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

  constructor(personaId: UUID, personaName: string, config: Partial<InboxConfig> = {}) {
    this.personaId = personaId;
    this.personaName = personaName;
    this.config = { ...DEFAULT_INBOX_CONFIG, ...config };
    this.signal = new EventEmitter();

    this.log(`📬 Inbox initialized (maxSize=${this.config.maxSize}, signal-based wakeup)`);
  }

  /**
   * Add item to inbox (non-blocking)
   * Accepts both messages and tasks
   * Traffic management: Drop lowest priority when full
   * SIGNAL-BASED: Instantly wakes waiting serviceInbox (no polling delay)
   */
  async enqueue(item: QueueItem): Promise<boolean> {
    // Check if over capacity
    if (this.queue.length >= this.config.maxSize) {
      // Sort by priority (highest first)
      this.queue.sort((a, b) => b.priority - a.priority);

      // Drop lowest priority item (traffic shed)
      const dropped = this.queue.pop();
      this.log(`⚠️  Queue full! Dropped low-priority ${dropped?.type} (priority=${dropped?.priority.toFixed(2)})`);
    }

    // Add item
    this.queue.push(item);

    // Re-sort by priority
    this.queue.sort((a, b) => b.priority - a.priority);

    // Log with type-specific details
    if (isInboxMessage(item)) {
      this.log(`📬 Enqueued message: ${item.senderId.slice(0, 8)} → priority=${item.priority.toFixed(2)} (queue=${this.queue.length})`);
    } else if (isInboxTask(item)) {
      this.log(`📬 Enqueued task: ${item.taskType} → priority=${item.priority.toFixed(2)} (queue=${this.queue.length})`);
    }

    // CRITICAL: Signal waiting serviceInbox (instant wakeup, no polling)
    this.signal.emit('work-available');

    return true;
  }

  /**
   * Check inbox without removing (non-blocking)
   * Returns top N items by priority
   */
  async peek(limit: number = 10): Promise<QueueItem[]> {
    return this.queue.slice(0, limit);
  }

  /**
   * Remove and return next item (blocking with timeout)
   * Returns null if no item within timeout
   */
  async pop(timeoutMs: number = 5000): Promise<QueueItem | null> {
    // Immediate check
    if (this.queue.length > 0) {
      const item = this.queue.shift()!;
      if (isInboxMessage(item)) {
        this.log(`📭 Popped message: ${item.id.slice(0, 8)} (queue=${this.queue.length})`);
      } else if (isInboxTask(item)) {
        this.log(`📭 Popped task: ${item.taskId.slice(0, 8)} (queue=${this.queue.length})`);
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
            this.log(`📭 Popped message (after wait): ${item.id.slice(0, 8)} (queue=${this.queue.length})`);
          } else if (isInboxTask(item)) {
            this.log(`📭 Popped task (after wait): ${item.taskId.slice(0, 8)} (queue=${this.queue.length})`);
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
    // Immediate check - if work available, return instantly
    if (this.queue.length > 0) {
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
  } {
    const highestPriority = this.queue.length > 0 ? this.queue[0].priority : null;
    const lowestPriority = this.queue.length > 0 ? this.queue[this.queue.length - 1].priority : null;

    return {
      size: this.getSize(),
      load: this.getLoad(),
      overloaded: this.isOverloaded(),
      highestPriority,
      lowestPriority
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
