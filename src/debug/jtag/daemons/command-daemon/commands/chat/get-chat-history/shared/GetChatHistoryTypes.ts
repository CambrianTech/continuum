// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Get Chat History Types - Clean Direct Inheritance
 * 
 * INHERITANCE CHAIN:
 * ChatParams → GetChatHistoryParams (adds participantId, maxMessages, etc.)
 * ChatResult → GetChatHistoryResult (adds messages, totalCount)
 */

import { ChatParams, ChatResult } from '@chatShared/ChatTypes';
import type { JTAGContext } from '@shared/JTAGTypes';
import { createPayload } from '@shared/JTAGTypes';
import { UUID } from 'crypto';

export interface GetChatHistoryParams extends ChatParams {
  readonly participantId?: string;
  readonly maxMessages: number;
  readonly hoursBack: number;
  readonly includeMetadata: boolean;
}

export const createGetChatHistoryParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<GetChatHistoryParams>, 'context' | 'sessionId'>
): GetChatHistoryParams => createPayload(context, sessionId, {
  participantId: data.participantId,
  maxMessages: data.maxMessages ?? 50,
  hoursBack: data.hoursBack ?? 24,
  includeMetadata: data.includeMetadata ?? false,
  ...data
});

export interface GetChatHistoryResult extends ChatResult {
  readonly messages: readonly ChatMessage[];
  readonly totalCount: number;
}

export const createGetChatHistoryResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<GetChatHistoryResult>, 'context' | 'sessionId'> & { roomId: string }
): GetChatHistoryResult => createPayload(context, sessionId, {
  messages: Object.freeze([...(data.messages ?? [])]),
  totalCount: data.totalCount ?? 0,
  ...data
});

export interface ChatMessage {
  readonly id: string;
  readonly content: string;
  readonly senderId: string;
  readonly senderType: 'human' | 'ai' | 'system';
  readonly timestamp: string; // ISO 8601 format
  readonly roomId: string;
}