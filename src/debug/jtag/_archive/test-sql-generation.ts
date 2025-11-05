#!/usr/bin/env tsx

/**
 * SQL Generation Test - Universal Query to SQL Translation
 *
 * Tests the SQLite adapter's ability to generate SQL from universal query operators
 * and displays the actual SQL queries being built
 */

import { SqliteStorageAdapter } from './daemons/data-daemon/server/SqliteStorageAdapter';
import type { StorageQuery, DataRecord } from './daemons/data-daemon/shared/DataStorageAdapter';
import type { UUID } from './system/core/types/CrossPlatformUUID';
import { generateUUID } from './system/core/types/CrossPlatformUUID';

// Test data structure
interface TestUser {
  name: string;
  age: number;
  email: string;
  active: boolean;
  lastLogin?: string;
  tags?: string[];
}

async function testSQLGeneration(): Promise<void> {
  console.log('🔍 SQL Generation Test - Universal Query Operators\n');

  // Initialize SQLite adapter
  const adapter = new SqliteStorageAdapter();
  await adapter.initialize({
    type: 'sqlite',
    namespace: 'sql-test-' + generateUUID(),
    options: {
      basePath: '.continuum/test',
      databaseName: 'sql-generation-test.sqlite',
      foreignKeys: false
    }
  });

  // Create test data first
  const testUsers: DataRecord<TestUser>[] = [
    {
      id: generateUUID() as UUID,
      collection: 'User',
      data: {
        name: 'Alice',
        age: 25,
        email: 'alice@example.com',
        active: true,
        lastLogin: '2024-01-15',
        tags: ['admin', 'developer']
      },
      metadata: { createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), version: 1 }
    },
    {
      id: generateUUID() as UUID,
      collection: 'User',
      data: {
        name: 'Bob',
        age: 30,
        email: 'bob@example.com',
        active: false,
        lastLogin: '2024-02-20',
        tags: ['user']
      },
      metadata: { createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), version: 1 }
    },
    {
      id: generateUUID() as UUID,
      collection: 'User',
      data: {
        name: 'Charlie',
        age: 35,
        email: 'charlie@example.com',
        active: true,
        tags: ['manager', 'developer']
      },
      metadata: { createdAt: new Date().toISOString(), updatedAt: new Date().toISOString(), version: 1 }
    }
  ];

  console.log('📝 Inserting test data...');
  for (const user of testUsers) {
    await adapter.create(user);
  }
  console.log(`✅ Inserted ${testUsers.length} test records\n`);

  // Test queries that should generate different SQL patterns
  const testQueries: Array<{ name: string; query: StorageQuery; expectedSQL?: string }> = [
    {
      name: 'Simple Equality ($eq)',
      query: {
        collection: 'User',
        filter: { name: { $eq: 'Alice' } }
      },
      expectedSQL: "WHERE JSON_EXTRACT(data, '$.name') = ?"
    },
    {
      name: 'Greater Than ($gt)',
      query: {
        collection: 'User',
        filter: { age: { $gt: 28 } }
      },
      expectedSQL: "WHERE JSON_EXTRACT(data, '$.age') > ?"
    },
    {
      name: 'Less Than or Equal ($lte)',
      query: {
        collection: 'User',
        filter: { age: { $lte: 30 } }
      },
      expectedSQL: "WHERE JSON_EXTRACT(data, '$.age') <= ?"
    },
    {
      name: 'In Array ($in)',
      query: {
        collection: 'User',
        filter: { name: { $in: ['Alice', 'Bob', 'David'] } }
      },
      expectedSQL: "WHERE JSON_EXTRACT(data, '$.name') IN (?, ?, ?)"
    },
    {
      name: 'Not Equal ($ne)',
      query: {
        collection: 'User',
        filter: { active: { $ne: false } }
      },
      expectedSQL: "WHERE JSON_EXTRACT(data, '$.active') != ?"
    },
    {
      name: 'Contains String ($contains)',
      query: {
        collection: 'User',
        filter: { email: { $contains: 'example.com' } }
      },
      expectedSQL: "WHERE JSON_EXTRACT(data, '$.email') LIKE ?"
    },
    {
      name: 'Field Exists ($exists)',
      query: {
        collection: 'User',
        filter: { lastLogin: { $exists: true } }
      },
      expectedSQL: "WHERE JSON_EXTRACT(data, '$.lastLogin') IS NOT NULL"
    },
    {
      name: 'Multiple Conditions (AND)',
      query: {
        collection: 'User',
        filter: {
          active: { $eq: true },
          age: { $gt: 20 }
        }
      },
      expectedSQL: "WHERE JSON_EXTRACT(data, '$.active') = ? AND JSON_EXTRACT(data, '$.age') > ?"
    },
    {
      name: 'Complex Query with Sorting',
      query: {
        collection: 'User',
        filter: {
          age: { $gte: 25 },
          email: { $contains: '@example' }
        },
        sort: [{ field: 'name', direction: 'asc' }],
        limit: 10
      },
      expectedSQL: "WHERE ... ORDER BY JSON_EXTRACT(data, '$.name') ASC LIMIT ?"
    }
  ];

  console.log('🧪 Testing SQL Generation for Universal Query Operators:\n');

  for (const test of testQueries) {
    console.log(`📊 ${test.name}:`);
    console.log(`   Query: ${JSON.stringify(test.query, null, 2)}`);

    if (test.expectedSQL) {
      console.log(`   Expected SQL Pattern: ${test.expectedSQL}`);
    }

    try {
      const result = await adapter.query<TestUser>(test.query);

      if (result.success) {
        console.log(`   ✅ Query executed successfully`);
        console.log(`   📈 Results: ${result.data?.length || 0} records found`);

        // Show sample results
        if (result.data && result.data.length > 0) {
          console.log(`   📋 Sample data:`);
          result.data.slice(0, 2).forEach((record, i) => {
            const data = record.data;
            console.log(`      ${i + 1}. ${data.name} (age: ${data.age}, active: ${data.active})`);
          });
        }
      } else {
        console.log(`   ❌ Query failed: ${result.error}`);
      }
    } catch (error) {
      console.log(`   💥 Exception: ${error instanceof Error ? error.message : 'Unknown error'}`);
    }

    console.log('');
  }

  await adapter.close();
}

async function main(): Promise<void> {
  console.log('🧪 SQL Generation Test Suite');
  console.log('============================\n');

  try {
    await testSQLGeneration();
    console.log('🎉 SQL generation tests completed!');
    console.log('\n💡 To see actual SQL queries, check the SQLite adapter debug output');
    console.log('   Look for "SQLite RUNSTATEMENT DEBUG" messages in the output above');
  } catch (error) {
    console.error('❌ Test failed:', error);
    process.exit(1);
  }
}

if (require.main === module) {
  main().catch(console.error);
}