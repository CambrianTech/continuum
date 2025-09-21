#!/usr/bin/env tsx
/**
 * Proper Database Seeding via JTAG Commands
 *
 * Uses shell commands to JTAG CLI - same connection that widgets use
 * This ensures single source of truth and immediate visibility
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import { generatedSeedData } from '../data/seed/generatedSeedData';
import { DATABASE_PATHS } from '../system/data/config/DatabaseConfig';

const execAsync = promisify(exec);

async function seedViaJTAG() {
  console.log('🌱 Seeding database via JTAG commands (single source of truth)...');

  try {
    // CRITICAL: Clear existing database tables first to prevent duplicates
    console.log('🧹 Clearing existing database tables...');
    try {
      // Clear SQLite database tables directly
      await execAsync(`sqlite3 ${DATABASE_PATHS.SQLITE} "DELETE FROM _data; DELETE FROM _collections;" 2>/dev/null || true`);
      console.log('✅ Database tables cleared');
    } catch (error: any) {
      console.log('ℹ️ Database tables not found or already empty, proceeding with seeding...');
    }

    // Seed users
    console.log('👥 Creating users via JTAG...');
    for (const user of generatedSeedData.collections.users) {
      const { id, createdAt, updatedAt, version, ...cleanUser } = user;

      const dataArg = JSON.stringify(JSON.stringify(cleanUser));
      const cmd = `./jtag data/create --collection=User --data=${dataArg}`;

      try {
        await execAsync(cmd);
        console.log(`✅ Created user: ${cleanUser.profile?.displayName || cleanUser.userId}`);
      } catch (error: any) {
        console.warn(`⚠️ Failed to create user ${cleanUser.userId}: ${error.message}`);
      }
    }

    // Seed rooms
    console.log('🏠 Creating rooms via JTAG...');
    for (const room of generatedSeedData.collections.rooms) {
      const { id, createdAt, updatedAt, version, ...cleanRoom } = room;

      const dataArg = JSON.stringify(JSON.stringify(cleanRoom));
      const cmd = `./jtag data/create --collection=Room --data=${dataArg}`;

      try {
        await execAsync(cmd);
        console.log(`✅ Created room: ${cleanRoom.name}`);
      } catch (error: any) {
        console.warn(`⚠️ Failed to create room ${cleanRoom.roomId}: ${error.message}`);
      }
    }

    // Seed messages
    console.log('💬 Creating messages via JTAG...');
    for (const message of generatedSeedData.collections.chat_messages) {
      const { id, createdAt, updatedAt, version, ...cleanMessage } = message;

      const dataArg = JSON.stringify(JSON.stringify(cleanMessage));
      const cmd = `./jtag data/create --collection=ChatMessage --data=${dataArg}`;

      try {
        await execAsync(cmd);
        console.log(`✅ Created message from: ${cleanMessage.authorId}`);
      } catch (error: any) {
        console.warn(`⚠️ Failed to create message: ${error.message}`);
      }
    }

    // Verify via JTAG
    console.log('\n📊 Verifying seeded data via JTAG...');

    const usersResult = await execAsync('./jtag data/list --collection=User');
    const userCount = usersResult.stdout.match(/"count":\s*(\d+)/)?.[1] || '0';
    console.log(`   Users: ${userCount}`);

    const roomsResult = await execAsync('./jtag data/list --collection=Room');
    const roomCount = roomsResult.stdout.match(/"count":\s*(\d+)/)?.[1] || '0';
    console.log(`   Rooms: ${roomCount}`);

    const messagesResult = await execAsync('./jtag data/list --collection=ChatMessage');
    const messageCount = messagesResult.stdout.match(/"count":\s*(\d+)/)?.[1] || '0';
    console.log(`   Messages: ${messageCount}`);

    console.log('\n🎉 Database seeding completed via JTAG (single source of truth)!');

  } catch (error: any) {
    console.error('❌ SEEDING FAILED:', error.message);
    process.exit(1);
  }
}

// Run if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  seedViaJTAG();
}

export default seedViaJTAG;