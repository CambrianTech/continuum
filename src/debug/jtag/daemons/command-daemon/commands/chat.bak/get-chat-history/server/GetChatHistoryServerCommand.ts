import { GetChatHistoryCommand } from '../shared/GetChatHistoryCommand';
import { GetChatHistoryParams, GetChatHistoryResult } from '../shared/GetChatHistoryTypes';
import type { ICommandDaemon } from '../../../../shared/CommandBase';
import type { JTAGContext } from '../../../../../../shared/JTAGTypes';

export class GetChatHistoryServerCommand extends GetChatHistoryCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  async execute(params: GetChatHistoryParams): Promise<GetChatHistoryResult> {
    try {
      console.log(`📜 SERVER: Getting chat history for room ${params.roomId}`);

      // Validate room access
      const hasAccess = await this.validateRoomAccess(params);
      if (!hasAccess) {
        return new GetChatHistoryResult({
          roomId: params.roomId,
          success: false,
          error: 'Access denied to room'
        });
      }

      // Get messages from data store
      const messages = await this.retrieveMessagesFromStore(params);

      // Apply filters if specified
      const filteredMessages = await this.applyFilters(messages, params);

      // Format messages for response
      const formattedMessages = await this.formatMessages(filteredMessages, params);

      // Update persona context if needed
      if (params.personaIntegration?.updatePersonaContext) {
        await this.updatePersonaContext(params, formattedMessages);
      }

      return new GetChatHistoryResult({
        roomId: params.roomId,
        success: true,
        messages: formattedMessages,
        totalMessages: messages.length,
        filteredMessages: filteredMessages.length,
        requestedCount: params.maxMessages,
        actualCount: formattedMessages.length,
        hasMoreMessages: await this.hasMoreMessages(params, messages.length),
        oldestMessageTimestamp: formattedMessages.length > 0 ? formattedMessages[formattedMessages.length - 1].timestamp : 0,
        newestMessageTimestamp: formattedMessages.length > 0 ? formattedMessages[0].timestamp : 0,
        queryTime: Date.now() - this.startTime
      });

    } catch (error) {
      console.error(`❌ SERVER: Get chat history error:`, error);
      return new GetChatHistoryResult({
        roomId: params.roomId,
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }

  private async validateRoomAccess(params: GetChatHistoryParams): Promise<boolean> {
    try {
      const accessResult = await this.commander.routeCommand({
        command: 'chat/validate-room-access',
        params: {
          participantId: params.requesterId,
          participantType: params.requesterType,
          roomId: params.roomId,
          requiredPermissions: ['read_messages']
        }
      });

      return accessResult.success && accessResult.hasAccess;
    } catch (error) {
      console.warn(`⚠️ SERVER: Room access validation failed:`, error);
      return false;
    }
  }

  private async retrieveMessagesFromStore(params: GetChatHistoryParams): Promise<any[]> {
    // Simulate database query - would be replaced with actual data store
    const mockMessages = [
      {
        messageId: 'msg_1',
        roomId: params.roomId,
        content: 'Welcome to the chat room!',
        senderId: 'system',
        senderType: 'system_agent',
        messageType: 'system',
        timestamp: Date.now() - 3600000, // 1 hour ago
        academyContext: null,
        deliveryStatus: 'delivered',
        metadata: {}
      },
      {
        messageId: 'msg_2',
        roomId: params.roomId,
        content: 'Hello everyone!',
        senderId: 'user_1',
        senderType: 'human',
        messageType: 'text',
        timestamp: Date.now() - 1800000, // 30 minutes ago
        academyContext: null,
        deliveryStatus: 'delivered',
        metadata: {}
      }
    ];

    // Apply basic ordering and limits
    let messages = mockMessages
      .filter(msg => msg.roomId === params.roomId)
      .sort((a, b) => b.timestamp - a.timestamp); // Newest first

    if (params.maxMessages) {
      messages = messages.slice(0, params.maxMessages);
    }

    return messages;
  }

  private async applyFilters(messages: any[], params: GetChatHistoryParams): Promise<any[]> {
    let filtered = messages;

    // Apply time filters
    if (params.filters?.sinceTimestamp) {
      filtered = filtered.filter(msg => msg.timestamp >= params.filters.sinceTimestamp);
    }

    if (params.filters?.beforeTimestamp) {
      filtered = filtered.filter(msg => msg.timestamp <= params.filters.beforeTimestamp);
    }

    // Apply sender filters
    if (params.filters?.senderIds && params.filters.senderIds.length > 0) {
      filtered = filtered.filter(msg => params.filters.senderIds.includes(msg.senderId));
    }

    if (params.filters?.senderTypes && params.filters.senderTypes.length > 0) {
      filtered = filtered.filter(msg => params.filters.senderTypes.includes(msg.senderType));
    }

    // Apply message type filters
    if (params.filters?.messageTypes && params.filters.messageTypes.length > 0) {
      filtered = filtered.filter(msg => params.filters.messageTypes.includes(msg.messageType));
    }

    // Apply content filters
    if (params.filters?.contentKeywords && params.filters.contentKeywords.length > 0) {
      filtered = filtered.filter(msg => 
        params.filters.contentKeywords.some(keyword => 
          msg.content.toLowerCase().includes(keyword.toLowerCase())
        )
      );
    }

    return filtered;
  }

  private async formatMessages(messages: any[], params: GetChatHistoryParams): Promise<any[]> {
    return messages.map(msg => ({
      ...msg,
      // Add any formatting specific to the request
      formattedContent: params.responseOptions?.includeFormatting ? 
        this.formatMessageContent(msg.content) : msg.content,
      // Add persona-specific information if needed
      personaRelevance: params.personaIntegration ? 
        this.calculatePersonaRelevance(msg, params.personaIntegration) : undefined
    }));
  }

  private formatMessageContent(content: string): string {
    // Simple formatting - could be enhanced
    return content.replace(/\n/g, '<br>');
  }

  private calculatePersonaRelevance(message: any, personaIntegration: any): number {
    // Calculate relevance score for persona - simplified version
    let relevance = 0.5; // Default neutral relevance

    // Higher relevance for messages addressed to the persona
    if (message.content.includes(`@${personaIntegration.personaId}`)) {
      relevance += 0.3;
    }

    // Higher relevance for messages from interesting senders
    if (message.senderType === 'human') {
      relevance += 0.1;
    }

    return Math.min(1.0, relevance);
  }

  private async hasMoreMessages(params: GetChatHistoryParams, totalFound: number): Promise<boolean> {
    // Check if there are more messages beyond what was returned
    return totalFound >= (params.maxMessages || 50);
  }

  private async updatePersonaContext(params: GetChatHistoryParams, messages: any[]): Promise<void> {
    try {
      if (!params.personaIntegration?.personaId) return;

      await this.commander.routeCommand({
        command: 'ai-provider/update-persona-context',
        params: {
          personaId: params.personaIntegration.personaId,
          roomId: params.roomId,
          contextType: 'chat_history',
          contextData: {
            messages: messages,
            messageCount: messages.length,
            timeRange: {
              oldest: messages.length > 0 ? messages[messages.length - 1].timestamp : 0,
              newest: messages.length > 0 ? messages[0].timestamp : 0
            }
          }
        }
      });
    } catch (error) {
      console.warn(`⚠️ SERVER: Persona context update failed:`, error);
    }
  }

  private startTime = Date.now();
}