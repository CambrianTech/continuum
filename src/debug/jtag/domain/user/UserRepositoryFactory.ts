/**
 * User Repository Factory - Creates repositories with different storage backends
 *
 * Easy switching between SQL, NoSQL, JSON, Memory, etc. via configuration.
 * Same repository API works with any adapter - just change the config.
 */

import { DataDaemon, type StorageStrategyConfig } from '../../daemons/data-daemon/shared/DataDaemon';
import { UserRepository, HumanUserRepository, AgentUserRepository, PersonaUserRepository } from './UserRepository';

/**
 * Repository configuration presets for different use cases
 */
export interface RepositoryConfig {
  name: string;
  description: string;
  storageConfig: StorageStrategyConfig;
}

export const REPOSITORY_PRESETS: Record<string, RepositoryConfig> = {
  // Development: Fast JSON files, easy to inspect
  development: {
    name: 'Development JSON',
    description: 'JSON files for easy development and debugging - compatible with JTAG commands',
    storageConfig: {
      strategy: 'file',
      backend: 'json',
      namespace: '', // No namespace - save directly to JTAG data path for full compatibility
      options: {
        basePath: '.continuum/jtag/data', // Match JTAG server configuration
        prettyPrint: true,
        backupOnWrite: true
      },
      features: {
        enableIndexing: false,
        enableCaching: true
      }
    }
  },

  // Production: High-performance SQLite
  production: {
    name: 'Production SQLite',
    description: 'SQLite database for production performance',
    storageConfig: {
      strategy: 'sql',
      backend: 'sqlite',
      namespace: 'continuum-prod',
      options: {
        filename: '.continuum/database/continuum.db',
        foreignKeys: true,
        wal: true, // Write-Ahead Logging for performance
        synchronous: 'NORMAL',
        journalMode: 'WAL'
      },
      features: {
        enableTransactions: true,
        enableIndexing: true,
        enableCaching: true
      }
    }
  },

  // Scale: PostgreSQL for large deployments
  scale: {
    name: 'Scale PostgreSQL',
    description: 'PostgreSQL for large-scale deployments',
    storageConfig: {
      strategy: 'sql',
      backend: 'postgresql',
      namespace: 'continuum-scale',
      options: {
        host: process.env.DB_HOST || 'localhost',
        port: parseInt(process.env.DB_PORT || '5432'),
        database: process.env.DB_NAME || 'continuum',
        user: process.env.DB_USER || 'continuum',
        password: process.env.DB_PASSWORD,
        ssl: process.env.DB_SSL === 'true',
        poolSize: 10
      },
      features: {
        enableTransactions: true,
        enableIndexing: true,
        enableReplication: true,
        enableCaching: true
      }
    }
  },

  // Testing: In-memory for fast tests
  testing: {
    name: 'Testing Memory',
    description: 'In-memory storage for fast testing',
    storageConfig: {
      strategy: 'memory',
      backend: 'memory',
      namespace: 'continuum-test',
      options: {
        maxSize: '100MB',
        persistence: false
      },
      features: {
        enableTransactions: false,
        enableIndexing: false,
        enableCaching: false
      }
    }
  },

  // NoSQL: MongoDB for document-based storage
  nosql: {
    name: 'NoSQL MongoDB',
    description: 'MongoDB for flexible document storage',
    storageConfig: {
      strategy: 'nosql',
      backend: 'mongodb',
      namespace: 'continuum-nosql',
      options: {
        uri: process.env.MONGODB_URI || 'mongodb://localhost:27017/continuum',
        collection: 'users',
        indexes: [
          { fields: { userId: 1 }, unique: true },
          { fields: { citizenType: 1, aiType: 1 } },
          { fields: { lastActiveAt: -1 } }
        ]
      },
      features: {
        enableTransactions: true,
        enableIndexing: true,
        enableSharding: true,
        enableCaching: true
      }
    }
  },

  // Hybrid: JSON for development, SQLite for production
  hybrid: {
    name: 'Hybrid Migration',
    description: 'JSON files migrating to SQLite',
    storageConfig: {
      strategy: 'hybrid',
      backend: 'json-to-sqlite',
      namespace: 'continuum-hybrid',
      options: {
        readFrom: '.continuum/database', // JSON files
        writeTo: '.continuum/database/continuum.db', // SQLite
        migrationMode: 'lazy', // Migrate on read
        keepOriginals: true
      },
      features: {
        enableTransactions: true,
        enableIndexing: true,
        enableCaching: true
      }
    }
  }
};

/**
 * Factory for creating repositories with different storage backends
 */
export class UserRepositoryFactory {
  private static instances = new Map<string, {
    dataDaemon: DataDaemon;
    userRepository: UserRepository;
    humanRepository: HumanUserRepository;
    agentRepository: AgentUserRepository;
    personaRepository: PersonaUserRepository;
  }>();

  /**
   * Create repositories with preset configuration
   */
  static async createWithPreset(presetName: keyof typeof REPOSITORY_PRESETS) {
    const preset = REPOSITORY_PRESETS[presetName];
    if (!preset) {
      throw new Error(`Unknown preset: ${presetName}`);
    }

    return await this.createWithConfig(preset.storageConfig, presetName);
  }

  /**
   * Create repositories with custom configuration
   */
  static async createWithConfig(config: StorageStrategyConfig, instanceKey: string = 'default') {
    // Check if we already have this instance
    if (this.instances.has(instanceKey)) {
      return this.instances.get(instanceKey)!;
    }

    // Create new DataDaemon with the configuration
    const dataDaemon = new DataDaemon(config);
    await dataDaemon.initialize();

    // Create all repository types
    const userRepository = new UserRepository(dataDaemon);
    const humanRepository = new HumanUserRepository(dataDaemon);
    const agentRepository = new AgentUserRepository(dataDaemon);
    const personaRepository = new PersonaUserRepository(dataDaemon);

    const instance = {
      dataDaemon,
      userRepository,
      humanRepository,
      agentRepository,
      personaRepository
    };

    this.instances.set(instanceKey, instance);
    return instance;
  }

  /**
   * Get existing instance
   */
  static getInstance(instanceKey: string = 'default') {
    const instance = this.instances.get(instanceKey);
    if (!instance) {
      throw new Error(`No repository instance found for key: ${instanceKey}`);
    }
    return instance;
  }

  /**
   * Create development repositories (JSON files)
   */
  static async createForDevelopment() {
    return await this.createWithPreset('development');
  }

  /**
   * Create production repositories (SQLite)
   */
  static async createForProduction() {
    return await this.createWithPreset('production');
  }

  /**
   * Create testing repositories (Memory)
   */
  static async createForTesting() {
    return await this.createWithPreset('testing');
  }

  /**
   * Create scale repositories (PostgreSQL)
   */
  static async createForScale() {
    return await this.createWithPreset('scale');
  }

  /**
   * List available presets
   */
  static getAvailablePresets(): Array<{ key: string; config: RepositoryConfig }> {
    return Object.entries(REPOSITORY_PRESETS).map(([key, config]) => ({ key, config }));
  }

  /**
   * Migrate between storage backends
   */
  static async migrate(
    fromInstanceKey: string,
    toInstanceKey: string,
    collections: string[] = ['users', 'user_sessions', 'user_permissions', 'room_participations']
  ) {
    const fromInstance = this.getInstance(fromInstanceKey);
    const toInstance = this.getInstance(toInstanceKey);

    console.log(`Migrating data from ${fromInstanceKey} to ${toInstanceKey}...`);

    for (const collection of collections) {
      console.log(`Migrating collection: ${collection}`);

      // Get all data from source
      const sourceResult = await fromInstance.dataDaemon.query({
        collection,
        filters: {}
      }, {
        sessionId: 'migration' as any,
        timestamp: new Date().toISOString(),
        source: 'migration'
      });

      if (sourceResult.success && sourceResult.data) {
        // Write all data to destination
        for (const record of sourceResult.data) {
          await toInstance.dataDaemon.create(
            collection,
            record.data,
            {
              sessionId: 'migration' as any,
              timestamp: new Date().toISOString(),
              source: 'migration'
            },
            record.id
          );
        }
        console.log(`Migrated ${sourceResult.data.length} records from ${collection}`);
      }
    }

    console.log('Migration completed successfully');
  }

  /**
   * Close all instances and clean up connections
   */
  static async closeAll() {
    for (const [key, instance] of this.instances.entries()) {
      try {
        await instance.dataDaemon.close();
        console.log(`Closed repository instance: ${key}`);
      } catch (error) {
        console.error(`Error closing repository instance ${key}:`, error);
      }
    }
    this.instances.clear();
  }

  /**
   * Close specific instance
   */
  static async close(instanceKey: string = 'default') {
    const instance = this.instances.get(instanceKey);
    if (instance) {
      await instance.dataDaemon.close();
      this.instances.delete(instanceKey);
    }
  }
}

/**
 * Convenience functions for common scenarios
 */

/**
 * Get repositories configured for current environment
 */
export async function getRepositories() {
  const env = process.env.NODE_ENV || 'development';

  switch (env) {
    case 'production':
      return await UserRepositoryFactory.createForProduction();
    case 'test':
      return await UserRepositoryFactory.createForTesting();
    case 'development':
    default:
      return await UserRepositoryFactory.createForDevelopment();
  }
}

/**
 * Quick access to user repository for current environment
 */
export async function getUserRepository(): Promise<UserRepository> {
  const { userRepository } = await getRepositories();
  return userRepository;
}