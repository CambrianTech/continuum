/**
 * Content and Room Data Classes for MainWidget
 * 
 * Proper TypeScript data classes for managing room/content information
 * with database integration support.
 */

export interface ContentInfo {
  id: string;
  name: string;
  type: 'room' | 'user_chat' | 'system';
  path: string;
  displayName: string;
  description?: string;
  isActive: boolean;
  participants?: string[];
  createdAt: Date;
  updatedAt: Date;
}

export interface RoomData {
  roomId: string;
  roomType: 'general' | 'academy' | 'user_chat' | 'private';
  name: string;
  displayName: string;
  description: string;
  participants: string[];
  isActive: boolean;
  messageCount: number;
  lastActivity: Date;
  createdBy?: string;
  createdAt: Date;
  updatedAt: Date;
}

export class ContentInfoManager {
  constructor(private widgetContext: any) {}

  /**
   * Get content info for a given path
   */
  async getContentByPath(path: string): Promise<ContentInfo | null> {
    const [, pathType, roomId] = path.split('/');
    
    if (pathType === 'chat') {
      return await this.getChatContentInfo(roomId);
    }
    
    return null;
  }

  /**
   * Get chat content information from database
   */
  private async getChatContentInfo(roomId: string): Promise<ContentInfo> {
    // Query database daemon for room information
    const roomData = await this.getRoomData(roomId);
    
    return {
      id: roomId,
      name: roomData.name,
      type: roomData.roomType === 'user_chat' ? 'user_chat' : 'room',
      path: `/chat/${roomId}`,
      displayName: roomData.displayName,
      description: roomData.description,
      isActive: roomData.isActive,
      participants: roomData.participants,
      createdAt: roomData.createdAt,
      updatedAt: roomData.updatedAt
    };
  }

  /**
   * Get room data from database daemon
   */
  private async getRoomData(roomId: string): Promise<RoomData> {
    try {
      // Use widget's database methods to query room data
      const roomExists = await this.widgetContext.getData(`room_${roomId}_exists`, false);
      
      if (roomExists) {
        // Get full room data from database
        const roomInfo = await this.widgetContext.getData(`room_${roomId}_info`, null);
        if (roomInfo) {
          return roomInfo as RoomData;
        }
      }
      
      // Create default room data if not found
      return await this.createDefaultRoomData(roomId);
    } catch (error) {
      console.error(`Failed to get room data for ${roomId}:`, error);
      return await this.createDefaultRoomData(roomId);
    }
  }

  /**
   * Create default room data for new or standard rooms
   */
  private async createDefaultRoomData(roomId: string): Promise<RoomData> {
    const now = new Date();
    
    // Default room configurations
    const defaultRooms: { [key: string]: Partial<RoomData> } = {
      'general': {
        name: 'general',
        displayName: 'General',
        description: 'General discussion room',
        roomType: 'general'
      },
      'academy': {
        name: 'academy', 
        displayName: 'Academy',
        description: 'Academy learning room',
        roomType: 'academy'
      }
    };

    let roomConfig = defaultRooms[roomId];
    
    // Handle user chat rooms
    if (roomId.startsWith('user-')) {
      roomConfig = {
        name: roomId,
        displayName: 'User Chat',
        description: 'Private user chat room',
        roomType: 'user_chat'
      };
    }

    // Create full room data object
    const roomData: RoomData = {
      roomId,
      roomType: (roomConfig?.roomType as any) || 'private',
      name: roomConfig?.name || roomId,
      displayName: roomConfig?.displayName || this.capitalizeFirst(roomId),
      description: roomConfig?.description || `Chat room: ${roomId}`,
      participants: [],
      isActive: true,
      messageCount: 0,
      lastActivity: now,
      createdAt: now,
      updatedAt: now
    };

    // Store in database if this widget has database access
    if (this.widgetContext.storeData) {
      await this.widgetContext.storeData(`room_${roomId}_info`, roomData, { persistent: true });
      await this.widgetContext.storeData(`room_${roomId}_exists`, true, { persistent: true });
    }

    return roomData;
  }

  /**
   * Create a new room (for user chats, etc.)
   */
  async createRoom(roomId: string, roomType: RoomData['roomType'] = 'private'): Promise<RoomData> {
    const roomData = await this.createDefaultRoomData(roomId);
    roomData.roomType = roomType;
    
    // Store in database
    if (this.widgetContext.storeData) {
      await this.widgetContext.storeData(`room_${roomId}_info`, roomData, { persistent: true });
      await this.widgetContext.storeData(`room_${roomId}_exists`, true, { persistent: true });
      await this.widgetContext.storeData(`room_${roomId}_messages`, [], { persistent: true });
    }
    
    console.log(`🏠 ContentInfoManager: Created new room: ${roomId} (${roomType})`);
    return roomData;
  }

  /**
   * Utility method for capitalizing strings
   */
  private capitalizeFirst(str: string): string {
    return str.charAt(0).toUpperCase() + str.slice(1);
  }
}