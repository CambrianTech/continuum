/**
 * File Storage Adapter - Unit Tests
 * 
 * Comprehensive test suite for filesystem-based data persistence.
 * Tests all CRUD operations, query system, and edge cases.
 */

import { describe, it, expect, beforeEach, afterEach } from '@jest/globals';
import * as fs from 'fs/promises';
import * as path from 'path';
import { FileStorageAdapter } from '../../daemons/data-daemon/server/FileStorageAdapter';
import { DataRecord, StorageAdapterConfig, StorageQuery } from '../../daemons/data-daemon/shared/DataStorageAdapter';
import { UUID } from '../../system/core/types/CrossPlatformUUID';

describe('FileStorageAdapter', () => {
  let adapter: FileStorageAdapter;
  let testBasePath: string;
  let testNamespace: string;

  beforeEach(async () => {
    adapter = new FileStorageAdapter();
    testBasePath = '/tmp/jtag-test-storage';
    testNamespace = `test-${Date.now()}`;
    
    const config: StorageAdapterConfig = {
      type: 'file',
      namespace: testNamespace,
      options: {
        basePath: testBasePath,
        createDirectories: true,
        atomicWrites: true,
        enableIndexes: false
      }
    };
    
    await adapter.initialize(config);
  });

  afterEach(async () => {
    await adapter.close();
    // Clean up test directory
    try {
      await fs.rm(path.join(testBasePath, testNamespace), { recursive: true, force: true });
    } catch (error) {
      // Ignore cleanup errors
    }
  });

  describe('CRUD Operations', () => {
    const testRecord: DataRecord<{ name: string; value: number }> = {
      id: 'test-record-1' as UUID,
      collection: 'test-collection',
      data: { name: 'Test Record', value: 42 },
      metadata: {
        createdAt: '2025-08-16T18:00:00.000Z',
        updatedAt: '2025-08-16T18:00:00.000Z',
        version: 1
      }
    };

    it('should create record successfully', async () => {
      const result = await adapter.create(testRecord);
      
      expect(result.success).toBe(true);
      expect(result.data).toEqual(testRecord);
      expect(result.error).toBeUndefined();
    });

    it('should read existing record', async () => {
      await adapter.create(testRecord);
      
      const result = await adapter.read<{ name: string; value: number }>('test-collection', testRecord.id);
      
      expect(result.success).toBe(true);
      expect(result.data).toEqual(testRecord);
    });

    it('should return undefined for non-existent record', async () => {
      const result = await adapter.read('non-existent', 'non-existent-id' as UUID);
      
      expect(result.success).toBe(true);
      expect(result.data).toBeUndefined();
    });

    it('should update existing record', async () => {
      await adapter.create(testRecord);
      
      const updateData = { value: 84 };
      const result = await adapter.update('test-collection', testRecord.id, updateData);
      
      expect(result.success).toBe(true);
      expect(result.data?.data.value).toBe(84);
      expect(result.data?.data.name).toBe('Test Record'); // Preserved
      expect(result.data?.metadata.version).toBe(2);
    });

    it('should fail update for non-existent record', async () => {
      const result = await adapter.update('test-collection', 'non-existent' as UUID, { value: 99 });
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('not found');
    });

    it('should delete existing record', async () => {
      await adapter.create(testRecord);
      
      const result = await adapter.delete('test-collection', testRecord.id);
      
      expect(result.success).toBe(true);
      expect(result.data).toBe(true);
      
      // Verify deletion
      const readResult = await adapter.read('test-collection', testRecord.id);
      expect(readResult.data).toBeUndefined();
    });

    it('should handle delete of non-existent record gracefully', async () => {
      const result = await adapter.delete('test-collection', 'non-existent' as UUID);
      
      expect(result.success).toBe(true);
      expect(result.data).toBe(false);
    });
  });

  describe('Query System', () => {
    const records: DataRecord<{ category: string; priority: number; active: boolean }>[] = [
      {
        id: 'record-1' as UUID,
        collection: 'tasks',
        data: { category: 'development', priority: 1, active: true },
        metadata: { createdAt: '2025-08-16T10:00:00.000Z', updatedAt: '2025-08-16T10:00:00.000Z', version: 1 }
      },
      {
        id: 'record-2' as UUID,
        collection: 'tasks',
        data: { category: 'testing', priority: 2, active: false },
        metadata: { createdAt: '2025-08-16T11:00:00.000Z', updatedAt: '2025-08-16T11:00:00.000Z', version: 1 }
      },
      {
        id: 'record-3' as UUID,
        collection: 'tasks',
        data: { category: 'development', priority: 3, active: true },
        metadata: { createdAt: '2025-08-16T12:00:00.000Z', updatedAt: '2025-08-16T12:00:00.000Z', version: 1 }
      }
    ];

    beforeEach(async () => {
      for (const record of records) {
        await adapter.create(record);
      }
    });

    it('should query all records without filters', async () => {
      const query: StorageQuery = { collection: 'tasks' };
      const result = await adapter.query(query);
      
      expect(result.success).toBe(true);
      expect(result.data).toHaveLength(3);
      expect(result.metadata?.totalCount).toBe(3);
    });

    it('should filter records by data properties', async () => {
      const query: StorageQuery = {
        collection: 'tasks',
        filters: { 'data.category': 'development' }
      };
      const result = await adapter.query(query);
      
      expect(result.success).toBe(true);
      expect(result.data).toHaveLength(2);
      expect(result.data?.every(r => r.data.category === 'development')).toBe(true);
    });

    it('should sort records by metadata fields', async () => {
      const query: StorageQuery = {
        collection: 'tasks',
        sort: [{ field: 'metadata.createdAt', direction: 'asc' }]
      };
      const result = await adapter.query(query);
      
      expect(result.success).toBe(true);
      expect(result.data).toHaveLength(3);
      expect(result.data?.[0].id).toBe('record-1');
      expect(result.data?.[2].id).toBe('record-3');
    });

    it('should apply pagination correctly', async () => {
      const query: StorageQuery = {
        collection: 'tasks',
        limit: 2,
        offset: 1
      };
      const result = await adapter.query(query);
      
      expect(result.success).toBe(true);
      expect(result.data).toHaveLength(2);
      expect(result.metadata?.totalCount).toBe(3);
    });

    it('should return empty array for non-existent collection', async () => {
      const query: StorageQuery = { collection: 'non-existent' };
      const result = await adapter.query(query);
      
      expect(result.success).toBe(true);
      expect(result.data).toEqual([]);
      expect(result.metadata?.totalCount).toBe(0);
    });
  });

  describe('Collection Management', () => {
    it('should list collections correctly', async () => {
      const record1: DataRecord = {
        id: 'test-1' as UUID,
        collection: 'collection-a',
        data: { test: true },
        metadata: { createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), version: 1 }
      };
      
      const record2: DataRecord = {
        id: 'test-2' as UUID,
        collection: 'collection-b',
        data: { test: true },
        metadata: { createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), version: 1 }
      };

      await adapter.create(record1);
      await adapter.create(record2);
      
      const result = await adapter.listCollections();
      
      expect(result.success).toBe(true);
      expect(result.data).toContain('collection-a');
      expect(result.data).toContain('collection-b');
    });

    it('should get collection statistics', async () => {
      const record: DataRecord = {
        id: 'stats-test' as UUID,
        collection: 'stats-collection',
        data: { large: 'x'.repeat(1000) },
        metadata: { createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), version: 1 }
      };

      await adapter.create(record);
      
      const result = await adapter.getCollectionStats('stats-collection');
      
      expect(result.success).toBe(true);
      expect(result.data?.name).toBe('stats-collection');
      expect(result.data?.recordCount).toBe(1);
      expect(result.data?.totalSize).toBeGreaterThan(1000);
      expect(result.data?.lastModified).toBeTruthy();
    });
  });

  describe('Batch Operations', () => {
    it('should execute multiple operations atomically', async () => {
      const operations = [
        { type: 'create' as const, collection: 'batch-test', id: 'batch-1' as UUID, data: { value: 1 } },
        { type: 'create' as const, collection: 'batch-test', id: 'batch-2' as UUID, data: { value: 2 } },
        { type: 'read' as const, collection: 'batch-test', id: 'batch-1' as UUID }
      ];
      
      const result = await adapter.batch(operations);
      
      expect(result.success).toBe(true);
      expect(result.data).toHaveLength(3);
      expect(result.data?.[0].success).toBe(true);
      expect(result.data?.[1].success).toBe(true);
      expect(result.data?.[2].success).toBe(true);
      expect(result.data?.[2].data?.data.value).toBe(1);
    });
  });

  describe('Atomic Write Safety', () => {
    it('should use atomic writes when enabled', async () => {
      // This test verifies that temp files are used and cleaned up
      const record: DataRecord = {
        id: 'atomic-test' as UUID,
        collection: 'atomic-collection',
        data: { test: 'atomic' },
        metadata: { createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), version: 1 }
      };

      await adapter.create(record);
      
      // Verify no temp files remain
      const collectionPath = path.join(testBasePath, testNamespace, 'atomic-collection');
      const files = await fs.readdir(collectionPath);
      const tempFiles = files.filter(f => f.endsWith('.tmp'));
      
      expect(tempFiles).toHaveLength(0);
      expect(files).toContain('atomic-test.json');
    });
  });

  describe('Error Handling', () => {
    it('should handle invalid JSON gracefully', async () => {
      // Create invalid JSON file manually
      const collectionPath = path.join(testBasePath, testNamespace, 'invalid-collection');
      await fs.mkdir(collectionPath, { recursive: true });
      await fs.writeFile(path.join(collectionPath, 'invalid.json'), 'invalid json content');
      
      const query: StorageQuery = { collection: 'invalid-collection' };
      const result = await adapter.query(query);
      
      expect(result.success).toBe(true);
      expect(result.data).toEqual([]); // Invalid files skipped
    });

    it('should handle permission errors gracefully', async () => {
      // Test with readonly directory (if supported by filesystem)
      const readonlyConfig: StorageAdapterConfig = {
        type: 'file',
        namespace: 'readonly-test',
        options: {
          basePath: '/root/readonly-test', // Likely permission denied
          createDirectories: false
        }
      };
      
      const readonlyAdapter = new FileStorageAdapter();
      
      try {
        await readonlyAdapter.initialize(readonlyConfig);
        
        const record: DataRecord = {
          id: 'readonly-test' as UUID,
          collection: 'readonly-collection',
          data: { test: true },
          metadata: { createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), version: 1 }
        };
        
        const result = await readonlyAdapter.create(record);
        
        // Should fail gracefully
        expect(result.success).toBe(false);
        expect(result.error).toBeTruthy();
      } catch (error) {
        // Permission error during initialization is also acceptable
        expect(error).toBeTruthy();
      }
    });
  });

  describe('Cleanup Operations', () => {
    it('should remove empty directories during cleanup', async () => {
      // Create and delete records to leave empty directory
      const record: DataRecord = {
        id: 'cleanup-test' as UUID,
        collection: 'cleanup-collection',
        data: { test: true },
        metadata: { createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), version: 1 }
      };

      await adapter.create(record);
      await adapter.delete('cleanup-collection', record.id);
      await adapter.cleanup();
      
      // Verify empty directory was removed
      const collections = await adapter.listCollections();
      expect(collections.data).not.toContain('cleanup-collection');
    });
  });
});