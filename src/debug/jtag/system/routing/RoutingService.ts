/**
 * RoutingService - Single source of truth for URL ↔ Entity resolution
 *
 * Resolves human-readable uniqueIds (e.g., "general") to UUIDs for database operations,
 * and builds human-readable URLs from entities.
 *
 * Key principle: URLs use uniqueId for readability, database uses UUID.
 *   /chat/general → UUID lookup → 5e71a0c8-0303-4eb8-a478-3a121248
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
   */
  async resolveRoom(identifier: string): Promise<ResolvedEntity | null> {
    // Check cache first
    const cached = this.getFromCache(this.roomCache, identifier);
    if (cached) return cached;

    try {
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

      console.warn(`🔍 RoutingService: Room not found: ${identifier}`);
      return null;
    } catch (error) {
      console.error(`❌ RoutingService: Error resolving room ${identifier}:`, error);
      return null;
    }
  }

  /**
   * Resolve a user identifier (uniqueId or UUID) to its UUID
   * Returns null if not found
   */
  async resolveUser(identifier: string): Promise<ResolvedEntity | null> {
    // Check cache first
    const cached = this.getFromCache(this.userCache, identifier);
    if (cached) return cached;

    try {
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
