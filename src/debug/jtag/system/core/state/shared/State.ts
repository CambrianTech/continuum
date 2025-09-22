/**
 * State Management - Simple and Clean
 *
 * Like Events.emit|subscribe but for state management
 * No complex generics, just simple working code
 */

import type { UUID } from '../../types/CrossPlatformUUID';

/**
 * Simple room entity
 */
export interface RoomEntity {
  id: UUID;
  displayName: string;
  description?: string;
  participants?: UUID[];
}

/**
 * Room state management
 */
export class RoomState {
  private entities = new Map<UUID, RoomEntity>();
  private _currentId: UUID | null = null;
  private subscribers = new Set<() => void>();

  // Current room ID as property
  get currentId(): UUID | null {
    return this._currentId;
  }

  set currentId(id: UUID | null) {
    this._currentId = id;
    this.notifySubscribers();
  }

  // Current room entity as property
  get currentRoom(): RoomEntity | null {
    return this._currentId ? this.entities.get(this._currentId) || null : null;
  }

  // Set current room (async method for compatibility)
  async setCurrentRoom(id: UUID): Promise<void> {
    this.currentId = id;
  }

  // Get current room (method for compatibility)
  getCurrentRoom(): RoomEntity | null {
    return this.currentRoom;
  }

  // Get current room ID (method for compatibility)
  getCurrentRoomId(): UUID | null {
    return this.currentId;
  }

  // Cache room
  async cacheRoom(room: RoomEntity): Promise<void> {
    this.entities.set(room.id, room);
    this.notifySubscribers();
  }

  // Get room by ID
  getRoom(id: UUID): RoomEntity | null {
    return this.entities.get(id) || null;
  }

  // Get all rooms
  getAllRooms(): RoomEntity[] {
    return Array.from(this.entities.values());
  }

  // Subscribe to changes
  subscribe(callback: () => void): () => void {
    this.subscribers.add(callback);
    return () => this.subscribers.delete(callback);
  }

  // Get room display name
  getRoomDisplayName(roomId?: UUID): string {
    const id = roomId || this.currentId;
    if (!id) return 'Unknown Room';

    const room = this.entities.get(id);
    return room?.displayName || 'Room';
  }

  // Refresh current room (compatibility method)
  async refreshCurrentRoom(): Promise<void> {
    if (this.currentId) {
      this.notifySubscribers();
    }
  }

  // Check if room is current
  isCurrentRoom(roomId: UUID): boolean {
    return this.currentId === roomId;
  }

  // Clear current room (method for compatibility)
  async clearCurrentRoom(): Promise<void> {
    this.currentId = null;
  }

  // Get state (compatibility method)
  getState() {
    return {
      currentRoomId: this.currentId,
      currentRoomEntity: this.getCurrentRoom(),
      availableRooms: this.entities,
      loading: false,
      error: null
    };
  }

  private notifySubscribers(): void {
    this.subscribers.forEach(callback => callback());
  }
}

/**
 * State management - simple and clean
 */
export abstract class State {
  public readonly room = new RoomState();
}