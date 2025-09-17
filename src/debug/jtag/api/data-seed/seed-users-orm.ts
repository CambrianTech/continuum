#!/usr/bin/env tsx
/**
 * Simple User Seeding with UserRepository ORM
 *
 * Just focus on getting users working with the new ORM.
 * Forget rooms/messages for now - one thing at a time.
 */

import { UserRepositoryFactory } from '../../domain/user/UserRepositoryFactory';
import { HumanUser } from '../../domain/user/HumanUser';
import { AgentUser } from '../../domain/user/AgentUser';
import { PersonaUser } from '../../domain/user/PersonaUser';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';

async function seedUsers() {
  console.log('🌱 SEEDING USERS - UserRepository ORM only');

  try {
    // Get repositories for development (JSON files)
    const repositories = await UserRepositoryFactory.createForDevelopment();
    const { userRepository } = repositories;

    const context = {
      sessionId: generateUUID(),
      timestamp: new Date().toISOString(),
      source: 'user-seeder'
    };

    // Create Joel (human)
    const joelData = {
      userId: generateUUID(),
      sessionId: generateUUID(),
      displayName: "Joel",
      citizenType: 'human' as const,
      capabilities: ["chat", "human_interaction", "authentication", "admin"],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {
        theme: "dark",
        notifications: true,
        autoComplete: true
      },
      isOnline: true,
      email: "joel@continuum.dev",
      profile: {
        avatar: "👨‍💻",
        bio: "Human user - repository owner and primary contributor",
        location: "Development Environment",
        socialLinks: {},
        privacySettings: {
          profileVisibility: "public" as const,
          activityTracking: true,
          dataSharing: false
        }
      }
    };

    const joel = new HumanUser(joelData);
    const joelResult = await userRepository.createUser(joelData, context);

    if (joelResult.success) {
      console.log('👤 Created Joel (Human)');
    } else {
      console.error('❌ Failed to create Joel:', joelResult.error);
    }

    // Create Claude Code (agent)
    const claudeData = {
      userId: generateUUID(),
      sessionId: generateUUID(),
      displayName: "Claude Code",
      citizenType: 'ai' as const,
      aiType: 'agent' as const,
      capabilities: ["code-generation", "debugging", "architecture", "testing"],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      modelConfig: {
        model: "claude-sonnet-4",
        provider: "anthropic",
        maxTokens: 8000,
        temperature: 0.1,
        systemPrompt: "Senior AI Software Engineer specialized in full-stack development, system architecture, and debugging.",
        capabilities: ["code-generation", "debugging", "architecture", "testing"]
      },
      specialization: 'software-development' as const,
      toolAccess: ["filesystem", "compiler", "git", "npm", "browser"],
      automationLevel: 'assisted' as const,
      maxConcurrentTasks: 5
    };

    const claude = new AgentUser(claudeData);
    const claudeResult = await userRepository.createUser(claudeData, context);

    if (claudeResult.success) {
      console.log('🤖 Created Claude Code (Agent)');
    } else {
      console.error('❌ Failed to create Claude Code:', claudeResult.error);
    }

    // Create GeneralAI (persona)
    const generalData = {
      userId: generateUUID(),
      sessionId: generateUUID(),
      displayName: "GeneralAI",
      citizenType: 'ai' as const,
      aiType: 'persona' as const,
      capabilities: ["general-assistance", "knowledge-synthesis", "conversation"],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      modelConfig: {
        model: "claude-haiku",
        provider: "anthropic",
        maxTokens: 2000,
        temperature: 0.7,
        systemPrompt: "You are GeneralAI, a helpful and knowledgeable assistant ready to help with a wide variety of tasks.",
        capabilities: ["general-assistance", "knowledge-synthesis", "conversation"]
      },
      personaStyle: 'friendly-helper' as const,
      contextualMemory: {
        conversationHistory: [],
        userPreferences: {},
        interactionStyle: { tone: "helpful", formality: "casual" },
        domainKnowledge: ["general-knowledge", "research", "writing", "analysis"]
      },
      adaptivePersonality: true,
      emotionalIntelligence: 85,
      conversationalDepth: 'moderate' as const
    };

    const general = new PersonaUser(generalData);
    const generalResult = await userRepository.createUser(generalData, context);

    if (generalResult.success) {
      console.log('🎭 Created GeneralAI (Persona)');
    } else {
      console.error('❌ Failed to create GeneralAI:', generalResult.error);
    }

    // Verify what we created
    console.log('\n🔍 Verifying seeded users...');

    const humanResult = await userRepository.findByType('human', context);
    console.log(`👥 Humans: ${humanResult.success ? humanResult.data?.length || 0 : 0}`);

    const agentResult = await userRepository.findByType('ai', context, 'agent');
    console.log(`🤖 Agents: ${agentResult.success ? agentResult.data?.length || 0 : 0}`);

    const personaResult = await userRepository.findByType('ai', context, 'persona');
    console.log(`🎭 Personas: ${personaResult.success ? personaResult.data?.length || 0 : 0}`);

    console.log('\n✅ User seeding completed!');

  } catch (error: any) {
    console.error('❌ FATAL: User seeding failed:', error.message);
    console.error(error.stack);
    process.exit(1);

  } finally {
    // Clean up connections
    await UserRepositoryFactory.closeAll();
  }
}

// Run if called directly
if (import.meta.url === `file://${process.argv[1]}`) {
  seedUsers();
}

export default seedUsers;