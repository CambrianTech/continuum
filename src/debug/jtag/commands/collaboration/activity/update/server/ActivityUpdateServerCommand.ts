/**
 * Activity Update Server Command
 */

import { CommandBase, type ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type { ActivityUpdateParams, ActivityUpdateResult } from '../shared/ActivityUpdateTypes';
import { Commands } from '@system/core/shared/Commands';
import { Events } from '@system/core/shared/Events';
import type { ActivityEntity } from '@system/data/entities/ActivityEntity';
import type { ActivityGetResult } from '../../get/shared/ActivityGetTypes';

export class ActivityUpdateServerCommand extends CommandBase<ActivityUpdateParams, ActivityUpdateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/activity/update', context, subpath, commander);
  }

  async execute(payload: JTAGPayload): Promise<ActivityUpdateResult> {
    const params = payload as ActivityUpdateParams;
    const { activityId, displayName, description, status, phase, progress, variables, settings, tags } = params;

    // Detect if activityId is UUID or uniqueId
    const isUUID = typeof activityId === 'string' && activityId.includes('-') && activityId.length === 36;

    // Get the activity
    const getResult = await Commands.execute('collaboration/activity/get', {
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
    const changedFields: string[] = [];
    const updates: Partial<ActivityEntity> = {};

    // Track basic field changes
    if (displayName !== undefined && displayName !== activity.displayName) {
      updates.displayName = displayName;
      changedFields.push('displayName');
    }
    if (description !== undefined && description !== activity.description) {
      updates.description = description;
      changedFields.push('description');
    }
    if (status !== undefined && status !== activity.status) {
      updates.status = status;
      changedFields.push('status');
      if (status === 'completed' || status === 'archived') {
        updates.endedAt = new Date();
      }
    }
    if (tags !== undefined) {
      updates.tags = tags;
      changedFields.push('tags');
    }

    // Handle state updates
    const previousPhase = activity.state?.phase;
    if (phase !== undefined || progress !== undefined || variables) {
      updates.state = {
        ...activity.state,
        ...(phase !== undefined && { phase }),
        ...(progress !== undefined && { progress }),
        ...(variables && {
          variables: { ...(activity.state?.variables || {}), ...variables }
        }),
        updatedAt: new Date()
      };
      if (phase !== undefined) changedFields.push('phase');
      if (progress !== undefined) changedFields.push('progress');
      if (variables) changedFields.push('variables');
    }

    // Handle settings updates
    if (settings) {
      updates.config = {
        ...activity.config,
        settings: { ...(activity.config?.settings || {}), ...settings }
      };
      changedFields.push('settings');
    }

    // Always update lastActivityAt
    updates.lastActivityAt = new Date();

    // No changes? Return current activity
    if (changedFields.length === 0) {
      return transformPayload(params, {
        success: true,
        activity,
        changedFields: []
      });
    }

    // Persist updates
    const updateResult = await Commands.execute('data/update', {
      collection: 'activities',
      id: activity.id,
      data: updates,
      context: params.context,
      sessionId: params.sessionId
    }) as unknown as { success: boolean; error?: string; data?: unknown };

    if (!updateResult.success) {
      return transformPayload(params, {
        success: false,
        error: updateResult.error || 'Failed to update activity'
      });
    }

    const updatedActivity = updateResult.data as ActivityEntity;

    // Emit events
    if (phase !== undefined && phase !== previousPhase) {
      Events.emit('activity:phase:changed', {
        activityId: activity.id,
        previousPhase,
        newPhase: phase,
        progress
      });
    }

    if (status === 'completed') {
      Events.emit('activity:completed', { activity: updatedActivity });
    }

    Events.emit('activity:updated', {
      activity: updatedActivity,
      changedFields
    });

    return transformPayload(params, {
      success: true,
      activity: updatedActivity,
      changedFields
    });
  }
}
