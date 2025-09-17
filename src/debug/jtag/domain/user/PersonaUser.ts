/**
 * PersonaUser - AI persona implementation
 *
 * Context-aware AI citizens with specific personalities and interaction styles.
 * Examples: Claude Code (coding assistant), GeneralAI (general help)
 */

import { AIUser, type AIUserData, type AIModelConfig } from './AIUser';
import type { BaseUserData } from './BaseUser';
import { randomUUID } from 'crypto';

/**
 * Persona interaction styles and behaviors
 */
export type PersonaStyle =
  | 'professional-assistant'
  | 'friendly-helper'
  | 'technical-expert'
  | 'creative-collaborator'
  | 'analytical-advisor'
  | 'casual-companion';

/**
 * Persona context and memory configuration
 */
export interface PersonaContext {
  readonly conversationHistory: readonly string[];
  readonly userPreferences: Record<string, unknown>;
  readonly interactionStyle: Record<string, unknown>;
  readonly domainKnowledge: readonly string[];
}

/**
 * Persona-specific data extending AI capabilities
 */
export interface PersonaUserData extends AIUserData {
  readonly aiType: 'persona';
  readonly personaStyle: PersonaStyle;
  readonly contextualMemory: PersonaContext;
  readonly adaptivePersonality: boolean;
  readonly emotionalIntelligence: number; // 0-100 scale
  readonly conversationalDepth: 'surface' | 'moderate' | 'deep';
}

/**
 * Persona User - Context-aware AI with personality
 *
 * AI citizens focused on natural conversation and contextual understanding.
 * Each persona maintains conversation history and adapts to user preferences.
 */
export class PersonaUser extends AIUser {
  declare protected readonly data: PersonaUserData;

  constructor(data: PersonaUserData) {
    super(data);
  }

  /**
   * Persona-specific implementations
   */
  getAdapterType(): string {
    return 'persona-contextual';
  }

  getInteractionModel(): 'ai-automated' {
    return 'ai-automated';
  }

  getCapabilitySet(): readonly string[] {
    const baseCapabilities = super.getCapabilitySet();
    const personaCapabilities = [
      'contextual-conversation',
      'personality-adaptation',
      'emotional-awareness',
      'conversation-continuity',
      'user-preference-learning',
      'natural-language-processing',
      ...this.getPersonaStyleCapabilities()
    ];
    return [...baseCapabilities, ...personaCapabilities];
  }

  /**
   * Persona-specific accessors
   */
  get personaStyle(): PersonaStyle {
    return this.data.personaStyle;
  }

  get contextualMemory(): PersonaContext {
    return this.data.contextualMemory;
  }

  get adaptivePersonality(): boolean {
    return this.data.adaptivePersonality;
  }

  get emotionalIntelligence(): number {
    return this.data.emotionalIntelligence;
  }

  get conversationalDepth(): PersonaUserData['conversationalDepth'] {
    return this.data.conversationalDepth;
  }

  /**
   * Persona-specific behaviors
   */
  canAdaptPersonality(): boolean {
    return this.adaptivePersonality;
  }

  hasHighEmotionalIntelligence(): boolean {
    return this.emotionalIntelligence >= 70;
  }

  canProvideDeepConversation(): boolean {
    return this.conversationalDepth === 'deep';
  }

  updatePersonaStyle(style: PersonaStyle): PersonaUser {
    const newData = {
      ...this.data,
      personaStyle: style
    };
    return new PersonaUser(newData);
  }

  updateConversationHistory(message: string): PersonaUser {
    const newHistory = [
      ...this.contextualMemory.conversationHistory,
      message
    ].slice(-100); // Keep last 100 messages

    const newData = {
      ...this.data,
      contextualMemory: {
        ...this.contextualMemory,
        conversationHistory: newHistory
      }
    };
    return new PersonaUser(newData);
  }

  updateUserPreferences(preferences: Record<string, unknown>): PersonaUser {
    const newData = {
      ...this.data,
      contextualMemory: {
        ...this.contextualMemory,
        userPreferences: { ...this.contextualMemory.userPreferences, ...preferences }
      }
    };
    return new PersonaUser(newData);
  }

  addDomainKnowledge(knowledge: string): PersonaUser {
    const newDomainKnowledge = [
      ...this.contextualMemory.domainKnowledge,
      knowledge
    ];

    const newData = {
      ...this.data,
      contextualMemory: {
        ...this.contextualMemory,
        domainKnowledge: newDomainKnowledge
      }
    };
    return new PersonaUser(newData);
  }

  clearConversationHistory(): PersonaUser {
    const newData = {
      ...this.data,
      contextualMemory: {
        ...this.contextualMemory,
        conversationHistory: []
      }
    };
    return new PersonaUser(newData);
  }

  /**
   * Persona style-specific capabilities
   */
  private getPersonaStyleCapabilities(): readonly string[] {
    switch (this.personaStyle) {
      case 'professional-assistant':
        return ['formal-communication', 'task-organization', 'professional-guidance'];

      case 'friendly-helper':
        return ['casual-conversation', 'empathetic-response', 'encouraging-support'];

      case 'technical-expert':
        return ['deep-technical-knowledge', 'detailed-explanations', 'problem-solving'];

      case 'creative-collaborator':
        return ['creative-ideation', 'brainstorming', 'artistic-guidance'];

      case 'analytical-advisor':
        return ['data-analysis', 'logical-reasoning', 'strategic-thinking'];

      case 'casual-companion':
        return ['informal-chat', 'entertainment', 'companionship'];

      default:
        return [];
    }
  }

  /**
   * Conversation context analysis
   */
  getConversationLength(): number {
    return this.contextualMemory.conversationHistory.length;
  }

  hasUserPreference(key: string): boolean {
    return key in this.contextualMemory.userPreferences;
  }

  getUserPreference<T>(key: string, defaultValue: T): T {
    return (this.contextualMemory.userPreferences[key] as T) ?? defaultValue;
  }

  hasDomainKnowledge(domain: string): boolean {
    return this.contextualMemory.domainKnowledge.includes(domain);
  }

  /**
   * Factory methods
   */
  static fromData(data: PersonaUserData): PersonaUser {
    return new PersonaUser(data);
  }

  static createNew(
    displayName: string,
    sessionId: string,
    personaStyle: PersonaStyle,
    modelConfig: AIModelConfig
  ): PersonaUser {
    const data: PersonaUserData = {
      userId: randomUUID(),
      sessionId,
      displayName,
      citizenType: 'ai',
      aiType: 'persona',
      personaStyle,
      modelConfig,
      contextualMemory: {
        conversationHistory: [],
        userPreferences: {},
        interactionStyle: {},
        domainKnowledge: []
      },
      adaptivePersonality: true,
      emotionalIntelligence: 75,
      conversationalDepth: 'moderate',
      capabilities: [],
      createdAt: new Date().toISOString(),
      lastActiveAt: new Date().toISOString(),
      preferences: {},
      isOnline: true
    };
    return new PersonaUser(data);
  }

  /**
   * Immutable update implementation
   */
  protected createInstance(data: BaseUserData): this {
    return new PersonaUser(data as PersonaUserData) as this;
  }
}