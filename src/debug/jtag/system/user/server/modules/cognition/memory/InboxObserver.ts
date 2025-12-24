/**
 * InboxObserver - Non-blocking observer for PersonaInbox
 *
 * Peeks at inbox activity without blocking the main thread
 * Used by MemoryConsolidationWorker to detect patterns
 */

import type { PersonaInbox, QueueItem } from '../../PersonaInbox';

export class InboxObserver {
  private log: (message: string) => void;

  constructor(private inbox: PersonaInbox, logger?: (message: string) => void) {
    this.log = logger || (() => {});
  }

  /**
   * Peek at recent inbox items (non-blocking)
   *
   * @param limit - Maximum number of items to peek
   * @returns Array of queue items (empty if inbox empty)
   */
  async peek(limit: number): Promise<QueueItem[]> {
    try {
      // Non-blocking peek at inbox
      // PersonaInbox.peek() is already non-blocking
      const items = await this.inbox.peek(limit);
      return items;
    } catch (error) {
      this.log(`❌ Error peeking inbox: ${error}`);
      return [];
    }
  }

  /**
   * Get inbox depth (how many items are queued)
   */
  async getDepth(): Promise<number> {
    try {
      // Get current queue size
      const items = await this.inbox.peek(1000); // Peek all
      return items.length;
    } catch (error) {
      this.log(`❌ Error getting inbox depth: ${error}`);
      return 0;
    }
  }
}
