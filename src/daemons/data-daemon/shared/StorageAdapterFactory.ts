/**
 * Storage Adapter Factory - Plugin System
 * 
 * Creates storage adapters dynamically based on configuration.
 * Supports plugin registration for extensible storage backends.
 */

import type { 
  DataStorageAdapter, 
  StorageAdapterConfig, 
  StorageCapabilities 
} from './DataStorageAdapter';
import { FileStorageAdapter } from '../server/FileStorageAdapter';
import { MemoryStorageAdapter } from '../server/MemoryStorageAdapter';

/**
 * Plugin interface for storage adapter registration
 */
export interface StorageAdapterPlugin {
  readonly name: string;
  readonly strategy: string;
  readonly backends: readonly string[];
  createAdapter(config: StorageAdapterConfig): DataStorageAdapter;
  getCapabilities(): StorageCapabilities;
}

/**
 * Workload requirements for strategy recommendation
 */
export interface WorkloadRequirements {
  readonly expectedRecords: number;
  readonly requiresTransactions?: boolean;
  readonly requiresPersistence?: boolean;
  readonly frequentQueries?: boolean;
  readonly averageRecordSize?: number;
  readonly queriesPerSecond?: number;
}

/**
 * Resource usage tracking
 */
export interface ResourceUsage {
  readonly totalAdapters: number;
  readonly memoryAdapters: number;
  readonly fileAdapters: number;
  readonly sqlAdapters: number;
  readonly activeConnections: number;
}

/**
 * Storage Adapter Factory - Plugin Architecture
 */
export class StorageAdapterFactory {
  private plugins = new Map<string, StorageAdapterPlugin>();
  private activeAdapters = new Set<DataStorageAdapter>();
  
  constructor() {
    this.registerBuiltinPlugins();
  }
  
  /**
   * Create adapter based on configuration
   */
  createAdapter(config: StorageAdapterConfig): DataStorageAdapter {
    this.validateConfig(config);
    
    const plugin = this.plugins.get(config.type);
    if (!plugin) {
      throw new Error(`Unknown storage adapter type: ${config.type}`);
    }
    
    try {
      const adapter = plugin.createAdapter(config);
      this.activeAdapters.add(adapter);
      return adapter;
    } catch (error) {
      throw new Error(`Failed to create adapter ${config.type}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  
  /**
   * Register custom plugin
   */
  registerPlugin(plugin: StorageAdapterPlugin): void {
    this.validatePlugin(plugin);
    
    if (this.plugins.has(plugin.name)) {
      throw new Error(`Plugin ${plugin.name} already registered`);
    }
    
    this.plugins.set(plugin.name, plugin);
  }
  
  /**
   * Get registered plugin names
   */
  getRegisteredPlugins(): string[] {
    return Array.from(this.plugins.keys());
  }
  
  /**
   * Get plugin capabilities
   */
  getPluginCapabilities(pluginName: string): StorageCapabilities | undefined {
    const plugin = this.plugins.get(pluginName);
    return plugin?.getCapabilities();
  }
  
  /**
   * Get supported strategies
   */
  getSupportedStrategies(): string[] {
    const strategies = new Set<string>();
    for (const plugin of this.plugins.values()) {
      strategies.add(plugin.strategy);
    }
    return Array.from(strategies);
  }
  
  /**
   * Get recommended backends for strategy
   */
  getRecommendedBackends(strategy: string): string[] {
    const backends = new Set<string>();
    for (const plugin of this.plugins.values()) {
      if (plugin.strategy === strategy) {
        plugin.backends.forEach(backend => backends.add(backend));
      }
    }
    return Array.from(backends);
  }
  
  /**
   * Recommend optimal strategy based on workload
   */
  recommendStrategy(requirements: WorkloadRequirements): string {
    const { expectedRecords, requiresTransactions, requiresPersistence, frequentQueries } = requirements;
    
    // Strategy selection logic
    if (expectedRecords < 1000 && !requiresPersistence && frequentQueries) {
      return 'memory';
    }
    
    if (expectedRecords < 10000 && requiresPersistence && !requiresTransactions) {
      return 'file';
    }
    
    if (requiresTransactions || expectedRecords > 50000) {
      return 'sql'; // Would recommend SQL if available
    }
    
    if (expectedRecords > 100000) {
      return 'nosql'; // Would recommend NoSQL if available
    }
    
    // Default fallback
    return requiresPersistence ? 'file' : 'memory';
  }
  
  /**
   * Select best available strategy from preferences
   */
  selectBestAvailableStrategy(preferences: string[]): string {
    for (const preference of preferences) {
      if (this.getSupportedStrategies().includes(preference)) {
        return preference;
      }
    }
    
    // Fallback to first available
    const available = this.getSupportedStrategies();
    if (available.length === 0) {
      throw new Error('No storage strategies available');
    }
    return available[0];
  }
  
  /**
   * Generate optimal configuration for workload
   */
  generateOptimalConfig(
    strategy: string, 
    namespace: string, 
    workload: WorkloadRequirements
  ): StorageAdapterConfig {
    const { expectedRecords, averageRecordSize, queriesPerSecond, requiresPersistence } = workload;
    
    let options: Record<string, any> = {};
    
    if (strategy === 'memory') {
      options = {
        maxRecords: Math.max(expectedRecords * 1.2, 1000), // 20% buffer
        enablePersistence: requiresPersistence || false
      };
    } else if (strategy === 'file') {
      options = {
        basePath: `/tmp/jtag-${namespace}`,
        createDirectories: true,
        atomicWrites: true,
        enableIndexes: (queriesPerSecond || 0) > 100 // Enable indexing for high query load
      };
    }
    
    return {
      type: strategy as any,
      namespace,
      options
    };
  }
  
  /**
   * Get active adapters
   */
  getActiveAdapters(): DataStorageAdapter[] {
    return Array.from(this.activeAdapters);
  }
  
  /**
   * Get resource usage statistics
   */
  getResourceUsage(): ResourceUsage {
    let memoryAdapters = 0;
    let fileAdapters = 0;
    let sqlAdapters = 0;
    
    for (const adapter of this.activeAdapters) {
      if (adapter instanceof MemoryStorageAdapter) {
        memoryAdapters++;
      } else if (adapter instanceof FileStorageAdapter) {
        fileAdapters++;
      } else {
        sqlAdapters++;
      }
    }
    
    return {
      totalAdapters: this.activeAdapters.size,
      memoryAdapters,
      fileAdapters,
      sqlAdapters,
      activeConnections: this.activeAdapters.size
    };
  }
  
  /**
   * Close all active adapters
   */
  async closeAllAdapters(): Promise<void> {
    const closePromises = Array.from(this.activeAdapters).map(async adapter => {
      try {
        await adapter.close();
        this.activeAdapters.delete(adapter);
      } catch (error) {
        console.error('Error closing adapter:', error);
      }
    });
    
    await Promise.all(closePromises);
  }
  
  /**
   * Register built-in plugins
   */
  private registerBuiltinPlugins(): void {
    // File storage plugin
    this.plugins.set('file', {
      name: 'file',
      strategy: 'file',
      backends: ['json'],
      createAdapter: (config: StorageAdapterConfig) => new FileStorageAdapter(),
      getCapabilities: (): StorageCapabilities => ({
        supportsTransactions: false,
        supportsIndexing: true,
        supportsFullTextSearch: false,
        supportsReplication: false,
        maxRecordSize: 100 * 1024 * 1024, // 100MB
        concurrentConnections: 1
      })
    });
    
    // Memory storage plugin
    this.plugins.set('memory', {
      name: 'memory',
      strategy: 'memory',
      backends: ['memory'],
      createAdapter: (config: StorageAdapterConfig) => new MemoryStorageAdapter(),
      getCapabilities: (): StorageCapabilities => ({
        supportsTransactions: false,
        supportsIndexing: true,
        supportsFullTextSearch: true,
        supportsReplication: false,
        maxRecordSize: 10 * 1024 * 1024, // 10MB
        concurrentConnections: 100
      })
    });
  }
  
  /**
   * Validate configuration
   */
  private validateConfig(config: StorageAdapterConfig): void {
    if (!config.type || typeof config.type !== 'string') {
      throw new Error('Storage adapter type is required');
    }
    
    if (!config.namespace || typeof config.namespace !== 'string' || config.namespace.trim() === '') {
      throw new Error('Storage adapter namespace is required and cannot be empty');
    }
    
    // Type-specific validation
    if (config.type === 'file') {
      if (config.options?.basePath === '') {
        throw new Error('File adapter basePath cannot be empty');
      }
    }
  }
  
  /**
   * Validate plugin interface
   */
  private validatePlugin(plugin: any): void {
    const requiredFields = ['name', 'strategy', 'backends', 'createAdapter', 'getCapabilities'];
    
    for (const field of requiredFields) {
      if (!plugin[field]) {
        throw new Error(`Invalid plugin: missing ${field}`);
      }
    }
    
    // Validate capabilities structure
    try {
      const capabilities = plugin.getCapabilities();
      const requiredCapabilities = [
        'supportsTransactions', 
        'supportsIndexing', 
        'supportsFullTextSearch', 
        'supportsReplication',
        'maxRecordSize',
        'concurrentConnections'
      ];
      
      for (const cap of requiredCapabilities) {
        if (capabilities[cap] === undefined) {
          throw new Error(`Invalid capabilities: missing ${cap}`);
        }
      }
    } catch (error) {
      throw new Error(`Invalid plugin capabilities: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
}