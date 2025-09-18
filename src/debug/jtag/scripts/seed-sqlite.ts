#!/usr/bin/env tsx
/**
 * Professional Database Seeding using ORM Export/Import
 *
 * Uses DataService built-in import/export functionality
 * Same mechanism used for regular data operations - no special seeding logic
 * Import entities that were previously exported from live system
 */

import { DataServiceFactory } from '../system/data/services/DataServiceFactory';
import { generatedSeedData } from '../data/seed/generatedSeedData';

async function seedDatabase() {
  console.log('🌱 Seeding database using ORM import/export functionality...');

  try {
    // Create DataService with proper initialization
    const dataService = await DataServiceFactory.createSQLiteOnly('.continuum/database/continuum.db');

    console.log('🔧 Initializing DataService and creating tables...');
    const initResult = await dataService.initialize();
    if (!initResult.success) {
      throw new Error(`DataService initialization failed: ${initResult.error?.message}`);
    }

    console.log(`📦 Loading seed data exported at: ${generatedSeedData.exportedAt}`);

    // Import users using ORM import method
    console.log('👥 Importing users...');
    const usersResult = await dataService.import('users', generatedSeedData.collections.users.map(user => {
      const { id, createdAt, updatedAt, version, ...cleanUser } = user;
      return cleanUser;
    }));
    if (!usersResult.success) {
      throw new Error(`Failed to import users: ${usersResult.error?.message}`);
    }
    console.log(`✅ Imported ${usersResult.data.imported} users`);
    if (usersResult.data.errors.length > 0) {
      console.warn('⚠️ User import errors:', usersResult.data.errors);
    }

    // Import rooms using ORM import method
    console.log('🏠 Importing rooms...');
    const roomsResult = await dataService.import('rooms', generatedSeedData.collections.rooms.map(room => {
      const { id, createdAt, updatedAt, version, ...cleanRoom } = room;
      return cleanRoom;
    }));
    if (!roomsResult.success) {
      throw new Error(`Failed to import rooms: ${roomsResult.error?.message}`);
    }
    console.log(`✅ Imported ${roomsResult.data.imported} rooms`);
    if (roomsResult.data.errors.length > 0) {
      console.warn('⚠️ Room import errors:', roomsResult.data.errors);
    }

    // Import messages using ORM import method
    console.log('💬 Importing chat messages...');
    const messagesResult = await dataService.import('chat_messages', generatedSeedData.collections.chat_messages.map(message => {
      const { id, createdAt, updatedAt, version, ...cleanMessage } = message;
      return cleanMessage;
    }));
    if (!messagesResult.success) {
      throw new Error(`Failed to import messages: ${messagesResult.error?.message}`);
    }
    console.log(`✅ Imported ${messagesResult.data.imported} messages`);
    if (messagesResult.data.errors.length > 0) {
      console.warn('⚠️ Message import errors:', messagesResult.data.errors);
    }

    // Verify what we imported using DataService.list()
    console.log('\n📊 Verifying imported data...');

    const usersVerify = await dataService.list('users');
    if (usersVerify.success) {
      console.log(`   Users: ${usersVerify.data.length} records`);
    }

    const roomsVerify = await dataService.list('rooms');
    if (roomsVerify.success) {
      console.log(`   Rooms: ${roomsVerify.data.length} records`);
    }

    const messagesVerify = await dataService.list('chat_messages');
    if (messagesVerify.success) {
      console.log(`   Chat Messages: ${messagesVerify.data.length} records`);
    }

    await dataService.close();

    console.log('\n🎉 Database seeding completed using ORM import/export!');
    console.log('💡 To export current data: dataService.exportAll([\'users\', \'rooms\', \'chat_messages\'])');

  } catch (error: any) {
    console.error('❌ SEEDING FAILED:', error.message);
    console.error(error.stack);
    process.exit(1);
  }
}

// Run if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  seedDatabase();
}

export default seedDatabase;