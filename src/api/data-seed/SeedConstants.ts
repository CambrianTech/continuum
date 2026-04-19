/**
 * Seed Data Constants — display/content metadata only.
 *
 * The legacy `USER_IDS` / `ROOM_IDS` / `MESSAGE_IDS` re-exports are GONE.
 * They re-exported the deterministic-UUID maps from `DefaultEntities`,
 * which violated the system-generated-globally-unique UUID rule and
 * produced ghost rooms/messages whose UUIDs never matched the seeded
 * `uuidv4()` IDs.
 *
 * Migration: resolve real UUIDs at call time via
 *   Commands.execute('data/list', { collection, filter: { uniqueId } })
 * and use the resolved row's `id`. See the comment at the top of
 * `DefaultEntities.ts` for the full pattern.
 */

import { COLLECTIONS } from '../../system/data/core/FieldMapping';

// Re-export the authoritative COLLECTIONS from FieldMapping for consistency
export { COLLECTIONS } from '../../system/data/core/FieldMapping';

// Re-export configuration data — labels and prose, no UUIDs.
export { USER_CONFIG, ROOM_CONFIG, MESSAGE_CONTENT } from '../../system/data/domains/DefaultEntities';

export function getCollectionName(key: keyof typeof COLLECTIONS): string {
  return COLLECTIONS[key];
}