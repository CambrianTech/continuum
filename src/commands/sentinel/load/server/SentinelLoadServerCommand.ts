/**
 * Sentinel Load Command - Server Implementation
 *
 * Load saved sentinel definitions from database.
 * Optionally run immediately after loading.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { SentinelLoadParams, SentinelLoadResult } from '../shared/SentinelLoadTypes';
import type { SentinelEntity } from '../../../../system/sentinel';
import { DataList } from '@commands/data/list/shared/DataListTypes';
import { DataUpdate } from '@commands/data/update/shared/DataUpdateTypes';
import { SentinelRun } from '@commands/sentinel/run/shared/SentinelRunTypes';

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
      const listResult = await DataList.execute({
        collection: COLLECTION,
        limit: 1,
        filter: { id: loadParams.id },
      });

      const items = listResult.items as unknown as SentinelEntity[];
      if (listResult.success && items.length > 0) {
        entity = items[0];
      }

      // If not found and id looks like shortId, try partial match
      if (!entity && loadParams.id.length <= 8) {
        const allResult = await DataList.execute({
          collection: COLLECTION,
          limit: 10,
        });

        const allItems = allResult.items as unknown as SentinelEntity[];
        entity = allItems.find(e => e.id.startsWith(loadParams.id));
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
      const runParams: Record<string, unknown> = {
        ...entity.definition,
        async: loadParams.async !== false,
      };

      if (loadParams.workingDir) {
        runParams.workingDir = loadParams.workingDir;
      }

      try {
        const runResult = await SentinelRun.execute(runParams as Parameters<typeof SentinelRun.execute>[0]);

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
          await DataUpdate.execute({
            collection: COLLECTION,
            id: entity.id,
            data: {
              executions: entity.executions,
              updatedAt: entity.updatedAt,
            },
          });

          return transformPayload(params, {
            success: true,
            entity,
            handle: runResult.handle,
            result: runResult.completed ? execution : undefined,
          });
        }
      } catch (error: unknown) {
        const message = error instanceof Error ? error.message : String(error);
        return transformPayload(params, {
          success: false,
          entity,
          error: `Failed to run: ${message}`,
        });
      }
    }

    return transformPayload(params, {
      success: true,
      entity,
    });
  }
}
