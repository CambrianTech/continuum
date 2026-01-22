/**
 * Live Join Command - Server Implementation
 *
 * Joins or creates a live call for a room.
 * Creates call if none exists, adds user as participant.
 */

import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { LiveJoinCommand } from '../shared/LiveJoinCommand';
import type { LiveJoinParams, LiveJoinResult } from '../shared/LiveJoinTypes';
import { CallEntity } from '@system/data/entities/CallEntity';
import { UserEntity } from '@system/data/entities/UserEntity';
import { RoomEntity } from '@system/data/entities/RoomEntity';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '@system/core/shared/Commands';
import { Events } from '@system/core/shared/Events';
import type { DataListParams, DataListResult } from '@commands/data/list/shared/DataListTypes';
import type { DataCreateParams, DataCreateResult } from '@commands/data/create/shared/DataCreateTypes';
import type { DataUpdateParams, DataUpdateResult } from '@commands/data/update/shared/DataUpdateTypes';
import { UserIdentityResolver } from '@system/user/shared/UserIdentityResolver';

export class LiveJoinServerCommand extends LiveJoinCommand {

  protected async executeJoin(params: LiveJoinParams): Promise<LiveJoinResult> {
    // 1. Resolve the entity (room/activity)
    const room = await this.resolveRoom(params.entityId, params);
    if (!room) {
      return transformPayload(params, {
        success: false,
        message: `Entity not found: ${params.entityId}`,
        session: null as any,
        sessionId: '' as UUID,
        existed: false,
        participants: [],
        myParticipant: null as any
      });
    }

    // 2. Get current user
    const user = await this.resolveCurrentUser(params);
    if (!user) {
      return transformPayload(params, {
        success: false,
        message: 'Could not identify current user',
        session: null as any,
        sessionId: '' as UUID,
        existed: false,
        participants: [],
        myParticipant: null as any
      });
    }

    // 3. Find or create active call for this room
    let call = await this.findActiveCall(room.id, params);
    let existed = true;

    if (!call) {
      call = await this.createCall(room.id, params);
      existed = false;
    }

    // 4. Add user as participant (if not already)
    const myParticipant = call.addParticipant(
      user.id,
      user.displayName,
      user.avatar
    );

    // 5. Save updated call
    await this.saveCall(call, params);

    // 6. Emit join event for other clients
    Events.emit(`live:joined:${call.id}`, {
      sessionId: call.id,
      participant: myParticipant
    });

    return transformPayload(params, {
      success: true,
      message: existed
        ? `Joined existing live call`
        : `Created and joined new live call`,
      session: call,
      sessionId: call.id,
      existed,
      participants: call.getActiveParticipants(),
      myParticipant
    });
  }

  /**
   * Resolve room by ID or uniqueId
   */
  private async resolveRoom(roomRef: string, params: LiveJoinParams): Promise<RoomEntity | null> {
    // Try by ID first
    let result = await Commands.execute<DataListParams, DataListResult<RoomEntity>>(
      DATA_COMMANDS.LIST,
      {
        collection: RoomEntity.collection,
        filter: { id: roomRef },
        limit: 1,
        context: params.context,
        sessionId: params.sessionId
      }
    );

    if (result.success && result.items && result.items.length > 0) {
      return result.items[0];
    }

    // Try by uniqueId
    result = await Commands.execute<DataListParams, DataListResult<RoomEntity>>(
      DATA_COMMANDS.LIST,
      {
        collection: RoomEntity.collection,
        filter: { uniqueId: roomRef },
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
   * Resolve current user
   */
  private async resolveCurrentUser(params: LiveJoinParams): Promise<UserEntity | null> {
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
        return result.items[0];
      }
    }

    // Fall back to UserIdentityResolver
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
        return result.items[0];
      }
    }

    return null;
  }

  /**
   * Find active call for room
   */
  private async findActiveCall(roomId: UUID, params: LiveJoinParams): Promise<CallEntity | null> {
    const result = await Commands.execute<DataListParams, DataListResult<CallEntity>>(
      DATA_COMMANDS.LIST,
      {
        collection: CallEntity.collection,
        filter: { roomId, status: 'active' },
        limit: 1,
        context: params.context,
        sessionId: params.sessionId
      }
    );

    if (result.success && result.items && result.items.length > 0) {
      // Reconstruct entity with methods
      const data = result.items[0];
      const call = new CallEntity();
      Object.assign(call, data);
      return call;
    }

    return null;
  }

  /**
   * Create new call
   */
  private async createCall(roomId: UUID, params: LiveJoinParams): Promise<CallEntity> {
    const call = new CallEntity();
    call.roomId = roomId;
    call.status = 'active';
    call.participants = [];
    call.peakParticipants = 0;
    call.totalParticipants = 0;

    const createResult = await Commands.execute<DataCreateParams, DataCreateResult<CallEntity>>(
      DATA_COMMANDS.CREATE,
      {
        collection: CallEntity.collection,
        data: call,
        context: params.context,
        sessionId: params.sessionId
      }
    );

    if (!createResult.success || !createResult.data) {
      throw new Error(`Failed to create call: ${createResult.error || 'Unknown error'}`);
    }

    // Return the created entity with methods attached
    const created = new CallEntity();
    Object.assign(created, createResult.data);
    return created;
  }

  /**
   * Save updated call
   */
  private async saveCall(call: CallEntity, params: LiveJoinParams): Promise<void> {
    await Commands.execute<DataUpdateParams, DataUpdateResult<CallEntity>>(
      DATA_COMMANDS.UPDATE,
      {
        collection: CallEntity.collection,
        id: call.id,
        data: {
          participants: call.participants,
          peakParticipants: call.peakParticipants,
          totalParticipants: call.totalParticipants,
          status: call.status,
          endedAt: call.endedAt
        },
        context: params.context,
        sessionId: params.sessionId
      }
    );
  }
}
