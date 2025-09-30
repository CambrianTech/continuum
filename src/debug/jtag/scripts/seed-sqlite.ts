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
import { UserStateEntity } from '../system/data/entities/UserStateEntity';
import { ContentTypeEntity } from '../system/data/entities/ContentTypeEntity';
import { TrainingSessionEntity } from '../system/data/entities/TrainingSessionEntity';

const execAsync = promisify(exec);

// ===== FACTORY FUNCTIONS FOR DATA GENERATION =====

/**
 * Create user capabilities based on user type
 */
function createUserCapabilities(type: 'human' | 'agent'): any {
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
  } else { // agent
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
 * Create user preferences with sensible defaults
 */
function createUserPreferences(): any {
  return {
    theme: 'dark',
    language: 'en',
    timezone: 'UTC',
    notifications: {
      mentions: true,
      directMessages: true,
      roomUpdates: false
    },
    privacy: {
      showOnlineStatus: true,
      allowDirectMessages: true,
      shareActivity: false
    }
  };
}

/**
 * Create complete user object
 */
function createUser(id: string, displayName: string, shortDescription: string, type: 'human' | 'agent', avatar: string, bio: string, location: string): any {
  return {
    id,
    displayName,
    shortDescription,
    type,
    profile: createUserProfile(displayName, avatar, bio, location),
    capabilities: createUserCapabilities(type),
    preferences: createUserPreferences(),
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
function createRoom(id: string, name: string, displayName: string, description: string, topic: string, memberCount: number, tags: string[], ownerId: string): any {
  return {
    id,
    name: name.toLowerCase(),
    displayName,
    description,
    topic,
    type: "public",
    status: "active",
    ownerId,
    lastMessageAt: new Date().toISOString(), // Set to current time for new rooms
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

/**
 * Create default content type registry
 */
function createDefaultContentTypes(): any[] {
  return [
    {
      id: 'ct-chat',
      type: 'chat',
      displayName: 'Chat Room',
      description: 'Real-time chat communication',
      category: 'communication',
      config: {
        widgetSelector: 'chat-widget',
        allowMultiple: true,
        autoSave: true,
        preloadData: true,
        requiredPermissions: ['chat:read', 'chat:write'],
        minUserType: 'human'
      },
      isActive: true,
      isBuiltIn: true,
      sortOrder: 10
    },
    {
      id: 'ct-academy',
      type: 'academy-session',
      displayName: 'Academy Training',
      description: 'AI training sessions with hyperparameters',
      category: 'development',
      config: {
        widgetSelector: 'chat-widget',
        allowMultiple: true,
        autoSave: true,
        preloadData: true,
        requiredPermissions: ['academy:read', 'academy:participate'],
        minUserType: 'human'
      },
      isActive: true,
      isBuiltIn: true,
      sortOrder: 20
    },
    {
      id: 'ct-user-list',
      type: 'user-list',
      displayName: 'User Directory',
      description: 'User management and directory',
      category: 'management',
      config: {
        widgetSelector: 'user-list-widget',
        allowMultiple: false,
        autoSave: false,
        preloadData: true,
        requiredPermissions: ['users:read'],
        minUserType: 'human'
      },
      isActive: true,
      isBuiltIn: true,
      sortOrder: 30
    }
  ];
}

/**
 * Create default user states - Fixed to match UserStateEntity schema
 */
function createDefaultUserStates(): any[] {
  return [
    {
      id: 'us-joel-chat',
      userId: USER_IDS.HUMAN,
      deviceId: 'browser-main',
      contentState: {
        openItems: [],
        lastUpdatedAt: new Date().toISOString()
      },
      preferences: {
        maxOpenTabs: 10,
        autoCloseAfterDays: 30,
        rememberScrollPosition: true,
        syncAcrossDevices: true,
        theme: 'dark'  // Custom preference for theme persistence
      }
    },
    {
      id: 'us-claude-chat',
      userId: USER_IDS.CLAUDE_CODE,
      deviceId: 'server-instance',
      contentState: {
        openItems: [],
        lastUpdatedAt: new Date().toISOString()
      },
      preferences: {
        maxOpenTabs: 15,
        autoCloseAfterDays: 60,
        rememberScrollPosition: true,
        syncAcrossDevices: true,
        theme: 'matrix'  // Custom preference for theme persistence
      }
    }
  ];
}

/**
 * Create default training sessions
 */
function createDefaultTrainingSessions(): any[] {
  return [
    {
      id: 'ts-js-fundamentals',
      roomId: ROOM_IDS.ACADEMY,
      teacherUserId: USER_IDS.CLAUDE_CODE,
      studentUserId: USER_IDS.HUMAN,
      sessionName: 'JavaScript Fundamentals',
      description: 'Learn core JavaScript concepts through interactive exercises',
      sessionType: 'teacher-student',
      status: 'active',
      curriculum: 'javascript-basics',
      startedAt: new Date().toISOString(),
      plannedDuration: 90,
      actualDuration: 15,
      hyperparameters: {
        learningRate: 0.15,
        scoreThreshold: 80.0,
        benchmarkInterval: 8,
        maxSessionLength: 120,
        adaptiveScoring: true,
        contextWindow: 25
      },
      learningObjectives: [
        {
          id: 'obj-variables',
          topic: 'variables-declarations',
          description: 'Understand var, let, and const declarations',
          targetScore: 85,
          currentScore: 78,
          completed: false,
          evidence: []
        },
        {
          id: 'obj-functions',
          topic: 'function-basics',
          description: 'Create and call functions effectively',
          targetScore: 80,
          completed: false,
          evidence: []
        }
      ],
      metrics: {
        messagesExchanged: 24,
        benchmarksPassed: 2,
        benchmarksFailed: 1,
        averageScore: 76.5,
        timeSpent: 15,
        objectivesCompleted: 0,
        scoreHistory: [
          {
            timestamp: new Date(Date.now() - 10 * 60 * 1000).toISOString(),
            score: 72,
            objective: 'variables-declarations'
          },
          {
            timestamp: new Date(Date.now() - 5 * 60 * 1000).toISOString(),
            score: 81,
            objective: 'function-basics'
          }
        ]
      },
      additionalParticipants: [],
      isArchived: false
    }
  ];
}

// ===== SEEDING FUNCTIONS =====

/**
 * Create a record via state/create command (user-scoped) with proper shell escaping
 */
async function createStateRecord(collection: string, data: any, id: string, userId?: string, displayName?: string): Promise<boolean> {
  const dataArg = JSON.stringify(data).replace(/'/g, `'\"'\"'`);
  const userIdArg = userId ? ` --userId=${userId}` : '';
  const cmd = `./jtag state/create --collection=${collection} --data='${dataArg}' --id=${id}${userIdArg}`;

  try {
    const result = await execAsync(cmd);
    const success = result.stdout.includes('\"success\": true');

    if (success) {
      console.log(`✅ Created ${collection} (state): ${displayName || id}${userId ? ` for user ${userId.slice(0, 8)}...` : ''}`);
      return true;
    } else {
      console.error(`❌ Failed to create ${collection} ${displayName || id}: Command returned unsuccessful result`);
      console.error(`Response: ${result.stdout}`);
      return false;
    }
  } catch (error: any) {
    const hasSuccess = error.stdout && error.stdout.includes('\"success\": true');

    if (hasSuccess) {
      console.log(`✅ Created ${collection} (state): ${displayName || id}${userId ? ` for user ${userId.slice(0, 8)}...` : ''}`);
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
 * Create a record via JTAG command with proper shell escaping
 */
async function createRecord(collection: string, data: any, id: string, displayName?: string, userId?: string): Promise<boolean> {
  const dataArg = JSON.stringify(data).replace(/'/g, `'"'"'`);
  const userIdArg = userId ? ` --userId=${userId}` : '';
  const cmd = `./jtag state/create --collection=${collection} --data='${dataArg}' --id=${id}${userIdArg}`;

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
async function seedRecords<T extends { id: string; displayName?: string }>(collection: string, records: T[], getDisplayName?: (record: T) => string, getUserId?: (record: T) => string): Promise<void> {
  console.log(`📝 Creating ${records.length} ${collection} records via state/create...`);

  let successCount = 0;
  for (const record of records) {
    const displayName = getDisplayName ? getDisplayName(record) : record.displayName || record.id;
    const userId = getUserId ? getUserId(record) : undefined;
    const success = await createRecord(collection, record, record.id, displayName, userId);
    if (success) successCount++;
  }

  console.log(`📊 Created ${successCount}/${records.length} ${collection} records`);

  if (successCount !== records.length) {
    throw new Error(`❌ Seeding failed for ${collection}: only ${successCount}/${records.length} records created successfully`);
  }
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
        "agent",
        "🤖",
        "AI assistant specialized in coding, architecture, and system design",
        "Anthropic Cloud"
      ),
      createUser(
        USER_IDS.GENERAL_AI,
        USER_CONFIG.GENERAL_AI.NAME,
        "General purpose assistant",
        "agent",
        "⚡",
        "General AI assistant for various tasks and conversations",
        "Anthropic Cloud"
      )
    ];

    // Create rooms using factory functions (using human user as owner)
    const rooms = [
      createRoom(
        ROOM_IDS.GENERAL,
        ROOM_CONFIG.GENERAL.NAME,
        ROOM_CONFIG.GENERAL.NAME,
        ROOM_CONFIG.GENERAL.DESCRIPTION,
        "Welcome to general discussion! Introduce yourself and chat about anything.",
        3,
        ["general", "welcome", "discussion"],
        USER_IDS.HUMAN
      ),
      createRoom(
        ROOM_IDS.ACADEMY,
        ROOM_CONFIG.ACADEMY.NAME,
        ROOM_CONFIG.ACADEMY.NAME,
        ROOM_CONFIG.ACADEMY.DESCRIPTION,
        "Share knowledge, tutorials, and collaborate on learning",
        2,
        ["academy", "learning", "education"],
        USER_IDS.HUMAN
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

    // Create content type registry and user states
    const contentTypes = createDefaultContentTypes();
    const userStates = createDefaultUserStates();
    const trainingSessions = createDefaultTrainingSessions();

    // Seed all data types using clean modular approach with user context
    await seedRecords(UserEntity.collection, users, (user) => user.displayName);
    await seedRecords(RoomEntity.collection, rooms, (room) => room.displayName, (room) => room.ownerId);
    await seedRecords(ChatMessageEntity.collection, messages,
      (msg) => msg.senderId === USER_IDS.HUMAN ? 'Joel' : msg.senderId === USER_IDS.CLAUDE_CODE ? 'Claude' : 'Unknown',
      (msg) => msg.senderId
    );
    await seedRecords(ContentTypeEntity.collection, contentTypes, (ct) => ct.displayName);
    // Use state commands for UserState seeding (user-scoped collection)
    console.log(`📝 Creating ${userStates.length} ${UserStateEntity.collection} records via state/create...`);
    let userStateSuccessCount = 0;
    for (const userState of userStates) {
      const displayName = `${userState.userId.slice(0,8)}...`;
      const success = await createStateRecord(
        UserStateEntity.collection,
        userState,
        userState.id,
        userState.userId,
        displayName
      );
      if (success) userStateSuccessCount++;
    }
    console.log(`📊 Created ${userStateSuccessCount}/${userStates.length} ${UserStateEntity.collection} records`);
    await seedRecords(TrainingSessionEntity.collection, trainingSessions, (ts) => ts.sessionName);

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