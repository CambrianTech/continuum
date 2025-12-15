/**
 * Data Vacuum Command - Server Implementation
 *
 * Reclaim disk space after bulk deletes (VACUUM for SQLite, OPTIMIZE for MySQL, etc.)
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
// import { ValidationError } from '../../../../system/core/types/ErrorTypes';  // Uncomment when adding validation
import type { DataVacuumParams, DataVacuumResult } from '../shared/DataVacuumTypes';
import { createDataVacuumResultFromParams } from '../shared/DataVacuumTypes';

export class DataVacuumServerCommand extends CommandBase<DataVacuumParams, DataVacuumResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Data Vacuum', context, subpath, commander);
  }

  async execute(params: DataVacuumParams): Promise<DataVacuumResult> {
    const startTime = Date.now();
    const dbHandle = params.dbHandle || 'primary';

    try {
      // Get database adapter
      const { DatabaseHandleRegistry } = await import('../../../../daemons/data-daemon/server/DatabaseHandleRegistry');
      const registry = DatabaseHandleRegistry.getInstance();
      const adapter = registry.getAdapter(dbHandle);

      // Get database size before vacuum
      const beforeSize = await adapter.getDatabaseSize();

      // Execute VACUUM (no-op for adapters that don't need it)
      await adapter.vacuum();

      // Get database size after vacuum
      const afterSize = await adapter.getDatabaseSize();

      const duration = Date.now() - startTime;
      const spaceReclaimed = beforeSize - afterSize;

      // Only log if actual work was done (size changed)
      if (spaceReclaimed > 0) {
        console.log(`✅ VACUUM complete on ${dbHandle}: ${beforeSize} → ${afterSize} bytes (${duration}ms)`);
      }

      return createDataVacuumResultFromParams(params, {
        success: true,
        dbHandle,
        beforeSize,
        afterSize,
        duration,
        timestamp: new Date().toISOString(),
      });
    } catch (error) {
      console.error(`❌ VACUUM failed on ${dbHandle}:`, error);
      return createDataVacuumResultFromParams(params, {
        success: false,
        dbHandle,
        beforeSize: 0,
        afterSize: 0,
        duration: Date.now() - startTime,
        timestamp: new Date().toISOString(),
      });
    }
  }
}
