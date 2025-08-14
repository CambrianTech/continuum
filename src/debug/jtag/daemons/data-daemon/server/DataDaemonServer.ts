/**
 * Data Daemon Server - JSON File Persistence
 * 
 * Server-side implementation of data daemon using JSON files
 * Simple, fast, no SQL complexity - just documents in collections
 * Each collection is a directory with UUID.json files
 */

import * as fs from 'fs';
import * as path from 'path';
import { DataDaemon } from '../shared/DataDaemon';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import { PersistenceError } from '../../../system/core/types/ErrorTypes';
import type { 
  DataRecord,
  DataCreateParams,
  DataReadParams, 
  DataUpdateParams,
  DataDeleteParams,
  DataListParams,
  DataExistsParams,
  DataResult
} from '../shared/DataTypes';
import {
  createDataCreateResult,
  createDataReadResult,
  createDataUpdateResult,
  createDataDeleteResult,
  createDataListResult,
  createDataExistsResult
} from '../shared/DataTypes';

/**
 * Data Daemon Server - JSON File Implementation
 */
export class DataDaemonServer extends DataDaemon {
  private dataDir: string;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
    
    // Store data in .continuum/jtag/data/
    this.dataDir = path.join(process.cwd(), 'examples/test-bench/.continuum/jtag/data');
    this.ensureDataDirectory();
  }

  /**
   * Ensure data directory exists
   */
  private ensureDataDirectory(): void {
    if (!fs.existsSync(this.dataDir)) {
      fs.mkdirSync(this.dataDir, { recursive: true });
      console.log(`🗄️ ${this.toString()}: Created data directory: ${this.dataDir}`);
    }
  }

  /**
   * Get collection directory path
   */
  private getCollectionDir(collection: string): string {
    return path.join(this.dataDir, collection);
  }

  /**
   * Get record file path
   */
  private getRecordPath(collection: string, id: UUID): string {
    return path.join(this.getCollectionDir(collection), `${id}.json`);
  }

  /**
   * Ensure collection directory exists
   */
  private ensureCollection(collection: string): void {
    const collectionDir = this.getCollectionDir(collection);
    if (!fs.existsSync(collectionDir)) {
      fs.mkdirSync(collectionDir, { recursive: true });
    }
  }

  /**
   * Handle create operation - create new document
   */
  protected async handleCreate(params: DataCreateParams): Promise<DataResult> {
    try {
      this.ensureCollection(params.collection);
      
      const id = params.id || generateUUID();
      const now = new Date().toISOString();
      
      const record: DataRecord = {
        id,
        collection: params.collection,
        data: params.data,
        createdAt: now,
        updatedAt: now,
        version: 1
      };

      const filePath = this.getRecordPath(params.collection, id);
      
      // Check if already exists
      if (fs.existsSync(filePath)) {
        return createDataCreateResult(this.context, params.sessionId, {
          success: false,
          error: new PersistenceError(filePath, 'create', `Record ${id} already exists in collection ${params.collection}`)
        });
      }

      fs.writeFileSync(filePath, JSON.stringify(record, null, 2));
      
      console.log(`✅ ${this.toString()}: Created record ${id} in ${params.collection}`);
      
      return createDataCreateResult(this.context, params.sessionId, {
        success: true,
        record
      });
    } catch (error) {
      console.error(`❌ ${this.toString()}: Create failed:`, error);
      return createDataCreateResult(this.context, params.sessionId, {
        success: false,
        error: new PersistenceError(this.getRecordPath(params.collection, params.id || generateUUID()), 'create', `Create operation failed: ${(error as Error).message}`, { cause: error })
      });
    }
  }

  /**
   * Handle read operation - get document by ID
   */
  protected async handleRead(params: DataReadParams): Promise<DataResult> {
    try {
      const filePath = this.getRecordPath(params.collection, params.id);
      
      if (!fs.existsSync(filePath)) {
        return createDataReadResult(this.context, params.sessionId, {
          success: true,
          found: false
        });
      }

      const recordData = fs.readFileSync(filePath, 'utf-8');
      const record = JSON.parse(recordData) as DataRecord;
      
      return createDataReadResult(this.context, params.sessionId, {
        success: true,
        found: true,
        record
      });
    } catch (error) {
      console.error(`❌ ${this.toString()}: Read failed:`, error);
      return createDataReadResult(this.context, params.sessionId, {
        success: false,
        found: false,
        error: new PersistenceError(this.getRecordPath(params.collection, params.id), 'read', `Read operation failed: ${(error as Error).message}`, { cause: error })
      });
    }
  }

  /**
   * Handle update operation - update existing document
   */
  protected async handleUpdate(params: DataUpdateParams): Promise<DataResult> {
    try {
      const filePath = this.getRecordPath(params.collection, params.id);
      
      if (!fs.existsSync(filePath)) {
        return createDataUpdateResult(this.context, params.sessionId, {
          success: false,
          updated: false,
          error: new PersistenceError(filePath, 'read', `Record ${params.id} not found in collection ${params.collection}`)
        });
      }

      const recordData = fs.readFileSync(filePath, 'utf-8');
      const existingRecord = JSON.parse(recordData) as DataRecord;
      
      const updatedRecord: DataRecord = {
        ...existingRecord,
        data: params.merge ? { ...existingRecord.data, ...params.data } : params.data,
        updatedAt: new Date().toISOString(),
        version: existingRecord.version + 1
      };

      fs.writeFileSync(filePath, JSON.stringify(updatedRecord, null, 2));
      
      console.log(`✅ ${this.toString()}: Updated record ${params.id} in ${params.collection}`);
      
      return createDataUpdateResult(this.context, params.sessionId, {
        success: true,
        updated: true,
        record: updatedRecord
      });
    } catch (error) {
      console.error(`❌ ${this.toString()}: Update failed:`, error);
      return createDataUpdateResult(this.context, params.sessionId, {
        success: false,
        updated: false,
        error: new PersistenceError(this.getRecordPath(params.collection, params.id), 'write', `Update operation failed: ${(error as Error).message}`, { cause: error })
      });
    }
  }

  /**
   * Handle delete operation - remove document
   */
  protected async handleDelete(params: DataDeleteParams): Promise<DataResult> {
    try {
      const filePath = this.getRecordPath(params.collection, params.id);
      
      if (!fs.existsSync(filePath)) {
        return createDataDeleteResult(this.context, params.sessionId, {
          success: true,
          deleted: false
        });
      }

      fs.unlinkSync(filePath);
      
      console.log(`✅ ${this.toString()}: Deleted record ${params.id} from ${params.collection}`);
      
      return createDataDeleteResult(this.context, params.sessionId, {
        success: true,
        deleted: true
      });
    } catch (error) {
      console.error(`❌ ${this.toString()}: Delete failed:`, error);
      return createDataDeleteResult(this.context, params.sessionId, {
        success: false,
        deleted: false,
        error: new PersistenceError(this.getRecordPath(params.collection, params.id), 'delete', `Delete operation failed: ${(error as Error).message}`, { cause: error })
      });
    }
  }

  /**
   * Handle list operation - list documents in collection
   */
  protected async handleList(params: DataListParams): Promise<DataResult> {
    try {
      const collectionDir = this.getCollectionDir(params.collection);
      
      if (!fs.existsSync(collectionDir)) {
        return createDataListResult(this.context, params.sessionId, {
          success: true,
          records: [],
          totalCount: 0,
          hasMore: false
        });
      }

      const files = fs.readdirSync(collectionDir).filter(f => f.endsWith('.json'));
      const records: DataRecord[] = [];
      
      for (const file of files) {
        try {
          const filePath = path.join(collectionDir, file);
          const recordData = fs.readFileSync(filePath, 'utf-8');
          const record = JSON.parse(recordData) as DataRecord;
          
          // Apply simple filter if provided
          if (params.filter && !this.matchesFilter(record.data, params.filter)) {
            continue;
          }
          
          records.push(record);
        } catch (error) {
          console.warn(`⚠️ ${this.toString()}: Failed to read record ${file}:`, error);
        }
      }
      
      // Sort by creation date (newest first)
      records.sort((a, b) => new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime());
      
      // Apply pagination
      const offset = params.offset || 0;
      const limit = params.limit || records.length;
      const paginatedRecords = records.slice(offset, offset + limit);
      const hasMore = offset + limit < records.length;
      
      return createDataListResult(this.context, params.sessionId, {
        success: true,
        records: paginatedRecords,
        totalCount: records.length,
        hasMore
      });
    } catch (error) {
      console.error(`❌ ${this.toString()}: List failed:`, error);
      return createDataListResult(this.context, params.sessionId, {
        success: false,
        records: [],
        totalCount: 0,
        hasMore: false,
        error: new PersistenceError(this.getCollectionDir(params.collection), 'read', `List operation failed: ${(error as Error).message}`, { cause: error })
      });
    }
  }

  /**
   * Handle exists operation - check if document exists
   */
  protected async handleExists(params: DataExistsParams): Promise<DataResult> {
    try {
      const filePath = this.getRecordPath(params.collection, params.id);
      const exists = fs.existsSync(filePath);
      
      return createDataExistsResult(this.context, params.sessionId, {
        success: true,
        exists
      });
    } catch (error) {
      console.error(`❌ ${this.toString()}: Exists check failed:`, error);
      return createDataExistsResult(this.context, params.sessionId, {
        success: false,
        exists: false,
        error: new PersistenceError(this.getRecordPath(params.collection, params.id), 'read', `Exists check failed: ${(error as Error).message}`, { cause: error })
      });
    }
  }

  /**
   * Simple filter matching (basic key-value equality)
   */
  private matchesFilter(data: any, filter: Record<string, any>): boolean {
    for (const [key, value] of Object.entries(filter)) {
      if (data[key] !== value) {
        return false;
      }
    }
    return true;
  }
}