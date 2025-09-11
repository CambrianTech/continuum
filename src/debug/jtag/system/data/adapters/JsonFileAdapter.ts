/**
 * JSON File Adapter - Legacy compatibility for existing .continuum/database/*.json files
 * 
 * Maintains backward compatibility with existing JSON storage
 * Handles the current file-based storage pattern used by DataDaemon
 */

import type { 
  BaseEntity,
  DataResult,
  DataError,
  DataOperationContext,
  QueryOptions,
  ISOString
} from '../domains/CoreTypes';
import { Ok, Err, createDataError } from '../domains/CoreTypes';
import { DataAdapter } from '../services/DataService';
import { generateUUID } from '../../core/types/CrossPlatformUUID';
import * as fs from 'fs/promises';
import * as path from 'path';

/**
 * JSON File Adapter Implementation
 */
export class JsonFileAdapter implements DataAdapter {
  readonly name = 'JsonFileAdapter';
  readonly capabilities = {
    supportsTransactions: false,
    supportsFullTextSearch: false,
    supportsRelations: false,
    supportsJsonQueries: true
  } as const;

  private databasePath: string;

  constructor(databasePath: string = '.continuum/database') {
    this.databasePath = databasePath;
  }

  async initialize(): Promise<DataResult<void>> {
    try {
      await fs.mkdir(this.databasePath, { recursive: true });
      return Ok(undefined);
    } catch (error: any) {
      return Err(createDataError('STORAGE_ERROR', `Failed to initialize JSON database: ${error.message}`));
    }
  }

  async close(): Promise<DataResult<void>> {
    // JSON files don't need cleanup
    return Ok(undefined);
  }

  async create<T extends BaseEntity>(
    collection: string, 
    data: Omit<T, keyof BaseEntity>, 
    context: DataOperationContext
  ): Promise<DataResult<T>> {
    try {
      const now = new Date().toISOString() as ISOString;
      const entity: T = {
        id: generateUUID(),
        createdAt: now,
        updatedAt: now,
        version: 1,
        ...data
      } as T;

      const filePath = path.join(this.databasePath, collection, `${entity.id}.json`);
      
      // Ensure collection directory exists
      await fs.mkdir(path.dirname(filePath), { recursive: true });
      
      // Write entity to individual file (current DataDaemon pattern)
      const fileData = {
        id: entity.id,
        collection,
        data: entity,
        metadata: {
          createdAt: entity.createdAt,
          updatedAt: entity.updatedAt,
          version: entity.version
        }
      };
      
      await fs.writeFile(filePath, JSON.stringify(fileData, null, 2), 'utf-8');
      return Ok(entity);

    } catch (error: any) {
      return Err(createDataError('STORAGE_ERROR', `Failed to create ${collection} record: ${error.message}`));
    }
  }

  async read<T extends BaseEntity>(
    collection: string, 
    id: string, 
    context: DataOperationContext
  ): Promise<DataResult<T | null>> {
    try {
      const filePath = path.join(this.databasePath, collection, `${id}.json`);
      
      try {
        const fileContent = await fs.readFile(filePath, 'utf-8');
        const fileData = JSON.parse(fileContent);
        return Ok(fileData.data as T);
      } catch (readError: any) {
        if (readError.code === 'ENOENT') {
          return Ok(null); // Not found is valid
        }
        throw readError;
      }

    } catch (error: any) {
      return Err(createDataError('STORAGE_ERROR', `Failed to read ${collection}/${id}: ${error.message}`));
    }
  }

  async update<T extends BaseEntity>(
    collection: string, 
    id: string, 
    data: Partial<T>, 
    context: DataOperationContext
  ): Promise<DataResult<T>> {
    try {
      // Read existing record
      const readResult = await this.read<T>(collection, id, context);
      if (!readResult.success) {
        return readResult;
      }

      if (!readResult.data) {
        return Err(createDataError('NOT_FOUND', `${collection}/${id} not found for update`));
      }

      const existing = readResult.data;
      const now = new Date().toISOString() as ISOString;
      
      const updated: T = {
        ...existing,
        ...data,
        updatedAt: now,
        version: existing.version + 1
      };

      // Write updated entity
      const filePath = path.join(this.databasePath, collection, `${id}.json`);
      const fileData = {
        id: updated.id,
        collection,
        data: updated,
        metadata: {
          createdAt: updated.createdAt,
          updatedAt: updated.updatedAt,
          version: updated.version
        }
      };

      await fs.writeFile(filePath, JSON.stringify(fileData, null, 2), 'utf-8');
      return Ok(updated);

    } catch (error: any) {
      return Err(createDataError('STORAGE_ERROR', `Failed to update ${collection}/${id}: ${error.message}`));
    }
  }

  async delete(
    collection: string, 
    id: string, 
    context: DataOperationContext
  ): Promise<DataResult<boolean>> {
    try {
      const filePath = path.join(this.databasePath, collection, `${id}.json`);
      
      try {
        await fs.unlink(filePath);
        return Ok(true);
      } catch (deleteError: any) {
        if (deleteError.code === 'ENOENT') {
          return Ok(false); // Already deleted
        }
        throw deleteError;
      }

    } catch (error: any) {
      return Err(createDataError('STORAGE_ERROR', `Failed to delete ${collection}/${id}: ${error.message}`));
    }
  }

  async list<T extends BaseEntity>(
    collection: string, 
    options: QueryOptions<T> = {}, 
    context: DataOperationContext = {} as DataOperationContext
  ): Promise<DataResult<T[]>> {
    try {
      const collectionPath = path.join(this.databasePath, collection);
      
      try {
        const files = await fs.readdir(collectionPath);
        const jsonFiles = files.filter(f => f.endsWith('.json'));
        
        const entities: T[] = [];
        
        for (const file of jsonFiles) {
          const filePath = path.join(collectionPath, file);
          try {
            const fileContent = await fs.readFile(filePath, 'utf-8');
            const fileData = JSON.parse(fileContent);
            entities.push(fileData.data as T);
          } catch (parseError) {
            // Skip malformed files
            continue;
          }
        }

        // Apply basic filtering and sorting
        let filtered = entities;

        // Apply filters if provided
        if (options.filters) {
          filtered = entities.filter(entity => {
            for (const [field, value] of Object.entries(options.filters!)) {
              if ((entity as any)[field] !== value) {
                return false;
              }
            }
            return true;
          });
        }

        // Apply sorting if provided
        if (options.orderBy?.length) {
          filtered.sort((a, b) => {
            for (const sort of options.orderBy!) {
              const aVal = (a as any)[sort.field];
              const bVal = (b as any)[sort.field];
              
              if (aVal < bVal) return sort.direction === 'ASC' ? -1 : 1;
              if (aVal > bVal) return sort.direction === 'ASC' ? 1 : -1;
            }
            return 0;
          });
        }

        // Apply pagination
        const offset = options.offset || 0;
        const limit = options.limit;
        
        if (limit !== undefined) {
          filtered = filtered.slice(offset, offset + limit);
        } else if (offset > 0) {
          filtered = filtered.slice(offset);
        }

        return Ok(filtered);

      } catch (dirError: any) {
        if (dirError.code === 'ENOENT') {
          return Ok([]); // Collection doesn't exist yet
        }
        throw dirError;
      }

    } catch (error: any) {
      return Err(createDataError('STORAGE_ERROR', `Failed to list ${collection}: ${error.message}`));
    }
  }

  async query<T extends BaseEntity>(
    collection: string,
    filters: Record<string, unknown>,
    options: QueryOptions<T> = {},
    context: DataOperationContext = {} as DataOperationContext
  ): Promise<DataResult<T[]>> {
    // For JSON files, query is the same as list with filters
    return await this.list<T>(collection, { ...options, filters }, context);
  }

  async count(
    collection: string,
    filters: Record<string, unknown> = {},
    context: DataOperationContext = {} as DataOperationContext
  ): Promise<DataResult<number>> {
    try {
      const listResult = await this.query(collection, filters, {}, context);
      if (!listResult.success) {
        return listResult as DataResult<number>;
      }
      return Ok(listResult.data.length);
    } catch (error: any) {
      return Err(createDataError('STORAGE_ERROR', `Failed to count ${collection}: ${error.message}`));
    }
  }
}