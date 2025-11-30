/**
 * BaseUser - Abstract base class for all user types
 *
 * Follows ARCHITECTURE-RULES.md:
 * - Proper abstraction with shared/browser/server pattern
 * - Generic programming with type constraints
 * - Clean inheritance hierarchy
 *
 * Hierarchy:
 * BaseUser (abstract)
 * ├── HumanUser extends BaseUser
 * └── AIUser extends BaseUser (abstract)
 *     ├── AgentUser extends AIUser
 *     └── PersonaUser extends AIUser
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { UserEntity } from '../../data/entities/UserEntity';
import { UserStateEntity } from '../../data/entities/UserStateEntity';
import type { IUserStateStorage } from '../storage/IUserStateStorage';
import type { JTAGContext } from '../../core/types/JTAGTypes';
import type { JTAGRouter } from '../../core/router/shared/JTAGRouter';
import type { UserCreateParams } from '../../../commands/user/create/shared/UserCreateTypes';
import type { UserCapabilities } from '../../data/entities/UserEntity';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../data/config/DatabaseConfig';
import type { RoomEntity } from '../../data/entities/RoomEntity';
import type { ChatMessageEntity } from '../../data/entities/ChatMessageEntity';
import { ROOM_UNIQUE_IDS } from '../../data/constants/RoomConstants';
import { DataEventNames } from '../../events/shared/EventSystemConstants';
import { Events } from '../../core/shared/Events';
import type { JTAGClient } from '../../core/client/shared/JTAGClient';

/**
 * BaseUser abstract class
 * Every connected client has a User object with entity and state
 */
export abstract class BaseUser {
  constructor(
    public readonly entity: UserEntity,
    public readonly state: UserStateEntity,
    protected readonly storage: IUserStateStorage,
    public readonly client?: JTAGClient
  ) {}

  public myRoomIds: Set<UUID> = new Set();

  /**
   * Initialize user - common setup, subclasses extend with type-specific logic
   * Base implementation loads user state, subclasses call super.initialize() then add their own
   */
  async initialize(): Promise<void> {
    // Load state from storage
    await this.loadState();

    // Load rooms this user is a member of
    await this.loadMyRooms();

    // Note: Auto-join to rooms is now handled by RoomMembershipDaemon via events
    // User creation → Event emitted → Daemon subscribes → Daemon adds user to rooms

    console.log(`✅ BaseUser ${this.displayName}: Base initialization complete`);
  }

  /**
   * Load rooms where this user is a member
   * All user types need to know which rooms they're in
   */
  protected async loadMyRooms(): Promise<void> {
    try {
      console.log(`🔧 LOAD-ROOMS-START: ${this.constructor.name} ${this.displayName} (id=${this.id.slice(0,8)}), current myRoomIds.size=${this.myRoomIds.size}`);

      // Query all rooms
      const roomsResult = await DataDaemon.query<RoomEntity>({
        collection: COLLECTIONS.ROOMS,
        filter: {}
      });

      if (!roomsResult.success || !roomsResult.data) {
        console.warn(`⚠️ ${this.constructor.name} ${this.displayName}: Failed to load rooms`);
        return;
      }

      console.log(`🔧 LOAD-ROOMS-QUERY: ${this.constructor.name} ${this.displayName} found ${roomsResult.data.length} total rooms`);

      // Filter rooms where this user is a member
      let memberCount = 0;
      for (const roomRecord of roomsResult.data) {
        const room = roomRecord.data;
        console.log(`🔧 ROOM-RECORD STRUCTURE: roomRecord.id=${roomRecord.id}, roomRecord.data.id=${room.id}, room.name=${room.name}`);

        // Use roomRecord.id (the record ID) not room.id (might be undefined in data payload)
        const roomId = roomRecord.id || room.id;
        console.log(`🔧 ROOM-RECORD: roomId=${roomId}, room.name=${room.name}, hasMembers=${!!room.members}`);

        const isMember = room.members.some((m: { userId: UUID }) => m.userId === this.id);
        if (isMember) {
          this.myRoomIds.add(roomId);
          memberCount++;
          console.log(`🚪 ${this.constructor.name} ${this.displayName}: Member of room "${room.name}"`);
        }
      }

      console.log(`🔧 LOAD-ROOMS-END: ${this.constructor.name} ${this.displayName}, found ${memberCount} memberships, myRoomIds.size=${this.myRoomIds.size}`);
    } catch (error) {
      console.error(`❌ ${this.constructor.name} ${this.displayName}: Error loading rooms:`, error);
    }
  }

  /**
   * Subscribe to chat events for a specific room
   * Helper method for subclasses to subscribe to room-specific chat events
   */
  protected subscribeToRoomChat(roomId: UUID, handler: (message: ChatMessageEntity) => Promise<void>): void {
    // ✅ Type-safe event name using DataEventNames utility
    const eventName = DataEventNames.created(COLLECTIONS.CHAT_MESSAGES);

    if (this.client) {
      // ✅ Use Events.subscribe (works anywhere via sharedInstance)
      Events.subscribe(eventName, async (messageData: ChatMessageEntity) => {
        // Only handle messages for this room
        if (messageData.roomId === roomId) {
          await handler(messageData);
        }
      });
      console.log(`📢 ${this.constructor.name} ${this.displayName}: Subscribed to room chat via Events.subscribe`);
    } else {
      // ❌ FALLBACK: Create isolated EventManager (won't receive events, but won't crash)
      const { EventManager } = require('../../events/shared/JTAGEventSystem');
      const eventManager = new EventManager();
      eventManager.events.on(eventName, async (messageData: ChatMessageEntity) => {
        if (messageData.roomId === roomId) {
          await handler(messageData);
        }
      });
      console.warn(`⚠️ ${this.constructor.name} ${this.displayName}: No client available, using isolated EventManager (events won't work)`);
    }
  }

  /**
   * Subscribe to all chat events (for users who want to handle all their rooms)
   * Subclasses can use this to listen to messages in any room they're a member of
   */
  protected subscribeToChatEvents(handler: (message: ChatMessageEntity) => Promise<void>): void {
    // ✅ Type-safe event name using DataEventNames utility
    const eventName = DataEventNames.created(COLLECTIONS.CHAT_MESSAGES);

    // Subscribe to each room separately with filter - only receive messages for rooms we're in
    console.log(`📢 ${this.constructor.name} ${this.displayName}: Subscribing to chat events for ${this.myRoomIds.size} room(s)`);

    for (const roomId of this.myRoomIds) {
      // ✅ Use Events.subscribe() with filter parameter - event system handles filtering
      // Pass userId+roomId as subscriberId to enable per-room deduplication
      // This ensures: 1 subscription per user per room, replacing old subscriptions on re-init
      Events.subscribe(eventName, async (messageData: ChatMessageEntity) => {
        console.log(`🔔 ${this.constructor.name} ${this.displayName}: EVENT HANDLER TRIGGERED for message ${messageData.id} in room ${roomId.slice(0,8)}`);
        console.log(`   Message: "${messageData.content?.text?.slice(0, 100)}"`);
        console.log(`   From: ${messageData.senderName} (${messageData.senderId.slice(0,8)})`);
        await handler(messageData);
      }, { where: { roomId } }, `${this.id}_${roomId}`);

      console.log(`✅ ${this.constructor.name} ${this.displayName}: Subscribed to room ${roomId.slice(0,8)}`);
    }
  }

  /**
   * Subscribe to room update events to handle dynamic membership changes
   * All user types need to track when they're added/removed from rooms
   */
  protected subscribeToRoomUpdates(handler: (room: RoomEntity) => Promise<void>): void {
    const eventName = DataEventNames.updated(COLLECTIONS.ROOMS);

    if (!this.client) {
      // ❌ FAIL FAST: No client = broken user, crash instead of creating zombie
      throw new Error(`${this.constructor.name} ${this.displayName}: Cannot subscribe to room updates - no client available. User is broken and must be discarded.`);
    }

    // ✅ Use Events.subscribe (works anywhere via sharedInstance)
    // Pass this.id as subscriberId to enable deduplication (prevents duplicate subscriptions)
    Events.subscribe(eventName, async (roomData: RoomEntity) => {
      await handler(roomData);
    }, undefined, this.id);
    console.log(`📢 ${this.constructor.name} ${this.displayName}: Subscribed to room updates via Events.subscribe`);
  }

  /**
   * Get user ID
   */
  get id(): UUID {
    return this.entity.id;
  }

  /**
   * Get user type
   */
  get type(): string {
    return this.entity.type;
  }

  /**
   * Get display name
   */
  get displayName(): string {
    return this.entity.displayName;
  }

  /**
   * Get home directory for this user - ABSOLUTE PATH from SystemPaths
   * This is the user's $HOME - SINGLE SOURCE OF TRUTH for all user data paths.
   *
   * IMPORTANT: Returns ABSOLUTE paths via SystemPaths.personas.dir() / SystemPaths.users.dir()
   * NOT relative paths - SystemPaths handles .continuum/ root resolution.
   *
   * Examples:
   * - PersonaUser: SystemPaths.personas.dir(uniqueId) → '.continuum/personas/claude-assistant-79a5e548'
   * - HumanUser: SystemPaths.users.dir(uniqueId) → '.continuum/users/joel-a1b2c3d4'
   * - AgentUser: SystemPaths.agents.dir(uniqueId) → '.continuum/agents/claude-code-e5f6g7h8'
   *
   * ALL user-specific paths (logs, memory, sessions, databases) MUST be built from this.
   * DO NOT construct paths anywhere else - delegate to SystemPaths for all path construction.
   */
  abstract get homeDirectory(): string;

  /**
   * Save state to storage backend
   */
  async saveState(): Promise<{ success: boolean; error?: string }> {
    return await this.storage.save(this.state);
  }

  /**
   * Load state from storage backend
   */
  async loadState(): Promise<void> {
    const loaded = await this.storage.load(this.entity.id, this.state.deviceId);
    if (loaded) {
      // Update current state with loaded values
      Object.assign(this.state, loaded);
    }
  }

  /**
   * Delete state from storage backend
   */
  async deleteState(): Promise<{ success: boolean; error?: string }> {
    return await this.storage.delete(this.entity.id, this.state.deviceId);
  }

  /**
   * Get user info for debugging
   */
  toString(): string {
    return `${this.constructor.name}(${this.displayName}, ${this.id.substring(0, 8)}...)`;
  }


  /**
   * Get default state for new user
   */
  protected static getDefaultState(userId: UUID): UserStateEntity {
    const state = new UserStateEntity();
    state.id = userId;
    state.userId = userId;
    state.deviceId = 'server-device';
    return state;
  }

  /**
   * Auto-join "general" room - all users start here
   * Uses stable ROOM_UNIQUE_IDS constant
   */
  protected static async addToGeneralRoom(userId: UUID, displayName: string): Promise<void> {
    await this.addToRoomByUniqueId(userId, ROOM_UNIQUE_IDS.GENERAL, displayName);
  }

  /**
   * Add user to room by uniqueId (stable identifier that won't break)
   * Subclasses use this to join additional rooms
   */
  protected static async addToRoomByUniqueId(userId: UUID, roomUniqueId: string, displayName: string): Promise<void> {
    // Query room by uniqueId (stable identifier)
    const roomsResult = await DataDaemon.query<RoomEntity>({
      collection: COLLECTIONS.ROOMS,
      filter: { uniqueId: roomUniqueId }
    });

    console.log(`🔍 ${this.name}: Query result for uniqueId="${roomUniqueId}":`, JSON.stringify(roomsResult, null, 2).slice(0, 500));

    if (!roomsResult.success || !roomsResult.data || roomsResult.data.length === 0) {
      console.warn(`⚠️ ${this.name}: Room with uniqueId "${roomUniqueId}" not found`);
      return;
    }

    // DataDaemon.query returns records, access .data property for entity
    const roomRecord = roomsResult.data[0];
    const room = roomRecord.data || roomRecord;
    console.log(`🔍 ${this.name}: First room:`, JSON.stringify(room, null, 2).slice(0, 400));
    console.log(`🔍 ${this.name}: Room id=${room?.id}, uniqueId=${room?.uniqueId}, name=${room?.name}`);

    if (!room || !room.id) {
      console.warn(`⚠️ ${this.name}: Room data missing id field`);
      return;
    }

    await this.addToRoom(userId, room.id, displayName);
  }

  /**
   * Helper: Add user to room by adding to members array
   * Shared by all user types for consistent room membership management
   */
  protected static async addToRoom(
    userId: UUID,
    roomId: UUID,
    displayName: string
  ): Promise<void> {
    // Read current room
    const roomResult = await DataDaemon.read<RoomEntity>(COLLECTIONS.ROOMS, roomId);
    if (!roomResult.success || !roomResult.data) {
      console.warn(`⚠️ ${this.name}.create: Room ${roomId} not found`);
      return;
    }

    const room = roomResult.data.data;

    // Check if already a member
    if (room.members.some((m: { userId: UUID }) => m.userId === userId)) {
      console.log(`ℹ️ ${this.name}.create: ${displayName} already member of room ${room.name}`);
      return;
    }

    // Add to members array
    const updatedMembers = [
      ...room.members,
      {
        userId,
        role: 'member' as const,
        joinedAt: new Date()
      }
    ];

    // Update room
    await DataDaemon.update<RoomEntity>(
      COLLECTIONS.ROOMS,
      roomId,
      { members: updatedMembers }
    );

    console.log(`✅ ${this.name}.create: Added ${displayName} to room ${room.name}`);
  }
}