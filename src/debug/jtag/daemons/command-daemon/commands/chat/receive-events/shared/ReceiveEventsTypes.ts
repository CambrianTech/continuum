// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Receive Events Types - Clean Direct Inheritance
 * 
 * INHERITANCE CHAIN:
 * ChatParams → ReceiveEventsParams (adds eventTypes, listening config)
 * ChatResult → ReceiveEventsResult (adds stream info, event count)
 */

import { ChatParams, ChatResult } from '@chatShared/ChatTypes';
import type { JTAGContext } from '@shared/JTAGTypes';

export class ReceiveEventsParams extends ChatParams {
  eventTypes?: string[];
  maxEvents?: number;
  timeoutMs?: number;

  constructor(data: Partial<ReceiveEventsParams> = {}, context: JTAGContext, sessionId: string) {
    super(data, context, sessionId);
    this.eventTypes = data.eventTypes ?? ['message', 'room_event'];
    this.maxEvents = data.maxEvents ?? 100;
    this.timeoutMs = data.timeoutMs ?? 30000;
  }
}

export class ReceiveEventsResult extends ChatResult {
  events: ChatEvent[];
  eventCount: number;
  streamActive: boolean;

  constructor(data: Partial<ReceiveEventsResult> & { roomId: string }, context: JTAGContext, sessionId: string) {
    super(data, context, sessionId);
    this.events = data.events ?? [];
    this.eventCount = data.eventCount ?? 0;
    this.streamActive = data.streamActive ?? false;
  }
}

export interface ChatEvent {
  id: string;
  type: string;
  data: any;
  timestamp: string;
  roomId: string;
  senderId?: string;
}