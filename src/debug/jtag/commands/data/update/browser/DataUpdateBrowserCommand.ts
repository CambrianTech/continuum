/**
 * Data Update Command - Browser Implementation (LOCAL-FIRST)
 *
 * ALL updates use offline-first storage:
 * 1. Update localStorage immediately (instant UI)
 * 2. Sync to server in background
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataUpdateParams, DataUpdateResult } from '../shared/DataUpdateTypes';
import { createDataUpdateResultFromParams } from '../shared/DataUpdateTypes';
import { DataUpdateCommand } from '../shared/DataUpdateCommand';
import { LocalStorageDataBackend } from '../../../../daemons/data-daemon/browser/LocalStorageDataBackend';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';

export class DataUpdateBrowserCommand extends DataUpdateCommand<BaseEntity> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-update', context, subpath, commander);
  }

  async execute(params: DataUpdateParams): Promise<DataUpdateResult<BaseEntity>> {
    const verbose = typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

    // 1. Update localStorage immediately
    const localResult = await this.updateLocal(params);
    verbose && console.log(`⚡ data/update: LOCAL ${params.collection}/${params.id}`);

    // 2. Sync to server in background (fire-and-forget)
    this.remoteExecute(params).catch(err => {
      console.warn(`data/update: Server sync failed for ${params.collection}/${params.id}:`, err);
    });

    return localResult;
  }

  private async updateLocal(params: DataUpdateParams): Promise<DataUpdateResult<BaseEntity>> {
    try {
      // Try update first
      let result = await LocalStorageDataBackend.update(params.collection, params.id, params.data);

      if (!result.success) {
        // Create if doesn't exist
        const entity = { id: params.id, ...params.data };
        await LocalStorageDataBackend.create(params.collection, entity as any);
      }

      const readResult = await LocalStorageDataBackend.read(params.collection, params.id);
      return createDataUpdateResultFromParams(params, {
        success: true,
        data: readResult.entity,
        found: true,
        id: params.id
      });
    } catch (error) {
      return createDataUpdateResultFromParams(params, {
        success: false,
        error: error instanceof Error ? error.message : 'Update failed',
        found: false,
        id: params.id
      });
    }
  }

  protected async executeDataCommand(params: DataUpdateParams): Promise<DataUpdateResult<BaseEntity>> {
    return this.updateLocal(params);
  }
}
