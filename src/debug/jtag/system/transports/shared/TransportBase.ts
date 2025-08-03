/**
 * Transport Base - Common functionality for all transport implementations
 * 
 * Provides shared logic and utilities that all transport implementations can use.
 * Follows the pattern from CommandBase for consistent architecture.
 */

import type { JTAGMessage } from '../../core/types/JTAGTypes';
import type { EventsInterface } from '../../events';
import type { JTAGTransport, TransportSendResult } from './TransportTypes';

/**
 * Universal Transport Adapter Interface - enforces plugin contract
 * Compatible with existing JTAGTransport implementations
 */
export interface ITransportAdapter {
  readonly name: string;
  readonly protocol?: string;               // Optional for backward compatibility
  readonly supportedRoles?: string[];      // Optional for backward compatibility  
  readonly supportedEnvironments?: string[]; // Optional for backward compatibility
  
  // Universal lifecycle - matches existing JTAGTransport interface
  connect?(url?: string): Promise<void>;   // Optional, existing transports use different connection patterns
  disconnect(): Promise<void>;
  send(message: any): Promise<any>;
  isConnected(): boolean;
  
  // Optional capabilities
  setMessageHandler?(handler: (message: any) => void): void;
  getHealth?(): any;
}

/**
 * Adapter Entry - Universal transport adapter registry interface
 * Used by generator for auto-discovery and registration
 */
export interface AdapterEntry {
  name: string;                    // 'websocket', 'http', 'udp-multicast'
  className: string;               // 'WebSocketTransportBrowser'
  adapterClass: new (...args: any[]) => ITransportAdapter; // ✅ Type-safe constructor
  protocol: string;                // Protocol identifier
  supportedRoles: string[];        // ['server', 'client'] 
  supportedEnvironments: string[]; // ['browser', 'server']
}

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

/**
 * @deprecated Use AdapterEntry instead - keeping for backward compatibility
 */
export interface TransportEntry {
  name: string;
  className: string;
  adapterClass: new (...args: any[]) => JTAGTransport;
}