/**
 * User Types - API Domain Concepts
 * 
 * Public API interfaces for user hierarchy and authentication.
 * These are the types that external consumers import and use.
 */

// Core user permissions and capabilities
export interface Permission {
  action: string;
  resource: string;
  granted: boolean;
}

export interface UserCapability {
  name: string;
  enabled: boolean;
  metadata?: Record<string, any>;
}

// Base user interface - all users implement this
export interface BaseUser {
  id: string;
  name: string;
  userType: UserType;
  isAuthenticated: boolean;
  permissions: Permission[];
  capabilities: UserCapability[];
  metadata?: Record<string, any>;
  createdAt: string;
  lastActiveAt: string;
}

// User type discriminator
export type UserType = 'human' | 'persona' | 'agent';

// Human user implementation
export interface HumanUser extends BaseUser {
  userType: 'human';
  email: string;
  profile: {
    avatar?: string;
    displayName?: string;
    preferences: Record<string, any>;
  };
}

// AI user base configuration
export interface AIConfig {
  name: string;
  model?: string;
  provider?: 'anthropic' | 'openai' | 'deepseek' | 'local';
  apiKey?: string;
  baseUrl?: string;
  metadata?: Record<string, any>;
}

// Abstract AI user base class
export abstract class AIUser implements BaseUser {
  public readonly id: string;
  public readonly name: string;
  public readonly userType: UserType;
  public readonly isAuthenticated: boolean = true;
  public readonly createdAt: string;
  public lastActiveAt: string;
  public readonly metadata: Record<string, any>;

  constructor(config: AIConfig, userType: UserType) {
    this.id = crypto.randomUUID();
    this.name = config.name;
    this.userType = userType;
    this.createdAt = new Date().toISOString();
    this.lastActiveAt = new Date().toISOString();
    this.metadata = {
      model: config.model,
      provider: config.provider,
      ...config.metadata
    };
  }

  // AI users get a combination of AI-specific + subclass permissions
  get permissions(): Permission[] {
    return [
      ...this.getCommonAIPermissions(),
      ...this.getAIPermissions()
    ];
  }

  // AI users get a combination of common + subclass capabilities  
  get capabilities(): UserCapability[] {
    return [
      ...this.getCommonAICapabilities(),
      ...this.getAICapabilities()
    ];
  }

  // Common permissions for all AI users
  private getCommonAIPermissions(): Permission[] {
    return [
      { action: 'chat', resource: '*', granted: true },
      { action: 'read_messages', resource: '*', granted: true }
    ];
  }

  // Common capabilities for all AI users
  private getCommonAICapabilities(): UserCapability[] {
    return [
      { name: 'natural_language', enabled: true },
      { name: 'conversation', enabled: true }
    ];
  }

  // Abstract methods that subclasses must implement
  protected abstract getAIPermissions(): Permission[];
  protected abstract getAICapabilities(): UserCapability[];

  // Update last active timestamp
  public updateLastActive(): void {
    this.lastActiveAt = new Date().toISOString();
  }
}

// Persona user configuration
export interface PersonaConfig extends AIConfig {
  persona: {
    personality: string;
    traits: string[];
    systemPrompt: string;
    temperature?: number;
    maxTokens?: number;
  };
  lora?: {
    adapter: string;
    weights: string;
    genomic?: Record<string, any>;
  };
}

// Persona user - AI with specific personality and capabilities
export class PersonaUser extends AIUser {
  public readonly persona: PersonaConfig['persona'];
  public readonly lora?: PersonaConfig['lora'];

  constructor(config: PersonaConfig) {
    super(config, 'persona');
    this.persona = config.persona;
    this.lora = config.lora;
  }

  protected getAIPermissions(): Permission[] {
    return [
      { action: 'creative_writing', resource: '*', granted: true },
      { action: 'roleplay', resource: '*', granted: true },
      { action: 'persona_interaction', resource: '*', granted: true }
    ];
  }

  protected getAICapabilities(): UserCapability[] {
    return [
      { name: 'creative_writing', enabled: true },
      { name: 'roleplay', enabled: true },
      { name: 'personality_adaptation', enabled: true },
      { name: 'contextual_response', enabled: true, metadata: { persona: this.persona.personality } }
    ];
  }
}

// Agent user configuration
export interface AgentConfig extends AIConfig {
  agent: {
    type: 'code' | 'research' | 'planning' | 'general';
    specialization?: string[];
    tools: string[];
    systemRole: string;
  };
  integration: {
    jtagEnabled: boolean;
    allowSystemCommands: boolean;
    maxExecutionTime?: number;
  };
}

// Agent user - AI connected via jtag for system integration
export class AgentUser extends AIUser {
  public readonly agent: AgentConfig['agent'];
  public readonly integration: AgentConfig['integration'];

  constructor(config: AgentConfig) {
    super(config, 'agent');
    this.agent = config.agent;
    this.integration = config.integration;
  }

  protected getAIPermissions(): Permission[] {
    const basePermissions = [
      { action: 'system_integration', resource: '*', granted: this.integration.jtagEnabled },
      { action: 'tool_access', resource: '*', granted: true }
    ];

    // Add system command permissions if enabled
    if (this.integration.allowSystemCommands) {
      basePermissions.push(
        { action: 'execute_commands', resource: '*', granted: true },
        { action: 'file_operations', resource: '*', granted: true },
        { action: 'process_management', resource: '*', granted: true }
      );
    }

    return basePermissions;
  }

  protected getAICapabilities(): UserCapability[] {
    const baseCapabilities = [
      { name: 'tool_integration', enabled: true, metadata: { tools: this.agent.tools } },
      { name: 'jtag_connectivity', enabled: this.integration.jtagEnabled }
    ];

    // Add type-specific capabilities
    switch (this.agent.type) {
      case 'code':
        baseCapabilities.push(
          { name: 'code_analysis', enabled: true },
          { name: 'debugging', enabled: true },
          { name: 'refactoring', enabled: true }
        );
        break;
      case 'research':
        baseCapabilities.push(
          { name: 'web_research', enabled: true },
          { name: 'data_analysis', enabled: true },
          { name: 'information_synthesis', enabled: true }
        );
        break;
      case 'planning':
        baseCapabilities.push(
          { name: 'strategic_planning', enabled: true },
          { name: 'task_decomposition', enabled: true },
          { name: 'workflow_optimization', enabled: true }
        );
        break;
      case 'general':
        baseCapabilities.push(
          { name: 'general_assistance', enabled: true },
          { name: 'multi_domain', enabled: true }
        );
        break;
    }

    return baseCapabilities;
  }
}

// User factory functions for easy creation
export function createHumanUser(config: {
  name: string;
  email: string;
  avatar?: string;
  displayName?: string;
  preferences?: Record<string, any>;
}): HumanUser {
  return {
    id: crypto.randomUUID(),
    name: config.name,
    userType: 'human',
    email: config.email,
    isAuthenticated: false, // Humans need to authenticate
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
      avatar: config.avatar,
      displayName: config.displayName || config.name,
      preferences: config.preferences || {}
    },
    createdAt: new Date().toISOString(),
    lastActiveAt: new Date().toISOString()
  };
}

// Type guards for runtime type checking
export function isHumanUser(user: BaseUser): user is HumanUser {
  return user.userType === 'human';
}

export function isPersonaUser(user: BaseUser): user is PersonaUser {
  return user.userType === 'persona';
}

export function isAgentUser(user: BaseUser): user is AgentUser {
  return user.userType === 'agent';
}

export function isAIUser(user: BaseUser): user is PersonaUser | AgentUser {
  return user.userType === 'persona' || user.userType === 'agent';
}

// Note: All types are already exported above, no need for duplicate exports