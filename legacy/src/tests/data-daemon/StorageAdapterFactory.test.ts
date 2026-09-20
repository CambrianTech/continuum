/**
 * Storage Adapter Factory - Unit Tests
 * 
 * Tests the plugin system architecture and storage adapter factory.
 * Validates that adapters can be created dynamically and plugins work correctly.
 */

import { describe, it, expect, beforeEach, afterEach } from '@jest/globals';
import { StorageAdapterFactory } from '../../daemons/data-daemon/shared/StorageAdapterFactory';
import { FileStorageAdapter } from '../../daemons/data-daemon/server/FileStorageAdapter';
import { MemoryStorageAdapter } from '../../daemons/data-daemon/server/MemoryStorageAdapter';
import { 
  DataStorageAdapter, 
  StorageAdapterConfig,
  StorageCapabilities
} from '../../daemons/data-daemon/shared/DataStorageAdapter';

describe('StorageAdapterFactory', () => {
  let factory: StorageAdapterFactory;

  beforeEach(() => {
    factory = new StorageAdapterFactory();
  });

  describe('Built-in Adapter Creation', () => {
    it('should create FileStorageAdapter for file strategy', () => {
      const config: StorageAdapterConfig = {
        type: 'file',
        namespace: 'test-file',
        options: {
          basePath: '/tmp/factory-test',
          createDirectories: true
        }
      };

      const adapter = factory.createAdapter(config);

      expect(adapter).toBeInstanceOf(FileStorageAdapter);
    });

    it('should create MemoryStorageAdapter for memory strategy', () => {
      const config: StorageAdapterConfig = {
        type: 'memory',
        namespace: 'test-memory',
        options: {
          maxRecords: 1000,
          enablePersistence: false
        }
      };

      const adapter = factory.createAdapter(config);

      expect(adapter).toBeInstanceOf(MemoryStorageAdapter);
    });

    it('should throw error for unknown adapter type', () => {
      const config: StorageAdapterConfig = {
        type: 'unknown-type' as any,
        namespace: 'test-unknown'
      };

      expect(() => factory.createAdapter(config)).toThrow('Unknown storage adapter type');
    });
  });

  describe('Plugin Registration System', () => {
    // Mock plugin for testing
    class MockDatabaseAdapter implements DataStorageAdapter {
      async initialize(config: StorageAdapterConfig): Promise<void> {
        // Mock implementation
      }

      async create(record: any): Promise<any> {
        return { success: true, data: record };
      }

      async read(collection: string, id: any): Promise<any> {
        return { success: true, data: undefined };
      }

      async update(collection: string, id: any, updateData: any): Promise<any> {
        return { success: true, data: undefined };
      }

      async delete(collection: string, id: any): Promise<any> {
        return { success: true, data: true };
      }

      async query(query: any): Promise<any> {
        return { success: true, data: [] };
      }

      async listCollections(): Promise<any> {
        return { success: true, data: [] };
      }

      async getCollectionStats(collection: string): Promise<any> {
        return { success: true, data: { name: collection, recordCount: 0 } };
      }

      async batch(operations: any[]): Promise<any> {
        return { success: true, data: [] };
      }

      async cleanup(): Promise<any> {
        return { success: true, data: true };
      }

      async close(): Promise<void> {
        // Mock implementation
      }
    }

    const mockPlugin = {
      name: 'mock-database',
      strategy: 'sql',
      backends: ['mockdb'],
      createAdapter: (config: StorageAdapterConfig) => new MockDatabaseAdapter(),
      getCapabilities: (): StorageCapabilities => ({
        supportsTransactions: true,
        supportsIndexing: true,
        supportsFullTextSearch: false,
        supportsReplication: false,
        maxRecordSize: 1024 * 1024,
        concurrentConnections: 10
      })
    };

    it('should register custom plugin successfully', () => {
      factory.registerPlugin(mockPlugin);

      const registeredPlugins = factory.getRegisteredPlugins();
      expect(registeredPlugins).toContain('mock-database');
    });

    it('should create adapter from registered plugin', () => {
      factory.registerPlugin(mockPlugin);

      const config: StorageAdapterConfig = {
        type: 'mock-database' as any,
        namespace: 'test-mock',
        options: {}
      };

      const adapter = factory.createAdapter(config);
      expect(adapter).toBeInstanceOf(MockDatabaseAdapter);
    });

    it('should get plugin capabilities', () => {
      factory.registerPlugin(mockPlugin);

      const capabilities = factory.getPluginCapabilities('mock-database');

      expect(capabilities?.supportsTransactions).toBe(true);
      expect(capabilities?.supportsIndexing).toBe(true);
      expect(capabilities?.maxRecordSize).toBe(1024 * 1024);
    });

    it('should prevent duplicate plugin registration', () => {
      factory.registerPlugin(mockPlugin);

      expect(() => factory.registerPlugin(mockPlugin)).toThrow('already registered');
    });

    it('should list supported strategies', () => {
      factory.registerPlugin(mockPlugin);

      const strategies = factory.getSupportedStrategies();

      expect(strategies).toContain('file');
      expect(strategies).toContain('memory'); 
      expect(strategies).toContain('sql'); // From mock plugin
    });
  });

  describe('Adapter Lifecycle Management', () => {
    it('should track active adapters', async () => {
      const config1: StorageAdapterConfig = {
        type: 'memory',
        namespace: 'tracker-1',
        options: { maxRecords: 100 }
      };

      const config2: StorageAdapterConfig = {
        type: 'memory',
        namespace: 'tracker-2',
        options: { maxRecords: 200 }
      };

      const adapter1 = factory.createAdapter(config1);
      const adapter2 = factory.createAdapter(config2);

      await adapter1.initialize(config1);
      await adapter2.initialize(config2);

      const activeAdapters = factory.getActiveAdapters();
      expect(activeAdapters).toHaveLength(2);

      await adapter1.close();
      await adapter2.close();
    });

    it('should cleanup all adapters on factory shutdown', async () => {
      const config: StorageAdapterConfig = {
        type: 'memory',
        namespace: 'cleanup-test',
        options: { maxRecords: 100 }
      };

      const adapter = factory.createAdapter(config);
      await adapter.initialize(config);

      expect(factory.getActiveAdapters()).toHaveLength(1);

      await factory.closeAllAdapters();

      expect(factory.getActiveAdapters()).toHaveLength(0);
    });
  });

  describe('Configuration Validation', () => {
    it('should validate required configuration fields', () => {
      const invalidConfigs = [
        { type: 'file' }, // Missing namespace
        { namespace: 'test' }, // Missing type
        { type: 'file', namespace: '' }, // Empty namespace
        { type: '', namespace: 'test' } // Empty type
      ];

      for (const config of invalidConfigs) {
        expect(() => factory.createAdapter(config as any)).toThrow();
      }
    });

    it('should validate adapter-specific options', () => {
      const fileConfigWithInvalidOptions: StorageAdapterConfig = {
        type: 'file',
        namespace: 'validation-test',
        options: {
          basePath: '', // Invalid empty path
          createDirectories: true
        }
      };

      expect(() => factory.createAdapter(fileConfigWithInvalidOptions)).toThrow('basePath');
    });

    it('should apply default options when not specified', () => {
      const minimalConfig: StorageAdapterConfig = {
        type: 'memory',
        namespace: 'minimal-test'
      };

      const adapter = factory.createAdapter(minimalConfig);
      expect(adapter).toBeInstanceOf(MemoryStorageAdapter);
    });
  });

  describe('Strategy-Backend Mapping', () => {
    it('should map strategies to appropriate backends', () => {
      const strategies = factory.getSupportedStrategies();
      
      expect(strategies).toContain('file');
      expect(strategies).toContain('memory');
      
      // Test that each strategy can create adapters
      for (const strategy of strategies) {
        if (strategy === 'file') {
          const config: StorageAdapterConfig = {
            type: 'file',
            namespace: `strategy-test-${strategy}`,
            options: { basePath: '/tmp/strategy-test' }
          };
          const adapter = factory.createAdapter(config);
          expect(adapter).toBeTruthy();
        } else if (strategy === 'memory') {
          const config: StorageAdapterConfig = {
            type: 'memory',
            namespace: `strategy-test-${strategy}`
          };
          const adapter = factory.createAdapter(config);
          expect(adapter).toBeTruthy();
        }
      }
    });

    it('should provide backend recommendations for strategies', () => {
      const fileRecommendations = factory.getRecommendedBackends('file');
      const memoryRecommendations = factory.getRecommendedBackends('memory');

      expect(fileRecommendations).toContain('json');
      expect(memoryRecommendations).toContain('memory');
    });
  });

  describe('Plugin Discovery and Capabilities', () => {
    it('should discover adapter capabilities automatically', () => {
      const fileCapabilities = factory.getPluginCapabilities('file');
      const memoryCapabilities = factory.getPluginCapabilities('memory');

      expect(fileCapabilities?.supportsTransactions).toBeDefined();
      expect(fileCapabilities?.supportsIndexing).toBeDefined();
      expect(memoryCapabilities?.supportsTransactions).toBeDefined();
      expect(memoryCapabilities?.maxRecordSize).toBeGreaterThan(0);
    });

    it('should recommend optimal strategy based on requirements', () => {
      const requirements = {
        expectedRecords: 1000,
        requiresTransactions: false,
        requiresPersistence: false,
        frequentQueries: true
      };

      const recommendation = factory.recommendStrategy(requirements);

      // Should recommend memory for small, non-persistent, frequent-query workloads
      expect(recommendation).toBe('memory');
    });

    it('should recommend file strategy for persistent requirements', () => {
      const requirements = {
        expectedRecords: 5000,
        requiresTransactions: false,
        requiresPersistence: true,
        frequentQueries: false
      };

      const recommendation = factory.recommendStrategy(requirements);
      expect(recommendation).toBe('file');
    });
  });

  describe('Error Recovery and Fallbacks', () => {
    it('should provide fallback strategies when primary fails', () => {
      const preferences = ['unknown-strategy', 'memory', 'file'];
      const fallback = factory.selectBestAvailableStrategy(preferences);

      expect(fallback).toBe('memory');
    });

    it('should handle adapter creation failures gracefully', () => {
      // Mock a plugin that throws during creation
      const faultyPlugin = {
        name: 'faulty-adapter',
        strategy: 'faulty',
        backends: ['broken'],
        createAdapter: () => { throw new Error('Adapter creation failed'); },
        getCapabilities: () => ({
          supportsTransactions: false,
          supportsIndexing: false,
          supportsFullTextSearch: false,
          supportsReplication: false,
          maxRecordSize: 0,
          concurrentConnections: 0
        })
      };

      factory.registerPlugin(faultyPlugin);

      const config: StorageAdapterConfig = {
        type: 'faulty-adapter' as any,
        namespace: 'faulty-test'
      };

      expect(() => factory.createAdapter(config)).toThrow('Adapter creation failed');
    });
  });

  describe('Performance and Resource Management', () => {
    it('should track adapter resource usage', async () => {
      const configs = Array.from({ length: 5 }, (_, i) => ({
        type: 'memory' as const,
        namespace: `perf-test-${i}`,
        options: { maxRecords: 100 }
      }));

      const adapters = configs.map(config => factory.createAdapter(config));
      
      // Initialize all adapters
      for (let i = 0; i < adapters.length; i++) {
        await adapters[i].initialize(configs[i]);
      }

      expect(factory.getActiveAdapters()).toHaveLength(5);

      // Check resource tracking
      const resourceUsage = factory.getResourceUsage();
      expect(resourceUsage.totalAdapters).toBe(5);
      expect(resourceUsage.memoryAdapters).toBe(5);
      expect(resourceUsage.fileAdapters).toBe(0);

      // Cleanup
      for (const adapter of adapters) {
        await adapter.close();
      }
    });

    it('should prevent resource exhaustion', async () => {
      // Try to create many adapters
      const maxAdapters = 50;
      const adapters: DataStorageAdapter[] = [];

      try {
        for (let i = 0; i < maxAdapters; i++) {
          const config: StorageAdapterConfig = {
            type: 'memory',
            namespace: `resource-test-${i}`,
            options: { maxRecords: 10 }
          };

          const adapter = factory.createAdapter(config);
          await adapter.initialize(config);
          adapters.push(adapter);
        }

        // Should either complete successfully or throw resource limit error
        const activeCount = factory.getActiveAdapters().length;
        expect(activeCount).toBeGreaterThan(0);
        expect(activeCount).toBeLessThanOrEqual(maxAdapters);

      } finally {
        // Cleanup all created adapters
        for (const adapter of adapters) {
          try {
            await adapter.close();
          } catch (e) {
            // Ignore cleanup errors
          }
        }
      }
    });
  });

  describe('Plugin Validation and Security', () => {
    it('should validate plugin interface compliance', () => {
      const incompletePlugin = {
        name: 'incomplete-plugin',
        strategy: 'incomplete'
        // Missing required methods
      };

      expect(() => factory.registerPlugin(incompletePlugin as any)).toThrow('Invalid plugin');
    });

    it('should validate plugin capabilities format', () => {
      const pluginWithBadCapabilities = {
        name: 'bad-capabilities',
        strategy: 'bad',
        backends: ['test'],
        createAdapter: () => new MemoryStorageAdapter(),
        getCapabilities: () => ({
          // Missing required capability fields
          supportsTransactions: true
        })
      };

      expect(() => factory.registerPlugin(pluginWithBadCapabilities as any)).toThrow('Invalid capabilities');
    });

    it('should prevent malicious plugin behavior', () => {
      const maliciousPlugin = {
        name: 'malicious-plugin',
        strategy: 'malicious',
        backends: ['evil'],
        createAdapter: () => {
          // Try to access global scope or do bad things
          (global as any).maliciousCode = true;
          throw new Error('Malicious behavior detected');
        },
        getCapabilities: () => ({
          supportsTransactions: false,
          supportsIndexing: false,
          supportsFullTextSearch: false,
          supportsReplication: false,
          maxRecordSize: 0,
          concurrentConnections: 0
        })
      };

      factory.registerPlugin(maliciousPlugin);

      const config: StorageAdapterConfig = {
        type: 'malicious-plugin' as any,
        namespace: 'malicious-test'
      };

      // Should catch and handle malicious behavior
      expect(() => factory.createAdapter(config)).toThrow();
      expect((global as any).maliciousCode).toBeUndefined();
    });
  });

  describe('Strategy Selection Intelligence', () => {
    it('should rank strategies by suitability', () => {
      const workloadProfiles = [
        {
          name: 'Development',
          requirements: {
            expectedRecords: 100,
            requiresTransactions: false,
            requiresPersistence: true,
            frequentQueries: false
          },
          expectedStrategy: 'file'
        },
        {
          name: 'Caching',
          requirements: {
            expectedRecords: 10000,
            requiresTransactions: false,
            requiresPersistence: false,
            frequentQueries: true
          },
          expectedStrategy: 'memory'
        },
        {
          name: 'Large Dataset',
          requirements: {
            expectedRecords: 100000,
            requiresTransactions: true,
            requiresPersistence: true,
            frequentQueries: true
          },
          expectedStrategy: 'sql' // Would be available with SQL plugins
        }
      ];

      for (const profile of workloadProfiles) {
        const recommendation = factory.recommendStrategy(profile.requirements);
        
        if (profile.expectedStrategy === 'sql') {
          // SQL not available yet, should fallback to file
          expect(['file', 'memory']).toContain(recommendation);
        } else {
          expect(recommendation).toBe(profile.expectedStrategy);
        }
      }
    });

    it('should consider adapter capabilities in recommendations', () => {
      const requiresTransactions = {
        expectedRecords: 1000,
        requiresTransactions: true,
        requiresPersistence: true,
        frequentQueries: false
      };

      const recommendation = factory.recommendStrategy(requiresTransactions);
      
      // Memory doesn't support transactions, should recommend file or SQL
      expect(recommendation).not.toBe('memory');
    });
  });

  describe('Configuration Generation', () => {
    it('should generate optimal configuration for workload', () => {
      const workload = {
        expectedRecords: 5000,
        averageRecordSize: 1024,
        queriesPerSecond: 10,
        requiresPersistence: true
      };

      const config = factory.generateOptimalConfig('file', 'performance-test', workload);

      expect(config.type).toBe('file');
      expect(config.namespace).toBe('performance-test');
      expect(config.options?.atomicWrites).toBe(true); // For reliability
      expect(config.options?.enableIndexes).toBe(true); // For query performance
    });

    it('should adjust configuration based on workload characteristics', () => {
      const highVolumeWorkload = {
        expectedRecords: 100000,
        averageRecordSize: 512,
        queriesPerSecond: 1000,
        requiresPersistence: false
      };

      const config = factory.generateOptimalConfig('memory', 'high-volume-test', highVolumeWorkload);

      expect(config.type).toBe('memory');
      expect(config.options?.maxRecords).toBeGreaterThan(100000);
      expect(config.options?.enablePersistence).toBe(false);
    });
  });
});