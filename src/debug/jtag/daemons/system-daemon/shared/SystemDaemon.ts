/**
 * SystemDaemon - System Configuration Manager
 *
 * Server-side singleton that provides efficient access to system configuration
 * with clean caching and event-driven invalidation.
 *
 * Philosophy:
 * - Query once, serve forever (no repeated DB hits)
 * - Clean cache invalidation via filtered events
 * - Direct DataDaemon communication
 * - Single source of truth for system config
 *
 * Usage:
 * ```typescript
 * const daemon = SystemDaemon.sharedInstance();
 * const config = daemon.getConfig();
 * const value = config.get('system/scheduling/timings/adapter-health-check');
 * await daemon.updateSetting('system/scheduling/timings/adapter-health-check', 45000, userId, 'System under load');
 * ```
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import { Events } from '../../../system/core/shared/Events';
import { ORM } from '../../data-daemon/server/ORM';
import { SystemConfigEntity, FACTORY_DEFAULTS, type SettingValue } from '../../../system/data/entities/SystemConfigEntity';
import type { StorageQuery, StorageResult } from '../../data-daemon/shared/DataStorageAdapter';
import { Logger } from '../../../system/core/logging/Logger';

const log = Logger.create('SystemDaemon', 'data');

/**
 * SystemDaemon - Efficient System Configuration Manager
 *
 * ARCHITECTURE:
 * - Server-side singleton (one instance per server process)
 * - Caches SystemConfigEntity after first query
 * - Subscribes to filtered events for cache invalidation
 * - Provides fast, concurrent access to config
 * - All config operations route through this daemon
 */
export class SystemDaemon {
  private static instance: SystemDaemon | null = null;
  private configCache: SystemConfigEntity | null = null;
  private context: JTAGContext;
  private isInitialized: boolean = false;

  private constructor(context: JTAGContext) {
    this.context = context;
  }

  /**
   * Get shared singleton instance
   */
  static sharedInstance(): SystemDaemon {
    if (!SystemDaemon.instance) {
      throw new Error('SystemDaemon not initialized - call SystemDaemon.initialize(context) first');
    }
    return SystemDaemon.instance;
  }

  /**
   * Initialize SystemDaemon (called by DataDaemonServer during startup)
   */
  static async initialize(context: JTAGContext): Promise<SystemDaemon> {
    if (SystemDaemon.instance) {
      return SystemDaemon.instance;
    }

    log.info('Initializing SystemDaemon...');
    const daemon = new SystemDaemon(context);
    await daemon.initializeInternal();
    SystemDaemon.instance = daemon;
    log.info('SystemDaemon initialized successfully');

    return daemon;
  }

  /**
   * Internal initialization - load config and subscribe to events
   */
  private async initializeInternal(): Promise<void> {
    if (this.isInitialized) {
      return;
    }

    // Query for singleton config entity
    log.info('Loading system configuration...');
    const query: StorageQuery = {
      collection: SystemConfigEntity.collection,
      filter: { name: 'default' },
      limit: 1
    };

    const result = await ORM.query<SystemConfigEntity>(query);

    if (!result.success || !result.data || result.data.length === 0) {
      // Config doesn't exist - create with factory defaults
      log.info('No system config found, creating with factory defaults...');
      this.configCache = await this.createDefaultConfig();
    } else {
      // Config exists - cache it
      this.configCache = result.data[0].data;
      log.info('System configuration loaded from database');
    }

    // Subscribe to config update events with FILTER
    // Only receive events for our singleton config entity (name='default')
    await Events.subscribe(
      'data:system_config:updated',
      (entity: SystemConfigEntity) => this.onConfigUpdated(entity),
      { where: { name: 'default' } } // ✅ Filtered subscription - only this entity
    );

    log.info('Subscribed to filtered system_config update events');

    this.isInitialized = true;
  }

  /**
   * Create default config entity with factory defaults
   */
  private async createDefaultConfig(): Promise<SystemConfigEntity> {
    const config = new SystemConfigEntity();
    config.name = 'default';
    config.settings = {};
    config.systemState = {
      currentLoad: 0.0,
      activeAICount: 1,
      lastUpdated: Date.now()
    };

    // Register all factory default settings
    for (const [path, metadata] of Object.entries(FACTORY_DEFAULTS)) {
      config.registerSetting(path, metadata as any);
    }

    // Store in database
    const storedConfig = await ORM.store<SystemConfigEntity>(
      SystemConfigEntity.collection,
      config
    );

    log.info('Created default system configuration with factory defaults');
    return storedConfig;
  }

  /**
   * Event handler for config updates - clean cache invalidation
   */
  private onConfigUpdated(entity: SystemConfigEntity): void {
    log.info('System configuration updated, refreshing cache...');
    this.configCache = entity;
  }

  /**
   * Get cached config entity (fast - no DB query)
   */
  getConfig(): SystemConfigEntity {
    if (!this.configCache) {
      throw new Error('SystemDaemon not initialized');
    }
    return this.configCache;
  }

  /**
   * Get setting value by path
   *
   * @example
   * const interval = daemon.getSetting('system/scheduling/timings/adapter-health-check');
   */
  getSetting(path: string): SettingValue | undefined {
    return this.configCache?.get(path);
  }

  /**
   * Update setting value
   *
   * @example
   * await daemon.updateSetting(
   *   'system/scheduling/timings/adapter-health-check',
   *   45000,
   *   userId,
   *   'System under heavy load'
   * );
   */
  async updateSetting(
    path: string,
    value: SettingValue,
    changedBy: UUID,
    reason?: string
  ): Promise<void> {
    if (!this.configCache) {
      throw new Error('SystemDaemon not initialized');
    }

    // Update in-memory cache
    this.configCache.set(path, value, changedBy, reason);

    // Persist to database (event automatically emitted by DataDaemon)
    await ORM.update<SystemConfigEntity>(
      SystemConfigEntity.collection,
      this.configCache.id,
      this.configCache
    );

    log.info(`Updated setting ${path} = ${value} (reason: ${reason || 'none'})`);
  }

  /**
   * Reset setting to factory default
   */
  async resetSetting(path: string, changedBy: UUID): Promise<void> {
    if (!this.configCache) {
      throw new Error('SystemDaemon not initialized');
    }

    this.configCache.reset(path, changedBy);

    // Persist to database
    await ORM.update<SystemConfigEntity>(
      SystemConfigEntity.collection,
      this.configCache.id,
      this.configCache
    );

    log.info(`Reset setting ${path} to factory default`);
  }

  /**
   * Reset entire group to factory defaults
   */
  async resetGroup(groupPath: string, changedBy: UUID): Promise<void> {
    if (!this.configCache) {
      throw new Error('SystemDaemon not initialized');
    }

    this.configCache.resetGroup(groupPath, changedBy);

    // Persist to database
    await ORM.update<SystemConfigEntity>(
      SystemConfigEntity.collection,
      this.configCache.id,
      this.configCache
    );

    log.info(`Reset group ${groupPath} to factory defaults`);
  }

  /**
   * Get all settings under a group path
   *
   * @example
   * const schedulingSettings = daemon.getGroup('system/scheduling');
   */
  getGroup(groupPath: string): { [path: string]: any } {
    if (!this.configCache) {
      throw new Error('SystemDaemon not initialized');
    }
    return this.configCache.getGroup(groupPath);
  }

  /**
   * Update system state (runtime values, not settings)
   */
  async updateSystemState(state: Partial<SystemConfigEntity['systemState']>): Promise<void> {
    if (!this.configCache) {
      throw new Error('SystemDaemon not initialized');
    }

    // Update in-memory cache
    this.configCache.systemState = {
      ...this.configCache.systemState,
      ...state,
      lastUpdated: Date.now()
    };

    // Persist to database
    await ORM.update<SystemConfigEntity>(
      SystemConfigEntity.collection,
      this.configCache.id,
      { systemState: this.configCache.systemState }
    );
  }

  /**
   * Get current system state
   */
  getSystemState(): SystemConfigEntity['systemState'] {
    if (!this.configCache) {
      throw new Error('SystemDaemon not initialized');
    }
    return this.configCache.systemState;
  }

  /**
   * Shutdown daemon
   */
  async shutdown(): Promise<void> {
    log.info('Shutting down SystemDaemon...');
    this.isInitialized = false;
    this.configCache = null;
    SystemDaemon.instance = null;
  }
}
