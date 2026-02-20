/**
 * Migration Verify Command - Server Implementation
 *
 * Forwards to Rust DataModule's migration/verify handler via IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { MigrationVerifyParams, MigrationVerifyResult } from '../shared/MigrationVerifyTypes';
import { createMigrationVerifyResultFromParams } from '../shared/MigrationVerifyTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class MigrationVerifyServerCommand extends CommandBase<MigrationVerifyParams, MigrationVerifyResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('migration/verify', context, subpath, commander);
  }

  async execute(params: MigrationVerifyParams): Promise<MigrationVerifyResult> {
    const rustClient = RustCoreIPCClient.getInstance();
    const result = await rustClient.execute<any>('migration/verify', {});

    if (!result.success) {
      return createMigrationVerifyResultFromParams(params, {
        success: false,
        verified: false,
        collections: [],
        error: result.error || 'No active migration to verify',
      });
    }

    const data = result.data ?? {};
    return createMigrationVerifyResultFromParams(params, {
      success: true,
      verified: data.verified ?? false,
      collections: data.collections ?? [],
    });
  }
}
