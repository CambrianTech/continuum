/**
 * DM Command - Server Implementation
 * Gets or creates a private room for a specific set of participants
 *
 * Set theory: {A, B} == {B, A} - same room regardless of who initiates
 */

import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { DmCommand } from '../shared/DmCommand';
import type { DmParams, DmResult } from '../shared/DmTypes';
import { RoomEntity, type RoomMember } from '@system/data/entities/RoomEntity';
import { UserEntity } from '@system/data/entities/UserEntity';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '@system/core/shared/Commands';
import type { DataListParams, DataListResult } from '@commands/data/list/shared/DataListTypes';
import type { DataCreateParams, DataCreateResult } from '@commands/data/create/shared/DataCreateTypes';
import type { DataUpdateParams, DataUpdateResult } from '@commands/data/update/shared/DataUpdateTypes';
import { UserIdentityResolver } from '@system/user/shared/UserIdentityResolver';
import { RoomResolver } from '@system/core/server/RoomResolver';

export class DmServerCommand extends DmCommand {


  protected async executeDm(params: DmParams): Promise<DmResult> {
    // 1. Get current user (the one initiating the DM)
    const caller = await this.resolveCallerIdentity(params);

    // 2. Normalize participants to array
    const otherParticipants = Array.isArray(params.participants)
      ? params.participants
      : [params.participants];

    // 3. Resolve all participant IDs
    const resolvedParticipants = await this.resolveParticipants(otherParticipants, params);

    // 4. Build full participant set (caller + others)
    const allParticipantIds = [caller.id, ...resolvedParticipants.map(p => p.id)];
    const allParticipantNames = [caller.entity.displayName, ...resolvedParticipants.map(p => p.entity.displayName)];

    // 5. Generate deterministic uniqueId from sorted set
    const uniqueId = this.generateDmUniqueId(allParticipantIds);

    // 6. Try to find existing room (by uniqueId or member set)
    const existingRoom = await this.findExistingDmRoom(uniqueId, allParticipantIds, params);
    if (existingRoom) {
      return transformPayload(params, {
        success: true,
        message: `Found existing DM: ${existingRoom.displayName}`,
        room: existingRoom,
        roomId: existingRoom.id,
        existed: true,
        uniqueId,
        participantIds: allParticipantIds
      });
    }

    // 7. Create new DM room
    const newRoom = await this.createDmRoom(
      uniqueId,
      allParticipantIds,
      allParticipantNames,
      caller.id,
      params.name,
      params
    );

    // 8. Invalidate room cache so new room is discoverable
    RoomResolver.invalidateCache();

    return transformPayload(params, {
      success: true,
      message: `Created DM: ${newRoom.displayName}`,
      room: newRoom,
      roomId: newRoom.id,
      existed: false,
      uniqueId,
      participantIds: allParticipantIds
    });
  }

  /**
   * Resolve caller identity (who's initiating the DM)
   *
   * Priority:
   * 1. params.callerId - Persona tool execution context
   * 2. params.personaId - Alternative persona context
   * 3. UserIdentityResolver - Human/CLI context fallback
   */
  private async resolveCallerIdentity(params: DmParams): Promise<{ id: UUID; entity: UserEntity }> {
    // Priority 1: Use callerId from persona tool context
    const callerIdFromParams = (params as any).callerId || (params as any).personaId;

    if (callerIdFromParams) {
      const result = await Commands.execute<DataListParams, DataListResult<UserEntity>>(
        DATA_COMMANDS.LIST,
        {
          collection: UserEntity.collection,
          filter: { id: callerIdFromParams },
          limit: 1,
          context: params.context,
          sessionId: params.sessionId
        }
      );

      if (result.success && result.items && result.items.length > 0) {
        const user = result.items[0];
        return { id: user.id, entity: user };
      }
    }

    // Priority 2: Fall back to UserIdentityResolver (human/CLI context)
    const identity = await UserIdentityResolver.resolve();

    if (identity.exists && identity.userId) {
      const result = await Commands.execute<DataListParams, DataListResult<UserEntity>>(
        DATA_COMMANDS.LIST,
        {
          collection: UserEntity.collection,
          filter: { id: identity.userId },
          limit: 1,
          context: params.context,
          sessionId: params.sessionId
        }
      );

      if (result.success && result.items && result.items.length > 0) {
        const user = result.items[0];
        return { id: user.id, entity: user };
      }
    }

    throw new Error(`Could not resolve caller identity: ${identity.displayName}`);
  }

  /**
   * Resolve participant IDs from IDs or uniqueIds
   */
  private async resolveParticipants(
    participantRefs: string[],
    params: DmParams
  ): Promise<Array<{ id: UUID; entity: UserEntity }>> {
    const resolved: Array<{ id: UUID; entity: UserEntity }> = [];

    for (const ref of participantRefs) {
      // Try by ID first
      let result = await Commands.execute<DataListParams, DataListResult<UserEntity>>(
        DATA_COMMANDS.LIST,
        {
          collection: UserEntity.collection,
          filter: { id: ref },
          limit: 1,
          context: params.context,
          sessionId: params.sessionId
        }
      );

      if (result.success && result.items && result.items.length > 0) {
        resolved.push({ id: result.items[0].id, entity: result.items[0] });
        continue;
      }

      // Try by uniqueId
      result = await Commands.execute<DataListParams, DataListResult<UserEntity>>(
        DATA_COMMANDS.LIST,
        {
          collection: UserEntity.collection,
          filter: { uniqueId: ref },
          limit: 1,
          context: params.context,
          sessionId: params.sessionId
        }
      );

      if (result.success && result.items && result.items.length > 0) {
        resolved.push({ id: result.items[0].id, entity: result.items[0] });
        continue;
      }

      throw new Error(`Participant not found: ${ref}`);
    }

    return resolved;
  }

  /**
   * Find existing DM room by uniqueId OR by exact member set
   *
   * Two-phase lookup:
   * 1. Try uniqueId (fast, deterministic)
   * 2. Fall back to member matching (handles UUID changes after reseed)
   */
  private async findExistingDmRoom(
    uniqueId: string,
    participantIds: UUID[],
    params: DmParams
  ): Promise<RoomEntity | null> {
    // Phase 1: Try by uniqueId (fast path)
    const byUniqueId = await Commands.execute<DataListParams, DataListResult<RoomEntity>>(
      DATA_COMMANDS.LIST,
      {
        collection: RoomEntity.collection,
        filter: { uniqueId },
        limit: 1,
        context: params.context,
        sessionId: params.sessionId
      }
    );

    if (byUniqueId.success && byUniqueId.items && byUniqueId.items.length > 0) {
      return byUniqueId.items[0];
    }

    // Phase 2: Search direct/private rooms and match by member set
    // This handles cases where user UUIDs changed (e.g., after reseed)
    const directRooms = await Commands.execute<DataListParams, DataListResult<RoomEntity>>(
      DATA_COMMANDS.LIST,
      {
        collection: RoomEntity.collection,
        filter: { type: participantIds.length === 2 ? 'direct' : 'private' },
        limit: 100,
        context: params.context,
        sessionId: params.sessionId
      }
    );

    if (directRooms.success && directRooms.items) {
      const sortedParticipants = [...participantIds].sort();

      for (const room of directRooms.items) {
        if (!room.members || room.members.length !== participantIds.length) continue;

        const roomMemberIds = room.members.map(m => m.userId).sort();
        const isMatch = sortedParticipants.every((id, i) => id === roomMemberIds[i]);

        if (isMatch) {
          // Found matching room - update its uniqueId to current format for future lookups
          await Commands.execute<DataUpdateParams, DataUpdateResult>(DATA_COMMANDS.UPDATE, {
            collection: RoomEntity.collection,
            id: room.id,
            data: { uniqueId },
            context: params.context,
            sessionId: params.sessionId
          });
          return room;
        }
      }
    }

    return null;
  }

  /**
   * Create a new DM room
   */
  private async createDmRoom(
    uniqueId: string,
    participantIds: UUID[],
    participantNames: string[],
    ownerId: UUID,
    customName: string | undefined,
    params: DmParams
  ): Promise<RoomEntity> {
    const now = new Date();

    const room = new RoomEntity();
    room.uniqueId = uniqueId;
    room.name = uniqueId; // System name (immutable)
    room.displayName = customName || this.generateDefaultDisplayName(participantNames);
    room.description = `Private conversation`;
    room.type = participantIds.length === 2 ? 'direct' : 'private';
    room.status = 'active';
    room.ownerId = ownerId;
    room.lastMessageAt = now;
    room.recipeId = 'dm';
    room.privacy = {
      isPublic: false,
      requiresInvite: true,
      allowGuestAccess: false,
      searchable: false
    };
    room.settings = {
      allowThreads: true,
      allowReactions: true,
      allowFileSharing: true,
      messageRetentionDays: 365,
      slowMode: 0
    };

    // Add all participants as members
    const members: RoomMember[] = participantIds.map((userId, index) => ({
      userId,
      role: index === 0 ? 'owner' : 'member' as const,
      joinedAt: now
    }));
    room.members = members;

    room.tags = ['dm', 'private'];

    // Store using data/create
    const createResult = await Commands.execute<DataCreateParams, DataCreateResult<RoomEntity>>(
      DATA_COMMANDS.CREATE,
      {
        collection: RoomEntity.collection,
        data: room,
        context: params.context,
        sessionId: params.sessionId
      }
    );

    if (!createResult.success || !createResult.data) {
      throw new Error(`Failed to create DM room: ${createResult.error || 'Unknown error'}`);
    }

    return createResult.data;
  }
}
