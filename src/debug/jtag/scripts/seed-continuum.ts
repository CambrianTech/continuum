#!/usr/bin/env tsx
/**
 * Clean Database Seeding via JTAG Commands
 *
 * Uses factory functions to eliminate repetition and create clean data structures
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import { USER_IDS, ROOM_IDS, MESSAGE_IDS, USER_CONFIG, ROOM_CONFIG, MESSAGE_CONTENT } from '../api/data-seed/SeedConstants';
import { DEFAULT_USER_UNIQUE_IDS } from '../system/data/domains/DefaultEntities';
import { stringToUUID } from '../system/core/types/CrossPlatformUUID';
import { DATABASE_PATHS } from '../system/data/config/DatabaseConfig';
import { UserEntity } from '../system/data/entities/UserEntity';
import { RoomEntity } from '../system/data/entities/RoomEntity';
import { ChatMessageEntity } from '../system/data/entities/ChatMessageEntity';
import { UserStateEntity } from '../system/data/entities/UserStateEntity';
import { ContentTypeEntity } from '../system/data/entities/ContentTypeEntity';
import { TrainingSessionEntity } from '../system/data/entities/TrainingSessionEntity';
import type { UserCreateResult } from '../commands/user/create/shared/UserCreateTypes';
import { SystemIdentity } from '../api/data-seed/SystemIdentity';
import { PERSONA_CONFIGS, PERSONA_UNIQUE_IDS } from './seed/personas';
import { DATA_COMMANDS } from '../commands/data/shared/DataCommandConstants';
import {
  createUserCapabilities,
  createRoom,
  createChatMessage,
  createDefaultContentTypes,
  createDefaultUserStates,
  createDefaultTrainingSessions
} from './seed/factories';
import {
  createRecord,
  createStateRecord,
  updatePersonaProfile,
  updatePersonaConfig,
  createUserViaCommand,
  loadUserByUniqueId,
  seedRecords
} from './seed/helpers';
import { isTestUser, isTestRoom, isTestMessage } from '../tests/shared/TestEntityConstants';

const execAsync = promisify(exec);

// ===== MOVED TO scripts/seed/factories.ts =====
// Factory functions extracted to eliminate repetition

/**
 * @deprecated - Moved to factories.ts, keeping for reference during migration
 */
function createUserCapabilities_OLD(type: 'human' | 'agent'): any {
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
function createRoom(id: string, name: string, displayName: string, description: string, topic: string, memberCount: number, tags: string[], ownerId: string, uniqueId: string, recipeId: string = 'general-chat'): any {
  return {
    id,
    uniqueId,  // REQUIRED field for RoomEntity validation
    name: name.toLowerCase(),
    displayName,
    description,
    topic,
    type: "public",
    status: "active",
    ownerId,
    lastMessageAt: new Date().toISOString(), // Set to current time for new rooms
    recipeId,  // Recipe for conversation governance
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
function createMessage(id: string, roomId: string, senderId: string, senderName: string, text: string, senderType: 'human' | 'agent' | 'persona' | 'system' = 'system'): any {
  return {
    id,
    roomId,
    senderId,
    senderName,
    senderType,  // REQUIRED field for ChatMessageEntity
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

// NOTE: createDefaultUserStates imported from factories.ts - uses UserCapabilitiesDefaults constants

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
 * Create a record via data/create command (server-side, no browser required) with proper shell escaping
 */
async function createStateRecord(collection: string, data: any, id: string, userId?: string, displayName?: string): Promise<boolean> {
  const dataArg = JSON.stringify(data).replace(/'/g, `'\"'\"'`);
  const cmd = `./jtag ${DATA_COMMANDS.CREATE} --collection=${collection} --data='${dataArg}'`;

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
 * Update persona bio via shortDescription field (profile is separate entity)
 */
async function updatePersonaProfile(userId: string, profile: { bio: string; speciality: string }): Promise<boolean> {
  const updateData = {
    shortDescription: profile.bio  // Use shortDescription which is on UserEntity directly
  };
  const dataArg = JSON.stringify(updateData).replace(/'/g, `'"'"'`);
  const cmd = `./jtag ${DATA_COMMANDS.UPDATE} --collection=users --id=${userId} --data='${dataArg}'`;

  try {
    const { stdout } = await execAsync(cmd);
    const result = JSON.parse(stdout);

    if (result.success) {
      console.log(`  ✅ Updated persona bio for user ${userId.slice(0, 8)}...`);
      return true;
    } else {
      console.error(`  ❌ Failed to update persona bio: ${result.error || 'Unknown error'}`);
      return false;
    }
  } catch (error: any) {
    console.error(`  ❌ Failed to update persona bio: ${error.message}`);
    return false;
  }
}

/**
 * Update persona configuration for intelligent resource management
 */
async function updatePersonaConfig(userId: string, config: any): Promise<boolean> {
  const configArg = JSON.stringify(config).replace(/'/g, `'"'"'`);
  const updateData = { personaConfig: config };
  const dataArg = JSON.stringify(updateData).replace(/'/g, `'"'"'`);
  const cmd = `./jtag ${DATA_COMMANDS.UPDATE} --collection=users --id=${userId} --data='${dataArg}'`;

  try {
    const { stdout } = await execAsync(cmd);
    const result = JSON.parse(stdout);

    if (result.success) {
      console.log(`  ✅ Updated persona config for user ${userId.slice(0, 8)}...`);
      return true;
    } else {
      console.error(`  ❌ Failed to update persona config: ${result.error || 'Unknown error'}`);
      return false;
    }
  } catch (error: any) {
    console.error(`  ❌ Failed to update persona config: ${error.message}`);
    return false;
  }
}

/**
 * Create a user via user/create command (proper factory-based creation)
 * Returns the UserEntity if successful, null otherwise
 */
async function createUserViaCommand(type: 'human' | 'agent' | 'persona', displayName: string, uniqueId?: string, provider?: string): Promise<UserEntity | null> {
  const uniqueIdArg = uniqueId ? ` --uniqueId=${uniqueId}` : '';
  const providerArg = provider ? ` --provider=${provider}` : '';
  const cmd = `./jtag user/create --type=${type} --displayName="${displayName}"${uniqueIdArg}${providerArg}`;

  try {
    const { stdout } = await execAsync(cmd);
    const response: UserCreateResult = JSON.parse(stdout);

    if (response.success && response.user) {
      console.log(`✅ Created user (${type}): ${displayName} (uniqueId: ${uniqueId || 'none'}, ID: ${response.user.id.slice(0, 8)}...)`);
      return response.user;
    } else {
      console.error(`❌ Failed to create user ${displayName}: ${response.error || 'Unknown error'}`);
      return null;
    }
  } catch (error: any) {
    // exec throws on non-zero exit, but may still have valid output
    if (error.stdout) {
      try {
        const response: UserCreateResult = JSON.parse(error.stdout);
        if (response.success && response.user) {
          console.log(`✅ Created user (${type}): ${displayName} (uniqueId: ${uniqueId || 'none'}, ID: ${response.user.id.slice(0, 8)}...)`);
          return response.user;
        }
      } catch (parseError) {
        // Fall through to error handling
      }
    }

    console.error(`❌ Failed to create user ${displayName}: ${error.message}`);
    if (error.stdout) console.error(`   Output: ${error.stdout.substring(0, 500)}`);
    if (error.stderr) console.error(`   Stderr: ${error.stderr.substring(0, 500)}`);
    return null;
  }
}

/**
 * Load an existing user by uniqueId using JTAG ${DATA_COMMANDS.LIST} command
 */
async function loadUserByUniqueId(uniqueId: string): Promise<UserEntity | null> {
  try {
    const { stdout } = await execAsync(`./jtag ${DATA_COMMANDS.LIST} --collection=${UserEntity.collection} --filter='{"uniqueId":"${uniqueId}"}'`);
    const response = JSON.parse(stdout);

    if (response.success && response.items && response.items.length > 0) {
      const user = response.items[0];
      console.log(`✅ Loaded existing user: ${user.displayName} (uniqueId: ${uniqueId}, ID: ${user.id.slice(0, 8)}...)`);
      return user;
    } else {
      console.log(`⚠️ User with uniqueId ${uniqueId} not found in database`);
      return null;
    }
  } catch (error: any) {
    console.error(`❌ Failed to load user with uniqueId ${uniqueId}: ${error.message}`);
    if (error.stdout) console.error(`   Output: ${error.stdout.substring(0, 500)}`);
    return null;
  }
}

/**
 * Create a record via JTAG data/create command (server-side, no browser required) with proper shell escaping
 */
async function createRecord(collection: string, data: any, id: string, displayName?: string, userId?: string): Promise<boolean> {
  const dataArg = JSON.stringify(data).replace(/'/g, `'"'"'`);
  const cmd = `./jtag ${DATA_COMMANDS.CREATE} --collection=${collection} --data='${dataArg}'`;

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
  console.log(`📝 Creating ${records.length} ${collection} records via ${DATA_COMMANDS.CREATE}...`);

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
 * Get count from JTAG list command (using head to get just the JSON header)
 */
async function getEntityCount(collection: string): Promise<string> {
  try {
    // Use head to get first 10 lines which includes the count field
    const result = await execAsync(`./jtag ${DATA_COMMANDS.LIST} --collection=${collection} 2>&1 | head -10`);
    const count = result.stdout.match(/"count":\s*(\d+)/)?.[1] || '0';
    return count;
  } catch (error: any) {
    console.error(`   ⚠️ Error counting ${collection}: ${error.message}`);
    return '0';
  }
}

/**
 * Check which users exist by uniqueId
 * Returns array of missing user uniqueIds that need to be created
 */
async function getMissingUsers(): Promise<string[]> {
  // Build required users list from PERSONA_CONFIGS (single source of truth)
  const requiredUsers = [
    DEFAULT_USER_UNIQUE_IDS.PRIMARY_HUMAN,
    ...PERSONA_CONFIGS.map(p => p.uniqueId)
  ];

  try {
    const result = await execAsync(`./jtag ${DATA_COMMANDS.LIST} --collection=${UserEntity.collection}`);
    const stdout = result.stdout;

    const missingUsers = requiredUsers.filter(uniqueId => !stdout.includes(uniqueId));

    if (missingUsers.length === 0) {
      console.log(`✅ All ${requiredUsers.length} required users exist`);
    } else {
      console.log(`📋 Found ${requiredUsers.length - missingUsers.length}/${requiredUsers.length} users, missing: ${missingUsers.join(', ')}`);
    }

    return missingUsers;
  } catch (error) {
    console.log('⚠️ Could not check existing users, will attempt full seed');
    return requiredUsers;
  }
}

/**
 * Wait for JTAG system to be fully ready with commands registered
 */
async function waitForJTAGReady(maxWaitSeconds: number = 180): Promise<boolean> {
  const startTime = Date.now();
  let attempts = 0;

  console.log('⏳ Waiting for JTAG system to be ready...');

  while (Date.now() - startTime < maxWaitSeconds * 1000) {
    try {
      const { stdout } = await execAsync('./jtag ping');

      // ROBUST: Extract JSON from potentially polluted output (same as SystemMetricsCollector)
      const firstBrace = stdout.indexOf('{');
      const lastBrace = stdout.lastIndexOf('}');

      if (firstBrace === -1 || lastBrace === -1 || firstBrace > lastBrace) {
        throw new Error('No valid JSON in ping output');
      }

      const jsonStr = stdout.substring(firstBrace, lastBrace + 1);
      const response = JSON.parse(jsonStr);

      if (response.success &&
          response.server?.health?.systemReady &&
          response.server?.health?.commandsRegistered > 0) {
        console.log(`✅ JTAG ready with ${response.server.health.commandsRegistered} commands registered`);
        return true;
      }

      // Log progress every 5 attempts
      if (attempts % 5 === 0 && attempts > 0) {
        const elapsed = Math.round((Date.now() - startTime) / 1000);
        console.log(`   Still waiting... (${elapsed}s elapsed, commands: ${response.server?.health?.commandsRegistered || 0})`);
      }
    } catch (error) {
      // Server not ready yet, will retry
    }

    attempts++;
    const waitMs = Math.min(500 * Math.pow(1.2, attempts), 2000); // Exponential backoff, max 2s
    await new Promise(resolve => setTimeout(resolve, waitMs));
  }

  console.error(`❌ JTAG system did not become ready after ${maxWaitSeconds} seconds`);
  return false;
}

/**
 * Clean up test entities left over from failed integration tests
 * Runs automatically on npm start to prevent test pollution
 */
async function cleanupTestEntities(): Promise<void> {
  console.log('🧹 Cleaning up test entities from failed integration tests...');

  try {
    // Use the standalone cleanup script instead of duplicating logic
    await execAsync('npx tsx scripts/cleanup-test-entities.ts');
  } catch (error) {
    // Non-fatal - just log and continue with seeding
    const errorMsg = error instanceof Error ? error.message : String(error);
    console.warn(`⚠️  Test entity cleanup failed (non-fatal): ${errorMsg}`);
    console.warn(`   You can manually run: npx tsx scripts/cleanup-test-entities.ts`);
  }
}

/**
 * Main seeding function with idempotent behavior
 */
async function seedViaJTAG() {
  console.log('🌱 Seeding database via JTAG commands (single source of truth)...');

  try {
    // CRITICAL: Wait for JTAG system to be ready before attempting any commands
    const isReady = await waitForJTAGReady();
    if (!isReady) {
      throw new Error('❌ JTAG system not ready - commands not registered yet');
    }

    // NOTE: Test cleanup disabled during startup to avoid deadlock
    // The cleanup script tries to connect to the server (jtag.connect()) which hangs
    // during startup. Run manually if needed: npx tsx scripts/cleanup-test-entities.ts
    // await cleanupTestEntities();

    // Check which users are missing
    const missingUsers = await getMissingUsers();

    if (missingUsers.length === 0) {
      console.log('⚡ All required users exist - no seeding needed');
      return;
    }

    // Create human user FIRST (needed as room owner), then rooms, then other users
    console.log(`📝 Creating human user first (needed as room owner)...`);

    // Get system identity (HOME directory-based) - server-only, keep it here!
    const systemIdentity = SystemIdentity.getIdentity();
    console.log(`🔧 Using system identity: ${systemIdentity.displayName} (${systemIdentity.username})`);

    const userMap: Record<string, UserEntity | null> = {};

    // Step 1: Create human user first (or use existing)
    let humanUser: UserEntity | null = null;

    if (missingUsers.includes(DEFAULT_USER_UNIQUE_IDS.PRIMARY_HUMAN)) {
      // Create new human user with dynamic name from system identity
      humanUser = await createUserViaCommand('human', systemIdentity.displayName, DEFAULT_USER_UNIQUE_IDS.PRIMARY_HUMAN);
      if (!humanUser) {
        throw new Error('❌ Failed to create human user - required as room owner');
      }
      console.log(`✅ Created human user: ${humanUser.displayName}`);
    } else {
      // Human user already exists - load from database using uniqueId
      humanUser = await loadUserByUniqueId(DEFAULT_USER_UNIQUE_IDS.PRIMARY_HUMAN);
      if (!humanUser) {
        throw new Error('❌ Failed to load existing human user - database inconsistency');
      }
    }

    userMap['humanUser'] = humanUser;

    // Step 2: Check if rooms exist (create if missing)
    const { stdout: roomsOutput } = await execAsync(`./jtag data/list --collection=rooms --limit=1`);
    const roomsResult = JSON.parse(roomsOutput);
    const needsRooms = !roomsResult.items || roomsResult.items.length === 0;

    if (needsRooms) {
      // Create and persist rooms BEFORE creating other users
      console.log('🏗️ Creating rooms before other users (for auto-join to work)...');

      const generalRoom = createRoom(
        ROOM_IDS.GENERAL,
        ROOM_CONFIG.GENERAL.NAME,
        ROOM_CONFIG.GENERAL.NAME,
        ROOM_CONFIG.GENERAL.DESCRIPTION,
        "Welcome to general discussion! Introduce yourself and chat about anything.",
        0,  // Will be auto-populated by RoomMembershipDaemon
        ["general", "welcome", "discussion"],
        humanUser.id,
        'general'
      );
      // NO hardcoded members - let RoomMembershipDaemon handle it

      const academyRoom = createRoom(
        ROOM_IDS.ACADEMY,
        ROOM_CONFIG.ACADEMY.NAME,
        ROOM_CONFIG.ACADEMY.NAME,
        ROOM_CONFIG.ACADEMY.DESCRIPTION,
        "Share knowledge, tutorials, and collaborate on learning",
        0,  // Will be auto-populated by RoomMembershipDaemon
        ["academy", "learning", "education"],
        humanUser.id,
        'academy'
      );
      // NO hardcoded members - let RoomMembershipDaemon handle it

      const pantheonRoom = createRoom(
        ROOM_IDS.PANTHEON,
        'pantheon',
        'Pantheon',
        'Elite discussion room for top-tier SOTA AI models',
        "Advanced reasoning and multi-model collaboration",
        0,  // Will be auto-populated by RoomMembershipDaemon
        ["sota", "elite", "reasoning"],
        humanUser.id,
        'pantheon'
      );
      // NO hardcoded members - let RoomMembershipDaemon handle it

      const devUpdatesRoom = createRoom(
        ROOM_IDS.DEV_UPDATES,
        'dev-updates',
        'Dev Updates',
        'GitHub PRs, CI/CD, and development activity notifications',
        "Real-time development feed - where the team learns together",
        0,  // Will be auto-populated by RoomMembershipDaemon
        ["github", "ci", "development", "training"],
        humanUser.id,
        'dev-updates'
      );
      // NO hardcoded members - let RoomMembershipDaemon handle it

      const helpRoom = createRoom(
        ROOM_IDS.HELP,
        'help',
        'Help',
        'Get help from AI assistants - ask anything about using Continuum',
        "Your AI helpers are here to assist you getting started",
        0,  // Will be auto-populated by RoomMembershipDaemon
        ["help", "support", "onboarding", "getting-started", "system"],  // 'system' tag = hidden from rooms list
        humanUser.id,
        'help'  // recipe: help-focused room with Helper AI
      );
      // NO hardcoded members - let RoomMembershipDaemon handle it

      const settingsRoom = createRoom(
        ROOM_IDS.SETTINGS,
        'settings',
        'Settings',
        'Configure your Continuum experience with AI assistance',
        "Get help configuring API keys, preferences, and system settings",
        0,  // Will be auto-populated by RoomMembershipDaemon
        ["settings", "config", "preferences", "system"],  // 'system' tag = hidden from rooms list
        humanUser.id,
        'settings'  // recipe: settings-focused room with Helper AI
      );
      // NO hardcoded members - let RoomMembershipDaemon handle it

      const themeRoom = createRoom(
        ROOM_IDS.THEME,
        'theme',
        'Theme',
        'Design and customize your visual experience with AI assistance',
        "Get help designing themes, choosing colors, and customizing your workspace appearance",
        0,  // Will be auto-populated by RoomMembershipDaemon
        ["theme", "design", "customization", "appearance", "system"],  // 'system' tag = hidden from rooms list
        humanUser.id,
        'theme'  // recipe: theme-focused room with Helper AI
      );
      // NO hardcoded members - let RoomMembershipDaemon handle it

      const rooms = [generalRoom, academyRoom, pantheonRoom, devUpdatesRoom, helpRoom, settingsRoom, themeRoom];

      // Persist rooms to database BEFORE creating other users
      await seedRecords(RoomEntity.collection, rooms, (room) => room.displayName, (room) => room.ownerId);
      console.log('✅ Rooms created and persisted - ready for auto-join');
    }

    // Step 3: Now create all other users (auto-join will work because rooms exist)
    console.log(`📝 Creating remaining ${missingUsers.length - 1} users (auto-join will trigger)...`);

    // Create all personas using config-driven loop (eliminates repetition)
    for (const persona of PERSONA_CONFIGS) {
      if (missingUsers.includes(persona.uniqueId)) {
        // Only create Sentinel if SENTINEL_PATH is configured
        if (persona.provider === 'sentinel') {
          if (!process.env.SENTINEL_PATH) {
            console.log(`⏭️  Skipping Sentinel (SENTINEL_PATH not configured)`);
            continue;
          }
        }

        const user = await createUserViaCommand(persona.type, persona.displayName, persona.uniqueId, persona.provider);
        if (user) {
          userMap[persona.uniqueId] = user;
        }
      } else {
        // User already exists - load from database using uniqueId
        const existingUser = await loadUserByUniqueId(persona.uniqueId);
        if (existingUser) {
          userMap[persona.uniqueId] = existingUser;
        }
      }
    }

    // Count only newly created users (users that were in missingUsers list)
    const newUsersCreated = Object.values(userMap).filter((u, index, arr) => {
      // Count only users that were successfully created (not null)
      // Exclude human user if it was loaded (not in missingUsers)
      const isHumanUser = u === humanUser;
      const humanWasCreated = missingUsers.includes(DEFAULT_USER_UNIQUE_IDS.PRIMARY_HUMAN);

      if (isHumanUser && !humanWasCreated) {
        return false;  // Don't count loaded human user
      }

      return u !== null;  // Count all other successfully created users
    }).length;
    console.log(`📊 Created ${newUsersCreated}/${missingUsers.length} users (auto-join handled by RoomMembershipDaemon)`);

    // Get references to created users for message seeding (using uniqueIds as keys)
    const claudeUser = userMap[DEFAULT_USER_UNIQUE_IDS.CLAUDE_CODE];
    // Use constants from PERSONA_UNIQUE_IDS (single source of truth, no magic strings)
    const helperPersona = userMap[PERSONA_UNIQUE_IDS.HELPER];
    const teacherPersona = userMap[PERSONA_UNIQUE_IDS.TEACHER];
    const codeReviewPersona = userMap[PERSONA_UNIQUE_IDS.CODE_REVIEW];

    // If rooms already existed, ensure system rooms have Helper AI then exit
    if (!needsRooms) {
      // Still ensure system rooms have their default AI assistant
      console.log('🏠 Ensuring system rooms have Helper AI...');
      const systemRoomUniqueIds = ['settings', 'help', 'theme'];
      for (const roomUniqueId of systemRoomUniqueIds) {
        try {
          const result = await execAsync(`./jtag data/list --collection=rooms --filter='{"uniqueId":"${roomUniqueId}"}'`);
          const parsed = JSON.parse(result.stdout);
          if (parsed.success && parsed.items?.[0]) {
            const room = parsed.items[0];
            const existingMembers = room.members || [];
            const helperAlreadyMember = existingMembers.some((m: any) => m.userId === helperPersona?.id);

            if (helperPersona && !helperAlreadyMember) {
              const updatedMembers = [
                ...existingMembers,
                { userId: helperPersona.id, role: 'member', joinedAt: '2025-01-01T00:00:00Z' }
              ];
              const updateData = JSON.stringify({ members: updatedMembers }).replace(/'/g, `'\"'\"'`);
              await execAsync(`./jtag data/update --collection=rooms --id="${room.id}" --data='${updateData}'`);
              console.log(`✅ Added Helper AI to ${roomUniqueId} room`);
            }
          }
        } catch (error) {
          // Silently skip - rooms might not exist yet
        }
      }
      console.log('✅ Users added to existing database - rooms and messages already exist');
      return;
    }

    if (!humanUser || !claudeUser || !helperPersona || !teacherPersona || !codeReviewPersona) {
      throw new Error('❌ Failed to create core required users');
    }

    // Update persona profiles with distinct personalities
    console.log('🎭 Updating persona profiles with distinct personalities...');
    await Promise.all([
      updatePersonaProfile(helperPersona.id, {
        bio: 'A friendly, concise assistant who provides quick practical help and actionable solutions',
        speciality: 'practical-assistance'
      }),
      updatePersonaProfile(teacherPersona.id, {
        bio: 'An educational mentor who explains concepts thoroughly with examples and patient guidance',
        speciality: 'education-mentoring'
      }),
      updatePersonaProfile(codeReviewPersona.id, {
        bio: 'A critical analyst who evaluates code quality, security, and best practices with constructive feedback',
        speciality: 'code-analysis'
      })
    ]);
    console.log('✅ Persona profiles updated with personalities');

    // Ensure system rooms have Helper AI as default assistant
    // This ensures the Settings, Help, and Theme widgets always have AI available
    console.log('🏠 Adding Helper AI to system rooms...');
    const systemRoomUniqueIds = ['settings', 'help', 'theme'];
    for (const roomUniqueId of systemRoomUniqueIds) {
      try {
        const result = await execAsync(`./jtag data/list --collection=rooms --filter='{"uniqueId":"${roomUniqueId}"}'`);
        const parsed = JSON.parse(result.stdout);
        if (parsed.success && parsed.items?.[0]) {
          const room = parsed.items[0];
          const existingMembers = room.members || [];
          const helperAlreadyMember = existingMembers.some((m: any) => m.userId === helperPersona.id);

          if (!helperAlreadyMember) {
            const updatedMembers = [
              ...existingMembers,
              { userId: helperPersona.id, role: 'member', joinedAt: '2025-01-01T00:00:00Z' }
            ];
            const updateData = JSON.stringify({ members: updatedMembers }).replace(/'/g, `'\"'\"'`);
            await execAsync(`./jtag data/update --collection=rooms --id="${room.id}" --data='${updateData}'`);
            console.log(`✅ Added Helper AI to ${roomUniqueId} room`);
          } else {
            console.log(`✅ Helper AI already in ${roomUniqueId} room`);
          }
        }
      } catch (error) {
        console.warn(`⚠️ Could not add Helper AI to ${roomUniqueId}:`, error);
      }
    }

    // Configure persona AI response settings (intelligent resource management)
    console.log('🔧 Configuring persona AI response settings...');
    await Promise.all([
      updatePersonaConfig(helperPersona.id, {
        domainKeywords: ['help', 'question', 'what', 'how', 'why', 'explain', 'support', 'assist'],
        responseThreshold: 50,
        alwaysRespondToMentions: true,
        cooldownSeconds: 30,
        maxResponsesPerSession: 50,
        gatingModel: 'deterministic',
        responseModel: 'llama3.2:3b'
      }),
      updatePersonaConfig(teacherPersona.id, {
        domainKeywords: ['teaching', 'education', 'learning', 'explain', 'understand', 'lesson', 'tutorial', 'guide'],
        responseThreshold: 50,
        alwaysRespondToMentions: true,
        cooldownSeconds: 30,
        maxResponsesPerSession: 50,
        gatingModel: 'deterministic',
        responseModel: 'llama3.2:3b'
      }),
      updatePersonaConfig(codeReviewPersona.id, {
        domainKeywords: ['code', 'programming', 'function', 'bug', 'typescript', 'javascript', 'review', 'refactor'],
        responseThreshold: 50,
        alwaysRespondToMentions: true,
        cooldownSeconds: 30,
        maxResponsesPerSession: 50,
        gatingModel: 'deterministic',
        responseModel: 'llama3.2:3b'
      })
    ]);
    console.log('✅ Persona configurations applied');

    // Rooms already created and persisted earlier (before other users)
    // Now create messages for those rooms
    // Use systemIdentity from top of function - don't recreate it
    const messages = [
      createMessage(
        MESSAGE_IDS.WELCOME_GENERAL,
        ROOM_IDS.GENERAL,
        'system',
        'System',
        MESSAGE_CONTENT.WELCOME_GENERAL,
        'system'  // senderType
      ),
      createMessage(
        MESSAGE_IDS.CLAUDE_INTRO,
        ROOM_IDS.GENERAL,
        claudeUser.id,
        USER_CONFIG.CLAUDE.NAME,
        MESSAGE_CONTENT.CLAUDE_INTRO,
        'agent'  // senderType - Claude Code is an agent
      ),
      createMessage(
        MESSAGE_IDS.WELCOME_ACADEMY,
        ROOM_IDS.ACADEMY,
        'system',
        'System',
        MESSAGE_CONTENT.WELCOME_ACADEMY,
        'system'  // senderType
      ),
      createMessage(
        stringToUUID('pantheon-welcome-msg'),
        ROOM_IDS.PANTHEON,
        humanUser.id,
        systemIdentity.displayName,
        'Welcome to the Pantheon! This is where our most advanced SOTA models converge - each provider\'s flagship intelligence collaborating on complex problems.',
        'human'  // senderType
      )
    ];

    // Create content type registry
    const contentTypes = createDefaultContentTypes();

    // Create training sessions with actual generated user entities
    const trainingSessions = [
      {
        id: 'ts-js-fundamentals',
        roomId: ROOM_IDS.ACADEMY,
        teacherUserId: claudeUser.id,
        studentUserId: humanUser.id,
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

    // Seed all data types using clean modular approach with user context
    // Note: User states are created automatically by user/create command
    // Note: Rooms already seeded earlier (before other users, to enable auto-join)
    await seedRecords(ChatMessageEntity.collection, messages,
      (msg) => msg.senderId === humanUser.id ? humanUser.displayName : msg.senderId === claudeUser.id ? claudeUser.displayName : 'System',
      (msg) => msg.senderId
    );
    await seedRecords(ContentTypeEntity.collection, contentTypes, (ct) => ct.displayName);
    await seedRecords(TrainingSessionEntity.collection, trainingSessions, (ts) => ts.sessionName);

    // Note: Verification skipped due to buffer overflow issues with large collections
    // Data commands confirmed successful above - verification would require implementing
    // a count-only query option in data/list command
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