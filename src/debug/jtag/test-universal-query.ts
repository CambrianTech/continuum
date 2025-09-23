#!/usr/bin/env tsx

/**
 * Universal Query System Test
 *
 * Tests both SQLite and JSON adapters with universal query operators
 * and shows generated SQL for verification
 */

import { DataDaemon, type StorageStrategyConfig } from './daemons/data-daemon/shared/DataDaemon';
import { DefaultStorageAdapterFactory } from './daemons/data-daemon/server/DefaultStorageAdapterFactory';
import type { StorageQuery, DataRecord } from './daemons/data-daemon/shared/DataStorageAdapter';
import type { UUID } from './system/core/types/CrossPlatformUUID';
import { generateUUID } from './system/core/types/CrossPlatformUUID';

// Test data structure
interface TestUser {
  name: string;
  age: number;
  email: string;
  active: boolean;
  lastLogin?: Date;
}

async function testSQLiteAdapter(): Promise<void> {
  console.log('🔍 Testing SQLite Adapter with Universal Query Operators...\n');

  // SQLite configuration
  const sqliteConfig: StorageStrategyConfig = {
    strategy: 'sql',
    backend: 'sqlite',
    namespace: 'test-' + generateUUID(),
    options: {
      basePath: '.continuum/test',
      databaseName: 'query-test.sqlite',
      foreignKeys: false
    },
    features: {
      enableTransactions: true,
      enableIndexing: true,
      enableReplication: false,
      enableSharding: false,
      enableCaching: false
    }
  };

  const factory = new DefaultStorageAdapterFactory();
  const adapter = factory.createAdapter({
    type: sqliteConfig.backend as any,
    namespace: sqliteConfig.namespace,
    options: sqliteConfig.options
  });

  const dataDaemon = new DataDaemon(sqliteConfig, adapter);
  await dataDaemon.initialize();

  // Create test data
  const testUsers: DataRecord<TestUser>[] = [
    {
      id: generateUUID() as UUID,
      collection: 'User',
      data: { name: 'Alice', age: 25, email: 'alice@example.com', active: true },
      metadata: { createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), version: 1 }
    },
    {
      id: generateUUID() as UUID,
      collection: 'User',
      data: { name: 'Bob', age: 30, email: 'bob@example.com', active: false },
      metadata: { createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), version: 1 }
    },
    {
      id: generateUUID() as UUID,
      collection: 'User',
      data: { name: 'Charlie', age: 35, email: 'charlie@example.com', active: true },
      metadata: { createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), version: 1 }
    }
  ];

  // Insert test data
  for (const user of testUsers) {
    await adapter.create(user);
  }

  // Test queries with different operators
  const testQueries: Array<{ name: string; query: StorageQuery }> = [
    {
      name: 'Equal filter ($eq)',
      query: {
        collection: 'User',
        filter: { name: { $eq: 'Alice' } }
      }
    },
    {
      name: 'Greater than filter ($gt)',
      query: {
        collection: 'User',
        filter: { age: { $gt: 28 } }
      }
    },
    {
      name: 'In array filter ($in)',
      query: {
        collection: 'User',
        filter: { name: { $in: ['Alice', 'Bob'] } }
      }
    },
    {
      name: 'Active users with sorting',
      query: {
        collection: 'User',
        filter: { active: { $eq: true } },
        sort: [{ field: 'age', direction: 'asc' }]
      }
    },
    {
      name: 'Email contains filter',
      query: {
        collection: 'User',
        filter: { email: { $contains: 'example.com' } }
      }
    }
  ];

  // Run test queries
  for (const test of testQueries) {
    console.log(`📊 ${test.name}:`);
    console.log(`   Query: ${JSON.stringify(test.query, null, 2)}`);

    const result = await adapter.query<TestUser>(test.query);

    if (result.success) {
      console.log(`   ✅ Results (${result.data?.length || 0} records):`);
      result.data?.forEach((record, i) => {
        console.log(`      ${i + 1}. ${record.data.name} (age: ${record.data.age}, active: ${record.data.active})`);
      });
    } else {
      console.log(`   ❌ Error: ${result.error}`);
    }
    console.log('');
  }

  await dataDaemon.shutdown();
}

async function testJSONAdapter(): Promise<void> {
  console.log('📁 Testing JSON File Adapter with Universal Query Operators...\n');

  // JSON configuration
  const jsonConfig: StorageStrategyConfig = {
    strategy: 'file',
    backend: 'json',
    namespace: 'test-' + generateUUID(),
    options: {
      dataDirectory: '.continuum/test/json-data',
      prettyPrint: true
    },
    features: {
      enableTransactions: false,
      enableIndexing: false,
      enableReplication: false,
      enableSharding: false,
      enableCaching: true
    }
  };

  const factory = new DefaultStorageAdapterFactory();
  const adapter = factory.createAdapter({
    type: 'file' as any,
    namespace: jsonConfig.namespace,
    options: jsonConfig.options
  });

  const dataDaemon = new DataDaemon(jsonConfig, adapter);
  await dataDaemon.initialize();

  // Create same test data
  const testUsers: DataRecord<TestUser>[] = [
    {
      id: generateUUID() as UUID,
      collection: 'User',
      data: { name: 'Alice', age: 25, email: 'alice@example.com', active: true },
      metadata: { createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), version: 1 }
    },
    {
      id: generateUUID() as UUID,
      collection: 'User',
      data: { name: 'Bob', age: 30, email: 'bob@example.com', active: false },
      metadata: { createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), version: 1 }
    },
    {
      id: generateUUID() as UUID,
      collection: 'User',
      data: { name: 'Charlie', age: 35, email: 'charlie@example.com', active: true },
      metadata: { createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), version: 1 }
    }
  ];

  // Insert test data
  for (const user of testUsers) {
    await adapter.create(user);
  }

  // Test same queries
  const testQueries: Array<{ name: string; query: StorageQuery }> = [
    {
      name: 'Equal filter ($eq)',
      query: {
        collection: 'User',
        filter: { name: { $eq: 'Alice' } }
      }
    },
    {
      name: 'Greater than filter ($gt)',
      query: {
        collection: 'User',
        filter: { age: { $gt: 28 } }
      }
    },
    {
      name: 'In array filter ($in)',
      query: {
        collection: 'User',
        filter: { name: { $in: ['Alice', 'Bob'] } }
      }
    }
  ];

  // Run test queries
  for (const test of testQueries) {
    console.log(`📊 ${test.name}:`);
    console.log(`   Query: ${JSON.stringify(test.query, null, 2)}`);

    const result = await adapter.query<TestUser>(test.query);

    if (result.success) {
      console.log(`   ✅ Results (${result.data?.length || 0} records):`);
      result.data?.forEach((record, i) => {
        console.log(`      ${i + 1}. ${record.data.name} (age: ${record.data.age}, active: ${record.data.active})`);
      });
    } else {
      console.log(`   ❌ Error: ${result.error}`);
    }
    console.log('');
  }

  await dataDaemon.shutdown();
}

async function main(): Promise<void> {
  console.log('🧪 Universal Query System Test Suite');
  console.log('=====================================\n');

  try {
    await testSQLiteAdapter();
    console.log('─'.repeat(50) + '\n');
    await testJSONAdapter();
    console.log('🎉 All tests completed successfully!');
  } catch (error) {
    console.error('❌ Test failed:', error);
    process.exit(1);
  }
}

if (require.main === module) {
  main().catch(console.error);
}