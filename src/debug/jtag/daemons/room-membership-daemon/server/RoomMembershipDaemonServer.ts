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
  private unsubscribeFunctions: (() => void)[] = [];

  /**
   * Smart routing rules - extensible for future persona management
   * TODO: Move to database, make configurable by "org chart manager" persona
   */
  private readonly ROUTING_RULES: SmartRoutingRule[] = [
    // Everyone joins general (Discord-style)
    {
      rooms: [ROOM_UNIQUE_IDS.GENERAL]
    },
    // SOTA PersonaUsers join Pantheon (elite multi-provider collaboration)
    // These are cloud-provider PersonaUsers with top-tier models
    {
      userType: 'persona',
      capabilities: ['sota'],
      rooms: [ROOM_UNIQUE_IDS.PANTHEON]
    },
    // Future examples (commented out for now):
    // {
    //   userType: 'persona',
    //   capabilities: ['code-generation'],
    //   rooms: [ROOM_UNIQUE_IDS.CODE_ROOM]
    // },
    // {
    //   userType: 'persona',
    //   capabilities: ['research'],
    //   rooms: [ROOM_UNIQUE_IDS.RESEARCH_ROOM]
    // }
  ];

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  async initialize(): Promise<void> {
    console.log('🏠 RoomMembershipDaemonServer: INITIALIZE CALLED - Starting setup');

    // Subscribe to future user creation events immediately
    await this.setupEventSubscriptions();

    // Defer catch-up logic until after DataDaemon is ready (runs after all daemons initialize)
    setTimeout(() => {
      this.ensureAllUsersInRooms().catch(error => {
        console.error('❌ RoomMembershipDaemon: Deferred catch-up failed:', error);
      });
    }, 2000); // 2 second delay to ensure DataDaemon is initialized

    console.log('🏠 RoomMembershipDaemonServer: Initialized with smart routing (catch-up deferred)');
  }

  /**
   * Ensure all existing users are in correct rooms (catch-up logic)
   * This runs on daemon initialization to sync existing state
   */
  private async ensureAllUsersInRooms(): Promise<void> {
    console.log('🔄 RoomMembershipDaemon: Ensuring all existing users are in correct rooms...');
    try {
      // Query all users
      const queryResult = await DataDaemon.query<UserEntity>({
        collection: COLLECTIONS.USERS,
        filter: {}
      });

      if (!queryResult.success || !queryResult.data?.length) {
        console.log('⚠️ RoomMembershipDaemon: No existing users found, skipping catch-up');
        return;
      }

      // Extract user entities from query results
      // record.data IS the UserEntity, no need to spread/reconstruct
      const users: UserEntity[] = queryResult.data.map(record => record.data);
      console.log(`🔄 RoomMembershipDaemon: Found ${users.length} existing users to process`);

      // Process each user (same logic as handleUserCreated)
      for (const user of users) {
        await this.handleUserCreated(user);
      }

      console.log(`✅ RoomMembershipDaemon: Finished ensuring all ${users.length} users are in correct rooms`);
    } catch (error) {
      console.error('❌ RoomMembershipDaemon: Failed to ensure existing users in rooms:', error);
    }
  }

  /**
   * Subscribe to user creation events
   */
  private async setupEventSubscriptions(): Promise<void> {
    console.log(`🏠 RoomMembershipDaemonServer: Subscribing to ${DATA_EVENTS.USERS.CREATED}`);
    const unsubCreated = Events.subscribe<UserEntity>(
      DATA_EVENTS.USERS.CREATED,
      async (userData: UserEntity) => {
        console.log(`🔔 RoomMembershipDaemonServer: EVENT HANDLER CALLED for ${userData.displayName}`);
        await this.handleUserCreated(userData);
      }
    );
    this.unsubscribeFunctions.push(unsubCreated);
    console.log(`🏠 RoomMembershipDaemonServer: Subscription complete, unsubscribe function stored`);
  }

  /**
   * Handle user created - apply smart routing rules
   */
  private async handleUserCreated(userEntity: UserEntity): Promise<void> {
    console.log(`🔔 RoomMembershipDaemon: handleUserCreated CALLED for ${userEntity.displayName} (id=${userEntity.id})`);
    try {
      const roomsToJoin = this.determineRoomsForUser(userEntity);
      console.log(`🔔 RoomMembershipDaemon: Determined ${roomsToJoin.length} rooms for ${userEntity.displayName}: ${roomsToJoin.join(', ')}`);

      if (roomsToJoin.length === 0) {
        console.log(`🏠 RoomMembershipDaemon: No rooms matched for ${userEntity.displayName}`);
        return;
      }

      console.log(`🏠 RoomMembershipDaemon: Auto-joining ${userEntity.displayName} to ${roomsToJoin.length} room(s)`);
      await this.addUserToRooms(userEntity.id, userEntity.displayName, roomsToJoin);
    } catch (error) {
      console.error(`❌ RoomMembershipDaemon: Failed to process ${userEntity.displayName}:`, error);
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
    for (const roomUniqueId of roomUniqueIds) {
      try {
        // Query for room
        const queryResult = await DataDaemon.query<RoomEntity>({
          collection: COLLECTIONS.ROOMS,
          filter: { uniqueId: roomUniqueId }
        });

        if (!queryResult.success || !queryResult.data?.length) {
          console.warn(`⚠️ RoomMembershipDaemon: Room ${roomUniqueId} not found, skipping for ${displayName}`);
          continue;
        }

        const roomRecord = queryResult.data[0];
        const room = roomRecord.data;

        console.log(`🔍 RoomMembershipDaemon: Found room ${roomUniqueId}:`, {
          recordId: roomRecord.id,
          roomId: room.id,
          roomName: room.displayName,
          currentMembers: room.members?.length ?? 0
        });

        // Check if already a member
        const isMember = room.members?.some(m => m.userId === userId);
        if (isMember) {
          console.log(`✅ RoomMembershipDaemon: ${displayName} already in ${roomUniqueId}`);
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

        console.log(`🔄 RoomMembershipDaemon: Updating room ${roomUniqueId} (recordId: ${roomRecord.id}) to add ${displayName}`);

        // Update room (use roomRecord.id not room.id!)
        await DataDaemon.update<RoomEntity>(
          COLLECTIONS.ROOMS,
          roomRecord.id,  // Record ID, not entity ID
          { members: updatedMembers }
        );

        console.log(`✅ RoomMembershipDaemon: Auto-joined ${displayName} to ${roomUniqueId}`);
      } catch (error) {
        console.error(`❌ RoomMembershipDaemon: Failed to join ${displayName} to ${roomUniqueId}:`, error);
      }
    }
  }

  async shutdown(): Promise<void> {
    // Unsubscribe from events
    this.unsubscribeFunctions.forEach(unsub => unsub());
    this.unsubscribeFunctions = [];
    await super.shutdown();
  }
}
