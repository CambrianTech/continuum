/**
 * Chat Event Constants - Single Source of Truth
 * 
 * Prevents event name mismatches between server emission and client listening
 */

export const CHAT_EVENTS = {
  MESSAGE_RECEIVED: 'chat:message-received',
  PARTICIPANT_JOINED: 'chat:participant-joined', 
  PARTICIPANT_LEFT: 'chat:participant-left',
  ROOM_CREATED: 'chat:room-created',
  ROOM_UPDATED: 'chat:room-updated'
} as const;

export const CHAT_EVENT_TYPES = Object.values(CHAT_EVENTS);

export type ChatEventType = typeof CHAT_EVENTS[keyof typeof CHAT_EVENTS];