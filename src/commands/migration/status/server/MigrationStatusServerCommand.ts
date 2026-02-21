/**
 * Migration Status Command - Server Implementation
 *
 * Forwards to Rust DataModule's migration/status handler via IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { MigrationStatusParams, MigrationStatusResult } from '../shared/MigrationStatusTypes';
import { createMigrationStatusResultFromParams } from '../shared/MigrationStatusTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class MigrationStatusServerCommand extends CommandBase<MigrationStatusParams, MigrationStatusResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('migration/status', context, subpath, commander);
  }

  async execute(params: MigrationStatusParams): Promise<MigrationStatusResult> {
    const rustClient = RustCoreIPCClient.getInstance();
    const result = await rustClient.execute<any>('migration/status', {});

    if (!result.success) {
      return createMigrationStatusResultFromParams(params, {
        success: false,
        total: 0,
        migrated: 0,
        failed: 0,
        paused: false,
        collections: [],
        error: result.error || 'No active migration',
      });
    }

    const data = result.data ?? {};
    return createMigrationStatusResultFromParams(params, {
      success: true,
      total: data.total ?? 0,
      migrated: data.migrated ?? 0,
      failed: data.failed ?? 0,
      paused: data.paused ?? false,
      collections: data.collections ?? [],
    });
  }
}
