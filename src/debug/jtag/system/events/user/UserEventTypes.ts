/**
 * User Event Types
 *
 * Type definitions for user entity events
 * Parallel to ChatEventTypes.ts for consistency
 */

import { USER_EVENTS } from './UserEventConstants';
import type { UserData } from '../../data/domains/User';
import type { BaseUser } from '../../../domain/user/BaseUser';

/**
 * User created event data
 * Sent when a new user (Human or AI) is created
 */
export interface UserCreatedEventData {
  eventType: 'user:user-created';
  userId: string;
  user: BaseUser;
  userData: UserData;
  timestamp: string;
  [key: string]: unknown;
}

/**
 * User updated event data
 * Sent when user profile, status, or capabilities are modified
 */
export interface UserUpdatedEventData {
  eventType: 'user:user-updated';
  userId: string;
  user: BaseUser;
  userData: UserData;
  changes: string[]; // Array of changed field names
  timestamp: string;
  [key: string]: unknown;
}

/**
 * User deleted event data
 * Sent when a user is removed from the system
 */
export interface UserDeletedEventData {
  eventType: 'user:user-deleted';
  userId: string;
  timestamp: string;
  [key: string]: unknown;
}

/**
 * User status changed event data
 * Sent when user online status changes
 */
export interface UserStatusChangedEventData {
  eventType: 'user:status-changed';
  userId: string;
  status: 'online' | 'offline' | 'away' | 'busy';
  lastActiveAt: string;
  timestamp: string;
  [key: string]: unknown;
}

/**
 * Type map for user events - used for type-safe event handling
 */
export type UserEventMap = {
  [USER_EVENTS.USER_CREATED]: UserCreatedEventData;
  [USER_EVENTS.USER_UPDATED]: UserUpdatedEventData;
  [USER_EVENTS.USER_DELETED]: UserDeletedEventData;
  [USER_EVENTS.USER_STATUS_CHANGED]: UserStatusChangedEventData;
};