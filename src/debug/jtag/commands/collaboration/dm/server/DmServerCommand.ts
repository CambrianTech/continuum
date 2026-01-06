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

    // 6. Try to find existing room
    const existingRoom = await this.findRoomByUniqueId(uniqueId, params);
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
   */
  private async resolveCallerIdentity(params: DmParams): Promise<{ id: UUID; entity: UserEntity }> {
    const identity = await UserIdentityResolver.resolve();

    if (identity.exists && identity.userId) {
      const result = await Commands.execute<DataListParams<UserEntity>, DataListResult<UserEntity>>(
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
      let result = await Commands.execute<DataListParams<UserEntity>, DataListResult<UserEntity>>(
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
      result = await Commands.execute<DataListParams<UserEntity>, DataListResult<UserEntity>>(
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
   * Find existing room by uniqueId
   */
  private async findRoomByUniqueId(uniqueId: string, params: DmParams): Promise<RoomEntity | null> {
    const result = await Commands.execute<DataListParams<RoomEntity>, DataListResult<RoomEntity>>(
      DATA_COMMANDS.LIST,
      {
        collection: RoomEntity.collection,
        filter: { uniqueId },
        limit: 1,
        context: params.context,
        sessionId: params.sessionId
      }
    );

    if (result.success && result.items && result.items.length > 0) {
      return result.items[0];
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
    const createResult = await Commands.execute<DataCreateParams<RoomEntity>, DataCreateResult<RoomEntity>>(
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
