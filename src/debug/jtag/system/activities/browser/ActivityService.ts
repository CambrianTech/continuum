/**
 * ActivityService - Browser-side service for managing activities
 *
 * Singleton service that provides client-side access to activities.
 * Uses Commands to communicate with server for persistence.
 * Caches activities locally for fast access.
 */

import { Commands } from '../../core/shared/Commands';
import { Events } from '../../core/shared/Events';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { ActivityEntity } from '../../data/entities/ActivityEntity';
import type {
  IActivityService,
  CreateActivityParams,
  ActivityFilters,
  UpdateActivityParams,
  ParticipantActionParams,
  ACTIVITY_EVENTS
} from '../shared/ActivityTypes';
import { generateActivityUniqueId } from '../shared/ActivityTypes';

/**
 * Browser-side Activity Service
 *
 * Provides:
 * - Local cache of activities
 * - Command-based server communication
 * - Event subscription for real-time updates
 */
export class ActivityService implements IActivityService {
  private static instance: ActivityService;
  private cache: Map<UUID, ActivityEntity> = new Map();
  private uniqueIdIndex: Map<string, UUID> = new Map();
  private initialized = false;

  private constructor() {
    this.setupEventListeners();
  }

  static getInstance(): ActivityService {
    if (!ActivityService.instance) {
      ActivityService.instance = new ActivityService();
    }
    return ActivityService.instance;
  }

  /**
   * Setup event listeners for real-time updates
   */
  private setupEventListeners(): void {
    // Listen for activity updates from server
    Events.subscribe('data:activities:created', (event: { data: ActivityEntity }) => {
      this.updateCache(event.data);
    });

    Events.subscribe('data:activities:updated', (event: { data: ActivityEntity }) => {
      this.updateCache(event.data);
    });

    Events.subscribe('data:activities:deleted', (event: { data: { id: UUID } }) => {
      this.removeFromCache(event.data.id);
    });
  }

  /**
   * Update local cache with activity
   */
  private updateCache(activity: ActivityEntity): void {
    this.cache.set(activity.id, activity);
    if (activity.uniqueId) {
      this.uniqueIdIndex.set(activity.uniqueId, activity.id);
    }
  }

  /**
   * Remove activity from cache
   */
  private removeFromCache(activityId: UUID): void {
    const activity = this.cache.get(activityId);
    if (activity?.uniqueId) {
      this.uniqueIdIndex.delete(activity.uniqueId);
    }
    this.cache.delete(activityId);
  }

  // ============ IActivityService Implementation ============

  async createActivity(params: CreateActivityParams): Promise<ActivityEntity> {
    const uniqueId = params.uniqueId || generateActivityUniqueId(params.recipeId, params.ownerId);

    const result = await Commands.execute('data/create', {
      collection: 'activities',
      data: {
        uniqueId,
        displayName: params.displayName,
        description: params.description,
        recipeId: params.recipeId,
        ownerId: params.ownerId,
        status: 'active',
        participants: params.initialParticipants?.map(p => ({
          userId: p.userId,
          role: p.role,
          joinedAt: new Date(),
          isActive: true
        })) || [],
        state: {
          phase: 'initial',
          progress: 0,
          variables: {},
          updatedAt: new Date()
        },
        config: {
          settings: {},
          ...params.initialConfig
        },
        startedAt: new Date(),
        lastActivityAt: new Date(),
        tags: params.tags || []
      }
    });

    if (!result.success || !result.data) {
      throw new Error(result.error || 'Failed to create activity');
    }

    const activity = result.data as unknown as ActivityEntity;
    this.updateCache(activity);
    return activity;
  }

  async getActivity(activityId: UUID): Promise<ActivityEntity | null> {
    // Check cache first
    const cached = this.cache.get(activityId);
    if (cached) {
      return cached;
    }

    // Fetch from server
    const result = await Commands.execute('data/read', {
      collection: 'activities',
      id: activityId
    });

    if (!result.success || !result.data) {
      return null;
    }

    const activity = result.data as unknown as ActivityEntity;
    this.updateCache(activity);
    return activity;
  }

  async getActivityByUniqueId(uniqueId: string): Promise<ActivityEntity | null> {
    // Check index first
    const cachedId = this.uniqueIdIndex.get(uniqueId);
    if (cachedId) {
      return this.cache.get(cachedId) || null;
    }

    // Fetch from server
    const result = await Commands.execute('data/list', {
      collection: 'activities',
      filter: { uniqueId },
      limit: 1
    });

    if (!result.success || !result.items?.length) {
      return null;
    }

    const activity = result.items[0] as unknown as ActivityEntity;
    this.updateCache(activity);
    return activity;
  }

  async listActivities(filters?: ActivityFilters, limit = 50): Promise<ActivityEntity[]> {
    const filter: Record<string, unknown> = {};

    if (filters?.recipeId) {
      filter.recipeId = filters.recipeId;
    }
    if (filters?.status) {
      filter.status = Array.isArray(filters.status)
        ? { $in: filters.status }
        : filters.status;
    }
    if (filters?.ownerId) {
      filter.ownerId = filters.ownerId;
    }
    if (filters?.tags?.length) {
      // Activities with any of the specified tags
      filter.tags = { $contains: filters.tags };
    }

    const result = await Commands.execute('data/list', {
      collection: 'activities',
      filter: Object.keys(filter).length > 0 ? filter : undefined,
      limit,
      orderBy: [{ field: 'lastActivityAt', direction: 'desc' }]
    });

    if (!result.success || !result.items) {
      return [];
    }

    const activities = result.items as unknown as ActivityEntity[];
    activities.forEach(a => this.updateCache(a));
    return activities;
  }

  async updateActivity(params: UpdateActivityParams): Promise<ActivityEntity> {
    const activity = await this.getActivity(params.activityId);
    if (!activity) {
      throw new Error(`Activity not found: ${params.activityId}`);
    }

    const updates: Partial<ActivityEntity> = {};

    if (params.displayName !== undefined) updates.displayName = params.displayName;
    if (params.description !== undefined) updates.description = params.description;
    if (params.status !== undefined) updates.status = params.status;
    if (params.tags !== undefined) updates.tags = params.tags;

    // Handle state updates
    if (params.phase !== undefined || params.progress !== undefined || params.variables) {
      updates.state = {
        ...activity.state,
        ...(params.phase !== undefined && { phase: params.phase }),
        ...(params.progress !== undefined && { progress: params.progress }),
        ...(params.variables && {
          variables: { ...activity.state.variables, ...params.variables }
        }),
        updatedAt: new Date()
      };
    }

    // Handle settings updates
    if (params.settings) {
      updates.config = {
        ...activity.config,
        settings: { ...activity.config.settings, ...params.settings }
      };
    }

    updates.lastActivityAt = new Date();

    const result = await Commands.execute('data/update', {
      collection: 'activities',
      id: params.activityId,
      data: updates
    });

    if (!result.success || !result.data) {
      throw new Error(result.error || 'Failed to update activity');
    }

    const updated = result.data as unknown as ActivityEntity;
    this.updateCache(updated);
    return updated;
  }

  async addParticipant(params: ParticipantActionParams): Promise<void> {
    const activity = await this.getActivity(params.activityId);
    if (!activity) {
      throw new Error(`Activity not found: ${params.activityId}`);
    }

    // Check if already participating
    const existing = activity.participants.find(
      p => p.userId === params.userId && p.isActive
    );
    if (existing) {
      return; // Already a participant
    }

    const newParticipant = {
      userId: params.userId,
      role: params.role || 'participant',
      joinedAt: new Date(),
      isActive: true,
      roleConfig: params.roleConfig
    };

    await Commands.execute('data/update', {
      collection: 'activities',
      id: params.activityId,
      data: {
        participants: [...activity.participants, newParticipant],
        lastActivityAt: new Date()
      }
    });

    // Emit event
    Events.emit('activity:participant:joined', {
      activityId: params.activityId,
      participant: newParticipant
    });
  }

  async removeParticipant(params: Omit<ParticipantActionParams, 'role' | 'roleConfig'>): Promise<void> {
    const activity = await this.getActivity(params.activityId);
    if (!activity) {
      throw new Error(`Activity not found: ${params.activityId}`);
    }

    const updatedParticipants = activity.participants.map(p => {
      if (p.userId === params.userId && p.isActive) {
        return { ...p, isActive: false, leftAt: new Date() };
      }
      return p;
    });

    await Commands.execute('data/update', {
      collection: 'activities',
      id: params.activityId,
      data: {
        participants: updatedParticipants,
        lastActivityAt: new Date()
      }
    });

    // Emit event
    Events.emit('activity:participant:left', {
      activityId: params.activityId,
      userId: params.userId
    });
  }

  async setPhase(activityId: UUID, phase: string, progress?: number): Promise<void> {
    const activity = await this.getActivity(activityId);
    if (!activity) {
      throw new Error(`Activity not found: ${activityId}`);
    }

    const previousPhase = activity.state.phase;

    await Commands.execute('data/update', {
      collection: 'activities',
      id: activityId,
      data: {
        state: {
          ...activity.state,
          phase,
          progress: progress ?? activity.state.progress,
          updatedAt: new Date()
        },
        lastActivityAt: new Date()
      }
    });

    // Emit event
    Events.emit('activity:phase:changed', {
      activityId,
      previousPhase,
      newPhase: phase,
      progress
    });
  }

  async setVariable(activityId: UUID, key: string, value: unknown): Promise<void> {
    const activity = await this.getActivity(activityId);
    if (!activity) {
      throw new Error(`Activity not found: ${activityId}`);
    }

    await Commands.execute('data/update', {
      collection: 'activities',
      id: activityId,
      data: {
        state: {
          ...activity.state,
          variables: {
            ...activity.state.variables,
            [key]: value
          },
          updatedAt: new Date()
        },
        lastActivityAt: new Date()
      }
    });
  }

  async completeActivity(activityId: UUID): Promise<void> {
    const activity = await this.updateActivity({
      activityId,
      status: 'completed'
    });

    // Set end time
    await Commands.execute('data/update', {
      collection: 'activities',
      id: activityId,
      data: { endedAt: new Date() }
    });

    // Emit event
    Events.emit('activity:completed', { activity });
  }

  async archiveActivity(activityId: UUID): Promise<void> {
    await this.updateActivity({
      activityId,
      status: 'archived'
    });
  }

  async getActivitiesForUser(userId: UUID, activeOnly = true): Promise<ActivityEntity[]> {
    // Get activities where user is owner
    const ownedResult = await Commands.execute('data/list', {
      collection: 'activities',
      filter: {
        ownerId: userId,
        ...(activeOnly && { status: 'active' })
      },
      orderBy: [{ field: 'lastActivityAt', direction: 'desc' }]
    });

    const owned = (ownedResult.items as unknown as ActivityEntity[]) || [];

    // For participant activities, we'd need a more complex query
    // For now, return owned activities
    // TODO: Add participant query when data layer supports JSON array contains

    owned.forEach(a => this.updateCache(a));
    return owned;
  }

  async getCurrentActivityForRecipe(recipeId: string, userId: UUID): Promise<ActivityEntity | null> {
    const result = await Commands.execute('data/list', {
      collection: 'activities',
      filter: {
        recipeId,
        ownerId: userId,
        status: 'active'
      },
      orderBy: [{ field: 'lastActivityAt', direction: 'desc' }],
      limit: 1
    });

    if (!result.success || !result.items?.length) {
      return null;
    }

    const activity = result.items[0] as unknown as ActivityEntity;
    this.updateCache(activity);
    return activity;
  }

  // ============ Utility Methods ============

  /**
   * Get or create an activity for a recipe
   * Used for singleton activities like settings, help, etc.
   */
  async getOrCreateActivity(
    recipeId: string,
    userId: UUID,
    displayName: string
  ): Promise<ActivityEntity> {
    // Try to find existing
    const existing = await this.getCurrentActivityForRecipe(recipeId, userId);
    if (existing) {
      return existing;
    }

    // Create new
    return this.createActivity({
      recipeId,
      displayName,
      ownerId: userId,
      initialParticipants: [{ userId, role: 'owner' }]
    });
  }

  /**
   * Clear the local cache
   */
  clearCache(): void {
    this.cache.clear();
    this.uniqueIdIndex.clear();
  }
}

// Export singleton getter
export const getActivityService = () => ActivityService.getInstance();
