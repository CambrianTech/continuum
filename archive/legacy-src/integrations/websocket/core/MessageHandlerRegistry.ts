// ISSUES: 0 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * Message Handler Registry - Central registration for WebSocket message handlers
 * 
 * ✅ HANDLER REGISTRATION: Daemons register handlers for message types they handle
 * ✅ PRIORITY SUPPORT: Higher priority handlers processed first
 * ✅ CLEAN UNREGISTRATION: Proper cleanup when daemons stop
 */

import { MessageHandler, MessageHandlerRegistry, HandlerRegistration } from '../types/MessageHandler';

export class DefaultMessageHandlerRegistry implements MessageHandlerRegistry {
  private handlers = new Map<string, HandlerRegistration[]>();
  
  registerHandler(messageType: string, handler: MessageHandler, daemonName?: string, options?: { allowReplace?: boolean }): void {
    if (!this.handlers.has(messageType)) {
      this.handlers.set(messageType, []);
    }
    
    const handlerList = this.handlers.get(messageType)!;
    const resolvedDaemonName = daemonName || 'unknown';
    const allowReplace = options?.allowReplace ?? true; // Default: replace duplicates
    
    // Check for existing handler from same daemon
    const existingIndex = handlerList.findIndex(reg => reg.daemonName === resolvedDaemonName);
    
    if (existingIndex !== -1) {
      if (allowReplace) {
        // Replace existing handler
        handlerList.splice(existingIndex, 1);
        console.log(`📋 Replaced existing handler for '${messageType}' from ${resolvedDaemonName}`);
      } else {
        // Throw error on duplicate
        throw new Error(`Handler for '${messageType}' from daemon '${resolvedDaemonName}' already exists. Use allowReplace: true to override.`);
      }
    }
    
    const registration: HandlerRegistration = {
      messageType,
      handler,
      daemonName: resolvedDaemonName,
      registeredAt: new Date()
    };
    
    handlerList.push(registration);
    
    // Sort by priority (highest first)
    handlerList.sort((a, b) => {
      const priorityA = a.handler.priority || 0;
      const priorityB = b.handler.priority || 0;
      return priorityB - priorityA;
    });
    
    console.log(`📋 Registered handler for '${messageType}' from ${resolvedDaemonName} (priority: ${handler.priority || 0})`);
  }
  
  unregisterHandler(messageType: string, handler: MessageHandler): void {
    const handlerList = this.handlers.get(messageType);
    if (!handlerList) return;
    
    const index = handlerList.findIndex(reg => reg.handler === handler);
    if (index !== -1) {
      const removed = handlerList.splice(index, 1)[0];
      console.log(`📋 Unregistered handler for '${messageType}' from ${removed.daemonName}`);
    }
    
    // Clean up empty lists
    if (handlerList.length === 0) {
      this.handlers.delete(messageType);
    }
  }
  
  getHandlers(messageType: string): MessageHandler[] {
    const registrations = this.handlers.get(messageType) || [];
    return registrations.map(reg => reg.handler);
  }
  
  hasHandlers(messageType: string): boolean {
    const handlerList = this.handlers.get(messageType);
    return handlerList !== undefined && handlerList.length > 0;
  }
  
  /**
   * Get all registered message types
   */
  getRegisteredTypes(): string[] {
    return Array.from(this.handlers.keys());
  }
  
  /**
   * Get handler count for a message type
   */
  getHandlerCount(messageType: string): number {
    return this.handlers.get(messageType)?.length || 0;
  }
  
  /**
   * Get registration info for debugging
   */
  getRegistrationInfo(): HandlerRegistration[] {
    const allRegistrations: HandlerRegistration[] = [];
    for (const registrations of this.handlers.values()) {
      allRegistrations.push(...registrations);
    }
    return allRegistrations;
  }
  
  /**
   * Clear all handlers (useful for testing)
   */
  clear(): void {
    this.handlers.clear();
    console.log('📋 Cleared all message handlers');
  }
}

// Global registry instance
export const MESSAGE_HANDLER_REGISTRY = new DefaultMessageHandlerRegistry();