/**
 * DaemonRouter - Handles routing messages between daemons
 * Extracted from WebSocketDaemon to follow single responsibility principle
 */

import { EventEmitter } from 'events';
import { DaemonMessage, DaemonResponse } from '../../../daemons/base/DaemonProtocol';

export class DaemonRouter extends EventEmitter {
  private registeredDaemons = new Map<string, any>();
  
  /**
   * Register a daemon for message routing
   */
  registerDaemon(daemon: any): void {
    if (!daemon || !daemon.name) {
      throw new Error('Invalid daemon: must have a name property');
    }
    
    this.registeredDaemons.set(daemon.name, daemon);
    this.emit('daemon-registered', { name: daemon.name });
  }
  
  /**
   * Get all registered daemon names
   */
  getRegisteredDaemons(): string[] {
    return Array.from(this.registeredDaemons.keys());
  }
  
  /**
   * Check if a daemon is registered
   */
  hasDaemon(name: string): boolean {
    return this.registeredDaemons.has(name);
  }
  
  /**
   * Route a message to a specific daemon
   */
  async routeMessage(message: DaemonMessage): Promise<DaemonResponse> {
    const targetDaemon = message.to;
    
    if (!targetDaemon) {
      return {
        success: false,
        error: 'No target daemon specified in message'
      };
    }
    
    const daemon = this.registeredDaemons.get(targetDaemon);
    
    if (!daemon) {
      return {
        success: false,
        error: `Daemon ${targetDaemon} not found. Registered: [${this.getRegisteredDaemons().join(', ')}]`
      };
    }
    
    if (!daemon.handleMessage) {
      return {
        success: false,
        error: `Daemon ${targetDaemon} does not support message handling`
      };
    }
    
    try {
      return await daemon.handleMessage(message);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Daemon ${targetDaemon} error: ${errorMessage}`
      };
    }
  }
  
  /**
   * Broadcast a message to all daemons (for events like shutdown)
   */
  async broadcastMessage(message: Omit<DaemonMessage, 'to'>): Promise<Map<string, DaemonResponse>> {
    const responses = new Map<string, DaemonResponse>();
    
    for (const [name, daemon] of this.registeredDaemons) {
      if (daemon.handleMessage) {
        try {
          const response = await daemon.handleMessage({ ...message, to: name });
          responses.set(name, response);
        } catch (error) {
          responses.set(name, {
            success: false,
            error: error instanceof Error ? error.message : String(error)
          });
        }
      }
    }
    
    return responses;
  }
}