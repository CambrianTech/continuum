/**
 * Chat Send Message Types - Universal Message Sending
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

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
  readonly success: boolean;
  readonly messageId?: UUID;
  readonly timestamp?: string;
  readonly message?: {
    messageId: UUID;
    roomId: UUID;
    content: string;
    senderName: string;
    timestamp: string;
  };
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
    success: true,
    context: params.context,
    sessionId: params.sessionId,
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
    success: false,
    context: params.context,
    sessionId: params.sessionId,
    error
  };
}