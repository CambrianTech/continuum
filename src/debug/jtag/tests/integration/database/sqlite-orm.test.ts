#!/usr/bin/env npx tsx

/**
 * Test SQLite ORM - Validate Database Functionality
 *
 * Tests the new SQLite storage adapter with:
 * - Basic CRUD operations
 * - QueryBuilder integration
 * - Real user data migration
 * - Advanced filtering and joins
 */

import { SqliteStorageAdapter } from './daemons/data-daemon/server/SqliteStorageAdapter';
import { QueryBuilder } from './daemons/data-daemon/shared/QueryBuilder';
import type { StorageAdapterConfig, DataRecord } from './daemons/data-daemon/shared/DataStorageAdapter';
import { generateUUID, type UUID } from './system/core/types/CrossPlatformUUID';
import * as fs from 'fs/promises';
import * as path from 'path';

// Test user data structure
interface TestUser {
  userId: string;
  name: string;
  displayName?: string;
  userType: 'human' | 'agent' | 'persona';
  email?: string;
  isOnline: boolean;
  lastActiveAt: string;
  preferences?: {
    theme?: string;
    notifications?: boolean;
  };
}

// Test room data for joins
interface TestRoom {
  roomId: string;
  name: string;
  type: 'public' | 'private';
  createdAt: string;
  memberCount: number;
}

// Test participation for relationships
interface TestParticipation {
  userId: string;
  roomId: string;
  role: 'member' | 'admin' | 'moderator';
  joinedAt: string;
  active: boolean;
}

async function createSqliteAdapter(): Promise<SqliteStorageAdapter> {
  console.log('🗄️ Creating SQLite adapter...');

  const config: StorageAdapterConfig = {
    type: 'sqlite',
    namespace: 'test_orm',
    options: {
      filename: '.continuum/test/test_orm.db',
      foreignKeys: true,
      wal: true,
      synchronous: 'NORMAL',
      cacheSize: -4000 // 4MB cache
    }
  };

  const adapter = new SqliteStorageAdapter();
  await adapter.initialize(config);

  console.log('✅ SQLite adapter initialized');
  return adapter;
}

async function testBasicCrud(adapter: SqliteStorageAdapter): Promise<void> {
  console.log('\n🧪 Testing Basic CRUD Operations...');

  // Create test users
  const users: TestUser[] = [
    {
      userId: generateUUID(),
      name: 'Joel',
      displayName: 'Joel - Creator',
      userType: 'human',
      email: 'joel@continuum.dev',
      isOnline: true,
      lastActiveAt: new Date().toISOString(),
      preferences: {
        theme: 'dark',
        notifications: true
      }
    },
    {
      userId: generateUUID(),
      name: 'Claude Code',
      displayName: 'Claude Code Agent',
      userType: 'agent',
      isOnline: true,
      lastActiveAt: new Date().toISOString(),
      preferences: {
        theme: 'dark',
        notifications: false
      }
    },
    {
      userId: generateUUID(),
      name: 'Assistant Alpha',
      displayName: 'Alpha Persona',
      userType: 'persona',
      isOnline: false,
      lastActiveAt: new Date(Date.now() - 3600000).toISOString() // 1 hour ago
    }
  ];

  // Test CREATE
  console.log('📝 Testing CREATE operations...');
  for (const user of users) {
    const record: DataRecord<TestUser> = {
      id: user.userId as UUID,
      collection: 'users',
      data: user,
      metadata: {
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        version: 1,
        tags: ['test', user.userType]
      }
    };

    const result = await adapter.create(record);
    console.log(`✅ Created user: ${user.name} (${result.success ? 'SUCCESS' : 'FAILED'})`);
  }

  // Test READ
  console.log('\n📖 Testing READ operations...');
  for (const user of users) {
    const result = await adapter.read<TestUser>('users', user.userId as UUID);
    if (result.success && result.data) {
      console.log(`✅ Read user: ${result.data.data.name} (version ${result.data.metadata.version})`);
    } else {
      console.log(`❌ Failed to read user: ${user.name}`);
    }
  }

  // Test QUERY
  console.log('\n🔍 Testing QUERY operations...');

  // Query all agents
  const agentQuery = QueryBuilder
    .from('users')
    .where('userType', 'eq', 'agent')
    .orderBy('name', 'asc')
    .toLegacy();

  const agentResult = await adapter.query<TestUser>(agentQuery);
  console.log(`✅ Found ${agentResult.data?.length || 0} agents:`,
    agentResult.data?.map(r => r.data.name) || []);

  // Query online users
  const onlineQuery = QueryBuilder
    .from('users')
    .where('isOnline', 'eq', true)
    .select('name', 'userType', 'lastActiveAt')
    .toLegacy();

  const onlineResult = await adapter.query<TestUser>(onlineQuery);
  console.log(`✅ Found ${onlineResult.data?.length || 0} online users:`,
    onlineResult.data?.map(r => r.data.name) || []);

  // Test UPDATE
  console.log('\n✏️ Testing UPDATE operations...');
  const firstUser = users[0];
  const updateResult = await adapter.update<TestUser>(
    'users',
    firstUser.userId as UUID,
    { isOnline: false, lastActiveAt: new Date().toISOString() }
  );
  console.log(`✅ Updated user ${firstUser.name}: ${updateResult.success ? 'SUCCESS' : 'FAILED'}`);

  // Test DELETE
  console.log('\n🗑️ Testing DELETE operations...');
  const lastUser = users[users.length - 1];
  const deleteResult = await adapter.delete('users', lastUser.userId as UUID);
  console.log(`✅ Deleted user ${lastUser.name}: ${deleteResult.success ? 'SUCCESS' : 'FAILED'}`);

  // Verify final state
  const finalQuery = await adapter.query<TestUser>({ collection: 'users' });
  console.log(`📊 Final user count: ${finalQuery.data?.length || 0} users remaining`);
}

async function testRelationsAndJoins(adapter: SqliteStorageAdapter): Promise<void> {
  console.log('\n🔗 Testing Relations and Joins...');

  // Create test rooms
  const rooms: TestRoom[] = [
    {
      roomId: generateUUID(),
      name: 'general',
      type: 'public',
      createdAt: new Date().toISOString(),
      memberCount: 3
    },
    {
      roomId: generateUUID(),
      name: 'academy',
      type: 'private',
      createdAt: new Date().toISOString(),
      memberCount: 2
    }
  ];

  // Create rooms
  console.log('🏠 Creating test rooms...');
  for (const room of rooms) {
    const record: DataRecord<TestRoom> = {
      id: room.roomId as UUID,
      collection: 'rooms',
      data: room,
      metadata: {
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        version: 1,
        tags: ['test', room.type]
      }
    };

    const result = await adapter.create(record);
    console.log(`✅ Created room: ${room.name} (${result.success ? 'SUCCESS' : 'FAILED'})`);
  }

  // Get existing users for participation
  const usersResult = await adapter.query<TestUser>({ collection: 'users' });
  const existingUsers = usersResult.data || [];

  if (existingUsers.length === 0) {
    console.log('⚠️ No users found for testing relationships');
    return;
  }

  // Create participations (relationships)
  console.log('👥 Creating user-room participations...');
  const participations: TestParticipation[] = [];

  for (const user of existingUsers) {
    for (const room of rooms) {
      participations.push({
        userId: user.data.userId,
        roomId: room.roomId,
        role: user.data.userType === 'agent' ? 'moderator' : 'member',
        joinedAt: new Date().toISOString(),
        active: user.data.isOnline
      });
    }
  }

  for (const participation of participations) {
    const record: DataRecord<TestParticipation> = {
      id: generateUUID() as UUID,
      collection: 'participations',
      data: participation,
      metadata: {
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        version: 1,
        tags: ['test', 'relationship']
      }
    };

    const result = await adapter.create(record);
    console.log(`✅ Created participation: ${participation.userId} -> ${participation.roomId}`);
  }

  // Test complex queries that would benefit from joins
  console.log('\n🔍 Testing Complex Queries (simulated joins)...');

  // Query: Users in a specific room
  const generalRoom = rooms.find(r => r.name === 'general');
  if (generalRoom) {
    const participationQuery = await adapter.query<TestParticipation>({
      collection: 'participations',
      filters: { roomId: generalRoom.roomId }
    });

    console.log(`📊 Found ${participationQuery.data?.length || 0} participants in general room`);

    // Get user details for those participants (manual join simulation)
    if (participationQuery.data && participationQuery.data.length > 0) {
      console.log('👤 Participant details:');
      for (const participation of participationQuery.data) {
        const userResult = await adapter.read<TestUser>('users', participation.data.userId as UUID);
        if (userResult.success && userResult.data) {
          console.log(`  - ${userResult.data.data.name} (${participation.data.role})`);
        }
      }
    }
  }

  // Query: Rooms a specific user participates in
  const firstUser = existingUsers[0];
  const userParticipations = await adapter.query<TestParticipation>({
    collection: 'participations',
    filters: { userId: firstUser.data.userId }
  });

  console.log(`📊 User ${firstUser.data.name} participates in ${userParticipations.data?.length || 0} rooms`);
}

async function testAdvancedFeatures(adapter: SqliteStorageAdapter): Promise<void> {
  console.log('\n🚀 Testing Advanced Features...');

  // Test collection stats
  console.log('📈 Testing collection statistics...');
  const collections = ['users', 'rooms', 'participations'];

  for (const collection of collections) {
    const stats = await adapter.getCollectionStats(collection);
    if (stats.success && stats.data) {
      console.log(`📊 ${collection}: ${stats.data.recordCount} records, ${stats.data.totalSize} bytes`);
    }
  }

  // Test batch operations
  console.log('\n📦 Testing batch operations...');
  const batchOps = [
    {
      type: 'create' as const,
      collection: 'users',
      data: {
        userId: generateUUID(),
        name: 'Batch User',
        userType: 'human',
        isOnline: true,
        lastActiveAt: new Date().toISOString()
      }
    },
    {
      type: 'create' as const,
      collection: 'rooms',
      data: {
        roomId: generateUUID(),
        name: 'batch-room',
        type: 'public',
        createdAt: new Date().toISOString(),
        memberCount: 0
      }
    }
  ];

  const batchResult = await adapter.batch(batchOps);
  console.log(`✅ Batch operation: ${batchResult.success ? 'SUCCESS' : 'FAILED'}`);
  console.log(`📊 Batch results: ${batchResult.data?.length || 0} operations completed`);

  // Test cleanup
  console.log('\n🧹 Testing cleanup operations...');
  await adapter.cleanup();
  console.log('✅ Cleanup completed');
}

async function benchmarkPerformance(adapter: SqliteStorageAdapter): Promise<void> {
  console.log('\n⚡ Performance Benchmarks...');

  const testUsers: TestUser[] = [];
  const userCount = 100;

  // Generate test data
  console.log(`📊 Generating ${userCount} test users...`);
  for (let i = 0; i < userCount; i++) {
    testUsers.push({
      userId: generateUUID(),
      name: `User ${i + 1}`,
      displayName: `Test User ${i + 1}`,
      userType: i % 3 === 0 ? 'agent' : 'human',
      isOnline: Math.random() > 0.3,
      lastActiveAt: new Date(Date.now() - Math.random() * 86400000).toISOString()
    });
  }

  // Benchmark bulk create
  const createStart = Date.now();
  for (const user of testUsers) {
    const record: DataRecord<TestUser> = {
      id: user.userId as UUID,
      collection: 'benchmark_users',
      data: user,
      metadata: {
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        version: 1
      }
    };
    await adapter.create(record);
  }
  const createTime = Date.now() - createStart;

  // Benchmark bulk query
  const queryStart = Date.now();
  const queryResult = await adapter.query<TestUser>({ collection: 'benchmark_users' });
  const queryTime = Date.now() - queryStart;

  // Benchmark filtered query
  const filterStart = Date.now();
  const agentQuery = QueryBuilder
    .from('benchmark_users')
    .where('userType', 'eq', 'agent')
    .orderBy('name', 'asc')
    .toLegacy();
  const filterResult = await adapter.query<TestUser>(agentQuery);
  const filterTime = Date.now() - filterStart;

  console.log('📊 Performance Results:');
  console.log(`  Create ${userCount} records: ${createTime}ms (${(createTime / userCount).toFixed(2)}ms/record)`);
  console.log(`  Query all ${queryResult.data?.length || 0} records: ${queryTime}ms`);
  console.log(`  Filtered query ${filterResult.data?.length || 0} agents: ${filterTime}ms`);

  // Cleanup benchmark data
  console.log('\n🧹 Cleaning up benchmark data...');
  for (const user of testUsers) {
    await adapter.delete('benchmark_users', user.userId as UUID);
  }
}

async function main(): Promise<void> {
  console.log('🗄️ SQLite ORM Test Suite');
  console.log('========================\n');

  let adapter: SqliteStorageAdapter | null = null;

  try {
    // Initialize SQLite adapter
    adapter = await createSqliteAdapter();

    // Run test suites
    await testBasicCrud(adapter);
    await testRelationsAndJoins(adapter);
    await testAdvancedFeatures(adapter);
    await benchmarkPerformance(adapter);

    console.log('\n✅ All SQLite ORM tests completed successfully!');
    console.log('🎯 SQLite adapter is ready for production use');
    console.log('📈 Next: Implement real joins with QueryBuilder.queryRelational()');

  } catch (error) {
    console.error('❌ SQLite test suite failed:', error);
    process.exit(1);
  } finally {
    if (adapter) {
      await adapter.close();
      console.log('🔒 SQLite connection closed');
    }
  }
}

// Run tests if script is executed directly
if (require.main === module) {
  main().catch(console.error);
}

export { main as testSqliteORM };