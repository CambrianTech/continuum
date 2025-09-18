/**
 * Chat Event Types - Rust-like Strict Typing
 * 
 * Every event has explicit, predictable types.
 * No mysteries, no any types, no runtime surprises.
 */

import { CHAT_EVENTS } from './ChatEventConstants';

import type { ChatMessageData } from '../../../system/data/domains/ChatMessage';

export interface ChatMessageEventData {
  readonly eventType: 'chat:message-received';
  readonly roomId: string;
  readonly messageId: string;
  readonly message: ChatMessageData;  // Full domain object, not flat fields
  readonly timestamp: string;
}

export interface ChatParticipantEventData {
  readonly userId: string;
  readonly userName: string;
  readonly roomId: string;
  readonly timestamp: string;
  readonly userType: 'human' | 'ai';
}

export interface ChatRoomEventData {
  readonly roomId: string;
  readonly roomName: string;
  readonly roomType: 'public' | 'private';
  readonly timestamp: string;
  readonly participantCount?: number;
}

// Event Type Mapping - Rust-like explicit association
export type ChatEventMap = {
  [CHAT_EVENTS.MESSAGE_RECEIVED]: ChatMessageEventData;
  [CHAT_EVENTS.PARTICIPANT_JOINED]: ChatParticipantEventData;
  [CHAT_EVENTS.PARTICIPANT_LEFT]: ChatParticipantEventData;
  [CHAT_EVENTS.ROOM_CREATED]: ChatRoomEventData;
  [CHAT_EVENTS.ROOM_UPDATED]: ChatRoomEventData;
};

// Type-safe event names
export type ChatEventName = keyof ChatEventMap;

// Get event data type from event name
export type ChatEventDataFor<T extends ChatEventName> = ChatEventMap[T];