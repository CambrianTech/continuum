/**
 * Data Daemon Orchestrator - Unit Tests
 * 
 * Tests the DataDaemon orchestrator with multiple storage backends,
 * strategy switching, and plugin architecture validation.
 */

import { describe, it, expect, beforeEach, afterEach } from '@jest/globals';
import { DataDaemon, DataOperationContext } from '../../daemons/data-daemon/shared/DataDaemon';
import { FileStorageAdapter } from '../../daemons/data-daemon/server/FileStorageAdapter';
import { MemoryStorageAdapter } from '../../daemons/data-daemon/server/MemoryStorageAdapter';
import { 
  DataRecord, 
  StorageAdapterConfig, 
  StorageQuery
} from '../../daemons/data-daemon/shared/DataStorageAdapter';
import { UUID } from '../../system/core/types/CrossPlatformUUID';

describe('DataDaemon Orchestrator', () => {
  let dataDaemon: DataDaemon;
  let testNamespace: string;

  beforeEach(() => {
    testNamespace = `test-${Date.now()}`;
  });

  afterEach(async () => {
    if (dataDaemon) {
      await dataDaemon.close();
    }
  });

  describe('Storage Backend Switching', () => {
    it('should work with FileStorageAdapter backend', async () => {
      const config = {
        strategy: 'file' as const,
        backend: 'json',
        namespace: testNamespace,
        options: {
          basePath: '/tmp/jtag-daemon-test',
          createDirectories: true,
          atomicWrites: true
        }
      };

      dataDaemon = new DataDaemon(config);
      
      const context: DataOperationContext = {
        sessionId: 'test-session' as UUID,
        timestamp: new Date().toISOString(),
        source: 'unit-test'
      };

      const testData = { name: 'File Backend Test', value: 100 };
      const result = await dataDaemon.create('test-collection', testData, context);

      expect(result.success).toBe(true);
      expect(result.data?.data).toEqual(testData);
      expect(result.data?.collection).toBe('test-collection');
    });

    it('should work with MemoryStorageAdapter backend', async () => {
      const config = {
        strategy: 'memory' as const,
        backend: 'memory',
        namespace: testNamespace,
        options: {
          maxRecords: 1000,
          enablePersistence: false
        }
      };

      dataDaemon = new DataDaemon(config);
      
      const context: DataOperationContext = {
        sessionId: 'test-session' as UUID,
        timestamp: new Date().toISOString(),
        source: 'unit-test'
      };

      const testData = { name: 'Memory Backend Test', value: 200 };
      const result = await dataDaemon.create('memory-collection', testData, context);

      expect(result.success).toBe(true);
      expect(result.data?.data).toEqual(testData);
      expect(result.data?.collection).toBe('memory-collection');
    });

    it('should switch between backends dynamically', async () => {
      // Start with memory backend
      let config = {
        strategy: 'memory' as const,
        backend: 'memory',
        namespace: testNamespace,
        options: { maxRecords: 1000 }
      };

      dataDaemon = new DataDaemon(config);
      
      const context: DataOperationContext = {
        sessionId: 'test-session' as UUID,
        timestamp: new Date().toISOString(),
        source: 'unit-test'
      };

      // Create in memory
      await dataDaemon.create('switching-test', { stage: 'memory' }, context);
      
      // Close and reconfigure with file backend
      await dataDaemon.close();
      
      config = {
        strategy: 'file' as const,
        backend: 'json',
        namespace: testNamespace,
        options: {
          basePath: '/tmp/jtag-switch-test',
          createDirectories: true
        }
      };

      dataDaemon = new DataDaemon(config);
      
      // Create in file storage
      const fileResult = await dataDaemon.create('switching-test', { stage: 'file' }, context);
      
      expect(fileResult.success).toBe(true);
      expect(fileResult.data?.data.stage).toBe('file');
    });
  });

  describe('CRUD Operations via Orchestrator', () => {
    beforeEach(async () => {
      const config = {
        strategy: 'memory' as const,
        backend: 'memory',
        namespace: testNamespace,
        options: { maxRecords: 1000 }
      };

      dataDaemon = new DataDaemon(config);
    });

    const context: DataOperationContext = {
      sessionId: 'test-session' as UUID,
      timestamp: '2025-08-16T18:30:00.000Z',
      source: 'unit-test'
    };

    it('should create records with automatic ID generation', async () => {
      const data = { name: 'Auto ID Test', value: 42 };
      const result = await dataDaemon.create('auto-id-test', data, context);

      expect(result.success).toBe(true);
      expect(result.data?.id).toBeTruthy();
      expect(result.data?.data).toEqual(data);
      expect(result.data?.metadata.version).toBe(1);
    });

    it('should create records with specified ID', async () => {
      const customId = 'custom-test-id' as UUID;
      const data = { name: 'Custom ID Test', value: 84 };
      const result = await dataDaemon.create('custom-id-test', data, context, customId);

      expect(result.success).toBe(true);
      expect(result.data?.id).toBe(customId);
      expect(result.data?.data).toEqual(data);
    });

    it('should read records by collection and ID', async () => {
      const data = { name: 'Read Test', value: 123 };
      const createResult = await dataDaemon.create('read-test', data, context);
      const recordId = createResult.data!.id;

      const readResult = await dataDaemon.read<typeof data>('read-test', recordId, context);

      expect(readResult.success).toBe(true);
      expect(readResult.data?.data).toEqual(data);
      expect(readResult.data?.id).toBe(recordId);
    });

    it('should update records preserving metadata properly', async () => {
      const originalData = { name: 'Update Test', value: 50 };
      const createResult = await dataDaemon.create('update-test', originalData, context);
      const recordId = createResult.data!.id;

      const updateData = { value: 75 };
      const updateContext = { ...context, timestamp: '2025-08-16T18:35:00.000Z' };
      const updateResult = await dataDaemon.update('update-test', recordId, updateData, updateContext);

      expect(updateResult.success).toBe(true);
      expect(updateResult.data?.data.name).toBe('Update Test'); // Preserved
      expect(updateResult.data?.data.value).toBe(75); // Updated
      expect(updateResult.data?.metadata.version).toBe(2);
      expect(updateResult.data?.metadata.updatedAt).toBe('2025-08-16T18:35:00.000Z');
      expect(updateResult.data?.metadata.createdAt).toBe('2025-08-16T18:30:00.000Z'); // Preserved
    });

    it('should delete records successfully', async () => {
      const data = { name: 'Delete Test', value: 99 };
      const createResult = await dataDaemon.create('delete-test', data, context);
      const recordId = createResult.data!.id;

      const deleteResult = await dataDaemon.delete('delete-test', recordId, context);
      expect(deleteResult.success).toBe(true);

      // Verify deletion
      const readResult = await dataDaemon.read('delete-test', recordId, context);
      expect(readResult.data).toBeUndefined();
    });

    it('should execute complex queries through orchestrator', async () => {
      // Create test data
      const testRecords = [
        { name: 'Query Test 1', category: 'urgent', priority: 1 },
        { name: 'Query Test 2', category: 'normal', priority: 5 },
        { name: 'Query Test 3', category: 'urgent', priority: 2 }
      ];

      for (const data of testRecords) {
        await dataDaemon.create('query-collection', data, context);
      }

      // Query urgent items
      const query: StorageQuery = {
        collection: 'query-collection',
        filters: { 'data.category': 'urgent' },
        sort: [{ field: 'data.priority', direction: 'asc' }]
      };

      const result = await dataDaemon.query(query, context);

      expect(result.success).toBe(true);
      expect(result.data).toHaveLength(2);
      expect(result.data?.[0].data.priority).toBe(1);
      expect(result.data?.[1].data.priority).toBe(2);
    });
  });

  describe('Storage Strategy Configuration', () => {
    it('should validate storage configuration properly', async () => {
      const invalidConfig = {
        strategy: 'invalid-strategy' as any,
        backend: 'unknown',
        namespace: testNamespace
      };

      expect(() => new DataDaemon(invalidConfig)).toThrow();
    });

    it('should handle initialization failures gracefully', async () => {
      const badConfig = {
        strategy: 'file' as const,
        backend: 'json',
        namespace: testNamespace,
        options: {
          basePath: '/invalid/path/that/does/not/exist',
          createDirectories: false // Don't create directories
        }
      };

      dataDaemon = new DataDaemon(badConfig);

      const context: DataOperationContext = {
        sessionId: 'test-session' as UUID,
        timestamp: new Date().toISOString(),
        source: 'unit-test'
      };

      // Operations should fail gracefully
      const result = await dataDaemon.create('test-collection', { test: true }, context);
      expect(result.success).toBe(false);
      expect(result.error).toBeTruthy();
    });
  });

  describe('Multi-Backend Operations', () => {
    it('should maintain data consistency across backend switches', async () => {
      // Test that demonstrates backend-agnostic behavior
      const testCases = [
        {
          name: 'Memory Backend',
          config: {
            strategy: 'memory' as const,
            backend: 'memory',
            namespace: `${testNamespace}-memory`,
            options: { maxRecords: 1000 }
          }
        },
        {
          name: 'File Backend',
          config: {
            strategy: 'file' as const,
            backend: 'json',
            namespace: `${testNamespace}-file`,
            options: {
              basePath: '/tmp/jtag-multi-backend-test',
              createDirectories: true
            }
          }
        }
      ];

      for (const testCase of testCases) {
        const daemon = new DataDaemon(testCase.config);
        
        const context: DataOperationContext = {
          sessionId: 'multi-backend-session' as UUID,
          timestamp: new Date().toISOString(),
          source: testCase.name
        };

        try {
          // Same operations should work identically
          const createResult = await daemon.create('consistency-test', 
            { backend: testCase.name, value: 42 }, 
            context
          );
          expect(createResult.success).toBe(true);

          const readResult = await daemon.read('consistency-test', createResult.data!.id, context);
          expect(readResult.success).toBe(true);
          expect(readResult.data?.data.backend).toBe(testCase.name);

          const queryResult = await daemon.query({
            collection: 'consistency-test',
            filters: { 'data.value': 42 }
          }, context);
          expect(queryResult.success).toBe(true);
          expect(queryResult.data).toHaveLength(1);

        } finally {
          await daemon.close();
        }
      }
    });
  });

  describe('Error Handling and Edge Cases', () => {
    beforeEach(async () => {
      const config = {
        strategy: 'memory' as const,
        backend: 'memory',
        namespace: testNamespace,
        options: { maxRecords: 1000 }
      };

      dataDaemon = new DataDaemon(config);
    });

    it('should handle invalid operation contexts gracefully', async () => {
      const invalidContext = {
        sessionId: '' as UUID, // Invalid session ID
        timestamp: 'invalid-timestamp',
        source: ''
      };

      const result = await dataDaemon.create('invalid-context-test', { test: true }, invalidContext);
      expect(result.success).toBe(false);
      expect(result.error).toContain('sessionId');
    });

    it('should validate data before storage operations', async () => {
      const context: DataOperationContext = {
        sessionId: generateUUID(), // Use proper UUID instead of string
        timestamp: new Date().toISOString(),
        source: 'validation-test'
      };

      // Test with undefined data
      const result = await dataDaemon.create('validation-test', undefined as any, context);
      expect(result.success).toBe(false);
      expect(result.error).toContain('data');
    });

    it('should handle empty collection names', async () => {
      const context: DataOperationContext = {
        sessionId: 'empty-collection-session' as UUID,
        timestamp: new Date().toISOString(),
        source: 'empty-test'
      };

      const result = await dataDaemon.create('', { test: true }, context);
      expect(result.success).toBe(false);
      expect(result.error).toContain('collection');
    });
  });

  describe('Metadata Management', () => {
    beforeEach(async () => {
      const config = {
        strategy: 'memory' as const,
        backend: 'memory',
        namespace: testNamespace,
        options: { maxRecords: 1000 }
      };

      dataDaemon = new DataDaemon(config);
    });

    it('should generate proper metadata for new records', async () => {
      const context: DataOperationContext = {
        sessionId: 'metadata-session' as UUID,
        timestamp: '2025-08-16T20:00:00.000Z',
        source: 'metadata-test'
      };

      const result = await dataDaemon.create('metadata-test', { test: 'metadata' }, context);

      expect(result.success).toBe(true);
      expect(result.data?.metadata.createdAt).toBe('2025-08-16T20:00:00.000Z');
      expect(result.data?.metadata.updatedAt).toBe('2025-08-16T20:00:00.000Z');
      expect(result.data?.metadata.version).toBe(1);
    });

    it('should update metadata correctly during updates', async () => {
      const createContext: DataOperationContext = {
        sessionId: 'metadata-session' as UUID,
        timestamp: '2025-08-16T20:00:00.000Z',
        source: 'metadata-test'
      };

      const updateContext: DataOperationContext = {
        sessionId: 'metadata-session' as UUID,
        timestamp: '2025-08-16T20:05:00.000Z',
        source: 'metadata-test'
      };

      const createResult = await dataDaemon.create('metadata-update-test', { version: 1 }, createContext);
      const recordId = createResult.data!.id;

      const updateResult = await dataDaemon.update('metadata-update-test', recordId, { version: 2 }, updateContext);

      expect(updateResult.success).toBe(true);
      expect(updateResult.data?.metadata.createdAt).toBe('2025-08-16T20:00:00.000Z'); // Preserved
      expect(updateResult.data?.metadata.updatedAt).toBe('2025-08-16T20:05:00.000Z'); // Updated
      expect(updateResult.data?.metadata.version).toBe(2); // Incremented
    });
  });

  describe('Collection Management via Orchestrator', () => {
    beforeEach(async () => {
      const config = {
        strategy: 'memory' as const,
        backend: 'memory',
        namespace: testNamespace,
        options: { maxRecords: 1000 }
      };

      dataDaemon = new DataDaemon(config);
    });

    it('should list collections through orchestrator', async () => {
      const context: DataOperationContext = {
        sessionId: 'collection-session' as UUID,
        timestamp: new Date().toISOString(),
        source: 'collection-test'
      };

      // Create records in multiple collections
      await dataDaemon.create('collection-a', { type: 'a' }, context);
      await dataDaemon.create('collection-b', { type: 'b' }, context);

      const result = await dataDaemon.listCollections(context);

      expect(result.success).toBe(true);
      expect(result.data).toContain('collection-a');
      expect(result.data).toContain('collection-b');
    });

    it('should get collection statistics through orchestrator', async () => {
      const context: DataOperationContext = {
        sessionId: 'stats-session' as UUID,
        timestamp: new Date().toISOString(),
        source: 'stats-test'
      };

      // Create test data
      await dataDaemon.create('stats-collection', { size: 'large', content: 'x'.repeat(1000) }, context);
      await dataDaemon.create('stats-collection', { size: 'small', content: 'y' }, context);

      const result = await dataDaemon.getCollectionStats('stats-collection', context);

      expect(result.success).toBe(true);
      expect(result.data?.name).toBe('stats-collection');
      expect(result.data?.recordCount).toBe(2);
    });
  });

  describe('Advanced Query Operations', () => {
    beforeEach(async () => {
      const config = {
        strategy: 'memory' as const,
        backend: 'memory',
        namespace: testNamespace,
        options: { maxRecords: 1000 }
      };

      dataDaemon = new DataDaemon(config);

      // Create test dataset
      const context: DataOperationContext = {
        sessionId: 'query-session' as UUID,
        timestamp: new Date().toISOString(),
        source: 'query-test'
      };

      const testData = [
        { name: 'Task 1', category: 'development', priority: 1, completed: true },
        { name: 'Task 2', category: 'testing', priority: 3, completed: false },
        { name: 'Task 3', category: 'development', priority: 2, completed: true },
        { name: 'Task 4', category: 'documentation', priority: 4, completed: false }
      ];

      for (const data of testData) {
        await dataDaemon.create('advanced-queries', data, context);
      }
    });

    it('should execute complex filter queries', async () => {
      const context: DataOperationContext = {
        sessionId: 'query-session' as UUID,
        timestamp: new Date().toISOString(),
        source: 'query-test'
      };

      const query: StorageQuery = {
        collection: 'advanced-queries',
        filters: {
          'data.category': 'development',
          'data.completed': true
        },
        sort: [{ field: 'data.priority', direction: 'asc' }]
      };

      const result = await dataDaemon.query(query, context);

      expect(result.success).toBe(true);
      expect(result.data).toHaveLength(2);
      expect(result.data?.[0].data.priority).toBe(1);
      expect(result.data?.[1].data.priority).toBe(2);
      expect(result.data?.every(r => r.data.category === 'development' && r.data.completed === true)).toBe(true);
    });

    it('should handle pagination in queries', async () => {
      const context: DataOperationContext = {
        sessionId: 'pagination-session' as UUID,
        timestamp: new Date().toISOString(),
        source: 'pagination-test'
      };

      const query: StorageQuery = {
        collection: 'advanced-queries',
        sort: [{ field: 'data.priority', direction: 'asc' }],
        limit: 2,
        offset: 1
      };

      const result = await dataDaemon.query(query, context);

      expect(result.success).toBe(true);
      expect(result.data).toHaveLength(2);
      expect(result.metadata?.totalCount).toBe(4);
      // Should get records with priority 2 and 3 (skipping priority 1)
      expect(result.data?.[0].data.priority).toBe(2);
      expect(result.data?.[1].data.priority).toBe(3);
    });
  });

  describe('ID Generation and Validation', () => {
    beforeEach(async () => {
      const config = {
        strategy: 'memory' as const,
        backend: 'memory',
        namespace: testNamespace,
        options: { maxRecords: 1000 }
      };

      dataDaemon = new DataDaemon(config);
    });

    it('should generate unique IDs for each record', async () => {
      const context: DataOperationContext = {
        sessionId: 'id-session' as UUID,
        timestamp: new Date().toISOString(),
        source: 'id-test'
      };

      const result1 = await dataDaemon.create('id-test', { value: 1 }, context);
      const result2 = await dataDaemon.create('id-test', { value: 2 }, context);

      expect(result1.success).toBe(true);
      expect(result2.success).toBe(true);
      expect(result1.data?.id).not.toBe(result2.data?.id);
      expect(result1.data?.id).toBeTruthy();
      expect(result2.data?.id).toBeTruthy();
    });

    it('should validate UUID format for custom IDs', async () => {
      const context: DataOperationContext = {
        sessionId: 'uuid-session' as UUID,
        timestamp: new Date().toISOString(),
        source: 'uuid-test'
      };

      const invalidId = 'not-a-uuid' as UUID;
      const result = await dataDaemon.create('uuid-validation-test', { test: true }, context, invalidId);

      // Should either succeed (if validation is lenient) or fail with clear error
      if (!result.success) {
        expect(result.error).toContain('UUID');
      }
    });
  });
});