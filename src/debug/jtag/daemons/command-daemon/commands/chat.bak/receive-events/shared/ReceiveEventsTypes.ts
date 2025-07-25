/**
 * Receive Events Command - Shared Types
 * 
 * Types for establishing persona event streams and receiving real-time chat events.
 * Enables personas to respond to chat events like humans do.
 */

import { CommandParams, CommandResult } from '../../../../../shared/JTAGTypes';
import type { JTAGContext } from '../../../../../shared/JTAGTypes';
import type { ChatEventType } from '../../../chat-daemon/shared/ActivePersonaTypes';

/**
 * Receive Events Command Parameters - extends CommandParams
 */
export class ReceiveEventsParams extends CommandParams {
  personaId: string;
  eventTypes?: ChatEventType[];         // Specific event types to listen for
  roomIds?: string[];                   // Specific rooms to monitor
  
  // Stream configuration
  streamConfig?: EventStreamConfig;
  
  // Event filtering
  eventFilters?: EventFilters;
  
  // Response configuration
  responseConfig?: EventResponseConfig;
  
  // Multi-context awareness
  multiContextEnabled?: boolean;        // Enable cross-room context awareness
  contextRefreshInterval?: number;      // ms between context refreshes

  constructor(data: Partial<ReceiveEventsParams> = {}) {
    super();
    this.personaId = data.personaId || '';
    Object.assign(this, data);
  }
}

/**
 * Event Stream Configuration
 */
export interface EventStreamConfig {
  // Stream type
  streamType: 'websocket' | 'sse' | 'polling' | 'webhook';
  
  // Connection settings
  heartbeatInterval?: number;           // ms between heartbeats
  reconnectAttempts?: number;           // Number of reconnection attempts
  bufferSize?: number;                  // Event buffer size
  
  // Performance settings
  batchingEnabled?: boolean;            // Batch events for efficiency
  compressionEnabled?: boolean;         // Compress event data
  priorityBasedDelivery?: boolean;      // Deliver high-priority events first
  
  // Reliability settings
  guaranteedDelivery?: boolean;         // Ensure events are delivered
  duplicateDetection?: boolean;         // Detect and filter duplicates
  orderPreservation?: boolean;          // Preserve event order
}

/**
 * Event Filters
 */
export interface EventFilters {
  // Event type filters
  includedEventTypes?: ChatEventType[];
  excludedEventTypes?: ChatEventType[];
  
  // Priority filters
  minimumPriority?: 'low' | 'medium' | 'high' | 'urgent';
  
  // Content filters
  keywordFilters?: string[];           // Keywords to match
  senderFilters?: string[];            // Specific senders to monitor
  
  // Context filters
  relevanceThreshold?: number;         // 0-1 minimum relevance
  personalRelevanceWeight?: number;    // Weight for personal relevance
  academicRelevanceWeight?: number;    // Weight for academic relevance
  
  // Rate limiting
  maxEventsPerSecond?: number;         // Limit event rate
  burstLimit?: number;                 // Max events in burst
  cooldownPeriod?: number;             // ms cooldown after burst
}

/**
 * Event Response Configuration
 */
export interface EventResponseConfig {
  // Response behavior
  autoResponse?: boolean;              // Automatically respond to events
  responseDelay?: number;              // ms delay before responding
  
  // Response criteria
  responseCriteria?: ResponseCriteria;
  
  // Response generation
  responseGeneration?: ResponseGenerationConfig;
  
  // Response limits
  maxResponsesPerHour?: number;        // Limit response frequency
  responseTimeWindow?: number;         // ms time window for response limits
}

/**
 * Response Criteria
 */
export interface ResponseCriteria {
  // Relevance requirements
  minimumRelevance?: number;           // 0-1 minimum relevance to respond
  topicRelevance?: number;             // 0-1 minimum topic relevance
  personalRelevance?: number;          // 0-1 minimum personal relevance
  
  // Social requirements
  relationshipRequired?: boolean;      // Need relationship with participants
  invitationRequired?: boolean;        // Need explicit invitation
  expertiseRequired?: boolean;         // Need relevant expertise
  
  // Context requirements
  contextuallyAppropriate?: boolean;   // Must be appropriate for context
  noInterruption?: boolean;           // Don't interrupt ongoing conversations
  timingAppropriate?: boolean;        // Good timing to respond
  
  // Quality requirements
  valueAdded?: boolean;               // Response must add value
  originalContribution?: boolean;      // Must not repeat others
  clarityThreshold?: number;          // 0-1 minimum response clarity
}

/**
 * Response Generation Configuration
 */
export interface ResponseGenerationConfig {
  // AI provider settings
  aiProvider?: string;                 // 'openai', 'anthropic', 'mistral', etc.
  modelId?: string;                    // Specific model to use
  temperature?: number;                // 0-1 response creativity
  maxTokens?: number;                  // Maximum response length
  
  // Response characteristics
  responseStyle?: 'helpful' | 'educational' | 'casual' | 'professional' | 'adaptive';
  responseLength?: 'brief' | 'moderate' | 'detailed' | 'adaptive';
  personalityStrength?: number;        // 0-1 how much personality to show
  
  // Context integration
  includeMultiRoomContext?: boolean;   // Include context from other rooms
  includeRelationshipContext?: boolean; // Include relationship context
  includeAcademyContext?: boolean;     // Include Academy training context
  
  // Response optimization
  contextOptimization?: boolean;       // Optimize for context window
  tokenBudgetManagement?: boolean;     // Manage token usage
  responseQualityCheck?: boolean;      // Check response quality before sending
}

/**
 * Receive Events Result - extends CommandResult
 */
export class ReceiveEventsResult extends CommandResult {
  success: boolean;
  streamId?: string;                   // ID of established event stream
  personaId: string;
  connectionStatus: ConnectionStatus;
  
  // Stream details
  streamEndpoint?: string;             // WebSocket/SSE endpoint
  streamToken?: string;                // Authentication token
  
  // Configuration applied
  appliedConfig?: EventStreamConfig;
  appliedFilters?: EventFilters;
  
  // Performance metrics
  connectionTime?: number;             // ms time to establish connection
  eventsReceived?: number;             // Number of events received so far
  responsesGenerated?: number;         // Number of responses generated
  
  // Error handling
  error?: string;
  warnings?: string[];

  constructor(data: Partial<ReceiveEventsResult> & { personaId: string }) {
    super();
    this.success = data.success ?? false;
    this.streamId = data.streamId;
    this.personaId = data.personaId;
    this.connectionStatus = data.connectionStatus ?? 'disconnected';
    this.streamEndpoint = data.streamEndpoint;
    this.streamToken = data.streamToken;
    this.appliedConfig = data.appliedConfig;
    this.appliedFilters = data.appliedFilters;
    this.connectionTime = data.connectionTime;
    this.eventsReceived = data.eventsReceived ?? 0;
    this.responsesGenerated = data.responsesGenerated ?? 0;
    this.error = data.error;
    this.warnings = data.warnings;
  }
}

/**
 * Connection Status
 */
export type ConnectionStatus = 
  | 'connecting'                       // Establishing connection
  | 'connected'                        // Successfully connected
  | 'disconnected'                     // Not connected
  | 'reconnecting'                     // Attempting to reconnect
  | 'error'                           // Connection error
  | 'suspended';                      // Temporarily suspended

/**
 * Chat Event (received by persona)
 */
export interface ChatEvent {
  eventId: string;
  eventType: ChatEventType;
  timestamp: number;
  
  // Event source
  roomId: string;
  senderId?: string;                   // User/persona who triggered event
  
  // Event data
  eventData: EventData;
  
  // Context
  eventContext?: EventContext;
  
  // Metadata
  priority: 'low' | 'medium' | 'high' | 'urgent';
  requiresResponse?: boolean;
  responseDeadline?: number;           // timestamp for response deadline
}

/**
 * Event Data (varies by event type)
 */
export interface EventData {
  // Message events
  messageId?: string;
  messageContent?: string;
  messageType?: string;
  
  // User events
  userId?: string;
  userName?: string;
  userAction?: string;
  
  // Room events
  roomName?: string;
  topicChange?: string;
  activityLevel?: string;
  
  // Academy events
  sessionId?: string;
  milestone?: string;
  objective?: string;
  
  // Generic data
  [key: string]: any;
}

/**
 * Event Context
 */
export interface EventContext {
  // Conversation context
  conversationHistory?: EventMessage[];
  currentTopic?: string;
  activeParticipants?: string[];
  
  // Multi-room context
  relatedRooms?: string[];
  contextRelationships?: ContextRelationship[];
  
  // Academy context
  academySession?: AcademyEventContext;
  
  // Relationship context
  relationships?: ParticipantRelationship[];
}

/**
 * Event Message (context history)
 */
export interface EventMessage {
  messageId: string;
  senderId: string;
  content: string;
  timestamp: number;
  relevance: number;                   // 0-1 relevance to current event
}

/**
 * Context Relationship (between rooms/contexts)
 */
export interface ContextRelationship {
  primaryContext: string;
  relatedContext: string;
  relationshipType: string;
  strength: number;                    // 0-1 relationship strength
}

/**
 * Academy Event Context
 */
export interface AcademyEventContext {
  sessionId: string;
  currentObjectives: string[];
  sessionPhase: string;
  learningProgress: number;            // 0-1 session progress
  participantRoles: Map<string, string>;
}

/**
 * Participant Relationship
 */
export interface ParticipantRelationship {
  participantId: string;
  relationshipType: string;
  strength: number;                    // 0-1 relationship strength
  communicationHistory: string;
}

/**
 * Event Response (generated by persona)
 */
export interface EventResponse {
  responseId: string;
  eventId: string;                     // Event this responds to
  responseType: EventResponseType;
  
  // Response content
  content: string;
  attachments?: ResponseAttachment[];
  
  // Response metadata
  responseTime: number;                // ms time to generate response
  confidence: number;                  // 0-1 confidence in response
  priority: 'low' | 'medium' | 'high' | 'urgent';
  
  // Delivery configuration
  deliveryConfig?: ResponseDeliveryConfig;
  
  // Learning integration
  capabilitiesUsed?: string[];
  learningValue?: number;              // 0-1 learning value
  teachingMoment?: boolean;
}

/**
 * Event Response Type
 */
export type EventResponseType = 
  | 'direct_reply'                     // Direct reply to message
  | 'question'                         // Question for clarification
  | 'help_offer'                       // Offer to help
  | 'knowledge_share'                  // Share relevant knowledge
  | 'encouragement'                    // Supportive message
  | 'teaching_moment'                  // Educational content
  | 'social_interaction'               // Social/relationship building
  | 'proactive_contribution';          // Proactive valuable contribution

/**
 * Response Attachment
 */
export interface ResponseAttachment {
  type: 'link' | 'image' | 'file' | 'recipe' | 'technique';
  content: string;
  title?: string;
  description?: string;
}

/**
 * Response Delivery Configuration
 */
export interface ResponseDeliveryConfig {
  immediate?: boolean;                 // Send immediately
  scheduled?: number;                  // timestamp for scheduled delivery
  typingDelay?: number;                // ms to simulate typing
  confirmationRequired?: boolean;      // Require delivery confirmation
}

/**
 * Stream Health Metrics
 */
export interface StreamHealthMetrics {
  // Connection health
  connectionUptime: number;            // ms connected
  reconnectionCount: number;           // Number of reconnections
  lastHeartbeat: number;               // timestamp of last heartbeat
  
  // Event processing
  eventsReceived: number;              // Total events received
  eventsProcessed: number;             // Total events processed
  eventsFiltered: number;              // Events filtered out
  
  // Response performance
  responsesGenerated: number;          // Total responses generated
  averageResponseTime: number;         // ms average response time
  responseSuccessRate: number;         // 0-1 successful response rate
  
  // Error tracking
  processingErrors: number;            // Event processing errors
  connectionErrors: number;            // Connection errors
  responseErrors: number;              // Response generation errors
}

/**
 * Multi-Context State (for persona awareness)
 */
export interface MultiContextState {
  personaId: string;
  lastUpdated: number;                 // timestamp
  
  // Active contexts
  activeRooms: Map<string, RoomContextState>;
  activeDMs: Map<string, DMContextState>;
  
  // Context relationships
  contextRelationships: ContextRelationship[];
  
  // Attention distribution
  attentionDistribution: AttentionDistribution;
  
  // Academy integration
  academyContexts: Map<string, AcademyEventContext>;
}

/**
 * Room Context State
 */
export interface RoomContextState {
  roomId: string;
  attentionLevel: number;              // 0-1 attention allocated
  lastActivity: number;                // timestamp
  currentTopic: string;
  recentEvents: ChatEvent[];
  participantStates: Map<string, ParticipantState>;
}

/**
 * DM Context State
 */
export interface DMContextState {
  dmId: string;
  otherParticipant: string;
  attentionLevel: number;              // 0-1 attention allocated
  lastActivity: number;                // timestamp
  conversationGoal: string;
  recentEvents: ChatEvent[];
}

/**
 * Attention Distribution
 */
export interface AttentionDistribution {
  primaryFocus: string;                // Room/DM with primary attention
  secondaryFoci: string[];             // Other contexts being monitored
  attentionAllocations: Map<string, number>; // Attention per context
  maxConcurrentContexts: number;       // Max contexts to actively monitor
}

/**
 * Participant State
 */
export interface ParticipantState {
  participantId: string;
  status: 'active' | 'away' | 'typing';
  lastSeen: number;                    // timestamp
  relationshipLevel: number;           // 0-1 relationship strength
}