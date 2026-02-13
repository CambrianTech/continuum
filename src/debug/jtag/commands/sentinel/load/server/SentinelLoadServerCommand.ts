/**
 * Sentinel Load Command - Server Implementation
 *
 * Load saved sentinel definitions from database.
 * Optionally run immediately after loading.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import type { SentinelLoadParams, SentinelLoadResult } from '../shared/SentinelLoadTypes';
import type { SentinelEntity } from '../../../../system/sentinel';

const COLLECTION = 'sentinels';

export class SentinelLoadServerCommand extends CommandBase<SentinelLoadParams, SentinelLoadResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/load', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelLoadResult> {
    const loadParams = params as SentinelLoadParams;

    if (!loadParams.id) {
      return transformPayload(params, {
        success: false,
        error: 'id is required',
      });
    }

    // Try to find by full ID or shortId using data/list with filter
    let entity: SentinelEntity | undefined;

    try {
      // Try exact match first
      let listResult = await Commands.execute('data/list', {
        collection: COLLECTION,
        limit: 1,
        filter: { id: loadParams.id },
      } as any) as any;

      let items = listResult.items || listResult.data || [];
      if (listResult.success && items.length > 0) {
        entity = items[0] as SentinelEntity;
      }

      // If not found and id looks like shortId, try partial match
      if (!entity && loadParams.id.length <= 8) {
        listResult = await Commands.execute('data/list', {
          collection: COLLECTION,
          limit: 10,
        } as any) as any;

        items = listResult.items || listResult.data || [];
        entity = items.find((e: SentinelEntity) => e.id.startsWith(loadParams.id));
      }
    } catch {
      // Ignore
    }

    if (!entity) {
      return transformPayload(params, {
        success: false,
        error: `Sentinel not found: ${loadParams.id}`,
      });
    }

    // If run requested, execute the sentinel
    if (loadParams.run) {
      const runParams: any = {
        ...entity.definition,
        async: loadParams.async !== false,
      };

      if (loadParams.workingDir) {
        runParams.workingDir = loadParams.workingDir;
      }

      try {
        const runResult = await Commands.execute('sentinel/run', runParams) as any;

        // Record execution in entity
        if (runResult.handle) {
          const execution = {
            handle: runResult.handle,
            success: runResult.success,
            startedAt: new Date().toISOString(),
            completedAt: runResult.completed ? new Date().toISOString() : undefined,
            data: runResult.data,
          };

          entity.executions.unshift(execution);
          entity.updatedAt = new Date().toISOString();

          // Update entity in database
          await Commands.execute('data/update', {
            collection: COLLECTION,
            id: entity.id,
            data: {
              executions: entity.executions,
              updatedAt: entity.updatedAt,
            },
          } as any);

          return transformPayload(params, {
            success: true,
            entity,
            handle: runResult.handle,
            result: runResult.completed ? execution : undefined,
          });
        }
      } catch (error: any) {
        return transformPayload(params, {
          success: false,
          entity,
          error: `Failed to run: ${error.message}`,
        });
      }
    }

    return transformPayload(params, {
      success: true,
      entity,
    });
  }
}
