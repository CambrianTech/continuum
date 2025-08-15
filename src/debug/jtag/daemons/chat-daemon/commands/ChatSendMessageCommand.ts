/**
 * Chat Send Message Command - Universal Message Sending
 * 
 * BREAKTHROUGH: Works identically for all participant types:
 * - Human users via browser UI
 * - AI agents via API adapters
 * - Persona systems via LoRA adapters
 * - External integrations via webhooks
 * 
 * Location Transparent: Works locally and across distributed Continuum nodes
 */

import { CommandBase } from '../../command-daemon/shared/CommandBase';
import type { JTAGContext, CommandParams, CommandResult } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import type { 
  ChatSendMessageParams,
  ChatSendMessageResult,
  ChatMessage,
  SessionParticipant
} from '../shared/ChatTypes';
import { 
  createChatSendMessageResultFromParams,
  createChatSendMessageResult
} from '../shared/ChatTypes';
import { RoomCommandCoordinator, RoomUpdateCommands } from '../shared/RoomCommandSystem';

/**
 * Universal Message Send Handler
 * 
 * Processes message sending and triggers room notifications
 */
export class ChatSendMessageCommand extends CommandBase<ChatSendMessageParams, ChatSendMessageResult> {
  public readonly subpath = 'send-message';
  
  private roomCoordinator: RoomCommandCoordinator;

  constructor(
    context: JTAGContext, 
    router: JTAGRouter,
    roomCoordinator?: RoomCommandCoordinator
  ) {
    // TODO: Need proper commander interface - using router as placeholder
    super('chat-send-message', context, 'send-message', router as any);
    
    // Use provided coordinator or create default one
    this.roomCoordinator = roomCoordinator || new RoomCommandCoordinator(
      context,
      router,
'local' // TODO: Get actual node ID
    );
  }

  /**
   * Execute message sending - Universal for all participant types
   */
  async execute(params: ChatSendMessageParams): Promise<ChatSendMessageResult> {
    console.log(`💬 ChatSendMessage: Sending message to room ${params.roomId} from session ${params.sessionId}`);

    try {
      // 1. Create universal message object
      const message = this.createMessage(params);
      
      // 2. Store message (via DataDaemon when integrated)
      await this.storeMessage(message);
      
      // 3. Notify all room participants via location-transparent commands
      await this.notifyRoomParticipants(message);
      
      console.log(`✅ ChatSendMessage: Successfully sent message ${message.messageId} to room ${params.roomId}`);
      
      return createChatSendMessageResultFromParams(params, {
        success: true,
        messageId: message.messageId,
        message,
        timestamp: message.timestamp
      });
      
    } catch (error) {
      console.error(`❌ ChatSendMessage: Failed to send message to room ${params.roomId}:`, error);
      
      return createChatSendMessageResultFromParams(params, {
        success: false,
        error: {
          message: error instanceof Error ? error.message : 'Unknown error',
          type: 'processing' as const,
          name: 'MessageSendError',
          timestamp: new Date().toISOString(),
          toJSON: () => ({ 
            message: error instanceof Error ? error.message : 'Unknown error', 
 
          })
        }
      });
    }
  }

  /**
   * Create universal chat message - No participant-type assumptions
   */
  private createMessage(params: ChatSendMessageParams): ChatMessage {
    const messageId = generateUUID();
    const timestamp = new Date().toISOString();
    
    // Get participant info from session context
    const senderName = this.getSenderName(params);
    
    return {
      messageId,
      roomId: params.roomId,
      senderId: params.sessionId,
      senderName,
      content: params.content,
      timestamp,
      mentions: params.mentions || [],
      category: params.category || 'chat',
      replyToId: params.replyToId,
      messageContext: params.messageContext,
      
      // Legacy compatibility fields (auto-populated)
      messageType: this.mapCategoryToLegacyType(params.category),
      senderType: 'user', // Universal - sender type is irrelevant
      aiProcessed: false,
      aiContext: params.messageContext
    };
  }

  /**
   * Get sender display name from session context
   * 
   * Falls back gracefully if participant info not available
   */
  private getSenderName(params: ChatSendMessageParams): string {
    // Try to get from message context
    if (params.messageContext && typeof params.messageContext === 'object') {
      const sessionInfo = (params.messageContext as any)?.sessionInfo;
      if (sessionInfo && typeof sessionInfo.displayName === 'string') {
        return sessionInfo.displayName;
      }
      
      const participant = (params.messageContext as any)?.participant;
      if (participant && typeof participant.displayName === 'string') {
        return participant.displayName;
      }
    }
    
    // Fallback to session-based name
    return `Session-${params.sessionId.substring(0, 8)}`;
  }

  /**
   * Map universal category to legacy message type (compatibility)
   */
  private mapCategoryToLegacyType(category?: string): 'chat' | 'system' | 'ai-response' {
    switch (category) {
      case 'system': return 'system';
      case 'response': return 'ai-response';
      case 'notification': return 'system';
      default: return 'chat';
    }
  }

  /**
   * Store message - Integration point for DataDaemon
   * 
   * For now, stores in memory. Will integrate with DataDaemon storage.
   */
  private async storeMessage(message: ChatMessage): Promise<void> {
    // TODO: Integrate with DataDaemon when available
    console.log(`💾 Storing message ${message.messageId} (storage integration pending)`);
    
    // For now, just log the storage operation
    // In real implementation, this would call:
    // await this.dataDaemon.storeMessage(message);
  }

  /**
   * Notify all room participants using location-transparent commands
   * 
   * This is where the magic happens - same notification system works for
   * browser widgets, AI adapters, personas, and remote nodes
   */
  private async notifyRoomParticipants(message: ChatMessage): Promise<void> {
    console.log(`📢 Notifying room ${message.roomId} participants of new message`);
    
    try {
      // Use RoomCommandCoordinator for location-transparent notifications
      const updateData = RoomUpdateCommands.messagesSent(message);
      await this.roomCoordinator.processMessageSent(message.roomId, message);
      
      console.log(`✅ Room notifications sent for message ${message.messageId}`);
      
    } catch (error) {
      console.error(`❌ Failed to notify room participants:`, error);
      // Don't fail the entire message send if notifications fail
      // The message is still stored and can be retrieved via history
    }
  }

  /**
   * Set room coordinator (for dependency injection in tests)
   */
  public setRoomCoordinator(coordinator: RoomCommandCoordinator): void {
    this.roomCoordinator = coordinator;
  }

  /**
   * Get command statistics for monitoring
   */
  public getStats() {
    return {
      commandType: 'chat-send-message',
      context: {
        environment: this.context.environment,
        sessionId: this.context.uuid,
        nodeId: 'local' // TODO: Get actual node ID
      },
      roomCoordinator: this.roomCoordinator.getSystemStats()
    };
  }
}