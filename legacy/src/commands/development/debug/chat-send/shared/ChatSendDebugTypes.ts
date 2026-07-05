/**
 * Chat Send Debug Command Types
 *
 * Triggers chat widget to send a message for testing event flow
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';

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
/**
 * ChatSendDebug — Type-safe command executor
 *
 * Usage:
 *   import { ChatSendDebug } from '...shared/ChatSendDebugTypes';
 *   const result = await ChatSendDebug.execute({ ... });
 */
export const ChatSendDebug = {
  execute(params: CommandInput<ChatSendDebugParams>): Promise<ChatSendDebugResult> {
    return Commands.execute<ChatSendDebugParams, ChatSendDebugResult>('development/debug/chat-send', params as Partial<ChatSendDebugParams>);
  },
  commandName: 'development/debug/chat-send' as const,
} as const;
