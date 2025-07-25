/**
 * Send Room Event Command - Shared Types
 * 
 * Types for sending events within room-scoped coordination system.
 * Enables all participants (humans, personas, widgets) to send events to each other.
 */

import { CommandParams, CommandResult } from '../../../../../shared/JTAGTypes';
import type { JTAGContext } from '../../../../../shared/JTAGTypes';
import type { 
  RoomEventType, 
  RoomEventData, 
  EventPriority, 
  ParticipantType,
  EventDeliveryOptions,
  EventTargetFilters,
  EventDeliveryStatus,
  FailedDelivery
} from '../../room-events/shared/RoomEventTypes';

/**
 * Send Room Event Command Parameters - extends CommandParams
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
  correlationId?: string;             // For event correlation/tracking
  
  // Delivery configuration
  deliveryOptions?: EventDeliveryOptions;
  
  // Target filtering
  targetFilters?: EventTargetFilters;
  
  // Widget-specific options
  widgetEventOptions?: WidgetEventOptions;
  
  // Academy integration
  academyEventOptions?: AcademyEventOptions;

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
    this.widgetEventOptions = data.widgetEventOptions;
    this.academyEventOptions = data.academyEventOptions;
  }
}

/**
 * Widget Event Options
 */
export interface WidgetEventOptions {
  // Widget coordination
  triggerWidgetSync?: boolean;        // Trigger synchronization with other widgets
  updateSharedState?: boolean;        // Update shared widget state
  
  // Widget interaction
  interactionType?: WidgetInteractionType;
  interactionData?: any;
  
  // UI event propagation
  propagateToOtherWidgets?: boolean;  // Propagate to coordinating widgets
  uiUpdateRequired?: boolean;         // Requires UI update
  
  // Widget state management
  stateSnapshot?: any;                // Current widget state snapshot
  stateDelta?: any;                   // Changes since last state
  
  // Widget collaboration
  collaborativeAction?: boolean;      // Is this a collaborative action
  collaboratingWidgets?: string[];    // Other widgets involved
  
  // Performance hints
  lowPriorityUIUpdate?: boolean;      // UI update can be deferred
  batchWithOtherUpdates?: boolean;    // Can be batched with other updates
}

/**
 * Widget Interaction Type
 */
export type WidgetInteractionType = 
  | 'user_click'                     // User clicked on widget
  | 'user_input'                     // User provided input
  | 'data_request'                   // Widget requesting data
  | 'state_sync'                     // Widget state synchronization
  | 'coordination_request'           // Request coordination with other widgets
  | 'ui_update'                      // UI update event
  | 'widget_focus'                   // Widget gained/lost focus
  | 'widget_resize'                  // Widget resized
  | 'widget_move'                    // Widget moved
  | 'custom_interaction';            // Custom widget interaction

/**
 * Academy Event Options
 */
export interface AcademyEventOptions {
  // Academy session context
  sessionId?: string;
  sessionPhase?: string;
  currentObjectives?: string[];
  
  // Learning integration
  learningValue?: number;             // 0-1 learning value of this event
  teachingMoment?: boolean;           // Is this a teaching moment
  capabilityDemonstration?: string[]; // Capabilities being demonstrated
  
  // Performance tracking
  performanceMetrics?: AcademyPerformanceMetrics;
  
  // Evolution triggers
  evolutionTrigger?: boolean;         // Does this event trigger evolution
  evolutionContext?: string;          // Context for evolution
  
  // Academy coordination
  coordinateWithTrainer?: boolean;    // Coordinate with trainer persona
  updateTrainingProgress?: boolean;   // Update training progress
  
  // Assessment integration
  assessmentData?: AssessmentData;
  
  // Milestone tracking
  milestoneProgress?: MilestoneProgress;
}

/**
 * Academy Performance Metrics
 */
export interface AcademyPerformanceMetrics {
  responseTime?: number;              // ms response time
  accuracyScore?: number;             // 0-1 accuracy
  engagementLevel?: number;           // 0-1 engagement
  collaborationScore?: number;        // 0-1 collaboration effectiveness
  learningEfficiency?: number;        // 0-1 learning efficiency
  teachingEffectiveness?: number;     // 0-1 teaching effectiveness
}

/**
 * Assessment Data
 */
export interface AssessmentData {
  assessmentType: 'formative' | 'summative' | 'peer' | 'self' | 'automatic';
  assessorId?: string;                // Who is doing the assessment
  assesseeId?: string;                // Who is being assessed
  
  // Assessment criteria
  criteria: AssessmentCriteria[];
  
  // Assessment results
  overallScore?: number;              // 0-1 overall score
  feedback?: string;                  // Textual feedback
  
  // Improvement suggestions
  strengths?: string[];
  areasForImprovement?: string[];
  nextSteps?: string[];
}

/**
 * Assessment Criteria
 */
export interface AssessmentCriteria {
  criterion: string;
  weight: number;                     // 0-1 weight of this criterion
  score: number;                      // 0-1 score for this criterion
  evidence?: string;                  // Evidence supporting the score
  feedback?: string;                  // Specific feedback for this criterion
}

/**
 * Milestone Progress
 */
export interface MilestoneProgress {
  milestoneId: string;
  milestoneDescription: string;
  
  // Progress metrics
  currentProgress: number;            // 0-1 current progress
  progressDelta: number;              // Change in progress from this event
  
  // Completion status
  isCompleted: boolean;
  completionTimestamp?: number;
  
  // Evidence
  evidenceProvided: string[];         // Evidence of progress/completion
  
  // Next milestones
  enabledMilestones?: string[];       // Milestones this completion enables
}

/**
 * Send Room Event Result - extends CommandResult
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
  
  // Widget coordination results
  widgetCoordinationResults?: WidgetCoordinationResults;
  
  // Academy integration results
  academyIntegrationResults?: AcademyIntegrationResults;
  
  // Event impact
  eventImpact?: EventImpactMetrics;
  
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
    this.widgetCoordinationResults = data.widgetCoordinationResults;
    this.academyIntegrationResults = data.academyIntegrationResults;
    this.eventImpact = data.eventImpact;
    this.error = data.error;
    this.warnings = data.warnings;
  }
}

/**
 * Widget Coordination Results
 */
export interface WidgetCoordinationResults {
  // Widget sync results
  widgetsSynced: string[];            // Widgets that were synchronized
  syncFailures: string[];             // Widgets that failed to sync
  
  // State updates
  sharedStateUpdated: boolean;        // Was shared state updated
  stateUpdateConflicts: boolean;      // Were there state conflicts
  
  // Widget interactions
  triggeredInteractions: WidgetInteractionResult[];
  
  // UI updates
  uiUpdatesTriggered: number;         // Number of UI updates triggered
  uiUpdateLatency: number;            // ms average UI update latency
  
  // Performance
  coordinationLatency: number;        // ms coordination latency
  coordinationEfficiency: number;     // 0-1 coordination efficiency
}

/**
 * Widget Interaction Result
 */
export interface WidgetInteractionResult {
  widgetId: string;
  interactionType: WidgetInteractionType;
  success: boolean;
  latency: number;                    // ms interaction latency
  result?: any;                       // Interaction result data
  error?: string;                     // Error if interaction failed
}

/**
 * Academy Integration Results
 */
export interface AcademyIntegrationResults {
  // Learning tracking
  learningTracked: boolean;           // Was learning value tracked
  trainingProgressUpdated: boolean;   // Was training progress updated
  
  // Performance assessment
  performanceAssessed: boolean;       // Was performance assessed
  assessmentResults?: AssessmentResult[];
  
  // Evolution
  evolutionTriggered: boolean;        // Was evolution triggered
  evolutionDetails?: EvolutionDetails;
  
  // Milestone progress
  milestoneProgressUpdated: boolean;  // Was milestone progress updated
  milestonesCompleted: string[];      // Milestones completed by this event
  
  // Capability tracking
  capabilitiesTracked: string[];      // Capabilities tracked from this event
  capabilityLevelUpdates: CapabilityLevelUpdate[];
  
  // Academy coordination
  trainerNotified: boolean;           // Was trainer notified
  sessionUpdated: boolean;            // Was session state updated
}

/**
 * Assessment Result
 */
export interface AssessmentResult {
  assessmentId: string;
  assessorId: string;
  assesseeId: string;
  overallScore: number;               // 0-1 overall assessment score
  criteriaScores: Map<string, number>; // Scores per criteria
  feedback: string;
  timestamp: number;
}

/**
 * Evolution Details
 */
export interface EvolutionDetails {
  evolutionId: string;
  personaId: string;
  evolutionType: 'capability_growth' | 'learning_advancement' | 'relationship_development' | 'role_progression';
  evolutionTrigger: string;           // What triggered the evolution
  
  // Evolution changes
  capabilitiesGained: string[];
  capabilityLevelChanges: Map<string, number>; // capability -> level change
  newSkills: string[];
  
  // Evolution metadata
  evolutionSignificance: number;      // 0-1 significance of evolution
  evolutionReadiness: number;         // 0-1 readiness before evolution
  evolutionSuccess: boolean;          // Was evolution successful
  
  // Timeline
  evolutionStartTime: number;
  evolutionCompletionTime?: number;
  expectedDuration?: number;          // ms expected evolution duration
}

/**
 * Capability Level Update
 */
export interface CapabilityLevelUpdate {
  capability: string;
  previousLevel: number;              // 0-1 previous level
  newLevel: number;                   // 0-1 new level
  levelDelta: number;                 // Change in level
  evidence: string[];                 // Evidence supporting the update
  timestamp: number;
}

/**
 * Event Impact Metrics
 */
export interface EventImpactMetrics {
  // Reach
  participantsReached: number;        // Number of participants who received event
  participantsEngaged: number;        // Number who engaged with event
  
  // Response metrics
  responsesGenerated: number;         // Number of responses triggered
  averageResponseTime: number;        // ms average response time
  
  // Coordination impact
  widgetsCoordinated: number;         // Number of widgets coordinated
  personasActivated: number;          // Number of personas activated
  
  // Learning impact
  learningInteractionsTriggered: number; // Learning interactions triggered
  teachingMomentsCreated: number;     // Teaching moments created
  
  // Academy impact
  academySessionsAffected: number;    // Academy sessions affected
  trainingProgressImpacted: number;   // Training progress updates
  
  // Overall impact
  overallImpactScore: number;         // 0-1 overall impact score
  significanceLevel: 'minor' | 'moderate' | 'major' | 'transformational';
  
  // Ripple effects
  secondaryEventsTriggered: number;   // Events triggered by this event
  cascadeDepth: number;               // Depth of event cascade
}

/**
 * Event Validation Rules
 */
export interface EventValidationRules {
  // Basic validation
  requireValidParticipant: boolean;
  requireValidRoom: boolean;
  requireValidEventType: boolean;
  
  // Content validation
  maxEventDataSize: number;           // bytes
  allowedEventTypes: RoomEventType[];
  forbiddenEventTypes: RoomEventType[];
  
  // Rate limiting
  maxEventsPerSecond: number;
  maxEventsPerMinute: number;
  participantSpecificLimits: Map<string, RateLimit>;
  
  // Priority validation
  allowHighPriorityEvents: boolean;
  requireJustificationForUrgent: boolean;
  
  // Target validation
  validateTargetFilters: boolean;
  maxTargetParticipants: number;
  
  // Widget validation
  validateWidgetPermissions: boolean;
  allowWidgetCoordination: boolean;
  
  // Academy validation
  requireAcademySession: boolean;     // For Academy events
  validateAcademyPermissions: boolean;
}

/**
 * Rate Limit
 */
export interface RateLimit {
  eventsPerSecond: number;
  eventsPerMinute: number;
  eventsPerHour: number;
  burstLimit: number;                 // Max events in burst
  cooldownPeriod: number;             // ms cooldown after burst
}

/**
 * Event Correlation Tracking
 */
export interface EventCorrelationTracking {
  // Correlation chain
  rootEventId?: string;               // Original event that started chain
  parentEventId?: string;             // Direct parent event
  childEventIds: string[];            // Direct child events
  
  // Correlation context
  correlationReason: string;          // Why events are correlated
  correlationStrength: number;        // 0-1 correlation strength
  
  // Chain metrics
  chainDepth: number;                 // Depth in correlation chain
  chainBranchFactor: number;          // Average branches per event
  
  // Timing
  correlationTimespan: number;        // ms from root to this event
  expectedChainDuration?: number;     // ms expected total chain duration
  
  // Completion tracking
  chainComplete: boolean;             // Is correlation chain complete
  pendingEvents: string[];            // Events still expected in chain
}