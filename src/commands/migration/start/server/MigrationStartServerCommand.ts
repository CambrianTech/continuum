/**
 * Migration Start Command - Server Implementation
 *
 * Forwards to Rust DataModule's migration/start handler via IPC.
 * The Rust MigrationEngine handles streaming data transfer between any two StorageAdapters.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { MigrationStartParams, MigrationStartResult } from '../shared/MigrationStartTypes';
import { createMigrationStartResultFromParams } from '../shared/MigrationStartTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class MigrationStartServerCommand extends CommandBase<MigrationStartParams, MigrationStartResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('migration/start', context, subpath, commander);
  }

  async execute(params: MigrationStartParams): Promise<MigrationStartResult> {
    if (!params.source || params.source.trim() === '') {
      throw new ValidationError('source', 'Missing required parameter \'source\'. Provide a connection string (file path for SQLite, postgres:// URL for Postgres).');
    }
    if (!params.target || params.target.trim() === '') {
      throw new ValidationError('target', 'Missing required parameter \'target\'. Provide a connection string (file path for SQLite, postgres:// URL for Postgres).');
    }

    const rustClient = RustCoreIPCClient.getInstance();
    const result = await rustClient.execute<any>('migration/start', {
      source: params.source,
      target: params.target,
      batchSize: params.batchSize,
      throttleMs: params.throttleMs,
      collections: params.collections,
    });

    if (!result.success) {
      return createMigrationStartResultFromParams(params, {
        success: false,
        total: 0,
        migrated: 0,
        failed: 0,
        paused: false,
        collections: [],
        error: result.error || 'Migration start failed',
      });
    }

    const data = result.data ?? {};
    return createMigrationStartResultFromParams(params, {
      success: true,
      total: data.total ?? 0,
      migrated: data.migrated ?? 0,
      failed: data.failed ?? 0,
      paused: data.paused ?? false,
      collections: data.collections ?? [],
    });
  }
}
