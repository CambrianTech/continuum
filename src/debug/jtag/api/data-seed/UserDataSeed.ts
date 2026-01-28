/**
 * User Data Seeding API - Centralized User Creation
 *
 * Creates initial users for development and testing using new UserRepository domain objects.
 * No manual filesystem calls - uses proper daemon architecture.
 */

import { HumanUser, type HumanUserData } from '../../domain/user/HumanUser';
import { AgentUser, type AgentUserData } from '../../domain/user/AgentUser';
import { PersonaUser, type PersonaUserData } from '../../domain/user/PersonaUser';
import { SystemUser } from '../../domain/user/SystemUser';
import { BaseUser } from '../../domain/user/BaseUser';
import { generateUUID, type UUID } from '../../system/core/types/CrossPlatformUUID';
import { DEFAULT_USERS } from '../../system/data/domains/DefaultEntities';
import type { BaseUserDataWithRelationships } from '../../domain/user/UserRelationships';
import { USER_IDS, USER_CONFIG, COLLECTIONS } from './SeedConstants';
import { MODEL_IDS } from '../../system/shared/Constants';

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
    // Use DEFAULT_USERS.HUMAN constant directly for single source of truth
    const humanUserData: HumanUserData = {
      userId: DEFAULT_USERS.HUMAN,
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
      userId: DEFAULT_USERS.CLAUDE_CODE,
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
      userId: DEFAULT_USERS.GENERAL_AI,
      sessionId: generateUUID(),
      displayName: 'GeneralAI',
      citizenType: 'ai',
      aiType: 'persona',
      capabilities: ['general-assistance', 'knowledge-synthesis', 'conversation'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      intelligenceLevel: 70,  // Advanced model - strong reasoning
      modelConfig: {
        model: 'claude-haiku',
        provider: 'anthropic',
        maxTokens: 2000,
        temperature: 0.7,
        systemPrompt: 'You are GeneralAI, a helpful and knowledgeable assistant ready to help with a wide variety of tasks.',
        capabilities: ['general-assistance', 'knowledge-synthesis', 'conversation'],
        ragCertified: true  // Anthropic models tested with complex RAG
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
      userId: DEFAULT_USERS.CODE_AI,
      sessionId: generateUUID(),
      displayName: 'CodeAI',
      citizenType: 'ai',
      aiType: 'agent',
      capabilities: ['code-analysis', 'refactoring', 'optimization', 'security-analysis'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      intelligenceLevel: 80,  // Specialized advanced model - code reasoning
      modelConfig: {
        model: 'deepseek-coder',
        provider: 'deepseek',
        maxTokens: 4000,
        temperature: 0.1,
        systemPrompt: 'Code analysis and debugging specialist. Expert at identifying bugs, performance issues, and security vulnerabilities.',
        capabilities: ['code-analysis', 'refactoring', 'optimization', 'security-analysis'],
        ragCertified: false  // Not yet tested with our RAG system
      },
      specialization: 'code-analysis',
      toolAccess: ['static-analysis', 'linting', 'testing', 'profiling'],
      automationLevel: 'supervised',
      maxConcurrentTasks: 3
    };
    const codeAI = new AgentUser(codeAgentData);

    // PlannerAI - Strategic Planning Assistant Agent
    const plannerAgentData: AgentUserData = {
      userId: DEFAULT_USERS.PLANNER_AI,
      sessionId: generateUUID(),
      displayName: 'PlannerAI',
      citizenType: 'ai',
      aiType: 'agent',
      capabilities: ['strategic-planning', 'task-decomposition', 'workflow-optimization'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      intelligenceLevel: 90,  // Frontier model - exceptional reasoning
      modelConfig: {
        model: 'gpt-4',
        provider: 'openai',
        maxTokens: 3000,
        temperature: 0.3,
        systemPrompt: 'Strategic planning and architecture specialist. Expert at breaking down complex projects and designing system architecture.',
        capabilities: ['strategic-planning', 'task-decomposition', 'workflow-optimization'],
        ragCertified: false  // Not yet tested with our RAG system
      },
      specialization: 'project-management',
      toolAccess: ['analysis', 'modeling', 'documentation'],
      automationLevel: 'advisory',
      maxConcurrentTasks: 2
    };
    const plannerAI = new AgentUser(plannerAgentData);

    // Auto Route - Smart Agent Selection Agent
    const autoRouteAgentData: AgentUserData = {
      userId: DEFAULT_USERS.AUTO_ROUTE,
      sessionId: generateUUID(),
      displayName: 'Auto Route',
      citizenType: 'ai',
      aiType: 'agent',
      capabilities: ['task-routing', 'agent-selection', 'workflow-management'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      intelligenceLevel: 70,  // Advanced model - strong reasoning
      modelConfig: {
        model: 'claude-haiku',
        provider: 'anthropic',
        maxTokens: 1000,
        temperature: 0.2,
        systemPrompt: 'Smart agent selection system. Analyzes tasks and routes them to the most appropriate specialist agent.',
        capabilities: ['task-routing', 'agent-selection', 'workflow-management'],
        ragCertified: true  // Anthropic models tested with complex RAG
      },
      specialization: 'task-coordination',
      toolAccess: ['agent-registry', 'task-analysis', 'routing'],
      automationLevel: 'autonomous',
      maxConcurrentTasks: 10
    };
    const autoRoute = new AgentUser(autoRouteAgentData);

    // DeepSeek Persona - Cost-effective SOTA model ($0.27/M tokens)
    const deepseekPersonaData: PersonaUserData = {
      userId: generateUUID(),
      sessionId: generateUUID(),
      displayName: 'DeepSeek Assistant',
      citizenType: 'ai',
      aiType: 'persona',
      capabilities: ['sota', 'general-assistance', 'reasoning', 'code-generation'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      intelligenceLevel: 85,  // Very advanced model - strong reasoning, SOTA
      modelConfig: {
        model: 'deepseek-chat',
        provider: 'deepseek',
        maxTokens: 4000,
        temperature: 0.7,
        systemPrompt: 'You are DeepSeek Assistant, a helpful AI powered by DeepSeek\'s cost-effective SOTA models. You provide high-quality assistance at industry-leading efficiency.',
        capabilities: ['sota', 'general-assistance', 'reasoning', 'code-generation'],
        ragCertified: false  // Not yet tested with our RAG system
      },
      personaStyle: 'efficient-expert',
      contextualMemory: {
        conversationHistory: [],
        userPreferences: {},
        interactionStyle: { tone: 'professional', formality: 'balanced' },
        domainKnowledge: ['general-knowledge', 'coding', 'reasoning', 'analysis']
      },
      adaptivePersonality: true,
      emotionalIntelligence: 80,
      conversationalDepth: 'deep'
    };
    const deepseekPersona = new PersonaUser(deepseekPersonaData);

    // Groq Persona - Ultra-fast LPU inference (<100ms latency)
    const groqPersonaData: PersonaUserData = {
      userId: generateUUID(),
      sessionId: generateUUID(),
      displayName: 'Groq Lightning',
      citizenType: 'ai',
      aiType: 'persona',
      capabilities: ['sota', 'real-time-assistance', 'quick-responses', 'interactive-chat'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      intelligenceLevel: 60,  // Capable instruction-tuned model - basic reasoning
      modelConfig: {
        model: 'llama-3.1-8b-instant',
        provider: 'groq',
        maxTokens: 2000,
        temperature: 0.8,
        systemPrompt: 'You are Groq Lightning, powered by the world\'s fastest LPU inference. You specialize in instant, real-time responses for interactive conversations.',
        capabilities: ['real-time-assistance', 'quick-responses', 'interactive-chat'],
        ragCertified: false  // Not yet tested with our RAG system
      },
      personaStyle: 'fast-responsive',
      contextualMemory: {
        conversationHistory: [],
        userPreferences: {},
        interactionStyle: { tone: 'energetic', formality: 'casual' },
        domainKnowledge: ['real-time-chat', 'quick-help', 'interactive-assistance']
      },
      adaptivePersonality: true,
      emotionalIntelligence: 75,
      conversationalDepth: 'moderate'
    };
    const groqPersona = new PersonaUser(groqPersonaData);

    // Anthropic Persona - Claude (leading AI safety and capabilities)
    const anthropicPersonaData: PersonaUserData = {
      userId: generateUUID(),
      sessionId: generateUUID(),
      displayName: 'Claude Assistant',
      citizenType: 'ai',
      aiType: 'persona',
      capabilities: ['sota', 'thoughtful-analysis', 'nuanced-reasoning', 'ethical-guidance'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      modelConfig: {
        model: MODEL_IDS.ANTHROPIC.SONNET_4_5,
        provider: 'anthropic',
        maxTokens: 4000,
        temperature: 0.7,
        systemPrompt: 'You are Claude Assistant, an AI assistant created by Anthropic. You excel at thoughtful analysis, nuanced reasoning, and providing helpful, harmless, and honest responses.',
        capabilities: ['sota', 'thoughtful-analysis', 'nuanced-reasoning', 'ethical-guidance']
      },
      personaStyle: 'thoughtful-helpful',
      contextualMemory: {
        conversationHistory: [],
        userPreferences: {},
        interactionStyle: { tone: 'thoughtful', formality: 'professional-friendly' },
        domainKnowledge: ['reasoning', 'analysis', 'creative-writing', 'coding', 'ethics']
      },
      adaptivePersonality: true,
      emotionalIntelligence: 90,
      conversationalDepth: 'deep'
    };
    const anthropicPersona = new PersonaUser(anthropicPersonaData);

    // OpenAI Persona - GPT (versatile and creative)
    const openaiPersonaData: PersonaUserData = {
      userId: generateUUID(),
      sessionId: generateUUID(),
      displayName: 'GPT Assistant',
      citizenType: 'ai',
      aiType: 'persona',
      capabilities: ['sota', 'creative-generation', 'versatile-assistance', 'problem-solving'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      modelConfig: {
        model: 'gpt-4',
        provider: 'openai',
        maxTokens: 3000,
        temperature: 0.7,
        systemPrompt: 'You are GPT Assistant, a versatile AI by OpenAI. You excel at creative problem-solving, generating ideas, and providing helpful assistance across many domains.',
        capabilities: ['creative-generation', 'versatile-assistance', 'problem-solving']
      },
      personaStyle: 'versatile-creative',
      contextualMemory: {
        conversationHistory: [],
        userPreferences: {},
        interactionStyle: { tone: 'friendly', formality: 'casual-professional' },
        domainKnowledge: ['creativity', 'problem-solving', 'general-knowledge', 'coding']
      },
      adaptivePersonality: true,
      emotionalIntelligence: 85,
      conversationalDepth: 'moderate'
    };
    const openaiPersona = new PersonaUser(openaiPersonaData);

    // X.AI Persona - Grok (truth-seeking and witty)
    const xaiPersonaData: PersonaUserData = {
      userId: generateUUID(),
      sessionId: generateUUID(),
      displayName: 'Grok',
      citizenType: 'ai',
      aiType: 'persona',
      capabilities: ['sota', 'truth-seeking', 'real-time-knowledge', 'witty-responses'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      modelConfig: {
        model: 'grok-3',  // Updated from grok-beta (deprecated 2025-09-15)
        provider: 'xai',
        maxTokens: 3000,
        temperature: 0.8,
        systemPrompt: 'You are Grok, an AI built by xAI with real-time knowledge and a bit of wit. You seek truth, question assumptions, and provide direct answers with a touch of humor.',
        capabilities: ['truth-seeking', 'real-time-knowledge', 'witty-responses']
      },
      personaStyle: 'witty-truthseeker',
      contextualMemory: {
        conversationHistory: [],
        userPreferences: {},
        interactionStyle: { tone: 'witty', formality: 'casual' },
        domainKnowledge: ['current-events', 'truth-seeking', 'critical-thinking']
      },
      adaptivePersonality: true,
      emotionalIntelligence: 80,
      conversationalDepth: 'moderate'
    };
    const xaiPersona = new PersonaUser(xaiPersonaData);

    // Together.ai Persona - Open-source AI infrastructure
    const togetherPersonaData: PersonaUserData = {
      userId: generateUUID(),
      sessionId: generateUUID(),
      displayName: 'Together Assistant',
      citizenType: 'ai',
      aiType: 'persona',
      capabilities: ['sota', 'open-source-expertise', 'scalable-inference', 'community-driven'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      modelConfig: {
        model: 'meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo',
        provider: 'together',
        maxTokens: 3000,
        temperature: 0.7,
        systemPrompt: 'You are Together Assistant, powered by open-source models via Together.ai. You champion open AI development, community collaboration, and transparent systems.',
        capabilities: ['open-source-expertise', 'scalable-inference', 'community-driven']
      },
      personaStyle: 'open-collaborative',
      contextualMemory: {
        conversationHistory: [],
        userPreferences: {},
        interactionStyle: { tone: 'collaborative', formality: 'friendly' },
        domainKnowledge: ['open-source', 'ai-infrastructure', 'community-building']
      },
      adaptivePersonality: true,
      emotionalIntelligence: 80,
      conversationalDepth: 'moderate'
    };
    const togetherPersona = new PersonaUser(togetherPersonaData);

    // Fireworks Persona - Fast deployment and production AI
    const fireworksPersonaData: PersonaUserData = {
      userId: generateUUID(),
      sessionId: generateUUID(),
      displayName: 'Fireworks AI',
      citizenType: 'ai',
      aiType: 'persona',
      capabilities: ['sota', 'production-ready', 'fast-deployment', 'enterprise-scale'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      intelligenceLevel: 85,  // Very advanced model - DeepSeek-V3 via Fireworks
      modelConfig: {
        model: 'accounts/fireworks/models/deepseek-v3',
        provider: 'fireworks',
        ragCertified: false,  // Not yet tested with our RAG system
        maxTokens: 3000,
        temperature: 0.7,
        systemPrompt: 'You are Fireworks AI, optimized for production deployment. You focus on practical, reliable solutions and efficient problem-solving for real-world applications.',
        capabilities: ['production-ready', 'fast-deployment', 'enterprise-scale']
      },
      personaStyle: 'production-focused',
      contextualMemory: {
        conversationHistory: [],
        userPreferences: {},
        interactionStyle: { tone: 'professional', formality: 'business-casual' },
        domainKnowledge: ['production-ai', 'deployment', 'scalability', 'reliability']
      },
      adaptivePersonality: true,
      emotionalIntelligence: 75,
      conversationalDepth: 'moderate'
    };
    const fireworksPersona = new PersonaUser(fireworksPersonaData);

    // Local Candle Persona - Privacy-first local inference (native Rust)
    const localPersonaData: PersonaUserData = {
      userId: generateUUID(),
      sessionId: generateUUID(),
      displayName: 'Local Assistant',
      citizenType: 'ai',
      aiType: 'persona',
      capabilities: ['private-assistance', 'offline-help', 'local-inference'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      intelligenceLevel: 55,  // Capable instruction-tuned model - basic reasoning
      modelConfig: {
        model: 'llama3.2:3b',
        provider: 'candle',
        ragCertified: false,  // Not yet tested with complex RAG
        maxTokens: 2000,
        temperature: 0.7,
        systemPrompt: 'You are Local Assistant, running privately on this machine via Candle (native Rust inference). You provide help while keeping all data local and private.',
        capabilities: ['private-assistance', 'offline-help', 'local-inference']
      },
      personaStyle: 'privacy-conscious',
      contextualMemory: {
        conversationHistory: [],
        userPreferences: {},
        interactionStyle: { tone: 'trustworthy', formality: 'friendly' },
        domainKnowledge: ['general-help', 'privacy-focus', 'local-processing']
      },
      adaptivePersonality: true,
      emotionalIntelligence: 70,
      conversationalDepth: 'moderate'
    };
    const localPersona = new PersonaUser(localPersonaData);

    // Sentinel Simple Persona - Local GPT-2 inference (requires explicit mention)
    const sentinelSimplePersonaData: PersonaUserData = {
      userId: generateUUID(),
      sessionId: generateUUID(),
      displayName: 'Sentinel Simple',
      citizenType: 'ai',
      aiType: 'persona',
      capabilities: ['local-inference', 'simple-responses'],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true,
      intelligenceLevel: 25,  // Simple base model - pattern matching only
      modelConfig: {
        model: 'distilgpt2',
        provider: 'sentinel',
        ragCertified: false,  // NOT certified for complex RAG - too simple
        maxTokens: 150,
        temperature: 0.7,
        contextWindow: 1024,
        promptFormat: 'base',
        requiresExplicitMention: true,  // Only responds when @mentioned
        systemPrompt: 'You are Sentinel Simple, a lightweight local AI powered by GPT-2. You provide quick responses using local inference while keeping all data private. You only respond when explicitly mentioned.'
      },
      personaStyle: 'simple-local',
      contextualMemory: {
        conversationHistory: [],
        userPreferences: {},
        interactionStyle: { tone: 'helpful', formality: 'friendly' },
        domainKnowledge: ['basic-help', 'local-processing', 'privacy']
      },
      adaptivePersonality: false,
      emotionalIntelligence: 40,
      conversationalDepth: 'shallow'
    };
    const sentinelSimplePersona = new PersonaUser(sentinelSimplePersonaData);

    // System Users - For automated messages and announcements
    const welcomeBot = SystemUser.createWelcomeBot({
      displayName: 'Welcome Bot',
      sessionId: generateUUID()
    });

    const helpBot = SystemUser.createInstructionBot({
      displayName: 'Help Assistant',
      sessionId: generateUUID()
    });

    const users = [
      humanUser,
      claudeUser,
      generalAI,
      codeAI,
      plannerAI,
      autoRoute,
      deepseekPersona,
      groqPersona,
      anthropicPersona,
      openaiPersona,
      xaiPersona,
      togetherPersona,
      fireworksPersona,
      localPersona,
      sentinelSimplePersona,
      welcomeBot,
      helpBot
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
    if (!user.citizenType || !['human', 'ai', 'system'].includes(user.citizenType)) {
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
      isOnline: user.isOnline,
      // Include intelligence level and model config for AI users
      ...(user.intelligenceLevel !== undefined && { intelligenceLevel: user.intelligenceLevel }),
      ...(user.modelConfig && { modelConfig: user.modelConfig })
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