/**
 * UserEntityCache - In-memory TTL cache for UserEntity reads
 *
 * Eliminates redundant ORM.read(COLLECTIONS.USERS, userId) calls.
 * Before this cache:
 *   - CallerDetector reads UserEntity on EVERY command (no cache)
 *   - UserDaemon monitoring loop reads ALL users every 5s
 *   - SessionDaemon reads user on every session lookup
 *   = hundreds of redundant reads/minute → IPC socket saturation → ORM timeouts
 *
 * After:
 *   - First read populates cache, subsequent reads are O(1) Map lookups
 *   - Bulk setAll() from monitoring loop pre-warms the cache
 *   - 60s TTL ensures staleness is bounded
 *   - invalidate() for immediate eviction on known mutations
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { ORM } from '../../../daemons/data-daemon/server/ORM';
import { COLLECTIONS } from '../../shared/Constants';
import type { UserEntity } from '../../data/entities/UserEntity';

interface CacheEntry {
  entity: UserEntity;
  expiresAt: number;
}

export class UserEntityCache {
  private static _instance: UserEntityCache;

  private readonly _cache = new Map<UUID, CacheEntry>();
  private readonly _ttlMs = 60_000; // 60 seconds

  static get instance(): UserEntityCache {
    if (!UserEntityCache._instance) {
      UserEntityCache._instance = new UserEntityCache();
    }
    return UserEntityCache._instance;
  }

  /**
   * Get a cached UserEntity, or null if missing/expired.
   */
  get(userId: UUID): UserEntity | null {
    const entry = this._cache.get(userId);
    if (!entry) return null;
    if (Date.now() > entry.expiresAt) {
      this._cache.delete(userId);
      return null;
    }
    return entry.entity;
  }

  /**
   * Cache a single UserEntity.
   */
  set(userId: UUID, entity: UserEntity): void {
    this._cache.set(userId, {
      entity,
      expiresAt: Date.now() + this._ttlMs,
    });
  }

  /**
   * Bulk-cache an array of UserEntities (e.g., from monitoring loop query).
   * Single timestamp calculation for the whole batch.
   */
  setAll(entities: UserEntity[]): void {
    const expiresAt = Date.now() + this._ttlMs;
    for (const entity of entities) {
      if (entity.id) {
        this._cache.set(entity.id, { entity, expiresAt });
      }
    }
  }

  /**
   * Invalidate a specific user (call after known mutations).
   */
  invalidate(userId: UUID): void {
    this._cache.delete(userId);
  }

  /**
   * Read-through cache: returns cached entity or fetches from ORM.
   * This is the primary API for callers replacing raw ORM.read().
   */
  async read(userId: UUID): Promise<UserEntity | null> {
    const cached = this.get(userId);
    if (cached) return cached;

    const entity = await ORM.read<UserEntity>(COLLECTIONS.USERS, userId);
    if (entity) {
      this.set(userId, entity);
    }
    return entity;
  }

  /**
   * Number of cached entries (for diagnostics).
   */
  get size(): number {
    return this._cache.size;
  }
}
