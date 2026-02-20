/**
 * Migration Pause Command - Server Implementation
 *
 * Forwards to Rust DataModule's migration/pause handler via IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { MigrationPauseParams, MigrationPauseResult } from '../shared/MigrationPauseTypes';
import { createMigrationPauseResultFromParams } from '../shared/MigrationPauseTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class MigrationPauseServerCommand extends CommandBase<MigrationPauseParams, MigrationPauseResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('migration/pause', context, subpath, commander);
  }

  async execute(params: MigrationPauseParams): Promise<MigrationPauseResult> {
    const rustClient = RustCoreIPCClient.getInstance();
    const result = await rustClient.execute<any>('migration/pause', {});

    if (!result.success) {
      return createMigrationPauseResultFromParams(params, {
        success: false,
        paused: false,
        error: result.error || 'No active migration to pause',
      });
    }

    return createMigrationPauseResultFromParams(params, {
      success: true,
      paused: true,
    });
  }
}
