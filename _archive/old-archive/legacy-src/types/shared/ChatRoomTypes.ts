/**
 * Shared ChatRoom Types - Universal interfaces for daemon communication
 * 
 * These types ensure type safety across all ChatRoom daemon interactions
 * including commands, daemon communication, and client-server boundaries.
 */

import { ChatRoomOperations } from './CommandOperationTypes';

// Enums for type safety
export enum ChatRoomType {
  CHAT = 'chat',
  COLLABORATION = 'collaboration',
  SYSTEM = 'system'
}

export enum MessageType {
  TEXT = 'text',
  IMAGE = 'image',
  FILE = 'file',
  SYSTEM = 'system'
}

export enum ParticipantRole {
  OWNER = 'owner',
  ADMIN = 'admin',
  MEMBER = 'member',
  GUEST = 'guest'
}

export enum ParticipantStatus {
  ONLINE = 'online',
  OFFLINE = 'offline',
  AWAY = 'away'
}

// Core ChatRoom entities
export interface ChatRoom {
  id: string;
  name: string;
  type: ChatRoomType;
  participants: Set<string>;
  created_at: Date;
  created_by: string;
  metadata: Record<string, unknown>;
}

export interface ChatMessage {
  id: string;
  room_id: string;
  sender_id: string;
  content: string;
  timestamp: Date;
  message_type: MessageType;
  metadata: Record<string, unknown>;
}

export interface ChatParticipant {
  user_id: string;
  session_id: string;
  joined_at: Date;
  role: ParticipantRole;
  status: ParticipantStatus;
}

// ChatRoom daemon request/response types
export interface ChatRoomRequestData {
  correlationId: string;
  timestamp: number;
}

export interface CreateRoomRequest extends ChatRoomRequestData {
  name: string;
  type?: ChatRoomType;
  created_by: string;
  session_id?: string;
  metadata?: Record<string, unknown>;
}

export interface CreateRoomResponse {
  room_id: string;
  room: ChatRoom;
  participant_count: number;
}

export interface JoinRoomRequest extends ChatRoomRequestData {
  room_id: string;
  user_id: string;
  session_id?: string;
  role?: ParticipantRole.MEMBER | ParticipantRole.GUEST;
}

export interface JoinRoomResponse {
  success: boolean;
  participant_count: number;
  room_info: ChatRoom;
}

export interface SendMessageRequest extends ChatRoomRequestData {
  room_id: string;
  sender_id: string;
  content: string;
  message_type?: MessageType;
  session_id?: string;
  metadata?: Record<string, unknown>;
}

export interface SendMessageResponse {
  message_id: string;
  message: ChatMessage;
  room_participant_count: number;
  broadcast_count: number;
}

export interface GetMessagesRequest extends ChatRoomRequestData {
  room_id: string;
  user_id: string;
  limit?: number;
  offset?: number;
  since?: Date;
}

export interface GetMessagesResponse {
  messages: ChatMessage[];
  total_count: number;
  has_more: boolean;
  room_info: ChatRoom;
}

export interface ListRoomsRequest extends ChatRoomRequestData {
  user_id: string;
  room_type?: ChatRoomType;
  include_empty?: boolean;
}

export interface ListRoomsResponse {
  rooms: Array<{
    id: string;
    name: string;
    type: ChatRoomType;
    participant_count: number;
    last_message?: {
      content: string;
      timestamp: Date;
      sender_id: string;
    };
  }>;
  total_count: number;
}

export interface GetRoomInfoRequest extends ChatRoomRequestData {
  room_id: string;
  user_id: string;
}

export interface GetRoomInfoResponse {
  room: ChatRoom;
  participants: ChatParticipant[];
  message_count: number;
  user_role: ParticipantRole | null;
}

// Daemon event types
export interface ChatRoomDaemonRequest {
  type: 'chatroom_request';
  operation: ChatRoomOperations;
  correlationId: string;
  data: CreateRoomRequest | JoinRoomRequest | SendMessageRequest | GetMessagesRequest | ListRoomsRequest | GetRoomInfoRequest;
  timestamp: number;
}

export interface ChatRoomDaemonResponse {
  correlationId: string;
  success: boolean;
  data?: CreateRoomResponse | JoinRoomResponse | SendMessageResponse | GetMessagesResponse | ListRoomsResponse | GetRoomInfoResponse;
  error?: string;
  timestamp: number;
}

// Command result types
export interface ChatCommandResult {
  message?: string;
  messageId?: string;
  content?: string;
  room?: string;
  timestamp?: string;
  marshalId?: string;
  chatRoom?: SendMessageResponse;
  status?: string;
  available_rooms?: Array<{
    id: string;
    name: string;
    type: string;
    participant_count: number;
  }>;
  sessionId?: string;
  totalRooms?: number;
}

// Type guards for runtime validation
export function isSendMessageRequest(data: unknown): data is SendMessageRequest {
  return typeof data === 'object' && 
         data !== null && 
         'room_id' in data && 
         'sender_id' in data && 
         'content' in data &&
         'correlationId' in data;
}

export function isListRoomsRequest(data: unknown): data is ListRoomsRequest {
  return typeof data === 'object' && 
         data !== null && 
         'user_id' in data &&
         'correlationId' in data;
}

export function isChatRoomDaemonResponse(data: unknown): data is ChatRoomDaemonResponse {
  return typeof data === 'object' && 
         data !== null && 
         'correlationId' in data && 
         'success' in data &&
         'timestamp' in data;
}