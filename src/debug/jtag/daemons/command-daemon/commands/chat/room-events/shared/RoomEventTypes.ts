/**
 * Room Event Types - Isolated Room-Scoped Event Coordination
 * ===========================================================
 * 
 * Events are room-scoped, not global. Everyone (humans, personas, widgets) subscribes 
 * to specific room events and can send events to coordinate within that room.
 * 
 * Key Principles:
 * - Events are isolated per room - no global event broadcast
 * - Participants subscribe to specific rooms they're interested in
 * - Both consumption (subscribe) AND production (send) of room events
 * - Widgets interact with room events for real-time coordination
 * - Personas participate in room event streams like any other participant
 * 
 * Architecture:
 * - Room-scoped event buses (one per room)
 * - Subscription management per participant per room
 * - Event routing only within subscribed rooms
 * - Widget-persona-human coordination through shared event streams
 */

import { CommandParams, CommandResult } from '../../../../../../shared/JTAGTypes';
import type { JTAGContext } from '../../../../../../shared/JTAGTypes';

// ========================
// Room Event Subscription
// ========================

/**
 * Room Event Subscription Parameters
 */
export class RoomEventSubscriptionParams extends CommandParams {
  participantId: string;               // Human, persona, or widget ID
  participantType: ParticipantType;
  roomId: string;
  
  // Subscription configuration
  eventTypes?: RoomEventType[];        // Specific events to subscribe to
  eventFilters?: RoomEventFilters;
  
  // Subscription behavior
  subscriptionOptions?: SubscriptionOptions;
  
  // Widget-specific options
  widgetOptions?: WidgetSubscriptionOptions;

  constructor(data: Partial<RoomEventSubscriptionParams> = {}) {
    super();
    this.participantId = data.participantId || '';
    this.participantType = data.participantType || 'human';
    this.roomId = data.roomId || '';
    Object.assign(this, data);
  }
}

/**
 * Participant Type
 */
export type ParticipantType = 
  | 'human'                           // Human user
  | 'persona'                         // AI persona (genomic or RAG)
  | 'widget'                          // Browser widget
  | 'system_agent'                    // System service
  | 'automation';                     // Automated process

/**
 * Room Event Types (room-scoped coordination events)
 */
export type RoomEventType = 
  // Chat coordination
  | 'message_sent'                    // New message in room
  | 'message_edited'                  // Message edited
  | 'message_deleted'                 // Message deleted
  | 'typing_started'                  // Someone started typing
  | 'typing_stopped'                  // Someone stopped typing
  
  // Participant coordination
  | 'participant_joined'              // Someone joined room
  | 'participant_left'                // Someone left room
  | 'participant_status_changed'      // Participant status change
  | 'attention_focused'               // Participant focused on room
  | 'attention_unfocused'             // Participant unfocused from room
  
  // Widget coordination
  | 'widget_activated'                // Widget became active
  | 'widget_deactivated'              // Widget deactivated
  | 'widget_state_changed'            // Widget state updated
  | 'widget_data_updated'             // Widget data changed
  | 'widget_interaction'              // User interacted with widget
  
  // Academy coordination
  | 'academy_session_started'         // Academy session began
  | 'academy_session_ended'           // Academy session ended
  | 'academy_phase_changed'           // Session phase transition
  | 'academy_objective_completed'     // Learning objective achieved
  | 'academy_milestone_reached'       // Training milestone
  | 'capability_demonstrated'         // Capability shown by participant
  | 'evolution_triggered'             // Persona evolution event
  
  // Collaboration coordination
  | 'screen_share_started'            // Screen sharing began
  | 'screen_share_stopped'            // Screen sharing ended
  | 'media_shared'                    // Media file shared
  | 'document_collaboration'          // Document editing
  | 'whiteboard_updated'              // Whiteboard changes
  
  // Room state coordination
  | 'room_topic_changed'              // Room topic updated
  | 'room_settings_changed'           // Room configuration changed
  | 'room_mode_changed'               // Room mode (e.g., Academy mode)
  | 'room_focus_changed'              // Room primary focus shifted
  
  // Real-time coordination
  | 'cursor_moved'                    // Participant cursor position
  | 'viewport_changed'                // Participant viewport
  | 'scroll_position'                 // Scroll position in shared view
  | 'selection_changed'               // Text/element selection
  
  // Custom events
  | 'custom_event';                   // Custom application events

/**
 * Room Event Filters
 */
export interface RoomEventFilters {
  // Participant filters
  fromParticipants?: string[];        // Only events from specific participants
  excludeParticipants?: string[];     // Exclude events from participants
  participantTypes?: ParticipantType[]; // Only events from participant types
  
  // Content filters
  messageFilters?: MessageEventFilters;
  
  // Priority filters
  priorityThreshold?: EventPriority;
  
  // Frequency filters
  rateLimitPerSecond?: number;        // Max events per second
  batchingEnabled?: boolean;          // Batch rapid events
  
  // Relevance filters
  relevanceThreshold?: number;        // 0-1 minimum relevance
  personalRelevance?: boolean;        // Filter based on personal relevance
  
  // Widget-specific filters
  widgetInteractionOnly?: boolean;    // Only widget interaction events
  uiEventsOnly?: boolean;             // Only UI-related events
}

/**
 * Message Event Filters
 */
export interface MessageEventFilters {
  // Message content
  containsKeywords?: string[];
  excludeKeywords?: string[];
  messageTypes?: string[];
  
  // Message characteristics
  minLength?: number;
  maxLength?: number;
  hasAttachments?: boolean;
  hasMentions?: boolean;
  
  // Sender characteristics
  fromPersonas?: boolean;
  fromHumans?: boolean;
  fromNewParticipants?: boolean;
}

/**
 * Subscription Options
 */
export interface SubscriptionOptions {
  // Delivery preferences
  deliveryMode: 'real_time' | 'batched' | 'on_demand';
  batchingDelay?: number;             // ms delay for batching
  
  // Reliability
  guaranteedDelivery?: boolean;       // Ensure event delivery
  duplicateFiltering?: boolean;       // Filter duplicate events
  orderPreservation?: boolean;        // Preserve event order
  
  // Performance
  bufferSize?: number;                // Event buffer size
  compressionEnabled?: boolean;       // Compress event data
  
  // Subscription lifecycle
  autoUnsubscribeOnLeave?: boolean;   // Unsubscribe when leaving room
  persistSubscription?: boolean;      // Keep subscription across sessions
  subscriptionTTL?: number;          // Subscription time-to-live (ms)
  
  // Backfill
  backfillOnSubscribe?: boolean;     // Get recent events when subscribing
  backfillCount?: number;            // Number of events to backfill
}

/**
 * Widget Subscription Options
 */
export interface WidgetSubscriptionOptions {
  // Widget coordination
  coordinateWithOtherWidgets?: boolean; // Coordinate with other widgets
  widgetSyncEvents?: boolean;         // Sync widget state events
  
  // UI event capturing
  captureUIEvents?: boolean;          // Capture UI interaction events
  mouseMoveEvents?: boolean;          // Track mouse movement
  keyboardEvents?: boolean;           // Track keyboard input
  scrollEvents?: boolean;             // Track scrolling
  
  // Widget lifecycle
  activationEvents?: boolean;         // Widget activation/deactivation
  stateChangeEvents?: boolean;        // Widget state changes
  dataUpdateEvents?: boolean;         // Widget data updates
  
  // Performance for widgets
  throttleUIEvents?: number;          // ms throttling for UI events
  debounceUserInput?: number;         // ms debouncing for input
  batchUIUpdates?: boolean;           // Batch UI update events
}

// ========================
// Room Event Structure
// ========================

/**
 * Room Event (the actual event sent/received)
 */
export interface RoomEvent {
  eventId: string;
  eventType: RoomEventType;
  roomId: string;
  timestamp: number;
  
  // Event source
  sourceParticipantId: string;
  sourceParticipantType: ParticipantType;
  
  // Event data
  eventData: RoomEventData;
  
  // Event metadata
  priority: EventPriority;
  ttl?: number;                       // Time-to-live in ms
  correlationId?: string;             // For event correlation
  
  // Delivery tracking
  deliveryAttempts?: number;
  lastDeliveryAttempt?: number;
  
  // Widget interaction data
  widgetContext?: WidgetEventContext;
  
  // Academy context
  academyContext?: AcademyEventContext;
}

/**
 * Event Priority
 */
export type EventPriority = 
  | 'low'                            // Background events
  | 'normal'                         // Standard events
  | 'high'                           // Important events
  | 'urgent'                         // Critical events
  | 'immediate';                     // Must be delivered immediately

/**
 * Room Event Data (varies by event type)
 */
export interface RoomEventData {
  // Message events
  messageId?: string;
  messageContent?: string;
  messageType?: string;
  editedContent?: string;             // For message_edited
  
  // Participant events
  participantId?: string;
  participantName?: string;
  statusChange?: string;
  joinMethod?: string;                // How they joined
  
  // Widget events
  widgetId?: string;
  widgetType?: string;
  widgetState?: any;
  interactionType?: string;
  interactionData?: any;
  
  // Academy events
  sessionId?: string;
  objectiveId?: string;
  milestoneId?: string;
  capabilityId?: string;
  evolutionDetails?: any;
  
  // Media events
  mediaId?: string;
  mediaType?: string;
  mediaUrl?: string;
  sharingDuration?: number;
  
  // Collaboration events
  documentId?: string;
  cursorPosition?: { x: number; y: number };
  viewportInfo?: ViewportInfo;
  selectionInfo?: SelectionInfo;
  
  // Room state events
  previousValue?: any;
  newValue?: any;
  changeReason?: string;
  
  // Custom event data
  customData?: Record<string, any>;
}

/**
 * Widget Event Context
 */
export interface WidgetEventContext {
  widgetId: string;
  widgetType: string;
  widgetPosition?: { x: number; y: number; width: number; height: number };
  
  // UI context
  elementId?: string;
  elementType?: string;
  interactionContext?: string;
  
  // User interaction
  userAction?: string;
  inputData?: any;
  interactionSequence?: number;       // Sequence in interaction chain
  
  // Widget state
  widgetStateSnapshot?: any;
  stateChangeDelta?: any;
}

/**
 * Academy Event Context
 */
export interface AcademyEventContext {
  sessionId: string;
  sessionPhase: string;
  currentObjectives: string[];
  
  // Learning context
  learningProgress?: number;          // 0-1 session progress
  capabilityLevel?: number;           // 0-1 capability level
  
  // Performance context
  performanceMetrics?: PerformanceMetrics;
  
  // Evolution context
  evolutionReadiness?: number;        // 0-1 readiness for evolution
  evolutionTriggers?: string[];
}

/**
 * Performance Metrics
 */
export interface PerformanceMetrics {
  responseTime?: number;              // ms response time
  accuracyScore?: number;             // 0-1 accuracy
  engagementLevel?: number;           // 0-1 engagement
  collaborationScore?: number;        // 0-1 collaboration effectiveness
}

/**
 * Viewport Info
 */
export interface ViewportInfo {
  x: number;
  y: number;
  width: number;
  height: number;
  zoom?: number;
  scrollPosition?: { x: number; y: number };
}

/**
 * Selection Info
 */
export interface SelectionInfo {
  startPosition: { x: number; y: number };
  endPosition: { x: number; y: number };
  selectedText?: string;
  selectedElements?: string[];        // Element IDs
  selectionType: 'text' | 'element' | 'region';
}

// ========================
// Room Event Subscription Management
// ========================

/**
 * Room Event Subscription Result
 */
export class RoomEventSubscriptionResult extends CommandResult {
  success: boolean;
  subscriptionId: string;
  participantId: string;
  roomId: string;
  
  // Subscription details
  subscribedEventTypes: RoomEventType[];
  subscriptionStatus: SubscriptionStatus;
  
  // Event stream info
  eventStreamEndpoint?: string;       // WebSocket/SSE endpoint
  eventStreamToken?: string;          // Authentication token
  
  // Backfill data
  backfilledEvents?: RoomEvent[];
  backfillCount?: number;
  
  // Performance info
  subscriptionLatency?: number;       // ms to establish subscription
  expectedEventRate?: number;         // events per second
  
  // Error handling
  error?: string;
  warnings?: string[];

  constructor(data: Partial<RoomEventSubscriptionResult> & { 
    participantId: string; 
    roomId: string; 
  }) {
    super();
    this.success = data.success ?? false;
    this.subscriptionId = data.subscriptionId || `sub_${Date.now()}`;
    this.participantId = data.participantId;
    this.roomId = data.roomId;
    this.subscribedEventTypes = data.subscribedEventTypes || [];
    this.subscriptionStatus = data.subscriptionStatus || 'pending';
    this.eventStreamEndpoint = data.eventStreamEndpoint;
    this.eventStreamToken = data.eventStreamToken;
    this.backfilledEvents = data.backfilledEvents;
    this.backfillCount = data.backfillCount;
    this.subscriptionLatency = data.subscriptionLatency;
    this.expectedEventRate = data.expectedEventRate;
    this.error = data.error;
    this.warnings = data.warnings;
  }
}

/**
 * Subscription Status
 */
export type SubscriptionStatus = 
  | 'pending'                        // Subscription being established
  | 'active'                         // Actively receiving events
  | 'paused'                         // Temporarily paused
  | 'error'                          // Subscription error
  | 'expired'                        // Subscription expired
  | 'cancelled';                     // Subscription cancelled

// ========================
// Room Event Publishing
// ========================

/**
 * Send Room Event Parameters
 */
export class SendRoomEventParams extends CommandParams {
  roomId: string;
  sourceParticipantId: string;
  sourceParticipantType: ParticipantType;
  
  // Event details
  eventType: RoomEventType;
  eventData: RoomEventData;
  
  // Event options
  priority?: EventPriority;
  ttl?: number;                       // Time-to-live in ms
  correlationId?: string;
  
  // Delivery options
  deliveryOptions?: EventDeliveryOptions;
  
  // Target filtering
  targetFilters?: EventTargetFilters;

  constructor(data: Partial<SendRoomEventParams> = {}) {
    super();
    this.roomId = data.roomId || '';
    this.sourceParticipantId = data.sourceParticipantId || '';
    this.sourceParticipantType = data.sourceParticipantType || 'human';
    this.eventType = data.eventType || 'custom_event';
    this.eventData = data.eventData || {};
    this.priority = data.priority || 'normal';
    this.ttl = data.ttl;
    this.correlationId = data.correlationId;
    this.deliveryOptions = data.deliveryOptions;
    this.targetFilters = data.targetFilters;
  }
}

/**
 * Event Delivery Options
 */
export interface EventDeliveryOptions {
  // Delivery guarantees
  guaranteedDelivery?: boolean;       // Ensure delivery to all subscribers
  ackRequired?: boolean;              // Require acknowledgment
  retryAttempts?: number;             // Number of retry attempts
  
  // Delivery timing
  immediateDelivery?: boolean;        // Deliver immediately
  batchWithOthers?: boolean;          // Can be batched with other events
  deliveryDelay?: number;             // ms delay before delivery
  
  // Target selection
  deliverToAll?: boolean;             // Deliver to all room subscribers
  deliverToActive?: boolean;          // Only to active participants
  deliverToWidgets?: boolean;         // Include widgets in delivery
  deliverToPersonas?: boolean;        // Include personas in delivery
  
  // Reliability
  duplicateDetection?: boolean;       // Detect and filter duplicates
  orderPreservation?: boolean;        // Preserve order with other events
}

/**
 * Event Target Filters
 */
export interface EventTargetFilters {
  // Include/exclude participants
  includeParticipants?: string[];
  excludeParticipants?: string[];
  
  // Include/exclude participant types
  includeParticipantTypes?: ParticipantType[];
  excludeParticipantTypes?: ParticipantType[];
  
  // Activity-based targeting
  activeParticipantsOnly?: boolean;
  participantsWithAttention?: boolean; // Only participants with room attention
  
  // Widget targeting
  specificWidgets?: string[];         // Target specific widgets
  widgetTypes?: string[];             // Target widget types
  
  // Academy targeting
  academyParticipantsOnly?: boolean;  // Only Academy session participants
  specificRoles?: string[];           // Target specific Academy roles
  
  // Custom targeting
  customFilters?: Record<string, any>;
}

/**
 * Send Room Event Result
 */
export class SendRoomEventResult extends CommandResult {
  success: boolean;
  eventId: string;
  roomId: string;
  
  // Delivery metrics
  recipientCount: number;
  deliveryTime: number;               // ms time to deliver
  acknowledgedBy?: string[];          // Participants who acknowledged
  
  // Event processing
  eventProcessingTime?: number;       // ms time to process event
  routingTime?: number;               // ms time to route to subscribers
  
  // Delivery status
  deliveryStatus: EventDeliveryStatus;
  failedDeliveries?: FailedDelivery[];
  
  // Error handling
  error?: string;
  warnings?: string[];

  constructor(data: Partial<SendRoomEventResult> & { roomId: string }) {
    super();
    this.success = data.success ?? false;
    this.eventId = data.eventId || `evt_${Date.now()}`;
    this.roomId = data.roomId;
    this.recipientCount = data.recipientCount || 0;
    this.deliveryTime = data.deliveryTime || 0;
    this.acknowledgedBy = data.acknowledgedBy;
    this.eventProcessingTime = data.eventProcessingTime;
    this.routingTime = data.routingTime;
    this.deliveryStatus = data.deliveryStatus || 'pending';
    this.failedDeliveries = data.failedDeliveries;
    this.error = data.error;
    this.warnings = data.warnings;
  }
}

/**
 * Event Delivery Status
 */
export type EventDeliveryStatus = 
  | 'pending'                        // Delivery in progress
  | 'delivered'                      // Successfully delivered
  | 'partial'                        // Delivered to some recipients
  | 'failed'                         // Delivery failed
  | 'timeout';                       // Delivery timed out

/**
 * Failed Delivery
 */
export interface FailedDelivery {
  participantId: string;
  participantType: ParticipantType;
  failureReason: string;
  retryScheduled?: boolean;
  nextRetryAt?: number;               // timestamp
}

// ========================
// Room Event Bus Management
// ========================

/**
 * Room Event Bus State
 */
export interface RoomEventBusState {
  roomId: string;
  
  // Subscribers
  subscribers: Map<string, RoomEventSubscription>;
  activeSubscribers: number;
  
  // Event statistics
  eventsSent: number;
  eventsDelivered: number;
  eventsFailed: number;
  averageDeliveryTime: number;
  
  // Performance metrics
  busHealth: BusHealth;
  throughputMetrics: ThroughputMetrics;
  
  // Bus configuration
  busConfig: RoomEventBusConfig;
}

/**
 * Room Event Subscription (internal tracking)
 */
export interface RoomEventSubscription {
  subscriptionId: string;
  participantId: string;
  participantType: ParticipantType;
  
  // Subscription details
  eventTypes: Set<RoomEventType>;
  filters: RoomEventFilters;
  options: SubscriptionOptions;
  
  // Subscription state
  status: SubscriptionStatus;
  createdAt: number;
  lastActivity: number;
  lastEventDelivered: number;
  
  // Performance tracking
  eventsReceived: number;
  eventsFiltered: number;
  averageProcessingTime: number;
  
  // Connection info
  connectionInfo: ConnectionInfo;
}

/**
 * Connection Info
 */
export interface ConnectionInfo {
  connectionType: 'websocket' | 'sse' | 'polling' | 'webhook';
  connectionId: string;
  endpoint?: string;
  isConnected: boolean;
  lastHeartbeat?: number;
  connectionQuality: number;          // 0-1 connection quality
}

/**
 * Bus Health
 */
export interface BusHealth {
  healthScore: number;                // 0-1 overall health
  
  // Health indicators
  connectionHealth: number;           // 0-1 connection health
  deliveryHealth: number;             // 0-1 delivery reliability
  performanceHealth: number;          // 0-1 performance health
  
  // Issues
  activeIssues: BusIssue[];
  warningCount: number;
  errorCount: number;
  
  // Recovery
  lastHealthCheck: number;
  recoveryAttempts: number;
}

/**
 * Bus Issue
 */
export interface BusIssue {
  issueId: string;
  issueType: 'connection' | 'delivery' | 'performance' | 'capacity';
  severity: 'low' | 'medium' | 'high' | 'critical';
  description: string;
  timestamp: number;
  affectedParticipants: string[];
  resolutionStatus: 'open' | 'investigating' | 'resolving' | 'resolved';
}

/**
 * Throughput Metrics
 */
export interface ThroughputMetrics {
  eventsPerSecond: number;
  peakEventsPerSecond: number;
  averageEventSize: number;           // bytes
  
  // Capacity
  maxSubscribers: number;
  currentSubscribers: number;
  maxEventsPerSecond: number;
  
  // Performance
  averageLatency: number;             // ms
  deliverySuccessRate: number;        // 0-1
  
  // Resource usage
  memoryUsage: number;                // bytes
  cpuUsage: number;                   // 0-1
  networkBandwidth: number;           // bytes per second
}

/**
 * Room Event Bus Configuration
 */
export interface RoomEventBusConfig {
  // Capacity limits
  maxSubscribers: number;
  maxEventsPerSecond: number;
  maxEventSize: number;               // bytes
  maxEventQueueSize: number;
  
  // Performance tuning
  batchingEnabled: boolean;
  batchSize: number;
  batchingDelay: number;              // ms
  compressionEnabled: boolean;
  
  // Reliability
  retryAttempts: number;
  retryDelay: number;                 // ms
  duplicateDetection: boolean;
  orderPreservation: boolean;
  
  // Cleanup
  subscriptionTTL: number;            // ms
  eventTTL: number;                   // ms
  cleanupInterval: number;            // ms
  
  // Health monitoring
  healthCheckInterval: number;        // ms
  performanceMonitoring: boolean;
  alertingEnabled: boolean;
}

// ========================
// Widget-Specific Event Coordination
// ========================

/**
 * Widget Event Coordination
 */
export interface WidgetEventCoordination {
  // Widget registry
  activeWidgets: Map<string, WidgetEventParticipant>;
  
  // Widget synchronization
  widgetSyncEvents: WidgetSyncEvent[];
  
  // Widget collaboration
  collaborativeWidgets: string[];     // Widgets working together
  widgetInteractions: WidgetInteraction[];
  
  // Widget state management
  sharedWidgetState: Map<string, any>;
  widgetStateHistory: WidgetStateSnapshot[];
}

/**
 * Widget Event Participant
 */
export interface WidgetEventParticipant {
  widgetId: string;
  widgetType: string;
  roomId: string;
  
  // Widget characteristics
  isInteractive: boolean;
  canCoordinate: boolean;
  canShareState: boolean;
  
  // Event capabilities
  eventCapabilities: WidgetEventCapabilities;
  
  // Current state
  currentState: any;
  lastActivity: number;
  
  // Performance
  responseTime: number;               // ms average response time
  eventProcessingRate: number;        // events per second
}

/**
 * Widget Event Capabilities
 */
export interface WidgetEventCapabilities {
  // Event types the widget can send
  canSendEvents: RoomEventType[];
  
  // Event types the widget subscribes to
  subscribesToEvents: RoomEventType[];
  
  // Coordination capabilities
  canCoordinateWithOtherWidgets: boolean;
  canShareStateWithWidgets: boolean;
  canSynchronizeWithPersonas: boolean;
  
  // UI event capabilities
  capturesUIEvents: boolean;
  generatesUIEvents: boolean;
  canControlOtherWidgets: boolean;
}

/**
 * Widget Sync Event
 */
export interface WidgetSyncEvent {
  syncEventId: string;
  timestamp: number;
  
  // Synchronization details
  sourceWidgetId: string;
  targetWidgetIds: string[];
  syncType: 'state_sync' | 'action_sync' | 'data_sync' | 'ui_sync';
  
  // Sync data
  syncData: any;
  syncDelta?: any;                    // Only the changes
  
  // Sync status
  syncStatus: 'pending' | 'in_progress' | 'completed' | 'failed';
  participantAcknowledgments: Map<string, boolean>;
}

/**
 * Widget Interaction
 */
export interface WidgetInteraction {
  interactionId: string;
  timestamp: number;
  
  // Interaction participants
  initiatorWidgetId: string;
  respondingWidgetIds: string[];
  
  // Interaction type
  interactionType: 'data_request' | 'state_update' | 'coordinate_action' | 'ui_update';
  
  // Interaction data
  requestData?: any;
  responseData?: any;
  
  // Interaction flow
  interactionStatus: 'initiated' | 'in_progress' | 'completed' | 'failed';
  interactionSteps: InteractionStep[];
}

/**
 * Interaction Step
 */
export interface InteractionStep {
  stepId: string;
  timestamp: number;
  participantId: string;
  action: string;
  data?: any;
  status: 'pending' | 'completed' | 'failed';
}

/**
 * Widget State Snapshot
 */
export interface WidgetStateSnapshot {
  snapshotId: string;
  timestamp: number;
  widgetId: string;
  widgetState: any;
  changeReason: string;
  triggeredByEvent?: string;          // Event ID that triggered change
}