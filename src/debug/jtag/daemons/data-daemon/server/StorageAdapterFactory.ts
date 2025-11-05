/**
 * Storage Adapter Factory - Plugin System Implementation
 * 
 * Concrete factory for creating storage adapters based on configuration.
 * Supports File, Memory, and future SQL/NoSQL adapters.
 */

import { DataStorageAdapter, StorageAdapterFactory, type StorageAdapterConfig } from '../shared/DataStorageAdapter';
import { FileStorageAdapter } from './FileStorageAdapter';
import { MemoryStorageAdapter } from './MemoryStorageAdapter';
import { SqliteStorageAdapter } from './SqliteStorageAdapter';

/**
 * Concrete Storage Adapter Factory
 */
export class DefaultStorageAdapterFactory extends StorageAdapterFactory {
  
  /**
   * Create storage adapter based on configuration
   */
  createAdapter(config: StorageAdapterConfig): DataStorageAdapter {
    switch (config.type) {
      case 'file':
        return new FileStorageAdapter();
        
      case 'memory':
        return new MemoryStorageAdapter();
        
      case 'sqlite':
        return new SqliteStorageAdapter();
        
      case 'postgres':
        throw new Error('PostgreSQL adapter not yet implemented - use file storage for now');
        
      case 'mongodb':
        throw new Error('MongoDB adapter not yet implemented - use file storage for now');
        
      case 'network':
        throw new Error('Network adapter not yet implemented - use file storage for now');
        
      default:
        throw new Error(`Unsupported storage adapter type: ${config.type}`);
    }
  }
  
  /**
   * Get supported storage adapter types
   */
  getSupportedTypes(): string[] {
    return [
      'file',      // JSON file storage - ready
      'memory',    // In-memory storage - ready
      'sqlite',    // SQLite database - planned
      'postgres',  // PostgreSQL database - planned 
      'mongodb',   // MongoDB document store - planned
      'network'    // Distributed storage - planned
    ];
  }
  
  /**
   * Get supported backends for a storage strategy
   */
  getSupportedBackends(strategy: string): string[] {
    switch (strategy) {
      case 'file':
        return ['json', 'binary', 'structured'];
        
      case 'memory':
        return ['map', 'cache', 'session'];
        
      case 'sql':
        return ['sqlite', 'postgres', 'mysql'];
        
      case 'nosql':
        return ['mongodb', 'redis', 'leveldb'];
        
      case 'network':
        return ['p2p', 'distributed', 'consensus'];
        
      case 'hybrid':
        return ['cache+file', 'memory+sql', 'local+network'];
        
      default:
        return [];
    }
  }
  
  /**
   * Check if storage type is available
   */
  isTypeSupported(type: string): boolean {
    return this.getSupportedTypes().includes(type);
  }
  
  /**
   * Get adapter capabilities without creating instance
   */
  getAdapterCapabilities(type: string): {
    supportsTransactions: boolean;
    supportsIndexing: boolean;
    supportsFullTextSearch: boolean;
    supportsReplication: boolean;
    maxRecordSize: number;
    concurrentConnections: number;
  } {
    switch (type) {
      case 'file':
        return {
          supportsTransactions: false,
          supportsIndexing: false,
          supportsFullTextSearch: false,
          supportsReplication: false,
          maxRecordSize: 100 * 1024 * 1024, // 100MB per record
          concurrentConnections: 1
        };
        
      case 'memory':
        return {
          supportsTransactions: false,
          supportsIndexing: true,
          supportsFullTextSearch: true,
          supportsReplication: false,
          maxRecordSize: 10 * 1024 * 1024, // 10MB per record
          concurrentConnections: 1000
        };
        
      case 'sqlite':
        return {
          supportsTransactions: true,
          supportsIndexing: true,
          supportsFullTextSearch: true,
          supportsReplication: false,
          maxRecordSize: 1024 * 1024 * 1024, // 1GB per record
          concurrentConnections: 10
        };
        
      case 'postgres':
        return {
          supportsTransactions: true,
          supportsIndexing: true,
          supportsFullTextSearch: true,
          supportsReplication: true,
          maxRecordSize: Number.MAX_SAFE_INTEGER,
          concurrentConnections: 1000
        };
        
      default:
        return {
          supportsTransactions: false,
          supportsIndexing: false,
          supportsFullTextSearch: false,
          supportsReplication: false,
          maxRecordSize: 0,
          concurrentConnections: 0
        };
    }
  }
}