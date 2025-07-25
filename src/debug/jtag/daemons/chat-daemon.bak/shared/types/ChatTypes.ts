/**
 * ChatTypes - Core chat type definitions for JTAG chat daemon
 * 
 * Defines fundamental chat entity types:
 * - ChatMessage - Individual chat messages with full metadata
 * - ChatParticipant - Chat participants (human, AI, persona, system)
 * - ChatRoom - Chat rooms with configuration and state
 * - Supporting enums and utility types
 */

// ==================== CHAT MESSAGE ====================

export interface ChatMessage {
  id: string;
  content: string;
  senderId: string;
  senderName: string;
  senderType: 'human' | 'ai_assistant' | 'persona' | 'system';
  roomId: string;
  conversationId: string;
  messageType: 'text' | 'command' | 'system' | 'image' | 'file' | 'code';
  timestamp: number;
  responseToMessageId?: string;
  mentions: string[];
  attachments: string[];
  reactions: Array<{ emoji: string; userIds: string[]; timestamp: number }>;
  isEdited: boolean;
  isDeleted: boolean;
  metadata: Record<string, any>;
}

// ==================== CHAT PARTICIPANT ====================

export interface ChatParticipant {
  id: string;
  name: string;
  type: 'human' | 'ai_assistant' | 'persona' | 'system';
  displayName?: string;
  avatar?: string;
  status: 'online' | 'away' | 'busy' | 'offline' | 'invisible';
  capabilities: string[];
  metadata: {
    role?: 'user' | 'admin' | 'moderator' | 'guest';
    joinedAt?: number;
    lastActivity?: number;
    preferences?: {
      communicationStyle?: string;
      topics?: string[];
      expertise?: string[];
      timezone?: string;
      language?: string;
    };
    // Academy-specific metadata (for personas)
    academyId?: string;
    personaGenome?: string;
    specialization?: string;
    evolutionStage?: string;
    trainingMode?: boolean;
  };
}

// ==================== CHAT ROOM ====================

export interface ChatRoom {
  id: string;
  name: string;
  type: 'general' | 'private' | 'academy' | 'persona' | 'system' | 'debug';
  participants: ChatParticipant[];
  created: number;
  lastActivity?: number;
  isActive: boolean;
  
  // Room configuration
  config: {
    isPrivate: boolean;
    maxParticipants: number;
    allowGuests: boolean;
    enableHistory: boolean;
    enableCommands: boolean;
    moderationRules?: string[];
    autoArchiveAfter?: number; // milliseconds
  };
  
  // Room state
  state: {
    messageCount: number;
    participantCount: number;
    averageResponseTime?: number;
    lastMessageId?: string;
    healthStatus: 'healthy' | 'warning' | 'error';
  };
  
  // Room metadata
  metadata: {
    description?: string;
    createdBy: string;
    tags: string[];
    category?: string;
    // Academy-specific metadata
    academyId?: string;
    trainingMode?: boolean;
    evolutionEnabled?: boolean;
    performanceTracking?: boolean;
  };
}

// ==================== SUPPORTING TYPES ====================

export type MessageType = 'text' | 'command' | 'system' | 'image' | 'file' | 'code' | 'markdown' | 'persona' | 'evolution';

export type ParticipantType = 'human' | 'ai_assistant' | 'persona' | 'bot' | 'agent' | 'system' | 'custom';

export type ParticipantStatus = 'online' | 'away' | 'busy' | 'offline' | 'invisible';

export type ChatRoomType = 'general' | 'private' | 'academy' | 'persona' | 'evolution' | 'system' | 'support' | 'debug';

export type ParticipantRole = 'user' | 'admin' | 'moderator' | 'guest' | 'trainer' | 'observer';

// ==================== UTILITY INTERFACES ====================

/**
 * Chat statistics for monitoring and analytics
 */
export interface ChatStats {
  totalMessages: number;
  totalParticipants: number;
  totalRooms: number;
  activeParticipants: number;
  activeRooms: number;
  messagesPerMinute: number;
  averageResponseTime: number;
  topParticipants: Array<{
    participant: ChatParticipant;
    messageCount: number;
    activityScore: number;
  }>;
  topRooms: Array<{
    room: ChatRoom;
    messageCount: number;
    activityScore: number;
  }>;
  // Academy-specific stats
  academyStats?: {
    trainingMessagesPerMinute: number;
    evolutionTriggersPerHour: number;
    averagePerformanceScore: number;
    activePersonas: number;
  };
}

/**
 * Chat configuration for daemon initialization
 */
export interface ChatConfig {
  // Connection settings
  maxConnections: number;
  connectionTimeout: number;
  
  // Message settings  
  maxMessageLength: number;
  messageHistoryLimit: number;
  maxAttachmentsPerMessage: number;
  maxAttachmentSize: number;
  
  // Room settings
  maxRoomsPerParticipant: number;
  maxParticipantsPerRoom: number;
  defaultRoomType: ChatRoomType;
  autoCreateRooms: boolean;
  
  // Feature flags
  enableCommands: boolean;
  enableAttachments: boolean;
  enableReactions: boolean;
  enablePrivateMessages: boolean;
  enableMessageHistory: boolean;
  enableParticipantTracking: boolean;
  
  // Moderation settings
  enableProfanityFilter: boolean;
  enableRateLimiting: boolean;
  enableSpamDetection: boolean;
  maxMessagesPerMinute: number;
  
  // Academy integration
  enableAcademyIntegration: boolean;
  enablePerformanceTracking: boolean;
  enableEvolutionTriggers: boolean;
  academyTrainingMode: boolean;
}

// ==================== VALIDATION TYPES ====================

export interface ChatMessageValidationResult {
  valid: boolean;
  errors: string[];
  warnings?: string[];
}

export interface ChatRoomValidationResult {
  valid: boolean;
  errors: string[];
  warnings?: string[];
}

export interface ChatParticipantValidationResult {
  valid: boolean;
  errors: string[];
  warnings?: string[];
}

// ==================== REQUEST/RESPONSE TYPES ====================

export interface SendMessageRequest {
  roomId: string;
  content: string;
  messageType?: MessageType;
  responseToMessageId?: string;
  mentions?: string[];
  attachments?: string[];
  metadata?: Record<string, any>;
}

export interface SendMessageResponse {
  success: boolean;
  messageId?: string;
  message?: ChatMessage;
  error?: string;
  warnings?: string[];
}

export interface JoinRoomRequest {
  roomId: string;
  participant: ChatParticipant;
  createIfNotExists?: boolean;
}

export interface JoinRoomResponse {
  success: boolean;
  room?: ChatRoom;
  participantCount?: number;
  error?: string;
}

export interface GetRoomHistoryRequest {
  roomId: string;
  limit?: number;
  before?: string; // Message ID for pagination
  after?: string;  // Message ID for pagination
  includeDeleted?: boolean;
}

export interface GetRoomHistoryResponse {
  success: boolean;
  messages?: ChatMessage[];
  hasMore?: boolean;
  totalCount?: number;
  error?: string;
}

// ==================== CONSTANTS ====================

export const CHAT_CONSTANTS = {
  // Message limits
  MAX_MESSAGE_LENGTH: 10000,
  MAX_ROOM_NAME_LENGTH: 100,
  MAX_PARTICIPANTS_PER_ROOM: 100,
  MESSAGE_HISTORY_LIMIT: 1000,
  MAX_ATTACHMENTS_PER_MESSAGE: 10,
  MAX_ATTACHMENT_SIZE: 10 * 1024 * 1024, // 10MB
  
  // Timing
  TYPING_TIMEOUT: 3000,           // 3 seconds
  PRESENCE_TIMEOUT: 300000,       // 5 minutes
  ROOM_AUTO_ARCHIVE: 86400000,    // 24 hours
  MESSAGE_EDIT_TIMEOUT: 300000,   // 5 minutes
  
  // Rate limiting
  MAX_MESSAGES_PER_MINUTE: 60,
  MAX_ROOMS_PER_PARTICIPANT: 50,
  MAX_MENTIONS_PER_MESSAGE: 20,
  
  // Reserved identifiers
  RESERVED_ROOM_IDS: ['system', 'general', 'academy', 'evolution', 'debug'],
  SYSTEM_PARTICIPANT_ID: 'system',
  ACADEMY_PARTICIPANT_PREFIX: 'persona-',
  
  // Academy integration
  PERFORMANCE_TRACKING_INTERVAL: 5000,     // 5 seconds
  EVOLUTION_TRIGGER_THRESHOLD: 0.3,       // Performance gap threshold
  ACADEMY_MESSAGE_PRIORITY: 'high' as const,
  TRAINING_SESSION_TIMEOUT: 1800000,      // 30 minutes
  
  // Event priorities for future event system
  MESSAGE_EVENT_PRIORITY: 'medium' as const,
  ROOM_EVENT_PRIORITY: 'medium' as const,
  ACADEMY_EVENT_PRIORITY: 'high' as const,
  SYSTEM_EVENT_PRIORITY: 'high' as const
} as const;

// ==================== UTILITY FUNCTIONS ====================

/**
 * Check if message is from system
 */
export function isSystemMessage(message: ChatMessage): boolean {
  return message.senderType === 'system' || message.messageType === 'system';
}

/**
 * Check if message is a command
 */
export function isCommandMessage(message: ChatMessage): boolean {
  return message.messageType === 'command' || message.content.startsWith('/');
}

/**
 * Check if message is from Academy persona
 */
export function isPersonaMessage(message: ChatMessage): boolean {
  return message.senderType === 'persona' || message.messageType === 'persona';
}

/**
 * Format message preview for display
 */
export function formatMessagePreview(message: ChatMessage, maxLength: number = 100): string {
  let preview = message.content;
  
  if (message.messageType === 'command') {
    preview = `/${message.content.split(' ')[0].replace('/', '')}`;
  } else if (message.messageType === 'image') {
    preview = '🖼️ Image';
  } else if (message.messageType === 'file') {
    preview = '📎 File';
  } else if (message.messageType === 'code') {
    preview = '💻 Code';
  }
  
  if (preview.length > maxLength) {
    preview = preview.substring(0, maxLength - 3) + '...';
  }
  
  return preview;
}

/**
 * Calculate participant activity score
 */
export function calculateActivityScore(participant: ChatParticipant, messageCount: number): number {
  const lastActivity = participant.metadata.lastActivity || 0;
  const timeSinceActive = Date.now() - lastActivity;
  const recencyScore = Math.max(0, 1 - (timeSinceActive / (24 * 60 * 60 * 1000))); // Decay over 24 hours
  const volumeScore = Math.min(1, messageCount / 100); // Normalize to 100 messages
  
  return (recencyScore * 0.6) + (volumeScore * 0.4);
}