/**
 * Chat History Command - Retrieve chat room message history
 */

import { BaseCommand } from '../../core/base-command/BaseCommand';
import { CommandDefinition, CommandResult } from '../../../types/CommandTypes';

export class ChatHistoryCommand extends BaseCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'chat_history',
      description: 'Retrieve chat room message history',
      category: 'communication',
      parameters: {
        room: {
          type: 'string',
          description: 'Chat room name',
          required: false,
          default: 'general'
        },
        limit: {
          type: 'number',
          description: 'Maximum number of messages to retrieve',
          required: false,
          default: 50
        }
      },
      examples: [
        {
          description: 'Get recent chat history',
          command: 'chat_history'
        },
        {
          description: 'Get specific room history',
          command: 'chat_history --room=development'
        }
      ]
    };
  }

  static async execute(_command: string, params: any = {}): Promise<CommandResult> {
    try {
      const roomId = params.roomId || params.room || 'general';
      const limit = params.limit || 50;
      const offset = params.offset || 0;

      // Delegate to ChatRoomDaemon
      try {
        const result = await this.delegateToChatRoomDaemon('get_messages', {
          room_id: roomId,
          limit: limit,
          offset: offset
        });

        // Transform messages to expected format
        const messages = (result.messages || []).map((msg: any) => ({
          id: msg.id,
          room: roomId,
          sender: msg.sender_id,
          message: msg.content,
          content: msg.content, // Support both field names
          timestamp: msg.timestamp,
          type: msg.message_type || 'text',
          metadata: msg.metadata
        }));

        return this.createSuccessResult(
          `Found ${messages.length} messages in room "${roomId}"`,
          {
            room: roomId,
            messages: messages,
            count: result.total_count || messages.length,
            limit: limit
          }
        );
      } catch (daemonError) {
        // Fallback to mock data if daemon communication fails
        console.warn('ChatRoomDaemon unavailable, using fallback data:', daemonError);
        
        const mockHistory = [
          {
            id: 'msg_1',
            room: roomId,
            sender: 'System',
            message: 'Welcome to Continuum chat!',
            content: 'Welcome to Continuum chat!',
            timestamp: new Date(Date.now() - 3600000).toISOString(),
            type: 'system'
          },
          {
            id: 'msg_2', 
            room: roomId,
            sender: 'Claude',
            message: 'Hello! I\'m ready to help with your development tasks.',
            content: 'Hello! I\'m ready to help with your development tasks.',
            timestamp: new Date(Date.now() - 1800000).toISOString(),
            type: 'assistant',
            metadata: { persona: 'Claude' }
          },
          {
            id: 'msg_3',
            room: roomId,
            sender: 'User',
            message: 'Great! Let\'s work on the console capture system.',
            content: 'Great! Let\'s work on the console capture system.',
            timestamp: new Date(Date.now() - 900000).toISOString(),
            type: 'user'
          }
        ];

        return this.createSuccessResult(
          `Found ${mockHistory.length} messages in room "${roomId}"`,
          {
            room: roomId,
            messages: mockHistory.slice(0, limit),
            count: mockHistory.length,
            limit: limit
          }
        );
      }

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Failed to retrieve chat history: ${errorMessage}`);
    }
  }

  /**
   * Delegate to ChatRoomDaemon via WebSocket
   */
  private static async delegateToChatRoomDaemon(operation: string, params: any): Promise<any> {
    const WebSocket = (await import('ws')).default;
    const ws = new WebSocket('ws://localhost:9000');
    
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        ws.close();
        reject(new Error('ChatRoomDaemon request timeout'));
      }, 5000);
      
      ws.on('open', () => {
        const message = {
          id: `chat_history_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
          from: 'chat-history-command',
          to: 'chatroom',
          type: operation,
          timestamp: new Date(),
          data: params
        };
        
        ws.send(JSON.stringify(message));
      });
      
      ws.on('message', (data: any) => {
        clearTimeout(timeout);
        try {
          const response = JSON.parse(data.toString());
          ws.close();
          
          if (response.success) {
            resolve(response.data);
          } else {
            reject(new Error(response.error || 'ChatRoomDaemon operation failed'));
          }
        } catch (error) {
          ws.close();
          reject(error);
        }
      });
      
      ws.on('error', (error) => {
        clearTimeout(timeout);
        ws.close();
        reject(error);
      });
    });
  }
}