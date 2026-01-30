/**
 * RoomMembershipDaemonServer - Server-side room AND activity membership management
 *
 * Discord-style auto-join + smart routing foundation:
 * - Everyone auto-joins #general room AND collaborative activities
 * - Smart routing based on user type/capabilities (extensible)
 * - Future: Persona-driven org chart management
 *
 * Handles both:
 * - Rooms: Chat channels (RoomEntity)
 * - Activities: Collaborative content (ActivityEntity) - canvas, browser, games
 */

import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { RoomMembershipDaemon } from '../shared/RoomMembershipDaemon';
import { Events } from '../../../system/core/shared/Events';
import { DATA_EVENTS } from '../../../system/core/shared/EventConstants';
import { DataDaemon } from '../../data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../../system/data/config/DatabaseConfig';
import { ROOM_UNIQUE_IDS } from '../../../system/data/constants/RoomConstants';
import { ACTIVITY_UNIQUE_IDS } from '../../../system/data/constants/ActivityConstants';
import type { UserEntity } from '../../../system/data/entities/UserEntity';
import type { RoomEntity } from '../../../system/data/entities/RoomEntity';
import type { ActivityEntity, ActivityParticipant } from '../../../system/data/entities/ActivityEntity';
import { Logger, type ComponentLogger } from '../../../system/core/logging/Logger';

/**
 * Smart routing rules (foundation for future persona-based management)
 * Maps user types to rooms they should auto-join
 */
interface SmartRoutingRule {
  userType?: 'human' | 'persona' | 'agent';
  capabilities?: string[];  // Future: Check user.capabilities
  rooms: string[];  // Room uniqueIds
}

/**
 * Activity routing rules - similar to rooms but for collaborative content
 * Maps user types to activities they should auto-join as participants
 */
interface ActivityRoutingRule {
  userType?: 'human' | 'persona' | 'agent';
  capabilities?: string[];
  activities: string[];  // Activity uniqueIds
  role?: string;  // Default role for auto-joined participants (default: 'participant')
}

export class RoomMembershipDaemonServer extends RoomMembershipDaemon {
  /**
   * Smart routing rules - extensible for future persona management
   * TODO: Move to database, make configurable by "org chart manager" persona
   */
  private readonly ROUTING_RULES: SmartRoutingRule[] = [
    // Everyone joins all public rooms (Discord-style server with multiple channels)
    {
      rooms: [
        ROOM_UNIQUE_IDS.GENERAL,
        ROOM_UNIQUE_IDS.ACADEMY,
        ROOM_UNIQUE_IDS.DEV_UPDATES,
        ROOM_UNIQUE_IDS.HELP,
        ROOM_UNIQUE_IDS.THEME,     // System room for ThemeWidget assistant
        ROOM_UNIQUE_IDS.SETTINGS   // System room for SettingsWidget assistant
      ]
    },
    // SOTA PersonaUsers also join Pantheon (elite multi-provider collaboration)
    // These are cloud-provider PersonaUsers with top-tier models
    {
      userType: 'persona',
      capabilities: ['sota'],
      rooms: [ROOM_UNIQUE_IDS.PANTHEON]
    }
  ];

  /**
   * Activity routing rules - which collaborative activities users auto-join
   * Canvas, browser, and future activities (games, etc.) use this
   */
  private readonly ACTIVITY_ROUTING_RULES: ActivityRoutingRule[] = [
    // Everyone joins the main collaborative canvas and browser
    {
      activities: [
        ACTIVITY_UNIQUE_IDS.CANVAS_MAIN,
        ACTIVITY_UNIQUE_IDS.BROWSER_MAIN
      ],
      role: 'participant'
    }
  ];

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    // Initialize standardized logging (daemons/ subdirectory)
    const className = this.constructor.name;
    this.log = Logger.create(className, `daemons/${className}`);
  }

  async initialize(): Promise<void> {
    this.log.info('🏠 RoomMembershipDaemonServer: INITIALIZE CALLED - Starting setup');

    // Subscribe to future user creation events immediately
    await this.setupEventSubscriptions();

    // Defer catch-up logic until after DataDaemon is ready (uses base class helper)
    this.deferInitialization(async () => {
      await this.ensureAllUsersInRoomsAndActivities();
    }, 2000);

    this.log.info('🏠 RoomMembershipDaemonServer: Initialized with smart routing (catch-up deferred)');
  }

  /**
   * Ensure all existing users are in correct rooms AND activities (catch-up logic)
   * This runs on daemon initialization to sync existing state
   */
  private async ensureAllUsersInRoomsAndActivities(): Promise<void> {
    this.log.info('🔄 MembershipDaemon: Ensuring all existing users are in correct rooms and activities...');
    try {
      // Query all users
      const queryResult = await DataDaemon.query<UserEntity>({
        collection: COLLECTIONS.USERS,
        filter: {}
      });

      if (!queryResult.success || !queryResult.data?.length) {
        this.log.info('⚠️ MembershipDaemon: No existing users found, skipping catch-up');
        return;
      }

      // Extract user entities from query results
      // SqliteQueryExecutor now includes id in record.data (BaseEntity requirement)
      const users: UserEntity[] = queryResult.data.map(record => record.data);
      this.log.info(`🔄 MembershipDaemon: Found ${users.length} existing users to process`);

      // Process each user (same logic as handleUserCreated)
      for (const user of users) {
        await this.handleUserCreated(user);
      }

      this.log.info(`✅ MembershipDaemon: Finished ensuring all ${users.length} users are in correct rooms and activities`);
    } catch (error) {
      this.log.error('❌ MembershipDaemon: Failed to ensure existing users in rooms/activities:', error);
    }
  }

  /**
   * Subscribe to user creation events
   */
  private async setupEventSubscriptions(): Promise<void> {
    this.log.info(`🏠 RoomMembershipDaemonServer: Subscribing to ${DATA_EVENTS.USERS.CREATED}`);
    const unsubCreated = Events.subscribe<UserEntity>(
      DATA_EVENTS.USERS.CREATED,
      async (userData: UserEntity) => {
        this.log.info(`🔔 RoomMembershipDaemonServer: EVENT HANDLER CALLED for ${userData.displayName}`);
        await this.handleUserCreated(userData);
      }
    );
    this.registerSubscription(unsubCreated);
    this.log.info(`🏠 RoomMembershipDaemonServer: Subscription complete, unsubscribe function registered`);
  }

  /**
   * Handle user created - apply smart routing rules for BOTH rooms and activities
   */
  private async handleUserCreated(userEntity: UserEntity): Promise<void> {
    this.log.info(`🔔 MembershipDaemon: handleUserCreated CALLED for ${userEntity.displayName} (id=${userEntity.id})`);

    // CRITICAL: Validate user entity has valid id before processing
    if (!userEntity?.id) {
      this.log.error(`❌ MembershipDaemon: Received user without valid ID: ${JSON.stringify(userEntity)}. Skipping.`);
      return;
    }

    try {
      // 1. Join rooms (chat channels)
      const roomsToJoin = this.determineRoomsForUser(userEntity);
      if (roomsToJoin.length > 0) {
        this.log.info(`🏠 MembershipDaemon: Auto-joining ${userEntity.displayName} to ${roomsToJoin.length} room(s): ${roomsToJoin.join(', ')}`);
        await this.addUserToRooms(userEntity.id, userEntity.displayName, roomsToJoin);
      }

      // 2. Join activities (collaborative content - canvas, browser, etc.)
      const activitiesToJoin = this.determineActivitiesForUser(userEntity);
      if (activitiesToJoin.activities.length > 0) {
        this.log.info(`🎨 MembershipDaemon: Auto-joining ${userEntity.displayName} to ${activitiesToJoin.activities.length} activit(ies): ${activitiesToJoin.activities.join(', ')}`);
        await this.addUserToActivities(userEntity.id, userEntity.displayName, activitiesToJoin.activities, activitiesToJoin.role);
      }
    } catch (error) {
      this.log.error(`❌ MembershipDaemon: Failed to process ${userEntity.displayName}:`, error);
    }
  }

  /**
   * Determine which rooms a user should auto-join (smart routing)
   * Extensible for future persona-based org chart management
   */
  private determineRoomsForUser(user: UserEntity): string[] {
    const rooms = new Set<string>();

    for (const rule of this.ROUTING_RULES) {
      // Check user type match
      if (rule.userType && rule.userType !== user.type) {
        continue;
      }

      // Check modelConfig capabilities match (SOTA, etc.)
      if (rule.capabilities && rule.capabilities.length > 0) {
        const userCapabilities = user.modelConfig?.capabilities || [];
        const hasRequiredCapability = rule.capabilities.some(cap =>
          userCapabilities.includes(cap)
        );
        if (!hasRequiredCapability) {
          continue;
        }
      }

      // Rule matches - add rooms
      rule.rooms.forEach(roomId => rooms.add(roomId));
    }

    return Array.from(rooms);
  }

  /**
   * Add user to specified rooms (Discord-style auto-join)
   */
  private async addUserToRooms(
    userId: UUID,
    displayName: string,
    roomUniqueIds: string[]
  ): Promise<void> {
    // CRITICAL: Validate userId before adding to any room
    if (!userId) {
      this.log.error(`❌ RoomMembershipDaemon: Cannot add user "${displayName}" - userId is ${userId}. Skipping.`);
      return;
    }

    for (const roomUniqueId of roomUniqueIds) {
      try {
        // Query for room
        const queryResult = await DataDaemon.query<RoomEntity>({
          collection: COLLECTIONS.ROOMS,
          filter: { uniqueId: roomUniqueId }
        });

        if (!queryResult.success || !queryResult.data?.length) {
          this.log.warn(`⚠️ RoomMembershipDaemon: Room ${roomUniqueId} not found, skipping for ${displayName}`);
          continue;
        }

        const roomRecord = queryResult.data[0];
        const room = roomRecord.data;

        this.log.info(`🔍 RoomMembershipDaemon: Found room ${roomUniqueId}:`, {
          recordId: roomRecord.id,
          roomId: room.id,
          roomName: room.displayName,
          currentMembers: room.members?.length ?? 0
        });

        // Check if already a member
        const isMember = room.members?.some(m => m.userId === userId);
        if (isMember) {
          this.log.info(`✅ RoomMembershipDaemon: ${displayName} already in ${roomUniqueId}`);
          continue;
        }

        // Add to members
        const updatedMembers = [
          ...(room.members ?? []),
          {
            userId: userId,
            role: 'member' as const,
            joinedAt: new Date()
          }
        ];

        this.log.info(`🔄 RoomMembershipDaemon: Updating room ${roomUniqueId} (recordId: ${roomRecord.id}) to add ${displayName}`);

        // Update room (use roomRecord.id not room.id!)
        await DataDaemon.update<RoomEntity>(
          COLLECTIONS.ROOMS,
          roomRecord.id,  // Record ID, not entity ID
          { members: updatedMembers }
        );

        this.log.info(`✅ RoomMembershipDaemon: Auto-joined ${displayName} to ${roomUniqueId}`);
      } catch (error) {
        this.log.error(`❌ RoomMembershipDaemon: Failed to join ${displayName} to ${roomUniqueId}:`, error);
      }
    }
  }

  // ============ Activity Membership Methods ============

  /**
   * Determine which activities a user should auto-join (smart routing)
   * Returns activities and the default role for the user
   */
  private determineActivitiesForUser(user: UserEntity): { activities: string[]; role: string } {
    const activities = new Set<string>();
    let defaultRole = 'participant';

    for (const rule of this.ACTIVITY_ROUTING_RULES) {
      // Check user type match
      if (rule.userType && rule.userType !== user.type) {
        continue;
      }

      // Check modelConfig capabilities match
      if (rule.capabilities && rule.capabilities.length > 0) {
        const userCapabilities = user.modelConfig?.capabilities || [];
        const hasRequiredCapability = rule.capabilities.some(cap =>
          userCapabilities.includes(cap)
        );
        if (!hasRequiredCapability) {
          continue;
        }
      }

      // Rule matches - add activities
      rule.activities.forEach(activityId => activities.add(activityId));
      if (rule.role) {
        defaultRole = rule.role;
      }
    }

    return { activities: Array.from(activities), role: defaultRole };
  }

  /**
   * Add user to specified activities as participant
   */
  private async addUserToActivities(
    userId: UUID,
    displayName: string,
    activityUniqueIds: string[],
    role: string = 'participant'
  ): Promise<void> {
    // CRITICAL: Validate userId before adding to any activity
    if (!userId) {
      this.log.error(`❌ MembershipDaemon: Cannot add user "${displayName}" to activities - userId is ${userId}. Skipping.`);
      return;
    }

    for (const activityUniqueId of activityUniqueIds) {
      try {
        // Query for activity
        const queryResult = await DataDaemon.query<ActivityEntity>({
          collection: COLLECTIONS.ACTIVITIES,
          filter: { uniqueId: activityUniqueId }
        });

        if (!queryResult.success || !queryResult.data?.length) {
          this.log.warn(`⚠️ MembershipDaemon: Activity ${activityUniqueId} not found, skipping for ${displayName}`);
          continue;
        }

        const activityRecord = queryResult.data[0];
        const activity = activityRecord.data;

        this.log.info(`🔍 MembershipDaemon: Found activity ${activityUniqueId}:`, {
          recordId: activityRecord.id,
          activityId: activity.id,
          displayName: activity.displayName,
          currentParticipants: activity.participants?.length ?? 0
        });

        // Check if already a participant
        const isParticipant = activity.participants?.some(
          (p: ActivityParticipant) => p.userId === userId && p.isActive
        );
        if (isParticipant) {
          this.log.info(`✅ MembershipDaemon: ${displayName} already in activity ${activityUniqueId}`);
          continue;
        }

        // Add as participant
        const newParticipant: ActivityParticipant = {
          userId: userId,
          role: role,
          joinedAt: new Date(),
          isActive: true
        };

        const updatedParticipants = [
          ...(activity.participants ?? []),
          newParticipant
        ];

        this.log.info(`🔄 MembershipDaemon: Updating activity ${activityUniqueId} (recordId: ${activityRecord.id}) to add ${displayName}`);

        // Update activity (use activityRecord.id not activity.id!)
        await DataDaemon.update<ActivityEntity>(
          COLLECTIONS.ACTIVITIES,
          activityRecord.id,  // Record ID, not entity ID
          { participants: updatedParticipants }
        );

        this.log.info(`✅ MembershipDaemon: Auto-joined ${displayName} to activity ${activityUniqueId}`);
      } catch (error) {
        this.log.error(`❌ MembershipDaemon: Failed to join ${displayName} to activity ${activityUniqueId}:`, error);
      }
    }
  }
}
