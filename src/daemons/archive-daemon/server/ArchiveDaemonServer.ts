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
import * as net from 'net';
import type { ArchiveResponse } from '@shared/ipc/archive-worker/ArchiveMessageTypes';

import { DataList } from '../../../commands/data/list/shared/DataListTypes';
import { DataCreate } from '../../../commands/data/create/shared/DataCreateTypes';
import { DataDelete } from '../../../commands/data/delete/shared/DataDeleteTypes';
export class ArchiveDaemonServer extends ArchiveDaemon {
  private checkCounter = 0;
  private archiveConfigs: Map<string, ArchiveConfig> = new Map(); // Cached configs (re-scanned periodically)
  private readonly RESCAN_INTERVAL = 60; // Re-discover entities every 60 checks (1 hour)
  private isArchiving = false; // Mutex to prevent concurrent archive operations
  private workerSocketPath = '/tmp/jtag-archive-worker.sock';

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    // Initialize standardized logging (daemons/ subdirectory)
    // Logs go to .continuum/jtag/logs/system/daemons/ArchiveDaemonServer.log
    const className = this.constructor.name;
    this.log = Logger.create(className, `daemons/${className}`);
  }

  /**
   * Start the intermittent archive loop using DaemonBase.registerInterval()
   * Runs every 12 seconds, checks if archiving needed and queues tasks to Rust worker
   */
  /**
   * Send a message to the Rust ArchiveWorker
   */
  private async sendToWorker(message: unknown): Promise<ArchiveResponse> {
    return new Promise((resolve, reject) => {
      const client = net.connect(this.workerSocketPath);

      client.on('connect', () => {
        client.write(JSON.stringify(message) + '\n');
      });

      client.on('data', (data) => {
        try {
          const response = JSON.parse(data.toString()) as ArchiveResponse;
          client.end();
          resolve(response);
        } catch (error) {
          client.end();
          reject(new Error(`Failed to parse response: ${error}`));
        }
      });

      client.on('error', (error) => {
        reject(error);
      });

      // Timeout after 5 seconds
      setTimeout(() => {
        client.destroy();
        reject(new Error('Request timeout'));
      }, 5000);
    });
  }

  protected override async onStart(): Promise<void> {
    this.log.info('🗄️  Archive daemon starting with Rust worker...');

    // Check if Rust ArchiveWorker socket exists
    const fs = await import('fs');
    if (!fs.existsSync(this.workerSocketPath)) {
      this.log.warn(`🗄️  Worker socket not found: ${this.workerSocketPath}`);
      this.log.warn('🗄️  Archiving will be disabled until worker is available');
      return;
    }

    this.log.info(`🗄️  Found Rust ArchiveWorker at ${this.workerSocketPath}`);

    // Discover entities with @Archive() decorator
    this.discoverArchivableEntities();

    // Register periodic check (every 60 seconds)
    // Now safe: Uses Commands.execute() for coordinated DB access (no lock contention)
    this.registerInterval('archive-check', () => this.checkAndArchive(), 60000);

    this.log.info('🗄️  Archive daemon ready (checking every 60s - coordinated via DataDaemon)');
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
   * Check all monitored entities and queue archive tasks to Rust worker if needed
   * Periodically re-discovers entities to pick up new @Archive() configurations
   * SERIAL EXECUTION: Skips if previous operation still running
   */
  protected override async checkAndArchive(): Promise<void> {
    this.checkCounter++;

    // Mutex: prevent concurrent archive operations
    if (this.isArchiving) {
      this.log.info('🗄️  Archive check already in progress, skipping');
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

      let tasksQueued = 0;

      // Check each monitored entity using cached config
      for (const [collectionName, archiveConfig] of this.archiveConfigs.entries()) {
        try {
          // Count rows in active table
          const countResult = await DataList.execute({
            collection: collectionName,
            limit: 0
          });

          const currentCount = countResult.count || 0;

          // Queue archive task if over limit
          if (currentCount > archiveConfig.maxRows) {
            this.log.info(`🗄️  ${collectionName}: ${currentCount} rows exceeds ${archiveConfig.maxRows}, queueing archive task...`);

            const response = await this.sendToWorker({
              command: 'archive',
              task_id: `${collectionName}-${Date.now()}`,
              collection: collectionName,
              source_handle: archiveConfig.sourceHandle,
              dest_handle: archiveConfig.destHandle,
              batch_size: archiveConfig.rowsPerArchive
            });

            if (response.status === 'queued') {
              this.log.info(`🗄️  Task queued for ${collectionName} (position: ${response.queue_position})`);
              tasksQueued++;
            } else {
              this.log.warn(`🗄️  Unexpected response for ${collectionName}:`, response);
            }
          }
        } catch (error) {
          this.log.error(`Failed to queue archive for ${collectionName}:`, error);
        }
      }

      // Periodic status log (every 10 checks or when tasks queued)
      if (this.checkCounter % 10 === 0 || tasksQueued > 0) {
        if (tasksQueued > 0) {
          this.log.info(`🗄️  Check #${this.checkCounter}: Queued ${tasksQueued} archive tasks`);
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
    const { getDatabasePath } = await import('../../../system/config/ServerConfig');
    const fs = await import('fs');
    const path = await import('path');

    const registry = DatabaseHandleRegistry.getInstance();

    // Count rows in current archive file
    const countResult = await DataList.execute({
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
    const primaryDbPath = getDatabasePath();  // Expand $HOME in path
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

    // Note: We don't close the old handle because 'archive' is an alias, not the actual handle UUID
    // The alias will be re-pointed to the new handle
    // (Previous code called getAdapter() but didn't use the result - just checking existence)

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
        await DataCreate.execute({
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
    const verifyResult = await DataList.execute({
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
        await DataDelete.execute({
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
    const countResult = await DataList.execute({
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

    // CRITICAL: Cap work per cycle to prevent overwhelming system
    // Archive max 50 rows per 12-second cycle = ~250 rows/min throughput
    const maxRowsPerCycle = 50;
    const rowsThisCycle = Math.min(rowsToArchive, maxRowsPerCycle);

    this.log.info(`🗄️  Archiving ${rowsThisCycle} of ${rowsToArchive} rows from ${collection} (${sourceHandle} → ${destHandle})`);

    let totalArchived = 0;

    // Archive in batches (capped per cycle)
    while (totalArchived < rowsThisCycle) {
      const batchSize = Math.min(rowsPerArchive, rowsThisCycle - totalArchived);

      // Get oldest rows from source handle (ordered by archiveConfig.orderByField)
      const batchResult = await DataList.execute({
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

      const remainingInDb = currentCount - totalArchived;
      this.log.info(`🗄️  [${collection}] Progress: ${totalArchived}/${rowsThisCycle} rows this cycle (${remainingInDb} remain in primary)`);

      // Sleep between batches (give main thread breathing room)
      await new Promise(resolve => setTimeout(resolve, 500));
    }

    // Check if more archiving is needed
    const remainingToArchive = rowsToArchive - totalArchived;
    if (remainingToArchive > 0) {
      this.log.info(`🗄️  ⏸️  [${collection}] Cycle complete: ${totalArchived} rows archived, ${remainingToArchive} remain (will continue next cycle in 12s)`);
    } else {
      this.log.info(`🗄️  ✅ [${collection}] Archiving complete: ${totalArchived} rows (${sourceHandle} → ${destHandle})`);
    }

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
        const activeResult = await DataList.execute({
          collection: collectionName,
          dbHandle: archiveConfig.sourceHandle,
          limit: 0
        });

        // Count archived rows from destination handle (SAME collection name!)
        const archiveResult = await DataList.execute({
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
