/**
 * Activity Join Server Command
 */

import { CommandBase, type ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type { ActivityJoinParams, ActivityJoinResult } from '../shared/ActivityJoinTypes';
import { Commands } from '@system/core/shared/Commands';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { DataUpdateParams, DataUpdateResult } from '@commands/data/update/shared/DataUpdateTypes';
import { Events } from '@system/core/shared/Events';
import type { ActivityParticipant } from '@system/data/entities/ActivityEntity';
import type { ActivityGetResult } from '../../get/shared/ActivityGetTypes';

import { ActivityGet } from '../../get/shared/ActivityGetTypes';
import { DataUpdate } from '../../../../data/update/shared/DataUpdateTypes';
export class ActivityJoinServerCommand extends CommandBase<ActivityJoinParams, ActivityJoinResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/activity/join', context, subpath, commander);
  }

  async execute(payload: JTAGPayload): Promise<ActivityJoinResult> {
    const params = payload as ActivityJoinParams;
    const { activityId, role = 'participant', roleConfig } = params;

    if (!activityId) {
      return transformPayload(params, {
        success: false,
        error: 'activityId is required — provide the activity ID or uniqueId to join'
      });
    }

    // Get user ID from params or context
    const userId = params.userId || params.context?.userId;
    if (!userId) {
      return transformPayload(params, {
        success: false,
        error: 'User ID is required (provide userId or ensure context has userId)'
      });
    }

    // Detect if activityId is UUID or uniqueId
    const isUUID = typeof activityId === 'string' && activityId.includes('-') && activityId.length === 36;

    // Get the activity
    const getResult = await ActivityGet.execute({
      id: isUUID ? activityId : undefined,
      uniqueId: isUUID ? undefined : activityId,
      context: params.context,
      sessionId: params.sessionId
    }) as unknown as ActivityGetResult;

    if (!getResult.success || !getResult.activity) {
      return transformPayload(params, {
        success: false,
        error: `Activity not found: ${activityId}`
      });
    }

    const activity = getResult.activity;

    // Check if already a participant
    const existing = activity.participants?.find(p => p.userId === userId && p.isActive);
    if (existing) {
      return transformPayload(params, {
        success: true,
        activityId: activity.id,
        participant: existing
      });
    }

    // Create new participant
    const newParticipant: ActivityParticipant = {
      userId,
      role,
      joinedAt: new Date(),
      isActive: true,
      roleConfig
    };

    // Update activity with new participant
    const updateResult = await DataUpdate.execute({
      collection: 'activities',
      id: activity.id,
      data: {
        participants: [...(activity.participants || []), newParticipant],
        lastActivityAt: new Date()
      },
      context: params.context,
      sessionId: params.sessionId
    });

    if (!updateResult.success) {
      return transformPayload(params, {
        success: false,
        error: updateResult.error || 'Failed to add participant'
      });
    }

    // Emit participant joined event
    Events.emit('activity:participant:joined', {
      activityId: activity.id,
      participant: newParticipant
    });

    return transformPayload(params, {
      success: true,
      activityId: activity.id,
      participant: newParticipant
    });
  }
}
