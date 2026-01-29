/**
 * Entity Validation - shared validation logic for entity references
 *
 * Reused by:
 * - Recipe validation (validate defaults exist)
 * - Activity creation (validate inputs exist)
 * - Data commands (validate foreign keys)
 * - URL resolver (validate entity in path)
 */

import { Commands } from '../../core/shared/Commands';
import { DATA_COMMANDS } from '../../../commands/data/shared/DataCommandConstants';

const UUID_REGEX = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

/**
 * Check if a string is a valid UUIDv4
 */
export function isUUID(value: string): boolean {
  return UUID_REGEX.test(value);
}

/**
 * Validate that an entity reference exists in a collection
 * Accepts either UUID (id) or uniqueId
 *
 * @param collection - Collection to search (e.g., 'users', 'rooms')
 * @param value - UUID or uniqueId to validate
 * @returns true if entity exists, false otherwise
 */
export async function validateEntityRef(collection: string, value: string): Promise<boolean> {
  if (!value || !collection) return false;

  const filter = isUUID(value) ? { id: value } : { uniqueId: value };

  const result = await Commands.execute(DATA_COMMANDS.LIST, {
    collection,
    filter,
    limit: 1
  }) as unknown as { success: boolean; items?: unknown[] };

  return result.success && (result.items?.length ?? 0) > 0;
}

/**
 * Resolve an entity reference to its full entity
 * Returns undefined if not found
 *
 * @param collection - Collection to search
 * @param value - UUID or uniqueId to resolve
 * @returns The entity if found, undefined otherwise
 */
export async function resolveEntityRef<T>(collection: string, value: string): Promise<T | undefined> {
  if (!value || !collection) return undefined;

  const filter = isUUID(value) ? { id: value } : { uniqueId: value };

  const result = await Commands.execute(DATA_COMMANDS.LIST, {
    collection,
    filter,
    limit: 1
  }) as unknown as { success: boolean; items?: T[] };

  return result.success && result.items?.length ? result.items[0] : undefined;
}

/**
 * Validation result for batch validation
 */
export interface ValidationResult {
  valid: boolean;
  errors: string[];
}

/**
 * Validate multiple entity references
 *
 * @param refs - Array of { collection, value, name } to validate
 * @returns Validation result with any errors
 */
export async function validateEntityRefs(
  refs: Array<{ collection: string; value: string; name: string }>
): Promise<ValidationResult> {
  const errors: string[] = [];

  await Promise.all(
    refs.map(async ({ collection, value, name }) => {
      const exists = await validateEntityRef(collection, value);
      if (!exists) {
        errors.push(`${name}: "${value}" not found in ${collection}`);
      }
    })
  );

  return { valid: errors.length === 0, errors };
}
