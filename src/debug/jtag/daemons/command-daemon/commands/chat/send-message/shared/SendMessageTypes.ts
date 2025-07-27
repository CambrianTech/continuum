// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Send Message Types - Clean Direct Inheritance
 * 
 * INHERITANCE CHAIN:
 * ChatParams → SendMessageParams (adds content, senderId, messageType)
 * ChatResult → SendMessageResult (adds messageId, deliveredAt)
 */

import { ChatParams, ChatResult } from '@chatShared/ChatTypes';
import type { JTAGContext } from '@shared/JTAGTypes';

export class SendMessageParams extends ChatParams {
  content!: string;
  senderId?: string;
  messageType?: 'text' | 'system' | 'notification';

  constructor(data: Partial<SendMessageParams> = {}, context: JTAGContext, sessionId: string) {
    super(data, context, sessionId);
    this.content = data.content ?? '';
    this.senderId = data.senderId;
    this.messageType = data.messageType ?? 'text';
  }
}

export class SendMessageResult extends ChatResult {
  messageId!: string;
  deliveredAt?: string;

  constructor(data: Partial<SendMessageResult> & { messageId: string; roomId: string }, context: JTAGContext, sessionId: string) {
    super(data, context, sessionId);
    this.messageId = data.messageId;
    this.deliveredAt = data.deliveredAt;
  }
}