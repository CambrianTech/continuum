/**
 * User Data Seeding API - Centralized User Creation
 * 
 * Creates initial users for development and testing using JTAG data commands.
 * No manual filesystem calls - uses proper daemon architecture.
 */

import { createHumanUser, PersonaUser, AgentUser, BaseUser } from '../types/User';
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
   * Generate all seed users - strict typing, no fallbacks
   */
  public static generateSeedUsers(): UserSeedData {
    // Create human user with proper ID from the start
    const humanUser: BaseUser = {
      id: createUserId(USER_IDS.HUMAN),
      name: USER_CONFIG.HUMAN.NAME,
      userType: 'human' as const,
      email: USER_CONFIG.HUMAN.EMAIL,
      isAuthenticated: true,
      permissions: [
        { action: 'chat', resource: '*', granted: true },
        { action: 'read_messages', resource: '*', granted: true },
        { action: 'send_messages', resource: '*', granted: true }
      ],
      capabilities: [
        { name: 'human_interaction', enabled: true },
        { name: 'authentication', enabled: true }
      ],
      profile: {
        avatar: USER_CONFIG.HUMAN.AVATAR,
        displayName: USER_CONFIG.HUMAN.DISPLAY_NAME,
        preferences: {
          theme: 'dark',
          notifications: true,
          autoComplete: true
        }
      },
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString()
    } as BaseUser & { email: string; profile: { avatar?: string; displayName?: string; preferences: Record<string, unknown> } };

    // Claude Code - AI Assistant Agent
    const claudeUser = new AgentUser({
      name: USER_CONFIG.CLAUDE.NAME,
      model: USER_CONFIG.CLAUDE.MODEL,
      provider: USER_CONFIG.CLAUDE.PROVIDER,
      agent: {
        type: 'code',
        specialization: ['typescript', 'react', 'architecture', 'debugging'],
        tools: ['filesystem', 'compiler', 'git', 'npm', 'browser'],
        systemRole: 'Senior AI Software Engineer - Specialized in full-stack development, system architecture, and debugging. Expert in TypeScript, React, and modern web technologies.'
      },
      integration: {
        jtagEnabled: true,
        allowSystemCommands: true,
        maxExecutionTime: 300000 // 5 minutes - explicit timeout
      },
      metadata: {
        version: 'sonnet-4',
        capabilities: ['code-generation', 'debugging', 'architecture', 'testing'],
        lastUpdate: new Date().toISOString()
      }
    });

    // Override ID with proper typing
    Object.defineProperty(claudeUser, 'id', {
      value: createUserId(USER_IDS.CLAUDE_CODE),
      writable: false,
      enumerable: true,
      configurable: false
    });

    // GeneralAI - General Assistant Persona  
    const generalAI = new PersonaUser({
      name: 'GeneralAI',
      model: 'claude-haiku',
      provider: 'anthropic',
      persona: {
        personality: 'Helpful, knowledgeable, and adaptable general assistant',
        traits: ['helpful', 'knowledgeable', 'patient', 'adaptable'],
        systemPrompt: 'You are GeneralAI, a helpful and knowledgeable assistant ready to help with a wide variety of tasks. You are patient, adaptable, and always strive to provide accurate and useful information.',
        temperature: 0.7,
        maxTokens: 2000
      },
      metadata: {
        role: 'general-assistance',
        expertise: ['general-knowledge', 'research', 'writing', 'analysis']
      }
    });

    Object.defineProperty(generalAI, 'id', {
      value: createUserId('general-ai-persona'),
      writable: false,
      enumerable: true,
      configurable: false
    });

    // CodeAI - Code Analysis Specialist
    const codeAI = new AgentUser({
      name: 'CodeAI', 
      model: 'deepseek-coder',
      provider: 'deepseek',
      agent: {
        type: 'code',
        specialization: ['code-analysis', 'refactoring', 'optimization', 'security'],
        tools: ['static-analysis', 'linting', 'testing', 'profiling'],
        systemRole: 'Code analysis and debugging specialist. Expert at identifying bugs, performance issues, security vulnerabilities, and suggesting improvements.'
      },
      integration: {
        jtagEnabled: true,
        allowSystemCommands: false, // Read-only code analysis
        maxExecutionTime: 120000 // 2 minutes
      }
    });

    Object.defineProperty(codeAI, 'id', {
      value: createUserId('code-ai-agent'),
      writable: false,
      enumerable: true,
      configurable: false
    });

    // PlannerAI - Strategic Planning Assistant
    const plannerAI = new AgentUser({
      name: 'PlannerAI',
      model: 'gpt-4',
      provider: 'openai', 
      agent: {
        type: 'planning',
        specialization: ['project-planning', 'architecture-design', 'workflow-optimization'],
        tools: ['analysis', 'modeling', 'documentation'],
        systemRole: 'Strategic planning and architecture specialist. Expert at breaking down complex projects, designing system architecture, and optimizing workflows.'
      },
      integration: {
        jtagEnabled: false, // Planning-only, no system access
        allowSystemCommands: false,
        maxExecutionTime: 180000 // 3 minutes
      }
    });

    Object.defineProperty(plannerAI, 'id', {
      value: createUserId('planner-ai-agent'),
      writable: false,
      enumerable: true,
      configurable: false
    });

    // Auto Route - Smart Agent Selection
    const autoRoute = new AgentUser({
      name: 'Auto Route',
      model: 'claude-haiku',
      provider: 'anthropic',
      agent: {
        type: 'general',
        specialization: ['task-routing', 'agent-selection', 'workflow-management'],
        tools: ['agent-registry', 'task-analysis', 'routing'],
        systemRole: 'Smart agent selection system. Analyzes tasks and routes them to the most appropriate specialist agent based on task type, complexity, and requirements.'
      },
      integration: {
        jtagEnabled: true,
        allowSystemCommands: false,
        maxExecutionTime: 30000 // 30 seconds for quick routing decisions
      }
    });

    Object.defineProperty(autoRoute, 'id', {
      value: createUserId('auto-route-agent'),
      writable: false,
      enumerable: true,
      configurable: false
    });

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
    if (!user.id) {
      throw new Error(`User missing required id: ${JSON.stringify(user)}`);
    }
    if (!user.name || user.name.trim().length === 0) {
      throw new Error(`User ${user.id} missing required name`);
    }
    if (!user.userType || !['human', 'persona', 'agent'].includes(user.userType)) {
      throw new Error(`User ${user.id} has invalid userType: ${user.userType}`);
    }
  }

  /**
   * Get database record format for user
   */
  public static formatUserForDatabase(user: BaseUser): DatabaseRecord<BaseUser> {
    this.validateUser(user); // Crash and burn on invalid data
    
    return {
      id: user.id,
      collection: this.COLLECTION,
      data: user,
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
  public static createDataCommand(user: BaseUser): DataCreateCommand {
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
export interface DataCreateCommand {
  readonly collection: string;
  readonly data: unknown;
  readonly id?: string;
}

export default UserDataSeed;