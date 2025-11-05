// ISSUES: 0 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * Message Handler Interface - Proper handler registration pattern
 * 
 * ✅ CLEAN ARCHITECTURE: Daemons register handlers for message types they care about
 * ✅ OPEN/CLOSED: WebSocketDaemon doesn't need modification for new message types
 * ✅ SINGLE RESPONSIBILITY: Each daemon handles only its own message types
 */

import { DaemonResponse } from '../../../daemons/base/DaemonProtocol';

export interface MessageHandler {
  /**
   * Handle a specific message type
   */
  handle(data: unknown): Promise<DaemonResponse>;
  
  /**
   * Priority for handler registration (higher = processed first)
   */
  priority?: number;
}

export interface MessageHandlerRegistry {
  /**
   * Register a handler for a specific message type
   */
  registerHandler(messageType: string, handler: MessageHandler, daemonName?: string, options?: { allowReplace?: boolean }): void;
  
  /**
   * Unregister a handler for a message type
   */
  unregisterHandler(messageType: string, handler: MessageHandler): void;
  
  /**
   * Get all handlers for a message type
   */
  getHandlers(messageType: string): MessageHandler[];
  
  /**
   * Check if any handlers are registered for a message type
   */
  hasHandlers(messageType: string): boolean;
}

export interface HandlerRegistration {
  messageType: string;
  handler: MessageHandler;
  daemonName: string;
  registeredAt: Date;
}