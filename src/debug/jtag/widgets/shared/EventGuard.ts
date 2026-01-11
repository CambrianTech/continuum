/**
 * EventGuard - Prevent recursive event processing and race conditions
 *
 * Two utilities:
 * 1. withEventGuard() - Block nested processing of same event type
 * 2. EventSerializer - Ensure sequential processing of async handlers
 *
 * Usage:
 *   // Prevent recursive event handling
 *   Events.subscribe('room:selected', (data) => {
 *     withEventGuard('room:selected', () => {
 *       this.handleRoomSelected(data);
 *     });
 *   });
 *
 *   // Serialize concurrent async handlers
 *   const serializer = new EventSerializer();
 *   Events.subscribe('room:selected', async (data) => {
 *     await serializer.serialize('room-switch', async () => {
 *       await this.handleRoomSelected(data);
 *     });
 *   });
 */

// Global set of currently processing event types
const processingEvents = new Set<string>();

/**
 * Execute handler only if not already processing same event type.
 * Prevents infinite recursion from event handlers that trigger same event.
 *
 * @param eventName - Unique identifier for this event type
 * @param handler - Handler to execute
 * @returns Result of handler, or undefined if blocked
 */
export function withEventGuard<T>(eventName: string, handler: () => T): T | undefined {
  if (processingEvents.has(eventName)) {
    console.warn(`⚠️ EventGuard: Blocked recursive ${eventName} processing`);
    return undefined;
  }

  processingEvents.add(eventName);
  try {
    return handler();
  } finally {
    processingEvents.delete(eventName);
  }
}

/**
 * Async version of withEventGuard for async handlers
 */
export async function withEventGuardAsync<T>(
  eventName: string,
  handler: () => Promise<T>
): Promise<T | undefined> {
  if (processingEvents.has(eventName)) {
    console.warn(`⚠️ EventGuard: Blocked recursive ${eventName} processing`);
    return undefined;
  }

  processingEvents.add(eventName);
  try {
    return await handler();
  } finally {
    processingEvents.delete(eventName);
  }
}

/**
 * EventSerializer - Ensure sequential processing of concurrent events
 *
 * When multiple events fire rapidly, this ensures handlers run one at a time.
 * Prevents race conditions where handlers conflict with each other.
 */
export class EventSerializer {
  private processing = new Map<string, Promise<void>>();

  /**
   * Serialize handler execution by key.
   * If another handler with same key is running, wait for it first.
   *
   * @param key - Unique key for this type of operation
   * @param handler - Async handler to execute
   * @returns Result of handler
   */
  async serialize<T>(key: string, handler: () => Promise<T>): Promise<T | undefined> {
    // Wait for any in-flight processing of same key
    const existing = this.processing.get(key);
    if (existing) {
      await existing;
    }

    // Create promise for this processing
    let resolve: () => void;
    const promise = new Promise<void>(r => { resolve = r; });
    this.processing.set(key, promise);

    try {
      return await handler();
    } finally {
      resolve!();
      this.processing.delete(key);
    }
  }

  /**
   * Check if currently processing a key
   */
  isProcessing(key: string): boolean {
    return this.processing.has(key);
  }

  /**
   * Get all currently processing keys (for debugging)
   */
  getProcessingKeys(): string[] {
    return Array.from(this.processing.keys());
  }
}

/**
 * Global serializer instance for common operations
 */
export const globalSerializer = new EventSerializer();
