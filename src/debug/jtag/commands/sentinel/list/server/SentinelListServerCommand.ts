/**
 * Sentinel List Command - Server Implementation
 *
 * List saved sentinel definitions from database.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import type { SentinelListParams, SentinelListResult, SentinelSummary } from '../shared/SentinelListTypes';
import type { SentinelEntity } from '../../../../system/sentinel';

const COLLECTION = 'sentinels';

export class SentinelListServerCommand extends CommandBase<SentinelListParams, SentinelListResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/list', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelListResult> {
    const listParams = params as SentinelListParams;
    const limit = listParams.limit || 20;

    // Build filter
    const filter: Record<string, any> = {};

    if (listParams.type) {
      filter['definition.type'] = listParams.type;
    }

    if (listParams.templatesOnly) {
      filter.isTemplate = true;
    }

    if (listParams.tags && listParams.tags.length > 0) {
      filter['definition.tags'] = { $in: listParams.tags };
    }

    if (listParams.search) {
      filter['definition.name'] = { $regex: listParams.search, $options: 'i' };
    }

    try {
      const result = await Commands.execute('data/list', {
        collection: COLLECTION,
        filter: Object.keys(filter).length > 0 ? filter : undefined,
        limit,
        orderBy: [{ field: 'updatedAt', direction: 'desc' }],
      } as any) as any;

      if (!result.success) {
        return transformPayload(params, {
          success: false,
          sentinels: [],
          total: 0,
        });
      }

      const entities = (result.items || result.data || []) as SentinelEntity[];
      const summaries: SentinelSummary[] = entities.map(entity => {
        const lastExecution = entity.executions[0];
        return {
          id: entity.id,
          shortId: entity.id.slice(0, 8),
          name: entity.definition.name,
          type: entity.definition.type,
          description: entity.definition.description,
          tags: entity.definition.tags,
          isTemplate: entity.isTemplate,
          executionCount: entity.executions.length,
          lastRun: lastExecution?.startedAt,
          lastSuccess: lastExecution?.success,
          createdAt: entity.createdAt,
          createdBy: entity.createdBy,
        };
      });

      return transformPayload(params, {
        success: true,
        sentinels: summaries,
        total: result.count || result.total || summaries.length,
      });
    } catch (error: any) {
      return transformPayload(params, {
        success: false,
        sentinels: [],
        total: 0,
      });
    }
  }
}
