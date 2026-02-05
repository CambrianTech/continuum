/**
 * Database Handle Registry - Multi-Database Management System
 *
 * Storage-adapter-agnostic handle system for managing multiple database connections.
 * A DbHandle is an opaque identifier that can point to ANY DataStorageAdapter implementation:
 * - SQLite database
 * - JSON file storage
 * - Vector database (Qdrant, Pinecone)
 * - Graph database (Neo4j)
 * - Any future DataStorageAdapter
 *
 * **Design Principles**:
 * 1. Backward Compatible: No dbHandle parameter = uses 'default' handle
 * 2. Single Source of Truth: DATABASE_PATHS.SQLITE remains the default
 * 3. Explicit Handles: Must call data/open to get non-default handles
 * 4. Auto-cleanup: Handles close after inactivity or on explicit data/close
 * 5. Thread-safe: Registry acts as connection pool
 *
 * See docs/MULTI-DATABASE-HANDLES.md for full architecture
 */

import { DataStorageAdapter } from '../shared/DataStorageAdapter';
import { SqliteStorageAdapter } from './SqliteStorageAdapter';
import { RustWorkerStorageAdapter } from './RustWorkerStorageAdapter';
import { DATABASE_PATHS } from '../../../system/data/config/DatabaseConfig';
import { generateUUID, type UUID } from '../../../system/core/types/CrossPlatformUUID';
import { getDatabasePath, getServerConfig } from '../../../system/config/ServerConfig';

/**
 * Database handle - opaque identifier for ANY storage adapter
 * Can be:
 * - 'default': Main database (DATABASE_PATHS.SQLITE)
 * - UUID: Explicitly opened handle to any storage backend
 */
export type DbHandle = 'default' | UUID;

/**
 * Default handle constant - uses DATABASE_PATHS.SQLITE
 */
export const DEFAULT_HANDLE: DbHandle = 'default';

/**
 * Adapter types supported by the system
 * Extensible - add new types as needed (vector, graph, etc.)
 */
export type AdapterType = 'sqlite' | 'json' | 'vector' | 'graph' | 'rust';

/**
 * Open mode for database connections
 */
export type OpenMode = 'readonly' | 'readwrite' | 'create';

/**
 * SQLite-specific configuration
 */
export interface SqliteConfig {
  filename?: string;                   // Database file path (deprecated, use 'path')
  path?: string;                       // Database file path (preferred)
  mode?: OpenMode;                     // Open mode (default: readwrite)
  poolSize?: number;                   // Connection pool size
  foreignKeys?: boolean;               // Enable foreign key constraints
  wal?: boolean;                       // Write-Ahead Logging
}

/**
 * JSON file storage configuration
 */
export interface JsonConfig {
  path: string;                        // JSON file path
  pretty?: boolean;                    // Pretty-print JSON
  autoSave?: boolean;                  // Auto-save on changes
}

/**
 * Vector database configuration (Qdrant, Pinecone, etc.)
 */
export interface VectorConfig {
  endpoint: string;                    // Vector DB endpoint
  collection: string;                  // Collection name
  apiKey?: string;                     // API key (optional)
}

/**
 * Graph database configuration (Neo4j, etc.)
 */
export interface GraphConfig {
  endpoint: string;                    // Graph DB endpoint
  database?: string;                   // Database name
  username?: string;                   // Username
  password?: string;                   // Password
}

/**
 * Rust worker configuration (experimental)
 */
export interface RustConfig {
  filename: string;                    // Database file path
  socketPath?: string;                 // Unix socket path to Rust worker (default: /tmp/jtag-data-worker.sock)
  mode?: OpenMode;                     // Open mode (default: readwrite)
  storageType?: 'auto-detect' | 'internal-ssd' | 'external-ssd' | 'sd-card';  // Storage detection override
}

/**
 * Union type for all adapter configs
 */
export type AdapterConfig = SqliteConfig | JsonConfig | VectorConfig | GraphConfig | RustConfig;

/**
 * Handle metadata - tracks adapter type and config
 */
export interface HandleMetadata {
  adapter: AdapterType;
  config: AdapterConfig;
  openedAt: number;
  lastUsedAt: number;
  emitEvents?: boolean;  // Whether to emit CRUD events for operations on this handle (default: true)
}

/**
 * Database Handle Registry
 *
 * Manages open storage adapters across ANY backend type.
 * Singleton pattern ensures single connection pool per process.
 *
 * **Key Design**: Storage-adapter-agnostic!
 * - Handles map to DataStorageAdapter interface
 * - Works with SQLite, JSON, Vector DB, Graph DB, or any future adapter
 * - Default handle always points to main database (DATABASE_PATHS.SQLITE)
 */
export class DatabaseHandleRegistry {
  private static instance: DatabaseHandleRegistry;

  // Map handles to ANY DataStorageAdapter implementation
  private handles: Map<DbHandle, DataStorageAdapter>;

  // Track metadata for each handle (adapter type, config, timestamps)
  private handleMetadata: Map<DbHandle, HandleMetadata>;

  // Map alias names to handle UUIDs (e.g., 'primary' → UUID, 'archive' → UUID)
  private handleAliases: Map<string, DbHandle>;

  private constructor() {
    this.handles = new Map();
    this.handleMetadata = new Map();
    this.handleAliases = new Map();

    // Initialize default handle - always use TypeScript SQLite
    const expandedDbPath = getDatabasePath();
    console.log(`📦 DatabaseHandleRegistry: Using TypeScript SQLite (db: ${expandedDbPath})`);

    const defaultAdapter: DataStorageAdapter = new SqliteStorageAdapter();
    const adapterType: AdapterType = 'sqlite';

    defaultAdapter.initialize({
      type: 'sqlite',
      namespace: 'default',
      options: {
        filename: expandedDbPath
      }
    }).then(() => {
      console.log(`📦 DatabaseHandleRegistry: SQLite adapter initialized successfully`);
    }).catch((error) => {
      console.error('❌ DatabaseHandleRegistry: Failed to initialize SQLite adapter:', error);
    });

    this.handles.set(DEFAULT_HANDLE, defaultAdapter);
    this.handleMetadata.set(DEFAULT_HANDLE, {
      adapter: adapterType,
      config: { filename: expandedDbPath },
      openedAt: Date.now(),
      lastUsedAt: Date.now()
    });
  }

  /**
   * Get singleton instance
   */
  static getInstance(): DatabaseHandleRegistry {
    if (!DatabaseHandleRegistry.instance) {
      DatabaseHandleRegistry.instance = new DatabaseHandleRegistry();
    }
    return DatabaseHandleRegistry.instance;
  }

  /**
   * Open a new database connection and return handle
   *
   * @param adapter - Adapter type ('sqlite', 'json', 'vector', 'graph')
   * @param config - Adapter-specific configuration
   * @param options - Handle options (e.g., emitEvents)
   * @returns DbHandle - Opaque identifier for this connection
   *
   * @example
   * ```typescript
   * // Open training database
   * const handle = await registry.open('sqlite', {
   *   filename: '/datasets/prepared/continuum-git.sqlite',
   *   mode: 'readonly'
   * });
   *
   * // Open archive database without event emission
   * const archiveHandle = await registry.open('sqlite', {
   *   filename: '/path/to/archive.sqlite'
   * }, { emitEvents: false });
   *
   * // Open vector database
   * const vectorHandle = await registry.open('vector', {
   *   endpoint: 'http://localhost:6333',
   *   collection: 'code-embeddings',
   *   apiKey: process.env.QDRANT_API_KEY
   * });
   * ```
   */
  async open(adapter: AdapterType, config: AdapterConfig, options?: { emitEvents?: boolean }): Promise<DbHandle> {
    const handle = generateUUID();

    // Create adapter based on type
    // TODO: Add JSON, Vector, Graph adapters when implemented
    let storageAdapter: DataStorageAdapter;

    switch (adapter) {
      case 'sqlite': {
        const sqliteConfig = config as SqliteConfig;
        const dbPath = sqliteConfig.path || sqliteConfig.filename;
        if (!dbPath) {
          throw new Error('SQLite config requires either "path" or "filename" property');
        }
        storageAdapter = new SqliteStorageAdapter();
        await storageAdapter.initialize({
          type: 'sqlite',
          namespace: handle,
          options: {
            filename: dbPath
          }
        });
        break;
      }

      case 'rust': {
        const rustConfig = config as RustConfig;
        if (!rustConfig.filename) {
          throw new Error('Rust config requires "filename" property (database path)');
        }
        const socketPath = rustConfig.socketPath || '/tmp/jtag-data-daemon-worker.sock';
        storageAdapter = new RustWorkerStorageAdapter({
          socketPath,
          dbPath: rustConfig.filename,
          timeout: 30000
        });
        await storageAdapter.initialize({
          type: 'rust' as any,
          namespace: handle as string,
          options: {
            socketPath,
            dbPath: rustConfig.filename
          }
        });
        break;
      }

      case 'json':
      case 'vector':
      case 'graph':
        throw new Error(`Adapter type '${adapter}' not yet implemented. Only 'sqlite' and 'rust' are currently supported.`);

      default:
        throw new Error(`Unknown adapter type: ${adapter}`);
    }

    // Register handle
    this.handles.set(handle, storageAdapter);
    this.handleMetadata.set(handle, {
      adapter,
      config,
      openedAt: Date.now(),
      lastUsedAt: Date.now(),
      emitEvents: options?.emitEvents ?? true  // Default to emitting events
    });


    return handle;
  }

  /**
   * Register an alias name for a database handle
   *
   * @param alias - Alias name (e.g., 'primary', 'archive')
   * @param handle - The DbHandle (UUID) to map to
   *
   * @example
   * ```typescript
   * const handle = await registry.open('sqlite', { path: '/path/to/db.sqlite' });
   * registry.registerAlias('primary', handle);
   * // Now can use 'primary' instead of UUID
   * const adapter = registry.getAdapter('primary');
   * ```
   */
  registerAlias(alias: string, handle: DbHandle): void {
    if (!this.handles.has(handle)) {
      throw new Error(`Cannot register alias '${alias}': handle '${handle}' does not exist`);
    }
    this.handleAliases.set(alias, handle);
  }

  /**
   * Get adapter for handle (returns default if handle not found or omitted)
   *
   * **Backward Compatibility**: If handle is undefined/null, returns default adapter.
   * This ensures all existing code continues to work without modification.
   *
   * **Alias Resolution**: If handle is a string that exists in handleAliases, resolves to UUID first.
   *
   * @param handle - Database handle or alias name (optional, defaults to 'default')
   * @returns DataStorageAdapter - The storage adapter for this handle
   *
   * @example
   * ```typescript
   * // Get default adapter (backward compatible)
   * const adapter = registry.getAdapter();
   *
   * // Get specific adapter by handle UUID
   * const trainingAdapter = registry.getAdapter(trainingHandle);
   *
   * // Get adapter by alias name
   * const primaryAdapter = registry.getAdapter('primary');
   * const archiveAdapter = registry.getAdapter('archive');
   * ```
   */
  getAdapter(handle?: DbHandle): DataStorageAdapter {
    const actualHandle = handle || DEFAULT_HANDLE;

    // Resolve alias to UUID if applicable
    const resolvedHandle = this.handleAliases.get(actualHandle as string) || actualHandle;

    const adapter = this.handles.get(resolvedHandle);

    if (!adapter) {
      console.warn(`⚠️  Database handle '${actualHandle}' not found, using default`);
      return this.handles.get(DEFAULT_HANDLE)!;
    }

    // Update last used timestamp (for LRU eviction in future)
    const metadata = this.handleMetadata.get(resolvedHandle);
    if (metadata) {
      metadata.lastUsedAt = Date.now();
    }

    return adapter;
  }

  /**
   * Close database handle
   *
   * @param handle - Database handle to close
   * @throws Error if attempting to close default handle
   *
   * @example
   * ```typescript
   * await registry.close(trainingHandle);
   * ```
   */
  async close(handle: DbHandle): Promise<void> {
    if (handle === DEFAULT_HANDLE) {
      throw new Error('Cannot close default database handle');
    }

    const adapter = this.handles.get(handle);
    if (adapter) {
      await adapter.close();
      this.handles.delete(handle);
      this.handleMetadata.delete(handle);
      console.log(`🔌 DatabaseHandleRegistry: Closed handle ${handle}`);
    } else {
      console.warn(`⚠️  Database handle '${handle}' not found (already closed?)`);
    }
  }

  /**
   * List all open handles
   *
   * @returns Array of handle info (handle, adapter type, config, timestamps)
   *
   * @example
   * ```typescript
   * const handles = registry.listHandles();
   * console.log(`Open handles: ${handles.length}`);
   * handles.forEach(h => console.log(`  ${h.handle}: ${h.adapter} (${h.config.path})`));
   * ```
   */
  listHandles(): Array<{
    handle: DbHandle;
    adapter: AdapterType;
    config: AdapterConfig;
    isDefault: boolean;
    openedAt: number;
    lastUsedAt: number;
  }> {
    const result: Array<{
      handle: DbHandle;
      adapter: AdapterType;
      config: AdapterConfig;
      isDefault: boolean;
      openedAt: number;
      lastUsedAt: number;
    }> = [];

    for (const [handle, metadata] of this.handleMetadata.entries()) {
      result.push({
        handle,
        adapter: metadata.adapter,
        config: metadata.config,
        isDefault: handle === DEFAULT_HANDLE,
        openedAt: metadata.openedAt,
        lastUsedAt: metadata.lastUsedAt
      });
    }

    return result;
  }

  /**
   * Get handle metadata
   *
   * @param handle - Database handle
   * @returns HandleMetadata or undefined if handle not found
   */
  getMetadata(handle: DbHandle): HandleMetadata | undefined {
    return this.handleMetadata.get(handle);
  }

  /**
   * Check if handle exists and is open
   *
   * @param handle - Database handle
   * @returns true if handle exists and is open
   */
  isOpen(handle: DbHandle): boolean {
    return this.handles.has(handle);
  }
}
