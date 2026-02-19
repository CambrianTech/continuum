/**
 * Database Handle Registry - Multi-Database Path Management
 *
 * Lightweight handle system for managing database path mappings.
 * A DbHandle is an opaque identifier that maps to a database file path.
 *
 * **Architecture Note (2026-02-09)**:
 * This registry NO LONGER manages TypeScript SqliteStorageAdapter instances.
 * All actual database I/O goes through ORM → ORMRustClient → Rust DataModule.
 * This class is now purely a handle → path mapping service.
 *
 * **Design Principles**:
 * 1. Backward Compatible: No dbHandle parameter = uses 'default' handle
 * 2. Single Source of Truth: DATABASE_PATHS.SQLITE remains the default
 * 3. Explicit Handles: Must call data/open to get non-default handles
 * 4. Path Resolution: getDbPath() converts handle → database path for ORM
 *
 * See docs/MULTI-DATABASE-HANDLES.md for full architecture
 */

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
 * Well-known handle constants - use these instead of magic strings
 *
 * NOTE: These are just identifiers. The actual paths come from:
 * - DEFAULT: getDatabasePath() → config.env DATABASE_DIR
 * - ARCHIVE: getArchiveDir() → config.env DATABASE_ARCHIVE_DIR
 *
 * Single source of truth: ServerConfig resolves all paths from config.env
 */
export const DB_HANDLES = {
  /** Main database - uses getDatabasePath() */
  DEFAULT: 'default' as const,
  /** Archive database alias (must be registered via open() + registerAlias()) */
  ARCHIVE: 'archive' as const,
  /** Primary database alias (optional, same as DEFAULT) */
  PRIMARY: 'primary' as const,
} as const;

/** Type for well-known handle names */
export type DbHandleAlias = typeof DB_HANDLES[keyof typeof DB_HANDLES];

/**
 * Default handle constant - uses DATABASE_PATHS.SQLITE
 * @deprecated Use DB_HANDLES.DEFAULT instead
 */
export const DEFAULT_HANDLE: DbHandle = DB_HANDLES.DEFAULT;

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
 * Manages handle → path mappings for database operations.
 * Singleton pattern ensures consistent path resolution across the system.
 *
 * **Key Design (2026-02-09)**: Path-only registry!
 * - Handles map to database file paths (NOT to TypeScript adapters)
 * - All database I/O goes through ORM → ORMRustClient → Rust DataModule
 * - This class provides handle → path resolution via getDbPath()
 * - Default handle always points to main database (DATABASE_PATHS.SQLITE)
 */
export class DatabaseHandleRegistry {
  private static instance: DatabaseHandleRegistry;

  // Track metadata for each handle (adapter type, config, timestamps)
  private handleMetadata: Map<DbHandle, HandleMetadata>;

  // Map alias names to handle UUIDs (e.g., 'primary' → UUID, 'archive' → UUID)
  private handleAliases: Map<string, DbHandle>;

  private constructor() {
    this.handleMetadata = new Map();
    this.handleAliases = new Map();

    // Initialize default handle metadata
    const expandedDbPath = getDatabasePath();

    this.handleMetadata.set(DEFAULT_HANDLE, {
      adapter: 'rust' as AdapterType,  // All I/O goes through Rust
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
   * Open a new database handle and return it
   *
   * NOTE (2026-02-09): This no longer creates TypeScript adapters!
   * It just registers the handle → path mapping. All I/O goes through Rust.
   *
   * @param adapter - Adapter type ('sqlite' only supported via Rust)
   * @param config - Adapter-specific configuration
   * @param options - Handle options (e.g., emitEvents)
   * @returns DbHandle - Opaque identifier for this database
   *
   * @example
   * ```typescript
   * // Open training database
   * const handle = await registry.open('sqlite', {
   *   filename: '/datasets/prepared/continuum-git.sqlite',
   *   mode: 'readonly'
   * });
   *
   * // Then use with ORM:
   * const dbPath = registry.getDbPath(handle);
   * const data = await ORM.query({ collection: 'items' }, dbPath);
   * ```
   */
  async open(adapter: AdapterType, config: AdapterConfig, options?: { emitEvents?: boolean }): Promise<DbHandle> {
    const handle = generateUUID();

    // Validate config has a path
    switch (adapter) {
      case 'sqlite':
      case 'rust': {
        const sqliteConfig = config as SqliteConfig;
        const dbPath = sqliteConfig.path || sqliteConfig.filename;
        if (!dbPath) {
          throw new Error('SQLite config requires either "path" or "filename" property');
        }
        // Just register the path - Rust handles actual connections
        break;
      }

      case 'json':
      case 'vector':
      case 'graph':
        throw new Error(`Adapter type '${adapter}' not yet implemented. Only 'sqlite' is currently supported.`);

      default:
        throw new Error(`Unknown adapter type: ${adapter}`);
    }

    // Register handle metadata (path stored in config)
    this.handleMetadata.set(handle, {
      adapter: 'rust' as AdapterType,  // All I/O goes through Rust
      config,
      openedAt: Date.now(),
      lastUsedAt: Date.now(),
      emitEvents: options?.emitEvents ?? true
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
   * const dbPath = registry.getDbPath('primary');
   * const data = await ORM.query({ collection: 'items' }, dbPath);
   * ```
   */
  registerAlias(alias: string, handle: DbHandle): void {
    if (!this.handleMetadata.has(handle)) {
      throw new Error(`Cannot register alias '${alias}': handle '${handle}' does not exist`);
    }
    this.handleAliases.set(alias, handle);
  }

  /**
   * @deprecated Use getDbPath() instead - all I/O now goes through Rust DataModule
   *
   * This method is preserved for backward compatibility but will be removed.
   * Since 2026-02-09, no TypeScript adapters are created.
   *
   * @param handle - Database handle (ignored - returns null)
   * @returns null - No adapters exist, use getDbPath() instead
   */
  getAdapter(handle?: DbHandle): null {
    console.warn(`⚠️  DatabaseHandleRegistry.getAdapter() is DEPRECATED. Use getDbPath() instead.`);
    console.warn(`    All database I/O now goes through ORM → ORMRustClient → Rust DataModule.`);

    // Update last used timestamp
    const actualHandle = handle || DEFAULT_HANDLE;
    const resolvedHandle = this.handleAliases.get(actualHandle as string) || actualHandle;
    const metadata = this.handleMetadata.get(resolvedHandle);
    if (metadata) {
      metadata.lastUsedAt = Date.now();
    }

    return null;
  }

  /**
   * Close database handle
   *
   * NOTE (2026-02-09): This just removes the handle from the registry.
   * Rust manages connection pooling - no TypeScript adapter cleanup needed.
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

    const metadata = this.handleMetadata.get(handle);
    if (metadata) {
      this.handleMetadata.delete(handle);
      console.log(`🔌 DatabaseHandleRegistry: Closed handle ${handle.substring(0, 8)}...`);
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
   * Check if handle exists and is registered
   *
   * @param handle - Database handle
   * @returns true if handle exists in registry
   */
  isOpen(handle: DbHandle): boolean {
    return this.handleMetadata.has(handle);
  }

  /**
   * Get database path for a handle
   *
   * Returns the file path for the database associated with this handle.
   * Used to route operations through ORM with the correct database.
   *
   * @param handle - Database handle ('default' or UUID)
   * @returns Database file path, or null if handle not found or has no path
   */
  getDbPath(handle?: DbHandle): string | null {
    // Default handle uses main database
    if (!handle || handle === 'default') {
      return getDatabasePath();
    }

    const metadata = this.handleMetadata.get(handle);
    if (!metadata) return null;

    // Extract path from config based on adapter type
    const config = metadata.config;
    if ('path' in config && config.path) {
      return config.path;
    }
    if ('filename' in config && config.filename) {
      return config.filename;
    }

    return null;
  }
}
