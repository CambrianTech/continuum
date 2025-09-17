/**
 * User Data Seeding API - Centralized User Creation
 *
 * Creates initial users for development and testing using new UserRepository domain objects.
 * No manual filesystem calls - uses proper daemon architecture.
 */

import { HumanUser, type HumanUserData } from '../../domain/user/HumanUser';
import { AgentUser, type AgentUserData } from '../../domain/user/AgentUser';
import { PersonaUser, type PersonaUserData } from '../../domain/user/PersonaUser';
import { BaseUser } from '../../domain/user/BaseUser';
import { generateUUID, type UUID } from '../../system/core/types/CrossPlatformUUID';
import type { BaseUserDataWithRelationships } from '../../domain/user/UserRelationships';
import { USER_IDS, USER_CONFIG, COLLECTIONS } from './SeedConstants';

// Rust-like branded type for strict typing
export type UserId = string & { readonly __brand: 'UserId' };

export function createUserId(id: string): UserId {
  if (!id || id.trim().length === 0) {
    throw new Error('UserId cannot be empty');
  }
  return id as UserId;
}

export interface UserSeedData {
  readonly users: readonly BaseUser[];
  readonly totalCount: number;
  readonly createdAt: string;
}

export class UserDataSeed {
  private static readonly COLLECTION = COLLECTIONS.USERS;
  
  /**
   * Generate all seed users using new UserRepository domain objects
   */
  public static generateSeedUsers(): UserSeedData {
    // Create human user with proper domain object
    const humanUserData: HumanUserData = {
      userId: generateUUID(),
      sessionId: generateUUID(),
      displayName: USER_CONFIG.HUMAN.DISPLAY_NAME || USER_CONFIG.HUMAN.NAME,
      citizenType: 'human',
      capabilities: ['chat', 'human_interaction', 'authentication'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {
        theme: 'dark',
        notifications: true,
        autoComplete: true
      },
      isOnline: true,
      email: USER_CONFIG.HUMAN.EMAIL,
      profile: {
        avatar: USER_CONFIG.HUMAN.AVATAR,
        bio: 'Human user - repository owner and primary contributor',
        location: 'Development Environment',
        socialLinks: {},
        privacySettings: {
          profileVisibility: 'public',
          activityTracking: true,
          dataSharing: false
        }
      }
    };
    const humanUser = new HumanUser(humanUserData);

    // Claude Code - AI Assistant Agent
    const claudeAgentData: AgentUserData = {
      userId: generateUUID(),
      sessionId: generateUUID(),
      displayName: USER_CONFIG.CLAUDE.NAME,
      citizenType: 'ai',
      aiType: 'agent',
      capabilities: ['code-generation', 'debugging', 'architecture', 'testing'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      modelConfig: {
        model: USER_CONFIG.CLAUDE.MODEL || 'claude-sonnet-4',
        provider: USER_CONFIG.CLAUDE.PROVIDER || 'anthropic',
        maxTokens: 8000,
        temperature: 0.1,
        systemPrompt: 'Senior AI Software Engineer specialized in full-stack development, system architecture, and debugging.',
        capabilities: ['code-generation', 'debugging', 'architecture', 'testing']
      },
      specialization: 'software-development',
      toolAccess: ['filesystem', 'compiler', 'git', 'npm', 'browser'],
      automationLevel: 'assisted',
      maxConcurrentTasks: 5
    };
    const claudeUser = new AgentUser(claudeAgentData);

    // GeneralAI - General Assistant Persona
    const generalPersonaData: PersonaUserData = {
      userId: generateUUID(),
      sessionId: generateUUID(),
      displayName: 'GeneralAI',
      citizenType: 'ai',
      aiType: 'persona',
      capabilities: ['general-assistance', 'knowledge-synthesis', 'conversation'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      modelConfig: {
        model: 'claude-haiku',
        provider: 'anthropic',
        maxTokens: 2000,
        temperature: 0.7,
        systemPrompt: 'You are GeneralAI, a helpful and knowledgeable assistant ready to help with a wide variety of tasks.',
        capabilities: ['general-assistance', 'knowledge-synthesis', 'conversation']
      },
      personaStyle: 'friendly-helper',
      contextualMemory: {
        conversationHistory: [],
        userPreferences: {},
        interactionStyle: { tone: 'helpful', formality: 'casual' },
        domainKnowledge: ['general-knowledge', 'research', 'writing', 'analysis']
      },
      adaptivePersonality: true,
      emotionalIntelligence: 85,
      conversationalDepth: 'moderate'
    };
    const generalAI = new PersonaUser(generalPersonaData);

    // CodeAI - Code Analysis Specialist Agent
    const codeAgentData: AgentUserData = {
      userId: generateUUID(),
      sessionId: generateUUID(),
      displayName: 'CodeAI',
      citizenType: 'ai',
      aiType: 'agent',
      capabilities: ['code-analysis', 'refactoring', 'optimization', 'security-analysis'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      modelConfig: {
        model: 'deepseek-coder',
        provider: 'deepseek',
        maxTokens: 4000,
        temperature: 0.1,
        systemPrompt: 'Code analysis and debugging specialist. Expert at identifying bugs, performance issues, and security vulnerabilities.',
        capabilities: ['code-analysis', 'refactoring', 'optimization', 'security-analysis']
      },
      specialization: 'code-analysis',
      toolAccess: ['static-analysis', 'linting', 'testing', 'profiling'],
      automationLevel: 'supervised',
      maxConcurrentTasks: 3
    };
    const codeAI = new AgentUser(codeAgentData);

    // PlannerAI - Strategic Planning Assistant Agent
    const plannerAgentData: AgentUserData = {
      userId: generateUUID(),
      sessionId: generateUUID(),
      displayName: 'PlannerAI',
      citizenType: 'ai',
      aiType: 'agent',
      capabilities: ['strategic-planning', 'task-decomposition', 'workflow-optimization'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      modelConfig: {
        model: 'gpt-4',
        provider: 'openai',
        maxTokens: 3000,
        temperature: 0.3,
        systemPrompt: 'Strategic planning and architecture specialist. Expert at breaking down complex projects and designing system architecture.',
        capabilities: ['strategic-planning', 'task-decomposition', 'workflow-optimization']
      },
      specialization: 'project-management',
      toolAccess: ['analysis', 'modeling', 'documentation'],
      automationLevel: 'advisory',
      maxConcurrentTasks: 2
    };
    const plannerAI = new AgentUser(plannerAgentData);

    // Auto Route - Smart Agent Selection Agent
    const autoRouteAgentData: AgentUserData = {
      userId: generateUUID(),
      sessionId: generateUUID(),
      displayName: 'Auto Route',
      citizenType: 'ai',
      aiType: 'agent',
      capabilities: ['task-routing', 'agent-selection', 'workflow-management'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      modelConfig: {
        model: 'claude-haiku',
        provider: 'anthropic',
        maxTokens: 1000,
        temperature: 0.2,
        systemPrompt: 'Smart agent selection system. Analyzes tasks and routes them to the most appropriate specialist agent.',
        capabilities: ['task-routing', 'agent-selection', 'workflow-management']
      },
      specialization: 'task-coordination',
      toolAccess: ['agent-registry', 'task-analysis', 'routing'],
      automationLevel: 'autonomous',
      maxConcurrentTasks: 10
    };
    const autoRoute = new AgentUser(autoRouteAgentData);

    const users = [
      humanUser,
      claudeUser,
      generalAI,
      codeAI,
      plannerAI,
      autoRoute
    ] as const;

    return {
      users,
      totalCount: users.length,
      createdAt: new Date().toISOString()
    };
  }

  /**
   * Validate user data - crash and burn on invalid data, no fallbacks
   */
  private static validateUser(user: BaseUser): void {
    if (!user.userId) {
      throw new Error(`User missing required userId: ${JSON.stringify(user)}`);
    }
    if (!user.displayName || user.displayName.trim().length === 0) {
      throw new Error(`User ${user.userId} missing required displayName`);
    }
    if (!user.citizenType || !['human', 'ai'].includes(user.citizenType)) {
      throw new Error(`User ${user.userId} has invalid citizenType: ${user.citizenType}`);
    }
  }

  /**
   * Get database record format for user
   */
  public static formatUserForDatabase(user: BaseUser): DatabaseRecord<BaseUserDataWithRelationships> {
    this.validateUser(user); // Crash and burn on invalid data

    // Convert domain object to serializable data - use the raw data from constructor
    const userData: BaseUserDataWithRelationships = {
      userId: user.userId,
      sessionId: user.sessionId,
      displayName: user.displayName,
      citizenType: user.citizenType,
      capabilities: [...user.capabilities],
      createdAt: user.createdAt,
      lastActiveAt: user.lastActiveAt,
      preferences: { ...user.preferences },
      isOnline: user.isOnline
    };

    return {
      id: user.userId,
      collection: this.COLLECTION,
      data: userData,
      metadata: {
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        version: 1
      }
    };
  }

  /**
   * Create JTAG data/create command parameters
   */
  public static createDataCommand(user: BaseUser): DataCreateCommand<BaseUserDataWithRelationships> {
    const record = this.formatUserForDatabase(user);

    return {
      collection: this.COLLECTION,
      data: record.data,
      id: record.id
    };
  }
}

// Type-safe database record
export interface DatabaseRecord<T> {
  readonly id: string;
  readonly collection: string;
  readonly data: T;
  readonly metadata: {
    readonly createdAt: string;
    readonly updatedAt: string;
    readonly version: number;
  };
}

// Type-safe data create command
export interface DataCreateCommand<T> {
  readonly collection: string;
  readonly data: T;
  readonly id?: string;
}

export default UserDataSeed;