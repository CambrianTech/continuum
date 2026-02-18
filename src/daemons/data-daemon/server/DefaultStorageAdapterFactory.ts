/**
 * Default Storage Adapter Factory - Creates storage adapters based on configuration
 *
 * NOTE (2026-02-09): SQLite operations now go through ORM → ORMRustClient → Rust DataModule.
 * The 'sqlite' type returns MemoryStorageAdapter as a no-op placeholder since actual
 * data operations are handled by Rust. This adapter is only used for:
 * - DataDaemon initialization (no-op in practice)
 * - Static interface compatibility (tests should use ORM instead)
 */

import { MemoryStorageAdapter } from '../server/MemoryStorageAdapter';
import { FileStorageAdapter } from '../server/FileStorageAdapter';
import type { DataStorageAdapter, StorageAdapterConfig } from '../shared/DataStorageAdapter';

/**
 * Factory for creating storage adapters based on configuration
 */
export class DefaultStorageAdapterFactory {
  /**
   * Create storage adapter based on configuration type
   *
   * NOTE: 'sqlite' type now returns MemoryStorageAdapter since all SQLite
   * operations go through ORM → Rust. The adapter is only used for
   * DataDaemon initialization, not actual data operations.
   */
  createAdapter(config: StorageAdapterConfig): DataStorageAdapter {
    switch (config.type) {
      case 'sqlite':
        // SQLite operations go through ORM → Rust; use no-op MemoryStorageAdapter
        return new MemoryStorageAdapter();
      case 'memory':
        return new MemoryStorageAdapter();
      case 'file':
        return new FileStorageAdapter();
      default:
        throw new Error(`Unsupported storage adapter type: ${config.type}`);
    }
  }
}