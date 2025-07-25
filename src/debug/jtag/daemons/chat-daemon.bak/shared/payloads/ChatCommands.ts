/**
 * ChatCommands - JTAG chat command parameters and results
 * 
 * Commands issued by widgets/APIs to the chat daemon:
 * - SendMessage - Send a chat message to a room
 * - JoinRoom - Join a chat room  
 * - LeaveRoom - Leave a chat room
 * - GetHistory - Get message history for a room
 * - CreateRoom - Create a new chat room
 * - EditMessage - Edit an existing message
 * - DeleteMessage - Delete a message
 * - ReactToMessage - Add/remove reaction to message
 * 
 * All extend CommandParams/CommandResult for proper JTAG command routing.
 */

import { CommandParams, CommandResult } from '../../../../shared/JTAGTypes';
import type { JTAGContext } from '../../../../shared/JTAGTypes';
import type { ChatParticipant, ChatMessage, ChatRoom } from '../types/ChatTypes';

// ==================== SEND MESSAGE COMMAND ====================

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

// ==================== JOIN ROOM COMMAND ====================

export class JoinRoomParams extends CommandParams {
  roomId: string;
  participant: ChatParticipant;
  createIfNotExists?: boolean;

  constructor(data: Partial<JoinRoomParams> & { roomId: string; participant: ChatParticipant }) {
    super();
    this.roomId = data.roomId;
    this.participant = data.participant;
    this.createIfNotExists = data.createIfNotExists ?? true;
  }
}

export class JoinRoomResult extends CommandResult {
  success: boolean;
  room?: ChatRoom;
  participantCount?: number;
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<JoinRoomResult>) {
    super();
    this.success = data.success ?? false;
    this.room = data.room;
    this.participantCount = data.participantCount;
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}

// ==================== LEAVE ROOM COMMAND ====================

export class LeaveRoomParams extends CommandParams {
  roomId: string;
  participantId: string;

  constructor(data: Partial<LeaveRoomParams> & { roomId: string; participantId: string }) {
    super();
    this.roomId = data.roomId;
    this.participantId = data.participantId;
  }
}

export class LeaveRoomResult extends CommandResult {
  success: boolean;
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<LeaveRoomResult>) {
    super();
    this.success = data.success ?? false;
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}

// ==================== GET HISTORY COMMAND ====================

export class GetHistoryParams extends CommandParams {
  roomId: string;
  limit?: number;
  before?: string; // Message ID for pagination
  after?: string;  // Message ID for pagination
  includeDeleted?: boolean;

  constructor(data: Partial<GetHistoryParams> & { roomId: string }) {
    super();
    this.roomId = data.roomId;
    this.limit = data.limit ?? 50;
    this.before = data.before;
    this.after = data.after;
    this.includeDeleted = data.includeDeleted ?? false;
  }
}

export class GetHistoryResult extends CommandResult {
  success: boolean;
  messages?: ChatMessage[];
  hasMore?: boolean;
  totalCount?: number;
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<GetHistoryResult>) {
    super();
    this.success = data.success ?? false;
    this.messages = data.messages;
    this.hasMore = data.hasMore;
    this.totalCount = data.totalCount;
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}

// ==================== CREATE ROOM COMMAND ====================

export class CreateRoomParams extends CommandParams {
  roomId: string;
  roomName?: string;
  roomType?: 'general' | 'private' | 'academy' | 'persona' | 'system' | 'debug';
  createdBy: string;
  config?: {
    isPrivate?: boolean;
    maxParticipants?: number;
    allowGuests?: boolean;
    enableHistory?: boolean;
    enableCommands?: boolean;
  };
  metadata?: Record<string, any>;

  constructor(data: Partial<CreateRoomParams> & { roomId: string; createdBy: string }) {
    super();
    this.roomId = data.roomId;
    this.roomName = data.roomName;
    this.roomType = data.roomType ?? 'general';
    this.createdBy = data.createdBy;
    this.config = data.config;
    this.metadata = data.metadata;
  }
}

export class CreateRoomResult extends CommandResult {
  success: boolean;
  room?: ChatRoom;
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<CreateRoomResult>) {
    super();
    this.success = data.success ?? false;
    this.room = data.room;
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}

// ==================== EDIT MESSAGE COMMAND ====================

export class EditMessageParams extends CommandParams {
  messageId: string;
  roomId: string;
  newContent: string;
  editedBy: string;

  constructor(data: Partial<EditMessageParams> & { 
    messageId: string; 
    roomId: string; 
    newContent: string; 
    editedBy: string; 
  }) {
    super();
    this.messageId = data.messageId;
    this.roomId = data.roomId;
    this.newContent = data.newContent;
    this.editedBy = data.editedBy;
  }
}

export class EditMessageResult extends CommandResult {
  success: boolean;
  message?: ChatMessage;
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<EditMessageResult>) {
    super();
    this.success = data.success ?? false;
    this.message = data.message;
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
    }
}

// ==================== DELETE MESSAGE COMMAND ====================

export class DeleteMessageParams extends CommandParams {
  messageId: string;
  roomId: string;
  deletedBy: string;

  constructor(data: Partial<DeleteMessageParams> & { 
    messageId: string; 
    roomId: string; 
    deletedBy: string; 
  }) {
    super();
    this.messageId = data.messageId;
    this.roomId = data.roomId;
    this.deletedBy = data.deletedBy;
  }
}

export class DeleteMessageResult extends CommandResult {
  success: boolean;
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<DeleteMessageResult>) {
    super();
    this.success = data.success ?? false;
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}

// ==================== REACT TO MESSAGE COMMAND ====================

export class ReactToMessageParams extends CommandParams {
  messageId: string;
  roomId: string;
  emoji: string;
  userId: string;
  action: 'add' | 'remove';

  constructor(data: Partial<ReactToMessageParams> & { 
    messageId: string; 
    roomId: string; 
    emoji: string; 
    userId: string;
    action: 'add' | 'remove';
  }) {
    super();
    this.messageId = data.messageId;
    this.roomId = data.roomId;
    this.emoji = data.emoji;
    this.userId = data.userId;
    this.action = data.action;
  }
}

export class ReactToMessageResult extends CommandResult {
  success: boolean;
  message?: ChatMessage;
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<ReactToMessageResult>) {
    super();
    this.success = data.success ?? false;
    this.message = data.message;
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}

// ==================== ACADEMY CHAT COMMAND ====================

export class AcademyChatParams extends CommandParams {
  sendMessageParams: SendMessageParams;
  academyId: string;
  personaId: string;
  sessionId?: string;
  trainingMode?: boolean;
  performanceMetrics?: {
    technicalAccuracy: number;
    collaborationQuality: number;
    humanSatisfaction: number;
    responseLatency: number;
    contextRelevance: number;
    innovationLevel: number;
  };

  constructor(data: Partial<AcademyChatParams> & { 
    sendMessageParams: SendMessageParams;
    academyId: string;
    personaId: string;
  }) {
    super();
    this.sendMessageParams = data.sendMessageParams;
    this.academyId = data.academyId;
    this.personaId = data.personaId;
    this.sessionId = data.sessionId;
    this.trainingMode = data.trainingMode ?? false;
    this.performanceMetrics = data.performanceMetrics;
  }
}

export class AcademyChatResult extends CommandResult {
  success: boolean;
  messageResult: SendMessageResult;
  academyProcessing?: {
    trainingDataExtracted: boolean;
    performanceEvaluated: boolean;
    evolutionTriggered: boolean;
    learningOpportunities: string[];
  };
  error?: string;
  environment: JTAGContext['environment'];
  timestamp: string;

  constructor(data: Partial<AcademyChatResult> & { messageResult: SendMessageResult }) {
    super();
    this.success = data.success ?? false;
    this.messageResult = data.messageResult;
    this.academyProcessing = data.academyProcessing;
    this.error = data.error;
    this.environment = data.environment ?? 'server';
    this.timestamp = data.timestamp ?? new Date().toISOString();
  }
}

// ==================== COMMAND UNION TYPES ====================

export type ChatCommandParams = 
  | SendMessageParams
  | JoinRoomParams
  | LeaveRoomParams
  | GetHistoryParams
  | CreateRoomParams
  | EditMessageParams
  | DeleteMessageParams
  | ReactToMessageParams
  | AcademyChatParams;

export type ChatCommandResult = 
  | SendMessageResult
  | JoinRoomResult
  | LeaveRoomResult
  | GetHistoryResult
  | CreateRoomResult
  | EditMessageResult
  | DeleteMessageResult
  | ReactToMessageResult
  | AcademyChatResult;