/**
 * Seed Data Constants - Single Source of Truth
 *
 * All IDs, collections, and seed data values defined once.
 * Typing like Rust - strict, explicit, and predictable.
 */

import { COLLECTIONS } from '../../system/data/core/FieldMapping';
import {
  DEFAULT_USERS,
  DEFAULT_ROOMS,
  DEFAULT_MESSAGES,
  USER_CONFIG,
  ROOM_CONFIG,
  MESSAGE_CONTENT
} from '../../system/data/domains/DefaultEntities';

// Re-export the authoritative COLLECTIONS from FieldMapping for consistency
export { COLLECTIONS } from '../../system/data/core/FieldMapping';

// Re-export shared constants for backward compatibility
export const USER_IDS = DEFAULT_USERS;
export const ROOM_IDS = DEFAULT_ROOMS;
export const MESSAGE_IDS = DEFAULT_MESSAGES;

// Re-export configuration data
export { USER_CONFIG, ROOM_CONFIG, MESSAGE_CONTENT } from '../../system/data/domains/DefaultEntities';


// Type-safe getters to ensure constants are used correctly
export function getUserId(key: keyof typeof USER_IDS): string {
  return USER_IDS[key];
}

export function getRoomId(key: keyof typeof ROOM_IDS): string {
  return ROOM_IDS[key];
}

export function getMessageId(key: keyof typeof MESSAGE_IDS): string {
  return MESSAGE_IDS[key];
}

export function getCollectionName(key: keyof typeof COLLECTIONS): string {
  return COLLECTIONS[key];
}