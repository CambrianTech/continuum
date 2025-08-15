/**
 * Chat Daemon Response Types
 * 
 * Response types and factories specific to chat daemon operations.
 * Co-located with chat daemon to maintain proper module boundaries.
 */

import { type JTAGContext, createPayload } from '../../../system/core/types/JTAGTypes';
import { type BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

// Chat daemon response types
export interface ChatSuccessResponse extends BaseResponsePayload {
  data: unknown; // Specific chat results (message, room, participant list, etc.)
}

export interface ChatErrorResponse extends BaseResponsePayload {
  error: string;
  operation?: string;
}

export const createChatSuccessResponse = (
  data: unknown,
  context: JTAGContext,
  sessionId: UUID
): ChatSuccessResponse => createPayload(context, sessionId, {
  success: true,
  timestamp: new Date().toISOString(),
  data
});

export const createChatErrorResponse = (
  error: string,
  context: JTAGContext,
  operation: string | undefined,
  sessionId: UUID
): ChatErrorResponse => createPayload(context, sessionId, {
  success: false,
  timestamp: new Date().toISOString(),
  error,
  operation
});

// Union type for chat daemon responses
export type ChatResponse = ChatSuccessResponse | ChatErrorResponse;