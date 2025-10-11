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
import { DATABASE_PATHS } from '../system/data/config/DatabaseConfig';
import { UserEntity } from '../system/data/entities/UserEntity';
import { RoomEntity } from '../system/data/entities/RoomEntity';
import { ChatMessageEntity } from '../system/data/entities/ChatMessageEntity';
import { UserStateEntity } from '../system/data/entities/UserStateEntity';
import { ContentTypeEntity } from '../system/data/entities/ContentTypeEntity';
import { TrainingSessionEntity } from '../system/data/entities/TrainingSessionEntity';
import type { UserCreateResult } from '../commands/user/create/shared/UserCreateTypes';

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
function createRoom(id: string, name: string, displayName: string, description: string, topic: string, memberCount: number, tags: string[], ownerId: string, uniqueId: string): any {
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
    },
    {
      id: 'us-anonymous-browser',
      userId: '9959e413-31b5-4915-ba0a-884dc3015f4b',  // Anonymous user from session
      deviceId: 'browser-anonymous',
      contentState: {
        openItems: [],
        lastUpdatedAt: new Date().toISOString()
      },
      preferences: {
        maxOpenTabs: 10,
        autoCloseAfterDays: 30,
        rememberScrollPosition: true,
        syncAcrossDevices: true,
        theme: 'base'  // Default theme for anonymous users
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
 * Create a record via data/create command (server-side, no browser required) with proper shell escaping
 */
async function createStateRecord(collection: string, data: any, id: string, userId?: string, displayName?: string): Promise<boolean> {
  const dataArg = JSON.stringify(data).replace(/'/g, `'\"'\"'`);
  const cmd = `./jtag data/create --collection=${collection} --data='${dataArg}'`;

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
  const cmd = `./jtag data/update --collection=users --id=${userId} --data='${dataArg}'`;

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
  const cmd = `./jtag data/update --collection=users --id=${userId} --data='${dataArg}'`;

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
 * Create a record via JTAG data/create command (server-side, no browser required) with proper shell escaping
 */
async function createRecord(collection: string, data: any, id: string, displayName?: string, userId?: string): Promise<boolean> {
  const dataArg = JSON.stringify(data).replace(/'/g, `'"'"'`);
  const cmd = `./jtag data/create --collection=${collection} --data='${dataArg}'`;

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
  console.log(`📝 Creating ${records.length} ${collection} records via data/create...`);

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
    const result = await execAsync(`./jtag data/list --collection=${collection} 2>&1 | head -10`);
    const count = result.stdout.match(/"count":\s*(\d+)/)?.[1] || '0';
    return count;
  } catch (error: any) {
    console.error(`   ⚠️ Error counting ${collection}: ${error.message}`);
    return '0';
  }
}

/**
 * Check if database is already seeded by looking for primary-human user
 * This enables fast-path: <1 second when data exists vs 60+ seconds for full seed
 */
async function checkIfSeeded(): Promise<boolean> {
  try {
    const result = await execAsync(`./jtag data/list --collection=${UserEntity.collection}`);
    const stdout = result.stdout;

    // Check if primary-human exists in the output
    if (stdout.includes(DEFAULT_USER_UNIQUE_IDS.PRIMARY_HUMAN)) {
      const count = stdout.match(/"count":\s*(\d+)/)?.[1];
      if (count && parseInt(count) >= 6) {
        console.log('✅ Database already seeded (found primary-human user and 6+ users)');
        return true;
      }
    }
    return false;
  } catch (error) {
    return false;
  }
}

/**
 * Main seeding function with idempotent behavior
 */
async function seedViaJTAG() {
  console.log('🌱 Seeding database via JTAG commands (single source of truth)...');

  try {
    // Fast path: Check if already seeded (reduces 60+ seconds to <1 second)
    if (await checkIfSeeded()) {
      console.log('⚡ Skipping seeding - database already populated');
      return;
    }

    // Clear existing database tables using proper JTAG commands (NO RAW SQL!)
    console.log('🧹 Clearing existing database tables...');
    try {
      await execAsync(`./jtag data/truncate --collection=users`);
      await execAsync(`./jtag data/truncate --collection=rooms`);
      await execAsync(`./jtag data/truncate --collection=chat_messages`);
      console.log('✅ Database tables cleared via JTAG commands');
    } catch (error: any) {
      console.log('⚠️ Error clearing tables:', error.message);
    }

    // Create users via user/create command (proper factory-based creation)
    console.log('📝 Creating users via user/create command...');

    const humanUser = await createUserViaCommand('human', USER_CONFIG.HUMAN.DISPLAY_NAME, DEFAULT_USER_UNIQUE_IDS.PRIMARY_HUMAN);
    const claudeUser = await createUserViaCommand('agent', USER_CONFIG.CLAUDE.NAME, DEFAULT_USER_UNIQUE_IDS.CLAUDE_CODE, 'anthropic');
    const generalAIUser = await createUserViaCommand('agent', USER_CONFIG.GENERAL_AI.NAME, DEFAULT_USER_UNIQUE_IDS.GENERAL_AI, 'anthropic');

    // Create persona users for testing autonomous chat
    const helperPersona = await createUserViaCommand('persona', 'Helper AI', 'persona-helper-001');
    const teacherPersona = await createUserViaCommand('persona', 'Teacher AI', 'persona-teacher-001');
    const codeReviewPersona = await createUserViaCommand('persona', 'CodeReview AI', 'persona-codereview-001');

    const users = [humanUser, claudeUser, generalAIUser, helperPersona, teacherPersona, codeReviewPersona].filter(u => u !== null) as UserEntity[];
    console.log(`📊 Created ${users.length}/6 users (1 human, 2 agents, 3 personas)`);

    if (users.length !== 6 || !humanUser || !claudeUser || !generalAIUser || !helperPersona || !teacherPersona || !codeReviewPersona) {
      throw new Error('❌ Failed to create all required users');
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

    // Create rooms using actual generated user entity
    const generalRoom = createRoom(
      ROOM_IDS.GENERAL,
      ROOM_CONFIG.GENERAL.NAME,
      ROOM_CONFIG.GENERAL.NAME,
      ROOM_CONFIG.GENERAL.DESCRIPTION,
      "Welcome to general discussion! Introduce yourself and chat about anything.",
      6,  // Updated member count: 3 personas + human + 2 agents
      ["general", "welcome", "discussion"],
      humanUser.id,
      'general'  // uniqueId - stable identifier
    );

    // Add personas to general room members (they need room membership to receive chat events)
    generalRoom.members = [
      { userId: helperPersona.id, role: 'member', joinedAt: new Date().toISOString() },
      { userId: teacherPersona.id, role: 'member', joinedAt: new Date().toISOString() },
      { userId: codeReviewPersona.id, role: 'member', joinedAt: new Date().toISOString() }
    ];

    const academyRoom = createRoom(
      ROOM_IDS.ACADEMY,
      ROOM_CONFIG.ACADEMY.NAME,
      ROOM_CONFIG.ACADEMY.NAME,
      ROOM_CONFIG.ACADEMY.DESCRIPTION,
      "Share knowledge, tutorials, and collaborate on learning",
      3,  // Updated member count: 3 personas
      ["academy", "learning", "education"],
      humanUser.id,
      'academy'  // uniqueId - stable identifier
    );

    // Add personas to academy room members (learning-focused personas)
    academyRoom.members = [
      { userId: teacherPersona.id, role: 'member', joinedAt: new Date().toISOString() },
      { userId: helperPersona.id, role: 'member', joinedAt: new Date().toISOString() },
      { userId: codeReviewPersona.id, role: 'member', joinedAt: new Date().toISOString() }
    ];

    const rooms = [
      generalRoom,
      academyRoom
    ];

    // Create messages using actual generated user entities
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
    await seedRecords(RoomEntity.collection, rooms, (room) => room.displayName, (room) => room.ownerId);
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