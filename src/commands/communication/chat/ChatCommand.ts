/**
 * Chat Command - Core messaging functionality
 * 
 * CRITICAL SYSTEM COMMAND - Required for basic chat functionality
 */

import { DirectCommand } from '../../core/direct-command/DirectCommand.js';
import { CommandDefinition, CommandContext, CommandResult } from '../../core/base-command/BaseCommand.js';

export class ChatCommand extends DirectCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'chat',
      description: 'Core chat messaging functionality',
      parameters: {
        message: { type: 'string', required: false, description: 'Message to send' },
        room: { type: 'string', required: false, description: 'Chat room ID' }
      }
    };
  }

  async execute(params: any, context: CommandContext): Promise<CommandResult> {
    try {
      // Delegate to ChatRoomDaemon for actual chat functionality
      if (params.message) {
        const result = await this.delegateToChatRoomDaemon('send_message', {
          room_id: params.room || 'default',
          sender_id: context.session_id || 'unknown',
          content: params.message,
          message_type: 'text'
        });
        
        return {
          success: true,
          data: result
        };
      }

      // List available rooms
      const roomsResult = await this.delegateToChatRoomDaemon('list_rooms', {
        user_id: context.session_id || 'unknown'
      });

      return {
        success: true,
        data: {
          status: 'Chat system ready',
          available_rooms: roomsResult.rooms
        }
      };
    } catch (error) {
      // Fallback if ChatRoomDaemon not available
      return {
        success: false,
        error: `Chat system error: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  /**
   * Delegate to ChatRoomDaemon via internal message bus
   */
  private async delegateToChatRoomDaemon(operation: string, params: any): Promise<any> {
    // TODO: Implement actual daemon delegation via message bus
    // For now, return fallback responses to keep system working
    
    switch (operation) {
      case 'send_message':
        return {
          message_id: `msg_${Date.now()}`,
          message: {
            content: params.content,
            sender_id: params.sender_id,
            room_id: params.room_id,
            timestamp: new Date().toISOString()
          }
        };
      case 'list_rooms':
        return {
          rooms: [
            { id: 'default', name: 'General Chat', participant_count: 1 },
            { id: 'system', name: 'System Messages', participant_count: 0 }
          ]
        };
      default:
        throw new Error(`Unknown ChatRoomDaemon operation: ${operation}`);
    }
  }
}