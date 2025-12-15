/**
 * ArchiveDaemon Server - Server-specific implementation
 *
 * GENERATED FILE - DO NOT EDIT MANUALLY
 */

import { ArchiveDaemon } from '../shared/ArchiveDaemon';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { ENTITY_REGISTRY, initializeEntityRegistry } from '../../data-daemon/server/EntityRegistry';
import { getArchiveConfig, type ArchiveConfig } from '../../../system/data/decorators/FieldDecorators';
import { Commands } from '../../../system/core/shared/Commands';
import { DATA_COMMANDS } from '../../../commands/data/shared/DataCommandConstants';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import type { DataCreateParams, DataCreateResult } from '../../../commands/data/create/shared/DataCreateTypes';
import type { DataDeleteParams, DataDeleteResult } from '../../../commands/data/delete/shared/DataDeleteTypes';
import type { BaseEntity } from '../../../system/data/entities/BaseEntity';
import { Logger } from '../../../system/core/logging/Logger';

export class ArchiveDaemonServer extends ArchiveDaemon {
  private checkCounter = 0;
  private archiveConfigs: Map<string, ArchiveConfig> = new Map(); // Cached configs (re-scanned periodically)
  private readonly RESCAN_INTERVAL = 60; // Re-discover entities every 60 checks (1 hour)
  private isArchiving = false; // Mutex to prevent concurrent archive operations

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    // Initialize standardized logging (daemons/ subdirectory)
    // Logs go to .continuum/jtag/logs/system/daemons/ArchiveDaemonServer.log
    const className = this.constructor.name;
    this.log = Logger.create(className, `daemons/${className}`);
  }

  /**
   * Start the intermittent archive loop using DaemonBase.registerInterval()
   */
  protected override async onStart(): Promise<void> {
    this.log.info('🗄️  Archive daemon started (checking every 60 seconds)');

    // Use DaemonBase's registerInterval for proper concurrency management
    // This keeps the interval alive and handles cleanup automatically
    this.registerInterval('archive-check', async () => {
      this.checkCounter++;
      await this.checkAndArchive();
    }, 60000); // Check every 60 seconds (1 minute)
  }

  /**
   * Discover entities with @Archive() decorator
   * Re-scans periodically to pick up newly added entities
   */
  private discoverArchivableEntities(): void {
    // Initialize entity registry if empty (handles startup race condition)
    if (ENTITY_REGISTRY.size === 0) {
      this.log.info('🗄️  ENTITY_REGISTRY empty, initializing...');
      initializeEntityRegistry();
    }

    const previousCount = this.archiveConfigs.size;
    this.archiveConfigs.clear();

    for (const [collectionName, EntityClass] of ENTITY_REGISTRY.entries()) {
      const archiveConfig = getArchiveConfig(EntityClass);
      if (archiveConfig) {
        this.archiveConfigs.set(collectionName, archiveConfig);
      }
    }

    // Log changes in monitored entities
    if (this.archiveConfigs.size !== previousCount) {
      if (this.archiveConfigs.size === 0) {
        this.log.info('🗄️  No entities configured with @Archive() decorator');
      } else {
        this.log.info(`🗄️  Monitoring ${this.archiveConfigs.size} entities with @Archive():`);
        for (const [name, config] of this.archiveConfigs.entries()) {
          this.log.info(`    ${name} (max=${config.maxRows}, batch=${config.rowsPerArchive}, orderBy=${config.orderByField})`);
        }
      }
    }
  }

  /**
   * Stop the archive loop gracefully
   */
  protected override async onStop(): Promise<void> {
    this.log.info('🗄️  Stopping archive daemon loop');
    this.clearInterval('archive-check');
  }

  /**
   * Check all monitored entities and archive if needed
   * Periodically re-discovers entities to pick up new @Archive() configurations
   * SERIAL EXECUTION: Skips if previous operation still running
   */
  protected override async checkAndArchive(): Promise<void> {
    // Mutex: prevent concurrent archive operations
    if (this.isArchiving) {
      this.log.info('🗄️  Archive operation already in progress, skipping this check');
      return;
    }

    this.isArchiving = true;
    try {
      // Periodically re-scan for new entities (every RESCAN_INTERVAL checks)
      if (this.checkCounter === 1 || this.checkCounter % this.RESCAN_INTERVAL === 0) {
        this.discoverArchivableEntities();
      }

      if (this.archiveConfigs.size === 0) {
        return; // Nothing to do
      }

      let entitiesArchived = 0;

    // Check each monitored entity using cached config
    for (const [collectionName, archiveConfig] of this.archiveConfigs.entries()) {
      try {
        // Count rows in active table
        const countResult = await Commands.execute<DataListParams, DataListResult<BaseEntity>>(DATA_COMMANDS.LIST, {
          collection: collectionName,
          limit: 0
        });

        const currentCount = countResult.count || 0;

        // Archive if over limit
        if (currentCount > archiveConfig.maxRows) {
          this.log.info(`🗄️  ${collectionName}: ${currentCount} rows exceeds ${archiveConfig.maxRows}, archiving...`);

          const archived = await this.archiveEntity(
            collectionName,
            archiveConfig.maxRows,
            archiveConfig.rowsPerArchive
          );

          this.log.info(`🗄️  Archived ${archived} rows from ${collectionName}`);
          entitiesArchived++;
        }
      } catch (error) {
        this.log.error(`Failed to archive ${collectionName}:`, error);
      }
    }

      // Periodic status log (every 10 checks or when archiving happens)
      if (this.checkCounter % 10 === 0 || entitiesArchived > 0) {
        if (entitiesArchived > 0) {
          this.log.info(`🗄️  Check #${this.checkCounter}: Archived ${entitiesArchived} collections`);
        } else {
          this.log.info(`🗄️  Check #${this.checkCounter}: All ${this.archiveConfigs.size} entities within limits`);
        }
      }
    } finally {
      // Always reset mutex, even if error occurred
      this.isArchiving = false;
    }
  }

  /**
   * Check if archive file needs rotation and create new file if needed
   * Rotates when current archive file exceeds maxArchiveFileRows
   */
  private async checkAndRotateArchiveIfNeeded(archiveConfig: ArchiveConfig, collection: string): Promise<void> {
    const { DatabaseHandleRegistry } = await import('../../data-daemon/server/DatabaseHandleRegistry');
    const { DATABASE_PATHS } = await import('../../../system/data/config/DatabaseConfig');
    const fs = await import('fs');
    const path = await import('path');

    const registry = DatabaseHandleRegistry.getInstance();

    // Count rows in current archive file
    const countResult = await Commands.execute<DataListParams, DataListResult<BaseEntity>>(DATA_COMMANDS.LIST, {
      collection,
      dbHandle: archiveConfig.destHandle,
      limit: 0
    });

    const currentArchiveRows = countResult.count || 0;

    // Check if rotation needed
    if (currentArchiveRows < archiveConfig.maxArchiveFileRows) {
      return; // No rotation needed
    }

    this.log.info(`🗄️  Archive file for ${collection} has ${currentArchiveRows} rows (max: ${archiveConfig.maxArchiveFileRows}), rotating...`);

    // Find current archive file number and create next one
    const primaryDbPath = DATABASE_PATHS.SQLITE;
    const archiveDir = path.join(path.dirname(primaryDbPath), 'archive');
    const sourceDbName = path.basename(primaryDbPath, '.sqlite');

    const archiveFiles = fs.readdirSync(archiveDir)
      .filter((f: string) => f.startsWith(`${sourceDbName}-`) && f.endsWith('.sqlite'))
      .sort()
      .reverse(); // Most recent first

    if (archiveFiles.length === 0) {
      throw new Error('Cannot rotate archive: no archive files found');
    }

    // Parse current file number (e.g., 'database-001.sqlite' → 1)
    const currentFile = archiveFiles[0];
    const match = currentFile.match(new RegExp(`${sourceDbName}-(\\d+)\\.sqlite`));
    if (!match) {
      throw new Error(`Invalid archive file name format: ${currentFile}`);
    }

    const currentNum = parseInt(match[1], 10);
    const nextNum = currentNum + 1;
    const nextFile = `${sourceDbName}-${String(nextNum).padStart(3, '0')}.sqlite`;
    const nextPath = path.join(archiveDir, nextFile);

    this.log.info(`🗄️  Creating new archive file: ${nextPath}`);

    // Close old archive handle
    const oldHandle = registry.getAdapter(archiveConfig.destHandle);
    // Note: We don't close it because 'archive' is an alias, not the actual handle UUID
    // The alias will be re-pointed to the new handle

    // Open new archive file handle
    const newHandle = await registry.open('sqlite', {
      filename: nextPath
    });

    // Update 'archive' alias to point to new handle
    registry.registerAlias(archiveConfig.destHandle, newHandle);

    this.log.info(`🗄️  Archive rotated: ${currentFile} → ${nextFile}`);
  }

  /**
   * Copy-Verify-Delete pattern for safe archiving
   * Returns number of rows successfully archived
   */
  private async copyVerifyDelete(
    collection: string,
    rows: readonly BaseEntity[],
    sourceHandle: string,
    destHandle: string
  ): Promise<number> {
    // Step 1: Copy all rows to archive
    const copiedIds: string[] = [];

    for (const row of rows) {
      try {
        await Commands.execute<DataCreateParams, DataCreateResult>(DATA_COMMANDS.CREATE, {
          collection,
          data: row,
          dbHandle: destHandle,
          suppressEvents: true
        } as any);
        copiedIds.push(row.id);
      } catch (error) {
        this.log.error(`🗄️  Failed to copy row ${row.id} to archive:`, error);
      }
    }

    this.log.info(`🗄️  [${collection}] Copied ${copiedIds.length}/${rows.length} rows (${sourceHandle} → ${destHandle})`);

    if (copiedIds.length === 0) {
      this.log.error(`🗄️  Copy FAILED - no rows copied to archive`);
      return 0;
    }

    // Step 2: Verify all copied rows exist in archive
    const verifyResult = await Commands.execute<DataListParams, DataListResult<BaseEntity>>(DATA_COMMANDS.LIST, {
      collection,
      dbHandle: destHandle,
      filter: { id: { $in: copiedIds } },
      limit: copiedIds.length
    });

    const verifiedIds = new Set((verifyResult.items || []).map(item => item.id));
    this.log.info(`🗄️  [${collection}] Verified ${verifiedIds.size}/${copiedIds.length} rows in ${destHandle}`);

    if (verifiedIds.size === 0) {
      this.log.error(`🗄️  Verification FAILED - aborting delete`);
      return 0;
    }

    // Step 3: Delete only verified rows from source
    let deletedCount = 0;
    for (const id of verifiedIds) {
      try {
        await Commands.execute<DataDeleteParams, DataDeleteResult>(DATA_COMMANDS.DELETE, {
          collection,
          id,
          dbHandle: sourceHandle,
          suppressEvents: true
        } as any);
        deletedCount++;
      } catch (error) {
        this.log.error(`🗄️  Failed to delete row ${id} from source:`, error);
      }
    }

    this.log.info(`🗄️  [${collection}] Deleted ${deletedCount}/${verifiedIds.size} rows from ${sourceHandle}`);

    return verifiedIds.size;
  }

  /**
   * Archive rows from a specific entity collection
   * Returns the number of rows archived
   *
   * TODO: Refactor to use two-handle pattern instead of direct collection access
   * - Open sourceHandle (e.g., 'primary') → collection (e.g., 'chat_messages')
   * - Open destHandle (e.g., 'archive') → same collection name
   * - Use data/open, data/query-next, data/create, data/delete with handles
   * - Let ORM naturally handle schema on each handle
   */
  protected override async archiveEntity(
    collection: string,
    maxRows: number,
    rowsPerArchive: number
  ): Promise<number> {
    const EntityClass = ENTITY_REGISTRY.get(collection);
    if (!EntityClass) {
      throw new Error(`Entity not found in registry: ${collection}`);
    }

    const archiveConfig = getArchiveConfig(EntityClass);
    if (!archiveConfig) {
      throw new Error(`Entity ${collection} does not have @Archive() configuration`);
    }

    // Check if archive file needs rotation before archiving
    await this.checkAndRotateArchiveIfNeeded(archiveConfig, collection);

    // Two-handle pattern: same collection name, different database handles
    const sourceHandle = archiveConfig.sourceHandle;  // e.g., 'primary'
    const destHandle = archiveConfig.destHandle;      // e.g., 'archive'

    // Get current count from source handle
    const countResult = await Commands.execute<DataListParams, DataListResult<BaseEntity>>(DATA_COMMANDS.LIST, {
      collection,
      dbHandle: sourceHandle,
      limit: 0
    });
    const currentCount = countResult.count || 0;

    // Calculate rows to archive (with buffer for headroom)
    const targetCount = maxRows - rowsPerArchive;
    const rowsToArchive = currentCount - targetCount;

    if (rowsToArchive <= 0) {
      return 0;
    }

    this.log.info(`🗄️  Archiving ${rowsToArchive} rows from ${collection} (${sourceHandle} → ${destHandle})`);

    let totalArchived = 0;

    // Archive in batches
    while (totalArchived < rowsToArchive) {
      const batchSize = Math.min(rowsPerArchive, rowsToArchive - totalArchived);

      // Get oldest rows from source handle (ordered by archiveConfig.orderByField)
      const batchResult = await Commands.execute<DataListParams, DataListResult<BaseEntity>>(DATA_COMMANDS.LIST, {
        collection,
        dbHandle: sourceHandle,  // Read from source
        limit: batchSize,
        orderBy: [{ field: archiveConfig.orderByField, direction: 'asc' }]
      });

      const rowsToMove = batchResult.items || [];
      if (rowsToMove.length === 0) break;

      // Batch archive with verification (Star Trek transporter pattern)
      const archived = await this.copyVerifyDelete(collection, rowsToMove, sourceHandle, destHandle);
      totalArchived += archived;

      this.log.info(`🗄️  [${collection}] Progress: ${totalArchived}/${rowsToArchive} rows (${Math.round(totalArchived/rowsToArchive*100)}% complete)`);

      // Sleep between batches (low CPU impact)
      await new Promise(resolve => setTimeout(resolve, 100));
    }

    this.log.info(`🗄️  ✅ [${collection}] Archiving complete: ${totalArchived} rows (${sourceHandle} → ${destHandle})`);

    return totalArchived;
  }

  /**
   * Get statistics about archived data
   */
  protected override async getArchiveStats(): Promise<Record<string, { activeRows: number; archivedRows: number }>> {
    const stats: Record<string, { activeRows: number; archivedRows: number }> = {};

    for (const [collectionName, EntityClass] of ENTITY_REGISTRY.entries()) {
      const archiveConfig = getArchiveConfig(EntityClass);
      if (!archiveConfig) continue;

      try {
        // Count active rows from source handle
        const activeResult = await Commands.execute<DataListParams, DataListResult<BaseEntity>>(DATA_COMMANDS.LIST, {
          collection: collectionName,
          dbHandle: archiveConfig.sourceHandle,
          limit: 0
        });

        // Count archived rows from destination handle (SAME collection name!)
        const archiveResult = await Commands.execute<DataListParams, DataListResult<BaseEntity>>(DATA_COMMANDS.LIST, {
          collection: collectionName,  // Same name, different handle = different database
          dbHandle: archiveConfig.destHandle,
          limit: 0
        });

        stats[collectionName] = {
          activeRows: activeResult.count || 0,
          archivedRows: archiveResult.count || 0
        };
      } catch (error) {
        this.log.error(`Failed to get stats for ${collectionName}:`, error);
      }
    }

    return stats;
  }
}
