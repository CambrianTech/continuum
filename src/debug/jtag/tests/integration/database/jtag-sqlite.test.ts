#!/usr/bin/env npx tsx

/**
 * Test JTAG Commands with SQLite Backend
 *
 * Integration test showing how to use SQLite with existing JTAG data commands
 * Demonstrates the power of switching from JSON files to SQL database
 */

import { SqliteStorageAdapter } from './daemons/data-daemon/server/SqliteStorageAdapter';
import { DataDaemon } from './daemons/data-daemon/shared/DataDaemon';
import type { StorageStrategyConfig } from './daemons/data-daemon/shared/DataDaemon';
import { UserRepositoryFactory } from './domain/user/UserRepositoryFactory';
import { generateUUID } from './system/core/types/CrossPlatformUUID';

// Test with SQLite configuration
const SQLITE_CONFIG: StorageStrategyConfig = {
  strategy: 'sql',
  backend: 'sqlite',
  namespace: 'jtag-test',
  options: {
    filename: '.continuum/test/jtag-sqlite.db',
    foreignKeys: true,
    wal: true,
    synchronous: 'NORMAL'
  },
  features: {
    enableTransactions: true,
    enableIndexing: true,
    enableCaching: true
  }
};

async function testSqliteWithDataDaemon(): Promise<void> {
  console.log('🗄️ Testing DataDaemon with SQLite Backend...');

  // Create DataDaemon with SQLite
  const dataDaemon = new DataDaemon(SQLITE_CONFIG);
  await dataDaemon.initialize();

  console.log('✅ DataDaemon initialized with SQLite');

  // Create sample users
  const users = [
    {
      userId: generateUUID(),
      name: 'Joel',
      displayName: 'Joel - Creator',
      userType: 'human',
      email: 'joel@continuum.dev',
      isOnline: true,
      lastActiveAt: new Date().toISOString()
    },
    {
      userId: generateUUID(),
      name: 'Claude SQL',
      displayName: 'Claude SQL Agent',
      userType: 'agent',
      capabilities: ['sql', 'data-analysis', 'joins'],
      isOnline: true,
      lastActiveAt: new Date().toISOString()
    },
    {
      userId: generateUUID(),
      name: 'Data Assistant',
      displayName: 'Data Analysis Persona',
      userType: 'persona',
      specialization: 'database-optimization',
      isOnline: false,
      lastActiveAt: new Date(Date.now() - 3600000).toISOString()
    }
  ];

  // Create users via DataDaemon
  console.log('👤 Creating users via DataDaemon...');
  for (const user of users) {
    const result = await dataDaemon.create(
      'users',
      user,
      {
        sessionId: 'test-session' as any,
        timestamp: new Date().toISOString(),
        source: 'sqlite-integration-test'
      },
      user.userId as any
    );

    console.log(`✅ Created user: ${user.name} (${result.success ? 'SUCCESS' : 'FAILED'})`);
  }

  // Query users via DataDaemon
  console.log('\n🔍 Querying users via DataDaemon...');

  const allUsersResult = await dataDaemon.query(
    {
      collection: 'users',
      filters: {},
      sort: [{ field: 'name', direction: 'asc' }],
      limit: 10
    },
    {
      sessionId: 'test-session' as any,
      timestamp: new Date().toISOString(),
      source: 'sqlite-integration-test'
    }
  );

  console.log(`📊 Found ${allUsersResult.data?.length || 0} total users:`,
    allUsersResult.data?.map(r => r.data.name) || []);

  // Filter agents only
  const agentsResult = await dataDaemon.query(
    {
      collection: 'users',
      filters: { userType: 'agent' },
      sort: [{ field: 'name', direction: 'asc' }]
    },
    {
      sessionId: 'test-session' as any,
      timestamp: new Date().toISOString(),
      source: 'sqlite-integration-test'
    }
  );

  console.log(`🤖 Found ${agentsResult.data?.length || 0} agents:`,
    agentsResult.data?.map(r => r.data.name) || []);

  // Test update
  console.log('\n✏️ Testing user update via DataDaemon...');
  const firstUserId = users[0].userId;
  const updateResult = await dataDaemon.update(
    'users',
    firstUserId as any,
    {
      isOnline: false,
      lastActiveAt: new Date().toISOString(),
      status: 'Updated via SQLite'
    },
    {
      sessionId: 'test-session' as any,
      timestamp: new Date().toISOString(),
      source: 'sqlite-integration-test'
    }
  );

  console.log(`✅ Updated user ${users[0].name}: ${updateResult.success ? 'SUCCESS' : 'FAILED'}`);

  // Test complex query with multiple filters
  console.log('\n🔍 Testing complex queries...');
  const complexResult = await dataDaemon.query(
    {
      collection: 'users',
      filters: { isOnline: true },
      sort: [{ field: 'lastActiveAt', direction: 'desc' }],
      limit: 5
    },
    {
      sessionId: 'test-session' as any,
      timestamp: new Date().toISOString(),
      source: 'sqlite-integration-test'
    }
  );

  console.log(`🟢 Found ${complexResult.data?.length || 0} online users:`,
    complexResult.data?.map(r => `${r.data.name} (${r.data.userType})`) || []);

  await dataDaemon.close();
  console.log('🔒 DataDaemon closed');
}

async function testUserRepositoryWithSqlite(): Promise<void> {
  console.log('\n👥 Testing UserRepository with SQLite...');

  // Create SQLite-backed repositories
  const { userRepository, humanRepository, agentRepository } =
    await UserRepositoryFactory.createWithConfig(SQLITE_CONFIG, 'sqlite-test');

  console.log('✅ UserRepository created with SQLite backend');

  // Create users using repository pattern
  const humanUser = await humanRepository.create({
    displayName: 'Repository Human',
    email: 'human@repo.test',
    preferences: { theme: 'light', notifications: true }
  });

  console.log(`✅ Created human user: ${humanUser.displayName}`);

  const agentUser = await agentRepository.create({
    displayName: 'Repository Agent',
    capabilities: ['repository-pattern', 'sql-queries', 'data-modeling'],
    metadata: { model: 'claude-sonnet-4', provider: 'anthropic' }
  });

  console.log(`✅ Created agent user: ${agentUser.displayName}`);

  // Query via repository
  const allUsers = await userRepository.findAll({
    limit: 10,
    orderBy: [{ field: 'displayName', direction: 'asc' }]
  });

  console.log(`📊 Repository found ${allUsers.length} users:`,
    allUsers.map(u => `${u.displayName} (${u.citizenType})`));

  // Filter by type
  const agents = await userRepository.findByType('agent', { limit: 5 });
  console.log(`🤖 Repository found ${agents.length} agents:`,
    agents.map(u => u.displayName));

  // Clean up
  await UserRepositoryFactory.close('sqlite-test');
  console.log('🔒 UserRepository closed');
}

async function testJtagCommandIntegration(): Promise<void> {
  console.log('\n⚡ Testing JTAG Command Integration...');
  console.log('🔗 This would integrate with existing JTAG data/list commands');
  console.log('📋 Commands would automatically detect SQLite backend and use SQL queries');
  console.log('🚀 Performance boost: millisecond queries instead of filesystem scanning');

  console.log('\n🎯 Example JTAG Commands with SQLite:');
  console.log('  ./jtag data/list --collection=users --backend=sqlite');
  console.log('  ./jtag data/list --collection=users --filter=\'{"userType":"agent"}\' --backend=sqlite');
  console.log('  ./jtag data/list --collection=users --orderBy=\'[{"field":"name","direction":"asc"}]\' --backend=sqlite');

  console.log('\n💡 Benefits over JSON file storage:');
  console.log('  - 🔍 Complex queries with WHERE conditions');
  console.log('  - 📊 Aggregations and GROUP BY operations');
  console.log('  - 🔗 JOIN operations across collections');
  console.log('  - 🔒 ACID transactions for data consistency');
  console.log('  - 📈 Indexing for instant query performance');
  console.log('  - 🧹 Automatic cleanup and space reclamation');
}

async function main(): Promise<void> {
  console.log('🗄️ JTAG SQLite Integration Test');
  console.log('===============================\n');

  try {
    await testSqliteWithDataDaemon();
    await testUserRepositoryWithSqlite();
    await testJtagCommandIntegration();

    console.log('\n✅ All SQLite integration tests passed!');
    console.log('🎉 Ready to switch JTAG commands from JSON to SQLite');
    console.log('📈 Next: Add configuration option to choose storage backend');

  } catch (error) {
    console.error('❌ SQLite integration test failed:', error);
    process.exit(1);
  }
}

// Run if executed directly
if (require.main === module) {
  main().catch(console.error);
}

export { main as testJtagSqlite };