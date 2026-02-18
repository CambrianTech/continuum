/**
 * Chat Commands API - Public Interface Types
 * 
 * Consumer-first API for chat operations.
 * Uses the user types from api/types/User.ts
 */

import type { BaseUser } from '../../types/User';

// Message content and metadata
export interface MessageContent {
  text: string;
  attachments?: {
    type: 'file' | 'image' | 'link';
    url: string;
    name: string;
    metadata?: Record<string, any>;
  }[];
  formatting?: {
    markdown: boolean;
    code?: {
      language: string;
      syntax: boolean;
    };
  };
}

// Chat message operations
export interface SendMessageParams {
  roomId: string;
  content: MessageContent;
  sender: BaseUser;
  replyToId?: string;
  mentions?: string[]; // user IDs
  metadata?: Record<string, any>;
}

export interface ChatMessage {
  id: string;
  roomId: string;
  senderId: string;
  senderName: string;
  content: MessageContent;
  timestamp: string;
  replyToId?: string;
  mentions: string[];
  reactions: {
    emoji: string;
    users: string[];
    count: number;
  }[];
  status: 'sending' | 'sent' | 'delivered' | 'failed';
  metadata: Record<string, any>;
}

export interface SendMessageResult {
  success: boolean;
  message?: ChatMessage;
  messageId?: string;
  error?: string;
}

// Room operations
export interface CreateRoomParams {
  name: string;
  description?: string;
  isPrivate?: boolean;
  maxUsers?: number;
  creator: BaseUser;
  initialUsers?: BaseUser[];
  metadata?: Record<string, any>;
}

export interface ChatRoom {
  id: string;
  name: string;
  displayName: string;
  description: string;
  isPrivate: boolean;
  maxUsers?: number;
  creatorId: string;
  participants: BaseUser[];
  messageCount: number;
  unreadCount: number;
  lastMessage?: {
    content: string;
    timestamp: string;
    senderName: string;
  };
  createdAt: string;
  updatedAt: string;
  metadata: Record<string, any>;
}

export interface CreateRoomResult {
  success: boolean;
  room?: ChatRoom;
  roomId?: string;
  error?: string;
}

export interface JoinRoomParams {
  roomId: string;
  user: BaseUser;
}

export interface JoinRoomResult {
  success: boolean;
  room?: ChatRoom;
  joined: boolean;
  error?: string;
}

export interface LeaveRoomParams {
  roomId: string;
  userId: string;
}

export interface LeaveRoomResult {
  success: boolean;
  room?: ChatRoom;
  left: boolean;
  error?: string;
}

// Message history operations
export interface GetMessageHistoryParams {
  roomId: string;
  limit?: number;
  before?: string; // message ID
  after?: string; // message ID
  userId?: string; // filter by user
}

export interface GetMessageHistoryResult {
  success: boolean;
  messages: ChatMessage[];
  roomId: string;
  totalCount?: number;
  hasMore: boolean;
  error?: string;
}

// Room listing and search
export interface ListRoomsParams {
  userId?: string; // rooms user has access to
  includePrivate?: boolean;
  search?: string;
  limit?: number;
  offset?: number;
}

export interface ListRoomsResult {
  success: boolean;
  rooms: ChatRoom[];
  totalCount: number;
  hasMore: boolean;
  error?: string;
}

// User presence and status
export interface UpdateUserStatusParams {
  userId: string;
  status: 'online' | 'away' | 'busy' | 'offline';
  customMessage?: string;
}

export interface UserPresence {
  userId: string;
  status: 'online' | 'away' | 'busy' | 'offline';
  lastSeen: string;
  customMessage?: string;
}

export interface UpdateUserStatusResult {
  success: boolean;
  presence?: UserPresence;
  error?: string;
}

export interface GetRoomUsersParams {
  roomId: string;
  includeOffline?: boolean;
}

export interface GetRoomUsersResult {
  success: boolean;
  users: BaseUser[];
  presence: Record<string, UserPresence>;
  roomId: string;
  error?: string;
}

// Event system for real-time updates
export interface ChatEvent {
  type: 'message' | 'user_joined' | 'user_left' | 'room_created' | 'room_updated' | 'typing' | 'presence';
  roomId?: string;
  userId?: string;
  data: any;
  timestamp: string;
}

export interface SubscribeToEventsParams {
  roomIds?: string[];
  eventTypes?: ChatEvent['type'][];
  userId?: string;
}

export interface SubscribeToEventsResult {
  success: boolean;
  subscriptionId?: string;
  error?: string;
}

// Typing indicators
export interface SetTypingParams {
  roomId: string;
  userId: string;
  isTyping: boolean;
}

export interface SetTypingResult {
  success: boolean;
  error?: string;
}

// Export all chat command types
export type ChatCommandParams = 
  | SendMessageParams
  | CreateRoomParams
  | JoinRoomParams
  | LeaveRoomParams
  | GetMessageHistoryParams
  | ListRoomsParams
  | UpdateUserStatusParams
  | GetRoomUsersParams
  | SubscribeToEventsParams
  | SetTypingParams;

export type ChatCommandResult = 
  | SendMessageResult
  | CreateRoomResult
  | JoinRoomResult
  | LeaveRoomResult
  | GetMessageHistoryResult
  | ListRoomsResult
  | UpdateUserStatusResult
  | GetRoomUsersResult
  | SubscribeToEventsResult
  | SetTypingResult;