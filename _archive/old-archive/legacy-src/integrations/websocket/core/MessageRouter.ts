/**
 * Message Router - Clean Registry Pattern
 * 
 * ARCHITECTURAL PRINCIPLE: Dependency Inversion
 * - MessageRouter is a passive registry
 * - Components register themselves via registerHandler()
 * - NO hardcoded knowledge of dependents
 */

import { EventEmitter } from 'events';
import { WebSocketMessage } from '../types';
import { DaemonConnector } from './DaemonConnector';

export class MessageRouter extends EventEmitter {
  private handlers = new Map<string, MessageHandler>();

  constructor() {
    super();
    console.log('📮 MessageRouter: Initialized as passive registry');
  }

  async routeMessage(
    message: any,
    clientId: string,
    daemonConnector: DaemonConnector
  ): Promise<WebSocketMessage | null> {
    const messageType = message.type;
    const handler = this.handlers.get(messageType);
    
    if (!handler) {
      console.log(`❌ No handler registered for: ${messageType}`);
      return {
        type: 'error',
        data: { 
          error: `Unknown message type: ${messageType}`,
          availableTypes: Array.from(this.handlers.keys()),
          hint: 'Component must register handler via MessageRouter.registerHandler()'
        },
        timestamp: new Date().toISOString(),
        requestId: message.requestId
      };
    }

    try {
      console.log(`📨 Routing ${messageType} to registered handler`);
      const result = await handler(message.data, clientId, daemonConnector);
      
      return {
        type: 'response',
        data: result,
        timestamp: new Date().toISOString(),
        requestId: message.requestId
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ Handler error for ${messageType}:`, errorMessage);
      
      return {
        type: 'error',
        data: { 
          error: errorMessage,
          messageType: messageType
        },
        timestamp: new Date().toISOString(),
        requestId: message.requestId
      };
    }
  }

  /**
   * Register a message handler - called by components to register themselves
   */
  registerHandler(messageType: string, handler: MessageHandler): void {
    console.log(`📝 MessageRouter: Component registered handler for '${messageType}'`);
    this.handlers.set(messageType, handler);
  }

  /**
   * Unregister a message handler
   */
  unregisterHandler(messageType: string): boolean {
    console.log(`🗑️ MessageRouter: Unregistering handler for '${messageType}'`);
    return this.handlers.delete(messageType);
  }

  /**
   * Get all registered message types
   */
  getRegisteredTypes(): string[] {
    return Array.from(this.handlers.keys());
  }

  /**
   * Get count of registered handlers
   */
  getHandlerCount(): number {
    return this.handlers.size;
  }
}

/**
 * Message handler function signature
 */
export type MessageHandler = (
  data: any,
  clientId: string,
  daemonConnector: DaemonConnector
) => Promise<any>;

// Export for use by other modules
export const messageRouter = new MessageRouter();