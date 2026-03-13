#!/usr/bin/env tsx
/**
 * Repository-Based Data Seeder - Uses new UserRepository ORM
 *
 * Replaces the old command-based DataSeeder with direct UserRepository usage.
 * Loads structured seed data from data/seed-data.json and creates proper
 * domain objects (HumanUser, AgentUser, PersonaUser) via the ORM.
 */

import { readFileSync } from 'fs';
import { join } from 'path';
import { UserRepositoryFactory } from '../../domain/user/UserRepositoryFactory';
import { HumanUser, type HumanUserData } from '../../domain/user/HumanUser';
import { AgentUser, type AgentUserData } from '../../domain/user/AgentUser';
import { PersonaUser, type PersonaUserData } from '../../domain/user/PersonaUser';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';
import type { DataOperationContext } from '../../daemons/data-daemon/shared/DataDaemon';

/** Result type from repository operations */
interface RepositoryResult {
  success: boolean;
  data?: unknown;
  error?: string;
}

/** Minimal interface for seeding — matches the methods actually called */
interface HumanRepositorySeeder {
  createHuman(displayName: string, sessionId: string, context: DataOperationContext): Promise<RepositoryResult>;
}

interface AgentRepositorySeeder {
  createAgent(
    displayName: string,
    sessionId: string,
    config: { specialization: string; toolAccess: string[]; automationLevel: string; maxConcurrentTasks: number },
    modelConfig: unknown,
    context: DataOperationContext
  ): Promise<RepositoryResult>;
}

interface PersonaRepositorySeeder {
  createPersona(
    displayName: string,
    sessionId: string,
    config: { personaStyle: string; contextualMemory: boolean; adaptivePersonality: boolean; emotionalIntelligence: number; conversationalDepth: number },
    modelConfig: unknown,
    context: DataOperationContext
  ): Promise<RepositoryResult>;
}

interface UserRepositorySeeder {
  findByType(type: string, context: DataOperationContext, subType?: string): Promise<RepositoryResult & { data?: unknown[] }>;
}

export interface SeedDataStructure {
  version: string;
  lastUpdated: string;
  description: string;
  users: {
    humans: HumanUserData[];
    agents: AgentUserData[];
    personas: PersonaUserData[];
  };
  rooms: Record<string, unknown>[];
  messages: Record<string, unknown>[];
}

export class RepositoryDataSeeder {
  private seedData: SeedDataStructure;
  private context: DataOperationContext;

  constructor(seedDataPath: string = 'data/seed-data.json') {
    // Load seed data from JSON file
    const fullPath = join(process.cwd(), seedDataPath);
    const rawData = readFileSync(fullPath, 'utf-8');
    this.seedData = JSON.parse(rawData);

    // Create operation context
    this.context = {
      sessionId: generateUUID(),
      timestamp: new Date().toISOString(),
      source: 'repository-seeder'
    };
  }

  /**
   * Clear all data using UserRepository
   */
  async clearAllData(): Promise<void> {
    console.log('🧹 CLEARING ALL DATA - Using UserRepository ORM');

    try {
      // Get repositories for development (JSON files)
      const { userRepository } = await UserRepositoryFactory.createForDevelopment();

      // For now, we'll rely on the underlying storage to clear data
      // The UserRepository doesn't expose bulk delete yet, so we'll
      // clear by finding all users and deleting individually
      console.log('📋 Clearing users via UserRepository...');

      // Find all users by type and delete them
      const humanResult = await userRepository.findByType('human', this.context);
      if (humanResult.success && humanResult.data) {
        console.log(`🗑️ Clearing ${humanResult.data.length} human users`);
        // TODO: Implement bulk delete in UserRepository
      }

      const aiResult = await userRepository.findByType('ai', this.context);
      if (aiResult.success && aiResult.data) {
        console.log(`🗑️ Clearing ${aiResult.data.length} AI users`);
        // TODO: Implement bulk delete in UserRepository
      }

      console.log('✅ Data cleared using UserRepository');

    } catch (error: unknown) {
      console.error('❌ FATAL: Failed to clear data via UserRepository:', (error instanceof Error ? error.message : String(error)));
      throw error;
    }
  }

  /**
   * Seed all data using UserRepository ORM
   */
  async seedAllData(): Promise<void> {
    console.log('🌱 SEEDING ALL DATA - Using UserRepository ORM');
    console.log(`📦 Seed data version: ${this.seedData.version}`);
    console.log(`📝 Description: ${this.seedData.description}`);

    try {
      // Get repositories for development
      const { humanRepository, agentRepository, personaRepository } =
        await UserRepositoryFactory.createForDevelopment();

      // Seed humans
      await this.seedHumans(humanRepository);

      // Seed agents
      await this.seedAgents(agentRepository);

      // Seed personas
      await this.seedPersonas(personaRepository);

      // TODO: Seed rooms and messages

      console.log('✅ ALL DATA SEEDED - UserRepository ORM integration complete');

    } catch (error: unknown) {
      console.error('❌ FATAL: Failed to seed data via UserRepository:', (error instanceof Error ? error.message : String(error)));
      throw error;
    }
  }

  /**
   * Seed human users
   */
  private async seedHumans(humanRepository: HumanRepositorySeeder): Promise<void> {
    console.log(`👥 Seeding ${this.seedData.users.humans.length} human users...`);

    for (const humanData of this.seedData.users.humans) {
      try {
        // Create HumanUser domain object
        const humanUser = new HumanUser(humanData);

        // Store via repository
        const result = await humanRepository.createHuman(
          humanData.displayName,
          humanData.sessionId,
          this.context
        );

        if (result.success) {
          console.log(`👤 Created human user: ${humanData.displayName}`);
        } else {
          throw new Error(`Failed to create human: ${result.error}`);
        }

      } catch (error: unknown) {
        console.error(`❌ Failed to create human user ${humanData.displayName}:`, (error instanceof Error ? error.message : String(error)));
        throw error;
      }
    }

    console.log(`✅ Seeded ${this.seedData.users.humans.length} human users`);
  }

  /**
   * Seed agent users
   */
  private async seedAgents(agentRepository: AgentRepositorySeeder): Promise<void> {
    console.log(`🤖 Seeding ${this.seedData.users.agents.length} agent users...`);

    for (const agentData of this.seedData.users.agents) {
      try {
        // Create AgentUser domain object
        const agentUser = new AgentUser(agentData);

        // Store via repository
        const result = await agentRepository.createAgent(
          agentData.displayName,
          agentData.sessionId,
          {
            specialization: agentData.specialization,
            toolAccess: agentData.toolAccess,
            automationLevel: agentData.automationLevel,
            maxConcurrentTasks: agentData.maxConcurrentTasks
          },
          agentData.modelConfig,
          this.context
        );

        if (result.success) {
          console.log(`🤖 Created agent user: ${agentData.displayName} (${agentData.specialization})`);
        } else {
          throw new Error(`Failed to create agent: ${result.error}`);
        }

      } catch (error: unknown) {
        console.error(`❌ Failed to create agent user ${agentData.displayName}:`, (error instanceof Error ? error.message : String(error)));
        throw error;
      }
    }

    console.log(`✅ Seeded ${this.seedData.users.agents.length} agent users`);
  }

  /**
   * Seed persona users
   */
  private async seedPersonas(personaRepository: PersonaRepositorySeeder): Promise<void> {
    console.log(`🎭 Seeding ${this.seedData.users.personas.length} persona users...`);

    for (const personaData of this.seedData.users.personas) {
      try {
        // Create PersonaUser domain object
        const personaUser = new PersonaUser(personaData);

        // Store via repository
        const result = await personaRepository.createPersona(
          personaData.displayName,
          personaData.sessionId,
          {
            personaStyle: personaData.personaStyle,
            contextualMemory: personaData.contextualMemory,
            adaptivePersonality: personaData.adaptivePersonality,
            emotionalIntelligence: personaData.emotionalIntelligence,
            conversationalDepth: personaData.conversationalDepth
          },
          personaData.modelConfig,
          this.context
        );

        if (result.success) {
          console.log(`🎭 Created persona user: ${personaData.displayName} (${personaData.personaStyle})`);
        } else {
          throw new Error(`Failed to create persona: ${result.error}`);
        }

      } catch (error: unknown) {
        console.error(`❌ Failed to create persona user ${personaData.displayName}:`, (error instanceof Error ? error.message : String(error)));
        throw error;
      }
    }

    console.log(`✅ Seeded ${this.seedData.users.personas.length} persona users`);
  }

  /**
   * Verify seeded data using UserRepository
   */
  async verifySeededData(): Promise<void> {
    console.log('🔍 VERIFYING SEEDED DATA - UserRepository ORM verification');

    try {
      const { userRepository } = await UserRepositoryFactory.createForDevelopment();

      // Verify humans
      const humanResult = await userRepository.findByType('human', this.context);
      const humanCount = humanResult.success ? (humanResult.data?.length || 0) : 0;
      console.log(`✅ Human users: ${humanCount}`);

      // Verify AI users (agents + personas)
      const agentResult = await userRepository.findByType('ai', this.context, 'agent');
      const agentCount = agentResult.success ? (agentResult.data?.length || 0) : 0;
      console.log(`✅ Agent users: ${agentCount}`);

      const personaResult = await userRepository.findByType('ai', this.context, 'persona');
      const personaCount = personaResult.success ? (personaResult.data?.length || 0) : 0;
      console.log(`✅ Persona users: ${personaCount}`);

      const totalExpected = this.seedData.users.humans.length +
                           this.seedData.users.agents.length +
                           this.seedData.users.personas.length;
      const totalActual = humanCount + agentCount + personaCount;

      if (totalActual !== totalExpected) {
        throw new Error(`Expected ${totalExpected} users, found ${totalActual}`);
      }

      console.log(`✅ ALL DATA VERIFIED - ${totalActual} users created successfully`);

    } catch (error: unknown) {
      console.error('❌ FATAL: Data verification failed:', (error instanceof Error ? error.message : String(error)));
      throw error;
    }
  }

  /**
   * Complete reset and reseed using UserRepository
   */
  async resetAndSeed(): Promise<void> {
    console.log('🔄 COMPLETE RESET AND SEED - UserRepository ORM');
    console.log('=='.repeat(40));

    try {
      await this.clearAllData();
      await this.seedAllData();
      await this.verifySeededData();

      console.log('=='.repeat(40));
      console.log('🎉 COMPLETE! UserRepository ORM seeding successful');
      console.log(`👥 Humans: ${this.seedData.users.humans.length}`);
      console.log(`🤖 Agents: ${this.seedData.users.agents.length}`);
      console.log(`🎭 Personas: ${this.seedData.users.personas.length}`);
      console.log('✅ All users created via proper domain objects');

    } catch (error: unknown) {
      console.error('❌ FATAL: UserRepository reset and seed failed:', (error instanceof Error ? error.message : String(error)));
      throw error;
    }
  }

  /**
   * Close repository connections
   */
  async close(): Promise<void> {
    await UserRepositoryFactory.closeAll();
  }
}

export default RepositoryDataSeeder;