/**
 * Chat Poll Command Types - Get messages after a specific messageId
 *
 * Simple command for conversational research workflow:
 * 1. Send a question and get messageId
 * 2. Wait for responses (sleep)
 * 3. Poll for all messages after your question
 */

import type { JTAGContext, CommandParams, JTAGPayload, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { ChatMessageEntity } from '@system/data/entities/ChatMessageEntity';
import { Commands } from '../../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';

/**
 * Chat poll parameters
 */
export interface ChatPollParams extends CommandParams {
  readonly context: JTAGContext;
  readonly sessionId: UUID;

  // Message ID to poll from (returns all messages after this one)
  readonly afterMessageId: UUID;

  // Optional: limit number of messages returned
  readonly limit?: number;

  // Optional: room filter (accepts either roomId or room name)
  readonly roomId?: UUID;
  readonly room?: string;
}

/**
 * Chat poll result
 */
export interface ChatPollResult extends JTAGPayload {
  readonly context: JTAGContext;
  readonly sessionId: UUID;
  readonly success: boolean;
  readonly messages: ReadonlyArray<ChatMessageEntity>;
  readonly count: number;
  readonly afterMessageId: UUID;
  readonly timestamp: string;
  readonly error?: string;
}

/**
 * ChatPoll — Type-safe command executor
 *
 * Usage:
 *   import { ChatPoll } from '...shared/ChatPollTypes';
 *   const result = await ChatPoll.execute({ ... });
 */
export const ChatPoll = {
  execute(params: CommandInput<ChatPollParams>): Promise<ChatPollResult> {
    return Commands.execute<ChatPollParams, ChatPollResult>('collaboration/chat/poll', params as Partial<ChatPollParams>);
  },
  commandName: 'collaboration/chat/poll' as const,
} as const;

/**
 * Factory function for creating CollaborationChatPollParams
 */
export const createCollaborationChatPollParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ChatPollParams, 'context' | 'sessionId' | 'userId'>
): ChatPollParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating CollaborationChatPollResult with defaults
 */
export const createCollaborationChatPollResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ChatPollResult, 'context' | 'sessionId' | 'userId'>
): ChatPollResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart collaboration/chat/poll-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCollaborationChatPollResultFromParams = (
  params: ChatPollParams,
  differences: Omit<ChatPollResult, 'context' | 'sessionId' | 'userId'>
): ChatPollResult => transformPayload(params, differences);

