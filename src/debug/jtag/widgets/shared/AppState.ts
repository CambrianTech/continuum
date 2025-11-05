/**
 * Global Application State Management
 *
 * Centralized reactive state that widgets can watch and modify.
 * Changes to state automatically trigger events for subscribers.
 */

import { ReactiveState } from './ReactiveState';
import type { RoomEntity } from '../../system/data/entities/RoomEntity';

export interface AppStateData {
  currentRoomId: string | null;
  currentRoomEntity: RoomEntity | null;
}

class AppStateManager extends ReactiveState<AppStateData> {
  constructor() {
    super({
      currentRoomId: null,
      currentRoomEntity: null
    });
  }

  /**
   * Change the current room (triggers watchers automatically)
   */
  changeRoom(roomId: string, roomEntity?: RoomEntity): void {
    console.log(`🎯 AppState: Changing room to "${roomId}"`);

    // Update state - watchers will be notified automatically
    this.current.currentRoomId = roomId;
    this.current.currentRoomEntity = roomEntity || null;
  }

  /**
   * Get current room info
   */
  getCurrentRoom(): { roomId: string | null, roomEntity: RoomEntity | null } {
    return {
      roomId: this.current.currentRoomId,
      roomEntity: this.current.currentRoomEntity
    };
  }
}

// Export singleton instance
export const appState = new AppStateManager();