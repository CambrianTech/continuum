// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Chat Command Base Class
 * 
 * Base class for all chat-related commands to reduce duplication.
 * Provides common chat patterns and functionality.
 * 
 * SHARED CHAT PATTERNS:
 * ✅ roomId validation and handling
 * ✅ Standard chat result formatting
 * ✅ Common error handling patterns
 * ✅ Participant identification helpers
 * ✅ Browser/server delegation patterns
 */

import { CommandBase, type ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { ChatParams, ChatResult } from '@chatShared/ChatTypes';

/**
 * Base class for all chat commands
 */
export abstract class ChatCommandBase<
  TParams extends ChatParams = ChatParams,
  TResult extends ChatResult = ChatResult
> extends CommandBase<TParams, TResult> {

  constructor(commandName: string, context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(commandName, context, subpath, commander);
  }

  /**
   * Validate common chat parameters
   */
  protected validateChatParams(params: TParams): void {
    if (!params.roomId) {
      throw new Error('Missing required parameter: roomId');
    }
  }

  /**
   * Create standardized chat error result
   */
  protected createChatErrorResult(roomId: string, error: string): TResult {
    return {
      success: false,
      roomId,
      environment: this.context.environment,
      timestamp: new Date().toISOString(),
      error
    } as TResult;
  }

  /**
   * Log chat operation with consistent formatting
   */
  protected logChatOperation(operation: string, roomId: string, details?: string): void {
    const emoji = this.context.environment === 'browser' ? '💬' : '🏠';
    const env = this.context.environment.toUpperCase();
    console.log(`${emoji} ${env}: ${operation} for room ${roomId}${details ? ` - ${details}` : ''}`);
  }
}