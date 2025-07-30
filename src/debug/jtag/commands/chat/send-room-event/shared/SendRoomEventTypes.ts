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

import { ChatParams, ChatResult, createChatParams, createChatResult } from '@commandsChat/shared/ChatTypes';
import type { JTAGContext } from '@shared/JTAGTypes';
import type { JTAGError } from '@shared/ErrorTypes';
import { UUID } from '@shared/CrossPlatformUUID';

export interface SendRoomEventParams extends ChatParams {
  readonly sourceParticipantId: string;
  readonly sourceParticipantType: 'human' | 'ai' | 'system';
  readonly eventType: string;
  readonly eventData: any;
  readonly priority?: 'low' | 'normal' | 'high';
  readonly deliveryOptions?: {
    guaranteedDelivery: boolean;
    deliverToAll: boolean;
    immediateDelivery: boolean;
    batchWithOthers: boolean;
  };
  readonly widgetEventOptions?: {
    stateSnapshot?: any;
    stateDelta?: any;
  };
}

export const createSendRoomEventParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    roomId: string;
    sourceParticipantId?: string;
    sourceParticipantType?: 'human' | 'ai' | 'system';
    eventType?: string;
    eventData?: any;
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
  }
): SendRoomEventParams => ({
  ...createChatParams(context, sessionId, data),
  sourceParticipantId: data.sourceParticipantId ?? '',
  sourceParticipantType: data.sourceParticipantType ?? 'human',
  eventType: data.eventType ?? 'custom_event',
  eventData: data.eventData ?? {},
  priority: data.priority ?? 'normal',
  deliveryOptions: data.deliveryOptions ?? {
    guaranteedDelivery: true,
    deliverToAll: true,
    immediateDelivery: true,
    batchWithOthers: false
  },
  widgetEventOptions: data.widgetEventOptions
});

export interface SendRoomEventResult extends ChatResult {
  readonly eventId: string;
  readonly participants?: string[];
  readonly recipientCount?: number;
  readonly deliveryTime?: number;
  readonly deliveryStatus?: {
    delivered: number;
    failed: number;
    pending: number;
  };
  readonly widgetCoordinationResults?: any;
  readonly academyIntegrationResults?: any;
  readonly eventImpact?: {
    significanceLevel: 'minor' | 'moderate' | 'major';
    participantsReached: number;
  };
}

export const createSendRoomEventResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    roomId: string;
    success: boolean;
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
    error?: JTAGError;
  }
): SendRoomEventResult => ({
  ...createChatResult(context, sessionId, data),
  eventId: data.eventId,
  participants: data.participants,
  recipientCount: data.recipientCount,
  deliveryTime: data.deliveryTime,
  deliveryStatus: data.deliveryStatus,
  widgetCoordinationResults: data.widgetCoordinationResults,
  academyIntegrationResults: data.academyIntegrationResults,
  eventImpact: data.eventImpact
});