/**
 * DataService Factory - Creates properly configured DataService instances
 * 
 * Handles adapter configuration and hybrid setups
 * Provides production-ready data service configurations
 */

import { DataService, DataServiceConfig } from './DataService';
import { HybridAdapter, HybridAdapterConfig } from '../adapters/HybridAdapter';
import { JsonFileAdapter } from '../adapters/JsonFileAdapter';
import { SQLiteAdapter } from '../adapters/SQLiteAdapter';
import type { SessionId, UserId } from '../domains/CoreTypes';
import { SessionId as createSessionId, UserId as createUserId } from '../domains/CoreTypes';
import { generateUUID } from '../../core/types/CrossPlatformUUID';

/**
 * Data Service Configuration Presets
 */
export enum DataServiceMode {
  JSON_ONLY = 'json_only',           // Legacy JSON files only
  SQLITE_ONLY = 'sqlite_only',       // Pure SQLite database
  HYBRID_MIGRATE = 'hybrid_migrate',  // JSON→SQLite with auto-migration
  HYBRID_COMPAT = 'hybrid_compat'    // JSON+SQLite compatibility mode
}

/**
 * Factory Configuration
 */
export interface DataServiceFactoryConfig {
  readonly mode: DataServiceMode;
  readonly paths: {
    readonly jsonDatabase?: string;
    readonly sqliteDatabase?: string;
  };
  readonly context: {
    readonly userId?: UserId;
    readonly sessionId?: SessionId;
    readonly source: string;
  };
  readonly migration?: {
    readonly autoMigrate?: boolean;
    readonly migrateOnWrite?: boolean;
  };
}

/**
 * DataService Factory
 */
export class DataServiceFactory {
  
  /**
   * Create a DataService with the specified configuration
   */
  static async create(config: DataServiceFactoryConfig): Promise<DataService> {
    const sessionId = config.context.sessionId || createSessionId(generateUUID());
    
    const serviceConfig: DataServiceConfig = {
      adapters: {},
      defaultAdapter: await this.createDefaultAdapter(config),
      context: {
        userId: config.context.userId,
        sessionId,
        source: config.context.source
      }
    };

    // Create collection-specific adapters if needed
    const adapters = await this.createCollectionAdapters(config);
    const mutableConfig = { ...serviceConfig, adapters };

    const dataService = new DataService(mutableConfig);
    await dataService.initialize();
    
    return dataService;
  }

  /**
   * Create a DataService for existing JSON file compatibility
   */
  static async createJsonCompatible(
    jsonDatabasePath: string = '.continuum/database',
    sessionId?: SessionId,
    userId?: UserId
  ): Promise<DataService> {
    return await this.create({
      mode: DataServiceMode.JSON_ONLY,
      paths: { jsonDatabase: jsonDatabasePath },
      context: {
        sessionId: sessionId || createSessionId(generateUUID()),
        userId,
        source: 'json-compatibility'
      }
    });
  }

  /**
   * Create a DataService with hybrid JSON→SQLite migration
   */
  static async createHybridMigrating(
    jsonDatabasePath: string = '.continuum/database',
    sqliteDatabasePath: string = '.continuum/database/continuum.db',
    sessionId?: SessionId,
    userId?: UserId
  ): Promise<DataService> {
    return await this.create({
      mode: DataServiceMode.HYBRID_MIGRATE,
      paths: { 
        jsonDatabase: jsonDatabasePath,
        sqliteDatabase: sqliteDatabasePath
      },
      context: {
        sessionId: sessionId || createSessionId(generateUUID()),
        userId,
        source: 'hybrid-migration'
      },
      migration: {
        autoMigrate: true,
        migrateOnWrite: true
      }
    });
  }

  /**
   * Create a high-performance SQLite-only DataService
   */
  static async createSQLiteOnly(
    sqliteDatabasePath: string = '.continuum/database/continuum.db',
    sessionId?: SessionId,
    userId?: UserId
  ): Promise<DataService> {
    return await this.create({
      mode: DataServiceMode.SQLITE_ONLY,
      paths: { sqliteDatabase: sqliteDatabasePath },
      context: {
        sessionId: sessionId || createSessionId(generateUUID()),
        userId,
        source: 'sqlite-performance'
      }
    });
  }

  /**
   * Create default adapter based on configuration
   */
  private static async createDefaultAdapter(config: DataServiceFactoryConfig) {
    switch (config.mode) {
      case DataServiceMode.JSON_ONLY:
        return new JsonFileAdapter(config.paths.jsonDatabase);

      case DataServiceMode.SQLITE_ONLY:
        return new SQLiteAdapter(config.paths.sqliteDatabase);

      case DataServiceMode.HYBRID_MIGRATE:
      case DataServiceMode.HYBRID_COMPAT:
        const jsonAdapter = new JsonFileAdapter(config.paths.jsonDatabase);
        const sqliteAdapter = new SQLiteAdapter(config.paths.sqliteDatabase);
        
        const hybridConfig: HybridAdapterConfig = {
          readAdapters: [jsonAdapter, sqliteAdapter],
          writeAdapter: sqliteAdapter,
          migration: {
            autoMigrate: config.migration?.autoMigrate ?? true,
            migrateOnWrite: config.migration?.migrateOnWrite ?? true
          }
        };

        return new HybridAdapter(hybridConfig);

      default:
        throw new Error(`Unsupported DataService mode: ${config.mode}`);
    }
  }

  /**
   * Create collection-specific adapters
   */
  private static async createCollectionAdapters(config: DataServiceFactoryConfig): Promise<Record<string, any>> {
    const adapters: Record<string, any> = {};

    // Example: Chat messages might benefit from SQLite performance
    if (config.mode === DataServiceMode.HYBRID_MIGRATE || config.mode === DataServiceMode.HYBRID_COMPAT) {
      // Chat collection gets hybrid treatment for performance
      const jsonAdapter = new JsonFileAdapter(config.paths.jsonDatabase);
      const sqliteAdapter = new SQLiteAdapter(config.paths.sqliteDatabase);
      
      adapters.chat_messages = new HybridAdapter({
        readAdapters: [jsonAdapter, sqliteAdapter],
        writeAdapter: sqliteAdapter,
        migration: {
          autoMigrate: true,
          migrateOnWrite: true
        }
      });

      // Users collection gets hybrid treatment for full features
      adapters.users = new HybridAdapter({
        readAdapters: [jsonAdapter, sqliteAdapter],
        writeAdapter: sqliteAdapter,
        migration: {
          autoMigrate: true,
          migrateOnWrite: false // Don't auto-migrate user updates
        }
      });
    }

    return adapters;
  }

  /**
   * Get recommended configuration for different use cases
   */
  static getRecommendedConfig(useCase: 'development' | 'production' | 'migration' | 'compatibility'): Partial<DataServiceFactoryConfig> {
    switch (useCase) {
      case 'development':
        return {
          mode: DataServiceMode.JSON_ONLY,
          paths: { jsonDatabase: '.continuum/database' }
        };

      case 'production':
        return {
          mode: DataServiceMode.SQLITE_ONLY,
          paths: { sqliteDatabase: '.continuum/database/continuum.db' }
        };

      case 'migration':
        return {
          mode: DataServiceMode.HYBRID_MIGRATE,
          paths: { 
            jsonDatabase: '.continuum/database',
            sqliteDatabase: '.continuum/database/continuum.db'
          },
          migration: {
            autoMigrate: true,
            migrateOnWrite: true
          }
        };

      case 'compatibility':
        return {
          mode: DataServiceMode.HYBRID_COMPAT,
          paths: { 
            jsonDatabase: '.continuum/database',
            sqliteDatabase: '.continuum/database/continuum.db'
          },
          migration: {
            autoMigrate: false,
            migrateOnWrite: false
          }
        };

      default:
        return {
          mode: DataServiceMode.JSON_ONLY,
          paths: { jsonDatabase: '.continuum/database' }
        };
    }
  }
}