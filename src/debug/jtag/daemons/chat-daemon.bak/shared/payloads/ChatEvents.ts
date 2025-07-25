/**
 * ChatEvents - JTAG chat event payloads
 * 
 * Events are scoped to chat rooms - you listen to specific room events.
 * Event routing: /chat/room/{roomId}/events/{eventType}
 * 
 * Room-scoped Events:
 * - MessageSentEvent - Message sent to room
 * - MessageEditedEvent - Message edited in room  
 * - MessageDeletedEvent - Message deleted in room
 * - MessageReactionEvent - Reaction added/removed in room
 * - ParticipantJoinedEvent - Participant joined room
 * - ParticipantLeftEvent - Participant left room
 * - ParticipantTypingEvent - Participant typing in room
 * 
 * Academy Events (room-scoped):
 * - AcademyMessageEvent - Academy-aware message with performance tracking
 * - PersonaEvolutionEvent - Persona evolution triggered in room
 * - TrainingDataEvent - Training data extracted from room conversation
 * 
 * All extend JTAGPayload for transport compatibility.
 */

import { JTAGPayload } from '../../../../shared/JTAGTypes';
import type { ChatParticipant, ChatMessage } from '../types/ChatTypes';

// ==================== BASE ROOM EVENT ====================

/**
 * Base class for all room-scoped chat events
 */
export abstract class ChatRoomEvent extends JTAGPayload {
  roomId: string;
  timestamp: number;
  eventId: string;

  constructor(roomId: string) {
    super();
    this.roomId = roomId;
    this.timestamp = Date.now();
    this.eventId = `event-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  }
}

// ==================== MESSAGE EVENTS ====================

/**
 * Message sent to room event
 */
export class MessageSentEvent extends ChatRoomEvent {
  readonly eventType = 'message_sent';
  message: ChatMessage;
  sender: ChatParticipant;

  constructor(roomId: string, message: ChatMessage, sender: ChatParticipant) {
    super(roomId);
    this.message = message;
    this.sender = sender;
  }
}

/**
 * Message edited in room event
 */
export class MessageEditedEvent extends ChatRoomEvent {
  readonly eventType = 'message_edited';
  messageId: string;
  oldContent: string;
  newContent: string;
  editedBy: ChatParticipant;
  editedAt: number;

  constructor(
    roomId: string, 
    messageId: string, 
    oldContent: string, 
    newContent: string, 
    editedBy: ChatParticipant
  ) {
    super(roomId);
    this.messageId = messageId;
    this.oldContent = oldContent;
    this.newContent = newContent;
    this.editedBy = editedBy;
    this.editedAt = Date.now();
  }
}

/**
 * Message deleted in room event
 */
export class MessageDeletedEvent extends ChatRoomEvent {
  readonly eventType = 'message_deleted';
  messageId: string;
  deletedBy: ChatParticipant;
  deletedAt: number;

  constructor(roomId: string, messageId: string, deletedBy: ChatParticipant) {
    super(roomId);
    this.messageId = messageId;
    this.deletedBy = deletedBy;
    this.deletedAt = Date.now();
  }
}

/**
 * Message reaction added/removed in room event
 */
export class MessageReactionEvent extends ChatRoomEvent {
  readonly eventType = 'message_reaction';
  messageId: string;
  emoji: string;
  userId: string;
  userName: string;
  action: 'added' | 'removed';

  constructor(
    roomId: string, 
    messageId: string, 
    emoji: string, 
    userId: string, 
    userName: string,
    action: 'added' | 'removed'
  ) {
    super(roomId);
    this.messageId = messageId;
    this.emoji = emoji;
    this.userId = userId;
    this.userName = userName;
    this.action = action;
  }
}

// ==================== PARTICIPANT EVENTS ====================

/**
 * Participant joined room event
 */
export class ParticipantJoinedEvent extends ChatRoomEvent {
  readonly eventType = 'participant_joined';
  participant: ChatParticipant;
  participantCount: number;

  constructor(roomId: string, participant: ChatParticipant, participantCount: number) {
    super(roomId);
    this.participant = participant;
    this.participantCount = participantCount;
  }
}

/**
 * Participant left room event
 */
export class ParticipantLeftEvent extends ChatRoomEvent {
  readonly eventType = 'participant_left';
  participant: ChatParticipant;
  participantCount: number;

  constructor(roomId: string, participant: ChatParticipant, participantCount: number) {
    super(roomId);
    this.participant = participant;
    this.participantCount = participantCount;
  }
}

/**
 * Participant typing in room event
 */
export class ParticipantTypingEvent extends ChatRoomEvent {
  readonly eventType = 'participant_typing';
  participant: ChatParticipant;
  isTyping: boolean;

  constructor(roomId: string, participant: ChatParticipant, isTyping: boolean) {
    super(roomId);
    this.participant = participant;
    this.isTyping = isTyping;
  }
}

/**
 * Participant status changed in room event
 */
export class ParticipantStatusEvent extends ChatRoomEvent {
  readonly eventType = 'participant_status';
  participant: ChatParticipant;
  oldStatus: string;
  newStatus: string;

  constructor(roomId: string, participant: ChatParticipant, oldStatus: string, newStatus: string) {
    super(roomId);
    this.participant = participant;
    this.oldStatus = oldStatus;
    this.newStatus = newStatus;
  }
}

// ==================== ACADEMY EVENTS (ROOM-SCOPED) ====================

/**
 * Academy message event - Academy-aware message with performance tracking
 * Scoped to the room where the Academy conversation is happening
 */
export class AcademyMessageEvent extends ChatRoomEvent {
  readonly eventType = 'academy_message';
  message: ChatMessage;
  academyId: string;
  personaId: string;
  sessionId: string;
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

  constructor(
    roomId: string,
    message: ChatMessage,
    academyId: string,
    personaId: string,
    sessionId: string,
    performanceMetrics: AcademyMessageEvent['performanceMetrics'],
    learningData: AcademyMessageEvent['learningData']
  ) {
    super(roomId);
    this.message = message;
    this.academyId = academyId;
    this.personaId = personaId;
    this.sessionId = sessionId;
    this.performanceMetrics = performanceMetrics;
    this.learningData = learningData;
  }
}

/**
 * Persona evolution triggered in room event
 * Happens when a persona evolves during conversation in a room
 */
export class PersonaEvolutionEvent extends ChatRoomEvent {
  readonly eventType = 'persona_evolution';
  personaId: string;
  academyId: string;
  evolutionTrigger: {
    triggered: boolean;
    reason: 'performance_gap' | 'new_pattern' | 'collaboration_failure' | 'innovation_opportunity';
    capabilityGaps: string[];
    suggestedImprovements: string[];
    urgency: 'low' | 'medium' | 'high';
  };
  conversationContext: {
    messageId: string;
    conversationId: string;
    participantIds: string[];
    performanceScore: number;
  };

  constructor(
    roomId: string,
    personaId: string,
    academyId: string,
    evolutionTrigger: PersonaEvolutionEvent['evolutionTrigger'],
    conversationContext: PersonaEvolutionEvent['conversationContext']
  ) {
    super(roomId);
    this.personaId = personaId;
    this.academyId = academyId;
    this.evolutionTrigger = evolutionTrigger;
    this.conversationContext = conversationContext;
  }
}

/**
 * Training data extracted from room conversation event
 * Fired when valuable training data is identified in room conversation
 */
export class TrainingDataEvent extends ChatRoomEvent {
  readonly eventType = 'training_data';
  conversationId: string;
  messageIds: string[];
  trainingValue: number;
  dataType: 'dialogue' | 'correction' | 'example' | 'pattern' | 'collaboration';
  extractedData: {
    inputs: string[];
    outputs: string[];
    context: string[];
    quality: number;
    tags: string[];
  };
  academyRelevance: {
    personaIds: string[];
    capabilities: string[];
    learningObjectives: string[];
  };

  constructor(
    roomId: string,
    conversationId: string,
    messageIds: string[],
    trainingValue: number,
    dataType: TrainingDataEvent['dataType'],
    extractedData: TrainingDataEvent['extractedData'],
    academyRelevance: TrainingDataEvent['academyRelevance']
  ) {
    super(roomId);
    this.conversationId = conversationId;
    this.messageIds = messageIds;
    this.trainingValue = trainingValue;
    this.dataType = dataType;
    this.extractedData = extractedData;
    this.academyRelevance = academyRelevance;
  }
}

// ==================== EVENT UNION TYPES ====================

export type ChatEventType = 
  | MessageSentEvent
  | MessageEditedEvent
  | MessageDeletedEvent
  | MessageReactionEvent
  | ParticipantJoinedEvent
  | ParticipantLeftEvent
  | ParticipantTypingEvent
  | ParticipantStatusEvent
  | AcademyMessageEvent
  | PersonaEvolutionEvent
  | TrainingDataEvent;

export type ChatEventTypeString = 
  | 'message_sent'
  | 'message_edited'
  | 'message_deleted'
  | 'message_reaction'
  | 'participant_joined'
  | 'participant_left'
  | 'participant_typing'
  | 'participant_status'
  | 'academy_message'
  | 'persona_evolution'
  | 'training_data';

// ==================== EVENT FACTORY FUNCTIONS ====================

/**
 * Create message sent event
 */
export function createMessageSentEvent(
  roomId: string,
  message: ChatMessage,
  sender: ChatParticipant
): MessageSentEvent {
  return new MessageSentEvent(roomId, message, sender);
}

/**
 * Create Academy message event
 */
export function createAcademyMessageEvent(
  roomId: string,
  message: ChatMessage,
  academyId: string,
  personaId: string,
  sessionId: string,
  performanceMetrics: AcademyMessageEvent['performanceMetrics'],
  learningData: AcademyMessageEvent['learningData']
): AcademyMessageEvent {
  return new AcademyMessageEvent(
    roomId,
    message,
    academyId,
    personaId,
    sessionId,
    performanceMetrics,
    learningData
  );
}

/**
 * Create participant joined event
 */
export function createParticipantJoinedEvent(
  roomId: string,
  participant: ChatParticipant,
  participantCount: number
): ParticipantJoinedEvent {
  return new ParticipantJoinedEvent(roomId, participant, participantCount);
}

// ==================== EVENT ROUTING HELPERS ====================

/**
 * Generate event routing path for room-scoped events
 */
export function getRoomEventPath(roomId: string, eventType: ChatEventTypeString): string {
  return `/chat/room/${roomId}/events/${eventType}`;
}

/**
 * Generate Academy event routing path for room-scoped Academy events
 */
export function getAcademyRoomEventPath(
  roomId: string, 
  academyId: string, 
  eventType: 'academy_message' | 'persona_evolution' | 'training_data'
): string {
  return `/chat/room/${roomId}/academy/${academyId}/events/${eventType}`;
}

// ==================== EVENT CONSTANTS ====================

export const CHAT_EVENT_CONSTANTS = {
  // Event routing paths
  ROOM_EVENT_BASE_PATH: '/chat/room',
  ACADEMY_EVENT_BASE_PATH: '/chat/room/{roomId}/academy/{academyId}/events',
  
  // Event priorities for future event system
  MESSAGE_EVENT_PRIORITY: 'medium' as const,
  PARTICIPANT_EVENT_PRIORITY: 'medium' as const,
  ACADEMY_EVENT_PRIORITY: 'high' as const,
  EVOLUTION_EVENT_PRIORITY: 'high' as const,
  
  // Event TTL (time-to-live) in milliseconds
  MESSAGE_EVENT_TTL: 3600000,        // 1 hour
  PARTICIPANT_EVENT_TTL: 1800000,    // 30 minutes
  ACADEMY_EVENT_TTL: 7200000,        // 2 hours
  TRAINING_DATA_TTL: 86400000,       // 24 hours
  
  // Event batching
  MAX_EVENTS_PER_BATCH: 50,
  EVENT_BATCH_TIMEOUT: 1000,         // 1 second
  
  // Academy event thresholds
  TRAINING_VALUE_THRESHOLD: 0.5,     // Minimum training value to emit event
  EVOLUTION_TRIGGER_THRESHOLD: 0.3,  // Performance gap that triggers evolution
  PERFORMANCE_TRACKING_INTERVAL: 5000 // 5 seconds
} as const;