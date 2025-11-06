#!/usr/bin/env tsx
/**
 * Cleanup Test Entities Script
 *
 * Removes test entities left over from failed integration tests.
 * Uses JTAG client directly instead of brittle CLI parsing.
 */

import { jtag } from '../server-index';
import { UserEntity } from '../system/data/entities/UserEntity';
import { RoomEntity } from '../system/data/entities/RoomEntity';
import { ChatMessageEntity } from '../system/data/entities/ChatMessageEntity';
import { isTestUser, isTestRoom, isTestMessage } from '../tests/shared/TestEntityConstants';

/**
 * Old uniqueId formats that should be cleaned up
 */
const OLD_BROKEN_UNIQUE_IDS = [
  'human-joel',
  'primary-human',
  'claude-code',
  'general-ai',
  'persona-helper-001',
  'persona-teacher-001',
  'persona-codereview-001',
];

async function cleanupTestEntities(): Promise<void> {
  console.log('🧹 Cleaning up test entities...');

  let client = null;

  try {
    // Connect to JTAG
    client = await jtag.connect();
    let cleanedCount = 0;

    // Clean up test users
    console.log('\n📋 Checking users...');
    const usersResult = await client.commands['data/list']({
      collection: UserEntity.collection,
      limit: 100
    });

    if (usersResult.success && Array.isArray(usersResult.items)) {
      console.log(`   Found ${usersResult.items.length} users (first 100)`);

      for (const user of usersResult.items) {
        // Check if test user
        if (isTestUser(user)) {
          console.log(`   🗑️  Deleting test user: ${user.displayName} (${user.uniqueId})`);
          await client.commands['data/delete']({
            collection: UserEntity.collection,
            id: user.id
          });
          cleanedCount++;
        }

        // Check for old broken uniqueId formats
        if (user.uniqueId && OLD_BROKEN_UNIQUE_IDS.includes(user.uniqueId)) {
          console.log(`   🗑️  Deleting old broken uniqueId: ${user.displayName} (${user.uniqueId})`);
          await client.commands['data/delete']({
            collection: UserEntity.collection,
            id: user.id
          });
          cleanedCount++;
        }
      }
    }

    // Clean up test rooms
    console.log('\n📋 Checking rooms...');
    const roomsResult = await client.commands['data/list']({
      collection: RoomEntity.collection,
      limit: 100
    });

    if (roomsResult.success && Array.isArray(roomsResult.items)) {
      console.log(`   Found ${roomsResult.items.length} rooms (first 100)`);

      for (const room of roomsResult.items) {
        if (isTestRoom(room)) {
          console.log(`   🗑️  Deleting test room: ${room.displayName} (${room.uniqueId})`);
          await client.commands['data/delete']({
            collection: RoomEntity.collection,
            id: room.id
          });
          cleanedCount++;
        }
      }
    }

    // Clean up test messages
    console.log('\n📋 Checking messages...');
    const messagesResult = await client.commands['data/list']({
      collection: ChatMessageEntity.collection,
      limit: 100
    });

    if (messagesResult.success && Array.isArray(messagesResult.items)) {
      console.log(`   Found ${messagesResult.items.length} messages (first 100)`);

      for (const message of messagesResult.items) {
        if (isTestMessage(message)) {
          console.log(`   🗑️  Deleting test message: ${message.id}`);
          await client.commands['data/delete']({
            collection: ChatMessageEntity.collection,
            id: message.id
          });
          cleanedCount++;
        }
      }
    }

    console.log(`\n✅ Cleanup complete: removed ${cleanedCount} test entities`);

  } catch (error) {
    console.error(`❌ Cleanup failed:`, error);
    process.exit(1);
  } finally {
    if (client) {
      await client.disconnect();
    }
  }
}

// Run cleanup
cleanupTestEntities();
