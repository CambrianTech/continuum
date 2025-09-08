/**
 * Chat Send Message Types - Universal Message Sending
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { ChatMessage } from '../../../../domain/chat/ChatMessage';

/**
 * Parameters for sending a chat message
 */
export interface ChatSendMessageParams extends CommandParams {
  readonly roomId: UUID;
  readonly content: string;
  readonly mentions?: UUID[];
  readonly category?: 'chat' | 'system' | 'response' | 'notification';
  readonly replyToId?: UUID;
  readonly messageContext?: any;
}

/**
 * Result of sending a chat message
 */
export interface ChatSendMessageResult extends CommandResult {
  readonly success?: boolean;
  readonly messageId?: UUID;
  readonly message?: ChatMessage;
  readonly error?: string;
}

/**
 * Create success result
 */
export function createChatSendMessageResult(
  params: ChatSendMessageParams,
  result: Partial<ChatSendMessageResult>
): ChatSendMessageResult {
  return {
    context: params.context,
    sessionId: params.sessionId,
    success: true,
    ...result
  };
}

/**
 * Create error result
 */
export function createChatSendMessageError(
  params: ChatSendMessageParams,
  error: string
): ChatSendMessageResult {
  return {
    context: params.context,
    sessionId: params.sessionId,
    success: false,
    error
  };
}