/**
 * SingleJsonFileAdapter - JSON file storage for singleton records
 *
 * Unlike JsonFileStorageAdapter (one JSON file per collection with array of records),
 * this stores a single record directly in a JSON file.
 *
 * Perfect for:
 * - logging_config.json - single config object
 * - system_config.json - single system config
 * - Per-persona configs
 *
 * Usage:
 *   DataDaemon.registerCollectionAdapter('logging_config', new SingleJsonFileAdapter({
 *     filePath: '.continuum/logging.json'
 *   }));
 */

import * as fs from 'fs';
import * as path from 'path';
import {
  DataStorageAdapter,
  type DataRecord,
  type StorageQuery,
  type StorageResult,
  type StorageAdapterConfig,
  type CollectionStats,
  type StorageOperation,
  type RecordData,
  type QueryExplanation,
  type CollectionSchema
} from '../shared/DataStorageAdapter';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

/**
 * Configuration for SingleJsonFileAdapter
 */
export interface SingleJsonFileConfig {
  /** Path to the JSON file (relative to cwd or absolute) */
  filePath: string;
  /** ID to use for the singleton record (default: 'singleton') */
  singletonId?: string;
  /** Collection name for this file */
  collection: string;
  /** Pretty print JSON (default: true) */
  prettyPrint?: boolean;
}

/**
 * SingleJsonFileAdapter - One JSON file = One record
 *
 * The JSON file contains the record data directly (not wrapped in array).
 * This allows natural editing of the JSON file.
 */
export class SingleJsonFileAdapter extends DataStorageAdapter {
  private config!: SingleJsonFileConfig;
  private filePath!: string;
  private singletonId!: string;
  private collection!: string;
  private prettyPrint: boolean = true;
  private lastModified: number = 0;
  private cachedData: RecordData | null = null;

  async initialize(config: StorageAdapterConfig): Promise<void> {
    const options = config.options as unknown as SingleJsonFileConfig;
    this.config = options;
    this.filePath = path.isAbsolute(options.filePath)
      ? options.filePath
      : path.join(process.cwd(), options.filePath);
    this.singletonId = options.singletonId || 'singleton';
    this.collection = options.collection;
    this.prettyPrint = options.prettyPrint !== false;

    // Ensure directory exists
    const dir = path.dirname(this.filePath);
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }
  }

  /**
   * Load data from JSON file with hot-reload support
   */
  private loadData(): RecordData | null {
    try {
      if (!fs.existsSync(this.filePath)) {
        return null;
      }

      const stats = fs.statSync(this.filePath);
      if (stats.mtimeMs > this.lastModified) {
        // File changed, reload
        const content = fs.readFileSync(this.filePath, 'utf-8');
        this.cachedData = JSON.parse(content);
        this.lastModified = stats.mtimeMs;
      }

      return this.cachedData;
    } catch (error) {
      console.warn(`SingleJsonFileAdapter: Failed to load ${this.filePath}:`, error);
      return null;
    }
  }

  /**
   * Save data to JSON file
   */
  private saveData(data: RecordData): void {
    const content = this.prettyPrint
      ? JSON.stringify(data, null, 2)
      : JSON.stringify(data);

    fs.writeFileSync(this.filePath, content, 'utf-8');

    const stats = fs.statSync(this.filePath);
    this.lastModified = stats.mtimeMs;
    this.cachedData = data;
  }

  /**
   * Wrap raw data into DataRecord format
   */
  private wrapAsRecord<T extends RecordData>(data: T): DataRecord<T> {
    return {
      id: this.singletonId as UUID,
      collection: this.collection,
      data,
      metadata: {
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        version: 1
      }
    };
  }

  async create<T extends RecordData>(record: DataRecord<T>): Promise<StorageResult<DataRecord<T>>> {
    const existing = this.loadData();
    if (existing) {
      return {
        success: false,
        error: `Singleton record already exists in ${this.filePath}`
      };
    }

    this.saveData(record.data);

    return {
      success: true,
      data: record
    };
  }

  async read<T extends RecordData>(collection: string, id: UUID): Promise<StorageResult<DataRecord<T>>> {
    if (id !== this.singletonId) {
      return {
        success: false,
        error: `SingleJsonFileAdapter only supports singleton id '${this.singletonId}', got '${id}'`
      };
    }

    const data = this.loadData();
    if (!data) {
      return {
        success: false,
        error: `No data found in ${this.filePath}`
      };
    }

    return {
      success: true,
      data: this.wrapAsRecord(data as T)
    };
  }

  async query<T extends RecordData>(query: StorageQuery): Promise<StorageResult<DataRecord<T>[]>> {
    const data = this.loadData();
    if (!data) {
      return {
        success: true,
        data: []
      };
    }

    // Return the singleton record in an array
    return {
      success: true,
      data: [this.wrapAsRecord(data as T)]
    };
  }

  async update<T extends RecordData>(collection: string, id: UUID, data: Partial<T>, incrementVersion = true): Promise<StorageResult<DataRecord<T>>> {
    if (id !== this.singletonId) {
      return {
        success: false,
        error: `SingleJsonFileAdapter only supports singleton id '${this.singletonId}', got '${id}'`
      };
    }

    const existing = this.loadData() || {};
    const merged = { ...existing, ...data };
    this.saveData(merged);

    return {
      success: true,
      data: this.wrapAsRecord(merged as T)
    };
  }

  async delete(collection: string, id: UUID): Promise<StorageResult<boolean>> {
    if (id !== this.singletonId) {
      return {
        success: false,
        error: `SingleJsonFileAdapter only supports singleton id '${this.singletonId}'`
      };
    }

    try {
      if (fs.existsSync(this.filePath)) {
        fs.unlinkSync(this.filePath);
      }
      this.cachedData = null;
      this.lastModified = 0;

      return { success: true, data: true };
    } catch (error) {
      return {
        success: false,
        error: `Failed to delete ${this.filePath}: ${error}`
      };
    }
  }

  async listCollections(): Promise<StorageResult<string[]>> {
    return {
      success: true,
      data: [this.collection]
    };
  }

  async getCollectionStats(collection: string): Promise<StorageResult<CollectionStats>> {
    const data = this.loadData();
    let fileSize = 0;
    try {
      const stats = fs.statSync(this.filePath);
      fileSize = stats.size;
    } catch {
      // File doesn't exist
    }

    return {
      success: true,
      data: {
        name: this.collection,
        recordCount: data ? 1 : 0,
        totalSize: fileSize,
        indices: [],
        lastModified: new Date().toISOString()
      }
    };
  }

  async batch<T extends RecordData>(operations: StorageOperation<T>[]): Promise<StorageResult<unknown[]>> {
    const results: unknown[] = [];

    for (const op of operations) {
      switch (op.type) {
        case 'create':
          results.push(await this.create(this.wrapAsRecord(op.data!)));
          break;
        case 'update':
          results.push(await this.update(this.collection, op.id!, op.data!, true));
          break;
        case 'delete':
          results.push(await this.delete(this.collection, op.id!));
          break;
      }
    }

    return { success: true, data: results };
  }

  async ensureSchema(_collection: string, _schema?: CollectionSchema): Promise<StorageResult<boolean>> {
    return { success: true, data: true };
  }

  async clear(): Promise<StorageResult<boolean>> {
    return this.delete(this.collection, this.singletonId as UUID);
  }

  async clearAll(): Promise<StorageResult<{ tablesCleared: string[]; recordsDeleted: number }>> {
    const had = this.loadData() !== null;
    await this.clear();
    return {
      success: true,
      data: {
        tablesCleared: [this.collection],
        recordsDeleted: had ? 1 : 0
      }
    };
  }

  async truncate(collection: string): Promise<StorageResult<boolean>> {
    return this.clear();
  }

  async cleanup(): Promise<void> {
    // Nothing to clean up
  }

  async close(): Promise<void> {
    this.cachedData = null;
  }

  async explainQuery(query: StorageQuery): Promise<QueryExplanation> {
    return {
      query,
      translatedQuery: `READ ${this.filePath}`,
      parameters: [],
      estimatedRows: 1,
      executionPlan: `Single JSON file read from ${this.filePath}`,
      adapterType: 'single-json-file',
      timestamp: new Date().toISOString()
    };
  }
}
