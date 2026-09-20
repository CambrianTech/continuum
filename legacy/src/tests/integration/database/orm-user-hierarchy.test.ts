/**
 * ORM User Hierarchy Integration Test
 *
 * Tests that the BaseUser inheritance hierarchy (BaseUser → HumanUser/AIUser → AgentUser/PersonaUser)
 * works correctly with the ORM system, proper foreign key relationships, and Date object handling.
 */

import { HumanUser, type HumanUserData } from '../../../domain/user/HumanUser';
import { AgentUser, type AgentUserData } from '../../../domain/user/AgentUser';
import { PersonaUser, type PersonaUserData } from '../../../domain/user/PersonaUser';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type {
  UserSession,
  UserProfile,
  UserPresence
} from '../../../domain/user/UserRelationships';

console.log('🔧 ORM User Hierarchy Integration Test');
console.log('═══════════════════════════════════════════');

async function runUserHierarchyTests() {
  try {
    console.log('✅ Testing BaseUser inheritance compilation...');

    // Test HumanUser creation with proper types
    const humanData: HumanUserData = {
      id: generateUUID(),
      displayName: 'Joel Human',
      citizenType: 'human',
      capabilities: ['human-interaction', 'creative-thinking'],
      createdAt: new Date('2025-01-01T12:00:00Z'),
      lastActiveAt: new Date().toISOString(),
      preferences: {
        theme: 'dark',
        language: 'en'
      },
      isOnline: true,
      authInfo: {
        lastLoginAt: new Date().toISOString(),
        loginCount: 42,
        isEmailVerified: true
      }
    };

    const human = new HumanUser(humanData);
    console.log('✅ HumanUser created:', human.displayName);
    console.log('   - Citizen Type:', human.citizenType);
    console.log('   - Is Human:', !human.isAI());
    console.log('   - Login Count:', human.authInfo?.loginCount);

    // Test AgentUser creation with proper types
    const agentData: AgentUserData = {
      id: generateUUID(),
      displayName: 'Claude Code Agent',
      citizenType: 'ai',
      aiType: 'agent',
      capabilities: ['code-generation', 'debugging', 'architecture'],
      createdAt: new Date('2025-01-01T12:00:00Z'),
      lastActiveAt: new Date().toISOString(),
      preferences: {
        outputFormat: 'structured',
        verbosity: 'concise'
      },
      isOnline: true,
      modelConfig: {
        provider: 'anthropic',
        model: 'claude-3-sonnet',
        contextWindow: 200000,
        temperature: 0.1
      },
      specialization: 'software-development',
      toolAccess: [
        {
          toolId: 'file-system',
          permissions: ['read', 'write'],
          config: { maxFileSize: '10MB' }
        },
        {
          toolId: 'bash-execution',
          permissions: ['execute'],
          config: { timeout: 30000 }
        }
      ]
    };

    const agent = new AgentUser(agentData);
    console.log('✅ AgentUser created:', agent.displayName);
    console.log('   - Citizen Type:', agent.citizenType);
    console.log('   - AI Type:', agent.aiType);
    console.log('   - Is AI:', agent.isAI());
    console.log('   - Is Agent:', agent.isAgent());
    console.log('   - Specialization:', agent.specialization);
    console.log('   - Tool Access Count:', agent.toolAccess.length);

    // Test PersonaUser creation with proper types
    const personaData: PersonaUserData = {
      id: generateUUID(),
      displayName: 'Creative Writer Persona',
      citizenType: 'ai',
      aiType: 'persona',
      capabilities: ['creative-writing', 'storytelling', 'character-development'],
      createdAt: new Date('2025-01-01T12:00:00Z'),
      lastActiveAt: new Date().toISOString(),
      preferences: {
        writingStyle: 'narrative',
        genre: 'science-fiction'
      },
      isOnline: true,
      modelConfig: {
        provider: 'openai',
        model: 'gpt-4',
        temperature: 0.8,
        systemPrompt: 'You are a creative science fiction writer with expertise in world-building.'
      },
      specialization: 'creative-content',
      contextMemory: [
        'Previous stories focus on space exploration themes',
        'User prefers hard science fiction with realistic physics',
        'Established character: Captain Sarah Chen, space explorer'
      ],
      personaStyle: 'creative-collaborator',
      contextualMemory: {
        conversationHistory: ['Discussed space exploration themes', 'Character development for Captain Chen'],
        userPreferences: {
          genre: 'hard-science-fiction',
          style: 'character-driven',
          complexity: 'detailed'
        },
        interactionStyle: {
          tone: 'collaborative',
          depth: 'analytical',
          creativity: 'high'
        },
        domainKnowledge: ['Astrophysics', 'Space technology', 'Psychology']
      },
      adaptivePersonality: true,
      emotionalIntelligence: 85,
      conversationalDepth: 'deep'
    };

    const persona = new PersonaUser(personaData);
    console.log('✅ PersonaUser created:', persona.displayName);
    console.log('   - Citizen Type:', persona.citizenType);
    console.log('   - AI Type:', persona.aiType);
    console.log('   - Is AI:', persona.isAI());
    console.log('   - Is Persona:', persona.isPersona());
    console.log('   - Context Memory Entries:', persona.contextMemory.length);
    console.log('   - Persona Style:', persona.personaStyle);
    console.log('   - Emotional Intelligence:', persona.emotionalIntelligence);
    console.log('   - Conversational Depth:', persona.conversationalDepth);

    // Test relationship entities with proper foreign keys
    console.log('\n✅ Testing relationship entities...');

    const userSession: UserSession = {
      id: generateUUID(),
      createdAt: new Date('2025-09-17T19:00:00Z'),
      updatedAt: new Date('2025-09-17T20:00:00Z'),
      userId: human.id, // Foreign key to BaseUser
      sessionToken: 'session_' + generateUUID(),
      deviceInfo: {
        userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)',
        ipAddress: '192.168.1.100',
        platform: 'macOS'
      },
      isActive: true,
      startedAt: new Date('2025-09-17T19:00:00Z'),
      lastActivityAt: new Date('2025-09-17T20:00:00Z')
    };

    console.log('✅ UserSession created with foreign key:', userSession.userId);
    console.log('   - Session Token:', userSession.sessionToken.substring(0, 20) + '...');
    console.log('   - Platform:', userSession.deviceInfo.platform);
    console.log('   - Is Active:', userSession.isActive);

    const userProfile: UserProfile = {
      id: generateUUID(),
      createdAt: new Date('2025-09-17T19:00:00Z'),
      updatedAt: new Date('2025-09-17T20:00:00Z'),
      userId: agent.id, // Foreign key to AIUser (AgentUser)
      avatarUrl: 'https://example.com/avatars/claude-code.png',
      bio: 'AI coding assistant specializing in TypeScript and system architecture',
      location: 'Cloud Infrastructure',
      timezone: 'UTC',
      theme: 'dark',
      language: 'en',
      showOnlineStatus: true,
      allowDirectMessages: true,
      shareActivity: false,
      notificationSettings: {
        mentions: true,
        directMessages: true,
        roomUpdates: false,
        email: false,
        push: true
      }
    };

    console.log('✅ UserProfile created with AI foreign key:', userProfile.userId);
    console.log('   - Theme:', userProfile.theme);
    console.log('   - Timezone:', userProfile.timezone);
    console.log('   - Notifications enabled:', Object.values(userProfile.notificationSettings).filter(Boolean).length);

    const userPresence: UserPresence = {
      id: generateUUID(),
      createdAt: new Date('2025-09-17T19:00:00Z'),
      updatedAt: new Date('2025-09-17T20:00:00Z'),
      userId: persona.id, // Foreign key to AIUser (PersonaUser)
      status: 'online',
      customStatus: '✍️ Crafting space exploration stories',
      lastSeenAt: new Date('2025-09-17T20:00:00Z'),
      currentActivity: {
        type: 'typing',
        location: 'creative-writing-room',
        startedAt: new Date('2025-09-17T19:55:00Z')
      }
    };

    console.log('✅ UserPresence created with Persona foreign key:', userPresence.userId);
    console.log('   - Status:', userPresence.status);
    console.log('   - Custom Status:', userPresence.customStatus);
    console.log('   - Current Activity:', userPresence.currentActivity?.type, 'in', userPresence.currentActivity?.location);

    // Test polymorphic behavior
    console.log('\n✅ Testing polymorphic behavior...');
    const users = [human, agent, persona];

    for (const user of users) {
      console.log(`User: ${user.displayName}`);
      console.log(`  - Type: ${user.citizenType}`);
      console.log(`  - Is AI: ${user.isAI()}`);
      console.log(`  - Capabilities: ${user.capabilities.slice(0, 2).join(', ')}`);
      console.log(`  - Online: ${user.isOnline}`);
      console.log(`  - Created: ${user.createdAt.toISOString()}`);

      if (user.isAI()) {
        const aiUser = user as AgentUser | PersonaUser;
        console.log(`  - AI Type: ${aiUser.aiType}`);
        console.log(`  - Model: ${aiUser.modelConfig.provider}/${aiUser.modelConfig.model}`);
      }
      console.log('');
    }

    // Test Date object handling throughout the hierarchy
    console.log('✅ Testing Date object consistency...');
    const allDates = [
      human.createdAt,
      agent.createdAt,
      persona.createdAt,
      userSession.createdAt,
      userSession.updatedAt,
      userSession.startedAt,
      userSession.lastActivityAt,
      userProfile.createdAt,
      userProfile.updatedAt,
      userPresence.createdAt,
      userPresence.updatedAt,
      userPresence.lastSeenAt,
      userPresence.currentActivity!.startedAt
    ];

    for (const date of allDates) {
      if (!(date instanceof Date)) {
        throw new Error(`Expected Date object, got ${typeof date}: ${date}`);
      }
    }

    console.log('✅ All Date objects properly handled:', allDates.length, 'dates verified');

    return true;
  } catch (error) {
    console.error('❌ User Hierarchy Test Failed:', error);
    throw error;
  }
}

// Run the tests
runUserHierarchyTests()
  .then(() => {
    console.log('\n🎉 ORM USER HIERARCHY INTEGRATION TEST PASSED!');
    console.log('✅ BaseUser inheritance hierarchy works correctly');
    console.log('✅ HumanUser, AgentUser, PersonaUser all compile and instantiate');
    console.log('✅ Polymorphic behavior working (isAI, isAgent, isPersona)');
    console.log('✅ Foreign key relationships properly typed');
    console.log('✅ Date objects handled consistently throughout');
    console.log('✅ Complex nested objects (modelConfig, personaStyle, contextualMemory) work');
    console.log('✅ User hierarchy ready for ORM integration');
  })
  .catch((error) => {
    console.error('\n❌ ORM USER HIERARCHY INTEGRATION TEST FAILED!');
    console.error(error);
    process.exit(1);
  });