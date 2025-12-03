/**
 * JTAG Event System - Promise-Powered Event Architecture
 */

export interface EventsInterface<T = unknown> {
  emit(eventName: string, data?: T): void;
  on(eventName: string, listener: (data?: T) => void): () => void;
  waitFor?(eventName: string, timeout?: number): Promise<T>;
}

/**
 * Base interface for events that can be sorted chronologically
 */
export interface TimestampedEvent {
  timestamp: number;
  [key: string]: unknown;
}

/**
 * Coalesced event data - contains latest state + count of merged events
 */
export interface CoalescedEventData<T = unknown> {
  data: T;  // Latest event data
  count: number;  // How many events were merged
  firstTimestamp: number;  // When first event arrived
  lastTimestamp: number;  // When last event arrived
}

/**
 * Pending event in coalescing buffer
 */
interface PendingEvent {
  eventName: string;
  contextKey: string;  // Key to identify duplicate events (e.g., "room:123")
  data: unknown;  // Latest data (for state updates)
  allData: unknown[];  // All data (for chat messages - preserves order)
  count: number;
  firstTimestamp: number;
  lastTimestamp: number;
  timer: ReturnType<typeof setTimeout>;
}

export class EventManager {
  listeners: Map<string, ((data?: unknown) => void)[]> = new Map();

  // Event coalescing buffer (key: "eventName:contextKey" -> PendingEvent)
  private coalescingBuffer: Map<string, PendingEvent> = new Map();
  private readonly COALESCE_DELAY = 100; // 100ms debounce window

  get events(): EventsInterface {
    return {
      emit: (eventName: string, data?: unknown) => {
        // Check if this event should be coalesced
        if (this.shouldCoalesce(eventName, data)) {
          this.coalesceEvent(eventName, data);
        } else {
          // Emit immediately
          this.emitImmediate(eventName, data);
        }
      },
      
      on: (eventName: string, listener: (data?: unknown) => void): (() => void) => {
        if (!this.listeners.has(eventName)) {
          this.listeners.set(eventName, []);
        }
        this.listeners.get(eventName)!.push(listener);
        
        // Return unsubscribe function
        return () => {
          const listeners = this.listeners.get(eventName);
          if (listeners) {
            const index = listeners.indexOf(listener);
            if (index > -1) {
              listeners.splice(index, 1);
            }
          }
        };
      },
      
      waitFor: async (eventName: string, timeout = 5000): Promise<unknown> => {
        return new Promise((resolve, reject) => {
          const timer = setTimeout(() => {
            reject(new Error(`Timeout waiting for event: ${eventName}`));
          }, timeout);

          const unsubscribe = this.events.on(eventName, (data) => {
            clearTimeout(timer);
            unsubscribe();
            resolve(data);
          });
        });
      }
    };
  }

  /**
   * Check if event should be coalesced based on event type
   * Coalesce: message events, state updates, rapid-fire notifications
   * Don't coalesce: user actions, system events, one-time events
   */
  private shouldCoalesce(eventName: string, data: unknown): boolean {
    // Coalesce chat message events (high-frequency)
    if (eventName.includes('message') || eventName.includes('chat:')) {
      return this.extractContextKey(eventName, data) !== null;
    }

    // Coalesce state update events
    if (eventName.includes('state:') || eventName.includes('update')) {
      return this.extractContextKey(eventName, data) !== null;
    }

    // Don't coalesce user actions or system events
    return false;
  }

  /**
   * Extract context key from event data (e.g., roomId for chat events)
   */
  private extractContextKey(eventName: string, data: unknown): string | null {
    if (!data || typeof data !== 'object') {
      return null;
    }

    const dataObj = data as Record<string, unknown>;

    // Try common context keys
    if (dataObj.roomId) return `room:${dataObj.roomId}`;
    if (dataObj.contextId) return `context:${dataObj.contextId}`;
    if (dataObj.userId) return `user:${dataObj.userId}`;
    if (dataObj.sessionId) return `session:${dataObj.sessionId}`;

    return null;
  }

  /**
   * Add event to coalescing buffer or update existing pending event
   */
  private coalesceEvent(eventName: string, data: unknown): void {
    const contextKey = this.extractContextKey(eventName, data);
    if (!contextKey) {
      // Can't extract context, emit immediately
      this.emitImmediate(eventName, data);
      return;
    }

    const bufferKey = `${eventName}:${contextKey}`;
    const existing = this.coalescingBuffer.get(bufferKey);
    const now = Date.now();

    if (existing) {
      // Update existing pending event
      clearTimeout(existing.timer);
      existing.data = data;  // Latest data (for state updates)
      existing.allData.push(data);  // Collect ALL data (for messages)
      existing.count++;
      existing.lastTimestamp = now;

      // Reset timer
      existing.timer = setTimeout(() => {
        this.flushCoalescedEvent(bufferKey);
      }, this.COALESCE_DELAY);

      //console.log(`🔄 Event coalesced: ${eventName} (${existing.count} merged)`);
    } else {
      // Create new pending event
      const timer = setTimeout(() => {
        this.flushCoalescedEvent(bufferKey);
      }, this.COALESCE_DELAY);

      this.coalescingBuffer.set(bufferKey, {
        eventName,
        contextKey,
        data,
        allData: [data],  // Start collecting all data
        count: 1,
        firstTimestamp: now,
        lastTimestamp: now,
        timer
      });
    }
  }

  /**
   * Flush coalesced event from buffer and emit
   */
  private flushCoalescedEvent(bufferKey: string): void {
    const pending = this.coalescingBuffer.get(bufferKey);
    if (!pending) return;

    this.coalescingBuffer.delete(bufferKey);

    // For message events, emit all collected data in chronological order
    if (pending.eventName.includes('message') || pending.eventName.includes('chat:')) {
      // Sort by ORIGINAL timestamp (from event originator, not coalescing time)
      const sortedData = pending.allData.sort((a, b) => {
        const aEvent = a as TimestampedEvent;
        const bEvent = b as TimestampedEvent;
        return (aEvent.timestamp || 0) - (bEvent.timestamp || 0);
      });

      // if (pending.count > 1) {
      //   console.log(`✅ Emitting ${pending.count} coalesced messages in chronological order (saved ${pending.count - 1} handler calls)`);
      // }

      // Emit each message individually in ORIGINAL chronological order
      for (const data of sortedData) {
        this.emitImmediate(pending.eventName, data);
      }
    } else {
      // For state updates, emit latest data only with coalescing metadata
      const coalescedData: CoalescedEventData = {
        data: pending.data,
        count: pending.count,
        firstTimestamp: pending.firstTimestamp,
        lastTimestamp: pending.lastTimestamp
      };

      // if (pending.count > 1) {
      //   console.log(`✅ Emitting coalesced state update: ${pending.eventName} (merged ${pending.count} events, saved ${pending.count - 1} emissions)`);
      // }

      this.emitImmediate(pending.eventName, coalescedData);
    }
  }

  /**
   * Emit event immediately to all listeners
   */
  private emitImmediate(eventName: string, data: unknown): void {
    const listeners = this.listeners.get(eventName) ?? [];
    listeners.forEach(listener => listener(data));
  }
}