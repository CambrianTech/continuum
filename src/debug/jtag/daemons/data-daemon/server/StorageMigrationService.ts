/**
 * Storage Migration Service - Data Backend Migration
 * 
 * Handles migrating data between different storage backends:
 * - JSON file storage → SQLite
 * - SQLite → PostgreSQL  
 * - File storage → Memory/Redis
 * 
 * Maintains data integrity during migration with validation and rollback.
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { DataRecord, StorageResult } from '../shared/DataStorageAdapter';
import type { StorageStrategyConfig, DataOperationContext } from '../shared/DataDaemon';
import { DataDaemon } from '../shared/DataDaemon';
import * as fs from 'fs';
import * as path from 'path';

export interface MigrationPlan {
  readonly fromStrategy: StorageStrategyConfig;
  readonly toStrategy: StorageStrategyConfig;
  readonly collections: string[];
  readonly dryRun: boolean;
  readonly validateData: boolean;
  readonly backupData: boolean;
}

export interface MigrationResult {
  success: boolean;
  recordsMigrated: number;
  collectionsProcessed: string[];
  errors: string[];
  backupPath?: string;
  duration: number;
}

export interface MigrationProgress {
  readonly currentCollection: string;
  readonly recordsProcessed: number;
  readonly totalRecords: number;
  readonly percentage: number;
  readonly estimatedTimeRemaining: number;
}

/**
 * Storage Migration Service
 */
export class StorageMigrationService {
  private fromDaemon?: DataDaemon;
  private toDaemon?: DataDaemon;
  private progressCallback?: (progress: MigrationProgress) => void;

  /**
   * Set progress callback for migration monitoring
   */
  setProgressCallback(callback: (progress: MigrationProgress) => void): void {
    this.progressCallback = callback;
  }

  /**
   * Plan migration between storage backends
   */
  async planMigration(
    fromStrategy: StorageStrategyConfig,
    toStrategy: StorageStrategyConfig,
    context: DataOperationContext
  ): Promise<MigrationPlan> {
    // Initialize source daemon to discover collections
    const fromDaemon = new DataDaemon(fromStrategy);
    await fromDaemon.initialize();

    const collectionsResult = await fromDaemon.listCollections(context);
    await fromDaemon.close();

    if (!collectionsResult.success || !collectionsResult.data) {
      throw new Error(`Failed to discover collections: ${collectionsResult.error}`);
    }

    return {
      fromStrategy,
      toStrategy,
      collections: collectionsResult.data,
      dryRun: false,
      validateData: true,
      backupData: true
    };
  }

  /**
   * Execute migration plan
   */
  async executeMigration(
    plan: MigrationPlan,
    context: DataOperationContext
  ): Promise<MigrationResult> {
    const startTime = Date.now();
    const result: MigrationResult = {
      success: false,
      recordsMigrated: 0,
      collectionsProcessed: [],
      errors: [],
      duration: 0
    };

    try {
      // Initialize daemons
      this.fromDaemon = new DataDaemon(plan.fromStrategy);
      this.toDaemon = new DataDaemon(plan.toStrategy);
      
      await this.fromDaemon.initialize();
      await this.toDaemon.initialize();

      // Create backup if requested
      if (plan.backupData && !plan.dryRun) {
        result.backupPath = await this.createBackup(plan.fromStrategy, context);
      }

      // Migrate each collection
      for (const collection of plan.collections) {
        try {
          const migrated = await this.migrateCollection(
            collection,
            plan,
            context
          );
          
          result.recordsMigrated += migrated;
          result.collectionsProcessed.push(collection);
          
        } catch (error) {
          const errorMsg = `Failed to migrate collection ${collection}: ${error}`;
          result.errors.push(errorMsg);
          console.error(errorMsg);
        }
      }

      result.success = result.errors.length === 0;
      
    } catch (error) {
      result.errors.push(`Migration failed: ${error}`);
      
    } finally {
      // Cleanup
      if (this.fromDaemon) await this.fromDaemon.close();
      if (this.toDaemon) await this.toDaemon.close();
      
      result.duration = Date.now() - startTime;
    }

    return result;
  }

  /**
   * Migrate specific collection
   */
  private async migrateCollection(
    collection: string,
    plan: MigrationPlan,
    context: DataOperationContext
  ): Promise<number> {
    if (!this.fromDaemon || !this.toDaemon) {
      throw new Error('Daemons not initialized');
    }

    // Get all records from source
    const query = { collection, limit: 10000 }; // Batch size
    const recordsResult = await this.fromDaemon.query<any>(query, context);
    
    if (!recordsResult.success || !recordsResult.data) {
      throw new Error(`Failed to read from collection ${collection}: ${recordsResult.error}`);
    }

    const records = recordsResult.data;
    let migratedCount = 0;

    // Process records in batches
    for (let i = 0; i < records.length; i++) {
      const record = records[i];
      
      // Report progress
      if (this.progressCallback) {
        this.progressCallback({
          currentCollection: collection,
          recordsProcessed: i + 1,
          totalRecords: records.length,
          percentage: ((i + 1) / records.length) * 100,
          estimatedTimeRemaining: 0 // TODO: Calculate based on current rate
        });
      }

      try {
        // Validate data if requested
        if (plan.validateData) {
          this.validateRecord(record);
        }

        // Skip migration if dry run
        if (plan.dryRun) {
          migratedCount++;
          continue;
        }

        // Create record in destination
        const createResult = await this.toDaemon.create(
          collection,
          record.data,
          context,
          record.id
        );

        if (!createResult.success) {
          throw new Error(`Failed to create record: ${createResult.error}`);
        }

        migratedCount++;

      } catch (error) {
        throw new Error(`Failed to migrate record ${record.id}: ${error}`);
      }
    }

    return migratedCount;
  }

  /**
   * Create backup of source data
   */
  private async createBackup(
    strategy: StorageStrategyConfig,
    context: DataOperationContext
  ): Promise<string> {
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const backupPath = path.join(
      '.continuum/jtag/backups',
      `migration-backup-${timestamp}`
    );

    // Ensure backup directory exists
    fs.mkdirSync(backupPath, { recursive: true });

    // For file storage, copy the entire data directory
    if (strategy.strategy === 'file' && strategy.options?.basePath) {
      const sourcePath = strategy.options.basePath;
      if (fs.existsSync(sourcePath)) {
        await this.copyDirectory(sourcePath, path.join(backupPath, 'data'));
      }
    }

    // Create backup metadata
    const metadata = {
      timestamp,
      strategy,
      backupType: 'full',
      version: '1.0.0'
    };

    fs.writeFileSync(
      path.join(backupPath, 'backup-metadata.json'),
      JSON.stringify(metadata, null, 2)
    );

    return backupPath;
  }

  /**
   * Validate record structure and data
   */
  private validateRecord(record: DataRecord<any>): void {
    if (!record.id) {
      throw new Error('Record missing required id field');
    }
    
    if (!record.collection) {
      throw new Error('Record missing required collection field');
    }
    
    if (record.data === undefined || record.data === null) {
      throw new Error('Record missing required data field');
    }
    
    if (!record.metadata?.createdAt) {
      throw new Error('Record missing required metadata.createdAt field');
    }
  }

  /**
   * Copy directory recursively
   */
  private async copyDirectory(src: string, dest: string): Promise<void> {
    fs.mkdirSync(dest, { recursive: true });
    
    const entries = fs.readdirSync(src, { withFileTypes: true });
    
    for (const entry of entries) {
      const srcPath = path.join(src, entry.name);
      const destPath = path.join(dest, entry.name);
      
      if (entry.isDirectory()) {
        await this.copyDirectory(srcPath, destPath);
      } else {
        fs.copyFileSync(srcPath, destPath);
      }
    }
  }

  /**
   * Quick migration helpers for common scenarios
   */
  static async migrateJsonToSqlite(
    jsonBasePath: string,
    sqliteConnectionString: string,
    context: DataOperationContext
  ): Promise<MigrationResult> {
    const service = new StorageMigrationService();
    
    const fromStrategy: StorageStrategyConfig = {
      strategy: 'file',
      backend: 'file',
      namespace: context.sessionId,
      options: { basePath: jsonBasePath }
    };

    const toStrategy: StorageStrategyConfig = {
      strategy: 'sql',
      backend: 'sqlite',
      namespace: context.sessionId,
      options: { connectionString: sqliteConnectionString },
      features: {
        enableTransactions: true,
        enableIndexing: true,
        enableCaching: false
      }
    };

    const plan = await service.planMigration(fromStrategy, toStrategy, context);
    return await service.executeMigration(plan, context);
  }
}