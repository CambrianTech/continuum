// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Receive Events Types - Clean Direct Inheritance
 * 
 * INHERITANCE CHAIN:
 * ChatParams → ReceiveEventsParams (adds eventTypes, listening config)
 * ChatResult → ReceiveEventsResult (adds stream info, event count)
 */

import { ChatParams, ChatResult } from '../../shared/ChatTypes';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface ReceiveEventsParams extends ChatParams {
  readonly eventTypes?: string[];
  readonly maxEvents?: number;
  readonly timeoutMs?: number;
}

export const createReceiveEventsParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ReceiveEventsParams>, 'context' | 'sessionId'>
): ReceiveEventsParams => createPayload(context, sessionId, {
  roomId: data.roomId ?? '',
  nodeId: data.nodeId,
  eventTypes: data.eventTypes ?? ['message', 'room_event'],
  maxEvents: data.maxEvents ?? 100,
  timeoutMs: data.timeoutMs ?? 30000,
  ...data
});

export interface ReceiveEventsResult extends ChatResult {
  readonly events: readonly ChatEvent[];
  readonly eventCount: number;
  readonly streamActive: boolean;
}

export const createReceiveEventsResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ReceiveEventsResult>, 'context' | 'sessionId'> & { 
    roomId: string;
    success: boolean;
  }
): ReceiveEventsResult => createPayload(context, sessionId, {
  timestamp: new Date().toISOString(),
  events: Object.freeze([...(data.events ?? [])]),
  eventCount: data.eventCount ?? 0,
  streamActive: data.streamActive ?? false,
  ...data
});

export interface ChatEvent {
  id: string;
  type: string;
  data: any;
  timestamp: string;
  roomId: string;
  senderId?: string;
}