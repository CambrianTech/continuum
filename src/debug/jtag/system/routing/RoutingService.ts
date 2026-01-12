/**
 * RoutingService - Single source of truth for URL ↔ Entity resolution
 *
 * Resolves human-readable uniqueIds (e.g., "general") to UUIDs for database operations,
 * and builds human-readable URLs from entities.
 *
 * Key principle: URLs use uniqueId for readability, database uses UUID.
 *   /chat/general → UUID lookup → 5e71a0c8-3038-4647-83a1-27631d385d72
 *
 * Usage:
 *   // Resolve identifier to UUID
 *   const roomId = await RoutingService.resolveRoom('general');
 *
 *   // Build URL from entity
 *   const url = RoutingService.buildRoomPath(room);  // → /chat/general
 */

import { Commands } from '../core/shared/Commands';
import { DATA_COMMANDS } from '../../commands/data/shared/DataCommandConstants';
import type { DataListParams, DataListResult } from '../../commands/data/list/shared/DataListTypes';
import type { RoomEntity } from '../data/entities/RoomEntity';
import type { UserEntity } from '../data/entities/UserEntity';
import type { UUID } from '../core/types/CrossPlatformUUID';
import { entityCache } from '../state/EntityCacheService';

/**
 * UUID regex pattern
 */
const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}/i;

/**
 * Check if a string looks like a UUID
 */
function isUUID(str: string): boolean {
  return UUID_PATTERN.test(str);
}

/**
 * Resolution result with both UUID and display info
 */
export interface ResolvedEntity {
  id: UUID;
  uniqueId: string;
  displayName: string;
}

class RoutingServiceImpl {
  private roomCache: Map<string, ResolvedEntity> = new Map();
  private userCache: Map<string, ResolvedEntity> = new Map();
  private readonly CACHE_TTL_MS = 5 * 60 * 1000; // 5 minutes
  private cacheTimestamps: Map<string, number> = new Map();

  /**
   * Resolve a room identifier (uniqueId or UUID) to its UUID
   * Returns null if not found
   *
   * Resolution order (Positronic cache-first):
   * 1. Local resolution cache (fast, TTL-based)
   * 2. EntityCacheService (unified entity cache)
   * 3. Database query (populates both caches)
   */
  async resolveRoom(identifier: string): Promise<ResolvedEntity | null> {
    // 1. Check local resolution cache first
    const cached = this.getFromCache(this.roomCache, identifier);
    if (cached) return cached;

    // 2. Check EntityCacheService (Positronic: single source of truth)
    const fromEntityCache = this.resolveRoomFromEntityCache(identifier);
    if (fromEntityCache) {
      this.addToCache(this.roomCache, identifier, fromEntityCache);
      return fromEntityCache;
    }

    try {
      // 3. Database query - populate both caches
      // If already a UUID, look up directly
      if (isUUID(identifier)) {
        const result = await Commands.execute<DataListParams, DataListResult<RoomEntity>>(
          DATA_COMMANDS.LIST,
          {
            collection: 'rooms',
            filter: { id: identifier },
            limit: 1
          }
        );

        if (result.success && result.items?.[0]) {
          const room = result.items[0];
          // Populate EntityCacheService
          entityCache.populate<RoomEntity>('rooms', [room]);

          const resolved: ResolvedEntity = {
            id: room.id as UUID,
            uniqueId: room.uniqueId || room.id,
            displayName: room.displayName || room.name || room.uniqueId || room.id
          };
          this.addToCache(this.roomCache, identifier, resolved);
          return resolved;
        }
        return null;
      }

      // Look up by uniqueId
      const result = await Commands.execute<DataListParams, DataListResult<RoomEntity>>(
        DATA_COMMANDS.LIST,
        {
          collection: 'rooms',
          filter: { uniqueId: identifier },
          limit: 1
        }
      );

      if (result.success && result.items?.[0]) {
        const room = result.items[0];
        // Populate EntityCacheService
        entityCache.populate<RoomEntity>('rooms', [room]);

        const resolved: ResolvedEntity = {
          id: room.id as UUID,
          uniqueId: room.uniqueId || room.id,
          displayName: room.displayName || room.name || room.uniqueId || room.id
        };
        this.addToCache(this.roomCache, identifier, resolved);
        // Also cache by UUID for faster future lookups
        this.addToCache(this.roomCache, room.id, resolved);
        return resolved;
      }

      // 4. Query by name (legacy support for commands using room name)
      const nameResult = await Commands.execute<DataListParams, DataListResult<RoomEntity>>(
        DATA_COMMANDS.LIST,
        {
          collection: 'rooms',
          filter: { name: identifier },
          limit: 1
        }
      );

      if (nameResult.success && nameResult.items?.[0]) {
        const room = nameResult.items[0];
        // Populate EntityCacheService
        entityCache.populate<RoomEntity>('rooms', [room]);

        const resolved: ResolvedEntity = {
          id: room.id as UUID,
          uniqueId: room.uniqueId || room.id,
          displayName: room.displayName || room.name || room.uniqueId || room.id
        };
        this.addToCache(this.roomCache, identifier, resolved);
        // Also cache by UUID and uniqueId for faster future lookups
        this.addToCache(this.roomCache, room.id, resolved);
        if (room.uniqueId) {
          this.addToCache(this.roomCache, room.uniqueId, resolved);
        }
        return resolved;
      }

      console.warn(`🔍 RoutingService: Room not found: ${identifier}`);
      return null;
    } catch (error) {
      console.error(`❌ RoutingService: Error resolving room ${identifier}:`, error);
      return null;
    }
  }

  /**
   * Try to resolve room from EntityCacheService (instant, no DB query)
   */
  private resolveRoomFromEntityCache(identifier: string): ResolvedEntity | null {
    // Check if it's a UUID - try direct lookup
    if (isUUID(identifier)) {
      const room = entityCache.get<RoomEntity>('rooms', identifier);
      if (room) {
        return {
          id: room.id as UUID,
          uniqueId: room.uniqueId || room.id,
          displayName: room.displayName || room.name || room.uniqueId || room.id
        };
      }
    }

    // Try to find by uniqueId in cached rooms
    const rooms = entityCache.getAll<RoomEntity>('rooms');
    const room = rooms.find(r => r.uniqueId === identifier);
    if (room) {
      return {
        id: room.id as UUID,
        uniqueId: room.uniqueId || room.id,
        displayName: room.displayName || room.name || room.uniqueId || room.id
      };
    }

    return null;
  }

  /**
   * Resolve a user identifier (uniqueId or UUID) to its UUID
   * Returns null if not found
   *
   * Resolution order (Positronic cache-first):
   * 1. Local resolution cache (fast, TTL-based)
   * 2. EntityCacheService (unified entity cache)
   * 3. Database query (populates both caches)
   */
  async resolveUser(identifier: string): Promise<ResolvedEntity | null> {
    // 1. Check local resolution cache first
    const cached = this.getFromCache(this.userCache, identifier);
    if (cached) return cached;

    // 2. Check EntityCacheService (Positronic: single source of truth)
    const fromEntityCache = this.resolveUserFromEntityCache(identifier);
    if (fromEntityCache) {
      this.addToCache(this.userCache, identifier, fromEntityCache);
      return fromEntityCache;
    }

    try {
      // 3. Database query - populate both caches
      // If already a UUID, look up directly
      if (isUUID(identifier)) {
        const result = await Commands.execute<DataListParams, DataListResult<UserEntity>>(
          DATA_COMMANDS.LIST,
          {
            collection: 'users',
            filter: { id: identifier },
            limit: 1
          }
        );

        if (result.success && result.items?.[0]) {
          const user = result.items[0];
          // Populate EntityCacheService
          entityCache.populate<UserEntity>('users', [user]);

          const resolved: ResolvedEntity = {
            id: user.id as UUID,
            uniqueId: user.uniqueId || user.id,
            displayName: user.displayName || user.uniqueId || user.id
          };
          this.addToCache(this.userCache, identifier, resolved);
          return resolved;
        }
        return null;
      }

      // Look up by uniqueId
      const result = await Commands.execute<DataListParams, DataListResult<UserEntity>>(
        DATA_COMMANDS.LIST,
        {
          collection: 'users',
          filter: { uniqueId: identifier },
          limit: 1
        }
      );

      if (result.success && result.items?.[0]) {
        const user = result.items[0];
        // Populate EntityCacheService
        entityCache.populate<UserEntity>('users', [user]);

        const resolved: ResolvedEntity = {
          id: user.id as UUID,
          uniqueId: user.uniqueId || user.id,
          displayName: user.displayName || user.uniqueId || user.id
        };
        this.addToCache(this.userCache, identifier, resolved);
        // Also cache by UUID
        this.addToCache(this.userCache, user.id, resolved);
        return resolved;
      }

      console.warn(`🔍 RoutingService: User not found: ${identifier}`);
      return null;
    } catch (error) {
      console.error(`❌ RoutingService: Error resolving user ${identifier}:`, error);
      return null;
    }
  }

  /**
   * Try to resolve user from EntityCacheService (instant, no DB query)
   */
  private resolveUserFromEntityCache(identifier: string): ResolvedEntity | null {
    // Check if it's a UUID - try direct lookup
    if (isUUID(identifier)) {
      const user = entityCache.get<UserEntity>('users', identifier);
      if (user) {
        return {
          id: user.id as UUID,
          uniqueId: user.uniqueId || user.id,
          displayName: user.displayName || user.uniqueId || user.id
        };
      }
    }

    // Try to find by uniqueId in cached users
    const users = entityCache.getAll<UserEntity>('users');
    const user = users.find(u => u.uniqueId === identifier);
    if (user) {
      return {
        id: user.id as UUID,
        uniqueId: user.uniqueId || user.id,
        displayName: user.displayName || user.uniqueId || user.id
      };
    }

    return null;
  }

  /**
   * Resolve any identifier based on content type
   */
  async resolve(contentType: string, identifier: string): Promise<ResolvedEntity | null> {
    switch (contentType) {
      case 'chat':
        return this.resolveRoom(identifier);
      case 'profile':
      case 'persona':
        return this.resolveUser(identifier);
      default:
        // For content types without entities, return a synthetic resolved entity
        return {
          id: identifier as UUID,
          uniqueId: identifier,
          displayName: identifier
        };
    }
  }

  /**
   * Build a URL path for a room (prefers uniqueId for readability)
   */
  buildRoomPath(room: { id: string; uniqueId?: string }): string {
    const identifier = room.uniqueId || room.id;
    return `/chat/${identifier}`;
  }

  /**
   * Build a URL path for a user profile (prefers uniqueId for readability)
   */
  buildProfilePath(user: { id: string; uniqueId?: string }): string {
    const identifier = user.uniqueId || user.id;
    return `/profile/${identifier}`;
  }

  /**
   * Build a URL path for any content type
   */
  buildPath(contentType: string, entity?: { id: string; uniqueId?: string }): string {
    if (!entity) {
      return `/${contentType}`;
    }

    const identifier = entity.uniqueId || entity.id;
    return `/${contentType}/${identifier}`;
  }

  /**
   * Clear all caches (useful for testing or after data changes)
   */
  clearCache(): void {
    this.roomCache.clear();
    this.userCache.clear();
    this.cacheTimestamps.clear();
  }

  /**
   * Get cache stats for debugging
   */
  getCacheStats(): { rooms: number; users: number } {
    return {
      rooms: this.roomCache.size,
      users: this.userCache.size
    };
  }

  // Private cache helpers

  private getFromCache(cache: Map<string, ResolvedEntity>, key: string): ResolvedEntity | null {
    const timestamp = this.cacheTimestamps.get(key);
    if (!timestamp || Date.now() - timestamp > this.CACHE_TTL_MS) {
      cache.delete(key);
      this.cacheTimestamps.delete(key);
      return null;
    }
    return cache.get(key) || null;
  }

  private addToCache(cache: Map<string, ResolvedEntity>, key: string, value: ResolvedEntity): void {
    cache.set(key, value);
    this.cacheTimestamps.set(key, Date.now());
  }
}

// Singleton export
export const RoutingService = new RoutingServiceImpl();

// Export type for external use
export { isUUID };

/**
 * ========================================
 * SERVER-SIDE CONVENIENCE FUNCTIONS
 * ========================================
 * These are the SINGLE SOURCE OF TRUTH for identifier resolution.
 * ALL server commands should use these - NO custom resolution logic!
 */

/**
 * Resolve a room identifier to its canonical UUID, uniqueId, and displayName.
 *
 * THIS IS THE SINGLE SOURCE OF TRUTH for room resolution.
 * DO NOT write custom resolution logic in commands - use this function.
 *
 * @param identifier - Room uniqueId (e.g., "general"), UUID, or name
 * @returns ResolvedEntity with id (UUID), uniqueId, displayName, or null if not found
 *
 * @example
 * const resolved = await resolveRoomIdentifier('general');
 * if (!resolved) return { success: false, error: 'Room not found' };
 * const roomId = resolved.id;  // UUID for database
 * const roomName = resolved.displayName;  // "General" for display
 */
export async function resolveRoomIdentifier(identifier: string): Promise<ResolvedEntity | null> {
  return RoutingService.resolveRoom(identifier);
}

/**
 * Resolve a user identifier to its canonical UUID, uniqueId, and displayName.
 *
 * THIS IS THE SINGLE SOURCE OF TRUTH for user resolution.
 * DO NOT write custom resolution logic in commands - use this function.
 *
 * @param identifier - User uniqueId (e.g., "joel"), UUID, or name
 * @returns ResolvedEntity with id (UUID), uniqueId, displayName, or null if not found
 */
export async function resolveUserIdentifier(identifier: string): Promise<ResolvedEntity | null> {
  return RoutingService.resolveUser(identifier);
}
