/**
 * RoomResolver - Centralized room resolution for tools and commands
 *
 * Resolves room parameters:
 * - "current" -> actual room ID/name from context
 * - room name -> room ID
 * - room ID -> room name
 *
 * Single source of truth for room lookups used by:
 * - PersonaToolExecutor (tool parameter resolution)
 * - Wall commands (room-scoped documents)
 * - Chat commands (room filtering)
 * - PersonaResponseGenerator (context injection)
 */

import type { UUID } from '../types/CrossPlatformUUID';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { Commands } from '../shared/Commands';
import { COLLECTIONS } from '../../shared/Constants';
import type { RoomEntity } from '../../data/entities/RoomEntity';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';

/**
 * Room info returned from resolver
 */
export interface ResolvedRoom {
  id: UUID;
  name: string;
  entity: RoomEntity;
}

/**
 * Room cache for performance (rooms rarely change)
 */
let roomCache: Map<UUID, RoomEntity> | null = null;
let cacheTimestamp = 0;
const CACHE_TTL_MS = 60000; // 1 minute cache

/**
 * RoomResolver - Static utility for room resolution
 */
export class RoomResolver {

  /**
   * Resolve a room parameter that may be:
   * - "current" - resolves to the contextId (roomId) from tool execution context
   * - Room name (e.g., "general") - looks up room ID by name
   * - Room UUID - validates and returns room info
   *
   * @param roomParam - The room parameter value from tool call
   * @param contextId - The contextId (roomId) from tool execution context
   * @returns Resolved room info, or null if not found
   */
  static async resolve(
    roomParam: string | undefined,
    contextId: UUID
  ): Promise<ResolvedRoom | null> {
    // If no room param, use contextId as default
    if (!roomParam || roomParam === 'current') {
      return this.resolveById(contextId);
    }

    // Try as UUID first
    const byId = await this.resolveById(roomParam as UUID);
    if (byId) {
      return byId;
    }

    // Try as room name
    return this.resolveByName(roomParam);
  }

  /**
   * Resolve room by UUID
   */
  static async resolveById(roomId: UUID): Promise<ResolvedRoom | null> {
    const rooms = await this.getAllRooms();
    const room = rooms.get(roomId);

    if (!room) {
      return null;
    }

    return {
      id: roomId,
      name: room.name,
      entity: room
    };
  }

  /**
   * Resolve room by name
   */
  static async resolveByName(roomName: string): Promise<ResolvedRoom | null> {
    const rooms = await this.getAllRooms();

    for (const [id, room] of rooms.entries()) {
      if (room.name === roomName) {
        return {
          id,
          name: room.name,
          entity: room
        };
      }
    }

    return null;
  }

  /**
   * Get room name from ID (convenience method)
   */
  static async getRoomName(roomId: UUID): Promise<string | null> {
    const resolved = await this.resolveById(roomId);
    return resolved?.name ?? null;
  }

  /**
   * Resolve "current" parameter in tool params
   * Returns the resolved room name for the parameter
   *
   * @param paramValue - The parameter value (may be "current")
   * @param contextId - The contextId (roomId) from context
   * @returns The resolved room name, or original value if not "current"
   */
  static async resolveCurrentParam(
    paramValue: string | undefined,
    contextId: UUID
  ): Promise<string | undefined> {
    if (!paramValue || paramValue !== 'current') {
      return paramValue;
    }

    const room = await this.resolveById(contextId);
    return room?.name;
  }

  /**
   * Get all rooms (with caching)
   */
  private static async getAllRooms(): Promise<Map<UUID, RoomEntity>> {
    const now = Date.now();

    // Return cached if still valid
    if (roomCache && (now - cacheTimestamp) < CACHE_TTL_MS) {
      return roomCache;
    }

    // Fetch from database
    const result = await Commands.execute<DataListParams, DataListResult<RoomEntity>>(
      DATA_COMMANDS.LIST,
      {
        collection: COLLECTIONS.ROOMS,
        filter: {}
      }
    );

    // Build cache
    roomCache = new Map();
    if (result.success && result.items) {
      for (const room of result.items) {
        if (room.id) {
          roomCache.set(room.id, room);
        }
      }
    }
    cacheTimestamp = now;

    return roomCache;
  }

  /**
   * Invalidate cache (call when rooms are created/deleted)
   */
  static invalidateCache(): void {
    roomCache = null;
    cacheTimestamp = 0;
  }

  /**
   * List all available room names (for error messages)
   */
  static async listRoomNames(): Promise<string[]> {
    const rooms = await this.getAllRooms();
    return Array.from(rooms.values()).map(r => r.name);
  }
}
