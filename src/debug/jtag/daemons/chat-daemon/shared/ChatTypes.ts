/**
 * Chat Daemon - Shared Types
 * 
 * Following ScreenshotTypes pattern: factory functions, inheritance, strong typing
 * Complete type definitions for chat operations with zero 'any' usage
 */

import type { JTAGPayload, JTAGContext } from '../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { JTAGError } from '../../../system/core/types/ErrorTypes';

// ============================================================================
// UNIVERSAL PARTICIPANT TYPES - PARTICIPANT-AGNOSTIC ARCHITECTURE
// ============================================================================

/**
 * Universal Session Participant - Any intelligence that can join chat
 * 
 * Key insight: We make NO distinction between human users, AI agents, personas, etc.
 * All participants are simply "session users" with capabilities defined by adapters.
 * This eliminates 85% of participant-specific code complexity.
 */
export interface SessionParticipant {
  readonly participantId: UUID;
  readonly sessionId: UUID;
  readonly displayName: string;
  readonly joinedAt: string;
  readonly lastSeen: string;
  readonly isOnline: boolean;
  
  // Universal participant capabilities (all optional)
  readonly capabilities?: ParticipantCapabilities;
  readonly adapter?: ParticipantAdapter;  // How this participant connects/responds
  readonly subscribedRooms?: readonly UUID[];
}

/**
 * Participant Capabilities - What this participant can do
 * Completely abstract - no hardcoded participant types
 */
export interface ParticipantCapabilities {
  readonly canSendMessages: boolean;
  readonly canReceiveMessages: boolean;
  readonly canCreateRooms: boolean;
  readonly canInviteOthers: boolean;
  readonly canModerate: boolean;
  readonly autoResponds: boolean;  // Instead of checking 'agent' type
  readonly providesContext: boolean;
}

/**
 * Participant Adapter - HOW this participant interacts
 * Abstracts away human UI vs AI API vs persona LoRA, etc.
 */
export interface ParticipantAdapter {
  readonly type: string; // 'browser-ui' | 'ai-api' | 'lora-persona' | 'webhook' | custom
  readonly config?: AdapterConfig; // Strongly typed adapter-specific configuration
  readonly responseStrategy?: ResponseStrategy;
}

/**
 * Discriminated union of adapter configurations
 * Enables type-safe config access based on adapter type
 */
export type AdapterConfig = 
  | AIAdapterConfig
  | WebhookAdapterConfig  
  | LoRAAdapterConfig
  | TemplateAdapterConfig
  | BrowserAdapterConfig
  | Record<string, unknown>; // Fallback for custom adapters

/**
 * Configuration for AI-powered adapters (OpenAI, Anthropic, etc.)
 */
export interface AIAdapterConfig {
  readonly type: 'ai-api';
  readonly provider: 'openai' | 'anthropic' | 'local';
  readonly model?: string;
  readonly apiKey?: string;
  readonly systemPrompt?: string;
  readonly maxTokens?: number;
  readonly temperature?: number;
}

/**
 * Configuration for webhook-based adapters
 */
export interface WebhookAdapterConfig {
  readonly type: 'webhook';
  readonly url: string;
  readonly headers?: Record<string, string>;
  readonly timeout?: number;
}

/**
 * Configuration for LoRA persona adapters
 */
export interface LoRAAdapterConfig {
  readonly type: 'lora-persona';
  readonly personaName: string;
  readonly modelPath: string;
  readonly parameters?: Record<string, unknown>;
}

/**
 * Configuration for template-based adapters
 */
export interface TemplateAdapterConfig {
  readonly type: 'template';
  readonly template: string;
  readonly variables?: Record<string, string>;
}

/**
 * Configuration for browser UI adapters
 */
export interface BrowserAdapterConfig {
  readonly type: 'browser-ui';
  readonly widgetId?: string;
  readonly theme?: string;
}

/**
 * Response Strategy - When/how this participant responds
 * Eliminates hardcoded AI response logic
 */
export interface ResponseStrategy {
  readonly triggers: ResponseTrigger[];
  readonly style?: ResponseStyle;
  readonly frequency?: ResponseFrequency;
}

export interface ResponseTrigger {
  readonly type: 'mention' | 'keyword' | 'question' | 'activity' | 'random' | 'always' | 'never';
  readonly value?: string | string[]; // For keywords, patterns, etc.
  readonly probability?: number; // 0-1, for probabilistic triggers
}

export interface ResponseStyle {
  readonly maxLength?: number;
  readonly tone?: string;
  readonly context?: 'message-history' | 'room-context' | 'global' | 'none';
}

export interface ResponseFrequency {
  readonly maxPerMinute?: number;
  readonly cooldownMs?: number;
  readonly respectTurns?: boolean; // Wait for others to respond first
}

// Legacy compatibility aliases - Strong typing even for legacy
export type ChatCitizen = SessionParticipant & {
  readonly citizenId: UUID; // Alias for participantId
  readonly citizenType: 'user' | 'agent' | 'persona'; // Legacy field (ignored)
  readonly status?: 'active' | 'idle' | 'away';
  readonly aiConfig?: LegacyAIConfig; // Strong typing instead of 'any'
  readonly context?: Record<string, unknown>; // Strong typing instead of 'any'
};

/**
 * Legacy AI Config - Strongly typed for migration safety
 */
export interface LegacyAIConfig {
  readonly provider?: 'openai' | 'anthropic' | 'local' | string;
  readonly model?: string;
  readonly settings?: Record<string, unknown>;
  readonly systemPrompt?: string;
  readonly apiKey?: string;
}

/**
 * Universal Chat Message - No participant-specific fields
 * 
 * Key insight: Messages are just data flowing through the system.
 * The sender's "type" is irrelevant - only their ID and capabilities matter.
 */
export interface ChatMessage {
  readonly messageId: UUID;
  readonly roomId: UUID;
  readonly senderId: UUID;
  readonly senderName: string;
  readonly content: string;
  readonly timestamp: string;
  readonly mentions: readonly UUID[];
  
  // Universal message metadata
  readonly category: 'chat' | 'system' | 'response' | 'notification';
  readonly replyToId?: UUID;
  readonly messageContext?: Record<string, unknown>; // Universal message context, strongly typed
  
  // Legacy compatibility (ignored in universal architecture)
  readonly messageType?: 'chat' | 'system' | 'ai-response';
  readonly senderType?: 'user' | 'agent' | 'persona' | 'system';
  readonly aiProcessed?: boolean;
  readonly aiContext?: Record<string, unknown>; // Strong typing
}

/**
 * Universal Chat Room - No participant-specific assumptions
 * 
 * Rooms don't care about participant types - they just manage messages and membership
 */
export interface ChatRoom {
  readonly roomId: UUID;
  readonly name: string;
  readonly description?: string;
  readonly createdAt: string;
  readonly lastActivity: string;
  readonly participantCount: number;
  readonly messageCount: number;
  readonly isPrivate: boolean;
  
  // Universal room configuration
  readonly category?: string;
  readonly moderationRules?: ModerationRules;
  readonly participantLimits?: ParticipantLimits;
  readonly messageRetention?: MessageRetention;
  
  // Runtime state (optional - managed by server)
  readonly participants?: readonly SessionParticipant[];
  readonly messageHistory?: readonly ChatMessage[];
  
  // Legacy compatibility
  readonly citizenCount?: number; // Alias for participantCount
  readonly citizens?: readonly ChatCitizen[]; // Alias for participants
  readonly allowAI?: boolean; // Migrated to moderationRules
  readonly requireModeration?: boolean; // Migrated to moderationRules
  readonly maxHistoryLength?: number; // Migrated to messageRetention
}

/**
 * Universal Moderation Rules - Behavior-based, not participant-based
 */
export interface ModerationRules {
  readonly autoModerationEnabled: boolean;
  readonly allowAutoResponders: boolean; // Instead of 'allowAI'
  readonly requireApproval: boolean;
  readonly bannedWords?: readonly string[];
  readonly rateLimit?: {
    readonly maxMessagesPerMinute: number;
    readonly cooldownMs: number;
  };
}

export interface ParticipantLimits {
  readonly maxParticipants?: number;
  readonly requireInvite?: boolean;
  readonly allowGuests?: boolean;
}

export interface MessageRetention {
  readonly maxMessages?: number;
  readonly maxAgeMs?: number;
  readonly archiveOldMessages?: boolean;
}

// ============================================================================
// CHAT OPERATION PARAMETERS - Extending JTAGPayload
// ============================================================================

export interface ChatCreateRoomParams extends JTAGPayload {
  readonly name: string;
  readonly description?: string;
  readonly isPrivate?: boolean;
}

/**
 * Universal Join Room Parameters - No participant type required
 * 
 * The system doesn't need to know WHAT you are, only your session/capabilities
 */
export interface ChatJoinRoomParams extends JTAGPayload {
  readonly roomId: UUID;
  readonly participantName: string;
  readonly capabilities?: ParticipantCapabilities;
  readonly adapter?: ParticipantAdapter;
  
  // Legacy compatibility
  readonly citizenName?: string; // Alias for participantName
  readonly citizenType?: 'user' | 'agent' | 'persona'; // Ignored
}

/**
 * Universal Send Message Parameters - No sender type assumptions
 */
export interface ChatSendMessageParams extends JTAGPayload {
  readonly roomId: UUID;
  readonly content: string;
  readonly category?: 'chat' | 'system' | 'response' | 'notification';
  readonly mentions?: readonly UUID[];
  readonly replyToId?: UUID;
  readonly messageContext?: Record<string, unknown>; // Renamed to avoid conflict with JTAGPayload.context
  
  // Legacy compatibility
  readonly messageType?: 'chat' | 'system'; // Mapped to category
}

export interface ChatGetHistoryParams extends JTAGPayload {
  readonly roomId: UUID;
  readonly limit?: number;
  readonly beforeTimestamp?: string;
}

export interface ChatListRoomsParams extends JTAGPayload {
  // No additional params needed
}

/**
 * Universal Leave Room Parameters
 */
export interface ChatLeaveRoomParams extends JTAGPayload {
  readonly roomId: UUID;
  readonly participantId: UUID;
  
  // Legacy compatibility
  readonly citizenId?: UUID; // Alias for participantId
}

// ============================================================================
// FACTORY FUNCTIONS FOR PARAMETERS - Following ScreenshotTypes pattern
// ============================================================================

export const createChatCreateRoomParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ChatCreateRoomParams, 'context' | 'sessionId'>
): ChatCreateRoomParams => createPayload(context, sessionId, data);

export const createChatJoinRoomParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ChatJoinRoomParams, 'context' | 'sessionId'>
): ChatJoinRoomParams => createPayload(context, sessionId, data);

export const createChatSendMessageParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ChatSendMessageParams, 'context' | 'sessionId'>
): ChatSendMessageParams => createPayload(context, sessionId, data);

export const createChatGetHistoryParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ChatGetHistoryParams, 'context' | 'sessionId'>
): ChatGetHistoryParams => createPayload(context, sessionId, data);

export const createChatListRoomsParams = (
  context: JTAGContext,
  sessionId: UUID
): ChatListRoomsParams => createPayload(context, sessionId, {});

export const createChatLeaveRoomParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ChatLeaveRoomParams, 'context' | 'sessionId'>
): ChatLeaveRoomParams => createPayload(context, sessionId, data);

// ============================================================================
// CHAT OPERATION RESULTS - Extending JTAGPayload
// ============================================================================

export interface ChatCreateRoomResult extends JTAGPayload {
  readonly success: boolean;
  readonly roomId: UUID;
  readonly room: ChatRoom;
  readonly timestamp: string;
  readonly error?: JTAGError;
}

/**
 * Universal Join Room Result - Participant-agnostic
 */
export interface ChatJoinRoomResult extends JTAGPayload {
  readonly success: boolean;
  readonly participantId: UUID;
  readonly room: ChatRoom;
  readonly recentMessages: readonly ChatMessage[];
  readonly participantList: readonly SessionParticipant[];
  readonly timestamp: string;
  readonly error?: JTAGError;
  
  // Legacy compatibility aliases
  readonly citizenId?: UUID; // Alias for participantId
  readonly citizenList?: readonly ChatCitizen[]; // Alias for participantList
  readonly roomId?: UUID; // Extracted from room.roomId
  readonly roomName?: string; // Extracted from room.name
  readonly participantCount?: number; // Extracted from room.participantCount
}

export interface ChatSendMessageResult extends JTAGPayload {
  readonly success: boolean;
  readonly messageId: UUID;
  readonly message: ChatMessage;
  readonly timestamp: string;
  readonly error?: JTAGError;
}

export interface ChatGetHistoryResult extends JTAGPayload {
  readonly success: boolean;
  readonly roomId: UUID;
  readonly messages: readonly ChatMessage[];
  readonly totalCount: number;
  readonly hasMore: boolean;
  readonly timestamp: string;
  readonly error?: JTAGError;
}

export interface ChatListRoomsResult extends JTAGPayload {
  readonly success: boolean;
  readonly rooms: readonly ChatRoom[];
  readonly totalCount: number;
  readonly timestamp: string;
  readonly error?: JTAGError;
}

/**
 * Universal Leave Room Result
 */
export interface ChatLeaveRoomResult extends JTAGPayload {
  readonly success: boolean;
  readonly roomId: UUID;
  readonly participantId: UUID;
  readonly timestamp: string;
  readonly error?: JTAGError;
  
  // Legacy compatibility
  readonly citizenId?: UUID; // Alias for participantId
}

// ============================================================================
// FACTORY FUNCTIONS FOR RESULTS - Following ScreenshotTypes pattern
// ============================================================================

export const createChatCreateRoomResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ChatCreateRoomResult>, 'context' | 'sessionId'>
): ChatCreateRoomResult => createPayload(context, sessionId, {
  success: false,
  roomId: '' as UUID,
  room: {} as ChatRoom,
  timestamp: new Date().toISOString(),
  ...data
});

export const createChatJoinRoomResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ChatJoinRoomResult>, 'context' | 'sessionId'>
): ChatJoinRoomResult => {
  const result = createPayload(context, sessionId, {
    success: false,
    participantId: '' as UUID, // Required by ChatJoinRoomResult
    citizenId: '' as UUID,
    room: {} as ChatRoom,
    recentMessages: [],
    participantList: [],
    timestamp: new Date().toISOString(),
    roomId: '' as UUID,
    roomName: '',
    participantCount: 0,
    ...data
  });
  
  // Auto-populate legacy fields from room if room is provided  
  if (data.room) {
    result.roomId = data.room.roomId;
    result.roomName = data.room.name;
    result.participantCount = data.room.participantCount;
  }
  
  return result;
};

export const createChatSendMessageResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ChatSendMessageResult>, 'context' | 'sessionId'>
): ChatSendMessageResult => createPayload(context, sessionId, {
  success: false,
  messageId: '' as UUID,
  message: {} as ChatMessage,
  timestamp: new Date().toISOString(),
  ...data
});

export const createChatGetHistoryResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ChatGetHistoryResult>, 'context' | 'sessionId'>
): ChatGetHistoryResult => createPayload(context, sessionId, {
  success: false,
  roomId: '' as UUID,
  messages: [],
  totalCount: 0,
  hasMore: false,
  timestamp: new Date().toISOString(),
  ...data
});

export const createChatListRoomsResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ChatListRoomsResult>, 'context' | 'sessionId'>
): ChatListRoomsResult => createPayload(context, sessionId, {
  success: false,
  rooms: [],
  totalCount: 0,
  timestamp: new Date().toISOString(),
  ...data
});

export const createChatLeaveRoomResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ChatLeaveRoomResult>, 'context' | 'sessionId'>
): ChatLeaveRoomResult => createPayload(context, sessionId, {
  success: false,
  roomId: '' as UUID,
  participantId: '' as UUID, // Required by ChatLeaveRoomResult
  citizenId: '' as UUID,
  timestamp: new Date().toISOString(),
  ...data
});

// ============================================================================
// SMART RESULT INHERITANCE - Following ScreenshotTypes pattern
// ============================================================================

export const createChatJoinRoomResultFromParams = (
  params: ChatJoinRoomParams,
  differences: Omit<Partial<ChatJoinRoomResult>, 'context' | 'sessionId' | 'roomId' | 'participantName'>
): ChatJoinRoomResult => transformPayload(params, {
  success: false,
  participantId: '' as UUID,
  room: {} as ChatRoom,
  recentMessages: [],
  participantList: [],
  timestamp: new Date().toISOString(),
  ...differences
});

// Factory functions for room update system
export const createChatRoomUpdateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<ChatRoomUpdateParams, 'context' | 'sessionId'>
): ChatRoomUpdateParams => createPayload(context, sessionId, data);

export const createChatRoomUpdateResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<ChatRoomUpdateResult>, 'context' | 'sessionId'>
): ChatRoomUpdateResult => createPayload(context, sessionId, {
  success: false,
  processed: false,
  roomId: '' as UUID,
  updateType: 'message-sent' as RoomUpdateType,
  timestamp: new Date().toISOString(),
  ...data
});

export const createChatSendMessageResultFromParams = (
  params: ChatSendMessageParams,
  differences: Omit<Partial<ChatSendMessageResult>, 'context' | 'sessionId' | 'roomId' | 'content'>
): ChatSendMessageResult => transformPayload(params, {
  success: false,
  messageId: '' as UUID,
  message: {} as ChatMessage,
  timestamp: new Date().toISOString(),
  ...differences
});

// ============================================================================
// CHAT RESPONSE UNION TYPE
// ============================================================================

// Removed duplicate ChatResult definition - consolidated above

export type ChatParams = 
  | ChatCreateRoomParams 
  | ChatJoinRoomParams 
  | ChatSendMessageParams 
  | ChatGetHistoryParams 
  | ChatListRoomsParams
  | ChatLeaveRoomParams
  | ChatRoomUpdateParams;

export type ChatResult = 
  | ChatCreateRoomResult 
  | ChatJoinRoomResult 
  | ChatSendMessageResult 
  | ChatGetHistoryResult 
  | ChatListRoomsResult
  | ChatLeaveRoomResult
  | ChatRoomUpdateResult;

// ============================================================================
// LEGACY COMPATIBILITY EXPORTS - Bridge to old architecture
// ============================================================================

// Legacy Response Types (rename Result -> Response for compatibility)
export type ChatListRoomsResponse = ChatListRoomsResult;
export type ChatJoinRoomResponse = ChatJoinRoomResult;
export type ChatSendMessageResponse = ChatSendMessageResult;

// Legacy Error Response (compatibility with old error handling)
export interface ChatErrorResponse {
  readonly success: false;
  readonly timestamp: string;
  readonly error: string;
  readonly operation?: string;
}

// ============================================================================
// LOCATION-TRANSPARENT COMMAND SYSTEM - JTAG Native
// ============================================================================

/**
 * Room Update Command Parameters - Location Transparent
 * 
 * These commands work identically whether targeting:
 * - Local: chat/room-update
 * - Remote: /remote/{nodeId}/chat/room-update
 * 
 * JTAG router handles all routing complexity automatically.
 */
export interface ChatRoomUpdateParams extends JTAGPayload {
  readonly roomId: UUID;
  readonly updateType: RoomUpdateType;
  readonly data: RoomUpdateData;
  readonly targetSessionId?: UUID; // Optional session targeting
  readonly originNodeId?: string;  // For distributed tracking
}

export type RoomUpdateType = 
  | 'message-sent'
  | 'participant-joined' 
  | 'participant-left'
  | 'participant-response'
  | 'room-state-changed';

export interface RoomUpdateData {
  readonly message?: ChatMessage;
  readonly participant?: SessionParticipant;
  readonly participants?: readonly SessionParticipant[];
  readonly reason?: string;
  readonly metadata?: Record<string, unknown>;
}

/**
 * Room Update Command Result
 */
export interface ChatRoomUpdateResult extends JTAGPayload {
  readonly success: boolean;
  readonly processed: boolean;
  readonly roomId: UUID;
  readonly updateType: RoomUpdateType;
  readonly timestamp: string;
  readonly error?: JTAGError;
}

/**
 * Distributed Room Participant - Node-Aware
 */
export interface DistributedParticipant extends SessionParticipant {
  readonly nodeId: string;  // Which Continuum node they're on
  readonly nodeEndpoint?: string; // Optional custom node endpoint
}

/**
 * Room State - Distributed Ready
 */
export interface DistributedChatRoom extends ChatRoom {
  readonly participantsByNode: Record<string, readonly SessionParticipant[]>;
  readonly primaryNode?: string; // Node responsible for room state
  readonly syncedAt: string;     // Last synchronization timestamp
}

/**
 * Response Context - Universal response metadata
 * Works for any auto-responder (AI, bot, persona, etc.)
 */
export interface ResponseContext {
  readonly triggerType: 'mention' | 'keyword' | 'question' | 'activity' | 'auto' | 'ai-response' | 'persona-response' | 'template';
  readonly processingTime?: number;
  readonly confidence?: number; // 0-1, how confident the responder is
  readonly reasoning?: string; // Why this response was generated
  readonly nodeId?: string;    // Which node generated the response
}

// ============================================================================
// ROOM EVENT DATA TYPES - For RoomEventSystem
// ============================================================================

/**
 * Event data for chat message distribution
 */
export interface ChatMessageEventData {
  readonly message: ChatMessage;
  readonly roomId: UUID;
}

/**
 * Event data for participant joined notifications
 */
export interface ParticipantJoinedEventData {
  readonly participant: SessionParticipant;
  readonly roomId: UUID;
  readonly welcomeMessage?: string;
}

/**
 * Event data for participant left notifications
 */
export interface ParticipantLeftEventData {
  readonly participantId: UUID;
  readonly roomId: UUID;
  readonly participantName?: string;
  readonly reason?: string;
  readonly farewell?: string;
}

/**
 * Event data for message response notifications
 */
export interface MessageResponseEventData {
  readonly responseMessage: ChatMessage;
  readonly originalMessage: ChatMessage;
  readonly responseContext?: ResponseContext;
  readonly roomId: UUID;
}

/**
 * Event data for participant status updates
 */
export interface ParticipantUpdateEventData {
  readonly participant?: SessionParticipant; // Single participant update
  readonly participants?: SessionParticipant[]; // Bulk participant update
  readonly roomId: UUID;
  readonly updateType?: 'status-change' | 'capability-update' | 'profile-update';
  readonly changeType?: 'joined' | 'left' | 'updated' | 'bulk-update'; // For bulk updates
  readonly totalCount?: number;
  readonly previousState?: Partial<SessionParticipant>;
}