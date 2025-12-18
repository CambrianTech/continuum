/**
 * Comms Test Daemon - THROWAWAY Testing Only
 *
 * Server-specific implementation with Rust adapter database testing!
 */

import { JTAGContext } from '../../../system/core/types/JTAGTypes';
import { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { CommsTestDaemon, type DatabaseTestParams, type DatabaseTestResult } from '../shared/CommsTestDaemon';
import { Logger } from '../../../system/core/logging/Logger';
import { RustAdapter } from '../../data-daemon/server/RustAdapter';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import path from 'path';
import fs from 'fs';

export class CommsTestDaemonServer extends CommsTestDaemon {
  private adapters: Map<string, RustAdapter> = new Map();

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    // Initialize standardized logging (daemons/ subdirectory) - overrides base class log
    const className = this.constructor.name;
    this.log = Logger.create(className, `daemons/${className}`);

    // Set singleton instance when system creates daemon
    instance = this;
  }

  /**
   * Server-specific initialization
   */
  protected override async initialize(): Promise<void> {
    await super.initialize();
    this.log.info(`🧪 ${this.toString()}: Server comms test daemon ready`);
  }

  /**
   * Override echo method with logging
   */
  override async testEcho(message: string): Promise<{ success: boolean; echo: string }> {
    this.log.info(`Echo test: ${message}`);
    return await super.testEcho(message);
  }

  /**
   * Test database operations with Rust adapter
   * Creates multiple concurrent database connections and performs operations
   */
  async testDatabase(params: DatabaseTestParams): Promise<DatabaseTestResult> {
    const startTime = Date.now();
    this.log.info(`🦀 Starting database test: ${params.dbCount} databases, ${params.operations} operations each`);

    // Ensure test directory exists
    const testDir = path.resolve(process.cwd(), params.testDir);
    if (!fs.existsSync(testDir)) {
      fs.mkdirSync(testDir, { recursive: true });
      this.log.info(`📁 Created test directory: ${testDir}`);
    }

    const results: DatabaseTestResult['databases'] = [];

    // Test each database concurrently
    const dbPromises = Array.from({ length: params.dbCount }, async (_, index) => {
      const dbStart = Date.now();
      const handle = `test-db-${index}`;
      const dbPath = path.join(testDir, `test-${index}.sqlite`);

      try {
        this.log.info(`🔧 [${handle}] Opening database: ${dbPath}`);

        // Create adapter
        const adapter = new RustAdapter();

        // Initialize with config
        await adapter.initialize({
          type: 'rust',
          namespace: `test-${index}`,
          options: {
            filename: dbPath,
            mode: 'create' as const,
            storageType: 'auto-detect'
          }
        });

        this.adapters.set(handle, adapter);

        // Perform operations
        this.log.info(`📝 [${handle}] Performing ${params.operations} operations`);

        for (let op = 0; op < params.operations; op++) {
          const recordId = generateUUID();
          const testData = {
            index: op,
            timestamp: new Date().toISOString(),
            message: `Test operation ${op} on database ${index}`
          };

          // Create DataRecord
          const record = {
            id: recordId,
            collection: 'test_records',
            data: testData,
            metadata: {
              createdAt: new Date().toISOString(),
              updatedAt: new Date().toISOString(),
              version: 1
            }
          };

          // Create record
          const createResult = await adapter.create(record);
          if (!createResult.success || !createResult.data) {
            throw new Error(`Failed to create record ${recordId}: ${createResult.error}`);
          }

          // Read it back
          const readResult = await adapter.read('test_records', recordId);
          if (!readResult.success || !readResult.data || readResult.data.id !== recordId) {
            throw new Error(`Failed to read back record ${recordId}`);
          }
        }

        const duration = Date.now() - dbStart;
        this.log.info(`✅ [${handle}] Completed in ${duration}ms`);

        return {
          handle,
          path: dbPath,
          operations: params.operations,
          duration,
          success: true
        };
      } catch (error) {
        const duration = Date.now() - dbStart;
        const errorMsg = error instanceof Error ? error.message : String(error);
        this.log.error(`❌ [${handle}] Failed: ${errorMsg}`);

        return {
          handle,
          path: dbPath,
          operations: 0,
          duration,
          success: false,
          error: errorMsg
        };
      }
    });

    // Wait for all databases to complete
    const dbResults = await Promise.all(dbPromises);
    results.push(...dbResults);

    const totalDuration = Date.now() - startTime;
    const totalOperations = results.reduce((sum, r) => sum + r.operations, 0);
    const allSuccess = results.every(r => r.success);

    this.log.info(`🏁 Test complete: ${totalOperations} operations in ${totalDuration}ms`);

    return {
      success: allSuccess,
      databases: results,
      totalDuration,
      totalOperations
    };
  }

  /**
   * Cleanup adapters
   */
  override async shutdown(): Promise<void> {
    this.log.info(`🧹 Closing ${this.adapters.size} adapters...`);

    // RustAdapter doesn't have explicit shutdown - just clear the map
    this.adapters.clear();
    await super.shutdown();
  }
}

// Singleton instance
let instance: CommsTestDaemonServer | null = null;

export function initializeCommsTestDaemon(context: JTAGContext, router: JTAGRouter): CommsTestDaemonServer {
  if (instance) {
    return instance;
  }

  instance = new CommsTestDaemonServer(context, router);
  return instance;
}

export function getCommsTestDaemon(): CommsTestDaemonServer {
  if (!instance) {
    throw new Error('CommsTestDaemon not initialized');
  }
  return instance;
}
