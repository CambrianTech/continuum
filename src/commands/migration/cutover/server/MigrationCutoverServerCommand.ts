/**
 * Migration Cutover Command - Server Implementation
 *
 * Forwards to Rust DataModule's migration/cutover handler via IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { MigrationCutoverParams, MigrationCutoverResult } from '../shared/MigrationCutoverTypes';
import { createMigrationCutoverResultFromParams } from '../shared/MigrationCutoverTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class MigrationCutoverServerCommand extends CommandBase<MigrationCutoverParams, MigrationCutoverResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('migration/cutover', context, subpath, commander);
  }

  async execute(params: MigrationCutoverParams): Promise<MigrationCutoverResult> {
    if (!params.current || params.current.trim() === '') {
      throw new ValidationError('current', 'Missing required parameter \'current\'. Provide the current connection string to decommission.');
    }
    if (!params.target || params.target.trim() === '') {
      throw new ValidationError('target', 'Missing required parameter \'target\'. Provide the target connection string to switch to.');
    }

    const rustClient = RustCoreIPCClient.getInstance();
    const result = await rustClient.execute<any>('migration/cutover', {
      current: params.current,
      target: params.target,
    });

    if (!result.success) {
      return createMigrationCutoverResultFromParams(params, {
        success: false,
        cutover: false,
        previousConnection: '',
        error: result.error || 'Cutover failed',
      });
    }

    const data = result.data ?? {};
    return createMigrationCutoverResultFromParams(params, {
      success: true,
      cutover: true,
      previousConnection: data.previousConnection ?? params.current,
    });
  }
}
