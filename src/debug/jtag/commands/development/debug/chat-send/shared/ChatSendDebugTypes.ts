/**
 * Chat Send Debug Command Types
 *
 * Triggers chat widget to send a message for testing event flow
 */

import type { CommandParams, CommandResult, JTAGContext } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface ChatSendDebugParams extends CommandParams {
  message: string;
  roomId?: string;
}

export interface ChatSendDebugResult extends CommandResult {
  success: boolean;
  sent: boolean;
  messageId?: string;
  error?: string;
}

export function createChatSendResult(
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ChatSendDebugResult, 'context' | 'sessionId'>
): ChatSendDebugResult {
  return {
    context,
    sessionId,
    ...data
  };
}