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
import { getTSVoiceOrchestrator } from '@system/voice/server';

import { DataList } from '../../../../data/list/shared/DataListTypes';
import { DataUpdate } from '../../../../data/update/shared/DataUpdateTypes';
export class LiveLeaveServerCommand extends LiveLeaveCommand {

  protected async executeLeave(params: LiveLeaveParams): Promise<LiveLeaveResult> {
    // 1. Get current user from params.userId (auto-injected by infrastructure)
    const user = await this.findUserById(params.userId, params);
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

    // 4. If a human left and no humans remain, end the call entirely.
    // AIs don't leave on their own — without this, avatar renderers and
    // LiveKit agents persist indefinitely, leaking GPU/memory.
    if (user.type === 'human') {
      const activeParticipants = call.getActiveParticipants();
      const humansRemaining = await this.countHumansInCall(activeParticipants, params);
      if (humansRemaining === 0 && activeParticipants.length > 0) {
        console.log(`🧹 Last human left call ${call.id} — ending call for ${activeParticipants.length} remaining AI participants`);
        for (const p of activeParticipants) {
          call.removeParticipant(p.userId);
        }
      }
    }

    // 5. Save updated call
    await this.saveCall(call, params);

    // 6. Emit leave event for other clients
    Events.emit(`live:left:${call.id}`, {
      sessionId: call.id,
      userId: user.id
    });

    const remainingParticipants = call.getActiveParticipants().length;
    const callEnded = call.status === 'ended';

    // 7. Unregister voice session if call ended
    if (callEnded) {
      try {
        getTSVoiceOrchestrator().unregisterSession(call.id);
      } catch (error) {
        console.warn('Failed to unregister voice session:', error);
      }
    }

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
   * Find user by ID from database
   */
  private async findUserById(userId: UUID, params: LiveLeaveParams): Promise<UserEntity | null> {
    const result = await DataList.execute<UserEntity>({
      dbHandle: 'default',
      collection: UserEntity.collection,
      filter: { id: userId },
      limit: 1,
      context: params.context,
      sessionId: params.sessionId
    });

    if (result.success && result.items && result.items.length > 0) {
      return result.items[0];
    }

    return null;
  }

  /**
   * Find call by ID
   */
  private async findCall(callId: string, params: LiveLeaveParams): Promise<CallEntity | null> {
    const result = await DataList.execute<CallEntity>({
        dbHandle: 'default',
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
   * Count how many active participants are human (not AI)
   */
  private async countHumansInCall(participants: { userId: UUID }[], params: LiveLeaveParams): Promise<number> {
    if (participants.length === 0) return 0;

    const userIds = participants.map(p => p.userId);
    const result = await DataList.execute<UserEntity>({
      dbHandle: 'default',
      collection: UserEntity.collection,
      filter: { id: { $in: userIds } },
      limit: userIds.length,
      context: params.context,
      sessionId: params.sessionId
    });

    if (!result.success || !result.items) return 0;
    return result.items.filter(u => u.type === 'human').length;
  }

  /**
   * Save updated call
   */
  private async saveCall(call: CallEntity, params: LiveLeaveParams): Promise<void> {
    await DataUpdate.execute<CallEntity>({
        dbHandle: 'default',
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
