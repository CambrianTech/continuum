/**
 * Room Data Seeding - Centralized Chat Room Creation
 *
 * Creates initial chat rooms using proper RoomEntity structure.
 * Uses JTAG data commands and stable uniqueId constants.
 */

import { RoomEntity } from '../../system/data/entities/RoomEntity';
import { ChatMessageEntity } from '../../system/data/entities/ChatMessageEntity';
import { ROOM_UNIQUE_IDS } from '../../system/data/constants/RoomConstants';
import { USER_IDS } from './SeedConstants';
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
   */
  public static generateSeedRooms(): RoomSeedData {
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
    general.ownerId = USER_IDS.HUMAN as UUID;
    general.lastMessageAt = now;
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
      { userId: USER_IDS.HUMAN as UUID, role: 'owner', joinedAt: now }
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
    academy.ownerId = USER_IDS.HUMAN as UUID;
    academy.lastMessageAt = now;
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
      { userId: USER_IDS.HUMAN as UUID, role: 'owner', joinedAt: now }
    ];
    academy.tags = ['education', 'learning'];
    rooms.push(academy);

    return {
      rooms: rooms as readonly RoomEntity[],
      totalCount: rooms.length,
      createdAt: now.toISOString()
    };
  }

  /**
   * Generate seed messages using ChatMessageEntity structure
   * Note: Room membership should be established before creating messages
   */
  public static generateSeedMessages(roomIds: Map<string, UUID>): ChatMessageEntity[] {
    const messages: ChatMessageEntity[] = [];
    const now = new Date();

    const generalRoomId = roomIds.get(ROOM_UNIQUE_IDS.GENERAL);
    const academyRoomId = roomIds.get(ROOM_UNIQUE_IDS.ACADEMY);

    if (generalRoomId) {
      // Welcome message for general room
      const welcome = new ChatMessageEntity();
      welcome.roomId = generalRoomId;
      welcome.senderId = USER_IDS.HUMAN as UUID;
      welcome.senderName = 'Joel';
      welcome.content = {
        text: 'Welcome to the General room! This is where we discuss development, collaborate, and share ideas.',
        attachments: []
      };
      welcome.status = 'sent';
      welcome.priority = 'normal';
      welcome.timestamp = now;
      welcome.reactions = [];
      messages.push(welcome);
    }

    if (academyRoomId) {
      // Academy welcome
      const academyWelcome = new ChatMessageEntity();
      academyWelcome.roomId = academyRoomId;
      academyWelcome.senderId = USER_IDS.HUMAN as UUID;
      academyWelcome.senderName = 'Joel';
      academyWelcome.content = {
        text: 'Welcome to the Academy! This room is for learning, tutorials, and educational discussions.',
        attachments: []
      };
      academyWelcome.status = 'sent';
      academyWelcome.priority = 'normal';
      academyWelcome.timestamp = new Date(now.getTime() + 1000);
      academyWelcome.reactions = [];
      messages.push(academyWelcome);
    }

    return messages;
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