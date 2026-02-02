/**
 * SessionStateHelper - Centralized convenience methods for accessing user state
 *
 * Like Android Context - provides intuitive, easy access to commonly needed user state:
 * - Current room (for room-aware commands)
 * - Current content item (tabs, documents, etc.)
 * - Open rooms/tabs
 * - Full user state
 *
 * Prevents every command from having to query and navigate UserStateEntity manually.
 */

import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { ContentItem } from '@system/data/entities/UserStateEntity';
import { UserStateEntity } from '@system/data/entities/UserStateEntity';
import { DataDaemon } from '@daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '@system/data/config/DatabaseConfig';
import { Logger, type ComponentLogger } from '@system/core/logging/Logger';

export class SessionStateHelper {
  private static log: ComponentLogger = Logger.create('SessionStateHelper', 'daemons/SessionStateHelper');

  /**
   * Get UserStateEntity for a given userId
   * @param userId - User ID to look up
   * @returns UserStateEntity with methods hydrated
   */
  static async getUserState(userId: UUID): Promise<UserStateEntity | null> {
    try {
      const userStateData = await DataDaemon.read<UserStateEntity>(COLLECTIONS.USER_STATES, userId);

      if (!userStateData) {
        this.log.warn(`UserState not found for userId: ${userId}`);
        return null;
      }

      // Hydrate UserStateEntity to get instance methods
      const userState = Object.assign(new UserStateEntity(), userStateData);

      return userState;
    } catch (error) {
      this.log.error('Failed to get user state:', error);
      return null;
    }
  }

  /**
   * Get current content item (tab/document/room user is viewing)
   * @param userId - User ID to look up
   * @returns Current ContentItem or null if none
   */
  static async getCurrentContentItem(userId: UUID): Promise<ContentItem | null> {
    const userState = await this.getUserState(userId);
    if (!userState) {
      return null;
    }

    return userState.getCurrentContentItem() || null;
  }

  /**
   * Get current room ID from user's content state
   * Convenience method for commands that need to auto-detect current room
   * @param userId - User ID to look up
   * @returns Room ID (entityId) if current content is a chat room, otherwise null
   */
  static async getCurrentRoom(userId: UUID): Promise<UUID | null> {
    const currentItem = await this.getCurrentContentItem(userId);

    if (!currentItem) {
      this.log.debug(`No current content item for user ${userId}`);
      return null;
    }

    // Only return entityId if current content is a chat room
    if (currentItem.type === 'chat' && currentItem.entityId) {
      return currentItem.entityId;
    }

    this.log.debug(`Current content type is '${currentItem.type}', not 'chat'`);
    return null;
  }

  /**
   * Get all open chat rooms for a user
   * @param userId - User ID to look up
   * @returns Array of ContentItems that are chat rooms
   */
  static async getOpenRooms(userId: UUID): Promise<ContentItem[]> {
    const userState = await this.getUserState(userId);
    if (!userState) {
      return [];
    }

    // Filter open items to only chat rooms
    return userState.contentState.openItems.filter(item => item.type === 'chat');
  }

  /**
   * Get all open content items (tabs) for a user
   * @param userId - User ID to look up
   * @returns Array of all open ContentItems
   */
  static async getOpenContent(userId: UUID): Promise<ContentItem[]> {
    const userState = await this.getUserState(userId);
    if (!userState) {
      return [];
    }

    return userState.contentState.openItems;
  }

  /**
   * Check if user has a specific room open
   * @param userId - User ID to check
   * @param roomId - Room ID to look for
   * @returns true if room is in user's open items
   */
  static async isRoomOpen(userId: UUID, roomId: UUID): Promise<boolean> {
    const openRooms = await this.getOpenRooms(userId);
    return openRooms.some(room => room.entityId === roomId);
  }

  /**
   * Get user's current working directory from shell state
   * @param userId - User ID to look up
   * @returns Current working directory or null
   */
  static async getCurrentWorkingDir(userId: UUID): Promise<string | null> {
    const userState = await this.getUserState(userId);
    if (!userState?.shellState) {
      return null;
    }

    return userState.shellState.currentWorkingDir;
  }
}
