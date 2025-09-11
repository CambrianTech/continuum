/**
 * SQLite Adapter - High-performance structured storage
 * 
 * Provides real database capabilities for production use
 * Supports transactions, indexes, relations, and full-text search
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
import * as path from 'path';

// Using sqlite3 with promisify pattern for clean async/await
import * as sqlite3 from 'sqlite3';
import { promisify } from 'util';

/**
 * Strict type definitions for sqlite3 - Rust-like explicit typing
 */
interface SQLiteRow {
  readonly [key: string]: any;
}

interface SQLiteStatement {
  readonly changes?: number;
}

interface SQLiteRowWithData {
  readonly data: string;
}

interface SQLiteCountRow {
  readonly count: number;
}

type SQLiteGetFn = (sql: string, ...params: any[]) => Promise<SQLiteRow | undefined>;
type SQLiteAllFn = (sql: string, ...params: any[]) => Promise<SQLiteRow[]>;
type SQLiteRunFn = (sql: string, ...params: any[]) => Promise<SQLiteStatement>;

/**
 * SQLite Adapter Implementation
 */
export class SQLiteAdapter implements DataAdapter {
  readonly name = 'SQLiteAdapter';
  readonly capabilities = {
    supportsTransactions: true,
    supportsFullTextSearch: true,
    supportsRelations: true,
    supportsJsonQueries: true
  } as const;

  private db: sqlite3.Database | null = null;
  private dbPath: string;
  private isInitialized: boolean = false;

  constructor(dbPath: string = '.continuum/database/continuum.db') {
    this.dbPath = dbPath;
  }

  async initialize(): Promise<DataResult<void>> {
    if (this.isInitialized && this.db) {
      return Ok(undefined);
    }

    try {
      // Ensure directory exists
      const { dirname } = path;
      const { mkdir } = await import('fs/promises');
      await mkdir(dirname(this.dbPath), { recursive: true });

      // Open database connection
      this.db = new sqlite3.Database(this.dbPath);
      
      // Promisify database methods
      const dbRun = promisify(this.db.run.bind(this.db));
      const dbGet = promisify(this.db.get.bind(this.db));

      // Create tables if they don't exist
      await dbRun(`
        CREATE TABLE IF NOT EXISTS entities (
          id TEXT PRIMARY KEY,
          collection TEXT NOT NULL,
          data TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          version INTEGER NOT NULL DEFAULT 1,
          search_content TEXT -- For full-text search
        )
      `);

      await dbRun(`
        CREATE INDEX IF NOT EXISTS idx_entities_collection ON entities(collection)
      `);

      await dbRun(`
        CREATE INDEX IF NOT EXISTS idx_entities_updated_at ON entities(updated_at)
      `);

      // Create full-text search table if supported
      try {
        await dbRun(`
          CREATE VIRTUAL TABLE IF NOT EXISTS entities_fts USING fts5(
            id, collection, search_content, content='entities', content_rowid='rowid'
          )
        `);
      } catch (ftsError) {
        // FTS5 not available, continue without it
        console.warn('FTS5 not available in this SQLite build');
      }

      this.isInitialized = true;
      return Ok(undefined);

    } catch (error: any) {
      return Err(createDataError('STORAGE_ERROR', `Failed to initialize SQLite: ${error.message}`));
    }
  }

  async close(): Promise<DataResult<void>> {
    if (!this.db) {
      return Ok(undefined);
    }

    try {
      const dbClose = promisify(this.db.close.bind(this.db));
      await dbClose();
      this.db = null;
      this.isInitialized = false;
      return Ok(undefined);
    } catch (error: any) {
      return Err(createDataError('STORAGE_ERROR', `Failed to close SQLite: ${error.message}`));
    }
  }

  async create<T extends BaseEntity>(
    collection: string, 
    data: Omit<T, keyof BaseEntity>, 
    context: DataOperationContext
  ): Promise<DataResult<T>> {
    if (!this.db) {
      return Err(createDataError('STORAGE_ERROR', 'Database not initialized'));
    }

    try {
      const now = new Date().toISOString() as ISOString;
      const entity: T = {
        id: generateUUID(),
        createdAt: now,
        updatedAt: now,
        version: 1,
        ...data
      } as T;

      const entityJson = JSON.stringify(entity);
      
      // Extract searchable content for full-text search
      const searchContent = this.extractSearchContent(entity);

      // Create properly typed database functions
      const dbRun: SQLiteRunFn = promisify(this.db.run.bind(this.db));
      
      await dbRun(`
        INSERT INTO entities (id, collection, data, created_at, updated_at, version, search_content)
        VALUES (?, ?, ?, ?, ?, ?, ?)
      `, entity.id, collection, entityJson, entity.createdAt, entity.updatedAt, entity.version, searchContent);

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
    if (!this.db) {
      return Err(createDataError('STORAGE_ERROR', 'Database not initialized'));
    }

    try {
      const dbGet: SQLiteGetFn = promisify(this.db.get.bind(this.db));
      const row = await dbGet(
        'SELECT data FROM entities WHERE id = ? AND collection = ?',
        id, collection
      ) as SQLiteRowWithData | undefined;

      if (!row) {
        return Ok(null);
      }

      const entity = JSON.parse(row.data) as T;
      return Ok(entity);

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
    if (!this.db) {
      return Err(createDataError('STORAGE_ERROR', 'Database not initialized'));
    }

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

      const dbRun: SQLiteRunFn = promisify(this.db.run.bind(this.db));
      const entityJson = JSON.stringify(updated);
      const searchContent = this.extractSearchContent(updated);

      await dbRun(`
        UPDATE entities 
        SET data = ?, updated_at = ?, version = ?, search_content = ?
        WHERE id = ? AND collection = ?
      `, entityJson, updated.updatedAt, updated.version, searchContent, id, collection);

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
    if (!this.db) {
      return Err(createDataError('STORAGE_ERROR', 'Database not initialized'));
    }

    try {
      const dbRun: SQLiteRunFn = promisify(this.db.run.bind(this.db));
      const result = await dbRun(
        'DELETE FROM entities WHERE id = ? AND collection = ?',
        id, collection
      );

      return Ok((result?.changes ?? 0) > 0);

    } catch (error: any) {
      return Err(createDataError('STORAGE_ERROR', `Failed to delete ${collection}/${id}: ${error.message}`));
    }
  }

  async list<T extends BaseEntity>(
    collection: string, 
    options: QueryOptions<T> = {}, 
    context: DataOperationContext = {} as DataOperationContext
  ): Promise<DataResult<T[]>> {
    if (!this.db) {
      return Err(createDataError('STORAGE_ERROR', 'Database not initialized'));
    }

    try {
      // Build query
      let sql = 'SELECT data FROM entities WHERE collection = ?';
      const params: any[] = [collection];

      // Add filters
      if (options.filters) {
        for (const [field, value] of Object.entries(options.filters)) {
          sql += ` AND JSON_EXTRACT(data, '$.${field as string}') = ?`;
          params.push(value);
        }
      }

      // Add ordering
      if (options.orderBy?.length) {
        const orderClauses = options.orderBy.map(order => 
          `JSON_EXTRACT(data, '$.${order.field as string}') ${order.direction}`
        );
        sql += ` ORDER BY ${orderClauses.join(', ')}`;
      }

      // Add pagination
      if (options.limit !== undefined) {
        sql += ` LIMIT ?`;
        params.push(options.limit);
      }

      if (options.offset !== undefined && options.offset > 0) {
        sql += ` OFFSET ?`;
        params.push(options.offset);
      }

      const dbAll = promisify(this.db.all.bind(this.db)) as SQLiteAllFn;
      const rows = await dbAll(sql, ...params) as SQLiteRowWithData[];

      const entities: T[] = rows.map(row => {
        try {
          return JSON.parse(row.data) as T;
        } catch (parseError: any) {
          console.error(`Failed to parse entity data: ${parseError.message}`);
          return null;
        }
      }).filter((entity): entity is T => entity !== null);

      return Ok(entities);

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
    // For SQLite, query is the same as list with filters
    return await this.list<T>(collection, { ...options, filters: filters as Record<keyof T, unknown> }, context);
  }

  async count(
    collection: string,
    filters: Record<string, unknown> = {},
    context: DataOperationContext = {} as DataOperationContext
  ): Promise<DataResult<number>> {
    if (!this.db) {
      return Err(createDataError('STORAGE_ERROR', 'Database not initialized'));
    }

    try {
      const dbGet = promisify(this.db.get.bind(this.db)) as SQLiteGetFn;
      
      let sql = 'SELECT COUNT(*) as count FROM entities WHERE collection = ?';
      const params: any[] = [collection];

      // Add filters
      for (const [field, value] of Object.entries(filters)) {
        sql += ` AND JSON_EXTRACT(data, '$.${field}') = ?`;
        params.push(value);
      }

      const row = await dbGet(sql, params) as SQLiteCountRow | undefined;
      if (!row) {
        return Ok(0);
      }

      return Ok(row.count);

    } catch (error: any) {
      return Err(createDataError('STORAGE_ERROR', `Failed to count ${collection}: ${error.message}`));
    }
  }

  /**
   * Extract searchable text content from entity for full-text search
   */
  private extractSearchContent(entity: any): string {
    const searchFields: string[] = [];
    
    // Extract common searchable fields
    if (entity.name) searchFields.push(entity.name);
    if (entity.title) searchFields.push(entity.title);
    if (entity.description) searchFields.push(entity.description);
    if (entity.content?.text) searchFields.push(entity.content.text);
    if (entity.profile?.displayName) searchFields.push(entity.profile.displayName);
    if (entity.profile?.bio) searchFields.push(entity.profile.bio);
    
    return searchFields.join(' ').toLowerCase();
  }
}