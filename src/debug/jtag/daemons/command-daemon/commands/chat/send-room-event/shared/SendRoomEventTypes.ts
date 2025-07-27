// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Send Room Event Command - Shared Types (Simplified)
 * 
 * Minimal, focused types for room event sending only.
 * Follows screenshot/navigate pattern - simple params and results.
 * 
 * DESIGN PRINCIPLES:
 * ✅ Single responsibility - only room event sending
 * ✅ Clean parameter interface with sensible defaults
 * ✅ Object.assign() constructor pattern
 * ✅ No over-engineering or god objects
 * ✅ Focused scope - just basic room event coordination
 */

import { ChatParams, ChatResult } from '@chatShared/ChatTypes';
import type { JTAGContext } from '@shared/JTAGTypes';

export class SendRoomEventParams extends ChatParams {
  sourceParticipantId!: string;
  sourceParticipantType!: 'human' | 'ai' | 'system';
  eventType!: string;
  eventData!: any;
  priority?: 'low' | 'normal' | 'high';
  deliveryOptions?: {
    guaranteedDelivery: boolean;
    deliverToAll: boolean;
    immediateDelivery: boolean;
    batchWithOthers: boolean;
  };
  widgetEventOptions?: {
    stateSnapshot?: any;
    stateDelta?: any;
  };

  constructor(data: Partial<SendRoomEventParams> = {}, context: JTAGContext, sessionId: string) {
    super(data, context, sessionId);
    Object.assign(this, {
      sourceParticipantId: '',
      sourceParticipantType: 'human',
      eventType: 'custom_event',
      eventData: {},
      priority: 'normal',
      deliveryOptions: {
        guaranteedDelivery: true,
        deliverToAll: true,
        immediateDelivery: true,
        batchWithOthers: false
      },
      ...data
    });
  }
}

export class SendRoomEventResult extends ChatResult {
  eventId: string;
  participants?: string[];
  recipientCount?: number;
  deliveryTime?: number;
  deliveryStatus?: {
    delivered: number;
    failed: number;
    pending: number;
  };
  widgetCoordinationResults?: any;
  academyIntegrationResults?: any;
  eventImpact?: {
    significanceLevel: 'minor' | 'moderate' | 'major';
    participantsReached: number;
  };

  constructor(data: Partial<SendRoomEventResult> & { eventId: string; roomId: string }, context: JTAGContext, sessionId: string) {
    super(data, context, sessionId);
    this.eventId = data.eventId;
    this.participants = data.participants;
    this.recipientCount = data.recipientCount;
    this.deliveryTime = data.deliveryTime;
    this.deliveryStatus = data.deliveryStatus;
    this.widgetCoordinationResults = data.widgetCoordinationResults;
    this.academyIntegrationResults = data.academyIntegrationResults;
    this.eventImpact = data.eventImpact;
  }
}