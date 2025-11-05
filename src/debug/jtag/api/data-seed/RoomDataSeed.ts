/**
 * Room Data Seeding - Centralized Chat Room Creation
 *
 * Creates initial chat rooms using proper RoomEntity structure.
 * Uses JTAG data commands and stable uniqueId constants.
 */

import { RoomEntity } from '../../system/data/entities/RoomEntity';
import { ChatMessageEntity } from '../../system/data/entities/ChatMessageEntity';
import { ROOM_UNIQUE_IDS } from '../../system/data/constants/RoomConstants';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import { COLLECTIONS } from '../../system/data/config/DatabaseConfig';

export interface RoomSeedData {
  readonly rooms: readonly RoomEntity[];
  readonly totalCount: number;
  readonly createdAt: string;
}

export class RoomDataSeed {
  private static readonly COLLECTION = COLLECTIONS.ROOMS;
  private static readonly MESSAGE_COLLECTION = COLLECTIONS.CHAT_MESSAGES;
  
  /**
   * Generate seed rooms using RoomEntity structure with stable uniqueIds
   * @param humanUserId - The userId of the system owner (from SystemIdentity)
   */
  public static generateSeedRooms(humanUserId: UUID): RoomSeedData {
    const now = new Date();
    const rooms: RoomEntity[] = [];

    // General room - all users auto-join here
    const general = new RoomEntity();
    general.uniqueId = ROOM_UNIQUE_IDS.GENERAL;
    general.name = 'general';
    general.displayName = 'General';
    general.description = 'Main discussion room for all users';
    general.topic = 'General chat and collaboration';
    general.type = 'public';
    general.status = 'active';
    general.ownerId = humanUserId;
    general.lastMessageAt = now;
    general.recipeId = 'general-chat'; // Recipe for collaborative conversation
    general.privacy = {
      isPublic: true,
      requiresInvite: false,
      allowGuestAccess: false,
      searchable: true
    };
    general.settings = {
      allowThreads: true,
      allowReactions: true,
      allowFileSharing: true,
      messageRetentionDays: 365,
      slowMode: 0
    };
    general.members = [
      { userId: humanUserId, role: 'owner', joinedAt: now }
    ];
    general.tags = ['general', 'chat'];
    rooms.push(general);

    // Academy room - learning and educational discussions
    const academy = new RoomEntity();
    academy.uniqueId = ROOM_UNIQUE_IDS.ACADEMY;
    academy.name = 'academy';
    academy.displayName = 'Academy';
    academy.description = 'Learning and educational discussions';
    academy.topic = 'Study, tutorials, and knowledge sharing';
    academy.type = 'public';
    academy.status = 'active';
    academy.ownerId = humanUserId;
    academy.lastMessageAt = now;
    academy.recipeId = 'general-chat'; // Same recipe for now (TODO: create academy-specific recipe)
    academy.privacy = {
      isPublic: true,
      requiresInvite: false,
      allowGuestAccess: false,
      searchable: true
    };
    academy.settings = {
      allowThreads: true,
      allowReactions: true,
      allowFileSharing: true,
      messageRetentionDays: 365,
      slowMode: 0
    };
    academy.members = [
      { userId: humanUserId, role: 'owner', joinedAt: now }
    ];
    academy.tags = ['education', 'learning'];
    rooms.push(academy);

    // Pantheon room - SOTA models only
    const pantheon = new RoomEntity();
    pantheon.uniqueId = ROOM_UNIQUE_IDS.PANTHEON;
    pantheon.name = 'pantheon';
    pantheon.displayName = 'Pantheon';
    pantheon.description = 'Elite discussion room for top-tier SOTA AI models';
    pantheon.topic = 'Advanced reasoning and multi-model collaboration';
    pantheon.type = 'public';
    pantheon.status = 'active';
    pantheon.ownerId = humanUserId;
    pantheon.lastMessageAt = now;
    pantheon.recipeId = 'general-chat'; // Same recipe for now (TODO: create pantheon-specific recipe)
    pantheon.privacy = {
      isPublic: true,
      requiresInvite: false,
      allowGuestAccess: false,
      searchable: true
    };
    pantheon.settings = {
      allowThreads: true,
      allowReactions: true,
      allowFileSharing: true,
      messageRetentionDays: 365,
      slowMode: 0
    };
    pantheon.members = [
      { userId: humanUserId, role: 'owner', joinedAt: now }
    ];
    pantheon.tags = ['sota', 'elite', 'reasoning'];
    rooms.push(pantheon);

    return {
      rooms: rooms as readonly RoomEntity[],
      totalCount: rooms.length,
      createdAt: now.toISOString()
    };
  }

  /**
   * Generate seed messages using ChatMessageEntity structure
   * Note: Room membership should be established before creating messages
   *
   * @param roomIds - Map of room unique IDs to their database IDs
   * @param humanUserId - The userId of the system owner (from SystemIdentity)
   * @param senderName - Name of the system owner (from SystemIdentity)
   *
   * REMOVED: Generic welcome messages are redundant - room info is already in the header
   */
  public static generateSeedMessages(roomIds: Map<string, UUID>, humanUserId: UUID, senderName: string = 'Human'): ChatMessageEntity[] {
    // No seed messages - let real conversations start naturally
    return [];
  }

  /**
   * Create JTAG data/store command for room (uses entity validation)
   */
  public static createRoomStoreData(room: RoomEntity): RoomEntity {
    const validation = room.validate();
    if (!validation.success) {
      throw new Error(`Room validation failed: ${validation.error}`);
    }
    return room;
  }

  /**
   * Create JTAG data/store command for message (uses entity validation)
   */
  public static createMessageStoreData(message: ChatMessageEntity): ChatMessageEntity {
    const validation = message.validate();
    if (!validation.success) {
      throw new Error(`Message validation failed: ${validation.error}`);
    }
    return message;
  }
}

export default RoomDataSeed;