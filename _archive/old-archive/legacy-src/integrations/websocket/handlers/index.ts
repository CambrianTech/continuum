/**
 * WebSocket Handler Registration
 * 
 * This module initializes all WebSocket handlers by having them
 * register themselves with MessageRouter. This follows proper
 * dependency inversion - components register with the router.
 */

import { registerCoreHandlers } from './CoreHandlers';
import { registerChatHandler } from './ChatHandler';

/**
 * Initialize all WebSocket handlers
 * Call this during system startup to register all handlers
 */
export function initializeWebSocketHandlers(): void {
  console.log('🚀 Initializing WebSocket handlers...');
  
  // Register core system handlers
  registerCoreHandlers();
  
  // Register application-specific handlers
  registerChatHandler();
  
  // Future handlers register themselves here:
  // registerCommandHandler();
  // registerFileHandler();
  // registerEventHandler();
  
  console.log('✅ All WebSocket handlers initialized');
}

// Re-export for convenience
export { messageRouter } from '../core/MessageRouter';