/**
 * Transport Base - Common functionality for all transport implementations
 * 
 * Provides shared logic and utilities that all transport implementations can use.
 * Follows the pattern from CommandBase for consistent architecture.
 */

import type { JTAGMessage } from '../../core/types/JTAGTypes';
import type { EventsInterface } from '../../events';
import type { JTAGTransport, TransportSendResult } from './TransportTypes';

export abstract class TransportBase implements JTAGTransport {
  public abstract readonly name: string;
  
  protected eventSystem?: EventsInterface;
  protected messageHandler?: (message: JTAGMessage) => void;
  protected connected = false;

  /**
   * Set event system for transport events
   */
  setEventSystem(eventSystem: EventsInterface): void {
    this.eventSystem = eventSystem;
  }

  /**
   * Set message handler for incoming messages
   */
  setMessageHandler(handler: (message: JTAGMessage) => void): void {
    this.messageHandler = handler;
  }

  /**
   * Check if transport is connected
   */
  isConnected(): boolean {
    return this.connected;
  }

  /**
   * Handle incoming message - delegates to registered handler
   */
  protected handleIncomingMessage(message: JTAGMessage): void {
    if (this.messageHandler) {
      this.messageHandler(message);
    }
  }

  /**
   * Create standardized transport result
   */
  protected createResult(success: boolean, sentCount?: number): TransportSendResult {
    return {
      success,
      timestamp: new Date().toISOString(),
      sentCount
    };
  }

  // Abstract methods that implementations must provide
  abstract send(message: JTAGMessage): Promise<TransportSendResult>;
  abstract disconnect(): Promise<void>;
  abstract reconnect?(): Promise<void>;
}