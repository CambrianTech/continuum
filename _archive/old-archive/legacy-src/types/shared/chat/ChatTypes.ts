/**
 * Shared Chat Types - Universal interfaces for chat system
 * 
 * Updated for UserPersona integration and UUID-based architecture
 * All IDs are UUIDv4, no magic strings
 */

import { UUID } from '../core/UserPersona';

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
  AWAY = 'away',
  BUSY = 'busy'
}

// Room connection and token system
export interface RoomToken {
  readonly tokenId: UUID;
  readonly userId: UUID;
  readonly roomId: UUID;
  readonly permissions: string[];
  readonly issuedAt: Date;
  readonly expiresAt: Date;
  readonly sessionId?: UUID;
}

// Core chat entities with UUID architecture
export interface ChatRoom {
  readonly id: UUID;
  readonly name: string;
  readonly type: ChatRoomType;
  readonly participants: Set<UUID>; // UserPersona IDs
  readonly created_at: Date;
  readonly created_by: UUID; // UserPersona ID
  readonly metadata: {
    default?: boolean;
    autoCreated?: boolean;
    category?: 'public' | 'private' | 'system';
    description?: string | undefined;
    maxParticipants?: number;
    [key: string]: unknown;
  };
}

export interface ChatMessage {
  readonly id: UUID;
  readonly room_id: UUID;
  readonly sender_id: UUID; // UserPersona ID
  readonly content: string;
  readonly timestamp: Date;
  readonly message_type: MessageType;
  readonly metadata: {
    edited?: boolean;
    edited_at?: Date;
    reply_to?: UUID; // Message ID
    mentions?: UUID[]; // UserPersona IDs
    [key: string]: unknown;
  };
}

export interface ChatParticipant {
  readonly user_id: UUID;
  readonly session_id: UUID;
  readonly joined_at: Date;
  readonly role: ParticipantRole;
  readonly status: ParticipantStatus;
  readonly last_seen: Date;
  readonly metadata: {
    nickname?: string;
    permissions?: string[];
    [key: string]: unknown;
  };
}

// Room connection result
export interface RoomConnectionResult {
  readonly success: boolean;
  readonly token?: RoomToken;
  readonly roomState?: {
    room: ChatRoom;
    participants: ChatParticipant[];
    recentMessages: ChatMessage[];
  };
  readonly error?: string;
}

// Request/Response types for daemon communication
export interface ChatRoomRequestData {
  readonly correlationId: UUID;
  readonly timestamp: number;
  readonly requesterId?: UUID; // UserPersona ID - optional for backward compatibility
}

export interface JoinRoomRequest extends ChatRoomRequestData {
  readonly room_id: UUID;
  readonly user_id: UUID;
  readonly session_id?: UUID;
  readonly role?: ParticipantRole.MEMBER | ParticipantRole.GUEST;
}

export interface JoinRoomResponse {
  readonly success: boolean;
  readonly token?: RoomToken;
  readonly room?: ChatRoom;
  readonly participant_count: number;
  readonly error?: string;
}

export interface SendMessageRequest extends ChatRoomRequestData {
  readonly room_id: UUID;
  readonly sender_id: UUID; // UserPersona ID
  readonly content: string;
  readonly message_type?: MessageType | undefined;
  readonly session_id?: UUID | undefined;
  readonly metadata?: Record<string, unknown> | undefined;
}

export interface SendMessageResponse {
  readonly message_id: UUID;
  readonly message: ChatMessage;
  readonly room_participant_count: number;
  readonly broadcast_count: number;
}

export interface GetMessagesRequest extends ChatRoomRequestData {
  readonly room_id: UUID;
  readonly user_id: UUID; // UserPersona ID
  readonly limit?: number;
  readonly offset?: number;
  readonly since?: Date;
}

export interface GetMessagesResponse {
  readonly messages: ChatMessage[];
  readonly total_count: number;
  readonly has_more: boolean;
  readonly room_info: ChatRoom;
}

export interface ListRoomsRequest extends ChatRoomRequestData {
  readonly user_id: UUID; // UserPersona ID
  readonly room_type?: ChatRoomType;
  readonly include_empty?: boolean;
}

export interface ListRoomsResponse {
  readonly rooms: Array<{
    readonly id: UUID;
    readonly name: string;
    readonly type: ChatRoomType;
    readonly participant_count: number;
    readonly last_message?: {
      readonly content: string;
      readonly timestamp: Date;
      readonly sender_id: UUID;
    };
  }>;
  readonly total_count: number;
}

// Legacy command result types (for backward compatibility)
export interface ChatCommandResult {
  readonly message?: string;
  readonly messageId?: UUID;
  readonly content?: string;
  readonly room?: UUID;
  readonly timestamp?: string;
  readonly marshalId?: UUID;
  readonly chatRoom?: SendMessageResponse;
  readonly status?: string;
  readonly availableRooms?: Array<{
    readonly id: UUID;
    readonly name: string;
    readonly type: string;
    readonly participantCount: number;
  }>;
  readonly sessionId?: UUID;
  readonly totalRooms?: number;
}

// Daemon communication types (for backward compatibility)
export interface ChatRoomDaemonRequest {
  readonly type: 'chatroom_request';
  readonly operation: string; // ChatRoomOperations
  readonly correlationId: UUID;
  readonly data: JoinRoomRequest | SendMessageRequest | GetMessagesRequest | ListRoomsRequest;
  readonly timestamp: number;
}

export interface ChatRoomDaemonResponse {
  readonly correlationId: UUID;
  readonly success: boolean;
  readonly data?: JoinRoomResponse | SendMessageResponse | GetMessagesResponse | ListRoomsResponse;
  readonly error?: string;
  readonly timestamp: number;
}

// Type guards for runtime validation
export function isJoinRoomRequest(data: unknown): data is JoinRoomRequest {
  return typeof data === 'object' && 
         data !== null && 
         'room_id' in data && 
         'user_id' in data && 
         'correlationId' in data;
}

export function isSendMessageRequest(data: unknown): data is SendMessageRequest {
  return typeof data === 'object' && 
         data !== null && 
         'room_id' in data && 
         'sender_id' in data && 
         'content' in data &&
         'correlationId' in data;
}

export function isRoomToken(data: unknown): data is RoomToken {
  return typeof data === 'object' &&
         data !== null &&
         'tokenId' in data &&
         'userId' in data &&
         'roomId' in data &&
         'permissions' in data;
}

export function isChatRoomDaemonResponse(data: unknown): data is ChatRoomDaemonResponse {
  return typeof data === 'object' && 
         data !== null && 
         'correlationId' in data && 
         'success' in data &&
         'timestamp' in data;
}