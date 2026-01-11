/**
 * Data Read Command - Browser Implementation (LOCAL-FIRST)
 *
 * ALL reads use offline-first storage:
 * 1. Check localStorage first (instant)
 * 2. If found, return immediately
 * 3. If not found, fetch from server and cache
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { DataReadCommand } from '../shared/DataReadCommand';
import { LocalStorageDataBackend } from '../../../../daemons/data-daemon/browser/LocalStorageDataBackend';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import type { DataReadParams, DataReadResult } from '../shared/DataReadTypes';
import { createDataReadResultFromParams } from '../shared/DataReadTypes';

export class DataReadBrowserCommand extends DataReadCommand<BaseEntity> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-read', context, subpath, commander);
  }

  async execute(params: DataReadParams): Promise<DataReadResult<BaseEntity>> {
    const verbose = typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;
    const backend = (params as any).backend ?? 'auto';

    // If backend is 'server', skip localStorage entirely
    if (backend === 'server') {
      verbose && console.log(`📡 data/read: SERVER-ONLY ${params.collection}/${params.id} (backend=server)`);
      const serverResult = await this.remoteExecute<DataReadParams, DataReadResult<BaseEntity>>(params);
      // Still cache the result for future reads
      if (serverResult.success && serverResult.found && serverResult.data) {
        LocalStorageDataBackend.create(params.collection, serverResult.data).catch(() => {});
      }
      return serverResult;
    }

    // 1. Try localStorage first (instant)
    const localResult = await this.readLocal(params);
    if (localResult.success && localResult.found) {
      verbose && console.log(`⚡ data/read: LOCAL hit ${params.collection}/${params.id}`);
      return localResult;
    }

    // 2. Fetch from server
    verbose && console.log(`🔄 data/read: LOCAL miss ${params.collection}/${params.id}, fetching from server...`);
    const serverResult = await this.remoteExecute<DataReadParams, DataReadResult<BaseEntity>>(params);

    // 3. Cache for future reads
    if (serverResult.success && serverResult.found && serverResult.data) {
      LocalStorageDataBackend.create(params.collection, serverResult.data).catch(() => {});
    }

    return serverResult;
  }

  private async readLocal(params: DataReadParams): Promise<DataReadResult<BaseEntity>> {
    try {
      const result = await LocalStorageDataBackend.read(params.collection, params.id);
      if (result.success && result.entity) {
        return createDataReadResultFromParams(params, {
          success: true,
          data: result.entity,
          found: true
        });
      }
      return createDataReadResultFromParams(params, { success: true, found: false });
    } catch {
      return createDataReadResultFromParams(params, { success: true, found: false });
    }
  }

  protected async executeDataCommand(params: DataReadParams): Promise<DataReadResult<BaseEntity>> {
    return this.readLocal(params);
  }
}
