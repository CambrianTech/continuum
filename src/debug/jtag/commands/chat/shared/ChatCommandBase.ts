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

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type{ ChatParams, ChatResult } from './ChatTypes';
import { createChatResult } from './ChatTypes';
import { ValidationError } from '../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

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
  protected createChatErrorResult(sessionId: UUID, roomId: string, error: string): TResult {
    return createChatResult(this.context, sessionId, {
      success: false,
      roomId,
      error: new ValidationError('chat', error)
    }) as TResult;
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