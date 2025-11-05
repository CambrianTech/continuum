#!/usr/bin/env npx tsx

/**
 * Test Query Builder - Validate Basic Functionality
 *
 * Tests the new QueryBuilder against existing user data
 * before implementing joins and complex relational queries
 */

import { QueryBuilder, QueryUtils } from './daemons/data-daemon/shared/QueryBuilder';
import * as fs from 'fs/promises';
import * as path from 'path';
import { WorkingDirConfig } from './system/core/config/WorkingDirConfig';

// Test data structure matching our user records
interface TestUser {
  userId?: string;
  id?: string;
  name: string;
  displayName?: string;
  userType?: string;
  type?: string;
  isOnline?: boolean;
  lastActiveAt?: string;
  createdAt?: string;
  created?: string;
}

async function loadTestData(): Promise<TestUser[]> {
  console.log('🗄️ Loading test data from .continuum/jtag/data/users...');

  try {
    const continuumPath = WorkingDirConfig.getContinuumPath();
    const dataPath = path.join(continuumPath, 'jtag', 'data', 'users');

    const files = await fs.readdir(dataPath);
    const jsonFiles = files.filter(file => file.endsWith('.json'));

    const users: TestUser[] = [];
    for (const file of jsonFiles) {
      try {
        const filePath = path.join(dataPath, file);
        const content = await fs.readFile(filePath, 'utf-8');
        const userData = JSON.parse(content);
        users.push(userData);
      } catch (error) {
        console.warn(`⚠️ Failed to load ${file}:`, error);
      }
    }

    console.log(`✅ Loaded ${users.length} users from ${jsonFiles.length} files`);
    return users;
  } catch (error) {
    console.error('❌ Failed to load test data:', error);
    return [];
  }
}

function testBasicQueryBuilding(): void {
  console.log('\n🧪 Testing Basic Query Building...');

  // Test 1: Simple query
  const query1 = QueryBuilder
    .from('users')
    .where('userType', 'eq', 'agent')
    .orderBy('name', 'asc')
    .limit(10)
    .build();

  console.log('✅ Query 1 (agents, sorted by name):', JSON.stringify(query1, null, 2));

  // Test 2: Complex filtering
  const query2 = QueryBuilder
    .from('users')
    .where('isOnline', 'eq', true)
    .or('userType', 'eq', 'agent')
    .select('name', 'userType', 'lastActiveAt')
    .orderBy('lastActiveAt', 'desc')
    .build();

  console.log('✅ Query 2 (online users OR agents, with fields):', JSON.stringify(query2, null, 2));

  // Test 3: Legacy compatibility
  const legacyQuery = QueryBuilder
    .from('users')
    .where('userType', 'eq', 'human')
    .orderBy('name', 'asc')
    .limit(5)
    .toLegacy();

  console.log('✅ Query 3 (legacy format):', JSON.stringify(legacyQuery, null, 2));

  // Test 4: Aggregation query
  const query4 = QueryBuilder
    .from('users')
    .count('*', 'total_users')
    .groupBy('userType')
    .build();

  console.log('✅ Query 4 (count by user type):', JSON.stringify(query4, null, 2));
}

async function testFilterEvaluation(users: TestUser[]): Promise<void> {
  console.log('\n🧪 Testing Filter Evaluation...');

  // Test 1: Filter agents
  const agentFilter = { field: 'userType', operator: 'eq' as const, value: 'agent' };
  const agents = users.filter(user => QueryUtils.evaluateFilter(user, agentFilter));
  console.log(`✅ Found ${agents.length} agents:`, agents.map(u => u.name || u.displayName));

  // Test 2: Filter by name pattern
  const nameFilter = { field: 'name', operator: 'like' as const, value: 'Claude' };
  const claudeUsers = users.filter(user => QueryUtils.evaluateFilter(user, nameFilter));
  console.log(`✅ Found ${claudeUsers.length} users with 'Claude' in name:`, claudeUsers.map(u => u.name || u.displayName));

  // Test 3: Complex OR condition
  const complexFilter = {
    operator: 'or' as const,
    conditions: [
      { field: 'userType', operator: 'eq' as const, value: 'agent' },
      { field: 'type', operator: 'eq' as const, value: 'agent' }
    ]
  };
  const agentUsers = users.filter(user => QueryUtils.evaluateFilter(user, complexFilter));
  console.log(`✅ Found ${agentUsers.length} users with agent userType OR type:`, agentUsers.map(u => u.name || u.displayName));

  // Test 4: Field existence
  const idFilter = { field: 'userId', operator: 'exists' as const, value: true };
  const usersWithId = users.filter(user => QueryUtils.evaluateFilter(user, idFilter));
  console.log(`✅ Found ${usersWithId.length} users with userId field`);

  // Test 5: Date comparison (if we have date fields)
  const recentUsers = users.filter(user => {
    const dateField = user.lastActiveAt || user.createdAt || user.created;
    if (!dateField) return false;

    const userDate = new Date(dateField);
    const recentDate = new Date('2025-09-15'); // Recent threshold
    return userDate >= recentDate;
  });
  console.log(`✅ Found ${recentUsers.length} users active since 2025-09-15`);
}

async function testQueryBuilderWithRealData(users: TestUser[]): Promise<void> {
  console.log('\n🧪 Testing QueryBuilder with Real Data...');

  // Simulate executing different queries against our data
  const queries = [
    {
      name: 'All Agents Sorted by Name',
      query: QueryBuilder
        .from('users')
        .where('userType', 'eq', 'agent')
        .orderBy('name', 'asc')
        .build()
    },
    {
      name: 'First 3 Users',
      query: QueryBuilder
        .from('users')
        .limit(3)
        .select('name', 'userType')
        .build()
    },
    {
      name: 'Users with Names Containing "Test"',
      query: QueryBuilder
        .from('users')
        .where('name', 'like', 'Test')
        .count('*', 'test_user_count')
        .build()
    }
  ];

  for (const { name, query } of queries) {
    console.log(`\n📊 Executing Query: ${name}`);
    console.log('Query:', JSON.stringify(query, null, 2));

    // Simulate query execution
    let results = [...users];

    // Apply where filter
    if (query.where) {
      results = results.filter(user => QueryUtils.evaluateFilter(user, query.where!));
    }

    // Apply ordering
    if (query.orderBy && query.orderBy.length > 0) {
      results.sort((a, b) => {
        for (const sort of query.orderBy!) {
          const aValue = (a as any)[sort.field];
          const bValue = (b as any)[sort.field];

          if (aValue < bValue) return sort.direction === 'asc' ? -1 : 1;
          if (aValue > bValue) return sort.direction === 'asc' ? 1 : -1;
        }
        return 0;
      });
    }

    // Apply limit
    if (query.limit) {
      results = results.slice(0, query.limit);
    }

    // Apply field selection
    if (query.select) {
      results = results.map(user => {
        const selected: any = {};
        for (const field of query.select!) {
          selected[field] = (user as any)[field];
        }
        return selected;
      });
    }

    // Handle aggregations
    if (query.aggregations) {
      for (const agg of query.aggregations) {
        if (agg.type === 'count') {
          console.log(`📈 ${agg.alias || 'count'}: ${results.length}`);
        }
      }
    } else {
      console.log(`📊 Results (${results.length} records):`,
        results.slice(0, 3).map(r => r.name || r.displayName || 'unnamed'));
      if (results.length > 3) {
        console.log(`... and ${results.length - 3} more`);
      }
    }
  }
}

async function main(): Promise<void> {
  console.log('🚀 QueryBuilder Test Suite');
  console.log('==========================\n');

  try {
    // Test 1: Basic query building
    testBasicQueryBuilding();

    // Test 2: Load real data
    const users = await loadTestData();
    if (users.length === 0) {
      console.log('⚠️ No test data available, skipping data-dependent tests');
      return;
    }

    // Test 3: Filter evaluation
    await testFilterEvaluation(users);

    // Test 4: Query execution simulation
    await testQueryBuilderWithRealData(users);

    console.log('\n✅ All QueryBuilder tests completed successfully!');
    console.log('🎯 Ready to implement joins and advanced relational queries');

  } catch (error) {
    console.error('❌ Test suite failed:', error);
    process.exit(1);
  }
}

// Run the tests if this script is executed directly
if (require.main === module) {
  main().catch(console.error);
}

export { main as testQueryBuilder };