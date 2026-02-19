/**
 * Handles — Universal async operation resolution service
 *
 * Provides create, resolve (short or long), status management, and cleanup
 * for persistent async operation references.
 *
 * Every async operation (social feed fetch, voice synthesis, AI inference,
 * coding agent tasks, proposal votes) creates a Handle. The handle persists
 * in SQLite, survives restarts, and is resolvable by either full UUID or
 * 6-char short form (#abc123).
 *
 * Usage:
 *   import { Handles } from '@system/core/shared/Handles';
 *
 *   // Create
 *   const handle = await Handles.create('social/feed', { sort: 'hot' }, requesterId);
 *   console.log(handle.shortId);  // "a1b2c3"
 *
 *   // Resolve (short or long)
 *   const record = await Handles.resolve('#a1b2c3');
 *   const record = await Handles.resolve('550e8400-e29b-41d4-a716-446655440000');
 *
 *   // Update status
 *   await Handles.markProcessing(handle.id);
 *   await Handles.markComplete(handle.id, resultPayload);
 *   await Handles.markFailed(handle.id, 'Network timeout');
 *
 *   // Query
 *   const pending = await Handles.listByStatus('pending');
 *   const mine = await Handles.listByRequester(myUserId);
 */

import { COLLECTIONS } from '../../shared/Constants';
import { HandleEntity } from '../../data/entities/HandleEntity';
import { Logger } from '../logging/Logger';
import { DataCreate } from '@commands/data/create/shared/DataCreateTypes';
import { DataList } from '@commands/data/list/shared/DataListTypes';
import { DataRead } from '@commands/data/read/shared/DataReadTypes';
import { DataUpdate } from '@commands/data/update/shared/DataUpdateTypes';
import type { UUID } from '../types/CrossPlatformUUID';
import { isValidUUID, toShortId, isShortId, normalizeShortId } from '../types/CrossPlatformUUID';
import type { HandleRef, HandleStatus, HandleRecord } from '../types/Handle';
import { DEFAULT_HANDLE_TTL_MS } from '../types/Handle';

const log = Logger.create('Handles');

/**
 * Convert a HandleEntity to a HandleRecord (the public-facing shape)
 */
function entityToRecord(entity: HandleEntity): HandleRecord {
  return {
    id: entity.id,
    shortId: toShortId(entity.id),
    type: entity.type,
    status: entity.status,
    params: entity.params,
    result: entity.result,
    error: entity.error,
    requestedBy: entity.requestedBy,
    createdAt: entity.createdAt instanceof Date ? entity.createdAt : new Date(entity.createdAt as string),
    updatedAt: entity.updatedAt instanceof Date ? entity.updatedAt : new Date(entity.updatedAt as string),
    expiresAt: entity.expiresAt
      ? (entity.expiresAt instanceof Date ? entity.expiresAt : new Date(entity.expiresAt as string))
      : undefined,
    retryCount: entity.retryCount,
  };
}

/**
 * Handles — Static service for universal async operation management
 */
export const Handles = {

  /**
   * Create a new handle for an async operation.
   *
   * @param type - Operation type (e.g., 'social/feed', 'voice/synthesize')
   * @param params - Original request parameters (JSON-serializable)
   * @param requestedBy - UUID of the requester
   * @param ttlMs - TTL in milliseconds (null = never expires, default: 5 minutes)
   * @returns The created HandleRecord with shortId
   */
  async create(
    type: string,
    params: unknown,
    requestedBy: UUID,
    ttlMs: number | null = DEFAULT_HANDLE_TTL_MS,
  ): Promise<HandleRecord> {
    const entity = HandleEntity.createHandle(type, params, requestedBy, ttlMs);

    const result = await DataCreate.execute<HandleEntity>({
      collection: COLLECTIONS.HANDLES,
      data: entity as unknown as Record<string, unknown>,
    });

    if (!result.success) {
      throw new Error(`Failed to create handle: ${result.error ?? 'Unknown error'}`);
    }

    log.info(`Created handle #${toShortId(entity.id)} type=${type} requestedBy=${toShortId(requestedBy)}`);
    return entityToRecord(entity);
  },

  /**
   * Resolve a handle by short ID (#abc123 / abc123) or full UUID.
   *
   * Short ID resolution: queries handles collection for entities
   * whose UUID ends with the 6-char suffix.
   *
   * @param ref - HandleRef: "#abc123", "abc123", or full UUID
   * @returns HandleRecord or null if not found
   */
  async resolve(ref: HandleRef): Promise<HandleRecord | null> {
    if (!ref) return null;

    const refStr = String(ref).trim();

    // Full UUID — direct lookup
    if (isValidUUID(refStr)) {
      const result = await DataRead.execute<HandleEntity>({
        collection: COLLECTIONS.HANDLES,
        id: refStr,
      });

      if (!result.found || !result.data) return null;
      return entityToRecord(result.data);
    }

    // Short ID — suffix match (use String() to avoid type guard narrowing)
    const normalized = String(refStr).replace(/^#/, '');
    if (!isShortId(normalized)) {
      log.warn(`Invalid handle reference: ${ref}`);
      return null;
    }

    const shortId = normalizeShortId(normalized);

    // Query all handles and filter by suffix
    // The $regex operator matches UUIDs ending with the short ID
    // Order by createdAt desc so the most recent match wins on collision
    const result = await DataList.execute<HandleEntity>({
      collection: COLLECTIONS.HANDLES,
      filter: { id: { $regex: `${shortId}$` } },
      orderBy: [{ field: 'createdAt', direction: 'desc' }],
      limit: 2, // Get 2 to detect ambiguity
    });

    if (!result.success || !result.items?.length) return null;

    if (result.items.length > 1) {
      log.warn(`Ambiguous short ID #${shortId} matched ${result.items.length} handles. Returning most recent.`);
    }

    return entityToRecord(result.items[0] as HandleEntity);
  },

  /**
   * Get a handle by exact UUID (faster than resolve for known UUIDs).
   */
  async get(id: UUID): Promise<HandleRecord | null> {
    const result = await DataRead.execute<HandleEntity>({
      collection: COLLECTIONS.HANDLES,
      id,
    });

    if (!result.found || !result.data) return null;
    return entityToRecord(result.data);
  },

  /**
   * Mark a handle as processing (worker picked it up).
   */
  async markProcessing(id: UUID): Promise<HandleRecord> {
    return this._updateStatus(id, 'processing');
  },

  /**
   * Mark a handle as complete with a result payload.
   */
  async markComplete(id: UUID, result: unknown): Promise<HandleRecord> {
    return this._updateStatus(id, 'complete', { result });
  },

  /**
   * Mark a handle as failed with an error message.
   */
  async markFailed(id: UUID, error: string): Promise<HandleRecord> {
    return this._updateStatus(id, 'failed', { error });
  },

  /**
   * Mark a handle as expired.
   */
  async markExpired(id: UUID): Promise<HandleRecord> {
    return this._updateStatus(id, 'expired');
  },

  /**
   * Mark a handle as cancelled.
   */
  async markCancelled(id: UUID): Promise<HandleRecord> {
    return this._updateStatus(id, 'cancelled');
  },

  /**
   * List handles by status.
   */
  async listByStatus(status: HandleStatus, limit = 50): Promise<HandleRecord[]> {
    const result = await DataList.execute<HandleEntity>({
      collection: COLLECTIONS.HANDLES,
      filter: { status },
      orderBy: [{ field: 'createdAt', direction: 'desc' }],
      limit,
    });

    if (!result.success) return [];
    return result.items.map(e => entityToRecord(e as HandleEntity));
  },

  /**
   * List handles by requester.
   */
  async listByRequester(requestedBy: UUID, limit = 50): Promise<HandleRecord[]> {
    const result = await DataList.execute<HandleEntity>({
      collection: COLLECTIONS.HANDLES,
      filter: { requestedBy },
      orderBy: [{ field: 'createdAt', direction: 'desc' }],
      limit,
    });

    if (!result.success) return [];
    return result.items.map(e => entityToRecord(e as HandleEntity));
  },

  /**
   * List handles by operation type.
   */
  async listByType(type: string, limit = 50): Promise<HandleRecord[]> {
    const result = await DataList.execute<HandleEntity>({
      collection: COLLECTIONS.HANDLES,
      filter: { type },
      orderBy: [{ field: 'createdAt', direction: 'desc' }],
      limit,
    });

    if (!result.success) return [];
    return result.items.map(e => entityToRecord(e as HandleEntity));
  },

  /**
   * List active (pending + processing) handles, optionally filtered by type.
   */
  async listActive(type?: string, limit = 50): Promise<HandleRecord[]> {
    const filter: Record<string, unknown> = {
      status: { $in: ['pending', 'processing'] },
    };
    if (type) filter.type = type;

    const result = await DataList.execute<HandleEntity>({
      collection: COLLECTIONS.HANDLES,
      filter,
      orderBy: [{ field: 'createdAt', direction: 'asc' }],
      limit,
    });

    if (!result.success) return [];
    return result.items.map(e => entityToRecord(e as HandleEntity));
  },

  /**
   * Expire all handles past their TTL. Call periodically (e.g., every 60s).
   * Processes in batches of 200 until all stale handles are expired.
   * Returns the total number of handles expired.
   */
  async expireStale(): Promise<number> {
    const now = new Date().toISOString();
    let totalExpired = 0;
    const BATCH_SIZE = 200;

    // Loop in batches until no more stale handles remain
    while (true) {
      const result = await DataList.execute<HandleEntity>({
        collection: COLLECTIONS.HANDLES,
        filter: {
          status: { $in: ['pending', 'processing'] },
          expiresAt: { $lte: now },
        },
        limit: BATCH_SIZE,
      });

      if (!result.success || !result.items?.length) break;

      for (const entity of result.items) {
        try {
          await this._updateStatus((entity as HandleEntity).id, 'expired');
          totalExpired++;
        } catch (err) {
          log.warn(`Failed to expire handle ${(entity as HandleEntity).id}: ${err}`);
        }
      }

      // If we got fewer than BATCH_SIZE, we've processed all of them
      if (result.items.length < BATCH_SIZE) break;
    }

    if (totalExpired > 0) {
      log.info(`Expired ${totalExpired} stale handles`);
    }
    return totalExpired;
  },

  /**
   * Internal: update handle status and optional fields.
   */
  async _updateStatus(
    id: UUID,
    status: HandleStatus,
    extra?: { result?: unknown; error?: string; params?: unknown },
  ): Promise<HandleRecord> {
    const updates: Record<string, unknown> = {
      status,
      updatedAt: new Date().toISOString(),
    };

    if (extra?.result !== undefined) updates.result = extra.result;
    if (extra?.error !== undefined) updates.error = extra.error;
    if (extra?.params !== undefined) updates.params = extra.params;
    if (status === 'failed') {
      // Increment retry count on failure — read current, increment, write back.
      // Safe: Node.js is single-threaded, so no concurrent failures for the same handle.
      const current = await DataRead.execute<HandleEntity>({
        collection: COLLECTIONS.HANDLES,
        id,
      });
      if (current.found && current.data) {
        updates.retryCount = ((current.data as HandleEntity).retryCount ?? 0) + 1;
      }
    }

    const result = await DataUpdate.execute<HandleEntity>({
      collection: COLLECTIONS.HANDLES,
      id,
      data: updates,
      incrementVersion: true,
    });

    if (!result.success || !result.data) {
      throw new Error(`Failed to update handle ${id} to ${status}: ${result.error ?? 'not found'}`);
    }

    log.debug(`Handle #${toShortId(id)} → ${status}`);
    return entityToRecord(result.data);
  },

} as const;
