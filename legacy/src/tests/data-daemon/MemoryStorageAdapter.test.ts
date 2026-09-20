/**
 * Memory Storage Adapter - Unit Tests
 * 
 * Comprehensive test suite for in-memory data persistence.
 * Tests performance, advanced queries, TTL, and persistence features.
 */

import { describe, it, expect, beforeEach, afterEach } from '@jest/globals';
import * as fs from 'fs/promises';
import { MemoryStorageAdapter } from '../../daemons/data-daemon/server/MemoryStorageAdapter';
import { DataRecord, StorageAdapterConfig, StorageQuery } from '../../daemons/data-daemon/shared/DataStorageAdapter';
import { UUID } from '../../system/core/types/CrossPlatformUUID';

describe('MemoryStorageAdapter', () => {
  let adapter: MemoryStorageAdapter;
  let testNamespace: string;

  beforeEach(async () => {
    adapter = new MemoryStorageAdapter();
    testNamespace = `memory-test-${Date.now()}`;
    
    const config: StorageAdapterConfig = {
      type: 'memory',
      namespace: testNamespace,
      options: {
        maxRecords: 1000,
        enablePersistence: false
      }
    };
    
    await adapter.initialize(config);
  });

  afterEach(async () => {
    await adapter.close();
  });

  describe('Basic CRUD Operations', () => {
    const testRecord: DataRecord<{ name: string; score: number }> = {
      id: 'memory-test-1' as UUID,
      collection: 'memory-collection',
      data: { name: 'Memory Test', score: 95 },
      metadata: {
        createdAt: '2025-08-16T18:00:00.000Z',
        updatedAt: '2025-08-16T18:00:00.000Z',
        version: 1
      }
    };

    it('should create record in memory', async () => {
      const result = await adapter.create(testRecord);
      
      expect(result.success).toBe(true);
      expect(result.data).toEqual(testRecord);
    });

    it('should prevent duplicate record creation', async () => {
      await adapter.create(testRecord);
      
      const result = await adapter.create(testRecord);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('already exists');
    });

    it('should read record from memory instantly', async () => {
      await adapter.create(testRecord);
      
      const result = await adapter.read<{ name: string; score: number }>('memory-collection', testRecord.id);
      
      expect(result.success).toBe(true);
      expect(result.data).toEqual(testRecord);
    });

    it('should update record preserving metadata', async () => {
      await adapter.create(testRecord);
      
      const updateData = { score: 100 };
      const result = await adapter.update('memory-collection', testRecord.id, updateData);
      
      expect(result.success).toBe(true);
      expect(result.data?.data.score).toBe(100);
      expect(result.data?.data.name).toBe('Memory Test');
      expect(result.data?.metadata.version).toBe(2);
      expect(result.data?.metadata.createdAt).toBe(testRecord.metadata.createdAt);
    });

    it('should delete record and clean up empty collections', async () => {
      await adapter.create(testRecord);
      
      const deleteResult = await adapter.delete('memory-collection', testRecord.id);
      expect(deleteResult.success).toBe(true);
      expect(deleteResult.data).toBe(true);
      
      // Verify collection was removed when empty
      const collections = await adapter.listCollections();
      expect(collections.data).not.toContain('memory-collection');
    });
  });

  describe('Advanced Query Features', () => {
    const queryTestRecords: DataRecord<{ category: string; priority: number; tags: string[] }>[] = [
      {
        id: 'query-1' as UUID,
        collection: 'advanced-queries',
        data: { category: 'urgent', priority: 1, tags: ['backend', 'critical'] },
        metadata: { 
          createdAt: '2025-08-16T10:00:00.000Z', 
          updatedAt: '2025-08-16T10:00:00.000Z', 
          version: 1,
          tags: ['production', 'monitoring']
        }
      },
      {
        id: 'query-2' as UUID,
        collection: 'advanced-queries',
        data: { category: 'normal', priority: 5, tags: ['frontend', 'ui'] },
        metadata: { 
          createdAt: '2025-08-16T11:00:00.000Z', 
          updatedAt: '2025-08-16T11:00:00.000Z', 
          version: 1,
          tags: ['development']
        }
      },
      {
        id: 'query-3' as UUID,
        collection: 'advanced-queries',
        data: { category: 'urgent', priority: 2, tags: ['backend', 'performance'] },
        metadata: { 
          createdAt: '2025-08-16T12:00:00.000Z', 
          updatedAt: '2025-08-16T12:00:00.000Z', 
          version: 1,
          tags: ['production']
        }
      }
    ];

    beforeEach(async () => {
      for (const record of queryTestRecords) {
        await adapter.create(record);
      }
    });

    it('should support MongoDB-style operators', async () => {
      const query: StorageQuery = {
        collection: 'advanced-queries',
        filters: { 
          'data.priority': { $lt: 3 },
          'data.category': 'urgent'
        }
      };
      const result = await adapter.query(query);
      
      expect(result.success).toBe(true);
      expect(result.data).toHaveLength(2);
      expect(result.data?.every(r => r.data.priority < 3 && r.data.category === 'urgent')).toBe(true);
    });

    it('should filter by time ranges', async () => {
      const query: StorageQuery = {
        collection: 'advanced-queries',
        timeRange: {
          start: '2025-08-16T10:30:00.000Z',
          end: '2025-08-16T11:30:00.000Z'
        }
      };
      const result = await adapter.query(query);
      
      expect(result.success).toBe(true);
      expect(result.data).toHaveLength(1);
      expect(result.data?.[0].id).toBe('query-2');
    });

    it('should filter by metadata tags', async () => {
      const query: StorageQuery = {
        collection: 'advanced-queries',
        tags: ['production']
      };
      const result = await adapter.query(query);
      
      expect(result.success).toBe(true);
      expect(result.data).toHaveLength(2);
      expect(result.data?.every(r => r.metadata.tags?.includes('production'))).toBe(true);
    });

    it('should support complex nested property queries', async () => {
      const query: StorageQuery = {
        collection: 'advanced-queries',
        filters: { 'data.tags.0': 'backend' }
      };
      const result = await adapter.query(query);
      
      expect(result.success).toBe(true);
      expect(result.data).toHaveLength(2);
      expect(result.data?.every(r => r.data.tags[0] === 'backend')).toBe(true);
    });
  });

  describe('Memory Limits and Performance', () => {
    it('should enforce memory limits', async () => {
      const limitedAdapter = new MemoryStorageAdapter();
      await limitedAdapter.initialize({
        type: 'memory',
        namespace: 'limited-test',
        options: { maxRecords: 2 }
      });

      // Create records up to limit
      await limitedAdapter.create({
        id: 'limit-1' as UUID,
        collection: 'limited',
        data: { test: 1 },
        metadata: { createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), version: 1 }
      });
      
      await limitedAdapter.create({
        id: 'limit-2' as UUID,
        collection: 'limited',
        data: { test: 2 },
        metadata: { createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), version: 1 }
      });

      // Third record should fail
      const result = await limitedAdapter.create({
        id: 'limit-3' as UUID,
        collection: 'limited',
        data: { test: 3 },
        metadata: { createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), version: 1 }
      });

      expect(result.success).toBe(false);
      expect(result.error).toContain('limit reached');
      
      await limitedAdapter.close();
    });

    it('should handle large datasets efficiently', async () => {
      const startTime = Date.now();
      
      // Create 100 records
      for (let i = 0; i < 100; i++) {
        await adapter.create({
          id: `perf-${i}` as UUID,
          collection: 'performance-test',
          data: { index: i, data: 'x'.repeat(100) },
          metadata: { createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), version: 1 }
        });
      }
      
      const createTime = Date.now() - startTime;
      
      // Query all records
      const queryStart = Date.now();
      const result = await adapter.query({ collection: 'performance-test' });
      const queryTime = Date.now() - queryStart;
      
      expect(result.success).toBe(true);
      expect(result.data).toHaveLength(100);
      expect(createTime).toBeLessThan(1000); // Should be very fast
      expect(queryTime).toBeLessThan(100);   // Should be very fast
    });
  });

  describe('TTL and Cleanup', () => {
    it('should cleanup expired records based on TTL', async () => {
      const expiredRecord: DataRecord = {
        id: 'expired-test' as UUID,
        collection: 'ttl-test',
        data: { test: 'expired' },
        metadata: { 
          createdAt: new Date(Date.now() - 10000).toISOString(), // 10 seconds ago
          updatedAt: new Date(Date.now() - 10000).toISOString(),
          version: 1,
          ttl: 5 // 5 second TTL - should be expired
        }
      };
      
      const validRecord: DataRecord = {
        id: 'valid-test' as UUID,
        collection: 'ttl-test',
        data: { test: 'valid' },
        metadata: { 
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
          version: 1,
          ttl: 3600 // 1 hour TTL - should be valid
        }
      };

      await adapter.create(expiredRecord);
      await adapter.create(validRecord);
      
      // Run cleanup
      await adapter.cleanup();
      
      // Check results
      const expiredResult = await adapter.read('ttl-test', expiredRecord.id);
      const validResult = await adapter.read('ttl-test', validRecord.id);
      
      expect(expiredResult.data).toBeUndefined(); // Should be deleted
      expect(validResult.data).toBeTruthy();      // Should remain
    });
  });
});