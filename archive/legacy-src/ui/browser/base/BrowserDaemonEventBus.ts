/**
 * BrowserDaemonEventBus - Browser-side event coordination
 * 
 * Mirrors server-side DaemonEventBus for consistent event handling
 * patterns across client and server environments.
 * 
 * Provides:
 * - Event subscription and emission
 * - Type-safe event handling
 * - Event isolation between daemons
 * - Debugging and monitoring capabilities
 */

export interface BrowserDaemonEvent {
  type: string;
  data: any;
  source: string;
  timestamp: string;
  id: string;
}

export type BrowserEventHandler = (event: BrowserDaemonEvent) => void | Promise<void>;

export class BrowserDaemonEventBus {
  private static instance: BrowserDaemonEventBus | null = null;
  private eventHandlers = new Map<string, BrowserEventHandler[]>();
  private eventHistory: BrowserDaemonEvent[] = [];
  private maxHistorySize = 1000;

  /**
   * Singleton pattern - mirrors server DaemonEventBus
   */
  static getInstance(): BrowserDaemonEventBus {
    if (!BrowserDaemonEventBus.instance) {
      BrowserDaemonEventBus.instance = new BrowserDaemonEventBus();
    }
    return BrowserDaemonEventBus.instance;
  }

  /**
   * Subscribe to event type
   */
  on(eventType: string, handler: BrowserEventHandler): void {
    if (!this.eventHandlers.has(eventType)) {
      this.eventHandlers.set(eventType, []);
    }
    
    const handlers = this.eventHandlers.get(eventType)!;
    handlers.push(handler);
    
    this.debugLog(`Handler registered for event: ${eventType}`);
  }

  /**
   * Unsubscribe from event type
   */
  off(eventType: string, handler?: BrowserEventHandler): void {
    if (!this.eventHandlers.has(eventType)) {
      return;
    }

    const handlers = this.eventHandlers.get(eventType)!;
    
    if (handler) {
      const index = handlers.indexOf(handler);
      if (index !== -1) {
        handlers.splice(index, 1);
        this.debugLog(`Specific handler removed for event: ${eventType}`);
      }
    } else {
      // Remove all handlers for this event type
      this.eventHandlers.set(eventType, []);
      this.debugLog(`All handlers removed for event: ${eventType}`);
    }
  }

  /**
   * Emit event to all subscribers
   */
  async emit(eventType: string, data: any, source: string = 'unknown'): Promise<void> {
    const event: BrowserDaemonEvent = {
      type: eventType,
      data,
      source,
      timestamp: new Date().toISOString(),
      id: this.generateEventId()
    };

    // Add to history
    this.addToHistory(event);

    // Get handlers for this event type
    const handlers = this.eventHandlers.get(eventType) || [];
    
    if (handlers.length === 0) {
      this.debugLog(`No handlers for event: ${eventType}`);
      return;
    }

    this.debugLog(`Emitting event: ${eventType} to ${handlers.length} handlers`);

    // Execute all handlers
    const promises = handlers.map(async (handler) => {
      try {
        await handler(event);
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        console.error(`[BrowserDaemonEventBus] Handler error for ${eventType}: ${errorMessage}`);
      }
    });

    await Promise.all(promises);
  }

  /**
   * Get all registered event types
   */
  getRegisteredEventTypes(): string[] {
    return Array.from(this.eventHandlers.keys());
  }

  /**
   * Get handler count for event type
   */
  getHandlerCount(eventType: string): number {
    return this.eventHandlers.get(eventType)?.length || 0;
  }

  /**
   * Get recent event history
   */
  getEventHistory(limit: number = 10): BrowserDaemonEvent[] {
    return this.eventHistory.slice(-limit);
  }

  /**
   * Clear all handlers and history
   */
  clear(): void {
    this.eventHandlers.clear();
    this.eventHistory = [];
    this.debugLog('Event bus cleared');
  }

  /**
   * Get diagnostics information
   */
  getDiagnostics() {
    return {
      registeredEventTypes: this.getRegisteredEventTypes(),
      totalHandlers: Array.from(this.eventHandlers.values())
        .reduce((sum, handlers) => sum + handlers.length, 0),
      eventHistorySize: this.eventHistory.length,
      recentEvents: this.getEventHistory(5)
    };
  }

  /**
   * Add event to history with size limit
   */
  private addToHistory(event: BrowserDaemonEvent): void {
    this.eventHistory.push(event);
    
    // Maintain history size limit
    if (this.eventHistory.length > this.maxHistorySize) {
      this.eventHistory.shift();
    }
  }

  /**
   * Generate unique event ID
   */
  private generateEventId(): string {
    return `evt_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  /**
   * Debug logging
   */
  private debugLog(message: string): void {
    const timestamp = new Date().toISOString();
    console.log(`[${timestamp}] [BrowserDaemonEventBus] ${message}`);
  }
}

/**
 * Convenience function to get the singleton instance
 */
export function getBrowserDaemonEventBus(): BrowserDaemonEventBus {
  return BrowserDaemonEventBus.getInstance();
}