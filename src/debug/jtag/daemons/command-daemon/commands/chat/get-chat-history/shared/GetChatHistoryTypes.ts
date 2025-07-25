// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Get Chat History Types - Clean Direct Inheritance
 * 
 * INHERITANCE CHAIN:
 * ChatParams → GetChatHistoryParams (adds participantId, maxMessages, etc.)
 * ChatResult → GetChatHistoryResult (adds messages, totalCount)
 */

import { ChatParams, ChatResult } from '@chatShared/ChatTypes';

export class GetChatHistoryParams extends ChatParams {
  readonly participantId?: string;
  readonly maxMessages!: number;
  readonly hoursBack!: number;
  readonly includeMetadata!: boolean;

  constructor(data: Partial<GetChatHistoryParams> = {}) {
    super(data);
    // Elegant spread with destructuring defaults
    const { participantId, maxMessages = 50, hoursBack = 24, includeMetadata = false } = data;
    Object.assign(this, { participantId, maxMessages, hoursBack, includeMetadata });
  }
}

export class GetChatHistoryResult extends ChatResult {
  readonly messages!: readonly ChatMessage[];
  readonly totalCount!: number;

  constructor(data: Partial<GetChatHistoryResult> & { roomId: string }) {
    super(data);
    // Elegant destructuring with frozen immutable arrays
    const { messages = [], totalCount = 0 } = data;
    Object.assign(this, { 
      messages: Object.freeze([...messages]), 
      totalCount 
    });
  }
}

export interface ChatMessage {
  readonly id: string;
  readonly content: string;
  readonly senderId: string;
  readonly senderType: 'human' | 'ai' | 'system';
  readonly timestamp: string; // ISO 8601 format
  readonly roomId: string;
}