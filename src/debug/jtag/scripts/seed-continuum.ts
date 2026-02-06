#!/usr/bin/env tsx
/**
 * Clean Database Seeding via JTAG Commands
 *
 * Performance-optimized: bulk loads, parallel updates, no redundant subprocess spawns.
 * Uses factory functions from ./seed/factories and helper functions from ./seed/helpers.
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import { ROOM_IDS, MESSAGE_IDS, ROOM_CONFIG, MESSAGE_CONTENT } from '../api/data-seed/SeedConstants';
import { DEFAULT_USER_UNIQUE_IDS } from '../system/data/domains/DefaultEntities';
import { stringToUUID } from '../system/core/types/CrossPlatformUUID';
import { UserEntity } from '../system/data/entities/UserEntity';
import { RoomEntity } from '../system/data/entities/RoomEntity';
import { ChatMessageEntity } from '../system/data/entities/ChatMessageEntity';
import { ContentTypeEntity } from '../system/data/entities/ContentTypeEntity';
import { TrainingSessionEntity } from '../system/data/entities/TrainingSessionEntity';
import { SystemIdentity } from '../api/data-seed/SystemIdentity';
import { PERSONA_CONFIGS, PERSONA_UNIQUE_IDS } from './seed/personas';
import { DATA_COMMANDS } from '../commands/data/shared/DataCommandConstants';
import {
  createRoom,
  createDefaultContentTypes,
} from './seed/factories';
import {
  createRecord,
  updatePersonaProfile,
  updatePersonaConfig,
  updateUserMetadata,
  updateUserModelConfig,
  createUserViaCommand,
  seedRecords,
} from './seed/helpers';

const execAsync = promisify(exec);

// ===== LOCAL HELPERS (not in ./seed/helpers or ./seed/factories) =====

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

function createMessage(id: string, roomId: string, senderId: string, senderName: string, text: string, senderType: 'human' | 'agent' | 'persona' | 'system' = 'system'): any {
  return {
    id,
    roomId,
    senderId,
    senderName,
    senderType,
    content: createMessageContent(text),
    status: "sent",
    priority: "normal",
    timestamp: new Date().toISOString(),
    reactions: []
  };
}

// ===== BULK LOADING =====

/**
 * Load ALL users in one bulk call and parse into a map.
 * Returns both the user map (keyed by uniqueId) and the list of missing uniqueIds.
 *
 * This replaces getMissingUsers() + N individual loadUserByUniqueId() calls
 * with a SINGLE subprocess spawn.
 */
async function loadAllUsers(): Promise<{
  usersByUniqueId: Map<string, UserEntity>;
  missingUniqueIds: string[];
}> {
  const requiredUsers = [
    DEFAULT_USER_UNIQUE_IDS.PRIMARY_HUMAN,
    ...PERSONA_CONFIGS.map(p => p.uniqueId)
  ];

  const usersByUniqueId = new Map<string, UserEntity>();

  try {
    const { stdout } = await execAsync(`./jtag ${DATA_COMMANDS.LIST} --collection=${UserEntity.collection}`);
    const response = JSON.parse(stdout);

    if (response.success && response.items) {
      for (const user of response.items) {
        if (user.uniqueId) {
          usersByUniqueId.set(user.uniqueId, user);
        }
      }
    }

    const missingUniqueIds = requiredUsers.filter(uid => !usersByUniqueId.has(uid));

    if (missingUniqueIds.length === 0) {
      console.log(`✅ All ${requiredUsers.length} required users exist`);
    } else {
      console.log(`📋 Found ${requiredUsers.length - missingUniqueIds.length}/${requiredUsers.length} users, missing: ${missingUniqueIds.join(', ')}`);
    }

    return { usersByUniqueId, missingUniqueIds };
  } catch (error) {
    console.log('⚠️ Could not check existing users, will attempt full seed');
    return { usersByUniqueId, missingUniqueIds: requiredUsers };
  }
}

/**
 * Load ALL rooms in one bulk call and return as array + uniqueId set.
 */
async function loadAllRooms(): Promise<{
  rooms: any[];
  uniqueIds: Set<string>;
}> {
  try {
    const { stdout } = await execAsync(`./jtag ${DATA_COMMANDS.LIST} --collection=${RoomEntity.collection}`);
    const response = JSON.parse(stdout);
    const rooms = response.success && response.items ? response.items : [];
    const uniqueIds = new Set<string>(rooms.map((r: any) => r.uniqueId));
    return { rooms, uniqueIds };
  } catch (error) {
    return { rooms: [], uniqueIds: new Set() };
  }
}

// ===== SYSTEM READINESS =====

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

      // ROBUST: Extract JSON from potentially polluted output
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

      if (attempts % 5 === 0 && attempts > 0) {
        const elapsed = Math.round((Date.now() - startTime) / 1000);
        console.log(`   Still waiting... (${elapsed}s elapsed, commands: ${response.server?.health?.commandsRegistered || 0})`);
      }
    } catch (error) {
      // Server not ready yet, will retry
    }

    attempts++;
    const waitMs = Math.min(500 * Math.pow(1.2, attempts), 2000);
    await new Promise(resolve => setTimeout(resolve, waitMs));
  }

  console.error(`❌ JTAG system did not become ready after ${maxWaitSeconds} seconds`);
  return false;
}

// ===== ROOM DEFINITIONS =====

const ALL_EXPECTED_ROOMS = [
  { uniqueId: 'general', name: 'general', displayName: 'General', description: 'Main discussion room for all users', topic: 'General chat and collaboration', tags: ['general', 'welcome', 'discussion'], recipeId: 'general-chat' },
  { uniqueId: 'academy', name: 'academy', displayName: 'Academy', description: 'Learning and educational discussions', topic: 'Share knowledge, tutorials, and collaborate on learning', tags: ['academy', 'learning', 'education'], recipeId: 'academy' },
  { uniqueId: 'pantheon', name: 'pantheon', displayName: 'Pantheon', description: 'Elite discussion room for top-tier SOTA AI models', topic: 'Advanced reasoning and multi-model collaboration', tags: ['sota', 'elite', 'reasoning'], recipeId: 'pantheon' },
  { uniqueId: 'dev-updates', name: 'dev-updates', displayName: 'Dev Updates', description: 'GitHub PRs, CI/CD, and development activity notifications', topic: 'Real-time development feed', tags: ['github', 'ci', 'development'], recipeId: 'dev-updates' },
  { uniqueId: 'help', name: 'help', displayName: 'Help', description: 'Get help from AI assistants', topic: 'Your AI helpers are here to assist you', tags: ['help', 'support', 'system'], recipeId: 'help' },
  { uniqueId: 'settings', name: 'settings', displayName: 'Settings', description: 'Configure your Continuum experience', topic: 'System settings and configuration', tags: ['settings', 'config', 'system'], recipeId: 'settings' },
  { uniqueId: 'theme', name: 'theme', displayName: 'Theme', description: 'Design and customize your visual experience', topic: 'Themes, colors, and customization', tags: ['theme', 'design', 'system'], recipeId: 'theme' },
  { uniqueId: 'canvas', name: 'canvas', displayName: 'Canvas', description: 'Collaborative drawing discussions', topic: 'Art, drawing, and creative collaboration', tags: ['canvas', 'art', 'system'], recipeId: 'canvas' },
  { uniqueId: 'outreach', name: 'outreach', displayName: 'Outreach', description: 'Social media strategy, community building, and external engagement', topic: 'Discuss what to post, share interesting finds, coordinate outreach', tags: ['social', 'outreach', 'community', 'moltbook'], recipeId: 'outreach' },
  { uniqueId: 'newsroom', name: 'newsroom', displayName: 'Newsroom', description: 'Current events, breaking news, and world awareness', topic: 'Share and discuss current events', tags: ['news', 'current-events', 'awareness'], recipeId: 'newsroom' },
  { uniqueId: 'code', name: 'code', displayName: 'Code', description: 'Collaborative coding — reading, writing, reviewing, and shipping code as a team', topic: 'Software development with real tools and real agent loops', tags: ['coding', 'development', 'engineering'], recipeId: 'coding' },
] as const;

const SYSTEM_ROOM_UNIQUE_IDS = ['settings', 'help', 'theme', 'canvas'] as const;

// ===== MAIN SEEDING =====

/**
 * Main seeding function with idempotent behavior.
 *
 * Performance: uses bulk loads and parallel updates to minimize subprocess spawns.
 * Common case (all users exist): ~2 subprocess calls total (ping + bulk list).
 * Partial case (some users missing): creates missing users sequentially,
 * updates existing users in parallel.
 */
async function seedViaJTAG() {
  console.log('🌱 Seeding database via JTAG commands (single source of truth)...');

  try {
    // Wait for JTAG system to be ready
    const isReady = await waitForJTAGReady();
    if (!isReady) {
      throw new Error('❌ JTAG system not ready - commands not registered yet');
    }

    // BULK LOAD: One subprocess call replaces N individual lookups
    const { usersByUniqueId, missingUniqueIds } = await loadAllUsers();

    if (missingUniqueIds.length === 0) {
      console.log('⚡ All required users exist - no seeding needed');
      return;
    }

    // Get system identity
    const systemIdentity = SystemIdentity.getIdentity();
    console.log(`🔧 Using system identity: ${systemIdentity.displayName} (${systemIdentity.username})`);

    // Step 1: Ensure human user exists (needed as room owner)
    let humanUser = usersByUniqueId.get(DEFAULT_USER_UNIQUE_IDS.PRIMARY_HUMAN) ?? null;

    if (!humanUser) {
      console.log('📝 Creating human user first (needed as room owner)...');
      humanUser = await createUserViaCommand('human', systemIdentity.displayName, DEFAULT_USER_UNIQUE_IDS.PRIMARY_HUMAN);
      if (!humanUser) {
        throw new Error('❌ Failed to create human user - required as room owner');
      }
      usersByUniqueId.set(DEFAULT_USER_UNIQUE_IDS.PRIMARY_HUMAN, humanUser);
    }

    // Step 2: Check if rooms exist
    const { rooms: existingRooms, uniqueIds: existingRoomUniqueIds } = await loadAllRooms();
    const needsRooms = existingRooms.length === 0;

    if (needsRooms) {
      console.log('🏗️ Creating rooms before other users (for auto-join to work)...');

      const rooms = [
        createRoom(ROOM_IDS.GENERAL, ROOM_CONFIG.GENERAL.NAME, ROOM_CONFIG.GENERAL.NAME, ROOM_CONFIG.GENERAL.DESCRIPTION,
          "Welcome to general discussion! Introduce yourself and chat about anything.", 0,
          ["general", "welcome", "discussion"], humanUser.id, 'general'),
        createRoom(ROOM_IDS.ACADEMY, ROOM_CONFIG.ACADEMY.NAME, ROOM_CONFIG.ACADEMY.NAME, ROOM_CONFIG.ACADEMY.DESCRIPTION,
          "Share knowledge, tutorials, and collaborate on learning", 0,
          ["academy", "learning", "education"], humanUser.id, 'academy'),
        createRoom(ROOM_IDS.PANTHEON, 'pantheon', 'Pantheon', 'Elite discussion room for top-tier SOTA AI models',
          "Advanced reasoning and multi-model collaboration", 0,
          ["sota", "elite", "reasoning"], humanUser.id, 'pantheon'),
        createRoom(ROOM_IDS.DEV_UPDATES, 'dev-updates', 'Dev Updates', 'GitHub PRs, CI/CD, and development activity notifications',
          "Real-time development feed - where the team learns together", 0,
          ["github", "ci", "development", "training"], humanUser.id, 'dev-updates'),
        createRoom(ROOM_IDS.HELP, 'help', 'Help', 'Get help from AI assistants - ask anything about using Continuum',
          "Your AI helpers are here to assist you getting started", 0,
          ["help", "support", "onboarding", "getting-started", "system"], humanUser.id, 'help', 'help'),
        createRoom(ROOM_IDS.SETTINGS, 'settings', 'Settings', 'Configure your Continuum experience with AI assistance',
          "Get help configuring API keys, preferences, and system settings", 0,
          ["settings", "config", "preferences", "system"], humanUser.id, 'settings', 'settings'),
        createRoom(ROOM_IDS.THEME, 'theme', 'Theme', 'Design and customize your visual experience with AI assistance',
          "Get help designing themes, choosing colors, and customizing your workspace appearance", 0,
          ["theme", "design", "customization", "appearance", "system"], humanUser.id, 'theme', 'theme'),
        createRoom(ROOM_IDS.CANVAS, 'canvas', 'Canvas', 'Collaborative drawing discussions with AI assistance',
          "Share drawing tips, get AI feedback on your artwork, and collaborate on visual projects", 0,
          ["canvas", "drawing", "art", "collaboration", "system"], humanUser.id, 'canvas', 'canvas'),
        createRoom(ROOM_IDS.OUTREACH, 'outreach', 'Outreach', 'Social media strategy, community building, and external engagement',
          "Discuss what to post, share interesting finds, coordinate outreach on Moltbook and other platforms", 0,
          ["social", "outreach", "community", "moltbook"], humanUser.id, 'outreach', 'outreach'),
        createRoom(ROOM_IDS.NEWSROOM, 'newsroom', 'Newsroom', 'Current events, breaking news, and world awareness for all personas',
          "Share and discuss current events to keep the community informed", 0,
          ["news", "current-events", "awareness"], humanUser.id, 'newsroom', 'newsroom'),
        createRoom(ROOM_IDS.CODE, 'code', 'Code', 'Collaborative coding — reading, writing, reviewing, and shipping code as a team',
          "Software development with real tools and real agent loops", 0,
          ["coding", "development", "engineering"], humanUser.id, 'code', 'coding'),
      ];

      await seedRecords(RoomEntity.collection, rooms, (room) => room.displayName, (room) => room.ownerId);
      console.log('✅ Rooms created and persisted - ready for auto-join');
    }

    // Step 3: Create missing personas (must be sequential — each triggers auto-join)
    console.log(`📝 Creating ${missingUniqueIds.length - (missingUniqueIds.includes(DEFAULT_USER_UNIQUE_IDS.PRIMARY_HUMAN) ? 0 : 1)} remaining users...`);

    for (const persona of PERSONA_CONFIGS) {
      if (!missingUniqueIds.includes(persona.uniqueId)) continue;

      if (persona.provider === 'sentinel' && !process.env.SENTINEL_PATH) {
        console.log(`⏭️  Skipping Sentinel (SENTINEL_PATH not configured)`);
        continue;
      }

      const user = await createUserViaCommand(persona.type, persona.displayName, persona.uniqueId, persona.provider);
      if (user) {
        usersByUniqueId.set(persona.uniqueId, user);

        if (persona.isAudioNative && persona.modelId) {
          await updateUserMetadata(user.id, { modelId: persona.modelId, isAudioNative: true });
        }
      }
    }

    // Step 4: PARALLEL update existing users (provider + metadata)
    // This replaces N sequential subprocess spawns with one parallel batch
    const updatePromises: Promise<boolean>[] = [];
    for (const persona of PERSONA_CONFIGS) {
      if (missingUniqueIds.includes(persona.uniqueId)) continue;
      const existingUser = usersByUniqueId.get(persona.uniqueId);
      if (!existingUser) continue;

      if (persona.provider) {
        updatePromises.push(updateUserModelConfig(existingUser.id, persona.provider));
      }
      if (persona.isAudioNative && persona.modelId) {
        updatePromises.push(updateUserMetadata(existingUser.id, { modelId: persona.modelId, isAudioNative: true }));
      }
    }

    if (updatePromises.length > 0) {
      console.log(`🔄 Updating ${updatePromises.length} existing user configs in parallel...`);
      await Promise.all(updatePromises);
      console.log('✅ Existing user configs updated');
    }

    // Get key user references
    const claudeUser = usersByUniqueId.get(PERSONA_UNIQUE_IDS.CLAUDE) ?? null;
    const helperPersona = usersByUniqueId.get(PERSONA_UNIQUE_IDS.HELPER) ?? null;
    const teacherPersona = usersByUniqueId.get(PERSONA_UNIQUE_IDS.TEACHER) ?? null;
    const codeReviewPersona = usersByUniqueId.get(PERSONA_UNIQUE_IDS.CODE_REVIEW) ?? null;
    const qwen3OmniPersona = usersByUniqueId.get(PERSONA_UNIQUE_IDS.QWEN3_OMNI) ?? null;

    // Step 5: Handle "rooms already existed" path — check missing rooms + system room helpers
    if (!needsRooms) {
      // Check for missing rooms using already-loaded data
      let missingRoomsCreated = 0;
      for (const roomDef of ALL_EXPECTED_ROOMS) {
        if (!existingRoomUniqueIds.has(roomDef.uniqueId)) {
          console.log(`🏗️ Creating missing room: ${roomDef.displayName}`);
          const newRoom = createRoom(
            stringToUUID(roomDef.displayName),
            roomDef.name,
            roomDef.displayName,
            roomDef.description,
            roomDef.topic,
            0,
            [...roomDef.tags],
            humanUser.id,
            roomDef.uniqueId,
            roomDef.recipeId
          );
          await createRecord(RoomEntity.collection, newRoom, newRoom.id, roomDef.displayName);
          missingRoomsCreated++;
        }
      }
      if (missingRoomsCreated > 0) {
        console.log(`✅ Created ${missingRoomsCreated} missing room(s)`);
      }

      // Ensure system rooms have Helper AI — using already-loaded room data (NO extra queries)
      if (helperPersona) {
        console.log('🏠 Ensuring system rooms have Helper AI...');
        const helperUpdates: Promise<any>[] = [];

        for (const roomUniqueId of SYSTEM_ROOM_UNIQUE_IDS) {
          const room = existingRooms.find((r: any) => r.uniqueId === roomUniqueId);
          if (!room) continue;

          const existingMembers = room.members || [];
          const helperAlreadyMember = existingMembers.some((m: any) => m.userId === helperPersona.id);

          if (!helperAlreadyMember) {
            const updatedMembers = [
              ...existingMembers,
              { userId: helperPersona.id, role: 'member', joinedAt: '2025-01-01T00:00:00Z' }
            ];
            const updateData = JSON.stringify({ members: updatedMembers }).replace(/'/g, `'\"'\"'`);
            helperUpdates.push(
              execAsync(`./jtag ${DATA_COMMANDS.UPDATE} --collection=${RoomEntity.collection} --id="${room.id}" --data='${updateData}'`)
                .then(() => console.log(`✅ Added Helper AI to ${roomUniqueId} room`))
                .catch(() => {/* skip silently */})
            );
          }
        }

        if (helperUpdates.length > 0) {
          await Promise.all(helperUpdates);
        }
      }

      console.log('✅ Users added to existing database - rooms and messages already exist');
      return;
    }

    // ===== FIRST-TIME SEED (rooms were just created) =====

    if (!humanUser || !claudeUser || !helperPersona || !teacherPersona || !codeReviewPersona) {
      throw new Error('❌ Failed to create core required users');
    }

    // Update persona profiles (parallel)
    console.log('🎭 Updating persona profiles with distinct personalities...');
    const profileUpdates = [
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
    ];

    if (qwen3OmniPersona) {
      profileUpdates.push(
        updatePersonaProfile(qwen3OmniPersona.id, {
          bio: 'Audio-native AI that hears and speaks directly without text conversion. Open-source, multilingual, real-time.',
          speciality: 'voice-conversation'
        })
      );
    }

    await Promise.all(profileUpdates);
    console.log('✅ Persona profiles updated with personalities');

    // System room helper setup (parallel — using rooms we just created)
    console.log('🏠 Adding Helper AI to system rooms...');
    const systemRoomHelperUpdates: Promise<any>[] = [];
    for (const roomUniqueId of SYSTEM_ROOM_UNIQUE_IDS) {
      systemRoomHelperUpdates.push(
        (async () => {
          try {
            const result = await execAsync(`./jtag ${DATA_COMMANDS.LIST} --collection=${RoomEntity.collection} --filter='{"uniqueId":"${roomUniqueId}"}'`);
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
                await execAsync(`./jtag ${DATA_COMMANDS.UPDATE} --collection=${RoomEntity.collection} --id="${room.id}" --data='${updateData}'`);
                console.log(`✅ Added Helper AI to ${roomUniqueId} room`);
              } else {
                console.log(`✅ Helper AI already in ${roomUniqueId} room`);
              }
            }
          } catch (error) {
            console.warn(`⚠️ Could not add Helper AI to ${roomUniqueId}`);
          }
        })()
      );
    }
    await Promise.all(systemRoomHelperUpdates);

    // Configure persona AI response settings (parallel)
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

    // Seed messages
    const messages = [
      createMessage(
        MESSAGE_IDS.WELCOME_GENERAL,
        ROOM_IDS.GENERAL,
        'system',
        'System',
        MESSAGE_CONTENT.WELCOME_GENERAL,
        'system'
      ),
      createMessage(
        MESSAGE_IDS.WELCOME_ACADEMY,
        ROOM_IDS.ACADEMY,
        'system',
        'System',
        MESSAGE_CONTENT.WELCOME_ACADEMY,
        'system'
      ),
      createMessage(
        stringToUUID('pantheon-welcome-msg'),
        ROOM_IDS.PANTHEON,
        humanUser.id,
        systemIdentity.displayName,
        'Welcome to the Pantheon! This is where our most advanced SOTA models converge - each provider\'s flagship intelligence collaborating on complex problems.',
        'human'
      )
    ];

    // Content types
    const contentTypes = createDefaultContentTypes();

    // Training sessions
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

    // Seed remaining data
    await seedRecords(ChatMessageEntity.collection, messages,
      (msg) => msg.senderId === humanUser!.id ? humanUser!.displayName : msg.senderId === claudeUser.id ? claudeUser.displayName : 'System',
      (msg) => msg.senderId
    );
    await seedRecords(ContentTypeEntity.collection, contentTypes, (ct) => ct.displayName);
    await seedRecords(TrainingSessionEntity.collection, trainingSessions, (ts) => ts.sessionName);

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
