/**
 * DatabaseDaemon - Universal Data Persistence and Management System
 * =================================================================
 * 
 * Manages all data persistence for Continuum including chat messages, room data,
 * session information, user preferences, and system configuration. Provides
 * unified storage interface for all other daemons.
 * 
 * CRITICAL TESTING REQUIREMENTS:
 * ===============================
 * INTEGRATION TEST COVERAGE NEEDED:
 * - Data persistence: Save/load operations across daemon restarts
 * - Query performance: Efficient data retrieval with indexing
 * - Transaction integrity: ACID compliance for critical operations
 * - Backup and recovery: Data protection and restoration capabilities
 * - Schema migration: Version updates without data loss
 * - Concurrent access: Multi-daemon read/write coordination
 * 
 * LOGGING STRATEGY FOR FAILURE DETECTION:
 * - Database operation timing and success rates
 * - Storage capacity monitoring and cleanup operations
 * - Data integrity validation with checksum verification
 * - Query optimization and performance metrics
 * - Backup completion and recovery testing
 * 
 * Architecture Integration:
 * - Serves ChatRoomDaemon for message and room persistence
 * - Serves SessionManagerDaemon for session state storage
 * - Serves PersonaDaemon for LoRA model and training data storage
 * - Integrates with ContinuumDirectoryDaemon for file-based storage
 * 
 * CRITICAL TODO LIST:
 * ===================
 * MODULARITY ISSUES:
 * - Storage backend should be pluggable (SQLite, PostgreSQL, etc.)
 * - Schema definitions should be separate modules per data type
 * - Query builder should use proper ORM or query abstraction
 * 
 * MISSING FUNCTIONALITY:
 * - Unit tests for all database operations
 * - Integration tests for concurrent access
 * - Performance testing with large datasets
 * - Backup and recovery automation
 * 
 * PERFORMANCE CONCERNS:
 * - Query optimization and indexing strategy
 * - Connection pooling for high-concurrency scenarios
 * - Data archiving for long-term storage efficiency
 */

import { RequestResponseDaemon, RequestHandlerMap } from '../base/RequestResponseDaemon';
import { DaemonType } from '../base/DaemonTypes';
import * as fs from 'fs/promises';
import * as path from 'path';

// Database operation types
interface DatabaseRecord {
  id: string;
  table: string;
  data: Record<string, any>;
  created_at: Date;
  updated_at: Date;
}

interface QueryOptions {
  limit?: number;
  offset?: number;
  orderBy?: string;
  orderDirection?: 'ASC' | 'DESC';
  where?: Record<string, any>;
}

export class DatabaseDaemon extends RequestResponseDaemon {
  private dataDirectory: string = '';
  private tables: Map<string, Map<string, DatabaseRecord>> = new Map();
  private indexes: Map<string, Map<string, Set<string>>> = new Map();

  public readonly name = 'database';
  public readonly version = '1.0.0';
  public readonly daemonType = DaemonType.DATABASE;

  constructor() {
    super();
  }

  getRequestHandlers(): RequestHandlerMap {
    return this.defineRequestHandlers();
  }

  protected defineRequestHandlers(): RequestHandlerMap {
    return {
      'save_record': this.handleSaveRecord.bind(this),
      'get_record': this.handleGetRecord.bind(this),
      'query_records': this.handleQueryRecords.bind(this),
      'update_record': this.handleUpdateRecord.bind(this),
      'delete_record': this.handleDeleteRecord.bind(this),
      'create_table': this.handleCreateTable.bind(this),
      'list_tables': this.handleListTables.bind(this),
      'backup_data': this.handleBackupData.bind(this),
      'restore_data': this.handleRestoreData.bind(this)
    };
  }

  private async handleSaveRecord(params: any): Promise<any> {
    const { table, data, id } = params;
    
    if (!table || !data) {
      throw new Error('Table and data are required');
    }

    // Ensure table exists
    if (!this.tables.has(table)) {
      await this.handleCreateTable({ table });
    }

    const recordId = id || `${table}_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    const now = new Date();

    const record: DatabaseRecord = {
      id: recordId,
      table,
      data,
      created_at: now,
      updated_at: now
    };

    const tableData = this.tables.get(table)!;
    tableData.set(recordId, record);

    // Update indexes
    await this.updateIndexes(table, recordId, data);

    // Persist to file
    await this.persistTable(table);

    this.log(`💾 Record saved in ${table}: ${recordId}`);

    return {
      success: true,
      record_id: recordId,
      record: record
    };
  }

  private async handleGetRecord(params: any): Promise<any> {
    const { table, id } = params;
    
    if (!table || !id) {
      throw new Error('Table and record ID are required');
    }

    const tableData = this.tables.get(table);
    if (!tableData) {
      throw new Error(`Table ${table} not found`);
    }

    const record = tableData.get(id);
    if (!record) {
      throw new Error(`Record ${id} not found in table ${table}`);
    }

    return {
      success: true,
      record: record
    };
  }

  private async handleQueryRecords(params: any): Promise<any> {
    const { table, options = {} }: { table: string; options: QueryOptions } = params;
    
    if (!table) {
      throw new Error('Table is required');
    }

    const tableData = this.tables.get(table);
    if (!tableData) {
      throw new Error(`Table ${table} not found`);
    }

    let records = Array.from(tableData.values());

    // Apply where clause
    if (options.where) {
      records = records.filter(record => {
        return Object.entries(options.where!).every(([key, value]) => {
          return record.data[key] === value;
        });
      });
    }

    // Apply ordering
    if (options.orderBy) {
      records.sort((a, b) => {
        const aVal = a.data[options.orderBy!] || a[options.orderBy! as keyof DatabaseRecord];
        const bVal = b.data[options.orderBy!] || b[options.orderBy! as keyof DatabaseRecord];
        
        if (options.orderDirection === 'DESC') {
          return bVal > aVal ? 1 : -1;
        }
        return aVal > bVal ? 1 : -1;
      });
    }

    // Apply pagination
    const offset = options.offset || 0;
    const limit = options.limit || records.length;
    const paginatedRecords = records.slice(offset, offset + limit);

    return {
      success: true,
      records: paginatedRecords,
      total_count: records.length,
      table: table
    };
  }

  private async handleUpdateRecord(params: any): Promise<any> {
    const { table, id, data } = params;
    
    if (!table || !id || !data) {
      throw new Error('Table, record ID, and data are required');
    }

    const tableData = this.tables.get(table);
    if (!tableData) {
      throw new Error(`Table ${table} not found`);
    }

    const record = tableData.get(id);
    if (!record) {
      throw new Error(`Record ${id} not found in table ${table}`);
    }

    // Update record
    record.data = { ...record.data, ...data };
    record.updated_at = new Date();

    // Update indexes
    await this.updateIndexes(table, id, record.data);

    // Persist to file
    await this.persistTable(table);

    this.log(`📝 Record updated in ${table}: ${id}`);

    return {
      success: true,
      record: record
    };
  }

  private async handleDeleteRecord(params: any): Promise<any> {
    const { table, id } = params;
    
    if (!table || !id) {
      throw new Error('Table and record ID are required');
    }

    const tableData = this.tables.get(table);
    if (!tableData) {
      throw new Error(`Table ${table} not found`);
    }

    const deleted = tableData.delete(id);
    if (!deleted) {
      throw new Error(`Record ${id} not found in table ${table}`);
    }

    // Remove from indexes
    await this.removeFromIndexes(table, id);

    // Persist to file
    await this.persistTable(table);

    this.log(`🗑️ Record deleted from ${table}: ${id}`);

    return {
      success: true,
      deleted_record_id: id
    };
  }

  private async handleCreateTable(params: any): Promise<any> {
    const { table } = params;
    
    if (!table) {
      throw new Error('Table name is required');
    }

    if (!this.tables.has(table)) {
      this.tables.set(table, new Map());
      this.indexes.set(table, new Map());
      
      // Create table file
      await this.persistTable(table);
      
      this.log(`📋 Table created: ${table}`);
    }

    return {
      success: true,
      table: table
    };
  }

  private async handleListTables(_params: any): Promise<any> {
    const tables = Array.from(this.tables.keys());
    
    return {
      success: true,
      tables: tables
    };
  }

  private async handleBackupData(params: any): Promise<any> {
    const { backup_name } = params;
    const backupPath = path.join(this.dataDirectory, 'backups', backup_name || `backup_${Date.now()}`);
    
    await fs.mkdir(path.dirname(backupPath), { recursive: true });
    
    const backupData = {
      timestamp: new Date().toISOString(),
      tables: Object.fromEntries(this.tables.entries())
    };
    
    await fs.writeFile(backupPath, JSON.stringify(backupData, null, 2));
    
    this.log(`💾 Data backup created: ${backupPath}`);
    
    return {
      success: true,
      backup_path: backupPath
    };
  }

  private async handleRestoreData(params: any): Promise<any> {
    const { backup_path } = params;
    
    if (!backup_path) {
      throw new Error('Backup path is required');
    }
    
    const backupData = JSON.parse(await fs.readFile(backup_path, 'utf-8'));
    this.tables = new Map(Object.entries(backupData.tables));
    
    // Rebuild indexes
    for (const [tableName, tableData] of this.tables.entries()) {
      this.indexes.set(tableName, new Map());
      for (const [recordId, record] of tableData.entries()) {
        await this.updateIndexes(tableName, recordId, (record as DatabaseRecord).data);
      }
    }
    
    this.log(`🔄 Data restored from: ${backup_path}`);
    
    return {
      success: true,
      restored_from: backup_path
    };
  }

  private async updateIndexes(table: string, recordId: string, data: Record<string, any>): Promise<void> {
    const tableIndexes = this.indexes.get(table) || new Map();
    
    // Simple indexing by field values
    for (const [field, value] of Object.entries(data)) {
      if (typeof value === 'string' || typeof value === 'number') {
        const fieldIndex = tableIndexes.get(field) || new Set();
        fieldIndex.add(recordId);
        tableIndexes.set(field, fieldIndex);
      }
    }
    
    this.indexes.set(table, tableIndexes);
  }

  private async removeFromIndexes(table: string, recordId: string): Promise<void> {
    const tableIndexes = this.indexes.get(table);
    if (tableIndexes) {
      for (const fieldIndex of tableIndexes.values()) {
        fieldIndex.delete(recordId);
      }
    }
  }

  private async persistTable(table: string): Promise<void> {
    const tableData = this.tables.get(table);
    if (tableData) {
      const filePath = path.join(this.dataDirectory, `${table}.json`);
      const data = Object.fromEntries(tableData.entries());
      await fs.writeFile(filePath, JSON.stringify(data, null, 2));
    }
  }

  private async loadTable(table: string): Promise<void> {
    const filePath = path.join(this.dataDirectory, `${table}.json`);
    
    try {
      const data = JSON.parse(await fs.readFile(filePath, 'utf-8'));
      const tableData = new Map(Object.entries(data)) as Map<string, DatabaseRecord>;
      this.tables.set(table, tableData);
      
      // Rebuild indexes
      this.indexes.set(table, new Map());
      for (const [recordId, record] of tableData.entries()) {
        await this.updateIndexes(table, recordId, (record as DatabaseRecord).data);
      }
      
      this.log(`📂 Loaded table: ${table} (${tableData.size} records)`);
    } catch (error) {
      // Table file doesn't exist yet, that's okay
      this.tables.set(table, new Map());
      this.indexes.set(table, new Map());
    }
  }

  protected async onStart(): Promise<void> {
    this.log('💾 Starting Database Daemon...');
    
    // Get data directory from ContinuumDirectoryDaemon
    try {
      const directoryResponse = await this.delegateToContinuumDirectoryDaemon('request_directory', {
        type: 'config',
        context: {
          artifactType: 'database',
          metadata: { purpose: 'persistent_storage' }
        }
      });
      this.dataDirectory = directoryResponse.path;
    } catch (error) {
      // Fallback if ContinuumDirectoryDaemon not available
      this.log(`⚠️ ContinuumDirectoryDaemon not available, using fallback path`);
      this.dataDirectory = path.join(process.env.HOME || '/tmp', '.continuum', 'database');
      await fs.mkdir(this.dataDirectory, { recursive: true });
    }
    
    // Load existing tables
    try {
      const files = await fs.readdir(this.dataDirectory);
      for (const file of files) {
        if (file.endsWith('.json') && !file.startsWith('backup_')) {
          const tableName = file.replace('.json', '');
          await this.loadTable(tableName);
        }
      }
    } catch (error) {
      this.log(`⚠️ Error loading tables: ${error}`);
    }
    
    this.log('✅ Database Daemon ready for data persistence');
  }

  protected async onStop(): Promise<void> {
    this.log('🛑 Stopping Database Daemon...');
    
    // Persist all tables
    for (const table of this.tables.keys()) {
      await this.persistTable(table);
    }
    
    this.log('✅ Database Daemon stopped, all data persisted');
  }

  /**
   * Delegate to ContinuumDirectoryDaemon for directory management
   */
  private async delegateToContinuumDirectoryDaemon(operation: string, _params: any): Promise<any> {
    // TODO: Implement actual daemon delegation via message bus
    // For now, return fallback responses to keep system working
    
    switch (operation) {
      case 'request_directory':
        const basePath = path.join(process.env.HOME || '/tmp', '.continuum');
        const dbPath = path.join(basePath, 'database');
        await fs.mkdir(dbPath, { recursive: true });
        
        return {
          path: dbPath,
          created: true,
          structure: ['database'],
          metadata: {
            type: 'database_storage',
            permissions: 'rw',
            estimatedSize: '0MB',
            retentionPolicy: 'permanent'
          }
        };
      default:
        throw new Error(`Unknown ContinuumDirectoryDaemon operation: ${operation}`);
    }
  }
}