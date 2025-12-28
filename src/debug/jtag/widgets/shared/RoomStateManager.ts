/**
 * Room State Manager - Centralized room selection state
 *
 * Ensures ChatWidget, RoomListWidget, and all UI components
 * stay synchronized with the current selected room.
 */

import { Events } from '../../system/core/shared/Events';
import { UI_EVENTS } from '../../system/core/shared/EventConstants';
import { DEFAULT_ROOMS } from '../../system/data/domains/DefaultEntities';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

export interface RoomState {
  roomId: UUID | null;
  roomName: string | null;
  uniqueId?: string | null;  // For human-readable URL building
}

class RoomStateManager {
  private currentRoom: RoomState = {
    roomId: null,
    roomName: null
  };

  private static instance: RoomStateManager;

  static getInstance(): RoomStateManager {
    if (!RoomStateManager.instance) {
      RoomStateManager.instance = new RoomStateManager();
    }
    return RoomStateManager.instance;
  }

  getCurrentRoom(): RoomState {
    return { ...this.currentRoom };
  }

  selectRoom(roomId: UUID, roomName: string, uniqueId?: string): void {
    console.log(`🏠 RoomStateManager: Selecting room "${roomName}" (${roomId})`);

    this.currentRoom = {
      roomId,
      roomName,
      uniqueId: uniqueId || null  // For human-readable URL building
    };

    // Emit event for all widgets to update
    Events.emit(UI_EVENTS.ROOM_SELECTED, this.currentRoom);
  }

  selectDefaultRoom(): void {
    // Auto-select General room on startup (with uniqueId for human-readable URLs)
    this.selectRoom(DEFAULT_ROOMS.GENERAL, 'General', 'general');
  }

  clearSelection(): void {
    console.log(`🏠 RoomStateManager: Clearing room selection`);

    this.currentRoom = {
      roomId: null,
      roomName: null
    };

    Events.emit(UI_EVENTS.ROOM_SELECTED, this.currentRoom);
  }

  subscribe(callback: (roomState: RoomState) => void): () => void {
    return Events.subscribe(UI_EVENTS.ROOM_SELECTED, callback);
  }
}

export const RoomState = RoomStateManager.getInstance();