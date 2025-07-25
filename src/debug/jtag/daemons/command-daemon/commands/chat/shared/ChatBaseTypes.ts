// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Chat Base Types - Granular Base Classes for Maximum Reuse
 * 
 * Creates specific base types for common chat patterns to reduce redundancy.
 * Each base type captures a specific pattern used across multiple commands.
 * 
 * PATTERN HIERARCHY:
 * ChatParams → ChatParticipantParams → specific command params
 * ChatResult → ChatEntityResult → specific command results
 */

import { ChatParams, ChatResult } from './ChatTypes';

/**
 * Base for commands that involve a participant (sender/receiver)
 */
export abstract class ChatParticipantParams extends ChatParams {
  participantId!: string;
  participantType?: 'human' | 'ai' | 'system';

  constructor(data: Partial<ChatParticipantParams> = {}) {
    super({ participantId: '', participantType: 'human', ...data });
  }
}

/**
 * Base for results that create/reference a specific entity (message, event, etc.)
 */
export abstract class ChatEntityResult extends ChatResult {
  entityId!: string;
  entityType!: string;

  constructor(data: Partial<ChatEntityResult> & { entityId: string; entityType: string; roomId: string }) {
    super(data);
    this.entityId = data.entityId;
    this.entityType = data.entityType;
  }
}

/**
 * Base for results that involve delivery/timing
 */
export abstract class ChatDeliveryResult extends ChatEntityResult {
  deliveredAt?: string;
  deliveryStatus?: 'pending' | 'delivered' | 'failed';

  constructor(data: Partial<ChatDeliveryResult> & { entityId: string; entityType: string; roomId: string }) {
    super(data);
    Object.assign(this, {
      deliveredAt: data.deliveredAt,
      deliveryStatus: data.deliveryStatus || 'delivered',
    });
  }
}

/**
 * Base for params that include content
 */
export abstract class ChatContentParams extends ChatParticipantParams {
  content!: string;
  contentType?: 'text' | 'markdown' | 'html';

  constructor(data: Partial<ChatContentParams> = {}) {
    super(data);
    Object.assign(this, {
      content: '',
      contentType: 'text',
      ...data
    });
  }
}