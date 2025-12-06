/**
 * IUserStateManager - Interface for UserStateEntity management
 *
 * Abstracts UserStateEntity operations from storage implementation
 * Handles state persistence, content tab management, room read state, etc.
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { UserStateEntity, ContentItem } from '../../../data/entities/UserStateEntity';

export interface IUserStateManager {
  /**
   * Load user state (creates default if doesn't exist)
   */
  load(): Promise<UserStateEntity>;

  /**
   * Save user state
   */
  save(state: UserStateEntity): Promise<{ success: boolean; error?: string }>;

  /**
   * Get current state (cached)
   */
  getState(): UserStateEntity | null;

  /**
   * Add content item to open tabs
   */
  addContentItem(item: Omit<ContentItem, 'lastAccessedAt'>): Promise<void>;

  /**
   * Remove content item from open tabs
   */
  removeContentItem(itemId: UUID): Promise<void>;

  /**
   * Set current content focus
   */
  setCurrentContent(itemId: UUID): Promise<boolean>;

  /**
   * Update room read state
   */
  updateRoomReadState(roomId: UUID, timestamp: Date, messageId?: UUID): Promise<void>;

  /**
   * Get last read timestamp for a room
   */
  getLastReadTimestamp(roomId: UUID): Date | undefined;

  /**
   * Check if message is unread
   */
  isMessageUnread(roomId: UUID, messageTimestamp: Date): boolean;
}
