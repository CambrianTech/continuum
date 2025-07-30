// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Send Message Types - Clean Direct Inheritance
 * 
 * INHERITANCE CHAIN:
 * ChatParams → SendMessageParams (adds content, senderId, messageType)
 * ChatResult → SendMessageResult (adds messageId, deliveredAt)
 */

import { ChatParams, ChatResult } from '@commandsChat/shared/ChatTypes';
import type { JTAGContext } from '@shared/JTAGTypes';
import { createPayload } from '@shared/JTAGTypes';
import type { UUID } from '@shared/CrossPlatformUUID';

export interface SendMessageParams extends ChatParams {
  readonly content: string;
  readonly senderId?: string;
  readonly messageType?: 'text' | 'system' | 'notification';
}

export const createSendMessageParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<SendMessageParams>, 'context' | 'sessionId'>
): SendMessageParams => createPayload(context, sessionId, {
  roomId: data.roomId ?? '',
  nodeId: data.nodeId,
  content: data.content ?? '',
  messageType: data.messageType ?? 'text',
  ...data
});

export interface SendMessageResult extends ChatResult {
  readonly messageId: string;
  readonly deliveredAt?: string;
}

export const createSendMessageResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<SendMessageResult>, 'context' | 'sessionId'> & {
    success: boolean;
    roomId: string;
    messageId: string;
  }
): SendMessageResult => createPayload(context, sessionId, {
  timestamp: new Date().toISOString(),
  ...data
});