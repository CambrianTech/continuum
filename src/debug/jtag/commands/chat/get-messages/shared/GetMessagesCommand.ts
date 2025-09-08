/**
 * Get Messages Command - Shared Base (Environment-Agnostic)
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { GetMessagesParams, GetMessagesResult, MessageData } from './GetMessagesTypes';

export abstract class GetMessagesCommand extends CommandBase<GetMessagesParams, GetMessagesResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('chat-get-messages', context, subpath, commander);
  }

  /**
   * Execute message retrieval - environment-agnostic
   */
  async execute(params: GetMessagesParams): Promise<GetMessagesResult> {
    const getParams = params;
    const startTime = Date.now();
    
    console.log(`📚 ${this.getEnvironmentLabel()}: Getting messages for room ${getParams.roomId}`);

    try {
      // Validate parameters
      if (!getParams.roomId || typeof getParams.roomId !== 'string') {
        return this.createError(getParams, 'roomId is required and must be a string');
      }

      const limit = Math.min(getParams.limit || 50, 100); // Max 100 messages
      
      // Get messages from storage layer
      const messages = await this.retrieveMessages(getParams, limit);
      
      // Check for pagination
      const hasMore = messages.length === limit;
      const nextCursor = hasMore ? messages[messages.length - 1]?.id : undefined;
      
      const duration = Date.now() - startTime;
      console.log(`✅ ${this.getEnvironmentLabel()}: Retrieved ${messages.length} messages in ${duration}ms`);

      return this.createSuccess(getParams, [...messages], hasMore, messages.length);

    } catch (error) {
      console.error(`❌ ${this.getEnvironmentLabel()}: Failed to get messages:`, error);
      
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      return this.createError(getParams, errorMessage);
    }
  }

  /**
   * Retrieve messages from storage - implemented per environment
   */
  protected abstract retrieveMessages(
    params: GetMessagesParams, 
    limit: number
  ): Promise<MessageData[]>;

  /**
   * Get environment label for logging
   */
  protected abstract getEnvironmentLabel(): string;

  /**
   * Type-safe success result creation
   */
  protected createSuccess(
    params: GetMessagesParams,
    messages: MessageData[],
    hasMore: boolean,
    totalCount?: number
  ): GetMessagesResult {
    return {
      context: params.context,
      sessionId: params.sessionId,
      success: true,
      messages,
      roomId: params.roomId,
      hasMore,
      totalCount
    };
  }

  /**
   * Type-safe error result creation
   */
  protected createError(params: GetMessagesParams, error: string): GetMessagesResult {
    return {
      context: params.context,
      sessionId: params.sessionId,
      success: false,
      messages: [],
      roomId: params.roomId,
      hasMore: false,
      error
    };
  }
}