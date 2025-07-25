/**
 * ChatPayloads - JTAG-compatible chat payloads (commands and events)
 * 
 * All payloads extend JTAGPayload foundation for transport compatibility.
 * Organized as:
 * - Command payloads (extend CommandParams/CommandResult) - Request/response
 * - Event payloads (extend JTAGPayload) - Fire-and-forget notifications
 * 
 * Chat Commands:
 * - SendMessageParams/Result - Send chat message to room
 * - JoinRoomParams/Result - Join/leave room operations  
 * - GetHistoryParams/Result - Retrieve message history
 * 
 * Chat Events:
 * - ChatMessageEvent - Message sent/edited/deleted notifications
 * - ChatRoomEvent - Room state change notifications
 * - AcademyChatEvent - Academy-aware chat with performance tracking
 */

import { CommandParams, CommandResult, JTAGPayload } from '../../../../shared/JTAGTypes';
import type { JTAGContext } from '../../../../shared/JTAGTypes';
import type { ChatParticipant, ChatMessage } from '../types/ChatTypes';

// ==================== CHAT COMMAND PARAMETERS ====================

/**
 * Send Message Command Parameters
 */
export class SendMessageParams extends CommandParams {
  roomId: string;
  content: string;
  messageType?: 'text' | 'command' | 'system' | 'image' | 'file' | 'code';
  conversationId?: string;
  responseToMessageId?: string;
  mentions?: string[];
  attachments?: string[];
  metadata?: Record<string, any>;

  constructor(data: Partial<SendMessageParams> & { roomId: string; content: string }) {
    super();
    this.roomId = data.roomId;
    this.content = data.content;
    this.messageType = data.messageType ?? 'text';
    this.conversationId = data.conversationId;
    this.responseToMessageId = data.responseToMessageId;
    this.mentions = data.mentions ?? [];
    this.attachments = data.attachments ?? [];
    this.metadata = data.metadata ?? {};
  }
}

/**
 * Send Message Command Result
 */
export class SendMessageResult extends CommandResult {
  success: boolean;
  messageId?: string;
  message?: ChatMessage;
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<SendMessageResult>) {
    super();
    this.success = data.success ?? false;
    this.messageId = data.messageId;
    this.message = data.message;
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}

// ==================== CHAT ROOM PAYLOAD ====================

/**
 * Chat room management payload
 */
export class ChatRoomPayload extends JTAGPayload {
  action: 'join_room' | 'leave_room' | 'create_room' | 'get_room_info' | 'list_participants';
  roomId: string;
  roomName?: string;
  roomType?: 'general' | 'private' | 'academy' | 'persona' | 'system' | 'debug';
  participantId?: string;
  participant?: ChatParticipant;
  roomConfig?: {
    isPrivate: boolean;
    maxParticipants: number;
    allowGuests: boolean;
    enableHistory: boolean;
    enableCommands: boolean;
  };

  constructor(data: Partial<ChatRoomPayload> & { action: ChatRoomPayload['action']; roomId: string }) {
    super();
    this.action = data.action;
    this.roomId = data.roomId;
    this.roomName = data.roomName;
    this.roomType = data.roomType;
    this.participantId = data.participantId;
    this.participant = data.participant;
    this.roomConfig = data.roomConfig;
  }
}

// ==================== CHAT HISTORY PAYLOAD ====================

/**
 * Chat history payload
 */
export class ChatHistoryPayload extends JTAGPayload {
  action: 'get_history' | 'search_messages' | 'get_thread';
  roomId: string;
  limit?: number;
  before?: string;
  after?: string;
  searchQuery?: string;
  threadId?: string;

  constructor(data: Partial<ChatHistoryPayload> & { action: ChatHistoryPayload['action']; roomId: string }) {
    super();
    this.action = data.action;
    this.roomId = data.roomId;
    this.limit = data.limit;
    this.before = data.before;
    this.after = data.after;
    this.searchQuery = data.searchQuery;
    this.threadId = data.threadId;
  }
}

// ==================== ACADEMY CHAT PAYLOAD ====================

/**
 * Academy chat payload - for Academy-aware chat with performance tracking
 */
export class AcademyChatPayload extends JTAGPayload {
  action: 'academy_message' | 'evolution_trigger' | 'training_data';
  chatPayload: ChatMessagePayload;
  academyId: string;
  personaId: string;
  sessionId: string;
  trainingMode: boolean;
  performanceMetrics: {
    technicalAccuracy: number;
    collaborationQuality: number;
    humanSatisfaction: number;
    responseLatency: number;
    contextRelevance: number;
    innovationLevel: number;
  };
  learningData: {
    conversationContext: string[];
    userIntent: string;
    responseQuality: number;
    learningOpportunities: string[];
    patternMatches: string[];
    trainingValue: number;
  };
  evolutionTrigger?: {
    triggered: boolean;
    reason: 'performance_gap' | 'new_pattern' | 'collaboration_failure' | 'innovation_opportunity';
    capabilityGaps: string[];
    suggestedImprovements: string[];
    urgency: 'low' | 'medium' | 'high';
  };

  constructor(data: Partial<AcademyChatPayload> & { 
    action: AcademyChatPayload['action']; 
    chatPayload: ChatMessagePayload;
    academyId: string;
    personaId: string;
  }) {
    super();
    this.action = data.action;
    this.chatPayload = data.chatPayload;
    this.academyId = data.academyId;
    this.personaId = data.personaId;
    this.sessionId = data.sessionId ?? `session-${Date.now()}`;
    this.trainingMode = data.trainingMode ?? false;
    this.performanceMetrics = data.performanceMetrics ?? {
      technicalAccuracy: 0.5,
      collaborationQuality: 0.5,
      humanSatisfaction: 0.5,
      responseLatency: 1000,
      contextRelevance: 0.5,
      innovationLevel: 0.5
    };
    this.learningData = data.learningData ?? {
      conversationContext: [],
      userIntent: '',
      responseQuality: 0.5,
      learningOpportunities: [],
      patternMatches: [],
      trainingValue: 0.5
    };
    this.evolutionTrigger = data.evolutionTrigger;
  }
}

// ==================== PAYLOAD UNION TYPES ====================

export type ChatPayloadType = 
  | ChatMessagePayload
  | ChatRoomPayload
  | ChatHistoryPayload
  | AcademyChatPayload;

// ==================== PAYLOAD FACTORY FUNCTIONS ====================

/**
 * Create chat message payload
 */
export function createChatMessagePayload(data: {
  action: 'send_message' | 'edit_message' | 'delete_message' | 'react_to_message';
  roomId: string;
  senderId: string;
  senderName: string;
  content: string;
  conversationId?: string;
  senderType?: 'human' | 'ai_assistant' | 'persona' | 'system';
  messageType?: 'text' | 'command' | 'system' | 'image' | 'file' | 'code';
  responseToMessageId?: string;
  mentions?: string[];
  attachments?: string[];
  metadata?: Record<string, any>;
}): ChatMessagePayload {
  return new ChatMessagePayload(data);
}

/**
 * Create chat room payload
 */
export function createChatRoomPayload(data: {
  action: 'join_room' | 'leave_room' | 'create_room' | 'get_room_info' | 'list_participants';
  roomId: string;
  roomName?: string;
  roomType?: 'general' | 'private' | 'academy' | 'persona' | 'system' | 'debug';
  participantId?: string;
  participant?: ChatParticipant;
}): ChatRoomPayload {
  return new ChatRoomPayload(data);
}

/**
 * Create Academy chat payload
 */
export function createAcademyChatPayload(data: {
  action: 'academy_message' | 'evolution_trigger' | 'training_data';
  chatPayload: ChatMessagePayload;
  academyId: string;
  personaId: string;
  sessionId?: string;
  trainingMode?: boolean;
  performanceMetrics?: AcademyChatPayload['performanceMetrics'];
  learningData?: AcademyChatPayload['learningData'];
  evolutionTrigger?: AcademyChatPayload['evolutionTrigger'];
}): AcademyChatPayload {
  return new AcademyChatPayload(data);
}