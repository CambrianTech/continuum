/**
 * Default Storage Adapter Factory - Creates storage adapters based on configuration
 *
 * Provides factory pattern for creating different storage adapter types
 * (SQLite, Memory, File) based on StorageAdapterConfig
 */

import { SqliteStorageAdapter } from '../server/SqliteStorageAdapter';
import { MemoryStorageAdapter } from '../server/MemoryStorageAdapter';
import { FileStorageAdapter } from '../server/FileStorageAdapter';
import { RustAdapter } from '../server/RustAdapter';
import type { DataStorageAdapter, StorageAdapterConfig } from '../shared/DataStorageAdapter';

/**
 * Factory for creating storage adapters based on configuration
 */
export class DefaultStorageAdapterFactory {
  /**
   * Create storage adapter based on configuration type
   */
  createAdapter(config: StorageAdapterConfig): DataStorageAdapter {
    switch (config.type) {
      case 'sqlite':
        return new SqliteStorageAdapter();
      case 'memory':
        return new MemoryStorageAdapter();
      case 'file':
        return new FileStorageAdapter();
      case 'rust':
        return new RustAdapter();
      default:
        throw new Error(`Unsupported storage adapter type: ${config.type}`);
    }
  }
}