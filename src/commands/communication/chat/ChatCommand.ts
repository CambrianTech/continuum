/**
 * Chat Command - Core messaging functionality
 * 
 * CRITICAL SYSTEM COMMAND - Required for basic chat functionality
 */

import { DirectCommand } from '../../core/direct-command/DirectCommand';
import { CommandDefinition, CommandContext, CommandResult } from '../../core/base-command/BaseCommand';
import { 
  ChatRoomDaemonRequest, 
  SendMessageRequest, 
  SendMessageResponse,
  ListRoomsRequest, 
  ListRoomsResponse,
  ChatCommandResult,
  MessageType,
  isChatRoomDaemonResponse
} from '../../../types/shared/ChatRoomTypes';
import { CommandOperation, ChatRoomOperations } from '../../../types/shared/CommandOperationTypes';

// Default room from JSON configuration - first room alphabetically
const DEFAULT_ROOM_ID = 'general';

export class ChatCommand extends DirectCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'chat',
      description: 'Core chat messaging functionality',
      category: 'communication',
      icon: '💬',
      examples: [
        {
          description: 'Send a message to default room',
          command: 'chat --message="Hello world"'
        }
      ],
      parameters: {
        message: { type: 'string', description: 'Message to send' },
        room: { type: 'string', description: 'Chat room ID' }
      }
    };
  }

  protected static async executeOperation(params: any, context?: CommandContext): Promise<CommandResult<ChatCommandResult>> {
    // Parameters are already parsed by DirectCommand.execute()
    
    try {
      if (params.message) {
        // Send message to real ChatRoomDaemon
        const sendRequest: SendMessageRequest = {
          room_id: params.room || DEFAULT_ROOM_ID,
          sender_id: context?.sessionId || 'unknown',
          content: params.message,
          message_type: MessageType.TEXT,
          correlationId: `chat_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
          timestamp: Date.now()
        };
        
        if (context?.sessionId) {
          sendRequest.session_id = context.sessionId;
        }
        
        const result = await this.sendMessageToChatRoom(sendRequest);

        // Also marshal the message for tracking/logging
        const { DataMarshalCommand } = await import('../../core/data-marshal/DataMarshalCommand');
        const marshalResult = await DataMarshalCommand.execute({
          operation: 'encode',
          data: {
            messageId: result.message_id,
            content: params.message,
            room: params.room || DEFAULT_ROOM_ID,
            sender: context?.sessionId || 'unknown',
            timestamp: new Date().toISOString(),
            chatRoomResult: result
          },
          encoding: 'json',
          source: 'chat-command',
          destination: 'chat-system'
        }, context);

        return this.createSuccessResult({
          message: 'Message sent successfully',
          messageId: result.message_id,
          content: params.message,
          room: params.room || DEFAULT_ROOM_ID,
          timestamp: result.message.timestamp.toISOString(),
          marshalId: marshalResult.data?.marshalId,
          chatRoom: result
        });
      }

      // Get real room list from ChatRoomDaemon
      const roomsResult = await this.listChatRooms({
        user_id: context?.sessionId || 'unknown',
        correlationId: `list_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
        timestamp: Date.now()
      });

      return this.createSuccessResult({
        status: 'Chat system ready',
        available_rooms: roomsResult.rooms || [],
        sessionId: context?.sessionId || 'unknown',
        totalRooms: roomsResult.total_count || 0
      });
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Chat command failed: ${errorMessage}`);
    }
  }

  /**
   * Send message to ChatRoomDaemon
   */
  private static async sendMessageToChatRoom(request: SendMessageRequest): Promise<SendMessageResponse> {
    return this.sendChatRoomRequest<SendMessageRequest, SendMessageResponse>(CommandOperation.SEND_MESSAGE, request);
  }

  /**
   * List rooms from ChatRoomDaemon
   */
  private static async listChatRooms(request: ListRoomsRequest): Promise<ListRoomsResponse> {
    return this.sendChatRoomRequest<ListRoomsRequest, ListRoomsResponse>(CommandOperation.LIST_ROOMS, request);
  }

  /**
   * Generic strongly-typed method to send requests to ChatRoomDaemon
   */
  private static async sendChatRoomRequest<TRequest extends { correlationId: string }, TResponse>(
    operation: ChatRoomOperations, 
    requestData: TRequest
  ): Promise<TResponse> {
    try {
      // Use the daemon bus to communicate with ChatRoomDaemon
      const { DAEMON_EVENT_BUS } = await import('../../../daemons/base/DaemonEventBus');
      
      return new Promise<TResponse>((resolve, reject) => {
        const correlationId = requestData.correlationId;
        const timeoutMs = 5000;
        
        // Set up response listener with type validation
        const responseHandler = (response: unknown) => {
          if (isChatRoomDaemonResponse(response) && response.correlationId === correlationId) {
            DAEMON_EVENT_BUS.off('chatroom_response', responseHandler);
            clearTimeout(timeoutHandle);
            
            if (response.success && response.data) {
              resolve(response.data as TResponse);
            } else {
              reject(new Error(response.error || 'ChatRoomDaemon operation failed'));
            }
          }
        };
        
        // Set up timeout
        const timeoutHandle = setTimeout(() => {
          DAEMON_EVENT_BUS.off('chatroom_response', responseHandler);
          reject(new Error(`ChatRoomDaemon request timed out after ${timeoutMs}ms`));
        }, timeoutMs);
        
        // Listen for response
        DAEMON_EVENT_BUS.on('chatroom_response', responseHandler);
        
        // Send strongly-typed request to ChatRoomDaemon
        const request: ChatRoomDaemonRequest = {
          type: 'chatroom_request',
          operation: operation,
          correlationId: correlationId,
          data: requestData as any, // Safe because we control the types
          timestamp: Date.now()
        };
        
        console.log(`💬 ChatCommand: Sending typed request to ChatRoomDaemon:`, request);
        DAEMON_EVENT_BUS.emit('chatroom_request', request);
      });
      
    } catch (error) {
      // No fallback - ChatRoomDaemon must be running for chat functionality
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error('❌ ChatRoomDaemon communication failed:', errorMessage);
      throw new Error(`ChatRoomDaemon unavailable: ${errorMessage}. Ensure ChatRoomDaemon is running with dynamic JSON configuration.`);
    }
  }

}

export default ChatCommand;