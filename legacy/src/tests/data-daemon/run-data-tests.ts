#!/usr/bin/env tsx
/**
 * Data Daemon Test Runner
 * 
 * Simple test runner for data daemon unit tests following JTAG patterns.
 * Tests storage adapters, orchestrator, and plugin system.
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { DataDaemon, DataOperationContext } from '../../daemons/data-daemon/shared/DataDaemon';
import { StorageAdapterFactory } from '../../daemons/data-daemon/shared/StorageAdapterFactory';
import { FileStorageAdapter } from '../../daemons/data-daemon/server/FileStorageAdapter';
import { MemoryStorageAdapter } from '../../daemons/data-daemon/server/MemoryStorageAdapter';
import { 
  DataRecord, 
  StorageAdapterConfig, 
  StorageQuery
} from '../../daemons/data-daemon/shared/DataStorageAdapter';
import { UUID } from '../../system/core/types/CrossPlatformUUID';

console.log('🗄️ Data Daemon Test Suite');

let testsPassed = 0;
let testsFailed = 0;

function test(description: string, testFn: () => Promise<void> | void): void {
  const run = async () => {
    try {
      await testFn();
      console.log(`✅ ${description}`);
      testsPassed++;
    } catch (error) {
      console.log(`❌ ${description}: ${error instanceof Error ? error.message : String(error)}`);
      testsFailed++;
    }
  };
  
  // Run test synchronously in the context it's called
  run().catch(error => {
    console.error(`Test runner error for "${description}":`, error);
    testsFailed++;
  });
}

function expect(actual: any) {
  return {
    toBe: (expected: any) => {
      if (actual !== expected) {
        throw new Error(`Expected ${expected}, got ${actual}`);
      }
    },
    toEqual: (expected: any) => {
      if (JSON.stringify(actual) !== JSON.stringify(expected)) {
        throw new Error(`Expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
      }
    },
    toBeTruthy: () => {
      if (!actual) {
        throw new Error(`Expected truthy value, got ${actual}`);
      }
    },
    toBeUndefined: () => {
      if (actual !== undefined) {
        throw new Error(`Expected undefined, got ${actual}`);
      }
    },
    toContain: (expected: any) => {
      if (!Array.isArray(actual) || !actual.includes(expected)) {
        throw new Error(`Expected array to contain ${expected}, got ${JSON.stringify(actual)}`);
      }
    },
    toHaveLength: (expected: number) => {
      if (!actual || actual.length !== expected) {
        throw new Error(`Expected length ${expected}, got ${actual?.length || 'undefined'}`);
      }
    },
    toBeGreaterThan: (expected: number) => {
      if (actual <= expected) {
        throw new Error(`Expected ${actual} to be greater than ${expected}`);
      }
    },
    not: {
      toBe: (expected: any) => {
        if (actual === expected) {
          throw new Error(`Expected not to equal ${expected}`);
        }
      },
      toContain: (expected: any) => {
        if (Array.isArray(actual) && actual.includes(expected)) {
          throw new Error(`Expected array not to contain ${expected}`);
        }
      }
    }
  };
}

async function runTests() {
  console.log('\n🧪 Testing StorageAdapterFactory...');
  
  // Test StorageAdapterFactory
  test('should create FileStorageAdapter for file strategy', () => {
    const factory = new StorageAdapterFactory();
    const config: StorageAdapterConfig = {
      type: 'file',
      namespace: 'test-file',
      options: {
        basePath: '/tmp/factory-test',
        createDirectories: true
      }
    };

    const adapter = factory.createAdapter(config);
    expect(adapter).toBeTruthy();
  });

  test('should create MemoryStorageAdapter for memory strategy', () => {
    const factory = new StorageAdapterFactory();
    const config: StorageAdapterConfig = {
      type: 'memory',
      namespace: 'test-memory',
      options: {
        maxRecords: 1000,
        enablePersistence: false
      }
    };

    const adapter = factory.createAdapter(config);
    expect(adapter).toBeTruthy();
  });

  console.log('\n🗄️ Testing DataDaemon Orchestrator...');

  // Test DataDaemon with Memory Backend
  test('should work with MemoryStorageAdapter backend', async () => {
    const config = {
      strategy: 'memory' as const,
      backend: 'memory',
      namespace: `test-${Date.now()}`,
      options: {
        maxRecords: 1000,
        enablePersistence: false
      }
    };

    const dataDaemon = new DataDaemon(config);
    
    try {
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
    } finally {
      await dataDaemon.close();
    }
  });

  // Test NEW STATIC INTERFACE - DataDaemon domain-owned methods
  test('should work with clean static interface (DataDaemon.store)', async () => {
    const config = {
      strategy: 'memory' as const,
      backend: 'memory',
      namespace: 'test-static-interface',
      options: {
        maxRecords: 1000,
        enablePersistence: false
      }
    };

    const dataDaemon = new DataDaemon(config);

    try {
      // Initialize static interface (like system would do)
      const context: DataOperationContext = {
        sessionId: 'test-session-static' as UUID,
        timestamp: new Date().toISOString(),
        source: 'unit-test-static'
      };

      DataDaemon.initialize(dataDaemon, context);

      // Test NEW PATTERN - clean, domain-owned interface
      const testData = { name: 'Static Interface Test', value: 999 };
      const storeResult = await DataDaemon.store<typeof testData>('static-test', testData);

      expect(storeResult.success).toBe(true);
      expect(storeResult.data?.data).toEqual(testData);
      expect(storeResult.data?.collection).toBe('static-test');

      // Test query method
      const queryResult = await DataDaemon.query<typeof testData>({
        collection: 'static-test',
        filters: { name: 'Static Interface Test' }
      });

      expect(queryResult.success).toBe(true);
      expect(queryResult.data?.length).toBe(1);
      expect(queryResult.data?.[0]?.data).toEqual(testData);

      console.log('✅ DataDaemon static interface test passed - clean, typed, auto-context!');
    } finally {
      await dataDaemon.close();
    }
  });

  // Test DataDaemon with File Backend
  test('should work with FileStorageAdapter backend', async () => {
    const config = {
      strategy: 'file' as const,
      backend: 'file',
      namespace: `test-${Date.now()}`,
      options: {
        basePath: '/tmp/jtag-daemon-test',
        createDirectories: true,
        atomicWrites: true
      }
    };

    const dataDaemon = new DataDaemon(config);
    
    try {
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
    } finally {
      await dataDaemon.close();
    }
  });

  console.log('\n💾 Testing CRUD Operations...');

  // Test CRUD operations
  test('should create, read, update, delete records', async () => {
    const config = {
      strategy: 'memory' as const,
      backend: 'memory',
      namespace: `crud-test-${Date.now()}`,
      options: { maxRecords: 1000 }
    };

    const dataDaemon = new DataDaemon(config);
    
    try {
      const context: DataOperationContext = {
        sessionId: 'crud-session' as UUID,
        timestamp: '2025-08-16T18:30:00.000Z',
        source: 'crud-test'
      };

      // Create
      const data = { name: 'CRUD Test', value: 42 };
      const createResult = await dataDaemon.create('crud-test', data, context);
      expect(createResult.success).toBe(true);
      
      const recordId = createResult.data!.id;

      // Read
      const readResult = await dataDaemon.read('crud-test', recordId, context);
      expect(readResult.success).toBe(true);
      expect(readResult.data?.data).toEqual(data);

      // Update
      const updateData = { value: 84 };
      const updateContext = { ...context, timestamp: '2025-08-16T18:35:00.000Z' };
      const updateResult = await dataDaemon.update('crud-test', recordId, updateData, updateContext);
      expect(updateResult.success).toBe(true);
      expect(updateResult.data?.data.value).toBe(84);
      expect(updateResult.data?.metadata.version).toBe(2);

      // Delete
      const deleteResult = await dataDaemon.delete('crud-test', recordId, context);
      expect(deleteResult.success).toBe(true);

      // Verify deletion
      const verifyResult = await dataDaemon.read('crud-test', recordId, context);
      expect(verifyResult.data).toBeUndefined();
      
    } finally {
      await dataDaemon.close();
    }
  });

  console.log('\n🔍 Testing Query System...');

  // Test Query operations
  test('should execute complex queries', async () => {
    const config = {
      strategy: 'memory' as const,
      backend: 'memory',
      namespace: `query-test-${Date.now()}`,
      options: { maxRecords: 1000 }
    };

    const dataDaemon = new DataDaemon(config);
    
    try {
      const context: DataOperationContext = {
        sessionId: 'query-session' as UUID,
        timestamp: new Date().toISOString(),
        source: 'query-test'
      };

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
      
    } finally {
      await dataDaemon.close();
    }
  });

  // Wait for all async tests to complete
  await new Promise(resolve => setTimeout(resolve, 100));

  console.log(`\n📊 Test Results: ${testsPassed} passed, ${testsFailed} failed`);
  
  if (testsFailed > 0) {
    console.log('❌ Some tests failed');
    process.exit(1);
  } else {
    console.log('✅ All data daemon tests passed!');
    process.exit(0);
  }
}

runTests().catch(error => {
  console.error('Test runner failed:', error);
  process.exit(1);
});