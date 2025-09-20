/**
 * User Event Constants
 *
 * Event type constants for user entity events
 * Parallel to chat events but for user domain
 */

export const USER_EVENTS = {
  USER_CREATED: 'user:user-created',
  USER_UPDATED: 'user:user-updated',
  USER_DELETED: 'user:user-deleted',
  USER_STATUS_CHANGED: 'user:status-changed'
} as const;

export type UserEventName = typeof USER_EVENTS[keyof typeof USER_EVENTS];