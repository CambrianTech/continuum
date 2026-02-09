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
import { getVoiceOrchestrator } from '@system/voice/server/VoiceOrchestrator';

import { DataList } from '../../../../data/list/shared/DataListTypes';
import { DataCreate } from '../../../../data/create/shared/DataCreateTypes';
import { DataUpdate } from '../../../../data/update/shared/DataUpdateTypes';
export class LiveJoinServerCommand extends LiveJoinCommand {

  protected async executeJoin(params: LiveJoinParams): Promise<LiveJoinResult> {
    // 1. Resolve the entity (room/activity)
    const room = await this.resolveRoom(params.entityId, params);
    if (!room) {
      return transformPayload(params, {
        success: false,
        message: `Entity not found: ${params.entityId}`,
        session: null as any,
        callId: '' as UUID,
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
        callId: '' as UUID,
        existed: false,
        participants: [],
        myParticipant: null as any
      });
    }

    // 3. Find or create active call for this room (with retry logic for race conditions)
    const { call, existed } = await this.findOrCreateCall(room, params);

    // 4. Add current user as participant (if not already in the call)
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

    // 7. Register with VoiceOrchestrator so AI participants can join the audio call
    // This connects AI personas to the streaming-core call server
    const allParticipantIds = call.getActiveParticipants().map(p => p.userId);
    try {
      await getVoiceOrchestrator().registerSession(call.id, room.id, allParticipantIds);
    } catch (error) {
      console.warn('Failed to register voice session:', error);
    }

    const result = {
      success: true,
      message: existed
        ? `Joined existing live call`
        : `Created and joined new live call`,
      session: call,
      callId: call.id,
      existed,
      participants: call.getActiveParticipants(),
      myParticipant
    };

    // DEBUG: Log what we're returning to browser
    console.error(`🎙️ LiveJoin RESULT: callId=${call.id.slice(0, 8)}, existed=${existed}, participants=${call.getActiveParticipants().length}, myParticipant=${myParticipant.displayName}`);

    return transformPayload(params, result);
  }

  /**
   * Resolve room by ID or uniqueId
   */
  private async resolveRoom(roomRef: string, params: LiveJoinParams): Promise<RoomEntity | null> {
    // Try by ID first
    let result = await DataList.execute<RoomEntity>({
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
    result = await DataList.execute<RoomEntity>({
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
   * Resolve current user - prefers context.userId (for PersonaUsers)
   *
   * Priority:
   * 1. params.context?.userId - When a PersonaUser executes a command, their ID is in context
   * 2. Legacy callerId/personaId - Deprecated, for backwards compatibility
   * 3. UserIdentityResolver - Fallback for CLI calls
   */
  private async resolveCurrentUser(params: LiveJoinParams): Promise<UserEntity | null> {
    // FIRST: Check context.userId (PersonaUsers set this)
    if (params.context?.userId) {
      const result = await DataList.execute<UserEntity>({
        collection: UserEntity.collection,
        filter: { id: params.context.userId },
        limit: 1,
        context: params.context,
        sessionId: params.sessionId
      });

      if (result.success && result.items && result.items.length > 0) {
        console.log('🔧 LiveJoinServerCommand.resolveCurrentUser USING CONTEXT userId', { userId: params.context.userId });
        return result.items[0];
      }
    }

    // SECOND: Check legacy callerId/personaId (deprecated)
    const callerIdFromParams = (params as any).callerId || (params as any).personaId;

    if (callerIdFromParams) {
      const result = await DataList.execute<UserEntity>({
        collection: UserEntity.collection,
        filter: { id: callerIdFromParams },
        limit: 1,
        context: params.context,
        sessionId: params.sessionId
      });

      if (result.success && result.items && result.items.length > 0) {
        return result.items[0];
      }
    }

    // FALLBACK: Use UserIdentityResolver (CLI calls)
    const identity = await UserIdentityResolver.resolve();

    if (identity.exists && identity.userId) {
      const result = await DataList.execute<UserEntity>({
        collection: UserEntity.collection,
        filter: { id: identity.userId },
        limit: 1,
        context: params.context,
        sessionId: params.sessionId
      });

      if (result.success && result.items && result.items.length > 0) {
        return result.items[0];
      }
    }

    return null;
  }

  /**
   * Atomically find or create active call for room
   * Handles race conditions when multiple participants join simultaneously
   */
  private async findOrCreateCall(room: RoomEntity, params: LiveJoinParams): Promise<{ call: CallEntity; existed: boolean }> {
    const maxRetries = 5;
    let lastError: Error | null = null;

    for (let attempt = 0; attempt < maxRetries; attempt++) {
      // Try to find existing call
      let call = await this.findActiveCall(room.id, params);
      if (call) {
        return { call, existed: true };
      }

      // No call found, try to create one
      try {
        call = await this.createCall(room.id, params);

        // NEW CALL: Add ALL room members as participants
        // This ensures AI personas are in the call from the start
        if (room.members && room.members.length > 0) {
          const memberIds = room.members.map(m => m.userId);
          const membersInfo = await this.lookupUsers(memberIds, params);

          for (const memberUser of membersInfo) {
            call.addParticipant(
              memberUser.id as UUID,
              memberUser.displayName || memberUser.uniqueId,
              memberUser.avatar
            );
          }
          console.log(`🎙️ LiveJoin: Added ${membersInfo.length} room members to new call ${call.id.slice(0, 8)}`);
        }

        return { call, existed: false };
      } catch (error) {
        lastError = error as Error;
        console.warn(`🎙️ LiveJoin: Create call failed (attempt ${attempt + 1}/${maxRetries}), retrying...`, error);

        // Exponential backoff: 100ms, 200ms, 400ms, 800ms, 1600ms
        const backoffMs = 100 * Math.pow(2, attempt);
        await new Promise(resolve => setTimeout(resolve, backoffMs));

        // Retry: another participant may have created the call while we were creating
        call = await this.findActiveCall(room.id, params);
        if (call) {
          console.log(`🎙️ LiveJoin: Found call created by another participant: ${call.id.slice(0, 8)}`);
          return { call, existed: true };
        }
      }
    }

    // All retries exhausted
    throw new Error(`Failed to find or create call after ${maxRetries} attempts: ${lastError?.message || 'Unknown error'}`);
  }

  /**
   * Find active call for room
   */
  private async findActiveCall(roomId: UUID, params: LiveJoinParams): Promise<CallEntity | null> {
    const result = await DataList.execute<CallEntity>({
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

    const createResult = await DataCreate.execute<CallEntity>({
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
    await DataUpdate.execute<CallEntity>({
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

  /**
   * Look up user info for a list of user IDs
   */
  private async lookupUsers(userIds: UUID[], params: LiveJoinParams): Promise<readonly UserEntity[]> {
    if (userIds.length === 0) return [];

    const result = await DataList.execute<UserEntity>({
        collection: UserEntity.collection,
        filter: { id: { $in: userIds } },
        limit: userIds.length,
        context: params.context,
        sessionId: params.sessionId
      }
    );

    return result.success && result.items ? result.items : [];
  }
}
