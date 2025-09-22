/**
 * StateBrowser - Browser-specific state management
 *
 * Extends shared State base class with browser-specific features like localStorage persistence
 */

import { State, RoomState, type RoomEntity } from '../shared/State';
import type { UUID } from '../../types/CrossPlatformUUID';

/**
 * Browser-specific room state with localStorage persistence
 */
export class RoomStateBrowser extends RoomState {
  private static readonly STORAGE_KEY_CURRENT_ROOM = 'jtag_current_room_id';
  private static readonly STORAGE_KEY_ROOM_CACHE = 'jtag_room_cache';

  // Override currentId setter to persist to localStorage
  set currentId(id: UUID | null) {
    super.currentId = id;
    try {
      if (id) {
        localStorage.setItem(RoomStateBrowser.STORAGE_KEY_CURRENT_ROOM, id);
      } else {
        localStorage.removeItem(RoomStateBrowser.STORAGE_KEY_CURRENT_ROOM);
      }
    } catch (error) {
      console.warn('⚠️ Failed to persist current room to localStorage:', error);
    }
  }

  // Override setCurrentRoom to persist to localStorage (compatibility method)
  async setCurrentRoom(id: UUID): Promise<void> {
    this.currentId = id;
  }

  // Override clearCurrentRoom to clear localStorage (compatibility method)
  async clearCurrentRoom(): Promise<void> {
    this.currentId = null;
  }

  // Override cacheRoom to persist room data
  async cacheRoom(room: RoomEntity): Promise<void> {
    await super.cacheRoom(room);
    this.persistRoomCache();
  }

  // Load initial state from localStorage
  loadFromStorage(): void {
    try {
      // Load current room ID
      const currentRoomId = localStorage.getItem(RoomStateBrowser.STORAGE_KEY_CURRENT_ROOM);
      if (currentRoomId) {
        // Use the parent setter directly to avoid localStorage recursion during initialization
        super.currentId = currentRoomId as UUID;
      }

      // Load room cache
      const cachedRoomsData = localStorage.getItem(RoomStateBrowser.STORAGE_KEY_ROOM_CACHE);
      if (cachedRoomsData) {
        const cachedRooms = JSON.parse(cachedRoomsData) as RoomEntity[];
        cachedRooms.forEach(room => super.cacheRoom(room));
      }
    } catch (error) {
      console.warn('⚠️ Failed to load state from localStorage:', error);
    }
  }

  private persistRoomCache(): void {
    try {
      const rooms = this.getAllRooms();
      localStorage.setItem(RoomStateBrowser.STORAGE_KEY_ROOM_CACHE, JSON.stringify(rooms));
    } catch (error) {
      console.warn('⚠️ Failed to persist room cache to localStorage:', error);
    }
  }
}

/**
 * Browser-specific state management
 */
export class StateBrowser extends State {
  public readonly room: RoomStateBrowser;

  constructor() {
    super();
    this.room = new RoomStateBrowser();

    // Load initial state from localStorage when created
    this.room.loadFromStorage();
  }
}