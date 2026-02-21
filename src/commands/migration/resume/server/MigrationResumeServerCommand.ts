/**
 * Migration Resume Command - Server Implementation
 *
 * Forwards to Rust DataModule's migration/resume handler via IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { MigrationResumeParams, MigrationResumeResult } from '../shared/MigrationResumeTypes';
import { createMigrationResumeResultFromParams } from '../shared/MigrationResumeTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class MigrationResumeServerCommand extends CommandBase<MigrationResumeParams, MigrationResumeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('migration/resume', context, subpath, commander);
  }

  async execute(params: MigrationResumeParams): Promise<MigrationResumeResult> {
    const rustClient = RustCoreIPCClient.getInstance();
    const result = await rustClient.execute<any>('migration/resume', {});

    if (!result.success) {
      return createMigrationResumeResultFromParams(params, {
        success: false,
        total: 0,
        migrated: 0,
        failed: 0,
        paused: true,
        collections: [],
        error: result.error || 'No active migration to resume',
      });
    }

    const data = result.data ?? {};
    return createMigrationResumeResultFromParams(params, {
      success: true,
      total: data.total ?? 0,
      migrated: data.migrated ?? 0,
      failed: data.failed ?? 0,
      paused: data.paused ?? false,
      collections: data.collections ?? [],
    });
  }
}
