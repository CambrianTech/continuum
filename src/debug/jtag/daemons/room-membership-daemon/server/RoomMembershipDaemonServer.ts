/**
 * RoomMembershipDaemonServer - Server-side room membership management
 *
 * Discord-style auto-join + smart routing foundation:
 * - Everyone auto-joins #general
 * - Smart routing based on user type/capabilities (extensible)
 * - Future: Persona-driven org chart management
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
import type { UserEntity } from '../../../system/data/entities/UserEntity';
import type { RoomEntity } from '../../../system/data/entities/RoomEntity';
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
      await this.ensureAllUsersInRooms();
    }, 2000);

    this.log.info('🏠 RoomMembershipDaemonServer: Initialized with smart routing (catch-up deferred)');
  }

  /**
   * Ensure all existing users are in correct rooms (catch-up logic)
   * This runs on daemon initialization to sync existing state
   */
  private async ensureAllUsersInRooms(): Promise<void> {
    this.log.info('🔄 RoomMembershipDaemon: Ensuring all existing users are in correct rooms...');
    try {
      // Query all users
      const queryResult = await DataDaemon.query<UserEntity>({
        collection: COLLECTIONS.USERS,
        filter: {}
      });

      if (!queryResult.success || !queryResult.data?.length) {
        this.log.info('⚠️ RoomMembershipDaemon: No existing users found, skipping catch-up');
        return;
      }

      // Extract user entities from query results
      // record.data IS the UserEntity, no need to spread/reconstruct
      const users: UserEntity[] = queryResult.data.map(record => record.data);
      this.log.info(`🔄 RoomMembershipDaemon: Found ${users.length} existing users to process`);

      // Process each user (same logic as handleUserCreated)
      for (const user of users) {
        await this.handleUserCreated(user);
      }

      this.log.info(`✅ RoomMembershipDaemon: Finished ensuring all ${users.length} users are in correct rooms`);
    } catch (error) {
      this.log.error('❌ RoomMembershipDaemon: Failed to ensure existing users in rooms:', error);
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
   * Handle user created - apply smart routing rules
   */
  private async handleUserCreated(userEntity: UserEntity): Promise<void> {
    this.log.info(`🔔 RoomMembershipDaemon: handleUserCreated CALLED for ${userEntity.displayName} (id=${userEntity.id})`);

    // CRITICAL: Validate user entity has valid id before processing
    if (!userEntity?.id) {
      this.log.error(`❌ RoomMembershipDaemon: Received user without valid ID: ${JSON.stringify(userEntity)}. Skipping.`);
      return;
    }

    try {
      const roomsToJoin = this.determineRoomsForUser(userEntity);
      this.log.info(`🔔 RoomMembershipDaemon: Determined ${roomsToJoin.length} rooms for ${userEntity.displayName}: ${roomsToJoin.join(', ')}`);

      if (roomsToJoin.length === 0) {
        this.log.info(`🏠 RoomMembershipDaemon: No rooms matched for ${userEntity.displayName}`);
        return;
      }

      this.log.info(`🏠 RoomMembershipDaemon: Auto-joining ${userEntity.displayName} to ${roomsToJoin.length} room(s)`);
      await this.addUserToRooms(userEntity.id, userEntity.displayName, roomsToJoin);
    } catch (error) {
      this.log.error(`❌ RoomMembershipDaemon: Failed to process ${userEntity.displayName}:`, error);
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
}
