#!/usr/bin/env tsx
/**
 * Clean Database Seeding via JTAG Commands
 *
 * Uses factory functions to eliminate repetition and create clean data structures
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import { USER_IDS, ROOM_IDS, MESSAGE_IDS, USER_CONFIG, ROOM_CONFIG, MESSAGE_CONTENT } from '../api/data-seed/SeedConstants';
import { DATABASE_PATHS } from '../system/data/config/DatabaseConfig';
import { UserEntity } from '../system/data/entities/UserEntity';
import { RoomEntity } from '../system/data/entities/RoomEntity';
import { ChatMessageEntity } from '../system/data/entities/ChatMessageEntity';

const execAsync = promisify(exec);

// ===== FACTORY FUNCTIONS FOR DATA GENERATION =====

/**
 * Create user capabilities based on user type
 */
function createUserCapabilities(type: 'human' | 'ai'): any {
  const baseCapabilities = {
    canSendMessages: true,
    canReceiveMessages: true,
    canTrain: false,
  };

  if (type === 'human') {
    return {
      ...baseCapabilities,
      canCreateRooms: true,
      canInviteOthers: true,
      canModerate: true,
      autoResponds: false,
      providesContext: false,
      canAccessPersonas: true,
    };
  } else {
    return {
      ...baseCapabilities,
      canCreateRooms: true,
      canInviteOthers: true,
      canModerate: true,
      autoResponds: true,
      providesContext: true,
      canAccessPersonas: false,
    };
  }
}

/**
 * Create user profile
 */
function createUserProfile(displayName: string, avatar: string, bio: string, location: string): any {
  return {
    displayName,
    avatar,
    bio,
    location,
    joinedAt: new Date().toISOString()
  };
}

/**
 * Create complete user object
 */
function createUser(id: string, displayName: string, shortDescription: string, type: 'human' | 'ai', avatar: string, bio: string, location: string): any {
  return {
    id,
    displayName,
    shortDescription,
    type,
    profile: createUserProfile(displayName, avatar, bio, location),
    capabilities: createUserCapabilities(type),
    status: "online",
    lastActiveAt: new Date().toISOString(),
    sessionsActive: []
  };
}

/**
 * Create room privacy settings
 */
function createRoomPrivacy(isPublic: boolean = true): any {
  return {
    isPublic,
    requiresInvite: false,
    allowGuestAccess: true,
    searchable: true
  };
}

/**
 * Create room settings
 */
function createRoomSettings(): any {
  return {
    allowReactions: true,
    allowThreads: true,
    allowFileSharing: true,
    messageRetentionDays: 365
  };
}

/**
 * Create room stats
 */
function createRoomStats(memberCount: number): any {
  return {
    memberCount,
    messageCount: 0,
    createdAt: new Date().toISOString(),
    lastActivityAt: new Date().toISOString()
  };
}

/**
 * Create complete room object
 */
function createRoom(id: string, name: string, displayName: string, description: string, topic: string, memberCount: number, tags: string[]): any {
  return {
    id,
    name: name.toLowerCase(),
    displayName,
    description,
    topic,
    type: "public",
    status: "active",
    privacy: createRoomPrivacy(),
    settings: createRoomSettings(),
    stats: createRoomStats(memberCount),
    members: [],
    tags
  };
}

/**
 * Create message content
 */
function createMessageContent(text: string): any {
  return {
    text,
    attachments: [],
    formatting: {
      markdown: false,
      mentions: [],
      hashtags: [],
      links: [],
      codeBlocks: []
    }
  };
}

/**
 * Create complete message object
 */
function createMessage(id: string, roomId: string, senderId: string, senderName: string, text: string): any {
  return {
    id,
    roomId,
    senderId,
    senderName,
    content: createMessageContent(text),
    status: "sent",
    priority: "normal",
    timestamp: new Date().toISOString(),
    reactions: []
  };
}

// ===== SEEDING FUNCTIONS =====

/**
 * Create a record via JTAG command with proper shell escaping
 */
async function createRecord(collection: string, data: any, id: string, displayName?: string): Promise<boolean> {
  const dataArg = JSON.stringify(data).replace(/'/g, `'"'"'`);
  const cmd = `./jtag data/create --collection=${collection} --data='${dataArg}' --id=${id}`;

  try {
    const result = await execAsync(cmd);
    const success = result.stdout.includes('"success": true');

    if (success) {
      console.log(`✅ Created ${collection}: ${displayName || id}`);
      return true;
    } else {
      console.error(`❌ Failed to create ${collection} ${displayName || id}: Command returned unsuccessful result`);
      console.error(`Response: ${result.stdout}`);
      return false;
    }
  } catch (error: any) {
    const hasSuccess = error.stdout && error.stdout.includes('"success": true');

    if (hasSuccess) {
      console.log(`✅ Created ${collection}: ${displayName || id}`);
      return true;
    } else {
      console.error(`❌ Failed to create ${collection} ${displayName || id}:`);
      console.error(`   Error: ${error.message}`);
      if (error.stdout) console.error(`   Output: ${error.stdout.substring(0, 500)}...`);
      if (error.stderr) console.error(`   Stderr: ${error.stderr.substring(0, 500)}...`);
      return false;
    }
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

/**
 * Get count from JTAG list command
 */
async function getEntityCount(collection: string): Promise<string> {
  try {
    const result = await execAsync(`./jtag data/list --collection=${collection}`);
    return result.stdout.match(/"count":\s*(\d+)/)?.[1] || '0';
  } catch (error: any) {
    return error.stdout ? (error.stdout.match(/"count":\s*(\d+)/)?.[1] || '0') : '0';
  }
}

/**
 * Main seeding function
 */
async function seedViaJTAG() {
  console.log('🌱 Seeding database via JTAG commands (single source of truth)...');

  try {
    // Clear existing database tables
    console.log('🧹 Clearing existing database tables...');
    try {
      await execAsync(`sqlite3 ${DATABASE_PATHS.SQLITE} "DELETE FROM _data; DELETE FROM _collections;" 2>/dev/null || true`);
      console.log('✅ Database tables cleared');
    } catch (error: any) {
      console.log('ℹ️ Database tables not found or already empty, proceeding with seeding...');
    }

    // Create users using factory functions
    const users = [
      createUser(
        USER_IDS.HUMAN,
        USER_CONFIG.HUMAN.DISPLAY_NAME,
        "System architect & dev lead",
        "human",
        USER_CONFIG.HUMAN.AVATAR,
        "System architect and lead developer",
        "San Francisco, CA"
      ),
      createUser(
        USER_IDS.CLAUDE_CODE,
        USER_CONFIG.CLAUDE.NAME,
        "Code architect & debugger ⚡",
        "ai",
        "🤖",
        "AI assistant specialized in coding, architecture, and system design",
        "Anthropic Cloud"
      ),
      createUser(
        USER_IDS.GENERAL_AI,
        USER_CONFIG.GENERAL_AI.NAME,
        "General purpose assistant",
        "ai",
        "⚡",
        "General AI assistant for various tasks and conversations",
        "Anthropic Cloud"
      )
    ];

    // Create rooms using factory functions
    const rooms = [
      createRoom(
        ROOM_IDS.GENERAL,
        ROOM_CONFIG.GENERAL.NAME,
        ROOM_CONFIG.GENERAL.NAME,
        ROOM_CONFIG.GENERAL.DESCRIPTION,
        "Welcome to general discussion! Introduce yourself and chat about anything.",
        3,
        ["general", "welcome", "discussion"]
      ),
      createRoom(
        ROOM_IDS.ACADEMY,
        ROOM_CONFIG.ACADEMY.NAME,
        ROOM_CONFIG.ACADEMY.NAME,
        ROOM_CONFIG.ACADEMY.DESCRIPTION,
        "Share knowledge, tutorials, and collaborate on learning",
        2,
        ["academy", "learning", "education"]
      )
    ];

    // Create messages using factory functions
    const messages = [
      createMessage(
        MESSAGE_IDS.WELCOME_GENERAL,
        ROOM_IDS.GENERAL,
        'system',
        'System',
        MESSAGE_CONTENT.WELCOME_GENERAL
      ),
      createMessage(
        MESSAGE_IDS.CLAUDE_INTRO,
        ROOM_IDS.GENERAL,
        USER_IDS.CLAUDE_CODE,
        'Claude Code',
        MESSAGE_CONTENT.CLAUDE_INTRO
      ),
      createMessage(
        MESSAGE_IDS.WELCOME_ACADEMY,
        ROOM_IDS.ACADEMY,
        'system',
        'System',
        MESSAGE_CONTENT.WELCOME_ACADEMY
      )
    ];

    // Seed all data types using clean modular approach
    await seedRecords(UserEntity.collection, users, (user) => user.displayName);
    await seedRecords(RoomEntity.collection, rooms, (room) => room.displayName);
    await seedRecords(ChatMessageEntity.collection, messages, (msg) =>
      msg.senderId === USER_IDS.HUMAN ? 'Joel' :
      msg.senderId === USER_IDS.CLAUDE_CODE ? 'Claude' : 'Unknown'
    );

    // Verify seeded data
    console.log('\n📊 Verifying seeded data via JTAG...');
    console.log(`   Users: ${await getEntityCount(UserEntity.collection)}`);
    console.log(`   Rooms: ${await getEntityCount(RoomEntity.collection)}`);
    console.log(`   Messages: ${await getEntityCount(ChatMessageEntity.collection)}`);

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