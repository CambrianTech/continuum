#!/usr/bin/env tsx
/**
 * Proper Database Seeding via JTAG Commands
 *
 * Uses shell commands to JTAG CLI - same connection that widgets use
 * This ensures single source of truth and immediate visibility
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import { USER_IDS, ROOM_IDS, MESSAGE_IDS, USER_CONFIG, ROOM_CONFIG, MESSAGE_CONTENT } from '../api/data-seed/SeedConstants';
import { DATABASE_PATHS } from '../system/data/config/DatabaseConfig';
import { UserEntity } from '../system/data/entities/UserEntity';
import { RoomEntity } from '../system/data/entities/RoomEntity';
import { ChatMessageEntity } from '../system/data/entities/ChatMessageEntity';

const execAsync = promisify(exec);

/**
 * Create a record via JTAG command with proper shell escaping
 */
async function createRecord(collection: string, data: any, id: string, displayName?: string): Promise<boolean> {
  const dataArg = JSON.stringify(data).replace(/'/g, `'"'"'`);
  const cmd = `./jtag data/create --collection=${collection} --data='${dataArg}' --id=${id}`;

  try {
    await execAsync(cmd);
    if (displayName) {
      console.log(`✅ Created ${collection}: ${displayName}`);
    } else {
      console.log(`✅ Created ${collection}: ${id}`);
    }
    return true;
  } catch (error: any) {
    console.warn(`⚠️ Failed to create ${collection} ${displayName || id}: ${error.message}`);
    console.warn(`Command: ${cmd}`);
    return false;
  }
}

/**
 * Seed multiple records of the same type
 */
async function seedRecords<T extends { id: string; displayName?: string }>(collection: string, records: T[], getDisplayName?: (record: T) => string): Promise<void> {
  console.log(`📝 Creating ${records.length} ${collection} records via JTAG...`);

  let successCount = 0;
  for (const record of records) {
    const displayName = getDisplayName ? getDisplayName(record) : record.displayName || record.id;
    const success = await createRecord(collection, record, record.id, displayName);
    if (success) successCount++;
  }

  console.log(`📊 Created ${successCount}/${records.length} ${collection} records`);
}

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

    // Prepare users data
    const users = [
      {
        id: USER_IDS.HUMAN,
        displayName: USER_CONFIG.HUMAN.DISPLAY_NAME,
        shortDescription: "System architect & dev lead",
        type: "human",
        profile: {
          displayName: USER_CONFIG.HUMAN.DISPLAY_NAME,
          avatar: USER_CONFIG.HUMAN.AVATAR,
          bio: "System architect and lead developer",
          location: "San Francisco, CA",
          joinedAt: new Date().toISOString()
        },
        capabilities: {
          canSendMessages: true,
          canReceiveMessages: true,
          canCreateRooms: true,
          canInviteOthers: true,
          canModerate: true,
          autoResponds: false,
          providesContext: false,
          canTrain: false,
          canAccessPersonas: true
        },
        status: "online",
        lastActiveAt: new Date().toISOString(),
        sessionsActive: []
      },
      {
        id: USER_IDS.CLAUDE_CODE,
        displayName: USER_CONFIG.CLAUDE.NAME,
        shortDescription: "Code architect & debugger ⚡",
        type: "ai",
        profile: {
          displayName: USER_CONFIG.CLAUDE.NAME,
          avatar: "🤖",
          bio: "AI assistant specialized in coding, architecture, and system design",
          location: "Anthropic Cloud",
          joinedAt: new Date().toISOString()
        },
        capabilities: {
          canSendMessages: true,
          canReceiveMessages: true,
          canCreateRooms: true,
          canInviteOthers: true,
          canModerate: true,
          autoResponds: true,
          providesContext: true,
          canTrain: false,
          canAccessPersonas: false
        },
        status: "online",
        lastActiveAt: new Date().toISOString(),
        sessionsActive: []
      },
      {
        id: USER_IDS.GENERAL_AI,
        displayName: USER_CONFIG.GENERAL_AI.NAME,
        shortDescription: "General purpose assistant",
        type: "ai",
        profile: {
          displayName: USER_CONFIG.GENERAL_AI.NAME,
          avatar: "⚡",
          bio: "General AI assistant for various tasks and conversations",
          location: "Anthropic Cloud",
          joinedAt: new Date().toISOString()
        },
        capabilities: {
          canSendMessages: true,
          canReceiveMessages: true,
          canCreateRooms: false,
          canInviteOthers: false,
          canModerate: false,
          autoResponds: true,
          providesContext: true,
          canTrain: false,
          canAccessPersonas: false
        },
        status: "online",
        lastActiveAt: new Date().toISOString(),
        sessionsActive: []
      }
    ];

    // Prepare rooms data
    const rooms = [
      {
        id: ROOM_IDS.GENERAL,
        name: ROOM_CONFIG.GENERAL.NAME.toLowerCase(),
        displayName: ROOM_CONFIG.GENERAL.NAME,
        description: ROOM_CONFIG.GENERAL.DESCRIPTION,
        topic: "Welcome to general discussion! Introduce yourself and chat about anything.",
        type: "public",
        status: "active",
        privacy: {
          isPublic: true,
          requiresInvite: false,
          allowGuestAccess: true,
          searchable: true
        },
        settings: {
          allowReactions: true,
          allowThreads: true,
          allowFileSharing: true,
          messageRetentionDays: 365
        },
        stats: {
          memberCount: 3,
          messageCount: 0,
          createdAt: new Date().toISOString(),
          lastActivityAt: new Date().toISOString()
        },
        members: [],
        tags: ["general", "welcome", "discussion"]
      },
      {
        id: ROOM_IDS.ACADEMY,
        name: ROOM_CONFIG.ACADEMY.NAME.toLowerCase(),
        displayName: ROOM_CONFIG.ACADEMY.NAME,
        description: ROOM_CONFIG.ACADEMY.DESCRIPTION,
        topic: "Share knowledge, tutorials, and collaborate on learning",
        type: "public",
        status: "active",
        privacy: {
          isPublic: true,
          requiresInvite: false,
          allowGuestAccess: true,
          searchable: true
        },
        settings: {
          allowReactions: true,
          allowThreads: true,
          allowFileSharing: true,
          messageRetentionDays: 365
        },
        stats: {
          memberCount: 2,
          messageCount: 0,
          createdAt: new Date().toISOString(),
          lastActivityAt: new Date().toISOString()
        },
        members: [],
        tags: ["academy", "learning", "education"]
      }
    ];

    // Prepare messages data
    const messages = [
      {
        id: MESSAGE_IDS.WELCOME_GENERAL,
        roomId: ROOM_IDS.GENERAL,
        senderId: 'system', // Use system user for welcome messages
        senderName: 'System',
        content: {
          text: MESSAGE_CONTENT.WELCOME_GENERAL,
          attachments: [],
          formatting: {
            markdown: false,
            mentions: [],
            hashtags: [],
            links: [],
            codeBlocks: []
          }
        },
        status: "sent",
        priority: "normal",
        timestamp: new Date().toISOString(),
        reactions: []
      },
      {
        id: MESSAGE_IDS.CLAUDE_INTRO,
        roomId: ROOM_IDS.GENERAL,
        senderId: USER_IDS.CLAUDE_CODE,
        senderName: 'Claude Code',
        content: {
          text: MESSAGE_CONTENT.CLAUDE_INTRO,
          attachments: [],
          formatting: {
            markdown: false,
            mentions: [],
            hashtags: [],
            links: [],
            codeBlocks: []
          }
        },
        status: "sent",
        priority: "normal",
        timestamp: new Date().toISOString(),
        reactions: []
      },
      {
        id: MESSAGE_IDS.WELCOME_ACADEMY,
        roomId: ROOM_IDS.ACADEMY,
        senderId: 'system', // Use system user for welcome messages
        senderName: 'System',
        content: {
          text: MESSAGE_CONTENT.WELCOME_ACADEMY,
          attachments: [],
          formatting: {
            markdown: false,
            mentions: [],
            hashtags: [],
            links: [],
            codeBlocks: []
          }
        },
        status: "sent",
        priority: "normal",
        timestamp: new Date().toISOString(),
        reactions: []
      }
    ];

    // Seed all data types using modular approach
    await seedRecords(UserEntity.collection, users, (user) => user.displayName);
    await seedRecords(RoomEntity.collection, rooms, (room) => room.displayName);
    await seedRecords(ChatMessageEntity.collection, messages, (msg) =>
      msg.senderId === USER_IDS.HUMAN ? 'Joel' :
      msg.senderId === USER_IDS.CLAUDE_CODE ? 'Claude' : 'Unknown'
    );

    // Verify via JTAG
    console.log('\n📊 Verifying seeded data via JTAG...');

    const usersResult = await execAsync(`./jtag data/list --collection=${UserEntity.collection}`);
    const userCount = usersResult.stdout.match(/"count":\s*(\d+)/)?.[1] || '0';
    console.log(`   Users: ${userCount}`);

    const roomsResult = await execAsync(`./jtag data/list --collection=${RoomEntity.collection}`);
    const roomCount = roomsResult.stdout.match(/"count":\s*(\d+)/)?.[1] || '0';
    console.log(`   Rooms: ${roomCount}`);

    const messagesResult = await execAsync(`./jtag data/list --collection=${ChatMessageEntity.collection}`);
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