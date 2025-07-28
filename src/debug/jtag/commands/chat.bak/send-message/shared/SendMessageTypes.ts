/**
 * Send Message Command - Shared Types
 * 
 * Common types and interfaces for sending chat messages through the chat daemon.
 * Used by both browser and server implementations for persona-chat integration.
 */

import { CommandParams, CommandResult } from '@shared/JTAGTypes';
import type { JTAGContext } from '@shared/JTAGTypes';

/**
 * Send Message Command Parameters - extends CommandParams
 */
export class SendMessageParams extends CommandParams {
  roomId: string;
  content: string;
  senderId?: string;                  // Persona ID or user ID
  messageType?: MessageType;
  replyToMessageId?: string;
  mentionUserIds?: string[];
  attachments?: MessageAttachment[];
  
  // Academy integration
  academyContext?: AcademyMessageContext;
  
  // Learning integration  
  learningValue?: number;             // 0-1 learning value
  teachingMoment?: boolean;           // Is this a teaching moment
  capabilitiesUsed?: string[];        // Capabilities demonstrated
  
  // Message characteristics
  urgency?: 'low' | 'medium' | 'high' | 'urgent';
  requiresResponse?: boolean;         // Does sender expect response
  conversationId?: string;            // Thread this belongs to
  
  // Delivery options
  deliveryOptions?: MessageDeliveryOptions;

  constructor(data: Partial<SendMessageParams> = {}) {
    super();
    this.roomId = data.roomId || '';
    this.content = data.content || '';
    Object.assign(this, data);
  }
}

/**
 * Message Types
 */
export type MessageType = 
  | 'chat'                           // Regular chat message
  | 'educational_response'           // Teaching/educational content  
  | 'question'                       // Question for others
  | 'help_request'                   // Request for help
  | 'knowledge_share'                // Sharing knowledge/expertise
  | 'encouragement'                  // Supportive/encouraging message
  | 'feedback'                       // Feedback on others' contributions
  | 'announcement'                   // Important announcement
  | 'system_message'                 // System-generated message
  | 'academy_instruction';           // Academy training instruction

/**
 * Message Attachment
 */
export interface MessageAttachment {
  type: 'image' | 'file' | 'link' | 'recipe' | 'technique';
  url?: string;
  filepath?: string;
  filename?: string;
  title?: string;
  description?: string;
  metadata?: Record<string, any>;
}

/**
 * Academy Message Context
 */
export interface AcademyMessageContext {
  sessionId: string;
  currentObjectives: string[];
  sessionPhase: string;
  trainingType: 'collaborative' | 'competitive' | 'socratic' | 'demonstration';
  
  // Learning context
  buildingUpon?: string[];           // Previous messages this builds on
  enablesLearning?: string[];        // What this message enables learning
  prerequisiteKnowledge?: string[];  // Knowledge needed to understand
  
  // Training coordination
  expectedResponses?: ExpectedResponse[];
  followUpOpportunities?: string[];
  
  // Assessment context
  assessmentCriteria?: string[];
  performanceIndicators?: string[];
}

/**
 * Expected Response
 */
export interface ExpectedResponse {
  fromParticipantType: 'student' | 'teacher' | 'peer' | 'any';
  responseType: 'question' | 'answer' | 'demonstration' | 'feedback';
  timeframe: 'immediate' | 'soon' | 'eventually';
  optional: boolean;
}

/**
 * Message Delivery Options
 */
export interface MessageDeliveryOptions {
  // Timing
  sendImmediately?: boolean;
  scheduledTime?: number;            // timestamp for scheduled delivery
  typingIndicator?: boolean;         // Show typing indicator
  
  // Formatting
  markdown?: boolean;                // Process markdown
  emojis?: boolean;                  // Process emojis
  mentions?: boolean;                // Process @mentions
  
  // Delivery confirmation
  requireConfirmation?: boolean;     // Require delivery confirmation
  readReceipts?: boolean;            // Request read receipts
  
  // Retry settings
  retryAttempts?: number;            // Number of retry attempts
  retryDelay?: number;               // ms between retries
}

/**
 * Send Message Result - extends CommandResult
 */
export class SendMessageResult extends CommandResult {
  success: boolean;
  messageId?: string;                // ID of sent message
  roomId: string;
  senderId?: string;
  timestamp: string;
  deliveryStatus: DeliveryStatus;
  
  // Message content info
  contentLength: number;
  messageType?: MessageType;
  
  // Delivery metrics
  deliveryTime?: number;             // ms time to deliver
  recipientCount?: number;           // Number of recipients
  
  // Academy integration
  academyIntegration?: {
    sessionId?: string;
    capabilityTracking?: boolean;
    learningValueRecorded?: boolean;
    evolutionTriggered?: boolean;
  };
  
  // Error handling
  error?: string;
  warnings?: string[];

  constructor(data: Partial<SendMessageResult> & { roomId: string }) {
    super();
    this.success = data.success ?? false;
    this.messageId = data.messageId;
    this.roomId = data.roomId;
    this.senderId = data.senderId;
    this.timestamp = data.timestamp ?? new Date().toISOString();
    this.deliveryStatus = data.deliveryStatus ?? 'pending';
    this.contentLength = data.contentLength ?? 0;
    this.messageType = data.messageType;
    this.deliveryTime = data.deliveryTime;
    this.recipientCount = data.recipientCount;
    this.academyIntegration = data.academyIntegration;
    this.error = data.error;
    this.warnings = data.warnings;
  }
}

/**
 * Delivery Status
 */
export type DeliveryStatus = 
  | 'pending'                        // Queued for delivery
  | 'sending'                        // Currently sending
  | 'delivered'                      // Successfully delivered
  | 'failed'                         // Delivery failed
  | 'retrying'                       // Retrying delivery
  | 'scheduled';                     // Scheduled for future delivery

/**
 * Message Validation
 */
export interface MessageValidation {
  contentValid: boolean;
  lengthValid: boolean;
  formatValid: boolean;
  permissionsValid: boolean;
  rateLimitValid: boolean;
  errors: string[];
  warnings: string[];
}

/**
 * Persona Message Context (for AI-generated messages)
 */
export interface PersonaMessageContext {
  personaId: string;
  personaType: 'genomic_persona' | 'rag_ai';
  
  // Context awareness
  multiRoomContext?: {
    activeRooms: string[];
    primaryRoom: string;
    contextRelationships: ContextRelationship[];
  };
  
  // Conversation context
  conversationHistory?: ConversationHistoryContext;
  relationshipContext?: RelationshipContext;
  
  // Learning context
  learningContext?: LearningContext;
  teachingContext?: TeachingContext;
  
  // Response context
  triggeringEvent?: TriggeringEvent;
  responseStrategy?: ResponseStrategy;
}

/**
 * Context Relationship
 */
export interface ContextRelationship {
  primaryContext: string;            // Room/DM ID
  relatedContext: string;            // Room/DM ID
  relationshipType: 'topic_overlap' | 'shared_participants' | 'temporal_proximity';
  strength: number;                  // 0-1 relationship strength
  description: string;
}

/**
 * Conversation History Context
 */
export interface ConversationHistoryContext {
  recentMessages: RecentMessage[];
  conversationTopic: string;
  conversationPhase: string;
  keyParticipants: string[];
  conversationGoal?: string;
}

/**
 * Recent Message
 */
export interface RecentMessage {
  messageId: string;
  senderId: string;
  content: string;
  timestamp: number;
  relevance: number;                 // 0-1 relevance to current context
  importance: number;                // 0-1 importance for understanding
}

/**
 * Relationship Context
 */
export interface RelationshipContext {
  relationships: ParticipantRelationship[];
  socialDynamics: SocialDynamics;
  communicationHistory: CommunicationHistory;
}

/**
 * Participant Relationship
 */
export interface ParticipantRelationship {
  participantId: string;
  relationshipType: 'friend' | 'colleague' | 'mentor' | 'student' | 'collaborator' | 'stranger';
  strength: number;                  // 0-1 relationship strength
  history: string;                   // Brief relationship history
  communicationStyle: string;       // How they usually communicate
}

/**
 * Learning Context
 */
export interface LearningContext {
  currentLearningGoals: string[];
  learningOpportunities: string[];
  knowledgeGaps: string[];
  learningStyle: string;
  recentLearning: string[];
}

/**
 * Teaching Context
 */
export interface TeachingContext {
  teachingOpportunities: string[];
  studentNeeds: string[];
  availableKnowledge: string[];
  teachingStyle: string;
  recentTeaching: string[];
}

/**
 * Triggering Event
 */
export interface TriggeringEvent {
  eventType: string;
  eventData: Record<string, any>;
  timestamp: number;
  urgency: number;                   // 0-1 urgency level
  relevance: number;                 // 0-1 relevance to persona
}

/**
 * Response Strategy
 */
export interface ResponseStrategy {
  strategyType: 'immediate_response' | 'thoughtful_response' | 'question_based' | 'teaching_moment';
  reasoning: string;
  expectedOutcome: string;
  alternativeStrategies: string[];
}

/**
 * Social Dynamics
 */
export interface SocialDynamics {
  groupDynamics: string;
  conversationTone: string;
  participationLevels: Map<string, number>;
  socialRoles: Map<string, string>;
}

/**
 * Communication History
 */
export interface CommunicationHistory {
  totalInteractions: number;
  recentInteractionQuality: number;  // 0-1 quality rating
  communicationPreferences: string[];
  successfulInteractionPatterns: string[];
}