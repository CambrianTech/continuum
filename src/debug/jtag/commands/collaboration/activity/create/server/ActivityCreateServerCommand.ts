/**
 * Activity Create Server Command
 *
 * Creates a new activity from a recipe template.
 * Uses data/create internally for persistence.
 */

import { CommandBase, type ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type { ActivityCreateParams, ActivityCreateResult } from '../shared/ActivityCreateTypes';
import { Commands } from '@system/core/shared/Commands';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { DataListParams, DataListResult } from '@commands/data/list/shared/DataListTypes';
import type { DataCreateParams, DataCreateResult } from '@commands/data/create/shared/DataCreateTypes';
import type { BaseEntity } from '@system/data/entities/BaseEntity';
import { Events } from '@system/core/shared/Events';
import type { ActivityEntity, ActivityParticipant } from '@system/data/entities/ActivityEntity';
import { generateActivityUniqueId } from '@system/activities/shared/ActivityTypes';

export class ActivityCreateServerCommand extends CommandBase<ActivityCreateParams, ActivityCreateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/activity/create', context, subpath, commander);
  }

  async execute(payload: JTAGPayload): Promise<ActivityCreateResult> {
    const params = payload as ActivityCreateParams;
    const { recipeId, displayName, description, ownerId, participants, config, tags } = params;

    // Get owner ID from params (CLI injects userId at top level)
    const ownerUserId = ownerId || params.userId || params.context?.userId;
    if (!ownerUserId) {
      return transformPayload(params, {
        success: false,
        error: 'Owner ID is required (provide ownerId or ensure context has userId)'
      });
    }

    // Verify recipe exists
    const recipeResult = await Commands.execute<DataListParams, DataListResult<BaseEntity>>(DATA_COMMANDS.LIST, {
      collection: 'recipes',
      filter: { uniqueId: recipeId },
      limit: 1,
      context: params.context,
      sessionId: params.sessionId
    });

    if (!recipeResult.success || !recipeResult.items?.length) {
      return transformPayload(params, {
        success: false,
        error: `Recipe not found: ${recipeId}`
      });
    }

    // Generate unique ID if not provided
    const uniqueId = params.uniqueId || generateActivityUniqueId(recipeId, ownerUserId);

    // Build participants list (owner always included)
    const activityParticipants: ActivityParticipant[] = [
      {
        userId: ownerUserId,
        role: 'owner',
        joinedAt: new Date(),
        isActive: true
      }
    ];

    // Add additional participants
    if (participants) {
      for (const p of participants) {
        if (p.userId !== ownerUserId) {  // Don't duplicate owner
          activityParticipants.push({
            userId: p.userId,
            role: p.role,
            joinedAt: new Date(),
            isActive: true,
            roleConfig: p.roleConfig
          });
        }
      }
    }

    // Create activity entity
    const activityData: Partial<ActivityEntity> = {
      uniqueId,
      displayName,
      description,
      recipeId,
      status: 'active',
      ownerId: ownerUserId,
      participants: activityParticipants,
      state: {
        phase: 'initial',
        progress: 0,
        variables: {},
        updatedAt: new Date()
      },
      config: {
        settings: {},
        ...config
      },
      startedAt: new Date(),
      lastActivityAt: new Date(),
      tags: tags || []
    };

    // Persist via data/create
    const createResult = await Commands.execute<DataCreateParams, DataCreateResult>(DATA_COMMANDS.CREATE, {
      collection: 'activities',
      data: activityData,
      context: params.context,
      sessionId: params.sessionId
    });

    if (!createResult.success) {
      return transformPayload(params, {
        success: false,
        error: createResult.error || 'Failed to create activity'
      });
    }

    const activity = createResult.data as ActivityEntity;

    // Emit activity created event
    Events.emit('activity:created', { activity });

    return transformPayload(params, {
      success: true,
      activity
    });
  }
}
