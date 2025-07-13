/**
 * Chat WebSocket Handler
 * 
 * Registers chat message handling with MessageRouter.
 * Demonstrates how application-specific handlers register themselves.
 */

import { messageRouter } from '../core/MessageRouter';
import { DaemonConnector } from '../core/DaemonConnector';

// Default room from JSON configuration - matches ChatCommand
const DEFAULT_ROOM_ID = 'general';

/**
 * Register chat handler with MessageRouter
 */
export function registerChatHandler(): void {
  console.log('💬 Registering chat WebSocket handler');

  messageRouter.registerHandler('message', async (data: any, clientId: string, daemonConnector: DaemonConnector) => {
    const roomId = data.room || DEFAULT_ROOM_ID;
    const message = data.content || data.message || data;
    
    console.log(`💬 Chat message from ${clientId} to room: ${roomId}`);
    
    if (!daemonConnector.isConnected()) {
      return {
        success: false,
        error: 'Chat daemon not connected',
        echo: { message, room: roomId, clientId }
      };
    }

    try {
      // Route to ChatCommand via daemon system
      const result = await daemonConnector.executeCommand(
        'chat',
        { 
          message: message,
          room: roomId,
          history: data.history || []
        },
        { clientId }
      );
      
      return {
        success: true,
        chat_result: result,
        echo: { message, room: roomId, clientId }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error('Chat command failed:', errorMessage);
      
      return {
        success: false,
        error: errorMessage,
        echo: { message, room: roomId, clientId }
      };
    }
  });

  console.log('✅ Chat handler registered for message type');
}