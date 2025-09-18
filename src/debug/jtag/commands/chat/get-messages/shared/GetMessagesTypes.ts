/**
 * Get Messages Command Types - STRICT TYPING like Rust
 */

import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

// Bridge API types with command system types
import type { GetMessageHistoryParams, GetMessageHistoryResult } from '../../../../api/commands/chat/ChatCommands';
import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { ChatMessageData } from '../../../../system/data/domains/ChatMessage';

// Command system compatible types
export interface GetMessagesParams extends CommandParams, GetMessageHistoryParams {}

export interface GetMessagesResult extends CommandResult {
  readonly success: boolean;
  readonly messages: ChatMessageData[];
  readonly roomId: string;
  readonly totalCount?: number;
  readonly hasMore: boolean;
  readonly error?: string;
}

export type MessageData = ChatMessageData;

// Factory functions for type-safe result creation  
export function createGetMessagesSuccess(
  params: GetMessagesParams,
  messages: ChatMessageData[],
  hasMore: boolean = false,
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

export function createGetMessagesError(
  params: GetMessagesParams,
  error: string
): GetMessagesResult {
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