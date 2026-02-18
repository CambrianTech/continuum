/**
 * Activity Types - Shared type definitions for the Activity system
 *
 * Activities are runtime instances of Recipes:
 * - Recipe = Template (static, defines behavior)
 * - Activity = Instance (dynamic, has evolving state)
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type {
  ActivityEntity,
  ActivityParticipant,
  ActivityState,
  ActivityConfig,
  ActivityStatus
} from '../../data/entities/ActivityEntity';

// Re-export entity types
export type {
  ActivityParticipant,
  ActivityState,
  ActivityConfig,
  ActivityStatus
};

/**
 * Activity creation parameters
 */
export interface CreateActivityParams {
  recipeId: string;
  displayName: string;
  description?: string;
  ownerId: UUID;
  uniqueId?: string;  // Optional - auto-generated if not provided
  initialParticipants?: Array<{ userId: UUID; role: string }>;
  initialConfig?: Partial<ActivityConfig>;
  tags?: string[];
}

/**
 * Activity query filters
 */
export interface ActivityFilters {
  recipeId?: string;
  status?: ActivityStatus | ActivityStatus[];
  ownerId?: UUID;
  participantId?: UUID;  // Find activities where user is a participant
  tags?: string[];
}

/**
 * Activity update parameters
 */
export interface UpdateActivityParams {
  activityId: UUID;
  displayName?: string;
  description?: string;
  status?: ActivityStatus;
  phase?: string;
  progress?: number;
  variables?: Record<string, unknown>;
  settings?: Record<string, unknown>;
  tags?: string[];
}

/**
 * Participant action parameters
 */
export interface ParticipantActionParams {
  activityId: UUID;
  userId: UUID;
  role?: string;
  roleConfig?: Record<string, unknown>;
}

/**
 * Activity events emitted when activities change
 */
export interface ActivityCreatedEvent {
  activity: ActivityEntity;
}

export interface ActivityUpdatedEvent {
  activity: ActivityEntity;
  changedFields: string[];
}

export interface ActivityParticipantJoinedEvent {
  activityId: UUID;
  participant: ActivityParticipant;
}

export interface ActivityParticipantLeftEvent {
  activityId: UUID;
  userId: UUID;
}

export interface ActivityPhaseChangedEvent {
  activityId: UUID;
  previousPhase: string;
  newPhase: string;
  progress?: number;
}

export interface ActivityCompletedEvent {
  activity: ActivityEntity;
}

/**
 * Activity event names - follows data:collection:action pattern
 */
export const ACTIVITY_EVENTS = {
  CREATED: 'data:activities:created',
  UPDATED: 'data:activities:updated',
  DELETED: 'data:activities:deleted',
  PARTICIPANT_JOINED: 'activity:participant:joined',
  PARTICIPANT_LEFT: 'activity:participant:left',
  PHASE_CHANGED: 'activity:phase:changed',
  COMPLETED: 'activity:completed'
} as const;

/**
 * Activity service interface - implemented by browser and server
 */
export interface IActivityService {
  /**
   * Create a new activity from a recipe
   */
  createActivity(params: CreateActivityParams): Promise<ActivityEntity>;

  /**
   * Get an activity by ID
   */
  getActivity(activityId: UUID): Promise<ActivityEntity | null>;

  /**
   * Get an activity by unique ID
   */
  getActivityByUniqueId(uniqueId: string): Promise<ActivityEntity | null>;

  /**
   * List activities with filters
   */
  listActivities(filters?: ActivityFilters, limit?: number): Promise<ActivityEntity[]>;

  /**
   * Update activity properties
   */
  updateActivity(params: UpdateActivityParams): Promise<ActivityEntity>;

  /**
   * Add a participant to an activity
   */
  addParticipant(params: ParticipantActionParams): Promise<void>;

  /**
   * Remove a participant from an activity
   */
  removeParticipant(params: Omit<ParticipantActionParams, 'role' | 'roleConfig'>): Promise<void>;

  /**
   * Change activity phase
   */
  setPhase(activityId: UUID, phase: string, progress?: number): Promise<void>;

  /**
   * Set a state variable
   */
  setVariable(activityId: UUID, key: string, value: unknown): Promise<void>;

  /**
   * Complete an activity
   */
  completeActivity(activityId: UUID): Promise<void>;

  /**
   * Archive an activity
   */
  archiveActivity(activityId: UUID): Promise<void>;

  /**
   * Get activities for a user (as participant or owner)
   */
  getActivitiesForUser(userId: UUID, activeOnly?: boolean): Promise<ActivityEntity[]>;

  /**
   * Get the current activity for a recipe (most recent active)
   */
  getCurrentActivityForRecipe(recipeId: string, userId: UUID): Promise<ActivityEntity | null>;
}

/**
 * Utility to generate unique activity ID from recipe + owner
 */
export function generateActivityUniqueId(recipeId: string, ownerId: UUID): string {
  const shortOwner = ownerId.slice(-8);
  const timestamp = Date.now().toString(36);
  return `${recipeId}-${shortOwner}-${timestamp}`;
}
