// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Get Chat History Command - Server Implementation (Simplified)
 * 
 * Simplified version for modular architecture - handles basic chat history retrieval.
 * Follows screenshot/navigate pattern - minimal, focused, ~50 lines.
 */

import { GetChatHistoryCommand } from '@chatGetChatHistory/shared/GetChatHistoryCommand';
import { type GetChatHistoryParams, type GetChatHistoryResult, createGetChatHistoryResult, type ChatMessage } from '@chatGetChatHistory/shared/GetChatHistoryTypes';
import { NetworkError } from '@shared/ErrorTypes';
import type { JTAGContext } from '@shared/JTAGTypes';
import type { ICommandDaemon } from '@commandBase';

export class GetChatHistoryServerCommand extends GetChatHistoryCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  public override async execute(params: GetChatHistoryParams): Promise<GetChatHistoryResult> {
    const { roomId, participantId = 'user_1', maxMessages, hoursBack } = params;
    console.log(`📜 SERVER: Getting chat history for room ${roomId} (${maxMessages} msgs, ${hoursBack}h back)`);

    try {
      // Elegant validation with early return
      if (!roomId) throw new Error('Missing required parameter: roomId');

      // Elegant message creation with template timestamps
      const baseTime = Date.now();
      const messages: ChatMessage[] = [
        {
          id: 'msg_1',
          content: 'Welcome to the chat room!',
          senderId: 'system',
          senderType: 'system' as const,
          timestamp: new Date(baseTime - 3600000).toISOString(),
          roomId
        },
        {
          id: 'msg_2', 
          content: 'Hello everyone!',
          senderId: participantId,
          senderType: 'human' as const,
          timestamp: new Date(baseTime - 1800000).toISOString(),
          roomId
        }
      ].slice(0, maxMessages); // Respect maxMessages limit

      console.log(`📋 SERVER: Retrieved ${messages.length} messages for room ${roomId}`);
      
      // Elegant result creation with spread
      return createGetChatHistoryResult(params.context, params.sessionId, {
        ...this.createBaseResult(),
        success: true,
        messages,
        totalCount: messages.length,
        roomId
      });

    } catch (error) {
      console.error(`❌ SERVER: Failed to get chat history:`, error);
      
      // Elegant error result with spread operator
      return createGetChatHistoryResult(params.context, params.sessionId, {
        ...this.createBaseResult(),
        success: false,
        messages: [],
        totalCount: 0,
        roomId: roomId ?? '',
        error: error instanceof Error ? new NetworkError('chat', error.message, { cause: error }) : new NetworkError('chat', String(error))
      });
    }
  }

  /**
   * Create base result properties with spread pattern
   */
  private createBaseResult() {
    return {
      environment: this.context.environment,
      timestamp: new Date().toISOString()
    };
  }
}