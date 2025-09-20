/**
 * User Event Constants
 *
 * Defines event types for user entity operations (create, update, delete)
 * Follows the same pattern as CHAT_EVENTS for consistency
 */

export const USER_EVENTS = {
  USER_CREATED: 'user:user-created',
  USER_UPDATED: 'user:user-updated',
  USER_DELETED: 'user:user-deleted',
  USER_STATUS_CHANGED: 'user:status-changed'
} as const;

export const USER_EVENT_TYPES = Object.values(USER_EVENTS);

export type UserEventType = typeof USER_EVENTS[keyof typeof USER_EVENTS];