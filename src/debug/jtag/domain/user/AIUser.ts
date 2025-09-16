/**
 * AIUser - AI citizen base class
 *
 * Abstract base for all AI-based system citizens.
 * Provides AI-specific capabilities while allowing specialized extensions.
 */

import { BaseUser, type BaseUserData } from './BaseUser';

/**
 * AI-specific data extending base user
 */
export interface AIUserData extends BaseUserData {
  readonly citizenType: 'ai';
  readonly aiType: 'agent' | 'persona';
}

/**
 * Abstract AI User - Base for all AI citizens
 *
 * Abstract class - must be extended by AgentUser or PersonaUser.
 * Provides common AI functionality for both agent and persona types.
 */
export abstract class AIUser extends BaseUser {
  declare protected readonly data: AIUserData;

  constructor(data: AIUserData) {
    super(data);
  }

  /**
   * AI-specific accessors
   */
  get modelConfig(): AIModelConfig {
    return this.data.modelConfig;
  }

  get aiType(): AIUserData['aiType'] {
    return this.data.aiType;
  }

  get specialization(): string | undefined {
    return this.data.specialization;
  }

  get contextMemory(): readonly string[] {
    return this.data.contextMemory || [];
  }

  /**
   * Common AI implementations
   */
  getInteractionModel(): 'ai-automated' {
    return 'ai-automated';
  }

  getCapabilitySet(): readonly string[] {
    return [
      'automated-response',
      'api-integration',
      'context-awareness',
      'multi-turn-conversation',
      'data-processing',
      'structured-output',
      ...this.capabilities
    ];
  }

  /**
   * AI-specific behaviors
   */
  updateModelConfig(modelConfig: Partial<AIModelConfig>): this {
    const newData = {
      ...this.data,
      modelConfig: { ...this.data.modelConfig, ...modelConfig }
    };
    return this.createInstance(newData);
  }

  addToContextMemory(context: string): this {
    const newData = {
      ...this.data,
      contextMemory: [...(this.data.contextMemory || []), context].slice(-50) // Keep last 50 contexts
    };
    return this.createInstance(newData);
  }

  clearContextMemory(): this {
    const newData = {
      ...this.data,
      contextMemory: []
    };
    return this.createInstance(newData);
  }

  /**
   * AI interaction patterns
   */
  canAutoRespond(): boolean {
    return true;
  }

  canProcessBulkData(): boolean {
    return true;
  }

  requiresManualConfirmation(): boolean {
    return false; // AI can operate autonomously
  }

  canMaintainContext(): boolean {
    return this.modelConfig.contextWindow !== undefined && this.modelConfig.contextWindow > 0;
  }

  /**
   * Type guards for AI subtypes
   */
  isAgent(): this is AgentUser {
    return this.aiType === 'agent';
  }

  isPersona(): this is PersonaUser {
    return this.aiType === 'persona';
  }

  /**
   * Factory method
   */
  static fromData(data: AIUserData): AIUser {
    // Factory delegates to concrete types based on aiType
    switch (data.aiType) {
      case 'agent':
        return AgentUser.fromData(data);
      case 'persona':
        return PersonaUser.fromData(data);
      default:
        throw new Error(`Unknown AI type: ${data.aiType}`);
    }
  }
}

/**
 * Forward declarations - Implemented in separate files
 */
export interface AgentUser extends AIUser {
  readonly aiType: 'agent';
}

export interface PersonaUser extends AIUser {
  readonly aiType: 'persona';
}

// These will be imported when the concrete classes are implemented
declare class AgentUser extends AIUser {}
declare class PersonaUser extends AIUser {}