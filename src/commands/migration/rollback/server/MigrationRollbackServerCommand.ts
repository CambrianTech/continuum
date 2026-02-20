/**
 * Migration Rollback Command - Server Implementation
 *
 * Forwards to Rust DataModule's migration/rollback handler via IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { MigrationRollbackParams, MigrationRollbackResult } from '../shared/MigrationRollbackTypes';
import { createMigrationRollbackResultFromParams } from '../shared/MigrationRollbackTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class MigrationRollbackServerCommand extends CommandBase<MigrationRollbackParams, MigrationRollbackResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('migration/rollback', context, subpath, commander);
  }

  async execute(params: MigrationRollbackParams): Promise<MigrationRollbackResult> {
    if (!params.current || params.current.trim() === '') {
      throw new ValidationError('current', 'Missing required parameter \'current\'. Provide the current connection string to roll back from.');
    }

    const rustClient = RustCoreIPCClient.getInstance();
    const result = await rustClient.execute<any>('migration/rollback', {
      current: params.current,
    });

    if (!result.success) {
      return createMigrationRollbackResultFromParams(params, {
        success: false,
        rolledBack: false,
        restoredConnection: '',
        error: result.error || 'Rollback failed — no previous connection stored',
      });
    }

    const data = result.data ?? {};
    return createMigrationRollbackResultFromParams(params, {
      success: true,
      rolledBack: true,
      restoredConnection: data.restoredConnection ?? data.previousConnection ?? '',
    });
  }
}
