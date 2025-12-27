/**
 * Activity List Server Command
 */

import { CommandBase, type ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type { ActivityListParams, ActivityListResult } from '../shared/ActivityListTypes';
import { Commands } from '@system/core/shared/Commands';
import type { ActivityEntity } from '@system/data/entities/ActivityEntity';

export class ActivityListServerCommand extends CommandBase<ActivityListParams, ActivityListResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/activity/list', context, subpath, commander);
  }

  async execute(payload: JTAGPayload): Promise<ActivityListResult> {
    const params = payload as ActivityListParams;
    const {
      recipeId,
      status,
      ownerId,
      participantId,
      tags,
      limit = 50,
      orderBy = 'lastActivityAt',
      orderDirection = 'desc'
    } = params;

    // Build filter
    const filter: Record<string, unknown> = {};

    if (recipeId) {
      filter.recipeId = recipeId;
    }

    if (status) {
      filter.status = Array.isArray(status) ? { $in: status } : status;
    }

    if (ownerId) {
      filter.ownerId = ownerId;
    }

    // Query activities
    const result = await Commands.execute('data/list', {
      collection: 'activities',
      filter: Object.keys(filter).length > 0 ? filter : undefined,
      limit,
      orderBy: [{ field: orderBy, direction: orderDirection }],
      context: params.context,
      sessionId: params.sessionId
    }) as unknown as { success: boolean; error?: string; items?: unknown[] };

    if (!result.success) {
      return transformPayload(params, {
        success: false,
        error: result.error || 'Failed to list activities'
      });
    }

    let activities = (result.items as ActivityEntity[]) || [];

    // Post-filter by participantId (JSON field query)
    if (participantId) {
      activities = activities.filter(a =>
        a.participants?.some(p => p.userId === participantId && p.isActive)
      );
    }

    // Post-filter by tags (JSON field query)
    if (tags?.length) {
      activities = activities.filter(a =>
        tags.some(tag => a.tags?.includes(tag))
      );
    }

    return transformPayload(params, {
      success: true,
      activities,
      total: activities.length
    });
  }
}
