#!/usr/bin/env tsx
/**
 * Test Migration from JSON to SQLite
 *
 * Tests if the hybrid migration mode can successfully migrate existing JSON data
 */

import { DataServiceFactory } from '../system/data/services/DataServiceFactory';

async function testMigration() {
  console.log('🔄 Testing JSON to SQLite migration...');

  try {
    // First, create a JSON-only service to verify current data
    console.log('\n📋 Step 1: Check current JSON data');
    const jsonService = await DataServiceFactory.createJsonCompatible();

    const usersResult = await jsonService.list('users');
    const roomsResult = await jsonService.list('rooms');
    const messagesResult = await jsonService.list('chat_messages');

    console.log(`   JSON Users: ${usersResult.success ? usersResult.data.length : 'failed'}`);
    console.log(`   JSON Rooms: ${roomsResult.success ? roomsResult.data.length : 'failed'}`);
    console.log(`   JSON Messages: ${messagesResult.success ? messagesResult.data.length : 'failed'}`);

    await jsonService.close();

    // Now create a hybrid migrating service
    console.log('\n📋 Step 2: Create hybrid migration service');
    const hybridService = await DataServiceFactory.createHybridMigrating();

    // Try to list data (should trigger migration)
    console.log('\n📋 Step 3: Trigger migration by reading data');
    const hybridUsersResult = await hybridService.list('users');
    const hybridRoomsResult = await hybridService.list('rooms');
    const hybridMessagesResult = await hybridService.list('chat_messages');

    console.log(`   Hybrid Users: ${hybridUsersResult.success ? hybridUsersResult.data.length : 'failed'}`);
    console.log(`   Hybrid Rooms: ${hybridRoomsResult.success ? hybridRoomsResult.data.length : 'failed'}`);
    console.log(`   Hybrid Messages: ${hybridMessagesResult.success ? hybridMessagesResult.data.length : 'failed'}`);

    await hybridService.close();

    // Finally, test pure SQLite service to confirm migration
    console.log('\n📋 Step 4: Verify SQLite contains migrated data');
    const sqliteService = await DataServiceFactory.createSQLiteOnly();

    const sqliteUsersResult = await sqliteService.list('users');
    const sqliteRoomsResult = await sqliteService.list('rooms');
    const sqliteMessagesResult = await sqliteService.list('chat_messages');

    console.log(`   SQLite Users: ${sqliteUsersResult.success ? sqliteUsersResult.data.length : 'failed'}`);
    console.log(`   SQLite Rooms: ${sqliteRoomsResult.success ? sqliteRoomsResult.data.length : 'failed'}`);
    console.log(`   SQLite Messages: ${sqliteMessagesResult.success ? sqliteMessagesResult.data.length : 'failed'}`);

    await sqliteService.close();

    console.log('\n✅ Migration test completed successfully!');
    console.log('💡 If SQLite numbers match JSON numbers, migration worked!');

  } catch (error: any) {
    console.error('❌ MIGRATION TEST FAILED:', error.message);
    console.error(error.stack);
  }
}

testMigration();