/**
 * Live Leave Command - Server Implementation
 *
 * Removes user from call. Ends call if last participant.
 */

import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { LiveLeaveCommand } from '../shared/LiveLeaveCommand';
import type { LiveLeaveParams, LiveLeaveResult } from '../shared/LiveLeaveTypes';
import { CallEntity } from '@system/data/entities/CallEntity';
import { UserEntity } from '@system/data/entities/UserEntity';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '@system/core/shared/Commands';
import { Events } from '@system/core/shared/Events';
import type { DataListParams, DataListResult } from '@commands/data/list/shared/DataListTypes';
import type { DataUpdateParams, DataUpdateResult } from '@commands/data/update/shared/DataUpdateTypes';
import { UserIdentityResolver } from '@system/user/shared/UserIdentityResolver';

export class LiveLeaveServerCommand extends LiveLeaveCommand {

  protected async executeLeave(params: LiveLeaveParams): Promise<LiveLeaveResult> {
    // 1. Get current user
    const user = await this.resolveCurrentUser(params);
    if (!user) {
      return transformPayload(params, {
        success: false,
        message: 'Could not identify current user',
        sessionEnded: false,
        remainingParticipants: 0
      });
    }

    // 2. Find the call
    const call = await this.findCall(params.sessionId, params);
    if (!call) {
      return transformPayload(params, {
        success: false,
        message: `Call not found: ${params.sessionId}`,
        sessionEnded: false,
        remainingParticipants: 0
      });
    }

    // 3. Remove user from participants
    const removed = call.removeParticipant(user.id);
    if (!removed) {
      return transformPayload(params, {
        success: false,
        message: 'User was not in call',
        sessionEnded: false,
        remainingParticipants: call.getActiveParticipants().length
      });
    }

    // 4. Save updated call
    await this.saveCall(call, params);

    // 5. Emit leave event for other clients
    Events.emit(`live:left:${call.id}`, {
      sessionId: call.id,
      userId: user.id
    });

    const remainingParticipants = call.getActiveParticipants().length;
    const callEnded = call.status === 'ended';

    return transformPayload(params, {
      success: true,
      message: callEnded
        ? 'Left call (call ended - no participants remaining)'
        : `Left call (${remainingParticipants} remaining)`,
      sessionEnded: callEnded,
      remainingParticipants
    });
  }

  /**
   * Resolve current user
   */
  private async resolveCurrentUser(params: LiveLeaveParams): Promise<UserEntity | null> {
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
   * Find call by ID
   */
  private async findCall(callId: string, params: LiveLeaveParams): Promise<CallEntity | null> {
    const result = await Commands.execute<DataListParams, DataListResult<CallEntity>>(
      DATA_COMMANDS.LIST,
      {
        collection: CallEntity.collection,
        filter: { id: callId },
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
   * Save updated call
   */
  private async saveCall(call: CallEntity, params: LiveLeaveParams): Promise<void> {
    await Commands.execute<DataUpdateParams, DataUpdateResult<CallEntity>>(
      DATA_COMMANDS.UPDATE,
      {
        collection: CallEntity.collection,
        id: call.id,
        data: {
          participants: call.participants,
          status: call.status,
          endedAt: call.endedAt
        },
        context: params.context,
        sessionId: params.sessionId
      }
    );
  }
}
