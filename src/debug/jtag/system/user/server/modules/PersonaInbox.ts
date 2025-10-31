/**
 * PersonaInbox - Traffic-managed message queue for autonomous personas
 *
 * Philosophy: "scheduling and self prioritization, but not neglecting so badly, like a traffic problem"
 *
 * Traffic Management Properties:
 * - Priority-based queue (high priority never starved)
 * - Graceful degradation (drop low priority when overloaded)
 * - Load awareness (personas see queue depth)
 * - Non-blocking operations (autonomous checking)
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';

/**
 * Message in persona's inbox
 */
export interface InboxMessage {
  messageId: string;      // Chat message ID
  roomId: UUID;           // Room where message was sent
  content: string;        // Message text
  senderId: UUID;         // Who sent it
  senderName: string;     // Sender display name
  timestamp: number;      // When message was sent
  priority: number;       // 0.0-1.0 (calculated relevance + urgency)
  mentions?: boolean;     // True if persona mentioned by name
}

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
 * PersonaInbox: Priority queue for autonomous message processing
 */
export class PersonaInbox {
  private readonly config: InboxConfig;
  private queue: InboxMessage[] = [];
  private readonly personaId: UUID;
  private readonly personaName: string;

  constructor(personaId: UUID, personaName: string, config: Partial<InboxConfig> = {}) {
    this.personaId = personaId;
    this.personaName = personaName;
    this.config = { ...DEFAULT_INBOX_CONFIG, ...config };

    this.log(`📬 Inbox initialized (maxSize=${this.config.maxSize})`);
  }

  /**
   * Add message to inbox (non-blocking)
   * Traffic management: Drop lowest priority when full
   */
  async enqueue(message: InboxMessage): Promise<boolean> {
    // Check if over capacity
    if (this.queue.length >= this.config.maxSize) {
      // Sort by priority (highest first)
      this.queue.sort((a, b) => b.priority - a.priority);

      // Drop lowest priority message (traffic shed)
      const dropped = this.queue.pop();
      this.log(`⚠️  Queue full! Dropped low-priority message (priority=${dropped?.priority.toFixed(2)})`);
    }

    // Add message
    this.queue.push(message);

    // Re-sort by priority
    this.queue.sort((a, b) => b.priority - a.priority);

    this.log(`📬 Enqueued: ${message.senderId.slice(0, 8)} → priority=${message.priority.toFixed(2)} (queue=${this.queue.length})`);

    return true;
  }

  /**
   * Check inbox without removing (non-blocking)
   * Returns top N messages by priority
   */
  async peek(limit: number = 10): Promise<InboxMessage[]> {
    return this.queue.slice(0, limit);
  }

  /**
   * Remove and return next message (blocking with timeout)
   * Returns null if no message within timeout
   */
  async pop(timeoutMs: number = 5000): Promise<InboxMessage | null> {
    // Immediate check
    if (this.queue.length > 0) {
      const message = this.queue.shift()!;
      this.log(`📭 Popped: ${message.messageId.slice(0, 8)} (queue=${this.queue.length})`);
      return message;
    }

    // Wait for message
    return new Promise<InboxMessage | null>((resolve) => {
      const startTime = Date.now();

      const checkInterval = setInterval(() => {
        if (this.queue.length > 0) {
          clearInterval(checkInterval);
          const message = this.queue.shift()!;
          this.log(`📭 Popped (after wait): ${message.messageId.slice(0, 8)} (queue=${this.queue.length})`);
          resolve(message);
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
   * Clear inbox (for testing/reset)
   */
  clear(): void {
    const cleared = this.queue.length;
    this.queue = [];
    this.log(`🗑️  Cleared ${cleared} messages`);
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
   */
  private log(message: string): void {
    if (!this.config.enableLogging) return;
    console.log(`[${this.personaName}:Inbox] ${message}`);
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

  return Math.min(1.0, priority); // Cap at 1.0
}
